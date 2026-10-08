//! AHK 侧「风格一致性 + 静默失败面」护栏 —— 原 `tools/lint_ahk_style.py` 的 **Rust 移植**。
//!
//! 把若干事实变成计数 + `文件:行`，并用 `--write-baseline` 冻结成基线；后续任何改动只要让
//! 某类计数**变差**（高于基线）即 exit 1。判据不是「绝对零」而是「不高于基线」。
//!
//! 检查项：
//! 1. `try_no_catch`   —— `try` 之后既无 `catch` 也无 `finally`。
//! 2. `catch_no_trace` —— `catch` 体内没有任何留痕调用。
//! 3. `log_sinks`      —— `bin/lib` 内 `FileAppend(..., "logs\...")` 触及的不同日志文件名数。
//! 4. `bom_files` / `crlf_files` / `tab_indent_files` —— 文本形态漂移。
//! 5. `spelling_drift` —— 同概念拼写分裂（Cpas/Casp 均为 Caps 的字母错位）。
//!
//! 🔴 移植保留原脚本四个易错形态：① `} catch {` 同行；② `catch as e` 体在下一行；
//! ③ `try {} finally {}` 不算静默；④ `/* */` 块注释里的关键词先剥再判。故按**大括号深度**
//! 求语句范围、catch 关键字按行内任意位置判定、先剥块注释。
//!
//! 退出码（逐字对齐）：0 = 不高于基线；1 = 有计数高于基线；2 = 基础设施/豁免腐烂/扫描退化。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

const ENGINE_ROOT: &str = "bin/lib";
const BASELINE: &str = "tools/lint_ahk_style.baseline.json";
const TEXT_DIR_SCOPES: &[&str] = &["bin/lib", "plugins"];
const TEXT_FILE_SCOPES: &[&str] = &["bin/*.ahk", "tools/*.ahk"];
const TEXT_EXCLUDE: &[&str] = &["bin/KeyFlux.ahk"];
const ENGINE_PREFIX: &str = "bin/lib/";
const VENDORED: &[&str] = &["bin/lib/core/Monitor.ahk"];
const SPELLING_WRONG: &[(&str, &str)] = &[("Cpas", "Caps"), ("Casp", "Caps")];
const TRACE_CALLS: &[&str] = &[
    "EngineLogWarn(",
    "EngineOnError(",
    "LogError(",
    "_log(",
    "_recordError(",
    "QSLogWarn(",
];
const MIN_EXPECTED_FILES: usize = 65;

/// 文档化豁免：(文件, 函数名) → 理由（按「文件 + 所在函数名」，抗行号漂移）。
const EXEMPT_FUNCS: &[((&str, &str), &str)] = &[
    (
        ("bin/lib/core/Functions.ahk", "EngineOnError"),
        "日志设施自身：兜底函数内不能再调日志",
    ),
    (
        ("bin/lib/core/Functions.ahk", "EngineLogWarn"),
        "日志设施自身：递归风险",
    ),
    (
        ("bin/lib/core/Functions.ahk", "KeyFluxExit"),
        "进程退出路径：日志设施可能已不可用",
    ),
    (
        ("bin/lib/core/WindowUtils.ahk", "TryTrayRestoreByNav"),
        "pwsh→powershell 降级；回退仍失败会抛出并由 EngineOnError 统一记录 ⇒ 不重复记",
    ),
];

/// 计数键顺序（与基线对比用；不含 files_scanned）。
const COUNTER_KEYS: &[&str] = &[
    "try_no_catch",
    "catch_no_trace",
    "log_sinks",
    "bom_files",
    "crlf_files",
    "tab_indent_files",
    "spelling_drift_hits",
];

fn re_func_def() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^\s*(?:static\s+)?([A-Za-z_]\w*)\s*\([^)]*\)\s*\{\s*$").expect("FUNC_DEF")
    })
}

fn re_log_sink() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#""logs\\([^"]+)""#).expect("log sink"))
}

fn re_try_head() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^try\b").expect("try head"))
}

fn re_catch_word() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\bcatch\b").expect("catch"))
}

fn re_finally_word() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\bfinally\b").expect("finally"))
}

