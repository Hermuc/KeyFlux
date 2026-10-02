//! 动作编辑的**纯逻辑**（零 UI 依赖）：类型目录、切类型清字段、校验、单选目录、各类型写入语义。
//!
//! 逐项复刻 `config-ui-avalonia/ViewModels/ActionEditorViewModel.cs`（598 行）：
//! `ActionTypeOption` / `WindowGroupOption` / `BuildTypeOptions` / `OnSelectedTypeChanged` /
//! `EvaluateWinTitleError` / `RadioCatalog` / `SelectRadio` / `RemapEditorVm` / `SendKeysEditorVm` /
//! `AhkCodeEditorVm` 的**可判定部分**。
//!
//! UI 侧只负责「把这里的产物摆成控件」（见 `ui::action_editor`）。

use crate::models::{Action, Config, PluginListResponse};
use crate::services::i18n;

/// 动作类型下拉项（`label` 为文案键，展示时经 i18n 翻译）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionTypeOption {
    pub id: i32,
    pub label_key: &'static str,
}

impl ActionTypeOption {
    /// 展示文案（当前语言）。
    pub fn label(self) -> String {
        i18n::t(self.label_key)
    }
}

/// 窗口分组下拉项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowGroupOption {
    pub id: i32,
    pub name: String,
}

/// 单选项（`label_key` 为文案键；选中时同时把备注写为该键，复刻 `changeActionComment`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioItem {
    pub value_id: i32,
    pub label_key: &'static str,
    pub hide_in_abbr: bool,
}

impl RadioItem {
    pub fn label(self) -> String {
        i18n::t(self.label_key)
    }
}

const fn ri(value_id: i32, label_key: &'static str, hide_in_abbr: bool) -> RadioItem {
    RadioItem {
        value_id,
        label_key,
        hide_in_abbr,
    }
}

/// 动作类型目录（复刻 `Action.vue` 的 `actionTypes`；第三列 = `hideInAbbr`）。
pub const ACTION_TYPES: [(i32, &str, bool); 10] = [
    (0, "200", false),
    (1, "201", false),
    (2, "202", false),
    (3, "203", false),
    (4, "204", true),
    (5, "205", true),
    (6, "206", false),
    (7, "207", false),
    (8, "208", false),
    (9, "209", false),
];

/// 动作类型选项（缩写语境过滤掉 `hideInAbbr` 的 4/5）。
pub fn type_options(is_abbr: bool) -> Vec<ActionTypeOption> {
    ACTION_TYPES
        .iter()
        .filter(|(_, _, hide_in_abbr)| !is_abbr || !hide_in_abbr)
        .map(|(id, label_key, _)| ActionTypeOption { id: *id, label_key })
        .collect()
}

/// 窗口分组选项（复刻 `windowGroups.filter(x => x.id >= 0)`）。
pub fn window_group_options(config: &Config) -> Vec<WindowGroupOption> {
    config
        .options
        .window_groups
        .iter()
        .filter(|group| group.id >= 0)
        .map(|group| WindowGroupOption {
            id: group.id,
            name: group.name.clone(),
        })
        .collect()
}

/// 切类型产生的变更信息（供 `change_abbr_enable` 联动判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeChange {
    pub old_type: i32,
    pub old_value: i32,
    pub new_type: i32,
}

/// 切换动作类型（复刻 `OnSelectedTypeChanged`）：
/// **清除除 `windowGroupId`/`typeId` 外的全部字段**；类型 0 置 `is_empty`。
///
/// 返回 `None` 表示类型未变化（复刻 C# 的提前返回，不产生任何副作用）。
pub fn apply_type_change(action: &mut Action, new_type_id: i32) -> Option<TypeChange> {
    if action.type_id == new_type_id {
        return None;
    }

    let change = TypeChange {
        old_type: action.type_id,
        old_value: action.value_id,
        new_type: new_type_id,
    };
    let keep_group = action.window_group_id;

    action.comment.clear();
    action.hotkey.clear();
    action.keys_to_send.clear();
    action.remap_to_key.clear();
    action.value_id = 0;
    action.win_title.clear();
    action.target.clear();
    action.args.clear();
    action.working_dir.clear();
    action.run_as_admin = false;
    action.run_in_background = false;
    action.detect_hidden_window = false;
    action.ahk_code.clear();
    action.action_id.clear();
    action.window_group_id = keep_group;
    action.type_id = new_type_id;
    action.is_empty = new_type_id == 0;

    Some(change)
}

