//! 缩写命令页的**纯逻辑**（零 UI 依赖）——本 crate 内缩写逻辑的**唯一实现**。
//!
//! 逐项复刻 `config-ui-avalonia/ViewModels/AbbrPageViewModel.cs`（123 行）：
//! chips 构造、`formatSpace` 空格外显、`del `/`rn ` 命令解析、备注汇总（键文本走 `formatSpace`）。
//!
//! 页面构成：缩写 chips（**无绿色绑定态**，选中=Sand / 其余=Ivory）+ 命令框 + 动作编辑面板 + 备注汇总。
//!
//! > 合并说明（2026-09-28）：`services::keymap` 中曾并存一套等价实现，已删除；
//! > 其中两处设计优于本节初版，**予以吸收**：
//! > 1. [`SPACE_MARK`] 用单色 `□` 而非旧版 `◻️`（走 emoji 回退会被渲染成彩色方块）；
//! > 2. [`chip_item_width`] 按最长标签估算统一格宽，而非写死常数。

use std::collections::{HashMap, HashSet};

use crate::models::{Config, Keymap};
use crate::services::keymap::{self, CommentEntry};

/// 尾部空格的可见替代符。
///
/// ⚠️ 旧版用 `◻️`（U+25FD + VS16）；该字符在 WinUI/Segoe UI 下会走 **emoji 回退**渲染成彩色圆角方块
/// （Avalonia 侧同样踩过 emoji 回退坑，见迁移知识库）⇒ 改用单色 `□`（U+25A1）。
pub const SPACE_MARK: char = '\u{25a1}';

/// chip 格宽下限（旧 `MinWidth=53`）。
pub const CHIP_MIN_WIDTH: f64 = 53.0;
/// chip 格宽上限（防止单个超长缩写把整片布局拉爆）。
pub const CHIP_MAX_WIDTH: f64 = 160.0;
/// chip 高度（旧 44）。
pub const CHIP_HEIGHT: f64 = 44.0;

/// 复刻 `formatSpace`：尾部**逐个空格**显式为 [`SPACE_MARK`]（缩写键常以空格结尾，否则不可见）。
pub fn format_space(hotkey: &str) -> String {
    let trimmed = hotkey.trim_end_matches(' ');
    let spaces = hotkey.len() - trimmed.len();
    format!("{trimmed}{}", SPACE_MARK.to_string().repeat(spaces))
}

/// 单条标签的文本宽度估算（字号 17 的 Segoe UI 近似）。
fn estimate_text_width(label: &str) -> f64 {
    label
        .chars()
        .map(|character| if character.is_ascii() { 9.0 } else { 17.0 })
        .sum()
}

/// chip 格宽估算：`VariableSizedWrapGrid` 是**统一格宽**布局，故按最长标签取宽，
/// 结果夹在 [`CHIP_MIN_WIDTH`, `CHIP_MAX_WIDTH`]（两侧内边距共 20px）。
pub fn chip_item_width<'a>(labels: impl IntoIterator<Item = &'a str>) -> f64 {
    let widest = labels
        .into_iter()
        .map(estimate_text_width)
        .fold(0.0, f64::max);
    (widest + 20.0).clamp(CHIP_MIN_WIDTH, CHIP_MAX_WIDTH)
}

/// 缩写条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbbrChip {
    /// 热键原文（含尾部空格）。
    pub hotkey: String,
    /// 显示文本（`format_space`）。
    pub label: String,
    /// 是否可点（触发键自身禁用；复刻 `IsEnabled = !IsDisabledKey`）。
    pub enabled: bool,
    /// 是否选中（选中 ⇒ Sand 底，其余 Ivory 底；**无绑定态**）。
    pub selected: bool,
}

/// 命令解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbbrCommand {
    /// 什么都不做（空输入）。
    None,
    /// `del <键>`：删除该条目。
    Delete { hotkey: String },
    /// `rn <新键>`：把当前选中项改名。
    Rename { from: String, to: String },
    /// 其余输入：选中该键（不存在时由编辑核心惰性创建）。
    Select { hotkey: String },
}

/// 命令执行后对页面状态的指示（复刻 `RunCmd` 的收尾：清空输入框 + 更新选中）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbbrCommandOutcome {
    pub command: AbbrCommand,
    /// 执行后应设置的选中热键（`None` = 不改动）。
    pub next_selection: Option<String>,
    /// 是否清空命令输入框。
    pub clear_input: bool,
}

