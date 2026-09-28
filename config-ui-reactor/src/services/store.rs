//! 配置保存链路与动作级共享逻辑。
//!
//! 严格复刻 `config-ui-avalonia/Services/ConfigSaver.cs`（其本身复刻旧 Vue 侧 `store/config.ts`）：
//!
//! | 函数 | 契约 |
//! |---|---|
//! | [`parse_keyboard_layout`] | 布局每行拆为按键列表，除 `singlePress` 外均加 `*` 前缀；鼠标类 keymap（hotkey 含 `button`）追加缺失的鼠标按钮行 |
//! | [`clean_for_save`] | 保存前清洗：剔除 `is_empty` 动作；自定义 keymap（`id > 4`）额外剔除不在布局键集合内的热键。**在副本上执行，绝不动用户正在编辑的内存模型** |
//! | [`change_abbr_enable`] | 依据 CapsLock 命令/缩写动作回填 Command / Abbreviation 两个 keymap 的 `enable` |
//! | [`normalize_key_name`] | 把 `bs`/`esc`/`del` 等简写规范化为 AHK 键名（大小写不敏感，含 `*` 前缀变体）|
//!
//! ⚠️ 与 C# 的唯一差异（有意）：热键表在 Rust 侧用 `BTreeMap`（有序）而非哈希表，
//! 使 PUT 载荷**逐字节可复现**；Go 侧反序列化到 `map` 且生成期迭代顺序本就随机，语义等价。

use std::collections::{BTreeMap, HashSet};

use crate::models::{Action, Config, Keymap};

/// 鼠标类 keymap 需要补齐的按钮（对齐 C# 字面量顺序）。
const MOUSE_BUTTONS: [&str; 7] = [
    "*LButton",
    "*MButton",
    "*RButton",
    "*WheelUp",
    "*WheelDown",
    "*XButton1",
    "*XButton2",
];

/// 内置 keymap 的 id 上界：`id > 4` 视为「自定义 keymap」（需按键集合过滤）。
const BUILTIN_KEYMAP_MAX_ID: i32 = 4;

/// 复刻 `parseKeyboardLayout(layout, keymapHotkey)`。
pub fn parse_keyboard_layout(layout: &str, keymap_hotkey: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();

    for raw_line in layout.split('\n') {
        if raw_line.trim().is_empty() {
            continue;
        }
        let keys: Vec<String> = raw_line
            .split_whitespace()
            .filter(|key| !key.trim().is_empty())
            .map(|key| {
                if key == "singlePress" {
                    key.to_string()
                } else {
                    format!("*{key}")
                }
            })
            .collect();
        if !keys.is_empty() {
            rows.push(keys);
        }
    }

    if keymap_hotkey.to_lowercase().contains("button") {
        let flat: HashSet<&str> = rows.iter().flatten().map(String::as_str).collect();
        let missing: Vec<String> = MOUSE_BUTTONS
            .iter()
            .filter(|button| !flat.contains(**button))
            .map(|button| (*button).to_string())
            .collect();
        if !missing.is_empty() {
            rows.push(missing);
        }
    }

    rows
}

/// 复刻 `_saveConfig` 的空键清洗：返回**可直接 PUT 的副本**（不动入参）。
pub fn clean_for_save(live: &Config) -> Config {
    // 结构深拷贝（对应 C# 的 JSON 往返；`is_empty`/`is_new` 本就不参与序列化）
    let mut payload = live.clone();

    for (live_keymap, payload_keymap) in live.keymaps.iter().zip(payload.keymaps.iter_mut()) {
        let layout_rows = parse_keyboard_layout(&live.options.keyboard_layout, &live_keymap.hotkey);
        let key_set: HashSet<&str> = layout_rows.iter().flatten().map(String::as_str).collect();

        let is_normal_keymap = live_keymap.id > BUILTIN_KEYMAP_MAX_ID;
        let mut keep: BTreeMap<String, Vec<Action>> = BTreeMap::new();

        for (hotkey, live_actions) in &live_keymap.hotkeys {
            let surviving: Vec<Action> = payload_keymap
                .hotkeys
                .get(hotkey)
                .map(|payload_actions| {
                    // 深拷贝保持列表顺序与数量 ⇒ 按下标对齐即可还原 is_empty 判定
                    let count = live_actions.len().min(payload_actions.len());
                    (0..count)
                        .filter(|index| !live_actions[*index].is_empty)
                        .map(|index| payload_actions[index].clone())
                        .collect()
                })
                .unwrap_or_default();

            if !surviving.is_empty() && (!is_normal_keymap || key_set.contains(hotkey.as_str())) {
                keep.insert(hotkey.clone(), surviving);
            }
        }

        payload_keymap.hotkeys = keep;
    }

    payload
}

