//! 发布运维工具的可复用逻辑 —— 原 Go `scripts/build_tools.go` 的 **Rust 移植**。
//!
//! 为什么逻辑放 lib 而 bin 只做薄壳：bin 独享的 `pub` 项既进不了单测，也会触发
//! `clippy -D warnings` 的 `dead_code`（与 `Cargo.toml` 里 generator 的既有理由同源）。
//! CLI 层（`src/bin/build_tools.rs`）只负责 argv → 退出码映射。
//!
//! **逐字契约**（Go 版行为，`:make_uploadLanZou` 两行调用；勿改）：
//! * `check_for_ahk_update`：GET 线上版本号，与入参**整串比较（不 trim）**，不等即"过期"；
//! * `update_share_link`：读 cwd 的 `share_link.json`（`{url,password}`，任一为空即错），
//!   把 `readme.md` 中**含「提取码」的行**改写为分享链接行；站点文档路径取第 2 参数或
//!   环境变量 `KEYFLUX_SITE_DOC`（都缺则跳过，不算错）；
//! * 文件改写语义与 Go `ReplaceInFile` 一致：逐行（**去行尾 `\r`**）→ 每行补 `\n`
//!   → 写同目录临时文件 → 原子 rename 覆盖。即"行尾统一 `\n`、末尾必有一个 `\n`"。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use super::http::platform_agent;

/// AHK 线上版本号地址（与 Go 版同址）。
pub const AHK_VERSION_URL: &str = "https://www.autohotkey.com/download/2.0/version.txt";
/// 网络超时（对齐 Go 版 `http.Client{Timeout: 5s}`）。
const AHK_TIMEOUT: Duration = Duration::from_secs(5);
/// 需要改写的行的判别标记（与 Go 版同）。
const SHARE_MARKER: &str = "提取码";
/// 分享链接 JSON 的文件名（由 `scripts/lanzou_client.py` 产出，相对 cwd）。
pub const SHARE_LINK_FILE: &str = "share_link.json";
/// 站点文档路径的环境变量名（Go 版同）。
pub const SITE_DOC_ENV: &str = "KEYFLUX_SITE_DOC";
/// 版本过期的**稳定**错误文案（bin 依此映射 exit 1，见 CLI 层）。
pub const OUTDATED: &str = "outdated ahk version";

/// `share_link.json` 的结构（Go `ShareLink`）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ShareLink {
    pub url: String,
    pub password: String,
}

impl ShareLink {
    /// Go 的 `Valid.OK()`：两字段都不允许为空。
    pub fn validated(self) -> Result<Self, String> {
        if self.url.is_empty() {
            return Err("url required".to_string());
        }
        if self.password.is_empty() {
            return Err("password required".to_string());
        }
        Ok(self)
    }
}

/// 拉取 AHK 线上版本号（**不 trim**：Go 版逐字比较，换行/空白都算不匹配）。
pub fn fetch_ahk_version() -> Result<String, String> {
    let agent = platform_agent(AHK_TIMEOUT);
    let mut response = agent
        .get(AHK_VERSION_URL)
        .call()
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    Ok(text)
}

/// 版本比较口径：**逐字相等**（Go 版直接比较读出内容，**不 trim** ——
/// 线上文件若带换行/空白，Go 也判过期）。拆成独立函数是为了可单测（网络部分不参与单测）。
pub fn version_is_outdated(online: &str, current: &str) -> bool {
    online != current
}

/// 校验 AHK 是否过期。`Err(OUTDATED)` = 版本不匹配（CLI 映射为 exit 1）；
/// 其余 `Err` = 网络/IO 失败（CLI 映射为 exit 2，对齐 Go 的 panic）。
pub fn check_for_ahk_update(current: &str) -> Result<(), String> {
    let online = fetch_ahk_version()?;
    if version_is_outdated(&online, current) {
        return Err(OUTDATED.to_string());
    }
    Ok(())
}

/// 分享链接行（`with_prefix` = 站点文档用的「下载地址: 」前缀版）。
pub fn share_line(version: &str, link: &ShareLink, with_prefix: bool) -> String {
    if with_prefix {
        format!(
            "- 下载地址: [KeyFlux {version}]({url}) ( 提取码 {password} )",
            url = link.url,
            password = link.password
        )
    } else {
        format!(
            "- [KeyFlux {version}]({url}) ( 提取码 {password} )",
            url = link.url,
            password = link.password
        )
    }
}

