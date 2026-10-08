//! 文本特征双端一致性对账 —— 原 `tools/texttype_conformance.py` 的 **Rust 移植**。
//!
//! 内置文本特征 (url/path/magnet/bilibili/plain) 的**取值与顺序**由两处独立声明承载：
//! * 面板镜像：`services/selected_action.rs` 的 `TEXT_TYPES`；
//! * 运行时：AHK `bin/lib/rules/SelectedAction.ahk` + 子模块 `SelectedAction/*.ahk`
//!   的 `TextFeatureSpecs` 注册表与 `MatchTextType` 命中逻辑。
//!
//! 两处靠共享向量 `testdata/text_types.json` 钉死。本工具把契约变成可执行断言，两层校验：
//! 1. **静态对账**：解析两侧注册表，逐项比对 value 与顺序，并断言与向量 `types` 完全一致；
//!    兜底特征唯一且居末。
//! 2. **运行时对账**：从 AHK 源**逐字提取**函数体（杜绝探针与产品代码漂移），按向量用例
//!    逐 (用例 × 特征) 求值，比对 expectTypes 全集。
//!
//! 退出码（逐字对齐原脚本）：0 = 全过；1 = 语义/注册表失配；2 = 基础设施错误
//! （文件缺失 / 解释器不可用 / 探针超时或异常）。

use std::collections::HashSet;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;

const RUST_SRC: &str = "config-ui-reactor/src/services/selected_action.rs";
const AHK_SRC: &str = "bin/lib/rules/SelectedAction.ahk";
const AHK_SUBDIR: &str = "bin/lib/rules/SelectedAction";
const VECTOR: &str = "testdata/text_types.json";

fn re_ahk_row() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r#"^[ \t]*\{[ \t]*value:[ \t]*"([a-z][a-z0-9_]*)"[ \t]*,[ \t]*named:[ \t]*(true|false)[ \t]*,[ \t]*ignoreCase:[ \t]*(true|false)[ \t]*,[ \t]*pattern:[ \t]*"([^"]*)"[ \t]*\}[ \t]*,[ \t]*$"#,
        )
        .expect("AHK_ROW")
    })
}

fn re_text_types_block() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?s)TEXT_TYPES:\s*\[\(&str,\s*&str\);\s*\d+\]\s*=\s*\[(.*?)\];")
            .expect("TEXT_TYPES block")
    })
}

fn re_rust_row() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"\("([a-z][a-z0-9_]*)",\s*"[^"]*"\)"#).expect("rust row"))
}

/// 读文本（utf-8-sig：剥 BOM）。
fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let body = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    Some(String::from_utf8_lossy(body).into_owned())
}

/// AHK 侧契约源文件清单 = 门面 + 子目录内全部 .ahk（按名排序）。
fn ahk_source_files(repo: &Path) -> Vec<PathBuf> {
    let mut out = vec![repo.join(AHK_SRC.replace('/', std::path::MAIN_SEPARATOR_STR))];
    let sub = repo.join(AHK_SUBDIR.replace('/', std::path::MAIN_SEPARATOR_STR));
    if sub.is_dir()
        && let Ok(entries) = fs::read_dir(&sub)
    {
        let mut names: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ahk"))
            .collect();
        names.sort();
        out.extend(names);
    }
    out
}