/// 窗口标题校验（复刻 `EvaluateWinTitleError` / `winTitleRules`）：
/// 裸写 `xxx.exe` 会被当窗口标题匹配而**永远失败** ⇒ 提示改用 `ahk_exe`。
///
/// 放行：空 / 以 `ahk_` 或 `ahk-expression:` 开头 / 含 `" ahk_"` 组合串。
pub fn evaluate_win_title_error(win_title: &str) -> Option<String> {
    if win_title.is_empty() {
        return None;
    }
    if win_title.starts_with("ahk_")
        || win_title.starts_with("ahk-expression:")
        || win_title.contains(" ahk_")
    {
        return None;
    }
    if win_title.to_ascii_lowercase().ends_with(".exe") {
        return Some(i18n::t("301err"));
    }
    None
}

/// 类型 1 的 `isEmpty` 重算（复刻 `watchEffect`：`!winTitle && !target`）。
pub fn refresh_empty_activate_or_run(action: &mut Action) {
    action.is_empty = action.win_title.is_empty() && action.target.is_empty();
}

/// 类型 5 候选重映射键（复刻 `RemapKey.vue` 的 `items`，允许自由输入）。
pub static REMAP_ITEMS: &[&str] = &[
    "Up",
    "Down",
    "Left",
    "Right",
    "Home",
    "End",
    "Backspace",
    "Delete",
    "Space",
    "Tab",
    "Enter",
    "Escape",
    "Insert",
    "CapsLock",
    "AppsKey",
    "PgUp",
    "PgDn",
    "LWin",
    "RWin",
    "LControl",
    "RControl",
    "LAlt",
    "RAlt",
    "LShift",
    "RShift",
    "PrintScreen",
    "Volume_Mute",
    "Volume_Up",
    "Volume_Down",
    "Media_Next",
    "Media_Prev",
    "Media_Stop",
    "Media_Play_Pause",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
];

/// 类型 8 的示例代码（复刻 `BuiltinFunction.vue` 的 `items`）。
pub static AHK_EXAMPLES: &[&str] = &[
    "CenterAndResizeWindow(1600, 1000)",
    "ProcessExistSendKeyOrRun(\"WeChat.exe\", \"^!w\", \"shortcuts\\微信.lnk\")",
];

/// 写入重映射键（复刻 `RemapEditorVm.RemapToKey` setter）：
/// 备注改写为「重映射为 X」（i18n `401`），`isEmpty = !remapToKey`。
pub fn apply_remap(action: &mut Action, value: &str) {
    action.remap_to_key = value.to_string();
    action.comment = if value.is_empty() {
        String::new()
    } else {
        format!("{} {value}", i18n::t("401"))
    };
    action.is_empty = value.is_empty();
}

/// 写入「输入按键或文本」（复刻 `SendKeysEditorVm.KeysToSend` setter）：`isEmpty = !keysToSend`。
pub fn apply_keys_to_send(action: &mut Action, value: &str) {
    action.keys_to_send = value.to_string();
    action.is_empty = value.is_empty();
}

/// 写入自定义函数代码（复刻 `AhkCodeEditorVm.AhkCode` setter）：`isEmpty = !ahkCode`。
pub fn apply_ahk_code(action: &mut Action, value: &str) {
    action.ahk_code = value.to_string();
    action.is_empty = value.is_empty();
}

