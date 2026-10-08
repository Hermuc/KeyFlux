//! 对 KeyFlux-CommandInput.exe 施加「抑制八角 keycap」数据 patch ——
//! 原 `tools/patch_command_input.py` 的 **Rust 移植**（逐字契约，勿改文案/退出码）。
//!
//! 背景（详见 docs/CONTRACTS.md §3.11）：命令框 exe 对 `a-zA-Z0-9` 62 个字符绘制八角
//! keycap 外框，判定所用字符白名单是 .rdata 里的**常量数据**（非代码）⇒ 可安全做等长
//! 数据 patch。把这 62 个字符逐字替换为 U+0001（保留 124 字节长度）后白名单不再匹配任何
//! 真实输入字符 ⇒ 走普通字形路径、无八角框；长度不变 ⇒ 不移动后续数据，段表/校验和不受影响。
//!
//! ⚠ 适用范围：命令框现为自研 Rust 产物（`command-input/`），**不含**上游那段 0x1CCA0
//! 白名单（它自绘结果列表）⇒ 本 patch 对它不适用，工具会自动识别（命中 `panicked at`
//! 标记 + 哨兵不符）并跳过（exit 0）。其余目标（历史上游闭源 exe）行为不变。
//!
//! 退出码：0 = 成功 / 幂等跳过 / 不适用；1 = 前置校验失败（文件不符预期，绝不盲写）。

use std::fs;
use std::path::Path;

/// patch 目标偏移（文件偏移 0x1CCA0 = RVA 0x1DAA0；单一真源，与 CONTRACTS §3.11 一致）。
const OFFSET: usize = 0x1CCA0;
/// 官方原版 keycap 白名单（62 个字符）。
const KEYCAP_WHITELIST: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// 白名单字符数（62）。
const N_CHARS: usize = KEYCAP_WHITELIST.len();
/// 替换用的不可打印控制字符（保留长度）。
const PATCH_CHAR: char = '\u{1}';

/// 前置哨兵：0x1CC80 起 16 B = "ace->End"（UTF-16LE，源自 "ace->EndDraw()"）。
const SENTINEL_BEFORE_OFF: usize = 0x1CC80;
const SENTINEL_BEFORE: [u8; 16] = [
    0x61, 0x00, 0x63, 0x00, 0x65, 0x00, 0x2d, 0x00, 0x3e, 0x00, 0x45, 0x00, 0x6e, 0x00, 0x64, 0x00,
];
/// 后置哨兵：0x1CD1C 起 16 B = NUL,NUL,'d','w','r','i','t','e'（源自 "\0\0dwriteFactory-"）。
const SENTINEL_AFTER_OFF: usize = 0x1CD1C;
const SENTINEL_AFTER: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x77, 0x00, 0x72, 0x00, 0x69, 0x00, 0x74, 0x00, 0x65, 0x00,
];
/// Rust 重写版识别标记：std 默认 panic handler 会把 "panicked at" 编进二进制，上游闭源 exe 不会。
const RUST_PANIC_MARKER: &[u8] = b"panicked at";

/// 白名单区域被替换后的字节（62 × U+0001 的 UTF-16LE = 124 B）。
fn patched_bytes() -> Vec<u8> {
    // U+0001 是 BMP 内码点，UTF-16LE 恒为单个 code unit 0x0001。
    let unit = (PATCH_CHAR as u16).to_le_bytes();
    std::iter::repeat_n(unit, N_CHARS).flatten().collect()
}

/// 官方原版白名单的 UTF-16LE 字节。
fn whitelist_bytes() -> Vec<u8> {
    KEYCAP_WHITELIST
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 目标是否为自研 Rust 命令框（command-input/ 的构建产物）。
fn is_rust_rewrite(blob: &[u8]) -> bool {
    blob.windows(RUST_PANIC_MARKER.len())
        .any(|w| w == RUST_PANIC_MARKER)
}

/// 返回 "patched" / "original" / "unknown"。
fn classify(blob: &[u8]) -> &'static str {
    let seg = &blob[OFFSET..OFFSET + N_CHARS * 2];
    if seg == patched_bytes().as_slice() {
        "patched"
    } else if seg == whitelist_bytes().as_slice() {
        "original"
    } else {
        "unknown"
    }
}

/// 校验 patch 点前后的哨兵字节；不符则返回错误描述（对齐 Python verify_sentinels）。
fn verify_sentinels(blob: &[u8]) -> Option<String> {
    let before = &blob[SENTINEL_BEFORE_OFF..SENTINEL_BEFORE_OFF + SENTINEL_BEFORE.len()];
    let after = &blob[SENTINEL_AFTER_OFF..SENTINEL_AFTER_OFF + SENTINEL_AFTER.len()];
    if before != SENTINEL_BEFORE {
        return Some(format!(
            "前置哨兵不符: 期望 {}, 实得 {} (偏移 {SENTINEL_BEFORE_OFF:#x}) —— exe 版本可能已变更, 拒绝写入",
            hex(&SENTINEL_BEFORE),
            hex(before)
        ));
    }
    if after != SENTINEL_AFTER {
        return Some(format!(
            "后置哨兵不符: 期望 {}, 实得 {} (偏移 {SENTINEL_AFTER_OFF:#x}) —— exe 版本可能已变更, 拒绝写入",
            hex(&SENTINEL_AFTER),
            hex(after)
        ));
    }
    None
}

