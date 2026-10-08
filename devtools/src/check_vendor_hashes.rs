//! 守卫 vendored 第三方内容免于被悄悄改动 —— 原 `tools/check-vendor-hashes.ps1` 的 Rust 移植。
//!
//! 若干被跟踪的文件是**第三方**（一个 AHK 类、AHK 运行时、一个工具、一个 fork crate）。它们
//! 看起来像我们自己的源码，故"顺手修个上游 typo"很容易且不留痕。本工具把每个目标钉到
//! `tools/vendor-manifest.json` 里记录的 SHA256；任何漂移都令闸门失败，逼迫显式、有记录的更新。
//!
//! * kind = file → 文件字节的 SHA256。
//! * kind = tree → 对每个文件（按 rel 排序、排除 `exclude`）拼 `"<rel>\n<file-sha256>\n"` 再 SHA256。
//!
//! 退出码：0 = 全部匹配；1 = 漂移 / 目标缺失 / manifest 缺失或腐烂。
//!
//! ## 排序口径（移植要点）
//! PowerShell `Sort-Object` 默认**文化感知、大小写不敏感**；Rust 字节序会把 `SoundControl`
//! 排到 `lib/Monitor` 前（大写 S < 小写 l），与已提交 manifest 的键序/树哈希不符。故这里用
//! 「先比小写、再比原串」近似文化感知排序 —— 判据是**能逐字节复现 manifest 里的树哈希**。

use std::fs;
use std::path::{Path, PathBuf};

use crate::util::{repo_root, sha256_file, sha256_hex};

/// 一个 vendored 目标。
struct Target {
    key: &'static str,
    kind: Kind,
    path: &'static str,
    exclude: &'static [&'static str],
}

enum Kind {
    File,
    Tree,
}

/// 「什么算 vendored」的单一真源（增删条目后须跑 `-Write` 重录）。
const TARGETS: &[Target] = &[
    Target {
        key: "file:bin/lib/Monitor.ahk",
        kind: Kind::File,
        path: "bin/lib/Monitor.ahk",
        exclude: &[],
    },
    Target {
        key: "file:bin/AutoHotkey64.exe",
        kind: Kind::File,
        path: "bin/AutoHotkey64.exe",
        exclude: &[],
    },
    Target {
        key: "file:bin/SoundControl.exe",
        kind: Kind::File,
        path: "bin/SoundControl.exe",
        exclude: &[],
    },
    Target {
        key: "file:tools/Rexplorer_x64.exe",
        kind: Kind::File,
        path: "tools/Rexplorer_x64.exe",
        exclude: &[],
    },
    Target {
        key: "tree:config-ui-reactor/vendor/windows-reactor",
        kind: Kind::Tree,
        path: "config-ui-reactor/vendor/windows-reactor",
        // PATCHES.md 是我们的补丁记录、.cargo-ok 是 cargo vendoring 标记；均非上游内容，
        // 不能扰动树哈希。
        exclude: &["PATCHES.md", ".cargo-ok"],
    },
];

/// 近似 PowerShell `Sort-Object`（文化感知、大小写不敏感）：先比小写，再比原串。
fn ps_less(a: &str, b: &str) -> std::cmp::Ordering {
    let la = a.to_lowercase();
    let lb = b.to_lowercase();
    la.cmp(&lb).then_with(|| a.cmp(b))
}

fn join(repo: &Path, rel: &str) -> PathBuf {
    repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
}

/// 递归收集 dir 下所有文件的 rel 路径（正斜杠），排除 exclude，按 ps_less 排序。
fn tree_rel_files(dir: &Path, exclude: &[&str]) -> Vec<String> {
    let mut rels = Vec::new();
    fn walk(dir: &Path, root: &Path, rels: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, rels);
            } else if p.is_file()
                && let Ok(relp) = p.strip_prefix(root)
            {
                rels.push(relp.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(dir, dir, &mut rels);
    rels.retain(|r| !exclude.contains(&r.as_str()));
    rels.sort_by(|a, b| ps_less(a, b));
    rels
}

/// 计算单个目标的哈希（大写十六进制）；目标缺失返回 Err（对齐 PS 的 throw → exit 1）。
fn target_hash(repo: &Path, t: &Target) -> Result<String, String> {
    let full = join(repo, t.path);
    match t.kind {
        Kind::Tree => {
            if !full.is_dir() {
                return Err(format!("missing vendored tree: {}", t.path));
            }
            let mut sb = String::new();
            for rel in tree_rel_files(&full, t.exclude) {
                let file_sha =
                    sha256_file(&full.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))?;
                sb.push_str(&rel);
                sb.push('\n');
                sb.push_str(&file_sha);
                sb.push('\n');
            }
            Ok(sha256_hex(sb.as_bytes()))
        }
        Kind::File => {
            if !full.is_file() {
                return Err(format!("missing vendored file: {}", t.path));
            }
            sha256_file(&full)
        }
    }
}

