//! 键位图系页面（模式矩阵 / 缩写 / 自定义热键）的**共享纯逻辑**（零 UI 依赖）。
//!
//! 逐项复刻 `config-ui-avalonia/ViewModels/KeymapEditorCore.cs` +
//! `KeymapPageViewModel.cs` 的**可判定部分**（选中态、禁用键、绑定态、备注汇总、键格几何）。
//! UI 层只做「状态 → 颜色/控件」的翻译（见 `ui::keymap_view`）。

use std::collections::{HashMap, HashSet};

use crate::models::{Action, Config, Keymap};
use crate::services::i18n;
use crate::services::store::parse_keyboard_layout;

/// 键盘一行的键格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCell {
    /// 热键（含 `*` 前缀，如 `"*q"`）。
    pub hotkey: String,
    /// 显示文本（去 `*` + 首字母大写）。
    pub label: String,
}

/// 键盘一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardRow {
    pub cells: Vec<KeyCell>,
}

/// 键格视觉状态（颜色映射在 UI 层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    /// 当前选中（旧：白底 + 陶土字/描边）。
    Selected,
    /// 触发键自身 ⇒ 禁用不可点（旧：BorderCream 底）。
    Disabled,
    /// 已绑定动作（旧：MutedGreenSoft 底；**缩写语境不显示**）。
    Bound,
    /// 空键（旧：Ivory 底）。
    Empty,
}

/// 备注汇总条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentEntry {
    /// **原始热键**（含 `*` 与尾部空格）。
    ///
    /// 键位图页用 [`key_text`] 展示；缩写页则用 `abbr::format_space(原始热键)`
    /// （旧版 `AbbrPageViewModel` 走 `FormatSpace(e.Hotkey)`，**不做**去 `*`/首字母大写）。
    pub hotkey: String,
    /// 键显示文本（`key_text`）。
    pub key_text: String,
    /// 合并后的备注（多行，`\r\n` 分隔）。
    pub comment: String,
}

