//! 命令框字体安装 —— Go `internal/script/font.go` 的移植。
//!
//! 落点 `font/font.ttf` **不可配置**：命令框 exe 内以 UTF-16 字面量硬编码
//! `font\font.ttf`，相对 exe 自身目录解析（`docs/CONTRACTS.md` §3.11.1）。
//!
//! 行为要点（与 Go 逐条对应）：
//! * **失败一律由调用方静默跳过**（本函数返回 `Err`，Go 侧调用是 `_ = InstallCommandFont`
//!   且注释写明"字体是纯表现层资源，不该让生成失败"）⇒ Rust 侧调用同样用 `let _ =`；
//! * 仅接受 **TrueType glyf** 轮廓：sfnt 签名 `0x00010000` / `'true'`；**CFF `'OTTO'` 一律拒绝**
//!   （命令框 exe 硬编码 TrueType face 类型，CFF 会在下游静默加载失败）；
//! * `ttcf` 集合解包一层：`numFonts` 上限 4096（与 C# `FaceCountProbeLimit` 同口径），
//!   首 face 偏移判据是 `off > size-4`（**不是** `>=` —— 恰好剩 4 字节是合法的，C# 侧
//!   2026-09-21 一并订正过）；
//! * 字重档位对应**预烘焙变体**（`<源名>.<档位>.ttf`，由 `tools/font_weight_prebake.py`
//!   生成），变体缺失/不合法 ⇒ **回落源字体本体**；`regular` 刻意映射空串（半径 0 就是源）。
//!
//! ⚠️ `NormalizeFontWeight` 是**精确匹配**（不做大小写归一）：未知/空 ⇒ `"regular"`。

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Go `script.FontTargetRel`：命令框字体落地相对路径（不可配置）。
pub const FONT_TARGET_REL: &str = "font/font.ttf";

/// Go `script.FontMaxBytes`：32 MiB（仅挡明显误选）。
pub const FONT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Go `fontCollectionFaceLimit`：字体集合允许的最大 face 数（与 C# 同口径）。
const FONT_COLLECTION_FACE_LIMIT: u32 = 4096;

/// Go `script.FontWeightVariants`：档位 -> 变体名（空串 = 用源字体本体）。
const FONT_WEIGHT_VARIANTS: [(&str, &str); 5] = [
    ("thin", "thin"),
    ("light", "light"),
    ("regular", ""),
    ("semibold", "semibold"),
    ("bold", "bold"),
];

/// Go `NormalizeFontWeight`：精确匹配档位名，未知/空回落 `regular`。
pub fn normalize_font_weight(weight: &str) -> String {
    if FONT_WEIGHT_VARIANTS.iter().any(|(name, _)| *name == weight) {
        weight.to_string()
    } else {
        "regular".to_string()
    }
}

/// Go `filepath.Ext` 的等价实现（含 `\` 与 `/` 两种分隔符，从末尾向前找最后一个 `.`）。
fn path_ext(path: &str) -> &str {
    let bytes = path.as_bytes();
    let mut index = bytes.len();
    while index > 0 {
        let byte = bytes[index - 1];
        if byte == b'.' {
            return &path[index - 1..];
        }
        if byte == b'\\' || byte == b'/' {
            break;
        }
        index -= 1;
    }
    ""
}

/// Go `VariantPath`：`<去扩展名>.<档位>.ttf`；`regular`（无变体）⇒ 空串。
pub fn variant_path(src: &str, weight: &str) -> String {
    let normalized = normalize_font_weight(weight);
    let Some((_, variant)) = FONT_WEIGHT_VARIANTS
        .iter()
        .find(|(name, _)| *name == normalized)
    else {
        return String::new();
    };
    if variant.is_empty() {
        return String::new();
    }
    let ext = path_ext(src);
    let stem = &src[..src.len() - ext.len()];
    format!("{stem}.{variant}{ext}")
}

/// Go `samePath`：任一侧 stat 失败 ⇒ 不相同（调用方据此继续复制）。
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Go `classifyFontKinds` 的结果：接受 / 拒绝（带原因）。
#[derive(Debug, PartialEq, Eq)]
pub enum FontKind {
    Accepted,
    Rejected(String),
}