fn re_catch_start() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^catch\b").expect("catch start"))
}

fn re_finally_start() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^finally\b").expect("finally start"))
}

// ---------------------------------------------------------------- 文本处理

fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

/// 把源行转成「纯代码行」：剥掉 `;` 行尾注释与 `/* ... */` 块注释（块注释跨行，in_block
/// 在行间持续）。串内 `;` / `/*` 不是注释（AHK 转义符是反引号）。
fn build_codes(lines: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut in_block = false;
    for line in lines {
        let chars: Vec<char> = line.chars().collect();
        let mut res: Vec<char> = Vec::new();
        let mut in_str = false;
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            if in_block {
                if ch == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    in_block = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if in_str {
                res.push(ch);
                if ch == '`' {
                    i += 1;
                    if i < chars.len() {
                        res.push(chars[i]);
                        i += 1;
                    }
                    continue;
                }
                if ch == '"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if ch == '"' {
                in_str = true;
                res.push(ch);
                i += 1;
                continue;
            }
            if ch == ';' {
                break;
            }
            if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                in_block = true;
                i += 2;
                continue;
            }
            res.push(ch);
            i += 1;
        }
        out.push(res.into_iter().collect());
    }
    out
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// 从 start 起第一个「strip 后非空」的代码行：(索引, strip 后内容)。
fn next_code_line(codes: &[String], start: usize) -> (Option<usize>, String) {
    for (k, code) in codes.iter().enumerate().skip(start) {
        let body = code.trim();
        if !body.is_empty() {
            return (Some(k), body.to_string());
        }
    }
    (None, String::new())
}

/// 按大括号扫描一行：返回 (扫描后深度, 深度首次归零之后的余文)。
fn scan_depth(seg: &str, mut depth: i64) -> (i64, Option<String>) {
    let chars: Vec<char> = seg.chars().collect();
    let mut rem: Option<String> = None;
    for (idx, ch) in chars.iter().enumerate() {
        if *ch == '{' {
            depth += 1;
        } else if *ch == '}' {
            depth -= 1;
            if depth == 0 && rem.is_none() {
                rem = Some(chars[idx + 1..].iter().collect());
            }
        }
    }
    (depth, rem)
}

/// Python `s[pos:]`（pos = find('{')，未命中时 pos=-1 ⇒ 取最后一个字符）。
fn slice_from_brace(s: &str) -> &str {
    match s.find('{') {
        Some(p) => &s[p..],
        None => match s.char_indices().last() {
            Some((i, _)) => &s[i..],
            None => s,
        },
    }
}

/// 分析 `try` 语句：返回 (结束行索引, 是否有 catch, 是否有 finally)。
fn analyze_try(codes: &[String], start: usize) -> (usize, bool, bool) {
    let seg0 = &codes[start];
    let stripped: String = seg0.trim().chars().skip(3).collect(); // 'try' 之后
    let body = stripped.trim();
    if !body.is_empty() && !body.ends_with('{') {
        if re_catch_word().is_match(seg0) {
            return (start, true, false);
        }
        if re_finally_word().is_match(seg0) {
            return (start, false, true);
        }
        let (_, nxt) = next_code_line(codes, start + 1);
        return (
            start,
            re_catch_start().is_match(&nxt),
            re_finally_start().is_match(&nxt),
        );
    }

    let mut depth = 0i64;
    for (k, code) in codes.iter().enumerate().skip(start) {
        let s: &str = if k == start {
            slice_from_brace(seg0)
        } else {
            code
        };
        let (d, rem) = scan_depth(s, depth);
        depth = d;
        if let Some(rem) = rem {
            if re_catch_word().is_match(&rem) {
                return (k, true, false);
            }
            if re_finally_word().is_match(&rem) {
                return (k, false, true);
            }
            let (_, nxt) = next_code_line(codes, k + 1);
            return (
                k,
                re_catch_start().is_match(&nxt),
                re_finally_start().is_match(&nxt),
            );
        }
    }
    (codes.len().saturating_sub(1), false, false)
}