/// 复刻 `getKeyText`：去掉 `*` 前缀并把首字符大写。
pub fn key_text(hotkey: &str) -> String {
    let stripped = hotkey.trim_start_matches('*');
    let mut chars = stripped.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 构建键盘网格行（布局解析复用 `store::parse_keyboard_layout`，保证与保存清洗同源）。
pub fn build_rows(layout: &str, keymap_hotkey: &str) -> Vec<KeyboardRow> {
    parse_keyboard_layout(layout, keymap_hotkey)
        .into_iter()
        .map(|row| KeyboardRow {
            cells: row
                .into_iter()
                .map(|hotkey| KeyCell {
                    label: key_text(&hotkey),
                    hotkey,
                })
                .collect(),
        })
        .collect()
}

/// 复刻 `IsAbbr`：`keymap.hotkey` 含 `Abbr`（**区分大小写**，与 C# `Ordinal` 一致）。
pub fn is_abbr(keymap: &Keymap) -> bool {
    keymap.hotkey.contains("Abbr")
}

/// 键格是否为单字符键（决定 43×43 正方形）。
pub fn is_single_char(cell: &KeyCell) -> bool {
    cell.label.chars().count() == 1
}

/// 复刻 `.small`：首行超过 10 键（如 104 键布局）时整体缩小字号。
pub fn small_font(rows: &[KeyboardRow]) -> bool {
    rows.first()
        .map(|row| row.cells.len() > 10)
        .unwrap_or(false)
}

/// 键格字号（复刻 20 / 15.7）。
pub fn key_font_size(small: bool) -> f64 {
    if small { 15.7 } else { 20.0 }
}

/// 复刻 `_disabledKeys`：对每个**已启用** keymap，其触发键（含 `*` 变体、全小写）
/// 在「自身模式」与「父模式」的格子中禁用。
///
/// 语义：避免把「进入模式的键」在其模式内再绑定动作。
pub fn disabled_keys(config: &Config) -> HashMap<i32, HashSet<String>> {
    let mut map: HashMap<i32, HashSet<String>> = HashMap::new();
    for keymap in config.keymaps.iter().filter(|km| km.enable) {
        let hotkey = keymap.hotkey.to_lowercase();
        for id in [keymap.id, keymap.parent_id] {
            let entry = map.entry(id).or_default();
            entry.insert(hotkey.clone());
            entry.insert(format!("*{hotkey}"));
        }
    }
    map
}

/// 查询某 keymap 的某键是否禁用（查询键统一转小写，与 C# 一致）。
pub fn is_disabled(disabled: &HashMap<i32, HashSet<String>>, keymap_id: i32, hotkey: &str) -> bool {
    disabled
        .get(&keymap_id)
        .map(|set| set.contains(&hotkey.to_lowercase()))
        .unwrap_or(false)
}

/// 复刻 `IsBound`：按**当前窗口分组**判定该键是否有非空动作（只读，不改模型）。
pub fn is_bound(keymap: &Keymap, hotkey: &str, window_group_id: i32) -> bool {
    keymap
        .hotkeys
        .get(hotkey)
        .map(|actions| {
            actions
                .iter()
                .any(|action| action.window_group_id == window_group_id && !action.is_empty)
        })
        .unwrap_or(false)
}

/// 键格状态（复刻 `Key.vue` 的 keyColor/disabled 判定顺序）。
pub fn cell_state(selected: bool, disabled: bool, bound: bool, is_abbr: bool) -> CellState {
    if selected {
        CellState::Selected
    } else if disabled {
        CellState::Disabled
    } else if !is_abbr && bound {
        CellState::Bound
    } else {
        CellState::Empty
    }
}

/// 复刻 `ActionCommentTable.getActionAllComment`：
/// 每键的备注 = 各**非空**动作按 `「分组名: 」+ 翻译后备注` 逐行拼接（分组 0 无前缀）；
/// 空备注整体剔除；最后按备注排序。
///
/// ⚠️ 与 C# 的唯一差异：C# 用 `StringComparison.CurrentCulture`（区域感知），
/// Rust 侧用码点序 —— 仅影响备注列顺序，不影响内容。
pub fn build_comment_entries(keymap: &Keymap, config: &Config) -> Vec<CommentEntry> {
    comment_pairs(keymap, config)
        .into_iter()
        .map(|(hotkey, comment)| CommentEntry {
            key_text: key_text(&hotkey),
            hotkey,
            comment,
        })
        .collect()
}

/// 备注汇总的公共部分：`(hotkey 原文, 合并备注)`，按备注排序（保留 hotkey 供两种显示口径消费）。
fn comment_pairs(keymap: &Keymap, config: &Config) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = keymap
        .hotkeys
        .iter()
        .filter_map(|(hotkey, actions)| {
            let mut lines: Vec<String> = Vec::new();
            for action in actions {
                if action.comment.is_empty() {
                    continue;
                }
                let prefix = if action.window_group_id == 0 {
                    String::new()
                } else {
                    let name = config
                        .options
                        .window_groups
                        .iter()
                        .find(|group| group.id == action.window_group_id)
                        .map(|group| group.name.clone())
                        .unwrap_or_default();
                    format!("{name}: ")
                };
                lines.push(format!("{prefix}{}", i18n::t(&action.comment)));
            }
            if lines.is_empty() {
                return None;
            }
            Some((hotkey.clone(), lines.join("\r\n")))
        })
        .collect();

    entries.sort_by(|left, right| left.1.cmp(&right.1));
    entries
}

/// 复刻 `_getAction` 的解析部分：按 (hotkey, windowGroupId) 取动作（只读，不惰性建）。
pub fn find_action<'a>(
    keymap: &'a Keymap,
    hotkey: &str,
    window_group_id: i32,
) -> Option<&'a Action> {
    keymap
        .hotkeys
        .get(hotkey)?
        .iter()
        .find(|action| action.window_group_id == window_group_id)
}