/// Go `classifyFontKinds`：按 sfnt 签名判定轮廓格式（`ttcf` 递归解包一层）。
pub fn classify_font_kinds(path: &Path, depth: u32) -> io::Result<FontKind> {
    let mut file = std::fs::File::open(path)?;
    let mut head = [0u8; 4];
    file.read_exact(&mut head)?;

    match &head {
        [0x00, 0x01, 0x00, 0x00] => Ok(FontKind::Accepted),
        b"true" => Ok(FontKind::Accepted),
        b"OTTO" => Ok(FontKind::Rejected(
            "CFF/OpenType(OTTO) 轮廓不受支持, 命令框只接受 TrueType glyf (可用 tools/font_otf2ttf.py 转换)"
                .to_string(),
        )),
        b"ttcf" => {
            if depth > 0 {
                return Ok(FontKind::Rejected("字体集合嵌套过深, 无法判定轮廓格式".to_string()));
            }
            // TTC 头: tag(4) + version(4) + numFonts(4) + offsetTable[numFonts](4 each)
            file.seek(SeekFrom::Start(0))?;
            let mut header = [0u8; 12];
            file.read_exact(&mut header)?;
            let num_fonts = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
            if num_fonts == 0 {
                return Ok(FontKind::Rejected("字体集合为空 (numFonts=0)".to_string()));
            }
            if num_fonts > FONT_COLLECTION_FACE_LIMIT {
                return Ok(FontKind::Rejected(
                    "字体集合的 face 数超出上限, 文件疑似损坏".to_string(),
                ));
            }
            let mut offset = [0u8; 4];
            file.read_exact(&mut offset)?;
            let size = file.metadata()?.len();
            let face_offset = u64::from(u32::from_be_bytes(offset));
            // 判据是 `off > size-4`（不是 `>=`）：恰好剩 4 字节时读取合法（与 C# 同口径）。
            if face_offset == 0 || face_offset > size - 4 {
                return Ok(FontKind::Rejected(
                    "字体集合的首 face 偏移越界, 文件疑似损坏".to_string(),
                ));
            }
            let mut face_tag = [0u8; 4];
            file.seek(SeekFrom::Start(face_offset))?;
            file.read_exact(&mut face_tag)?;
            match &face_tag {
                [0x00, 0x01, 0x00, 0x00] => Ok(FontKind::Accepted),
                b"true" => Ok(FontKind::Accepted),
                b"OTTO" => Ok(FontKind::Rejected(
                    "字体集合的首个 face 是 CFF/OpenType(OTTO) 轮廓, 不受支持".to_string(),
                )),
                _ => Ok(FontKind::Rejected(
                    "字体集合的 face 轮廓格式无法识别".to_string(),
                )),
            }
        }
        _ => Ok(FontKind::Rejected(
            "不是可识别的字体文件 (未知 sfnt 签名)".to_string(),
        )),
    }
}