/// 把门面 + 子模块拼成一份虚拟源（抽取按函数名定位，故顺序无关）。
fn read_ahk_virtual_source(repo: &Path) -> String {
    ahk_source_files(repo)
        .iter()
        .filter_map(|p| read_text(p))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 解析 selected_action.rs 的 TEXT_TYPES → value 有序列表。
fn parse_rust_registry(repo: &Path) -> Option<Vec<String>> {
    let src = read_text(&repo.join(RUST_SRC.replace('/', std::path::MAIN_SEPARATOR_STR)))?;
    let caps = re_text_types_block().captures(&src)?;
    let block = &caps[1];
    Some(
        re_rust_row()
            .captures_iter(block)
            .map(|c| c[1].to_string())
            .collect(),
    )
}

struct AhkRow {
    value: String,
    named: bool,
}

/// 逐字提取 `name(...) {` 到顶格 `}` 的函数体（含首尾行）。未找到返回 None。
fn extract_ahk_func(src: &str, name: &str) -> Option<String> {
    let pattern = format!(r"(?ms)^{}\(.*?\)[ \t]*\{{.*?^\}}", regex::escape(name));
    let re = Regex::new(&pattern).ok()?;
    re.find(src).map(|m| m.as_str().to_string())
}

fn parse_ahk_registry(repo: &Path) -> Option<Vec<AhkRow>> {
    let src = read_ahk_virtual_source(repo);
    let body = extract_ahk_func(&src, "TextFeatureSpecs")?;
    let mut rows = Vec::new();
    for line in body.lines() {
        if let Some(c) = re_ahk_row().captures(line) {
            rows.push(AhkRow {
                value: c[1].to_string(),
                named: &c[2] == "true",
            });
        }
    }
    Some(rows)
}

/// 把字符串转成 AHK 双引号字面量内容（AHK 只认反引号转义，不认反斜杠）。
fn ahk_escape(literal: &str) -> String {
    let mut out = String::with_capacity(literal.len());
    for ch in literal.chars() {
        match ch {
            '`' => out.push_str("``"),
            '"' => out.push_str("`\""),
            '\n' => out.push_str("`n"),
            '\r' => out.push_str("`r"),
            '\t' => out.push_str("`t"),
            _ => out.push(ch),
        }
    }
    out
}

/// 生成探针 AHK（函数体从源逐字提取，非手抄）；行尾 CRLF（对齐原脚本 newline="\r\n"）。
fn build_probe(
    src: &str,
    types: &[String],
    cases: &[serde_json::Value],
    result_path: &Path,
) -> String {
    let mut funcs: Vec<String> = Vec::new();
    for name in ["TextFeatureSpecs", "TextFeatureHit", "MatchTextType"] {
        match extract_ahk_func(src, name) {
            Some(body) => funcs.push(body),
            None if name == "TextFeatureHit" => continue, // 兼容尚未引入该辅助函数的旧版实现
            None => panic!("AHK 源中未找到函数 {name}"),
        }
    }

    let rp = result_path.to_string_lossy().replace('\\', "/");
    let mut probe: Vec<String> = vec![
        "#Requires AutoHotkey v2.0".into(),
        "; !!! 本文件由 devtools texttype-conformance 生成, 请勿手工编辑 !!!".into(),
        "; 下列函数体从 bin/lib/rules/SelectedAction.ahk + SelectedAction/*.ahk **逐字提取** (非手抄),".into(),
        "; 目的 = AHK PCRE2 命中行为与共享向量 (testdata/text_types.json) 逐条对齐。".into(),
        String::new(),
        funcs.join("\n"),
        String::new(),
        format!("p := \"{rp}\""),
        "buf := \"\"".into(),
        "try {".into(),
    ];
    for (ci, case) in cases.iter().enumerate() {
        let content = ahk_escape(case["content"].as_str().unwrap_or(""));
        for (ti, t) in types.iter().enumerate() {
            probe.push(format!(
                "\tbuf .= \"{ci}.{ti}=\" . (MatchTextType(\"{t}\", \"{content}\") ? \"1\" : \"0\") . \"`n\""
            ));
        }
    }
    probe.extend([
        "} catch as e {".to_string(),
        "\tbuf .= \"ERR:\" . e.Message . \"`n\"".into(),
        "}".into(),
        "try FileDelete(p)".into(),
        "try FileAppend(buf, p, \"UTF-8\")".into(),
        "ExitApp(0)".into(),
        String::new(),
    ]);
    probe.join("\r\n")
}

/// 拉起 AHK 解释器跑探针（120s 超时；stderr 单独 drain 防管道死锁）。
/// 返回 (exit_code, stderr_text)；超时返回 None。
fn run_probe(ahk_bin: &Path, probe_path: &Path) -> Option<(i32, String)> {
    let mut child = Command::new(ahk_bin)
        .arg("/ErrorStdOut")
        .arg(probe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stderr = child.stderr.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = reader.join().unwrap_or_default();
                return Some((status.code().unwrap_or(-1), text));
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(120) {
                    let _ = child.kill();
                    let _ = reader.join();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kf_textfeat_{}_{tag}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// CLI 入口。`args` 支持 `--repo <p>` / `--ahk <p>` / `--verbose` / `--static-only`。
#[allow(clippy::too_many_lines)]
pub fn run(args: &[String]) -> i32 {
    let mut repo_arg = ".".to_string();
    let mut ahk_arg = "bin/AutoHotkey64.exe".to_string();
    let mut static_only = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    repo_arg = v.clone();
                }
            }
            "--ahk" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    ahk_arg = v.clone();
                }
            }
            "--static-only" => static_only = true,
            "--verbose" => {} // 原脚本 --verbose 仅影响逐条打印；本移植保持简洁输出
            _ => {}
        }
        i += 1;
    }

    let repo = fs::canonicalize(&repo_arg).unwrap_or_else(|_| PathBuf::from(&repo_arg));
    for rel in [RUST_SRC, VECTOR] {
        let p = repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !p.is_file() {
            println!("[FAIL] 缺少文件: {rel}");
            return 2;
        }
    }
    for src_path in ahk_source_files(&repo) {
        if !src_path.is_file() {
            let rel = src_path
                .strip_prefix(&repo)
                .unwrap_or(&src_path)
                .to_string_lossy()
                .replace('\\', "/");
            println!("[FAIL] 缺少文件: {rel}");
            return 2;
        }
    }

    let doc: serde_json::Value =
        match read_text(&repo.join(VECTOR.replace('/', std::path::MAIN_SEPARATOR_STR)))
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            Some(v) => v,
            None => {
                println!("[FAIL] 向量文件无法解析: {VECTOR}");
                return 2;
            }
        };
    let types: Vec<String> = doc["types"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let cases = doc["cases"].as_array().cloned().unwrap_or_default();

    let rust_values = parse_rust_registry(&repo);
    let ahk_rows = parse_ahk_registry(&repo);
    let mut failures: Vec<String> = Vec::new();

    // ---- 1. 注册表静态对账 ----
    match &rust_values {
        None => failures.push(format!("未能从 {RUST_SRC} 解析出任何注册表项")),
        Some(vals) if *vals != types => failures.push(format!(
            "Rust 镜像取值/顺序 {:?} ≠ 向量 types {:?}",
            vals, types
        )),
        Some(_) => {}
    }
    match &ahk_rows {
        None => failures.push("AHK 源中未找到 TextFeatureSpecs() 注册表".to_string()),
        Some(rows) => {
            let ahk_values: Vec<String> = rows.iter().map(|r| r.value.clone()).collect();
            if ahk_values != types {
                failures.push(format!(
                    "AHK 注册表取值/顺序 {:?} ≠ 向量 types {:?}",
                    ahk_values, types
                ));
            }
        }
    }
    // 结构性不变量：兜底项唯一且居末
    if let Some(rows) = &ahk_rows {
        let fallbacks: Vec<String> = rows
            .iter()
            .filter(|r| !r.named)
            .map(|r| r.value.clone())
            .collect();
        let expected = types.last().cloned().unwrap_or_default();
        if fallbacks != vec![expected] {
            failures.push(format!(
                "兜底特征必须唯一且居末, 实际 = {:?} (types 末位 {:?})",
                fallbacks,
                types.last()
            ));
        }
    }

    if !failures.is_empty() {
        for f in &failures {
            println!("[FAIL] {f}");
        }
        return 1;
    }
    println!(
        "[OK] 注册表静态对账: {} 项, 顺序 {}",
        types.len(),
        types.join(" / ")
    );

    if static_only {
        return 0;
    }

    // ---- 2. AHK 运行时对账 ----
    let ahk_bin = {
        let p = PathBuf::from(&ahk_arg);
        if p.is_absolute() { p } else { repo.join(p) }
    };
    if !ahk_bin.is_file() {
        println!("[FAIL] 找不到 AHK 解释器: {}", ahk_bin.display());
        return 2;
    }

    let work = temp_dir("run");
    let code = (|| -> i32 {
        let probe_path = work.join("texttype_probe.ahk");
        let result_path = work.join("texttype_result.tsv");
        let src = read_ahk_virtual_source(&repo);
        let probe = build_probe(&src, &types, &cases, &result_path);
        if fs::write(&probe_path, probe).is_err() {
            println!("[FAIL] 无法写入探针文件");
            return 2;
        }

        let Some((exit, stderr)) = run_probe(&ahk_bin, &probe_path) else {
            println!("[FAIL] AHK 探针超时 (120s): 疑似运行时错误对话框阻塞");
            return 2;
        };
        if exit != 0 {
            let err = stderr.trim();
            let shown: String = err.chars().take(400).collect();
            println!("[FAIL] AHK 探针退出码 {exit}: {shown}");
            return 2;
        }
        if !result_path.is_file() {
            println!("[FAIL] AHK 探针未产出结果文件 (运行时异常?)");
            return 2;
        }

        let mut bits: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
        let result_text = read_text(&result_path).unwrap_or_default();
        for line in result_text.lines() {
            if line.starts_with("ERR:") {
                println!("[FAIL] AHK 运行时错误: {line}");
                return 2;
            }
            if let Some((k, v)) = line.split_once('=') {
                bits.insert(k.trim().to_string(), v.trim() == "1");
            }
        }

        let mut diffs: Vec<String> = Vec::new();
        for (ci, case) in cases.iter().enumerate() {
            let expected: HashSet<String> = case["expectTypes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let mut got: HashSet<String> = HashSet::new();
            let mut missing = false;
            for (ti, t) in types.iter().enumerate() {
                let key = format!("{ci}.{ti}");
                match bits.get(&key) {
                    None => {
                        diffs.push(format!("用例#{} 缺结果 ({key})", ci + 1));
                        missing = true;
                    }
                    Some(true) => {
                        got.insert(t.clone());
                    }
                    Some(false) => {}
                }
            }
            if !missing && got != expected {
                let mut e: Vec<String> = expected.into_iter().collect();
                e.sort();
                let mut g: Vec<String> = got.into_iter().collect();
                g.sort();
                let e = if e.is_empty() {
                    "<无>".to_string()
                } else {
                    e.join(", ")
                };
                let g = if g.is_empty() {
                    "<无>".to_string()
                } else {
                    g.join(", ")
                };
                let content = case["content"].as_str().unwrap_or("");
                let note = case["note"].as_str().unwrap_or("");
                diffs.push(format!(
                    "用例#{} content={content:?}\n    期望命中 [{e}]\n    AHK 命中 [{g}]\n    note={note}",
                    ci + 1
                ));
            }
        }

        if !diffs.is_empty() {
            for d in diffs.iter().take(20) {
                println!("[FAIL] AHK↔向量失配: {d}");
            }
            println!("[FAIL] 共 {} 条用例失配", diffs.len());
            return 1;
        }
        println!(
            "[OK] AHK 运行时对账: {} 用例 × {} 特征 = {} 次求值全部一致",
            cases.len(),
            types.len(),
            cases.len() * types.len()
        );
        0
    })();
    let _ = fs::remove_dir_all(&work);
    code
}
