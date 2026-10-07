//! AHK 文本层原语 —— Go `model/methods.go` + `generators/generators.go` 的同名移植。
//!
//! 这些函数跨越 Go/Rust 两端后必须**逐字节相同**：转义顺序、浮点格式、trim 语义的任一
//! 差异都会让 `KeyFlux.ahk` 字节不等。故除 `substr` / `contains_only_modifier`
//! （Go 侧未导出，取不到参考值）外，全部由 `tests/fixtures/text_primitives.json`
//! 对账 —— 该夹具 = **Go 冻结契约快照**（Go 后端 2026-10-06 退役, 36ccb83；不再有再生成入口,
//! 溯源: `git show 36ccb83^:config-server/internal/script/`）。
//!
//! ⚠️ 已知的跨语言陷阱（对账夹具已钉住）：
//! * `divide` 除零：Go `%.3f` 打印 **`+Inf`**，Rust `{:.3}` 打印 `inf` ⇒ 必须特判。
//! * `not_blank_lines("")`：Go 的 nil slice 序列化为 **`null`**（不是 `[]`）。
//! * `ahk_string` 的三步替换**有顺序**：反引号 → 双引号 → `" ;"`（空格后分号才是 AHK 注释）。

/// Go `model.AhkString`：转成 AHK 字符串字面量（转义反引号 / 双引号 / 空格后的分号）。
///
/// 替换顺序不可调换：Go 是先全局替换反引号，再替换双引号，最后处理 `" ;"`。
pub fn ahk_string(s: &str) -> String {
    let escaped = s.replace('`', "``").replace('"', "`\"");
    // 空格后的分号会被 AHK 解释为注释起始 ⇒ 转义为 `;
    let escaped = escaped.replace(" ;", " `;");
    format!("\"{escaped}\"")
}

/// Go `model.ToAHKFuncArg`：`ahk-expression:` 前缀视为**表达式直出**（去前缀 + 两端 trim），
/// 否则按字符串字面量处理。
pub fn to_ahk_func_arg(val: &str) -> String {
    const PREFIX: &str = "ahk-expression:";
    match val.strip_prefix(PREFIX) {
        Some(rest) => rest.trim().to_string(),
        None => ahk_string(val),
    }
}

/// Go `model.NotBlankLines`：按 `\n` 切分，**每行 trim**，丢弃空行。
pub fn not_blank_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Go `generators.divide`（模板函数 `divide`）。
///
/// 语义：`a/b`，结果 **≤ 0 返回空串**；否则格式化为 3 位小数。
/// ⚠️ 除零时 Go 得到 `+Inf` 并打印 `+Inf`（Rust 原生会打印 `inf`）⇒ 显式对齐。
pub fn divide(a: i64, b: i64) -> String {
    let result = a as f64 / b as f64;
    if result <= 0.0 {
        return String::new();
    }
    if result.is_nan() {
        // Go: NaN <= 0 为 false ⇒ 落到 Sprintf，输出 "NaN"
        return "NaN".to_string();
    }
    if result.is_infinite() {
        return "+Inf".to_string();
    }
    format!("{result:.3}")
}

/// Go `generators.concat`（模板函数 `concat`）。
pub fn concat(a: &str, b: &str) -> String {
    format!("{a}{b}")
}

/// Go `generators.escapeAhkHotkey`（模板函数 `escapeAhkHotkey`）：仅分号需要转义。
pub fn escape_ahk_hotkey(key: &str) -> String {
    if key == ";" {
        "`;".to_string()
    } else {
        key.to_string()
    }
}

/// Go `generators.join`（模板函数 `join`）。Go 版对非字符串元素会 panic，此处由类型保证。
pub fn join(separator: &str, elements: &[String]) -> String {
    elements.join(separator)
}

/// Go `generators.substr`：**按 rune（Unicode 码点）**切片，非字节。
///
/// `length` 为负时表示"从末尾回退"（`("abcdef", 1, -1)` ⇒ `"bcde"`）。
/// 移植口径与 Go 一致：`start >= len` ⇒ 空串；`start + length` 越界则截到末尾。
///
/// ⚠️ Go 版在 `start < 0` 时会 panic（直接切 rune 切片），此处同样不设防 —— 调用点
/// 只有 `sortHotkeys` 一处且恒传 `(1, -1)`。
pub fn substr(input: &str, start: isize, length: isize) -> String {
    let runes: Vec<char> = input.chars().collect();
    let total = runes.len() as isize;

    if start >= total {
        return String::new();
    }

    let mut len = length;
    if start + len > total {
        len = total - start;
    } else if len < 0 {
        len += total - start;
    }

    runes[start as usize..(start + len) as usize]
        .iter()
        .collect()
}