/// 复刻 `_getAction`：按 `(keymap, hotkey, windowGroupId)` 解析动作，**不存在则惰性初始化**。
///
/// * 键不存在 ⇒ 建空表；若 keymap 是**新建**且键为 `singlePress`，填入「输入触发键」默认动作；
/// * 分组不存在 ⇒ 补一个 `typeId=0 / isEmpty` 的占位动作，并按分组 id 保持升序。
///
/// ⚠️ 默认动作的备注文案沿旧实现**硬编码中文**（`"输入 X 键"`），不做 i18n —— 忠实复刻。
pub fn ensure_action<'a>(
    config: &'a mut Config,
    keymap_id: i32,
    hotkey: &str,
    window_group_id: i32,
) -> Option<&'a mut Action> {
    if hotkey.is_empty() {
        return None;
    }

    let keymap = config.keymaps.iter_mut().find(|km| km.id == keymap_id)?;

    if !keymap.hotkeys.contains_key(hotkey) {
        let mut actions: Vec<Action> = Vec::new();
        if keymap.is_new && hotkey == "singlePress" {
            let key = keymap
                .hotkey
                .trim_start_matches([' ', '#', '!', '^', '+', '<', '>', '*', '~', '$']);
            actions.push(Action {
                window_group_id: 0,
                type_id: 6,
                is_empty: false,
                keys_to_send: format!("{{blind}}{{{key}}}"),
                comment: format!("输入 {key} 键"),
                ..Default::default()
            });
        }
        keymap.hotkeys.insert(hotkey.to_string(), actions);
    }

    let actions = keymap.hotkeys.get_mut(hotkey)?;
    if let Some(index) = actions
        .iter()
        .position(|action| action.window_group_id == window_group_id)
    {
        return Some(&mut actions[index]);
    }

    actions.push(Action {
        window_group_id,
        type_id: 0,
        is_empty: true,
        ..Default::default()
    });
    actions.sort_by_key(|action| action.window_group_id);
    let index = actions
        .iter()
        .position(|action| action.window_group_id == window_group_id)?;
    Some(&mut actions[index])
}

/// 复刻 `removeHotkey`：删除该热键条目，返回是否真的删了。
pub fn remove_hotkey(keymap: &mut Keymap, hotkey: &str) -> bool {
    keymap.hotkeys.remove(hotkey).is_some()
}

/// 复刻 `changeHotkey`：把 `old_hotkey` 改名为 `new_hotkey`，返回最终生效的键名。
///
/// 三条分支（与 C# 逐条对应）：
/// 1. `old` 不存在或为空 ⇒ 原样返回 `new`（不动作）；
/// 2. `new` 已存在：若**当前键是未配置**（首个动作 `typeId == 0`）⇒ 删当前键、保留目标键；
///    若 `old == new`（且非未配置）⇒ 直接返回；否则删掉目标键再把当前键改名过去；
/// 3. 其余 ⇒ 直接改名。
///
/// ⚠️ C# 用「重建字典保持插入顺序」；Rust 侧 `BTreeMap` 天然有序 ⇒ 语义等价且更可复现。
pub fn change_hotkey(keymap: &mut Keymap, old_hotkey: &str, new_hotkey: &str) -> String {
    if old_hotkey.is_empty() || !keymap.hotkeys.contains_key(old_hotkey) {
        return new_hotkey.to_string();
    }

    let current_is_unconfigured = keymap
        .hotkeys
        .get(old_hotkey)
        .and_then(|actions| actions.first())
        .map(|action| action.type_id == 0)
        .unwrap_or(false);

    if keymap.hotkeys.contains_key(new_hotkey) {
        if current_is_unconfigured {
            keymap.hotkeys.remove(old_hotkey);
            return new_hotkey.to_string();
        }
        if new_hotkey == old_hotkey {
            return new_hotkey.to_string();
        }
        keymap.hotkeys.remove(new_hotkey);
    }

    if let Some(actions) = keymap.hotkeys.remove(old_hotkey) {
        keymap.hotkeys.insert(new_hotkey.to_string(), actions);
    }
    new_hotkey.to_string()
}