/// 取 `catch` 语句体（含 catch 行）：返回文本。catch 关键字可在行内任意位置。
fn catch_body(codes: &[String], i: usize) -> String {
    let seg = &codes[i];
    let Some(m) = re_catch_word().find(seg) else {
        return seg.clone();
    };
    let after = seg[m.end()..].trim();
    if after.ends_with('{') {
        // 从 catch 之后第一个 '{' 起按深度求范围
        let pos = seg[m.end()..]
            .find('{')
            .map(|p| p + m.end())
            .unwrap_or_else(|| m.end());
        let mut depth = 0i64;
        for (k, code) in codes.iter().enumerate().skip(i) {
            let s: &str = if k == i { &seg[pos..] } else { code };
            depth += s.matches('{').count() as i64;
            depth -= s.matches('}').count() as i64;
            if depth <= 0 {
                return codes[i..=k].join("\n");
            }
        }
        return codes[i..].join("\n");
    }
    // 无花括号体：收拢后续更深缩进的行
    let base = indent_of(&codes[i]);
    let mut body: Vec<String> = vec![codes[i].clone()];
    let mut k = i + 1;
    while k < codes.len() {
        if codes[k].trim().is_empty() {
            k += 1;
            continue;
        }
        if indent_of(&codes[k]) <= base {
            break;
        }
        body.push(codes[k].clone());
        k += 1;
    }
    body.join("\n")
}

/// {行索引: 函数名}（供豁免判定；按大括号深度求函数范围）。
fn function_spans(codes: &[String]) -> HashMap<usize, String> {
    let mut spans: HashMap<usize, String> = HashMap::new();
    let mut i = 0usize;
    while i < codes.len() {
        if let Some(mf) = re_func_def().captures(&codes[i]) {
            let name = mf[1].to_string();
            let s_from = slice_from_brace(&codes[i]);
            let mut depth = 0i64;
            let mut end = codes.len().saturating_sub(1);
            for (k, code) in codes.iter().enumerate().skip(i) {
                let s: &str = if k == i { s_from } else { code };
                let (d, rem) = scan_depth(s, depth);
                depth = d;
                if rem.is_some() {
                    end = k;
                    break;
                }
            }
            for k in i..=end {
                spans.insert(k, name.clone());
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    spans
}

// ---------------------------------------------------------------- 扫描聚合

#[derive(Default)]
struct Findings {
    try_no_catch: Vec<String>,
    catch_no_trace: Vec<String>,
    try_exempt: Vec<String>,
    bom_files: Vec<String>,
    crlf_files: Vec<String>,
    tab_indent_files: Vec<String>,
    log_sinks: BTreeSet<String>,
    /// 按 SPELLING_WRONG 顺序：wrong → 命中位置列表。
    spelling_drift: Vec<(String, Vec<String>)>,
    files_scanned: usize,
    exempt_hits: BTreeSet<(String, String)>,
}

fn scan_file(path: &Path, rel: &str, f: &mut Findings, is_engine: bool) {
    let raw = read_bytes(path);
    if raw.starts_with(&[0xEF, 0xBB, 0xBF][..]) {
        f.bom_files.push(rel.to_string());
    }
    let body = raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&raw);
    let text = String::from_utf8_lossy(body);
    if text.contains("\r\n") {
        f.crlf_files.push(rel.to_string());
    }
    let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let codes = build_codes(&lines);

    if !VENDORED.contains(&rel)
        && lines
            .iter()
            .any(|ln| !ln.trim().is_empty() && ln.starts_with('\t'))
    {
        f.tab_indent_files.push(rel.to_string());
    }

    if !is_engine {
        return;
    }

    for seg in &codes {
        if seg.contains("FileAppend(")
            && seg.contains("\"logs\\")
            && let Some(c) = re_log_sink().captures(seg)
        {
            f.log_sinks.insert(c[1].to_string());
        }
    }

    let spans = function_spans(&codes);
    for (i, seg_raw) in codes.iter().enumerate() {
        let seg = seg_raw.trim();
        if re_try_head().is_match(seg) {
            let (_, has_catch, has_finally) = analyze_try(&codes, i);
            if !has_catch && !has_finally {
                let fname = spans.get(&i).cloned();
                let key = (rel.to_string(), fname.clone().unwrap_or_default());
                let is_exempt =
                    fname.is_some() && exempt_reason(rel, fname.as_deref().unwrap()).is_some();
                if is_exempt {
                    f.try_exempt
                        .push(format!("{rel}:{} ({})", i + 1, fname.unwrap_or_default()));
                    f.exempt_hits.insert(key);
                } else {
                    f.try_no_catch.push(format!("{rel}:{}", i + 1));
                }
            }
        } else if re_catch_word().is_match(seg_raw) {
            let body = catch_body(&codes, i);
            if !TRACE_CALLS.iter().any(|tc| body.contains(tc)) {
                f.catch_no_trace.push(format!("{rel}:{}", i + 1));
            }
        }
    }
}

fn exempt_reason(rel: &str, func: &str) -> Option<&'static str> {
    EXEMPT_FUNCS
        .iter()
        .find(|((f, fn_name), _)| *f == rel && *fn_name == func)
        .map(|(_, reason)| *reason)
}

fn scan_naming(files: &[(String, PathBuf)], f: &mut Findings) {
    for (wrong, _canon) in SPELLING_WRONG {
        let mut hits = Vec::new();
        for (rel, path) in files {
            let raw = read_bytes(path);
            let body = raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&raw);
            let text = String::from_utf8_lossy(body);
            let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
            for (idx, ln) in build_codes(&lines).iter().enumerate() {
                if ln.contains(*wrong) {
                    hits.push(format!("{rel}:{}", idx + 1));
                }
            }
        }
        f.spelling_drift.push((wrong.to_string(), hits));
    }
}

