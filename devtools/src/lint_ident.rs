//! AHK v2 标识符冲突 lint —— 原 `tools/lint_ident.py` 的 **Rust 移植**（零桌面副作用）。
//!
//! ## 为什么存在
//! AHK 标识符**大小写不敏感**。若函数内局部变量与某函数（或内置）同名，则该函数体内
//! `W(...)` 这样的调用会解析成**局部变量**而非函数 ⇒ 运行期错误（"This local variable has
//! not been assigned a value"），而 `/Validate` 看不到它（语法完全合法）。此 bug 曾随
//! probe.ahk rev3 出货：`for i, w in list` 里的 `w` 撞上日志函数 `W`。
//!
//! ## 检查项（对文件内每个函数）
//! * ERROR 局部变量名 == 本文件定义的某函数名，且该名字在同一函数内被调用
//! * WARN  局部变量名 == 本文件定义的某函数名（无调用）
//! * ERROR `global` 变量名 == 本文件定义的某函数名
//! * WARN  局部变量名 == 已知 AHK 内置（Log/Trim/Format/…）
//!
//! 退出码 0 = 干净，1 = 有 ERROR 级发现（逐字对齐原 Python）。
//!
//! ## 移植要点
//! 原脚本的 `ASSIGN` / `CALL` 正则用了**负向后顾** `(?<![.:\w])` / `(?<![.\w])`，而
//! Rust `regex` crate 不支持 look-around。这里用**逐位置手动扫描**精确复刻 Python
//! `re.finditer` 的语义（后顾失败则从 start+1 重试，而非跳到 match 末尾），避免为
//! 一个 lint 引入 `fancy-regex` 新依赖（须过 cargo-deny）。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

/// AHK 关键字（小写）——函数名/调用名命中即排除。
const KEYWORDS: &[&str] = &[
    "if",
    "else",
    "for",
    "while",
    "loop",
    "until",
    "switch",
    "case",
    "default",
    "try",
    "catch",
    "finally",
    "return",
    "break",
    "continue",
    "throw",
    "global",
    "local",
    "static",
    "class",
    "new",
    "not",
    "and",
    "or",
    "is",
    "in",
    "contains",
    "super",
    "this",
    "get",
    "set",
    "__new",
    "__init",
    "__delete",
    "do",
    "goto",
    "gosub",
    "exit",
    "exitapp",
    "var",
    "fileappend",
    "msgbox",
];

/// 既是合理变量名、又常被调用的内置（小写）。精选而非穷举——文件内函数名冲突才是真闸门。
const BUILTINS: &[&str] = &[
    "log",
    "ln",
    "exp",
    "sqrt",
    "abs",
    "ceil",
    "floor",
    "round",
    "mod",
    "min",
    "max",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "format",
    "sort",
    "trim",
    "ltrim",
    "rtrim",
    "substr",
    "instr",
    "strlen",
    "strsplit",
    "integer",
    "float",
    "number",
    "string",
    "ismatch",
    "regexmatch",
    "type",
    "objget",
    "objset",
    "objhas",
    "isobject",
    "isnumber",
    "isalnum",
    "isspace",
    "random",
    "clamp",
    "array",
    "map",
    "buffer",
    "chr",
    "ord",
];

fn keywords() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| KEYWORDS.iter().copied().collect())
}

fn builtins() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| BUILTINS.iter().copied().collect())
}

fn re_func_def() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*\{\s*$").expect("FUNC_DEF")
    })
}

fn re_assign() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*(?::=|\+=|-=|\*=|/=|\.=|\|=|&=|\^=)")
            .expect("ASSIGN")
    })
}

fn re_for_vars() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\bfor\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:,\s*([A-Za-z_][A-Za-z0-9_]*)\s*)?\bin\b")
            .expect("FOR_VARS")
    })
}

fn re_call() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("CALL"))
}

fn re_global_decl() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*global\s+(.+)$").expect("GLOBAL_DECL"))
}

fn re_ident_only() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("IDENT"))
}