/// 页头标题（复刻 `HeaderTitle`）：keymap 有名字用名字，否则用触发键。
pub fn header_title(keymap: &Keymap) -> String {
    if keymap.name.is_empty() {
        keymap.hotkey.clone()
    } else {
        keymap.name.clone()
    }
}

/// 子模式信息（复刻 `ParentInfo`，label:503）：`parentId != 0` 时给出上层模式名。
pub fn parent_info(keymap: &Keymap, config: &Config) -> Option<String> {
    if keymap.parent_id == 0 {
        return None;
    }
    let name = config
        .keymaps
        .iter()
        .find(|candidate| candidate.id == keymap.parent_id)
        .map(|parent| {
            if parent.name.is_empty() {
                parent.hotkey.clone()
            } else {
                parent.name.clone()
            }
        })
        .unwrap_or_default();
    Some(format!("{}: {name}", i18n::t("503")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Action, Options, WindowGroup};

    fn action(window_group_id: i32, is_empty: bool, comment: &str) -> Action {
        Action {
            window_group_id,
            is_empty,
            comment: comment.to_string(),
            ..Default::default()
        }
    }

    fn keymap(id: i32, name: &str, hotkey: &str, enable: bool) -> Keymap {
        Keymap {
            id,
            name: name.to_string(),
            hotkey: hotkey.to_string(),
            enable,
            parent_id: 0,
            ..Default::default()
        }
    }

    #[test]
    fn key_text_strips_star_and_capitalizes() {
        assert_eq!(key_text("*q"), "Q");
        assert_eq!(key_text("*f1"), "F1");
        assert_eq!(key_text("singlePress"), "SinglePress");
        assert_eq!(key_text("*"), "");
        assert_eq!(key_text(""), "");
    }

    #[test]
    fn rows_reuse_save_path_layout_parsing() {
        let rows = build_rows("q w\nsinglePress", "f");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cells.len(), 2);
        assert_eq!(rows[0].cells[0].hotkey, "*q");
        assert_eq!(rows[0].cells[0].label, "Q");
        assert_eq!(rows[1].cells[0].hotkey, "singlePress");
        assert!(is_single_char(&rows[0].cells[0]));
        assert!(!is_single_char(&rows[1].cells[0]), "SinglePress 非单字符");
    }

    #[test]
    fn small_font_triggers_on_more_than_ten_keys_in_first_row() {
        let narrow = build_rows("q w e", "f");
        assert!(!small_font(&narrow));
        assert_eq!(key_font_size(false), 20.0);

        let wide = build_rows("q w e r t y u i o p a", "f");
        assert!(small_font(&wide), "11 键首行 ⇒ 缩小字号");
        assert_eq!(key_font_size(true), 15.7);

        assert!(!small_font(&[]));
    }

    #[test]
    fn disabled_keys_cover_self_and_parent_with_star_variants() {
        let mut child = keymap(5, "子模式", "j", true);
        child.parent_id = 1;
        let config = Config {
            keymaps: vec![
                keymap(1, "F 模式", "F", true),
                child,
                keymap(6, "禁用模式", "k", false),
            ],
            ..Default::default()
        };

        let disabled = disabled_keys(&config);
        // 自身模式禁用：hotkey 与 *hotkey（全小写）
        assert!(is_disabled(&disabled, 1, "f"));
        assert!(is_disabled(&disabled, 1, "*F"), "查询大小写不敏感");
        assert!(is_disabled(&disabled, 5, "j"));
        // 子模式触发键在其父模式中同样禁用（避免把进入子模式的键绑定成动作）
        assert!(is_disabled(&disabled, 1, "j"));
        // 反之不成立：父模式的触发键在子模式中**不**禁用（与 C# `_disabledKeys` 一致）
        assert!(!is_disabled(&disabled, 5, "f"));
        // 未启用 keymap 不参与
        assert!(!is_disabled(&disabled, 6, "k"));
        // 无关键格不受影响
        assert!(!is_disabled(&disabled, 5, "*q"));
    }

    #[test]
    fn bound_requires_matching_window_group_and_non_empty() {
        let mut keymap = keymap(5, "M", "j", true);
        let actions = vec![
            action(0, false, "a"),
            action(2, true, "b"), // 空动作 ⇒ 不算绑定
        ];
        keymap.hotkeys.insert("*q".to_string(), actions);

        assert!(is_bound(&keymap, "*q", 0));
        assert!(!is_bound(&keymap, "*q", 1), "该分组无动作");
        assert!(!is_bound(&keymap, "*q", 2), "仅有空动作");
        assert!(!is_bound(&keymap, "*q", 999));
        assert!(!is_bound(&keymap, "*zzz", 0));
    }

    #[test]
    fn cell_state_priority_matches_legacy() {
        // 选中优先级最高
        assert_eq!(cell_state(true, true, true, false), CellState::Selected);
        // 其次禁用
        assert_eq!(cell_state(false, true, true, false), CellState::Disabled);
        // 再绑定（非缩写语境）
        assert_eq!(cell_state(false, false, true, false), CellState::Bound);
        // 缩写语境不显示绑定态
        assert_eq!(cell_state(false, false, true, true), CellState::Empty);
        assert_eq!(cell_state(false, false, false, false), CellState::Empty);
    }

    #[test]
    fn comment_entries_prefix_group_and_skip_empty() {
        let mut keymap = keymap(5, "M", "j", true);
        keymap.hotkeys.insert(
            "*q".to_string(),
            vec![action(0, false, "输入 q 键"), action(2, false, "打开微信")],
        );
        keymap
            .hotkeys
            .insert("*w".to_string(), vec![action(0, true, "")]);

        let config = Config {
            options: Options {
                window_groups: vec![WindowGroup {
                    id: 2,
                    name: "微信".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let entries = build_comment_entries(&keymap, &config);
        assert_eq!(entries.len(), 1, "空备注键被剔除");
        assert_eq!(entries[0].key_text, "Q");
        // 分组 0 无前缀；分组 2 有「名字: 」前缀；多行以 CRLF 拼接
        assert_eq!(entries[0].comment, "输入 q 键\r\n微信: 打开微信");
    }

    #[test]
    fn header_title_and_parent_info() {
        let mut child = keymap(5, "", "j", true);
        assert_eq!(header_title(&child), "j", "无名字回退触发键");
        child.name = "我的模式".to_string();
        assert_eq!(header_title(&child), "我的模式");

        let config = Config {
            keymaps: vec![keymap(1, "", "F", true), child.clone()],
            ..Default::default()
        };
        assert_eq!(parent_info(&child, &config), None, "parentId=0 无上层信息");

        child.parent_id = 1;
        let info = parent_info(&child, &config).expect("有上层信息");
        assert!(info.contains("F"), "上层无名字时用触发键: {info}");
    }

    #[test]
    fn ensure_action_creates_placeholder_for_missing_group() {
        let mut config = Config {
            keymaps: vec![keymap(5, "M", "j", true)],
            ..Default::default()
        };
        let action = ensure_action(&mut config, 5, "*q", 0).expect("应创建");
        assert_eq!(action.type_id, 0);
        assert!(action.is_empty, "新分组补占位动作");
        assert_eq!(action.window_group_id, 0);
        assert!(config.keymaps[0].hotkeys.contains_key("*q"), "键表被建立");
    }

    #[test]
    fn ensure_action_fills_single_press_default_for_new_keymap() {
        let mut new_keymap = keymap(9, "新模式", "F7", true);
        new_keymap.is_new = true;
        let mut config = Config {
            keymaps: vec![new_keymap],
            ..Default::default()
        };
        let action = ensure_action(&mut config, 9, "singlePress", 0).expect("应创建");
        assert_eq!(action.type_id, 6);
        assert_eq!(action.keys_to_send, "{blind}{F7}", "修饰符被剥离");
        assert!(!action.is_empty);
        assert!(action.comment.contains("F7"));
    }

    #[test]
    fn ensure_action_is_idempotent_and_keeps_group_order() {
        let mut config = Config {
            keymaps: vec![keymap(5, "M", "j", true)],
            ..Default::default()
        };
        ensure_action(&mut config, 5, "*q", 2);
        ensure_action(&mut config, 5, "*q", 0);
        ensure_action(&mut config, 5, "*q", 1);
        let groups: Vec<i32> = config.keymaps[0].hotkeys["*q"]
            .iter()
            .map(|a| a.window_group_id)
            .collect();
        assert_eq!(groups, vec![0, 1, 2], "补入后保持分组升序");

        // 再次取同一分组不新增
        ensure_action(&mut config, 5, "*q", 1);
        assert_eq!(config.keymaps[0].hotkeys["*q"].len(), 3);
    }

    #[test]
    fn ensure_action_rejects_empty_hotkey_and_unknown_keymap() {
        let mut config = Config {
            keymaps: vec![keymap(5, "M", "j", true)],
            ..Default::default()
        };
        assert!(ensure_action(&mut config, 5, "", 0).is_none());
        assert!(ensure_action(&mut config, 999, "*q", 0).is_none());
        assert!(config.keymaps[0].hotkeys.is_empty(), "无效调用不改模型");
    }

    fn keymap_with_entries(entries: &[(&str, i32)]) -> Keymap {
        let mut keymap = keymap(5, "M", "j", true);
        for (hotkey, type_id) in entries {
            keymap.hotkeys.insert(
                (*hotkey).to_string(),
                vec![Action {
                    type_id: *type_id,
                    ..Default::default()
                }],
            );
        }
        keymap
    }

    #[test]
    fn remove_hotkey_reports_whether_it_existed() {
        let mut keymap = keymap_with_entries(&[("*a", 1)]);
        assert!(remove_hotkey(&mut keymap, "*a"));
        assert!(!remove_hotkey(&mut keymap, "*a"), "重复删除返回 false");
        assert!(keymap.hotkeys.is_empty());
    }

    #[test]
    fn change_hotkey_renames_keeping_actions() {
        let mut keymap = keymap_with_entries(&[("*a", 6)]);
        let result = change_hotkey(&mut keymap, "*a", "*b");
        assert_eq!(result, "*b");
        assert!(!keymap.hotkeys.contains_key("*a"));
        assert_eq!(keymap.hotkeys["*b"][0].type_id, 6, "动作随键迁移");
    }

    #[test]
    fn change_hotkey_drops_unconfigured_current_when_target_exists() {
        // 当前键未配置（typeId=0），目标键已配置 ⇒ 删当前键、目标键原样保留
        let mut keymap = keymap_with_entries(&[("*a", 0), ("*b", 6)]);
        let result = change_hotkey(&mut keymap, "*a", "*b");
        assert_eq!(result, "*b");
        assert!(!keymap.hotkeys.contains_key("*a"), "未配置的当前键被删除");
        assert_eq!(keymap.hotkeys["*b"][0].type_id, 6, "目标键未被覆盖");
    }

    #[test]
    fn change_hotkey_overwrites_target_when_current_is_configured() {
        let mut keymap = keymap_with_entries(&[("*a", 6), ("*b", 9)]);
        let result = change_hotkey(&mut keymap, "*a", "*b");
        assert_eq!(result, "*b");
        assert_eq!(keymap.hotkeys.len(), 1, "目标键被当前键覆盖");
        assert_eq!(keymap.hotkeys["*b"][0].type_id, 6);
    }

    #[test]
    fn change_hotkey_handles_same_key_and_missing_source() {
        // old == new 且已配置 ⇒ 原样返回，不删
        let mut keymap = keymap_with_entries(&[("*a", 6)]);
        assert_eq!(change_hotkey(&mut keymap, "*a", "*a"), "*a");
        assert!(keymap.hotkeys.contains_key("*a"));

        // old == new 且未配置 ⇒ 删自身
        let mut keymap = keymap_with_entries(&[("*a", 0)]);
        assert_eq!(change_hotkey(&mut keymap, "*a", "*a"), "*a");
        assert!(!keymap.hotkeys.contains_key("*a"));

        // 源不存在 ⇒ 不改动
        let mut keymap = keymap_with_entries(&[("*a", 6)]);
        assert_eq!(change_hotkey(&mut keymap, "*zzz", "*b"), "*b");
        assert!(!keymap.hotkeys.contains_key("*b"));
    }

    #[test]
    fn find_action_is_read_only_lookup() {
        let mut keymap = keymap(5, "M", "j", true);
        keymap
            .hotkeys
            .insert("*q".to_string(), vec![action(0, false, "a")]);
        assert!(find_action(&keymap, "*q", 0).is_some());
        assert!(find_action(&keymap, "*q", 1).is_none());
        assert!(find_action(&keymap, "*zzz", 0).is_none());
        assert_eq!(keymap.hotkeys.len(), 1, "查询不得写入模型");
    }

    #[test]
    fn remove_hotkey_reports_whether_entry_existed() {
        let mut abbr = keymap(3, "", "Abbr", true);
        abbr.hotkeys.insert("btw".to_string(), vec![]);
        assert!(remove_hotkey(&mut abbr, "btw"));
        assert!(!remove_hotkey(&mut abbr, "btw"));
        assert!(!remove_hotkey(&mut abbr, "zzz"));
    }

    #[test]
    fn change_hotkey_renames_and_resolves_conflicts() {
        let configured = Action {
            window_group_id: 0,
            type_id: 6,
            is_empty: false,
            comment: "mine".to_string(),
            ..Default::default()
        };

        // 常规改名
        let mut abbr = keymap(3, "", "Abbr", true);
        abbr.hotkeys
            .insert("btw".to_string(), vec![action(0, false, "x")]);
        assert_eq!(change_hotkey(&mut abbr, "btw", "brb"), "brb");
        assert!(abbr.hotkeys.contains_key("brb"));
        assert!(!abbr.hotkeys.contains_key("btw"));

        // 目标已存在 + 当前键未配置（首动作 typeId=0）⇒ 丢弃当前键，保留目标键
        let mut abbr = keymap(3, "", "Abbr", true);
        abbr.hotkeys
            .insert("btw".to_string(), vec![action(0, true, "")]);
        abbr.hotkeys
            .insert("brb".to_string(), vec![action(0, false, "old")]);
        assert_eq!(change_hotkey(&mut abbr, "btw", "brb"), "brb");
        assert!(!abbr.hotkeys.contains_key("btw"));
        assert_eq!(abbr.hotkeys["brb"][0].comment, "old", "目标键原样保留");

        // 目标已存在 + 当前键已配置 ⇒ 目标键被覆盖
        let mut abbr = keymap(3, "", "Abbr", true);
        abbr.hotkeys
            .insert("btw".to_string(), vec![configured.clone()]);
        abbr.hotkeys
            .insert("brb".to_string(), vec![action(0, false, "old")]);
        assert_eq!(change_hotkey(&mut abbr, "btw", "brb"), "brb");
        assert_eq!(abbr.hotkeys["brb"][0].comment, "mine");
        assert!(!abbr.hotkeys.contains_key("btw"));

        // old 不存在 ⇒ 原样返回 new 且不改模型
        assert_eq!(change_hotkey(&mut abbr, "zzz", "brb"), "brb");
        assert_eq!(abbr.hotkeys.len(), 1);

        // old == new 且已配置 ⇒ 不动
        assert_eq!(change_hotkey(&mut abbr, "brb", "brb"), "brb");
        assert_eq!(abbr.hotkeys["brb"][0].comment, "mine");
    }
}