/// 选中单选项（复刻 `RadioGroupEditorVm.SelectRadio`）：
/// 写 `valueId` + **把备注写为该选项的文案键** + `isEmpty = false`；返回 `(旧值, 新值)`。
pub fn select_radio(action: &mut Action, item: &RadioItem) -> (i32, i32) {
    let old_value = action.value_id;
    action.value_id = item.value_id;
    action.comment = item.label_key.to_string();
    // P7b: 内置单选与插件动作互斥 —— 选中内置项即清插件动作绑定
    action.action_id.clear();
    action.is_empty = false;
    (old_value, item.value_id)
}

/// P7b：选中插件动作（`full_id = "<pluginId>.<actionId>"`）。
/// 双字段过渡：`value_id` 写 9（「插件动作」子类型标记，旧版本可识别），渲染以
/// `action_id` 优先。返回旧 value_id 供缩写联动。
pub fn select_plugin_action(action: &mut Action, full_id: &str) -> i32 {
    let old_value = action.value_id;
    action.action_id = full_id.to_string();
    action.value_id = 9;
    action.comment = full_id.to_string();
    action.is_empty = false;
    old_value
}

// ---------------------------------------------------------------- 单选目录

/// 类型 2「系统」两行。
static SYSTEM_GROUPS: [&[RadioItem]; 2] = [
    &[
        ri(1, "17", false),
        ri(2, "18", false),
        ri(3, "19", false),
        ri(4, "20", false),
        ri(5, "21", false),
        ri(9, "2401", false),
        ri(6, "22", false),
    ],
    &[
        ri(7, "23", false),
        ri(8, "24", false),
        ri(10, "2402", false),
    ],
];

/// 类型 3「窗口」两行。
static WINDOW_GROUPS: [&[RadioItem]; 2] = [
    &[
        ri(1, "1", false),
        ri(2, "2", false),
        ri(3, "3", false),
        ri(4, "4", true),
        ri(5, "5", false),
        ri(6, "6", false),
        ri(7, "7", false),
        ri(15, "8", false),
        ri(16, "9", false),
    ],
    &[
        ri(8, "10", false),
        ri(9, "11", false),
        ri(10, "12", false),
        ri(11, "13", false),
        ri(12, "14", false),
        ri(13, "15", false),
        ri(14, "16", true),
    ],
];

/// 类型 4「鼠标」四行。
static MOUSE_GROUPS: [&[RadioItem]; 4] = [
    &[
        ri(1, "25", false),
        ri(2, "26", false),
        ri(3, "27", false),
        ri(4, "28", false),
    ],
    &[
        ri(5, "29", false),
        ri(6, "30", false),
        ri(7, "31", false),
        ri(8, "32", false),
    ],
    &[
        ri(9, "33", false),
        ri(10, "34", false),
        ri(11, "35", false),
        ri(12, "36", false),
    ],
    &[ri(13, "37", false)],
];

/// 类型 7「文本」四行。
static TEXT_GROUPS: [&[RadioItem]; 4] = [
    &[
        ri(1, "38", true),
        ri(2, "39", true),
        ri(3, "40", true),
        ri(4, "41", true),
        ri(5, "42", true),
        ri(6, "43", true),
        ri(7, "44", true),
        ri(8, "45", true),
    ],
    &[
        ri(9, "46", true),
        ri(10, "47", true),
        ri(11, "48", true),
        ri(12, "49", true),
        ri(13, "50", true),
        ri(14, "51", true),
        ri(15, "52", true),
        ri(16, "53", true),
    ],
    &[
        ri(17, "54", true),
        ri(33, "55", true),
        ri(18, "56", true),
        ri(19, "57", true),
        ri(30, "58", true),
        ri(31, "59", true),
        ri(32, "60", true),
        ri(29, "61", false),
    ],
    &[
        ri(20, "62", true),
        ri(21, "63", true),
        ri(22, "64", true),
        ri(23, "65", true),
        ri(24, "66", true),
        ri(25, "67", true),
        ri(26, "68", true),
        ri(27, "69", true),
        ri(28, "70", true),
    ],
];