/// `\w`（Python str 语义，含 Unicode 字母数字 + 下划线）。
fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// 复刻 `re.finditer` + 负向后顾：从每个位置尝试，后顾失败则 `start+1` 重试。
/// 返回每个命中 match 的捕获组 1（标识符）。`forbidden` = 前导字符判定（true ⇒ 拒绝）。
fn scan_with_lookbehind(re: &Regex, line: &str, forbidden: impl Fn(char) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos <= line.len() {
        let Some(caps) = re.captures(&line[pos..]) else {
            break;
        };
        let m = caps.get(0).expect("group 0");
        let abs_start = pos + m.start();
        let prev = if abs_start == 0 {
            None
        } else {
            line[..abs_start].chars().next_back()
        };
        if let Some(c) = prev
            && forbidden(c)
        {
            pos = abs_start + 1;
            continue;
        }
        if let Some(g) = caps.get(1) {
            out.push(g.as_str().to_string());
        }
        pos += m.end();
    }
    out
}

/// 剥掉 `;` 注释，尊重简单双引号串（逐字对齐 Python strip_comment；不处理 AHK 反引号转义）。
fn strip_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_str = false;
    for ch in line.chars() {
        if ch == '"' {
            in_str = !in_str;
            out.push(ch);
        } else if ch == ';' && !in_str {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}

/// 读文件（utf-8-sig：剥 BOM），返回「去行尾换行的原始行」。
/// 行切分对齐 Python `readlines()` + `rstrip("\n")`：`\r\n`/`\r` 统一为 `\n`，
/// 末尾换行不产生额外空行。
fn load_lines(path: &Path) -> Option<Vec<String>> {
    let bytes = fs::read(path).ok()?;
    let body = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    let text = String::from_utf8_lossy(body);
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();
    if lines.last().map(String::as_str) == Some("") {
        lines.pop();
    }
    Some(lines)
}

/// 函数保序表：(name, start, end_exclusive)。
type FuncSpans = Vec<(String, usize, usize)>;

/// 大括号匹配求函数范围：{name: (start, end_exclusive)}（保序）。
///
/// 复刻 Python `funcs` **字典**语义：同名函数折叠——保**首次出现**的顺序位置，
/// span 取**最后一次**（后写覆盖前值但不改插入位）。故返回去重后的保序表。
fn find_functions(clean: &[String]) -> FuncSpans {
    let mut funcs: Vec<(String, usize, usize)> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    let mut i = 0usize;
    while i < clean.len() {
        if let Some(caps) = re_func_def().captures(&clean[i]) {
            let name = caps[1].to_string();
            if !keywords().contains(name.to_lowercase().as_str()) {
                let mut depth =
                    clean[i].matches('{').count() as i64 - clean[i].matches('}').count() as i64;
                let mut j = i + 1;
                while j < clean.len() && depth > 0 {
                    depth += clean[j].matches('{').count() as i64;
                    depth -= clean[j].matches('}').count() as i64;
                    j += 1;
                }
                match index_of.get(&name) {
                    Some(&slot) => {
                        funcs[slot].1 = i;
                        funcs[slot].2 = j;
                    }
                    None => {
                        index_of.insert(name.clone(), funcs.len());
                        funcs.push((name, i, j));
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    funcs
}

/// 一条发现（含定位所需的 raw / s / e）。
struct Finding {
    sev: &'static str,
    func: String,
    var: String,
    msg: String,
    raw: Vec<String>,
    s: usize,
    e: usize,
}

/// 分析单文件：返回 (函数保序表, 发现列表)。
#[expect(
    clippy::too_many_lines,
    reason = "单文件分析：参数/局部/全局/调用收集 + 四类 finding 判定，拆分会割裂上下文"
)]
fn analyse(path: &Path) -> Option<(FuncSpans, Vec<Finding>)> {
    let raw = load_lines(path)?;
    let clean: Vec<String> = raw.iter().map(|l| strip_comment(l)).collect();
    let funcs = find_functions(&clean);
    let func_names_lower: HashMap<String, String> = funcs
        .iter()
        .map(|(n, _, _)| (n.to_lowercase(), n.clone()))
        .collect();
    let mut findings: Vec<Finding> = Vec::new();

    for (name, s, e) in &funcs {
        let body = &clean[s + 1..*e];
        // 参数名：group(2) split(',') → strip → split(":=")[0] → strip → lstrip("*") → strip
        let params_raw = re_func_def()
            .captures(&clean[*s])
            .map(|c| c[2].to_string())
            .unwrap_or_default();
        let params: Vec<String> = params_raw
            .split(',')
            .map(|p| {
                p.trim()
                    .split(":=")
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches('*')
                    .trim()
                    .to_string()
            })
            .filter(|p| !p.is_empty())
            .collect();

        // 保序「首见优先」字典：BTreeMap 天然按 lower 键排序（= Python sorted(items)）。
        let mut local_names: BTreeMap<String, String> = BTreeMap::new();
        let mut global_names: BTreeMap<String, String> = BTreeMap::new();
        for p in &params {
            local_names
                .entry(p.to_lowercase())
                .or_insert_with(|| p.clone());
        }
        for line in body {
            for m in scan_with_lookbehind(re_assign(), line, |c| c == '.' || c == ':' || is_word(c))
            {
                local_names.entry(m.to_lowercase()).or_insert(m.clone());
            }
            if let Some(caps) = re_for_vars().captures(line) {
                for g in [caps.get(1), caps.get(2)].into_iter().flatten() {
                    let v = g.as_str().to_string();
                    if !v.is_empty() {
                        local_names.entry(v.to_lowercase()).or_insert(v);
                    }
                }
            }
            if let Some(gm) = re_global_decl().captures(line) {
                for g in gm[1].split(',') {
                    let g = g.trim().trim_end_matches(';').trim();
                    if re_ident_only().is_match(g) {
                        global_names
                            .entry(g.to_lowercase())
                            .or_insert_with(|| g.to_string());
                    }
                }
            }
        }

        let mut calls: HashSet<String> = HashSet::new();
        for line in body {
            for m in scan_with_lookbehind(re_call(), line, |c| c == '.' || is_word(c)) {
                if !keywords().contains(m.to_lowercase().as_str()) {
                    calls.insert(m.to_lowercase());
                }
            }
        }

        for (low, orig) in &local_names {
            if let Some(target) = func_names_lower.get(low) {
                let called = calls.contains(low);
                let msg = if called {
                    format!(
                        "local variable shadows the function '{target}' AND '{orig}(' is called \
here -> the call resolves to the variable; runtime failure"
                    )
                } else {
                    format!("local variable shadows the function '{target}' (latent hazard)")
                };
                findings.push(Finding {
                    sev: if called { "ERROR" } else { "WARN" },
                    func: name.clone(),
                    var: orig.clone(),
                    msg,
                    raw: raw.clone(),
                    s: *s,
                    e: *e,
                });
            } else if builtins().contains(low.as_str()) {
                findings.push(Finding {
                    sev: "WARN",
                    func: name.clone(),
                    var: orig.clone(),
                    msg: format!("local variable shadows the built-in '{orig}'"),
                    raw: raw.clone(),
                    s: *s,
                    e: *e,
                });
            }
        }

        for (low, orig) in &global_names {
            if let Some(target) = func_names_lower.get(low) {
                findings.push(Finding {
                    sev: "ERROR",
                    func: name.clone(),
                    var: orig.clone(),
                    msg: format!("global variable shadows the function '{target}'"),
                    raw: raw.clone(),
                    s: *s,
                    e: *e,
                });
            }
        }
    }

    Some((funcs, findings))
}

/// Python `sorted(funcs.keys())` 的 list repr：`['A', 'b']`。
fn py_list_repr(names: &[String]) -> String {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    let items: Vec<String> = sorted.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", items.join(", "))
}

/// 定位发现所在行（仅用于人读注释，不影响退出码）。
fn locate_where(f: &Finding) -> Option<String> {
    let var_re = format!(r"({})\s*(?::=|\+=|\.=)", regex::escape(&f.var));
    let assign = Regex::new(&var_re).ok()?;
    let for_re = Regex::new(&format!(r"\bfor\b[^\n]*\b{}\b", regex::escape(&f.var))).ok()?;
    for idx in (f.s + 1)..f.e {
        let Some(line) = f.raw.get(idx) else { continue };
        let hit_assign =
            !scan_with_lookbehind(&assign, line, |c| c == '.' || is_word(c)).is_empty();
        if hit_assign || for_re.is_match(line) {
            let trimmed: String = line.trim().chars().take(110).collect();
            return Some(format!("line {}: {}", idx + 1, trimmed));
        }
    }
    None
}

/// CLI 入口：对每个路径 lint 并打印报告，返回退出码（有 ERROR ⇒ 1）。
pub fn run(paths: &[String]) -> i32 {
    let default_files = [
        "probe.ahk",
        "target_open.ahk",
        "target_folder.ahk",
        "target_msgbox.ahk",
        "target_save.ahk",
    ];
    let targets: Vec<String> = if paths.is_empty() {
        default_files.iter().map(|s| s.to_string()).collect()
    } else {
        paths.to_vec()
    };

    let mut total = 0usize;
    let mut any_error = false;
    for path in &targets {
        let p = Path::new(path);
        let Some((funcs, findings)) = analyse(p) else {
            eprintln!("[FAIL] 无法读取文件: {path}");
            continue;
        };
        println!("{}", "=".repeat(78));
        let names: Vec<String> = funcs.iter().map(|(n, _, _)| n.clone()).collect();
        println!(
            "FILE {path}  functions={} {}",
            funcs.len(),
            py_list_repr(&names)
        );
        if findings.is_empty() {
            println!("  CLEAN - no identifier collisions");
        }
        for f in &findings {
            total += 1;
            if f.sev == "ERROR" {
                any_error = true;
            }
            println!("  [{}] {}() var='{}' -> {}", f.sev, f.func, f.var, f.msg);
            if let Some(location) = locate_where(f) {
                println!("       {location}");
            }
            if f.sev == "ERROR" {
                let base = f.var.trim_end_matches(|c: char| c.is_ascii_digit());
                let base = if base.is_empty() { &f.var } else { base };
                println!(
                    "       FIX: rename the variable (e.g. '{}' -> '{}X') so it no longer \
matches the function name",
                    f.var, base
                );
            }
        }
    }
    println!("{}", "=".repeat(78));
    println!("TOTAL_FINDINGS={total}");
    if any_error { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kf-lint-ident-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write(dir: &Path, name: &str, body: &str) -> String {
        let p = dir.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p.to_string_lossy().to_string()
    }

    /// strip_comment：串内 `;` 不算注释，串外 `;` 起截断。
    #[test]
    fn strip_comment_honors_strings() {
        assert_eq!(strip_comment("a := 1 ; note"), "a := 1 ");
        assert_eq!(strip_comment(r#"x := "a;b" ; c"#), r#"x := "a;b" "#);
        assert_eq!(strip_comment("no comment"), "no comment");
    }

    /// 局部变量与同文件函数同名 + 该名字被调用 ⇒ ERROR（rev3 的真实 bug 形态）。
    #[test]
    fn local_shadowing_called_function_is_error() {
        let dir = unique_dir("err");
        let src = "W(msg) {\n    return msg\n}\nLaunch() {\n    for i, w in list\n    w(1)\n}\n";
        let path = write(&dir, "probe.ahk", src);
        let code = run(&[path]);
        assert_eq!(code, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 干净文件 ⇒ exit 0。
    #[test]
    fn clean_file_exits_zero() {
        let dir = unique_dir("clean");
        let src = "Foo(a, b) {\n    c := a + b\n    return c\n}\n";
        let path = write(&dir, "ok.ahk", src);
        assert_eq!(run(&[path]), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 局部变量撞内置名（Log）⇒ WARN，但无 ERROR ⇒ exit 0。
    #[test]
    fn builtin_shadow_is_warn_only() {
        let dir = unique_dir("warn");
        let src = "Foo() {\n    log := 1\n    return log\n}\n";
        let path = write(&dir, "w.ahk", src);
        assert_eq!(run(&[path]), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 后顾：`.prop :=` / `obj.field :=` 的成员赋值不算局部变量声明。
    #[test]
    fn member_assignment_not_local() {
        let line = "this.log := 1";
        let hits = scan_with_lookbehind(re_assign(), line, |c| c == '.' || c == ':' || is_word(c));
        assert!(
            hits.is_empty(),
            "点号后的标识符不应被当作局部变量: {hits:?}"
        );
    }

    /// load_lines：CRLF 归一 + 末尾换行不产生额外空行 + 剥 BOM。
    #[test]
    fn load_lines_matches_python_readlines() {
        let dir = unique_dir("lines");
        let p = dir.join("crlf.ahk");
        fs::write(&p, "\u{feff}a := 1\r\nb := 2\r\n").unwrap();
        let lines = load_lines(&p).unwrap();
        assert_eq!(lines, vec!["a := 1".to_string(), "b := 2".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }
}