/// Go `InstallCommandFont`：按配置把字体复制到命令框落点。
///
/// 调用方**必须**忽略返回值（`let _ =`），与 Go 的静默跳过口径一致 —— 字体是纯表现层
/// 资源，任何失败都不应阻断生成。
pub fn install_command_font(
    option: &crate::generator::model::CommandFontOption,
    base_dir: &str,
) -> Result<(), String> {
    // 1. 未自定义 ⇒ 不动部署目录的 font.ttf
    if option.source_path.is_empty() {
        return Ok(());
    }

    let mut src = option.source_path.clone();
    if !Path::new(&src).is_absolute() && !base_dir.is_empty() {
        src = Path::new(base_dir)
            .join(&src)
            .to_string_lossy()
            .into_owned();
    }
    let mut dst = FONT_TARGET_REL.to_string();
    if !base_dir.is_empty() {
        dst = Path::new(base_dir)
            .join(FONT_TARGET_REL)
            .to_string_lossy()
            .into_owned();
    }

    // 2. 源不存在/是目录 ⇒ 跳过
    let source_info = match std::fs::metadata(&src) {
        Ok(info) if !info.is_dir() => info,
        Ok(_) => {
            return Err(format!("命令框字体源不可用, 沿用现有字体: {src}: 是目录"));
        }
        Err(error) => {
            return Err(format!("命令框字体源不可用, 沿用现有字体: {src}: {error}"));
        }
    };
    // 3. 体积超上限 ⇒ 跳过
    if source_info.len() > FONT_MAX_BYTES {
        return Err(format!(
            "命令框字体源超过 {} 字节上限 ({}), 疑似误选, 沿用现有字体: {}",
            FONT_MAX_BYTES,
            source_info.len(),
            src
        ));
    }

    // 字重：先按档位试预烘焙变体，缺失/不合法 ⇒ 回落源字体本体
    let weight = normalize_font_weight(&option.weight);
    let mut selected = src.clone();
    let variant = variant_path(&src, &weight);
    if !variant.is_empty()
        && let Ok(variant_info) = std::fs::metadata(&variant)
        && !variant_info.is_dir()
        && variant_info.len() <= FONT_MAX_BYTES
        && matches!(
            classify_font_kinds(Path::new(&variant), 0),
            Ok(FontKind::Accepted)
        )
    {
        selected = variant;
    }

    // 5. 源已就位（自复制会把目标截断为 0 字节）
    if same_path(Path::new(&selected), Path::new(&dst)) {
        return Ok(());
    }

    // 4. 字体格式校验（必须 glyf，命令框 exe 只认 TrueType）
    match classify_font_kinds(Path::new(&selected), 0) {
        Err(error) => {
            return Err(format!(
                "命令框字体源读取失败, 沿用现有字体: {selected}: {error}"
            ));
        }
        Ok(FontKind::Rejected(reason)) => {
            return Err(format!(
                "命令框字体源不被接受, 沿用现有字体: {selected}: {reason}"
            ));
        }
        Ok(FontKind::Accepted) => {}
    }

    // 6. 目标目录可能不存在（老部署树没有 bin/font/）
    let target = PathBuf::from(&dst);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("命令框字体落点目录创建失败: {error}"))?;
    }
    let data = std::fs::read(&selected).map_err(|error| format!("命令框字体读取失败: {error}"))?;
    // 权限 0644（便携式部署，无多用户隔离需求）
    std::fs::write(&target, data).map_err(|error| format!("命令框字体写入失败: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_font_weight_is_exact_match() {
        // ⚠️ 精确匹配、不做大小写归一：未知/空一律回落 regular
        assert_eq!(normalize_font_weight("bold"), "bold");
        assert_eq!(normalize_font_weight("semibold"), "semibold");
        assert_eq!(normalize_font_weight(""), "regular");
        assert_eq!(normalize_font_weight("SemiBold"), "regular");
        assert_eq!(normalize_font_weight("extra"), "regular");
    }

    #[test]
    fn variant_path_appends_weight_and_regular_is_empty() {
        assert_eq!(
            variant_path("C:/x/Sarasa.ttf", "semibold"),
            "C:/x/Sarasa.semibold.ttf"
        );
        assert_eq!(
            variant_path("C:/x/Sarasa.ttf", "thin"),
            "C:/x/Sarasa.thin.ttf"
        );
        assert_eq!(variant_path("C:/x/Sarasa.ttf", "regular"), "");
        assert_eq!(variant_path("C:/x/Sarasa.ttf", "unknown"), "");
        // 无扩展名源
        assert_eq!(variant_path("C:/x/fontfile", "bold"), "C:/x/fontfile.bold");
    }

    /// 合成 sfnt 头验证轮廓判定（无需真实字体文件）。
    #[test]
    fn classify_font_kinds_accepts_truetype_and_rejects_cff() {
        let dir = std::env::temp_dir().join(format!("kf-font-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let write = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };

        // TrueType 1.0
        let ttf = write("a.ttf", &[0x00, 0x01, 0x00, 0x00, 0, 0, 0, 0]);
        assert!(matches!(
            classify_font_kinds(&ttf, 0),
            Ok(FontKind::Accepted)
        ));
        // 'true'
        let true_font = write("b.ttf", b"truexxxx");
        assert!(matches!(
            classify_font_kinds(&true_font, 0),
            Ok(FontKind::Accepted)
        ));
        // CFF OTTO ⇒ 拒绝
        let otto = write("c.otf", b"OTTOxxxx");
        assert!(matches!(
            classify_font_kinds(&otto, 0),
            Ok(FontKind::Rejected(reason)) if reason.contains("OTTO")
        ));
        // 未知签名
        let unknown = write("d.bin", b"JUNKxxxx");
        assert!(matches!(
            classify_font_kinds(&unknown, 0),
            Ok(FontKind::Rejected(reason)) if reason.contains("未知 sfnt 签名")
        ));

        // TTC：numFonts=1，face 偏移 12 指向 TrueType 签名
        let mut ttc = Vec::new();
        ttc.extend_from_slice(b"ttcf");
        ttc.extend_from_slice(&[0, 1, 0, 0]); // version
        ttc.extend_from_slice(&[0, 0, 0, 1]); // numFonts = 1
        ttc.extend_from_slice(&16u32.to_be_bytes()); // face offset = 16（offsetTable[0]）
        ttc.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]); // face tag
        let ttc_path = write("e.ttc", &ttc);
        assert!(matches!(
            classify_font_kinds(&ttc_path, 0),
            Ok(FontKind::Accepted)
        ));

        // TTC：首 face 是 OTTO ⇒ 拒绝
        let mut bad = ttc.clone();
        bad.splice(16..20, b"OTTO".iter().copied());
        let bad_path = write("f.ttc", &bad);
        assert!(matches!(
            classify_font_kinds(&bad_path, 0),
            Ok(FontKind::Rejected(reason)) if reason.contains("OTTO")
        ));

        // TTC：首 face 偏移越界 ⇒ 拒绝（判据 `off > size-4`，与 C# 同口径）
        let mut oob = ttc.clone();
        oob.splice(12..16, 999u32.to_be_bytes());
        let oob_path = write("g.ttc", &oob);
        assert!(matches!(
            classify_font_kinds(&oob_path, 0),
            Ok(FontKind::Rejected(reason)) if reason.contains("偏移越界")
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_command_font_skips_when_source_missing_or_empty() {
        // sourcePath 为空 ⇒ 直接成功（不动部署目录）
        let empty = crate::generator::model::CommandFontOption::default();
        assert!(install_command_font(&empty, "").is_ok());
        assert!(install_command_font(&empty, "C:/nonexistent-base").is_ok());

        // 源不存在 ⇒ Err（调用方忽略），且错误信息可诊断
        let missing = crate::generator::model::CommandFontOption {
            source_path: "font/definitely-missing.ttf".into(),
            weight: "regular".into(),
        };
        let error = install_command_font(&missing, "").unwrap_err();
        assert!(error.contains("命令框字体源不可用"), "{error}");
    }
}