/// 类型 9「KeyFlux」内置快路径两行（红线不动）。
/// P7b：原第三组 valueID 9（旧式插件动作薄壳）已删除 —— 插件动作改由
/// [`plugin_action_groups`] 从目录 `provides.actions[]` 动态聚合。
static KEYFLUX_GROUPS: [&[RadioItem]; 2] = [
    &[
        ri(1, "71", true),
        ri(2, "72", false),
        ri(3, "73", false),
        ri(4, "74", false),
    ],
    &[
        ri(5, "75", true),
        ri(6, "76", true),
        ri(7, "77", false),
        ri(8, "78", true),
    ],
];

/// 取某类型的单选分组目录（复刻 `RadioCatalog.GroupsFor`）。
pub fn radio_groups(type_id: i32) -> &'static [&'static [RadioItem]] {
    match type_id {
        2 => &SYSTEM_GROUPS,
        3 => &WINDOW_GROUPS,
        4 => &MOUSE_GROUPS,
        7 => &TEXT_GROUPS,
        9 => &KEYFLUX_GROUPS,
        _ => &[],
    }
}

/// 两两一行布局（复刻 `BuildRows`）：先按缩写语境过滤、丢弃空组，再**每行最多 2 组**。
pub fn radio_rows(type_id: i32, is_abbr: bool) -> Vec<Vec<Vec<RadioItem>>> {
    let built: Vec<Vec<RadioItem>> = radio_groups(type_id)
        .iter()
        .map(|group| {
            group
                .iter()
                .copied()
                .filter(|item| !is_abbr || !item.hide_in_abbr)
                .collect::<Vec<_>>()
        })
        .filter(|group| !group.is_empty())
        .collect();

    built.chunks(2).map(|pair| pair.to_vec()).collect()
}

/// 该类型是否有单选编辑器（复刻 `RebuildEditor` 的分发：2/3/4/7/9）。
pub fn has_radio_editor(type_id: i32) -> bool {
    matches!(type_id, 2 | 3 | 4 | 7 | 9)
}

// ---------------------------------------------------------------- 插件动作（P7b）

/// 插件动作选项（type 9 动态组；`full_id = "<pluginId>.<actionId>"`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginActionItem {
    pub full_id: String,
    pub label: String,
}