/// 手工构造与 PowerShell `ConvertTo-Json`（2 空格缩进）逐字一致的 JSON。
fn manifest_json(pairs: &[(String, String)]) -> String {
    let mut s = String::from("{\n");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            s.push_str(",\n");
        }
        s.push_str(&format!("  \"{k}\": \"{v}\""));
    }
    s.push_str("\n}\n");
    s
}

/// CLI 入口。`args` 支持 `-Write` / `--write`（重录 manifest）。
pub fn run(args: &[String]) -> i32 {
    let write = args
        .iter()
        .any(|a| a == "-Write" || a == "--write" || a == "-write");
    let repo = repo_root();
    let manifest_path = repo.join("tools/vendor-manifest.json");

    if write {
        let mut keys: Vec<&Target> = TARGETS.iter().collect();
        keys.sort_by(|a, b| ps_less(a.key, b.key));
        let mut pairs: Vec<(String, String)> = Vec::with_capacity(keys.len());
        for t in keys {
            match target_hash(&repo, t) {
                Ok(h) => pairs.push((t.key.to_string(), h)),
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            }
        }
        let json = manifest_json(&pairs);
        if fs::write(&manifest_path, json).is_err() {
            eprintln!("[FAIL] 无法写入 tools/vendor-manifest.json");
            return 1;
        }
        println!(
            "[ok] wrote {} vendor hash(es) -> tools/vendor-manifest.json",
            pairs.len()
        );
        return 0;
    }

    // ---- verify ----
    if !manifest_path.is_file() {
        println!(
            "[FAIL] missing tools/vendor-manifest.json -- run: devtools check-vendor-hashes -Write"
        );
        return 1;
    }
    let recorded: serde_json::Map<String, serde_json::Value> =
        match fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            Some(serde_json::Value::Object(m)) => m,
            _ => {
                println!("[FAIL] tools/vendor-manifest.json 无法解析");
                return 1;
            }
        };

    let mut failed = 0i32;
    for t in TARGETS {
        let Some(want) = recorded.get(t.key).and_then(|v| v.as_str()) else {
            println!("[FAIL] {} not in manifest -- re-run -Write", t.key);
            failed += 1;
            continue;
        };
        match target_hash(&repo, t) {
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
            Ok(got) => {
                if got != want {
                    println!(
                        "[FAIL] {} DRIFTED (recorded={} now={})",
                        t.key,
                        &want[..want.len().min(12)],
                        &got[..got.len().min(12)]
                    );
                    failed += 1;
                } else {
                    println!("  [ok] {}", t.key);
                }
            }
        }
    }

    // manifest 腐烂：记录里的键已不再被守卫
    let guarded: Vec<&str> = TARGETS.iter().map(|t| t.key).collect();
    let mut stale: Vec<&String> = recorded
        .keys()
        .filter(|k| !guarded.contains(&k.as_str()))
        .collect();
    stale.sort();
    for k in stale {
        println!("[FAIL] stale manifest key '{k}' -- remove it");
        failed += 1;
    }

    if failed > 0 {
        println!("[FAIL] vendor hash guard: {failed} problem(s).");
        println!("       If the change was intended, update vendor/README.md and run -Write.");
        return 1;
    }
    println!("[ok] vendored content unchanged");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_sort_is_case_insensitive_primary() {
        let mut v = vec!["SoundControl.exe", "lib/Monitor.ahk", "AutoHotkey64.exe"];
        v.sort_by(|a, b| ps_less(a, b));
        // 文化感知（大小写不敏感）：a < l < s，而非字节序的 A < S < l
        assert_eq!(
            v,
            vec!["AutoHotkey64.exe", "lib/Monitor.ahk", "SoundControl.exe"]
        );
    }

    #[test]
    fn manifest_json_matches_convertto_json_shape() {
        let pairs = vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ];
        assert_eq!(
            manifest_json(&pairs),
            "{\n  \"a\": \"1\",\n  \"b\": \"2\"\n}\n"
        );
    }

    #[test]
    fn sha256_hex_is_uppercase() {
        // SHA256("") 的标准值，大写
        assert_eq!(
            sha256_hex(b""),
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        );
    }
}
