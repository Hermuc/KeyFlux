//! 发布运维 CLI —— 原 Go `scripts/build_tools.go` 的 Rust 等价物（Go 工具链已退役）。
//!
//! 本 bin 是**薄壳**：逻辑在 lib（`config_ui_reactor::services::build_tools`，带单测），
//! 这里只做 argv → 退出码映射与契约文案（`--quiet` 由 `tools/build-tools.ps1` 保证
//! cargo 自身不往 stdout 写东西）。
//!
//! 用法（**cwd 必须是仓库根** —— 两个子命令都用相对路径，与 Go 版一致）：
//! ```text
//! build-tools checkForAHKUpdate <version>
//! build-tools updateShareLink   <version> [siteDocPath]
//! ```
//!
//! 退出码 / 文案（**逐字契约**，与 Go 版一致）：
//! * `checkForAHKUpdate`：相等 → 0；不匹配 → stdout `error: outdated ahk version` + 1；
//!   网络或读失败 → stderr 原因 + 2（Go 版此处 panic ⇒ exit 2）。
//! * `updateShareLink`：成功 → 0（站点文档成功 → 静默；未提供路径 → stdout
//!   `skip site doc update: pass the doc path as 2nd arg or set KEYFLUX_SITE_DOC`；
//!   提供了但失败 → stdout `update site doc failed: <err>`，**不影响退出码**）；
//!   `share_link.json` 缺失/非法/字段空 → stderr 原因 + 2。
//! * 未知子命令 → 静默 0（Go 版 `map` 未命中即返回，历史行为，勿改）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use config_ui_reactor::services::build_tools;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        eprintln!("usage: build-tools <checkForAHKUpdate|updateShareLink> [args...]");
        return ExitCode::from(2);
    };
    let rest = &args[2..];

    match command {
        "checkForAHKUpdate" => {
            let Some(version) = rest.first() else {
                eprintln!("usage: build-tools checkForAHKUpdate <version>");
                return ExitCode::from(2);
            };
            match build_tools::check_for_ahk_update(version) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) if error == build_tools::OUTDATED => {
                    // Go: fmt.Println("error: outdated ahk version") 后 os.Exit(1)
                    println!("error: {error}");
                    ExitCode::from(1)
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(2)
                }
            }
        }
        "updateShareLink" => {
            let Some(version) = rest.first() else {
                eprintln!("usage: build-tools updateShareLink <version> [siteDocPath]");
                return ExitCode::from(2);
            };
            // 站点文档路径：第 2 参数优先，其次环境变量（Go 同）
            let site_doc: Option<PathBuf> = rest
                .get(1)
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .or_else(|| {
                    std::env::var(build_tools::SITE_DOC_ENV)
                        .ok()
                        .filter(|value| !value.is_empty())
                        .map(PathBuf::from)
                });

            match build_tools::update_share_link(
                version,
                Path::new(build_tools::SHARE_LINK_FILE),
                Path::new("readme.md"),
                site_doc.as_deref(),
            ) {
                Ok(build_tools::SiteDocOutcome::Skipped) => {
                    println!(
                        "skip site doc update: pass the doc path as 2nd arg or set {}",
                        build_tools::SITE_DOC_ENV
                    );
                    ExitCode::SUCCESS
                }
                Ok(build_tools::SiteDocOutcome::Updated) => ExitCode::SUCCESS,
                Ok(build_tools::SiteDocOutcome::Failed(error)) => {
                    println!("update site doc failed: {error}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(2)
                }
            }
        }
        // Go 版未命中 map 即静默返回 ⇒ 保持一致（历史行为，勿改）
        _ => ExitCode::SUCCESS,
    }
}