/// 复刻 `changeAbbrEnable`：按 CapsLock 命令（typeId=9,valueId=6）/缩写（=5）动作
/// 回填倒数第 3（Command）与倒数第 2（Abbreviation）个 keymap 的 `enable`。
pub fn change_abbr_enable(config: &mut Config) {
    if config.keymaps.len() < 3 {
        return;
    }
    let caps_index = config.keymaps.len() - 3;
    let seem_index = config.keymaps.len() - 2;

    let mut caps_enable = false;
    let mut seem_enable = false;

    for keymap in config.keymaps.iter().filter(|keymap| keymap.enable) {
        // 不遍历缩写、设置（Vue: id 2~4 跳过）
        if (2..=4).contains(&keymap.id) {
            continue;
        }
        if caps_enable && seem_enable {
            break;
        }

        for actions in keymap.hotkeys.values() {
            for action in actions {
                if action.type_id == 9 && action.value_id == 6 {
                    caps_enable = true;
                    continue;
                }
                if action.type_id == 9 && action.value_id == 5 {
                    seem_enable = true;
                }
            }
        }
    }

    config.keymaps[caps_index].enable = caps_enable;
    config.keymaps[seem_index].enable = seem_enable;
}

/// 复刻 `normalizeKeyName`（大小写不敏感，含 `*` 前缀变体）。
pub fn normalize_key_name(hotkey: &str) -> String {
    fn base(key: &str) -> Option<&'static str> {
        match key.to_ascii_lowercase().as_str() {
            "esc" => Some("Escape"),
            "bs" => Some("Backspace"),
            "del" => Some("Delete"),
            "ins" => Some("Insert"),
            "lctrl" => Some("LControl"),
            "rctrl" => Some("RControl"),
            _ => None,
        }
    }

    match hotkey.strip_prefix('*') {
        Some(rest) => match base(rest) {
            Some(mapped) => format!("*{mapped}"),
            None => hotkey.to_string(),
        },
        None => match base(hotkey) {
            Some(mapped) => mapped.to_string(),
            None => hotkey.to_string(),
        },
    }
}

/// 便捷构造：某 keymap 的合法键集合（大小写敏感，与 JS `Set.has` 一致）。
pub fn keymap_key_set(layout: &str, keymap_hotkey: &str) -> HashSet<String> {
    parse_keyboard_layout(layout, keymap_hotkey)
        .into_iter()
        .flatten()
        .collect()
}