/// CLI 入口：argv → 退出码（文案逐字对齐原 Python 脚本）。
///
/// `args` = 位置参数 exe 路径 + 可选 `--check` / `--revert`（顺序不限）。
pub fn run(args: &[String]) -> i32 {
    let mut exe: Option<String> = None;
    let mut check = false;
    let mut revert = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "--revert" => revert = true,
            other => {
                if exe.is_none() {
                    exe = Some(other.to_string());
                }
            }
        }
    }
    let Some(exe) = exe else {
        println!("usage: devtools patch-command-input <exe> [--check] [--revert]");
        return 1;
    };
    let path = Path::new(&exe);
    if !path.is_file() {
        println!("[FAIL] 文件不存在: {exe}");
        return 1;
    }

    let Ok(mut blob) = fs::read(path) else {
        println!("[FAIL] 文件不存在: {exe}");
        return 1;
    };
    if blob.len() < OFFSET + N_CHARS * 2 {
        println!("[FAIL] 文件过小 ({} B), 不可能是目标 exe", blob.len());
        return 1;
    }

    if let Some(err) = verify_sentinels(&blob) {
        if is_rust_rewrite(&blob) {
            println!("[skip] {exe}");
            println!(
                "[skip] 目标为自研 Rust 命令框 (command-input/)，不含上游 0x1CCA0 keycap 白名单"
            );
            println!(
                "[skip] keycap patch 仅对闭源上游 exe 适用 —— Rust 版自绘结果列表，无需 patch"
            );
            return 0;
        }
        println!("[FAIL] {err}");
        return 1;
    }

    let state = classify(&blob);
    println!("[info] {exe}");
    println!(
        "[info] 大小 {} B, 偏移 {OFFSET:#x} 状态 = {state}",
        blob.len()
    );

    if check {
        return 0;
    }

    let want = if revert { "original" } else { "patched" };
    let label = if revert { "还原" } else { "施加" };

    if state == want {
        println!("[OK] 已是目标状态 ({want}), 幂等跳过");
        return 0;
    }

    if state == "unknown" {
        let seg = &blob[OFFSET..OFFSET + N_CHARS * 2];
        println!(
            "[FAIL] 白名单区域内容不符预期, 拒绝写入 (前 32 字节: {})",
            hex(&seg[..32.min(seg.len())])
        );
        return 1;
    }

    let payload = if revert {
        whitelist_bytes()
    } else {
        patched_bytes()
    };
    blob[OFFSET..OFFSET + N_CHARS * 2].copy_from_slice(&payload);

    // 回读复核：写盘后重新读文件确认落盘字节正确。
    if fs::write(path, &blob).is_err() {
        println!("[FAIL] 回读复核失败: 期望 {want}, 实得 unknown");
        return 1;
    }
    let reread = fs::read(path).unwrap_or_default();
    let after_state = if reread.len() >= OFFSET + N_CHARS * 2 {
        classify(&reread)
    } else {
        "unknown"
    };
    if after_state != want {
        println!("[FAIL] 回读复核失败: 期望 {want}, 实得 {after_state}");
        return 1;
    }

    println!(
        "[OK] {label}成功, 回读复核通过 ({want}); 长度不变 {} B",
        blob.len()
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个满足哨兵、白名单为 original 的最小伪 exe。
    fn fake_exe(whitelist_state: &str) -> Vec<u8> {
        let mut blob = vec![0u8; OFFSET + N_CHARS * 2 + 16];
        blob[SENTINEL_BEFORE_OFF..SENTINEL_BEFORE_OFF + 16].copy_from_slice(&SENTINEL_BEFORE);
        blob[SENTINEL_AFTER_OFF..SENTINEL_AFTER_OFF + 16].copy_from_slice(&SENTINEL_AFTER);
        let payload = match whitelist_state {
            "original" => whitelist_bytes(),
            "patched" => patched_bytes(),
            _ => vec![0xAA; N_CHARS * 2],
        };
        blob[OFFSET..OFFSET + N_CHARS * 2].copy_from_slice(&payload);
        blob
    }

    #[test]
    fn patched_bytes_are_124_bytes_of_u0001() {
        let p = patched_bytes();
        assert_eq!(p.len(), 124);
        assert!(p.chunks(2).all(|c| c == [0x01, 0x00]));
    }

    #[test]
    fn whitelist_bytes_roundtrip() {
        let w = whitelist_bytes();
        assert_eq!(w.len(), 124);
        assert_eq!(w[0], b'a');
        assert_eq!(w[1], 0x00);
    }

    #[test]
    fn classify_recognizes_states() {
        assert_eq!(classify(&fake_exe("original")), "original");
        assert_eq!(classify(&fake_exe("patched")), "patched");
        assert_eq!(classify(&fake_exe("garbage")), "unknown");
    }

    #[test]
    fn sentinels_verified_only_when_intact() {
        assert!(verify_sentinels(&fake_exe("original")).is_none());
        let mut broken = fake_exe("original");
        broken[SENTINEL_BEFORE_OFF] = 0xFF;
        assert!(verify_sentinels(&broken).is_some());
    }

    #[test]
    fn rust_marker_detected() {
        let mut blob = fake_exe("unknown");
        blob.extend_from_slice(b"some text panicked at src/main.rs");
        assert!(is_rust_rewrite(&blob));
        assert!(!is_rust_rewrite(&fake_exe("original")));
    }
}