/// 复刻 `runCmd`：整体**转小写**后按前缀分发；非空输入总是清空命令框。
///
/// ⚠️ 前置空白**不**触发前缀分支（旧版 `StartsWith("del ")` 语义），已被测试固化。
pub fn parse_command(input: &str, selected_hotkey: &str) -> AbbrCommandOutcome {
    if input.trim().is_empty() {
        return AbbrCommandOutcome {
            command: AbbrCommand::None,
            next_selection: None,
            clear_input: false,
        };
    }

    let lowered = input.to_lowercase();

    if let Some(rest) = lowered.strip_prefix("del ") {
        return AbbrCommandOutcome {
            command: AbbrCommand::Delete {
                hotkey: rest.to_string(),
            },
            next_selection: Some(String::new()),
            clear_input: true,
        };
    }

    if let Some(rest) = lowered.strip_prefix("rn ") {
        return AbbrCommandOutcome {
            command: AbbrCommand::Rename {
                from: selected_hotkey.to_string(),
                to: rest.to_string(),
            },
            next_selection: Some(rest.to_string()),
            clear_input: true,
        };
    }

    AbbrCommandOutcome {
        command: AbbrCommand::Select {
            hotkey: lowered.clone(),
        },
        next_selection: Some(lowered),
        clear_input: true,
    }
}

/// 构造缩写 chips（复刻 `RefreshChips`）：按 `hotkeys` 顺序，标签走 `format_space`。
pub fn build_chips(
    keymap: &Keymap,
    disabled: &HashMap<i32, HashSet<String>>,
    selected_hotkey: Option<&str>,
) -> Vec<AbbrChip> {
    keymap
        .hotkeys
        .keys()
        .map(|hotkey| AbbrChip {
            hotkey: hotkey.clone(),
            label: format_space(hotkey),
            enabled: !keymap::is_disabled(disabled, keymap.id, hotkey),
            selected: selected_hotkey == Some(hotkey.as_str()),
        })
        .collect()
}