/// 文本形态 scope 下的全部 .ahk（键 = 仓库相对路径，正斜杠），排序去重。
fn discover(repo: &Path) -> Vec<(String, PathBuf)> {
    fn add(full: &Path, repo: &Path, set: &mut BTreeMap<String, PathBuf>) {
        let Ok(relp) = full.strip_prefix(repo) else {
            return;
        };
        let rel = relp.to_string_lossy().replace('\\', "/");
        if TEXT_EXCLUDE.contains(&rel.as_str()) {
            return;
        }
        set.insert(rel, full.to_path_buf());
    }
    fn walk(dir: &Path, repo: &Path, set: &mut BTreeMap<String, PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, repo, set);
            } else if p.extension().and_then(|s| s.to_str()) == Some("ahk") {
                add(&p, repo, set);
            }
        }
    }

    let mut set: BTreeMap<String, PathBuf> = BTreeMap::new();
    for scope in TEXT_DIR_SCOPES {
        let root = repo.join(scope.replace('/', std::path::MAIN_SEPARATOR_STR));
        walk(&root, repo, &mut set);
    }
    for pattern in TEXT_FILE_SCOPES {
        let dir_part = pattern.trim_end_matches("*.ahk"); // "bin/" 或 "tools/"
        let dir = repo.join(dir_part.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("ahk") {
                    add(&p, repo, &mut set);
                }
            }
        }
    }
    set.into_iter().collect()
}

fn scan(repo: &Path) -> (Vec<(String, PathBuf)>, Findings) {
    let files = discover(repo);
    let mut f = Findings {
        files_scanned: files.len(),
        ..Default::default()
    };
    for (rel, path) in &files {
        scan_file(path, rel, &mut f, rel.starts_with(ENGINE_PREFIX));
    }
    scan_naming(&files, &mut f);
    (files, f)
}

fn to_counters(f: &Findings) -> BTreeMap<String, i64> {
    let mut m = BTreeMap::new();
    m.insert("files_scanned".to_string(), f.files_scanned as i64);
    m.insert("try_no_catch".to_string(), f.try_no_catch.len() as i64);
    m.insert("catch_no_trace".to_string(), f.catch_no_trace.len() as i64);
    m.insert("log_sinks".to_string(), f.log_sinks.len() as i64);
    m.insert("bom_files".to_string(), f.bom_files.len() as i64);
    m.insert("crlf_files".to_string(), f.crlf_files.len() as i64);
    m.insert(
        "tab_indent_files".to_string(),
        f.tab_indent_files.len() as i64,
    );
    m.insert(
        "spelling_drift_hits".to_string(),
        f.spelling_drift.iter().map(|(_, v)| v.len() as i64).sum(),
    );
    m
}

