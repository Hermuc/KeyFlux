//! 上游 windows-reactor 是否有更新 —— 原 `tools/check-upstream-reactor.ps1` 的 Rust 移植。
//!
//! `config-ui-reactor/Cargo.toml` 带着一个 vendored fork（`[patch.crates-io] windows-reactor =
//! { path = "vendor/windows-reactor" }`）。补丁面（P1..P8）对编译器/测试/CI **不可见**，故上游
//! 发新版时仓库里没有任何东西会察觉 —— fork 悄悄变旧。本工具把它变成可见、可操作的信号：
//! 上游更新 = "去 re-base fork"（见 vendor/windows-reactor/PATCHES.md §4）。
//!
//! 只**检测**，不自动 bump pin / 重放补丁（那是人工步骤，fork re-base 可能撞在任何地方）。
//! 退出码：0 = 上游仍在 pin 范围内；1 = 上游已越过 pin / 查询失败 / 找不到 pin。

use std::sync::OnceLock;

use regex::Regex;

use crate::util::{curl_get, repo_root};

/// 依赖需求正则，如 `windows-reactor = "0.100"`。锚定行首 ⇒ 不会误配 `windows-reactor-setup`；
/// `[patch.crates-io]` 那条用 `{ path = ... }`（非引号串）也不会被匹配。
/// 注：原 PS 脚本此正则**硬编码** `windows-reactor`（与 -Crate 参数无关），逐字保留。
fn re_pin() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"(?m)^\s*windows-reactor\s*=\s*"([^"]+)""#).expect("windows-reactor pin")
    })
}

/// CLI 入口。`args` 支持 `--crate <name>` / `--cargo-toml <path>`。
pub fn run(args: &[String]) -> i32 {
    let mut crate_name = "windows-reactor".to_string();
    let mut cargo_toml = "config-ui-reactor/Cargo.toml".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--crate" | "-Crate" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    crate_name = v.clone();
                }
            }
            "--cargo-toml" | "-CargoToml" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    cargo_toml = v.clone();
                }
            }
            _ => {}
        }
        i += 1;
    }

    let repo = repo_root();
    let toml_path = repo.join(cargo_toml.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !toml_path.is_file() {
        println!("[FAIL] not found: {}", toml_path.display());
        return 1;
    }
    let toml = match std::fs::read_to_string(&toml_path) {
        Ok(t) => t,
        Err(e) => {
            println!("[FAIL] not found: {} ({e})", toml_path.display());
            return 1;
        }
    };

    let Some(caps) = re_pin().captures(&toml) else {
        println!("[FAIL] could not find the windows-reactor version requirement in Cargo.toml");
        return 1;
    };
    let pinned = caps[1].to_string();
    println!("[deps-watch] pinned requirement: windows-reactor = \"{pinned}\"");

    // crates.io API（描述性 User-Agent 是 crates.io 政策要求）。HTTP 走系统 curl。
    let url = format!("https://crates.io/api/v1/crates/{crate_name}");
    let body = match curl_get(&url, "KeyFlux-deps-watch") {
        Ok(b) => b,
        Err(e) => {
            println!("[FAIL] crates.io query failed: {e}");
            return 1;
        }
    };
    let latest = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("crate")
                .and_then(|c| c.get("max_stable_version"))
                .and_then(|s| s.as_str())
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty());
    let Some(latest) = latest else {
        println!("[FAIL] crates.io returned no max_stable_version");
        return 1;
    };
    println!("[deps-watch] upstream max stable:  {latest}");

    // Cargo caret 语义："0.100" = ">=0.100.0, <0.101.0"。故 pin 被 `0.100` 或 `0.100.<patch>`
    // 满足；major/minor 不同 = 上游已越过。
    let in_range = latest == pinned || latest.starts_with(&format!("{pinned}."));
    if in_range {
        println!("[deps-watch] OK -- upstream is still within the pinned requirement");
        return 0;
    }

    println!("[deps-watch] NEWER upstream windows-reactor available: {latest} (pin = {pinned})");
    println!(
        "[deps-watch] action: re-base the vendored fork per vendor/windows-reactor/PATCHES.md section 4,"
    );
    println!(
        "[deps-watch]         then update the pin + PATCHES.md (upstream version/sha, diff surface, commit)."
    );
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_regex_matches_dep_not_setup_or_patch() {
        let toml = "[dependencies]\nwindows-reactor = \"0.100\"\nwindows-reactor-setup = \"0.100\"\n[patch.crates-io]\nwindows-reactor = { path = \"vendor/windows-reactor\" }\n";
        let caps = re_pin().captures(toml).expect("should match");
        assert_eq!(&caps[1], "0.100");
        // 只应命中带引号串的那条（-setup 名字不同不匹配；patch 用 { path } 非引号串）
        let all: Vec<String> = re_pin()
            .captures_iter(toml)
            .map(|c| c[1].to_string())
            .collect();
        assert_eq!(
            all,
            vec!["0.100".to_string()],
            "只应命中 windows-reactor = \"0.100\""
        );
    }

    #[test]
    fn caret_range_semantics() {
        let in_range = |latest: &str, pinned: &str| {
            latest == pinned || latest.starts_with(&format!("{pinned}."))
        };
        assert!(in_range("0.100", "0.100"));
        assert!(in_range("0.100.3", "0.100"));
        assert!(!in_range("0.101", "0.100"));
        assert!(!in_range("0.101.0", "0.100"));
        assert!(!in_range("1.0.0", "0.100"));
    }

    /// 找不到 Cargo.toml ⇒ exit 1。
    #[test]
    fn missing_toml_fails() {
        assert_eq!(run(&["--cargo-toml".into(), "__nope__.toml".into()]), 1);
    }
}