/// 站点文档的处置结果（CLI 负责打印对应文案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteDocOutcome {
    /// 未提供路径（第 2 参数与环境变量都缺）—— 跳过不是错误。
    Skipped,
    /// 已改写。
    Updated,
    /// 提供了路径但改写失败（Go 只记日志、不影响退出码）。
    Failed(String),
}

/// 读 `share_link.json` 并改写 `readme`（+ 可选站点文档）。
///
/// * `link_path` 通常传 `SHARE_LINK_FILE`（相对 cwd，与 Go 一致）；
/// * `site_doc = None` 时按 Go 语义跳过并返回 `SiteDocOutcome::Skipped`。
pub fn update_share_link(
    version: &str,
    link_path: &Path,
    readme: &Path,
    site_doc: Option<&Path>,
) -> Result<SiteDocOutcome, String> {
    let raw = fs::read_to_string(link_path).map_err(|error| error.to_string())?;
    let link: ShareLink = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    let link = link.validated()?;

    let plain = share_line(version, &link, false);
    replace_lines_in_file(readme, |line| {
        if contains_marker(line, SHARE_MARKER) {
            plain.as_bytes().to_vec()
        } else {
            line.to_vec()
        }
    })?;

    let Some(site_doc) = site_doc else {
        return Ok(SiteDocOutcome::Skipped);
    };
    let prefixed = share_line(version, &link, true);
    match replace_lines_in_file(site_doc, |line| {
        if contains_marker(line, SHARE_MARKER) {
            prefixed.as_bytes().to_vec()
        } else {
            line.to_vec()
        }
    }) {
        Ok(()) => Ok(SiteDocOutcome::Updated),
        Err(error) => Ok(SiteDocOutcome::Failed(error)),
    }
}

/// 字节级子串查找（等价 Go `strings.Index`）—— 避免对非 UTF-8 内容做有损转换。
fn contains_marker(line: &[u8], marker: &str) -> bool {
    let needle = marker.as_bytes();
    line.len() >= needle.len() && line.windows(needle.len()).any(|window| window == needle)
}

/// 逐行改写并**原子替换**（语义逐字对齐 Go `ReplaceInFile`）。
///
/// 行切分与 Go `bufio.ScanLines` 一致：按 `\n` 切分、**去掉行尾 `\r`**、
/// 末尾的换行不产生额外空行；输出每行补 `\n`（故结果必定以 `\n` 结尾）。
pub fn replace_lines_in_file(
    path: &Path,
    handler: impl Fn(&[u8]) -> Vec<u8>,
) -> Result<(), String> {
    let content = fs::read(path).map_err(|error| error.to_string())?;

    let mut out: Vec<u8> = Vec::with_capacity(content.len() + 64);
    if !content.is_empty() {
        let mut segments = content.split(|&byte| byte == b'\n').peekable();
        while let Some(segment) = segments.next() {
            // 末段为空 = 原文件以 `\n` 结尾 ⇒ Go 的 Scanner 不会产出该空行
            if segments.peek().is_none() && segment.is_empty() {
                break;
            }
            let line = match segment.last() {
                Some(b'\r') => &segment[..segment.len() - 1],
                _ => segment,
            };
            out.extend_from_slice(&handler(line));
            out.push(b'\n');
        }
    }

    let temp = temp_path(path);
    {
        let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
        use std::io::Write as _;
        file.write_all(&out).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })
}