/// 备注汇总（键文本 = **原始热键**经 `format_space`，复刻 `RefreshComments`）。
///
/// ⚠️ 与键位图页不同：此处**不做** `key_text` 的去 `*` 与首字母大写
/// （旧版为 `FormatSpace(e.Hotkey)`，`Hotkey` 是原文）。
pub fn build_comment_entries(keymap: &Keymap, config: &Config) -> Vec<CommentEntry> {
    keymap::build_comment_entries(keymap, config)
        .into_iter()
        .map(|entry| CommentEntry {
            key_text: format_space(&entry.hotkey),
            ..entry
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Action, Options, WindowGroup};

    fn keymap_with(entries: &[(&str, i32)]) -> Keymap {
        let mut keymap = Keymap {
            id: 3,
            name: "Abbr".to_string(),
            hotkey: "Abbr".to_string(),
            enable: true,
            ..Default::default()
        };
        for (hotkey, type_id) in entries {
            keymap.hotkeys.insert(
                (*hotkey).to_string(),
                vec![Action {
                    type_id: *type_id,
                    comment: format!("c{type_id}"),
                    ..Default::default()
                }],
            );
        }
        keymap
    }

    #[test]
    fn format_space_reveals_trailing_spaces() {
        // 与 keymap 侧合并后的唯一断言集（原两处测试合并）
        assert_eq!(format_space("addr"), "addr");
        assert_eq!(format_space("addr "), format!("addr{SPACE_MARK}"));
        assert_eq!(format_space("a  "), format!("a{SPACE_MARK}{SPACE_MARK}"));
        assert_eq!(format_space(""), "");
        assert_eq!(format_space("   "), SPACE_MARK.to_string().repeat(3));
        assert_eq!(format_space("a b"), "a b", "中间空格不动");
        assert_eq!(
            SPACE_MARK, '\u{25a1}',
            "必须用单色 □ 而非 ◻️（后者走 emoji 回退）"
        );
    }

    #[test]
    fn chip_width_clamps_to_bounds() {
        assert_eq!(chip_item_width(["a"]), CHIP_MIN_WIDTH, "短标签取下限");
        assert!(
            chip_item_width(["address"]) > CHIP_MIN_WIDTH,
            "长于下限的标签按估算取宽"
        );
        assert_eq!(
            chip_item_width([std::iter::repeat_n('x', 60).collect::<String>().as_str()]),
            CHIP_MAX_WIDTH,
            "超长标签夹到上限"
        );
        assert_eq!(chip_item_width(std::iter::empty::<&str>()), CHIP_MIN_WIDTH);
        // CJK 比 ASCII 宽：同样 2 字符，CJK 更宽
        assert!(chip_item_width(["中文"]) > chip_item_width(["ab"]));
    }

    #[test]
    fn empty_or_blank_command_does_nothing() {
        for input in ["", "   ", "\t"] {
            let outcome = parse_command(input, "ab");
            assert_eq!(outcome.command, AbbrCommand::None);
            assert_eq!(outcome.next_selection, None);
            assert!(!outcome.clear_input, "空输入不清空命令框");
        }
    }

    #[test]
    fn del_command_targets_lowercased_remainder() {
        let outcome = parse_command("DEL AbC", "x");
        assert_eq!(
            outcome.command,
            AbbrCommand::Delete {
                hotkey: "abc".to_string()
            }
        );
        assert_eq!(
            outcome.next_selection,
            Some(String::new()),
            "删除后清空选中"
        );
        assert!(outcome.clear_input);
    }

    #[test]
    fn rn_command_renames_from_current_selection() {
        let outcome = parse_command("rn New AB", "old ab");
        assert_eq!(
            outcome.command,
            AbbrCommand::Rename {
                from: "old ab".to_string(),
                to: "new ab".to_string(),
            }
        );
        assert_eq!(outcome.next_selection, Some("new ab".to_string()));
        assert!(outcome.clear_input);
    }

    #[test]
    fn other_input_selects_lowercased_text() {
        let outcome = parse_command("Hello World", "");
        assert_eq!(
            outcome.command,
            AbbrCommand::Select {
                hotkey: "hello world".to_string()
            }
        );
        assert_eq!(outcome.next_selection, Some("hello world".to_string()));
    }

    #[test]
    fn prefix_without_space_or_after_blank_is_treated_as_selection() {
        // "del" 后无空格 ⇒ 不构成删除命令
        assert_eq!(
            parse_command("del", "x").command,
            AbbrCommand::Select {
                hotkey: "del".to_string()
            }
        );
        // 前置空白 ⇒ 不触发前缀分支（与 C# 一致）
        assert_eq!(
            parse_command(" del x", "").command,
            AbbrCommand::Select {
                hotkey: " del x".to_string()
            }
        );
    }

    #[test]
    fn chips_follow_hotkey_order_with_no_bound_state() {
        let keymap = keymap_with(&[("*b", 1), ("a ", 1), ("*a", 6)]);
        let mut disabled = HashMap::new();
        disabled.insert(3, HashSet::from(["a ".to_string()]));

        let chips = build_chips(&keymap, &disabled, Some("*b"));
        let hotkeys: Vec<&str> = chips.iter().map(|chip| chip.hotkey.as_str()).collect();
        assert_eq!(hotkeys, vec!["*a", "*b", "a "], "BTreeMap 有序 ⇒ 稳定顺序");

        let star_a = chips.iter().find(|chip| chip.hotkey == "*a").unwrap();
        assert!(!star_a.selected, "选中项唯一：*b 才是选中");
        let star_b = chips.iter().find(|chip| chip.hotkey == "*b").unwrap();
        assert!(star_b.selected, "选中项标记");
        assert!(star_b.enabled);

        let a_space = chips.iter().find(|chip| chip.hotkey == "a ").unwrap();
        assert_eq!(a_space.label, format!("a{SPACE_MARK}"));
        assert!(!a_space.enabled, "禁用键不可点");
    }

    #[test]
    fn chips_mark_trigger_key_disabled() {
        let mut keymap = keymap_with(&[("abbr", 1)]);
        keymap.hotkey = "Abbr".to_string();
        let config = Config {
            keymaps: vec![keymap.clone()],
            ..Default::default()
        };
        let chips = build_chips(&keymap, &keymap::disabled_keys(&config), None);
        assert!(!chips[0].enabled, "触发键自身不可点");
    }

    #[test]
    fn comment_entries_use_raw_hotkey_with_format_space() {
        let keymap = keymap_with(&[("btw ", 1)]);
        let config = Config {
            options: Options {
                window_groups: vec![WindowGroup {
                    id: 0,
                    name: "g".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let entries = build_comment_entries(&keymap, &config);
        assert_eq!(entries[0].key_text, format!("btw{SPACE_MARK}"));

        // 对照：键位图页口径走 key_text（去 * + 首字母大写，保留尾部空格）
        let matrix = keymap::build_comment_entries(&keymap, &config);
        assert_eq!(matrix[0].key_text, "Btw ", "矩阵页走 key_text");
    }
}