/// 内部：构造一个带热键的 keymap（测试与调用方共用）。
pub fn keymap_with_hotkeys(
    id: i32,
    hotkey: &str,
    hotkeys: BTreeMap<String, Vec<Action>>,
) -> Keymap {
    Keymap {
        id,
        hotkey: hotkey.to_string(),
        hotkeys,
        enable: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Options;

    fn action(type_id: i32, value_id: i32, is_empty: bool) -> Action {
        Action {
            type_id,
            value_id,
            is_empty,
            ..Default::default()
        }
    }

    #[test]
    fn layout_rows_get_star_prefix_except_single_press() {
        let rows = parse_keyboard_layout("q w e\nsinglePress\n\n a s d ", "f");
        assert_eq!(rows.len(), 3, "空行必须跳过");
        assert_eq!(rows[0], vec!["*q", "*w", "*e"]);
        assert_eq!(rows[1], vec!["singlePress"]);
        assert_eq!(rows[2], vec!["*a", "*s", "*d"]);
    }

    #[test]
    fn mouse_keymap_appends_missing_buttons_once() {
        // 布局里写的是**裸键名**，`*` 由解析器添加
        let rows = parse_keyboard_layout("q w\nLButton", "button");
        // 末行补齐除已存在的 *LButton 之外的全部鼠标按钮
        let last = rows.last().unwrap();
        assert_eq!(last.len(), 6);
        assert!(!last.contains(&"*LButton".to_string()));
        assert!(last.contains(&"*WheelUp".to_string()));

        // 全部齐全时不再追加行
        let full = "LButton MButton RButton WheelUp WheelDown XButton1 XButton2";
        let rows = parse_keyboard_layout(full, "button");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 7);

        // 裸名已带 `*` 会被二次加前缀（与 C# 行为一致，勿「顺手修复」）
        let doubled = parse_keyboard_layout("*LButton", "button");
        assert!(doubled[0].contains(&"**LButton".to_string()));
    }

    #[test]
    fn hotkey_containing_button_is_case_insensitive() {
        let rows = parse_keyboard_layout("q", "myButtonGroup");
        assert_eq!(rows.len(), 2, "含 button（忽略大小写）应追加鼠标行");
    }

    #[test]
    fn clean_for_save_drops_empty_actions() {
        let mut hotkeys = BTreeMap::new();
        hotkeys.insert(
            "*q".to_string(),
            vec![action(1, 0, false), action(2, 0, true), action(3, 0, false)],
        );
        let mut config = Config {
            options: Options {
                keyboard_layout: "q".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        config.keymaps.push(keymap_with_hotkeys(1, "f", hotkeys));

        let payload = clean_for_save(&config);
        let kept = payload.keymaps[0].hotkeys.get("*q").unwrap();
        assert_eq!(kept.len(), 2, "isEmpty 动作必须被剔除");
        assert_eq!(kept[0].type_id, 1);
        assert_eq!(kept[1].type_id, 3);

        // 原模型不受污染
        assert_eq!(config.keymaps[0].hotkeys.get("*q").unwrap().len(), 3);
    }

    #[test]
    fn clean_for_save_filters_custom_keymap_by_layout_keys() {
        let mut hotkeys = BTreeMap::new();
        hotkeys.insert("*q".to_string(), vec![action(1, 0, false)]);
        hotkeys.insert("*zzz".to_string(), vec![action(1, 0, false)]);

        let mut config = Config {
            options: Options {
                keyboard_layout: "q w e".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        // id = 9 > 4 ⇒ 自定义 keymap，需按键集合过滤
        config
            .keymaps
            .push(keymap_with_hotkeys(9, "custom", hotkeys));

        let payload = clean_for_save(&config);
        let kept = &payload.keymaps[0].hotkeys;
        assert!(kept.contains_key("*q"), "布局内键应保留");
        assert!(!kept.contains_key("*zzz"), "布局外键应被剔除");
    }

    #[test]
    fn clean_for_save_keeps_builtin_keymaps_regardless_of_layout() {
        let mut hotkeys = BTreeMap::new();
        hotkeys.insert("*zzz".to_string(), vec![action(1, 0, false)]);

        let mut config = Config {
            options: Options {
                keyboard_layout: "q".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        // id = 2 ≤ 4 ⇒ 内置 keymap，不做键集合过滤
        config.keymaps.push(keymap_with_hotkeys(2, "abbr", hotkeys));

        let payload = clean_for_save(&config);
        assert!(payload.keymaps[0].hotkeys.contains_key("*zzz"));
    }

    #[test]
    fn clean_for_save_drops_hotkey_when_all_actions_empty() {
        let mut hotkeys = BTreeMap::new();
        hotkeys.insert("*q".to_string(), vec![action(1, 0, true)]);

        let mut config = Config {
            options: Options {
                keyboard_layout: "q".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        config.keymaps.push(keymap_with_hotkeys(1, "f", hotkeys));

        let payload = clean_for_save(&config);
        assert!(
            payload.keymaps[0].hotkeys.is_empty(),
            "全部动作被剔除后，该热键整体消失"
        );
    }

    /// 真实形态：`[F模式(id1), J模式(id5), Command(id2), Abbreviation(id3), Options(id4)]`
    /// ⇒ 倒数第 3（index 2）= Command，倒数第 2（index 3）= Abbreviation。全部初始 `enable = false`。
    fn config_with_five_keymaps() -> Config {
        let mut config = Config::default();
        for (id, hotkey) in [(1, "f"), (5, "j"), (2, "cmd"), (3, "abbr"), (4, "options")] {
            let mut keymap = keymap_with_hotkeys(id, hotkey, BTreeMap::new());
            keymap.enable = false;
            config.keymaps.push(keymap);
        }
        config
    }

    const COMMAND_INDEX: usize = 2;
    const ABBREVIATION_INDEX: usize = 3;

    fn hotkeys_with(actions: Vec<Action>) -> BTreeMap<String, Vec<Action>> {
        let mut hotkeys = BTreeMap::new();
        hotkeys.insert("*q".to_string(), actions);
        hotkeys
    }

    #[test]
    fn change_abbr_enable_marks_command_and_abbreviation() {
        let mut config = config_with_five_keymaps();
        // 源：内置模式页（id=1，不在 2..=4 内）且已启用
        config.keymaps[0].hotkeys = hotkeys_with(vec![action(9, 6, false), action(9, 5, false)]);
        config.keymaps[0].enable = true;

        change_abbr_enable(&mut config);

        assert!(config.keymaps[COMMAND_INDEX].enable, "Command 应被启用");
        assert!(
            config.keymaps[ABBREVIATION_INDEX].enable,
            "Abbreviation 应被启用"
        );
    }

    #[test]
    fn change_abbr_enable_only_marks_triggered_slot() {
        let mut config = config_with_five_keymaps();
        // 只有缩写动作（typeId=9,valueId=5）：Command 不应被启用
        config.keymaps[0].hotkeys = hotkeys_with(vec![action(9, 5, false)]);
        config.keymaps[0].enable = true;

        change_abbr_enable(&mut config);

        assert!(
            !config.keymaps[COMMAND_INDEX].enable,
            "无 CapsLock 命令则 Command 保持关闭"
        );
        assert!(
            config.keymaps[ABBREVIATION_INDEX].enable,
            "有缩写动作则应启用"
        );
    }

    #[test]
    fn change_abbr_enable_skips_id_2_to_4_sources() {
        let mut config = config_with_five_keymaps();
        // 唯一带 CapsLock 命令的 keymap 是 id=4（2..=4 内）⇒ 不参与判定
        config.keymaps[4].hotkeys = hotkeys_with(vec![action(9, 6, false)]);
        config.keymaps[4].enable = true;

        change_abbr_enable(&mut config);

        assert!(!config.keymaps[COMMAND_INDEX].enable);
        assert!(!config.keymaps[ABBREVIATION_INDEX].enable);
    }

    #[test]
    fn change_abbr_enable_ignores_disabled_keymaps() {
        let mut config = config_with_five_keymaps();
        config.keymaps[0].hotkeys = hotkeys_with(vec![action(9, 6, false)]);
        config.keymaps[0].enable = false; // 未启用 ⇒ 不计入判定

        change_abbr_enable(&mut config);

        assert!(!config.keymaps[COMMAND_INDEX].enable);
        assert!(!config.keymaps[ABBREVIATION_INDEX].enable);
    }

    #[test]
    fn change_abbr_enable_noop_when_less_than_three_keymaps() {
        let mut config = Config::default();
        config
            .keymaps
            .push(keymap_with_hotkeys(1, "f", BTreeMap::new()));
        change_abbr_enable(&mut config); // 不应 panic
        assert!(config.keymaps[0].enable);
    }

    #[test]
    fn normalize_key_name_covers_case_and_star_variants() {
        assert_eq!(normalize_key_name("esc"), "Escape");
        assert_eq!(normalize_key_name("ESC"), "Escape");
        assert_eq!(normalize_key_name("*esc"), "*Escape");
        assert_eq!(normalize_key_name("bs"), "Backspace");
        assert_eq!(normalize_key_name("del"), "Delete");
        assert_eq!(normalize_key_name("ins"), "Insert");
        assert_eq!(normalize_key_name("lctrl"), "LControl");
        assert_eq!(normalize_key_name("rctrl"), "RControl");
        assert_eq!(normalize_key_name("*rctrl"), "*RControl");
        // 未登记项原样返回（含大小写）
        assert_eq!(normalize_key_name("F1"), "F1");
        assert_eq!(normalize_key_name("*a"), "*a");
    }

    #[test]
    fn keymap_key_set_is_case_sensitive() {
        let set = keymap_key_set("q w", "f");
        assert!(set.contains("*q"));
        assert!(!set.contains("*Q"), "大小写敏感：*Q 不在集合内");
    }
}
