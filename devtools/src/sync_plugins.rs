//! 把 bundled（官方）插件同步进 `<OutDir>/data/plugins`，再执行 tombstone 清单 ——
//! 原 `tools/sync-plugins.ps1` 的 Rust 移植。
//!
//! tombstone（`config.options.plugins.removed`，2026-10-02 P4）：任何被用户删除的 bundled
//! 插件目录在复制**之后**再删掉，令"用户删了"胜过"随包出货"。
//!
//! 复制语义与 Makefile sync-plugins 的内联 robocopy 一致：**无 /MIR** —— 只增/改，绝不动
//! 用户导入的插件。退出码：0 = 成功（含无 config.json / config 不可读）；1 = 源缺失或 robocopy 失败。

use std::fs;
use std::path::PathBuf;

use crate::util::{repo_root, robocopy_ok, run_robocopy};

/// CLI 入口。`args` 第一个非旗标即 `-OutDir`（必填）。
pub fn run(args: &[String]) -> i32 {
    let mut out_dir: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-OutDir" | "--out-dir" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    out_dir = Some(v.clone());
                }
            }
            other => {
                if out_dir.is_none() {
                    out_dir = Some(other.to_string());
                }
            }
        }
        i += 1;
    }
    let Some(out_dir) = out_dir else {
        eprintln!("usage: devtools sync-plugins -OutDir <dir>");
        return 1;
    };

    let repo = repo_root();
    let src = repo.join("plugins/examples");
    let out = PathBuf::from(&out_dir);
    let dst = out.join("data/plugins");

    if !src.is_dir() {
        eprintln!("[FAIL] bundled plugin source not found: {}", src.display());
        return 1;
    }
    if fs::create_dir_all(&dst).is_err() {
        eprintln!("[FAIL] 无法创建目标目录: {}", dst.display());
        return 1;
    }

    // robocopy 退出码 0-7 为成功（位标志）；>= 8 为失败。
    let src_s = src.to_string_lossy().into_owned();
    let dst_s = dst.to_string_lossy().into_owned();
    let rc = run_robocopy(&[&src_s, &dst_s, "/E", "/NFL", "/NDL", "/NJH", "/NJS"]);
    let rc = match rc {
        Ok(code) => code,
        Err(e) => {
            eprintln!("[FAIL] robocopy {e}");
            return 1;
        }
    };
    if !robocopy_ok(rc) {
        eprintln!("[FAIL] robocopy exit {rc}");
        return 1;
    }

    // --- tombstone：删除用户已删的 bundled 插件 ---
    let config_path = out.join("data/config.json");
    if !config_path.is_file() {
        println!("[ok] sync-plugins done (no config.json, no tombstones)");
        return 0;
    }

    let removed: Vec<String> = match fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        Some(v) => v
            .get("options")
            .and_then(|o| o.get("plugins"))
            .and_then(|p| p.get("removed"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        None => {
            println!("[warn] config.json unreadable, tombstones skipped");
            return 0;
        }
    };

    for id in &removed {
        if id.trim().is_empty() {
            continue;
        }
        let dir = dst.join(id);
        if dir.is_dir() {
            let _ = fs::remove_dir_all(&dir);
            println!("[tombstone] removed bundled plugin: {id}");
        }
    }

    println!("[ok] sync-plugins done");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 缺 -OutDir ⇒ usage + exit 1。
    #[test]
    fn missing_outdir_fails() {
        assert_eq!(run(&[]), 1);
    }

    #[test]
    fn robocopy_convention() {
        assert!(robocopy_ok(0));
        assert!(robocopy_ok(7));
        assert!(!robocopy_ok(8));
    }
}
