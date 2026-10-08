//! 契约标识符（ID / 名称）词表的**单一实现**（多维度优化：结构 & 去重）。
//!
//! 背景：`^[a-z][a-z0-9_]{0,N}$` 这类判定一度在 5 处各写一份 ——
//! `generator::plugins`（手写 3 个）、`server::settings_store`（手写，逐字重复）、
//! `server::handlers_behaviors`（手写 + 内联进 plugin-action 判定）、
//! `server::validate`（每次调用现场编译 `regex`）。语义全靠"逐字抄对"维持：任一处漏改
//! 就是**静默放行或误拒**，而既有对账闸门（`models/contract.rs`）只比 wire 字段名，
//! 看不见这些谓词。
//!
//! 本模块是**叶层**（零依赖），`generator` 与 `server` 均可直接用；调用方只给长度上限
//! （`None` = 不限长），不再各写循环。词族与长度上限 `max` 的口径：
//!
//! | 函数 | 模式 | 典型 `max` |
//! |---|---|---|
//! | [`lower_ident`] | `^[a-z][a-z0-9_]{0,max-1}$` | 插件 ID / 行为 ID `Some(32)`、匹配类型 ID `Some(24)`、文件分组名 `None` |
//! | [`mixed_ident`] | `^[A-Za-z][A-Za-z0-9_]{0,max-1}$` | 设置项 key / 插件动作 id `Some(32)` |
//! | [`ahk_ident`] | `^[A-Za-z_][A-Za-z0-9_]{0,max-1}$` | 晚初始化函数名 `Some(64)` |
//! | [`plugin_action`] | `^plugin:[a-z][a-z0-9_]{0,31}:[A-Za-z0-9_-]{1,64}$` | —— |
//!
//! 判据等效性：字符集按 **ASCII 字节**判定，非 ASCII 字节一律拒绝（故长度按字节数比较
//! 与按字符数比较在此等价）；`None` 对应 Go 的无界 `*` 量词。

/// `^[a-z][a-z0-9_]{0,max-1}$`；`max_len = None` 表示不限长。
pub(crate) fn lower_ident(value: &str, max_len: Option<usize>) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    if max_len.is_some_and(|max| bytes.len() > max) {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// `^[A-Za-z][A-Za-z0-9_]{0,max-1}$`；`None` = 不限长。
pub(crate) fn mixed_ident(value: &str, max_len: Option<usize>) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    if max_len.is_some_and(|max| bytes.len() > max) {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// `^[A-Za-z_][A-Za-z0-9_]{0,max-1}$`（AHK 标识符可下划线开头）；`None` = 不限长。
pub(crate) fn ahk_ident(value: &str, max_len: Option<usize>) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !(bytes[0] == b'_' || bytes[0].is_ascii_alphabetic()) {
        return false;
    }
    if max_len.is_some_and(|max| bytes.len() > max) {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// `^plugin:[a-z][a-z0-9_]{0,31}:[A-Za-z0-9_-]{1,64}$`（行为包对插件动作的引用）。
pub(crate) fn plugin_action(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("plugin:") else {
        return false;
    };
    let Some((plugin_id, action)) = rest.split_once(':') else {
        return false;
    };
    if !lower_ident(plugin_id, Some(32)) {
        return false;
    }
    let name = action.as_bytes();
    !name.is_empty()
        && name.len() <= 64
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `lower_ident`：首字符小写、词表、长度上限（`None` 不限长）三个边界一起钉住。
    #[test]
    fn lower_ident_boundaries() {
        assert!(lower_ident("a", Some(32)));
        assert!(lower_ident("quick_switch", Some(32)));
        assert!(lower_ident(&"a".repeat(32), Some(32)), "恰好上限应通过");
        assert!(!lower_ident(&"a".repeat(33), Some(32)), "超长必须拒绝");
        assert!(!lower_ident("", Some(32)));
        assert!(!lower_ident("A", Some(32)), "首字符必须小写");
        assert!(!lower_ident("1a", Some(32)));
        assert!(!lower_ident("a-b", Some(32)), "连字符不在词表");
        assert!(!lower_ident("a中", Some(32)), "非 ASCII 拒绝");
        // None = 不限长（与 Go `^[a-z][a-z0-9_]*$` 同口径）
        assert!(lower_ident(&"a".repeat(200), None));
        assert!(!lower_ident(&format!("{}-", "a".repeat(199)), None));
    }

    /// `mixed_ident`：与 `lower_ident` 只差「首字符大小写均可」。
    #[test]
    fn mixed_ident_boundaries() {
        assert!(mixed_ident("Key", Some(32)));
        assert!(mixed_ident("k", Some(32)));
        assert!(mixed_ident(&"a".repeat(32), Some(32)));
        assert!(!mixed_ident(&"a".repeat(33), Some(32)));
        assert!(!mixed_ident("_key", Some(32)), "下划线不可作首字符");
        assert!(!mixed_ident("k-e", Some(32)));
        assert!(!mixed_ident("", Some(32)));
    }

    /// `ahk_ident`：唯一允许下划线开头的一族。
    #[test]
    fn ahk_ident_boundaries() {
        assert!(ahk_ident("_init", Some(64)));
        assert!(ahk_ident("QuickSwitchMain", Some(64)));
        assert!(ahk_ident(&"a".repeat(64), Some(64)));
        assert!(!ahk_ident(&"a".repeat(65), Some(64)));
        assert!(
            ahk_ident("_", Some(64)),
            "单个下划线是合法标识符（与旧实现一致）"
        );
        assert!(!ahk_ident("1a", Some(64)));
        assert!(!ahk_ident("a-b", Some(64)));
    }

    /// `plugin_action`：两段都必须合法，动作名不得含冒号。
    #[test]
    fn plugin_action_boundaries() {
        assert!(plugin_action("plugin:quick_switch:do_thing"));
        assert!(plugin_action(&format!(
            "plugin:{}:{}",
            "a".repeat(32),
            "b".repeat(64)
        )));
        assert!(!plugin_action(&format!("plugin:{}:b", "a".repeat(33))));
        assert!(!plugin_action(&format!(
            "plugin:quick_switch:{}",
            "b".repeat(65)
        )));
        assert!(!plugin_action("plugin:quick_switch"), "缺第二段");
        assert!(!plugin_action("quick_switch:do"), "缺 plugin: 前缀");
        assert!(!plugin_action("plugin:Quick:do"), "插件 ID 首字符必须小写");
        assert!(!plugin_action("plugin:quick:"), "动作名不得为空");
        assert!(
            !plugin_action("plugin:quick:do:extra"),
            "动作名含冒号必须拒绝"
        );
    }
}