/// Go `generators.containsOnlyModifier`：串非空且**去掉修饰键字符后为空**。
///
/// （用于 `renderKeymap` 判断 hotkey 是否只是修饰键组合，是则热键名改用 `customHotkeys`。）
pub fn contains_only_modifier(hotkey: &str) -> bool {
    let trimmed = hotkey.trim();
    !trimmed.is_empty() && trimmed.trim_matches(|c| "#!^+<>*~$".contains(c)).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::HashMap;

    /// Go 侧导出的对账夹具（见模块头注的再生成命令）。
    #[derive(Deserialize)]
    struct StrCase {
        #[serde(rename = "in")]
        input: String,
        out: String,
    }

    #[derive(Deserialize)]
    struct ConcatCase {
        a: String,
        b: String,
        out: String,
    }

    #[derive(Deserialize)]
    struct DivideCase {
        a: i64,
        b: i64,
        out: String,
    }

    #[derive(Deserialize)]
    struct JoinCase {
        sep: String,
        elems: Vec<String>,
        out: String,
    }

    #[derive(Deserialize)]
    struct LinesCase {
        #[serde(rename = "in")]
        input: String,
        /// Go 的 nil slice 会序列化成 `null`（不是 `[]`）。
        out: Option<Vec<String>>,
    }

    type Fixture = HashMap<String, serde_json::Value>;

    fn fixture() -> Fixture {
        let path = "tests/fixtures/text_primitives.json";
        let raw = std::fs::read_to_string(path).unwrap_or_else(|error| {
            panic!(
                "读取对账夹具 {path} 失败: {error}\n\
                 夹具 = Go 冻结契约快照 (仓库自带文件, 缺失即仓库不完整); \
                 历史实现在 git 36ccb83^:config-server/internal/script/"
            )
        });
        serde_json::from_str(&raw).expect("夹具 JSON 解析失败")
    }

    fn cases<T: for<'de> Deserialize<'de>>(fixture: &Fixture, key: &str) -> Vec<T> {
        serde_json::from_value(
            fixture
                .get(key)
                .unwrap_or_else(|| panic!("夹具缺少 {key}"))
                .clone(),
        )
        .unwrap_or_else(|error| panic!("夹具 {key} 反序列化失败: {error}"))
    }

    /// 与 Go 实现逐值对账（转义顺序 + 浮点格式 + trim 语义）。
    #[test]
    fn matches_go_reference_fixture() {
        let fixture = fixture();

        for case in cases::<StrCase>(&fixture, "ahk_string") {
            assert_eq!(
                ahk_string(&case.input),
                case.out,
                "ahk_string({:?})",
                case.input
            );
        }
        for case in cases::<StrCase>(&fixture, "to_ahk_func_arg") {
            assert_eq!(
                to_ahk_func_arg(&case.input),
                case.out,
                "to_ahk_func_arg({:?})",
                case.input
            );
        }
        for case in cases::<StrCase>(&fixture, "escape_ahk_hotkey") {
            assert_eq!(
                escape_ahk_hotkey(&case.input),
                case.out,
                "escape_ahk_hotkey({:?})",
                case.input
            );
        }
        for case in cases::<ConcatCase>(&fixture, "concat") {
            assert_eq!(
                concat(&case.a, &case.b),
                case.out,
                "concat({:?},{:?})",
                case.a,
                case.b
            );
        }
        for case in cases::<DivideCase>(&fixture, "divide") {
            assert_eq!(
                divide(case.a, case.b),
                case.out,
                "divide({},{})",
                case.a,
                case.b
            );
        }
        for case in cases::<JoinCase>(&fixture, "join") {
            assert_eq!(
                join(&case.sep, &case.elems),
                case.out,
                "join({:?},…)",
                case.sep
            );
        }
        for case in cases::<LinesCase>(&fixture, "not_blank_lines") {
            // Go 的 nil ⇒ null ⇒ 视作空
            let want = case.out.unwrap_or_default();
            assert_eq!(
                not_blank_lines(&case.input),
                want,
                "not_blank_lines({:?})",
                case.input
            );
        }
    }

    #[test]
    fn divide_matches_go_on_zero_and_negative() {
        // Go 侧实测（夹具同值）：除零得 +Inf 并打印 "+Inf"；0/负结果为空串。
        assert_eq!(divide(1, 0), "+Inf");
        assert_eq!(divide(0, 5), "");
        assert_eq!(divide(-1, 2), "");
    }

    #[test]
    fn substr_is_rune_based_with_negative_length() {
        assert_eq!(substr("abcdef", 1, -1), "bcde");
        assert_eq!(substr("abc", 0, 2), "ab");
        assert_eq!(substr("abc", 5, 1), "");
        // rune 计数而非字节计数（"é" 与 "😀" 都算 1 个 rune）
        assert_eq!(substr("aé😀b", 1, -1), "é😀");
    }

    #[test]
    fn contains_only_modifier_covers_modifier_charset() {
        assert!(contains_only_modifier("#!^"));
        assert!(contains_only_modifier(" #!^ "));
        assert!(contains_only_modifier("<^>!"));
        assert!(!contains_only_modifier("q"));
        assert!(!contains_only_modifier(""));
        assert!(!contains_only_modifier("*CapsLock"));
    }
}
