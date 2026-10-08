//! 开发/CI 工具 CLI —— 原 `tools/*.py` 脚本的 Rust 移植入口。
//!
//! 独立轻量 crate（仅 `regex` + `serde_json`，无 WinUI）：逻辑在各模块（带单测），
//! 本 bin 是**薄壳**，只做 argv → 退出码映射。刻意与 config-ui-reactor 分离，令
//! CI 闸门 job（`make lint` / `check-texttypes` / `lint-ahk-style`）秒级构建。
//!
//! 用法（cwd 必须是**仓库根** —— 各子命令用仓库相对路径，与原脚本一致）：
//! ```text
//! devtools patch-command-input <exe> [--check] [--revert]
//! devtools lint-ident <file.ahk>...
//! devtools texttype-conformance [--repo <p>] [--ahk <p>] [--static-only]
//! devtools lint-ahk-style [--repo <p>] [--report] [--write-baseline] [--verbose]
//! ```

mod check_deps;
mod check_freshness;
mod check_upstream_reactor;
mod check_vendor_hashes;
mod lint_ahk_style;
mod lint_ident;
mod patch_command_input;
mod sync_plugins;
mod texttype;
mod util;
mod verify_deploy;

use std::process::ExitCode;

const USAGE: &str = "usage: devtools <patch-command-input|lint-ident|texttype-conformance|lint-ahk-style|check-vendor-hashes|verify-deploy|check-upstream-reactor|check-freshness|check-deps|sync-plugins> [args...]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let rest = &args[2..];

    let code = match command {
        "patch-command-input" => patch_command_input::run(rest),
        "lint-ident" => lint_ident::run(rest),
        "texttype-conformance" => texttype::run(rest),
        "lint-ahk-style" => lint_ahk_style::run(rest),
        "check-vendor-hashes" => check_vendor_hashes::run(rest),
        "verify-deploy" => verify_deploy::run(rest),
        "check-upstream-reactor" => check_upstream_reactor::run(rest),
        "check-freshness" => check_freshness::run(rest),
        "check-deps" => check_deps::run(rest),
        "sync-plugins" => sync_plugins::run(rest),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    ExitCode::from(code as u8)
}