/// 从插件目录聚合 type 9 的动态组：**每个声明了 `provides.actions` 的插件一组**
///（内置快路径 1-8 保持编译期直连，不经此处 —— 快路径红线）。
/// 目录未加载 / 无插件声明 ⇒ 空Vec（编辑器不渲染插件动作区）。
pub fn plugin_action_groups(catalog: Option<&PluginListResponse>) -> Vec<Vec<PluginActionItem>> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    catalog
        .plugins
        .iter()
        .filter_map(|manifest| {
            let actions = manifest.provides.as_ref()?.actions.as_slice();
            if actions.is_empty() {
                return None;
            }
            let items = actions
                .iter()
                .map(|action| PluginActionItem {
                    full_id: format!("{}.{}", manifest.id, action.id),
                    label: action.label.clone(),
                })
                .collect::<Vec<_>>();
            Some(items)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Options, WindowGroup};

    fn action_with(type_id: i32, value_id: i32, group_id: i32) -> Action {
        Action {
            type_id,
            value_id,
            window_group_id: group_id,
            comment: "旧备注".to_string(),
            hotkey: "esc".to_string(),
            keys_to_send: "abc".to_string(),
            remap_to_key: "F1".to_string(),
            win_title: "标题".to_string(),
            target: "C:\\x.exe".to_string(),
            args: "-a".to_string(),
            working_dir: "C:\\".to_string(),
            run_as_admin: true,
            run_in_background: true,
            detect_hidden_window: true,
            ahk_code: "code".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn type_options_hide_four_and_five_in_abbr() {
        let normal = type_options(false);
        assert_eq!(normal.len(), 10);
        assert_eq!(normal[0].id, 0);
        assert_eq!(normal[9].id, 9);

        let abbr = type_options(true);
        let ids: Vec<i32> = abbr.iter().map(|o| o.id).collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 6, 7, 8, 9], "缩写语境隐藏 4/5");
    }

    #[test]
    fn window_group_options_skip_negative_ids() {
        let config = Config {
            options: Options {
                window_groups: vec![
                    WindowGroup {
                        id: 0,
                        name: "默认".to_string(),
                        ..Default::default()
                    },
                    WindowGroup {
                        id: -1,
                        name: "内置".to_string(),
                        ..Default::default()
                    },
                    WindowGroup {
                        id: 2,
                        name: "微信".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        let options = window_group_options(&config);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].id, 0);
        assert_eq!(options[1].name, "微信");
    }

    #[test]
    fn type_change_clears_everything_but_group_and_type() {
        let mut action = action_with(1, 5, 7);
        let change = apply_type_change(&mut action, 6).expect("类型变化");

        assert_eq!(
            change,
            TypeChange {
                old_type: 1,
                old_value: 5,
                new_type: 6
            }
        );
        assert_eq!(action.type_id, 6);
        assert_eq!(action.window_group_id, 7, "窗口分组必须保留");
        assert_eq!(action.value_id, 0);
        assert!(action.comment.is_empty());
        assert!(action.hotkey.is_empty());
        assert!(action.keys_to_send.is_empty());
        assert!(action.remap_to_key.is_empty());
        assert!(action.win_title.is_empty());
        assert!(action.target.is_empty());
        assert!(action.args.is_empty());
        assert!(action.working_dir.is_empty());
        assert!(action.ahk_code.is_empty());
        assert!(!action.run_as_admin);
        assert!(!action.run_in_background);
        assert!(!action.detect_hidden_window);
        assert!(!action.is_empty, "类型 6 非空");
    }

    #[test]
    fn type_change_to_zero_marks_empty_and_same_type_is_noop() {
        let mut action = action_with(1, 5, 0);
        apply_type_change(&mut action, 0);
        assert!(action.is_empty, "类型 0（未配置）置 isEmpty");

        let mut action = action_with(3, 2, 0);
        assert_eq!(
            apply_type_change(&mut action, 3),
            None,
            "同类型不产生副作用"
        );
        assert_eq!(action.value_id, 2, "同类型不改动字段");
        assert_eq!(action.comment, "旧备注");
    }

    #[test]
    fn win_title_error_follows_legacy_rules() {
        assert_eq!(evaluate_win_title_error(""), None);
        assert_eq!(evaluate_win_title_error("ahk_exe wechat.exe"), None);
        assert_eq!(evaluate_win_title_error("ahk-expression: ..."), None);
        assert_eq!(evaluate_win_title_error("标题 ahk_exe wechat.exe"), None);
        // 裸写 .exe ⇒ 报错（永远匹配失败）
        assert!(evaluate_win_title_error("wechat.exe").is_some());
        assert!(
            evaluate_win_title_error("WeChat.EXE").is_some(),
            "大小写不敏感"
        );
        // 非 .exe 结尾的普通标题放行
        assert_eq!(evaluate_win_title_error("微信"), None);
    }

    #[test]
    fn activate_or_run_empty_requires_both_fields_blank() {
        let mut action = action_with(1, 0, 0);
        refresh_empty_activate_or_run(&mut action);
        assert!(!action.is_empty);

        action.win_title.clear();
        refresh_empty_activate_or_run(&mut action);
        assert!(!action.is_empty, "仍有 target");

        action.target.clear();
        refresh_empty_activate_or_run(&mut action);
        assert!(action.is_empty, "两者皆空才 isEmpty");
    }

    #[test]
    fn radio_rows_pack_two_groups_per_row() {
        let rows = radio_rows(4, false);
        assert_eq!(rows.len(), 2, "4 组 ⇒ 2 行");
        assert_eq!(rows[0].len(), 2);
        assert_eq!(rows[1].len(), 2);
        assert_eq!(rows[0][0][0].value_id, 1);

        // P7b: 值 9 静态项移除 (子类型标记, 不可直选) —— 类型 9 仅剩 2 组 ⇒ 1 行;
        // 插件动作由 plugin_action_groups 动态拼接 (UI 层职责, 不在本目录)。
        let rows = radio_rows(9, false);
        assert_eq!(rows.len(), 1, "2 组 ⇒ 1 行 (P7b 值 9 静态项移除)");
        assert_eq!(rows[0].len(), 2);
    }

    #[test]
    fn radio_rows_filter_hidden_options_and_drop_empty_groups() {
        let rows = radio_rows(9, true);
        let flat: Vec<i32> = rows
            .iter()
            .flatten()
            .flatten()
            .map(|item| item.value_id)
            .collect();
        assert!(!flat.contains(&1), "值 1 标了 hideInAbbr");
        assert!(flat.contains(&2), "值 2 未隐藏");
        assert!(
            !flat.contains(&9),
            "P7b: 值 9 = 插件动作子类型标记, 不在静态目录"
        );
    }

    #[test]
    fn unknown_type_has_no_radio_catalog() {
        assert!(radio_groups(1).is_empty());
        assert!(radio_rows(1, false).is_empty());
        assert!(!has_radio_editor(1));
        for type_id in [2, 3, 4, 7, 9] {
            assert!(has_radio_editor(type_id));
            assert!(!radio_rows(type_id, false).is_empty());
        }
    }

    #[test]
    fn select_radio_writes_comment_and_clears_empty() {
        let mut action = Action {
            value_id: 3,
            comment: "旧".to_string(),
            is_empty: true,
            ..Default::default()
        };
        let item = RadioItem {
            value_id: 7,
            label_key: "23",
            hide_in_abbr: false,
        };
        let (old, new) = select_radio(&mut action, &item);
        assert_eq!((old, new), (3, 7));
        assert_eq!(action.value_id, 7);
        assert_eq!(action.comment, "23", "备注写成选项文案键");
        assert!(!action.is_empty);
    }

    #[test]
    fn remap_writes_comment_and_empty_flag() {
        let mut action = Action::default();
        apply_remap(&mut action, "F5");
        assert_eq!(action.remap_to_key, "F5");
        assert!(
            action.comment.ends_with("F5"),
            "备注含目标键: {}",
            action.comment
        );
        assert!(!action.is_empty);

        apply_remap(&mut action, "");
        assert!(action.comment.is_empty(), "清空后备注也清空");
        assert!(action.is_empty);
    }

    #[test]
    fn send_keys_and_ahk_code_track_empty() {
        let mut action = Action::default();
        apply_keys_to_send(&mut action, "^!w");
        assert_eq!(action.keys_to_send, "^!w");
        assert!(!action.is_empty);
        apply_keys_to_send(&mut action, "");
        assert!(action.is_empty);

        apply_ahk_code(&mut action, "CenterAndResizeWindow(1600, 1000)");
        assert!(!action.is_empty);
        apply_ahk_code(&mut action, "");
        assert!(action.is_empty);
    }

    #[test]
    fn catalogs_have_expected_scope() {
        assert_eq!(REMAP_ITEMS.len(), 55, "复刻 RemapKey.vue 的 items 数量");
        assert!(REMAP_ITEMS.contains(&"F12"));
        assert!(REMAP_ITEMS.contains(&"Media_Play_Pause"));
        assert_eq!(AHK_EXAMPLES.len(), 2);
        assert!(AHK_EXAMPLES[1].contains("ProcessExistSendKeyOrRun"));
    }
}