// ---------------------------------------------------------------- Python repr 复刻

fn py_str_list(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", parts.join(", "))
}

fn py_tuple_set(items: &BTreeSet<(String, String)>) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|(a, b)| format!("('{a}', '{b}')"))
        .collect();
    format!("[{}]", parts.join(", "))
}

// ---------------------------------------------------------------- 主流程

/// CLI 入口。`args` 支持 `--repo <p>` / `--verbose` / `--report` / `--write-baseline`。
#[allow(clippy::too_many_lines)]
pub fn run(args: &[String]) -> i32 {
    let mut repo_arg = ".".to_string();
    let mut write_baseline = false;
    let mut report = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    repo_arg = v.clone();
                }
            }
            "--write-baseline" => write_baseline = true,
            "--report" => report = true,
            "--verbose" => {}
            _ => {}
        }
        i += 1;
    }

    let repo = fs::canonicalize(&repo_arg).unwrap_or_else(|_| PathBuf::from(&repo_arg));
    if !repo
        .join(ENGINE_ROOT.replace('/', std::path::MAIN_SEPARATOR_STR))
        .is_dir()
    {
        println!("[FAIL] 找不到 {ENGINE_ROOT}（--repo 指错了？）");
        return 2;
    }

    let (_files, f) = scan(&repo);
    let cur = to_counters(&f);

    // 豁免清单腐烂自检：声明了豁免却一处未命中 ⇒ 函数被改名/删除。
    let declared: BTreeSet<(String, String)> = EXEMPT_FUNCS
        .iter()
        .map(|((f_, fn_), _)| (f_.to_string(), fn_.to_string()))
        .collect();
    let stale: Vec<&(String, String)> = declared.difference(&f.exempt_hits).collect();
    if !stale.is_empty() {
        println!(
            "[FAIL] 豁免清单已腐烂（{} 项未命中任何 finding）—— 请同步 EXEMPT_FUNCS：",
            stale.len()
        );
        for key in stale {
            let reason = exempt_reason(&key.0, &key.1).unwrap_or("");
            println!("    ('{}', '{}') :: {reason}", key.0, key.1);
        }
        return 2;
    }

    if (cur["files_scanned"] as usize) < MIN_EXPECTED_FILES {
        println!(
            "[FAIL] 只扫到 {} 个 .ahk（预期 >= {MIN_EXPECTED_FILES}）—— 扫描器退化了，先修工具再信数字",
            cur["files_scanned"]
        );
        return 2;
    }

    println!("AHK 风格/静默失败扫描：");
    println!(
        "  文本形态 scope       {}",
        TEXT_DIR_SCOPES
            .iter()
            .chain(TEXT_FILE_SCOPES.iter())
            .copied()
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("  静默失败/日志 scope  {ENGINE_ROOT}  (仅引擎核心)");
    println!("  文件数               {:>3}", cur["files_scanned"]);
    println!("  try 无 catch/finally {:>3}", cur["try_no_catch"]);
    println!("  catch 无留痕         {:>3}", cur["catch_no_trace"]);
    println!(
        "  （文档化豁免         {:>3}  {}）",
        f.try_exempt.len(),
        py_tuple_set(&f.exempt_hits)
    );
    let sinks: Vec<String> = f.log_sinks.iter().cloned().collect();
    println!(
        "  日志 sink            {:>3}  {}",
        cur["log_sinks"],
        py_str_list(&sinks)
    );
    println!("  BOM 文件             {:>3}", cur["bom_files"]);
    println!("  CRLF 文件            {:>3}", cur["crlf_files"]);
    println!(
        "  tab 缩进文件         {:>3}  {}",
        cur["tab_indent_files"],
        py_str_list(&f.tab_indent_files)
    );
    let spelling: Vec<String> = f
        .spelling_drift
        .iter()
        .map(|(k, v)| format!("'{k}': {}", v.len()))
        .collect();
    println!(
        "  拼写分裂命中         {:>3}  {{{}}}",
        cur["spelling_drift_hits"],
        spelling.join(", ")
    );

    let bpath = repo.join(BASELINE.replace('/', std::path::MAIN_SEPARATOR_STR));
    if write_baseline {
        let json = serde_json::to_string_pretty(&cur).expect("serialize counters");
        if fs::write(&bpath, format!("{json}\n")).is_err() {
            println!("[FAIL] 无法写入基线: {BASELINE}");
            return 2;
        }
        println!("[ok] 基线已写入 {BASELINE}");
        return 0;
    }

    if report {
        return 0;
    }

    if !bpath.is_file() {
        println!("[FAIL] 基线不存在：{BASELINE} —— 首次请跑 --write-baseline");
        return 2;
    }
    let base: BTreeMap<String, i64> = match fs::read_to_string(&bpath)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(v) => v,
        None => {
            println!("[FAIL] 基线无法解析：{BASELINE}");
            return 2;
        }
    };

    let mut worse: Vec<String> = Vec::new();
    println!("  --- 与基线对比 ---");
    for key in COUNTER_KEYS {
        let b = *base.get(*key).unwrap_or(&0);
        let c = cur[*key];
        let prefix = if c <= b { "  " } else { ">>" };
        println!("  {prefix} {key:<20} 基线 {b:>3} -> 现在 {c:>3}");
        if c > b {
            worse.push(format!("{key}: {b} -> {c}"));
        }
    }

    if !worse.is_empty() {
        println!("[FAIL] 有 {} 类计数**高于基线**（护栏生效）：", worse.len());
        for w in &worse {
            println!("    {w}");
        }
        println!("  若是有意为之，请重新评估并显式更新基线（--write-baseline）。");
        return 1;
    }

    println!("[ok] 所有计数均不高于基线");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 块注释里的 catch/try 字样不算真代码（头注 ④）。
    #[test]
    fn build_codes_strips_block_comments() {
        let lines = vec![
            "/* try { */".to_string(),
            "x := 1 ; catch 注释".to_string(),
            "y := \"a;b\"".to_string(),
        ];
        let codes = build_codes(&lines);
        assert_eq!(codes[0], "");
        assert_eq!(codes[1], "x := 1 ");
        assert_eq!(codes[2], "y := \"a;b\"");
    }

    /// `} catch {` 同行不算静默（头注 ①）；`try {} finally {}` 也不算（头注 ③）。
    #[test]
    fn try_with_same_line_catch_not_silent() {
        let codes = build_codes(&["try { Foo() } catch { Bar() }".to_string()]);
        let (_, has_catch, _) = analyze_try(&codes, 0);
        assert!(has_catch);
    }

    #[test]
    fn try_with_finally_not_silent() {
        let codes = build_codes(&[
            "try {".into(),
            "  Foo()".into(),
            "} finally {".into(),
            "}".into(),
        ]);
        let (_, has_catch, has_finally) = analyze_try(&codes, 0);
        assert!(!has_catch);
        assert!(has_finally);
    }

    /// 裸 try 无 catch/finally ⇒ 静默。
    #[test]
    fn bare_try_is_silent() {
        let codes = build_codes(&["try {".into(), "  Foo()".into(), "}".into()]);
        let (_, has_catch, has_finally) = analyze_try(&codes, 0);
        assert!(!has_catch && !has_finally);
    }

    /// catch 体在下一行（无花括号）也能收到留痕调用（头注 ②）。
    #[test]
    fn catch_body_collects_indented_next_line() {
        let codes = build_codes(&[
            "try { Foo() }".into(),
            "catch as e".into(),
            "    EngineLogWarn(e)".into(),
            "Done()".into(),
        ]);
        let body = catch_body(&codes, 1);
        assert!(body.contains("EngineLogWarn("));
    }

    /// scan_depth：深度归零后返回余文。
    #[test]
    fn scan_depth_returns_remainder() {
        let (d, rem) = scan_depth("{ a }", 0);
        assert_eq!(d, 0);
        assert_eq!(rem.as_deref(), Some(""));
        let (d2, rem2) = scan_depth("} catch {", -1);
        assert_eq!(d2, -1);
        assert!(rem2.is_none());
    }

    #[test]
    fn counters_are_sorted_for_baseline() {
        let f = Findings::default();
        let c = to_counters(&f);
        let keys: Vec<&String> = c.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
