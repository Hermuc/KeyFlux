//! 双语文案表（沿用旧版 `Resources/i18n.json` 的**格式与语义**）。
//!
//! 契约（逐字对齐 `config-ui-avalonia/Services/I18n.cs`）：
//! * 表结构：`{ "<key>": { "zh": "...", "en": "..." } }`，键为字符串（数字键 + 6 个非数字键
//!   `301err` / `301hint` / `1101_applied` / `1102_deleted` / `1103_only` / `1104_any`）；
//! * `t(key)`：空 key → `""`；剥掉 `label:` 前缀；按当前语言取值，为空则回退另一语言，
//!   仍为空则**原样返回 key**；
//! * `t_fmt(key, args)`：占位符 `{0}` `{1}`…；**占位符不匹配不抛异常**，回退未填充模板；
//! * 语言：`"en"`（忽略大小写）→ English，其余一律中文。
//!
//! 与旧版差异：旧版因 Avalonia `AssetLoader` 的限制必须做成**松散资源**（运行期探测路径）；
//! Rust 侧改用 `include_str!` **编译期内嵌**，既免路径探测，也让守卫测试可直接对账。
//!
//! ⚠️ 键数实测（2026-09-28）= **398**（旧记忆记的 393 已过时）。

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// 编译期内嵌的双语文案表（唯一真源：`resources/i18n.json`）。
const I18N_JSON: &str = include_str!("../../resources/i18n.json");

/// 当前语言。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    Zh,
    En,
}

impl Lang {
    pub const ZH: &'static str = "zh";
    pub const EN: &'static str = "en";

    /// 对齐旧版 `ApplyConfigLanguage`：`"en"`（忽略大小写）→ En，其余 → Zh。
    pub fn from_config(value: &str) -> Self {
        if value.eq_ignore_ascii_case(Self::EN) {
            Self::En
        } else {
            Self::Zh
        }
    }
}

/// 一条文案（任一侧可为 `null`，旧版模型即 `(string? Zh, string? En)`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub zh: Option<String>,
    #[serde(default)]
    pub en: Option<String>,
}

fn table() -> &'static HashMap<String, Entry> {
    static TABLE: OnceLock<HashMap<String, Entry>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(I18N_JSON).expect("resources/i18n.json must be valid UTF-8 JSON")
    })
}

/// 已加载的文案键数量（供守卫测试对账，不参与 UI 逻辑）。
pub fn key_count() -> usize {
    table().len()
}

/// 当前语言是否为英文。
static IS_EN: AtomicBool = AtomicBool::new(false);

pub fn language() -> Lang {
    if IS_EN.load(Ordering::Relaxed) {
        Lang::En
    } else {
        Lang::Zh
    }
}

pub fn set_language(lang: Lang) {
    IS_EN.store(lang == Lang::En, Ordering::Relaxed);
}

/// 依据 config 的 `options.language` 设置语言。
pub fn apply_config_language(config_language: &str) {
    set_language(Lang::from_config(config_language));
}

/// 翻译。查不到当前语言时回退另一语言，仍查不到则原样返回 key。
pub fn t(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let k = key.strip_prefix("label:").unwrap_or(key);
    if let Some(entry) = table().get(k) {
        let (primary, fallback) = match language() {
            Lang::Zh => (entry.zh.as_deref(), entry.en.as_deref()),
            Lang::En => (entry.en.as_deref(), entry.zh.as_deref()),
        };
        if let Some(text) = primary.filter(|s| !s.is_empty()) {
            return text.to_string();
        }
        if let Some(text) = fallback.filter(|s| !s.is_empty()) {
            return text.to_string();
        }
    }
    k.to_string()
}

/// 翻译 + 占位符填充（`{0}` `{1}`…）。占位符缺失或数量不匹配时**不抛异常**，
/// 返回未填充的模板 —— 旧版用 `catch (FormatException) return template`，此处天然无异常。
pub fn t_fmt(key: &str, args: &[&str]) -> String {
    let template = t(key);
    if args.is_empty() {
        return template;
    }
    let mut out = template;
    for (index, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{index}}}"), arg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 语言是全局状态，测试默认并行 ⇒ 触碰语言的用例串行化，避免相互干扰。
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// 守卫：文案键数与真源一致（变更文案表必须同步此断言，并在注释里写明缘由）。
    #[test]
    fn key_count_matches_contract() {
        // 2026-09-30 实测：401 = 数字键 + 6 个非数字键（较 398 新增 2593/2594：
        // 选项页「亚克力毛玻璃效果」标签 + 其偏好保存失败提示）。
        assert_eq!(key_count(), 401, "i18n 键数变化时必须同步本断言");
    }

    #[test]
    fn non_numeric_keys_exist() {
        for k in [
            "301err",
            "301hint",
            "1101_applied",
            "1102_deleted",
            "1103_only",
            "1104_any",
        ] {
            assert!(table().contains_key(k), "缺少非数字键 {k}");
        }
    }

    #[test]
    fn empty_key_returns_empty() {
        assert_eq!(t(""), "");
    }

    #[test]
    fn missing_key_returns_key_itself() {
        assert_eq!(t("no-such-key-xyz"), "no-such-key-xyz");
    }

    #[test]
    fn label_prefix_is_stripped() {
        assert_eq!(t("label:1"), t("1"));
    }

    #[test]
    fn language_switch_and_fallback() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_language(Lang::En);
        assert_eq!(t("1"), "Close");
        let en = t("1");
        set_language(Lang::Zh);
        let zh = t("1");
        assert!(!zh.is_empty());
        assert_ne!(zh, en, "中英应不同（键 1 两侧均已翻译）");
    }

    #[test]
    fn fill_placeholders_and_keep_template_on_mismatch() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_language(Lang::Zh);
        assert_eq!(t_fmt("1", &[]), t("1"), "无参数时返回模板");
        let out = t_fmt("1", &["x"]);
        assert!(!out.is_empty());
        assert_eq!(out, t("1"), "模板无占位符时保持原样，不抛异常");
    }

    #[test]
    fn lang_from_config_is_case_insensitive() {
        assert_eq!(Lang::from_config("EN"), Lang::En);
        assert_eq!(Lang::from_config("en"), Lang::En);
        assert_eq!(Lang::from_config("zh"), Lang::Zh);
        assert_eq!(Lang::from_config(""), Lang::Zh);
        assert_eq!(Lang::from_config("fr"), Lang::Zh);
    }
}
