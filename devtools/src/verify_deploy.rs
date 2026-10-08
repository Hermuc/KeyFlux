//! 部署「双腿」暂存一致性机械校验 —— 原 `tools/verify_deploy.ps1` 的 Rust 移植。
//!
//! KeyFlux 出货两个由**同一次 cargo release 构建**产出的二进制 + 一个自建 exe：
//! * leg A（backend）：`target/release/settings.exe` → `bin/settings.exe`
//! * leg B（panel）：`target/release/keyflux-settings.exe` → `bin/ui/KeyFlux.Settings.exe`
//! * leg C（command input）：`command-input/target/release/…` → `bin/KeyFlux-CommandInput.exe`
//!
//! 每一起「部署没问题」的事故都是某条腿悄悄变旧（各腿由不同步骤产出/复制，无人比对）。
//! 更糟：跨腿 md5/sha **全相等**本身就是红旗（= 同一个 exe 被拷进了两个槽位）。
//!
//! 校验：① 每条腿 built == staged（leg C 可选：built 缺失则 skip 不 fail）；② 已暂存的腿
//! 两两 distinct；③ 每个 staged 文件存在。退出码 0 = 一致；1 = 至少一条断言失败。

use std::path::PathBuf;

use crate::util::{repo_root, sha256_file};

/// 一条腿的校验结果哈希（None = 未产出可比哈希）。
struct Leg {
    label: &'static str,
    hash: Option<String>,
}

#[allow(clippy::too_many_lines)]
pub fn run(args: &[String]) -> i32 {
    let mut release_dir = "config-ui-reactor/target/release".to_string();
    let mut bin_root = "bin".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--release-dir" | "-ReleaseDir" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    release_dir = v.clone();
                }
            }
            "--bin-root" | "-BinRoot" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    bin_root = v.clone();
                }
            }
            _ => {}
        }
        i += 1;
    }

    let repo = repo_root();
    let mut failures: Vec<String> = Vec::new();

    // 校验单条腿；返回可比哈希（None = 缺失/跳过）。
    let assert_leg = |label: &'static str,
                      built: PathBuf,
                      staged: PathBuf,
                      optional: bool,
                      failures: &mut Vec<String>|
     -> Option<String> {
        if !built.is_file() {
            if optional {
                println!(
                    "  [skip] {label} -- built artifact not present ({}); build it to enable this leg",
                    built.display()
                );
            } else {
                failures.push(format!(
                    "{label} : built artifact missing -> {} (run: make buildClientReactor)",
                    built.display()
                ));
            }
            return None;
        }
        if !staged.is_file() {
            failures.push(format!(
                "{label} : staged artifact missing -> {} (run the leg's build/copy step)",
                staged.display()
            ));
            return None;
        }
        let h_built = sha256_file(&built).unwrap_or_default();
        let h_staged = sha256_file(&staged).unwrap_or_default();
        if h_built != h_staged {
            failures.push(format!(
                "{label} : STALE staging -- built={} staged={}; {} is not a copy of {}",
                &h_built[..h_built.len().min(16)],
                &h_staged[..h_staged.len().min(16)],
                staged.display(),
                built.display()
            ));
            return Some(h_staged);
        }
        println!(
            "  [ok] {label}  sha256={}",
            &h_staged[..h_staged.len().min(16)]
        );
        Some(h_staged)
    };

    println!("[verify-deploy] deploy-leg staging consistency");

    let j = |parts: &[&str]| -> PathBuf {
        let mut p = repo.clone();
        for part in parts {
            for seg in part.split('/') {
                if !seg.is_empty() {
                    p = p.join(seg);
                }
            }
        }
        p
    };

    let legs: Vec<Leg> = vec![
        Leg {
            label: "backend       bin/settings.exe",
            hash: assert_leg(
                "backend       bin/settings.exe",
                j(&[&release_dir, "settings.exe"]),
                j(&[&bin_root, "settings.exe"]),
                false,
                &mut failures,
            ),
        },
        Leg {
            label: "panel         bin/ui/KeyFlux.Settings.exe",
            hash: assert_leg(
                "panel         bin/ui/KeyFlux.Settings.exe",
                j(&[&release_dir, "keyflux-settings.exe"]),
                j(&[&bin_root, "ui/KeyFlux.Settings.exe"]),
                false,
                &mut failures,
            ),
        },
        Leg {
            label: "command-input bin/KeyFlux-CommandInput.exe",
            hash: assert_leg(
                "command-input bin/KeyFlux-CommandInput.exe",
                j(&["command-input/target/release/keyflux-command-input.exe"]),
                j(&[&bin_root, "KeyFlux-CommandInput.exe"]),
                true,
                &mut failures,
            ),
        },
    ];

    // 反向校验：已暂存的腿必须两两 distinct —— 相等哈希 = 一个 exe 落进了两个槽位。
    let mut seen: Vec<(String, &'static str)> = Vec::new();
    for leg in &legs {
        if let Some(h) = &leg.hash {
            let short = leg_label_short(leg.label);
            if let Some((_, prev)) = seen.iter().find(|(ph, _)| ph == h) {
                failures.push(format!(
                    "staged '{short}' and '{prev}' are IDENTICAL (sha256={}) -- one exe was copied into two legs",
                    &h[..h.len().min(16)]
                ));
            } else {
                seen.push((h.clone(), short));
            }
        }
    }

    if !failures.is_empty() {
        for f in &failures {
            println!("[FAIL] {f}");
        }
        eprintln!(
            "[FAIL] verify-deploy: {} problem(s) -- deploy legs are inconsistent",
            failures.len()
        );
        return 1;
    }

    println!("[verify-deploy] OK -- legs consistent and distinct");
    0
}

/// PS 侧 distinct 失败消息用的是短标签（'backend'/'panel'/'command-input'）。
fn leg_label_short(label: &str) -> &'static str {
    if label.starts_with("backend") {
        "backend"
    } else if label.starts_with("panel") {
        "panel"
    } else {
        "command-input"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_label_mapping() {
        assert_eq!(leg_label_short("backend       bin/settings.exe"), "backend");
        assert_eq!(
            leg_label_short("panel         bin/ui/KeyFlux.Settings.exe"),
            "panel"
        );
        assert_eq!(
            leg_label_short("command-input bin/KeyFlux-CommandInput.exe"),
            "command-input"
        );
    }

    /// 全部缺失（非 optional）⇒ 两条 failure ⇒ exit 1；optional 腿缺失只 skip。
    #[test]
    fn missing_required_legs_fail() {
        // 指向一个不存在的 release/bin 根，backend/panel 必失败，command-input 走 skip。
        let code = run(&[
            "--release-dir".into(),
            "__no_such_release__".into(),
            "--bin-root".into(),
            "__no_such_bin__".into(),
        ]);
        assert_eq!(code, 1);
    }
}