/// 同目录临时文件名（同目录 = rename 不跨卷；名字带 pid 避免并发互踩）。
fn temp_path(path: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".kf-build-tools.");
    name.push(format!("{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kf-build-tools-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn upper(line: &[u8]) -> Vec<u8> {
        line.to_ascii_uppercase()
    }

    /// Go `ReplaceInFile` 的行语义：`\n` 结尾、去行尾 `\r`、末尾必补一个 `\n`。
    #[test]
    fn replace_lines_matches_go_semantics() {
        let dir = unique_dir("semantics");
        let file = dir.join("a.txt");

        fs::write(&file, b"alpha\r\nbeta\n").unwrap();
        replace_lines_in_file(&file, upper).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"ALPHA\nBETA\n");

        // 末尾无换行 ⇒ 输出补一个
        fs::write(&file, b"solo").unwrap();
        replace_lines_in_file(&file, upper).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"SOLO\n");

        // 空文件 ⇒ 保持空（Go 的 Scanner 无行可产）
        fs::write(&file, b"").unwrap();
        replace_lines_in_file(&file, upper).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"");

        // 连续空行保留（不被折叠）
        fs::write(&file, b"a\n\nb\n").unwrap();
        replace_lines_in_file(&file, upper).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"A\n\nB\n");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 只有含「提取码」的行被替换，其余行逐字节保留。
    #[test]
    fn only_marker_lines_are_replaced() {
        let dir = unique_dir("marker");
        let file = dir.join("readme.md");
        fs::write(&file, "标题\n- [KeyFlux 1.0](old) ( 提取码 abc )\n尾行\n").unwrap();

        let link = ShareLink {
            url: "https://example.com/f".to_string(),
            password: "pwd".to_string(),
        };
        let line = share_line("1.0-beta1", &link, false);
        replace_lines_in_file(&file, |raw| {
            if contains_marker(raw, SHARE_MARKER) {
                line.as_bytes().to_vec()
            } else {
                raw.to_vec()
            }
        })
        .unwrap();

        let text = fs::read_to_string(&file).unwrap();
        assert_eq!(
            text,
            "标题\n- [KeyFlux 1.0-beta1](https://example.com/f) ( 提取码 pwd )\n尾行\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 版本比较**不 trim**（Go 口径）：线上内容带换行/空白即判过期。
    #[test]
    fn version_compare_is_verbatim() {
        assert!(!version_is_outdated("2.0.29", "2.0.29"));
        assert!(version_is_outdated("2.0.29\n", "2.0.29"));
        assert!(version_is_outdated("2.0.19", "2.0.29"));
        assert!(version_is_outdated("", "2.0.29"));
    }

    /// 两条格式化串逐字（Go 的 `format` 两张：readme / 站点文档）。
    #[test]
    fn share_line_formats_are_locked() {
        let link = ShareLink {
            url: "u".to_string(),
            password: "p".to_string(),
        };
        assert_eq!(
            share_line("v", &link, false),
            "- [KeyFlux v](u) ( 提取码 p )"
        );
        assert_eq!(
            share_line("v", &link, true),
            "- 下载地址: [KeyFlux v](u) ( 提取码 p )"
        );
    }

    /// `share_link.json` 校验：两字段任一为空即错（Go `Valid.OK()`）。
    #[test]
    fn share_link_validation() {
        let ok = ShareLink {
            url: "u".into(),
            password: "p".into(),
        };
        assert!(ok.clone().validated().is_ok());
        assert_eq!(
            ShareLink {
                url: String::new(),
                password: "p".into()
            }
            .validated()
            .unwrap_err(),
            "url required"
        );
        assert_eq!(
            ShareLink {
                url: "u".into(),
                password: String::new()
            }
            .validated()
            .unwrap_err(),
            "password required"
        );
    }

    /// 端到端：无站点文档 ⇒ Skipped；有站点文档 ⇒ Updated；路径不存在 ⇒ Failed（不算错）。
    #[test]
    fn update_share_link_outcomes() {
        let dir = unique_dir("e2e");
        let link_path = dir.join(SHARE_LINK_FILE);
        let readme = dir.join("readme.md");
        fs::write(&link_path, r#"{"url":"https://x/y","password":"pw"}"#).unwrap();
        fs::write(&readme, "- [KeyFlux 1](old) ( 提取码 zz )\n").unwrap();

        let outcome = update_share_link("9.9", &link_path, &readme, None).unwrap();
        assert_eq!(outcome, SiteDocOutcome::Skipped);
        assert_eq!(
            fs::read_to_string(&readme).unwrap(),
            "- [KeyFlux 9.9](https://x/y) ( 提取码 pw )\n"
        );

        let site = dir.join("site.md");
        fs::write(&site, "- [KeyFlux 1](old) ( 提取码 zz )\n").unwrap();
        let outcome = update_share_link("9.9", &link_path, &readme, Some(&site)).unwrap();
        assert_eq!(outcome, SiteDocOutcome::Updated);
        assert_eq!(
            fs::read_to_string(&site).unwrap(),
            "- 下载地址: [KeyFlux 9.9](https://x/y) ( 提取码 pw )\n"
        );

        let missing = dir.join("nope.md");
        let outcome = update_share_link("9.9", &link_path, &readme, Some(&missing)).unwrap();
        assert!(matches!(outcome, SiteDocOutcome::Failed(_)));

        // 链接文件缺失 ⇒ 整体 Err
        assert!(update_share_link("9.9", &dir.join("nope.json"), &readme, None).is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
