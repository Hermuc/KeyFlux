//! 依赖供应链审计 —— 原 `tools/check-deps.ps1` 的 Rust 移植。
//!
//! 对仓库里每个 cargo 工程跑：
//! * `cargo audit` —— RUSTSEC 漏洞库（已知漏洞）；
//! * `cargo deny --config <repo>/deny.toml check` —— advisories + licenses + bans + sources。
//!
//! 「装两个工具、再对每个 crate 跑一遍」的序列必须只存在一处（抄两份就会漂移，同
//! kf-tools.ps1 的教训）。CI job：deps-watch.yml（每周）；本地：`make check-deps`。
//!
//! 前置：cargo 在 PATH；cargo-audit 与 cargo-deny 已安装。需要网络（advisory DB + crates.io）。
//! 退出码：0 = 每个 crate 干净；非零 = 有发现，或缺必需工具。

use std::path::Path;
use std::process::Command;

use crate::util::repo_root;

/// 仓库内全部 cargo 工程（各自独立 Cargo.lock）。
const CRATES: &[&str] = &["config-ui-reactor", "command-input", "devtools"];

/// 工具是否在 PATH（尝试 `--version`；spawn 失败 = 未安装）。
fn tool_present(name: &str) -> bool {
    Command::new(name).arg("--version").output().is_ok()
}

/// 在 dir 下运行命令（继承 stdout/stderr，令审计输出可见）；返回退出码。
fn run_in(dir: &Path, program: &str, args: &[&str]) -> i32 {
    match Command::new(program).args(args).current_dir(dir).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("{program} 启动失败: {e}");
            1
        }
    }
}

/// CLI 入口。`args` 支持 `-SkipUnlocked` / `--skip-unlocked`。
pub fn run(args: &[String]) -> i32 {
    let skip_unlocked = args
        .iter()
        .any(|a| a == "-SkipUnlocked" || a == "--skip-unlocked");

    let repo = repo_root();
    let config = repo.join("deny.toml");
    if !config.is_file() {
        println!("[FAIL] missing config: {}", config.display());
        return 1;
    }
    let config_str = config.to_string_lossy().to_string();

    if !tool_present("cargo") {
        println!("[FAIL] cargo not on PATH");
        return 1;
    }
    let audit = tool_present("cargo-audit");
    let deny = tool_present("cargo-deny");
    if !audit && !deny {
        println!("[FAIL] neither cargo-audit nor cargo-deny installed.");
        println!("       install: cargo install cargo-audit cargo-deny");
        return 1;
    }

    let mut failed = 0i32;
    for krate in CRATES {
        let dir = repo.join(krate);
        if !dir.join("Cargo.lock").is_file() {
            if skip_unlocked {
                println!("  [skip] {krate} (no Cargo.lock)");
                continue;
            }
            println!("[FAIL] {krate} has no Cargo.lock -- cannot audit");
            failed += 1;
            continue;
        }
        if audit {
            println!("== cargo audit @ {krate}");
            if run_in(&dir, "cargo", &["audit"]) != 0 {
                println!("[FAIL] cargo audit @ {krate}");
                failed += 1;
            }
        }
        if deny {
            println!("== cargo deny @ {krate} (config: deny.toml)");
            if run_in(&dir, "cargo", &["deny", "--config", &config_str, "check"]) != 0 {
                println!("[FAIL] cargo deny @ {krate}");
                failed += 1;
            }
        }
    }

    if failed > 0 {
        println!("[FAIL] dependency audit: {failed} finding(s)");
        return 1;
    }
    println!("[ok] dependency audit clean");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_presence_detects_missing_binary() {
        // 一个几乎不可能存在的工具名 ⇒ 缺席
        assert!(!tool_present("definitely-not-a-real-tool-xyz"));
    }

    #[test]
    fn crates_include_devtools() {
        assert!(CRATES.contains(&"devtools"));
        assert!(CRATES.contains(&"config-ui-reactor"));
        assert!(CRATES.contains(&"command-input"));
    }
}
