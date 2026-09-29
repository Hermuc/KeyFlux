//! 注册计划（Oracle 机制的生成端）—— Go `generators/plan.go`（286 行）的同名移植。
//!
//! 计划是对"运行时应当注册什么"的**确定性**描述，输出 `plan.json`；它也是 P3 里
//! **第一个能端到端过对账闸门**的 Rust 生成器单元（`reference/*.plan.json`）。
//!
//! ⚠️ 三处"逐字节"要点（都由对账抓，不能靠直觉）：
//! 1. **Go 的 nil slice ⇒ JSON `null`，不是 `[]`** —— `keymaps` / `entries` / `windowGroups` /
//!    `matchTypes` 在无内容时都是 nil ⇒ 必须序列化成 `null`（用 `Option<Vec<_>>`）。
//!    注意 `abbr.capslock/semicolon` 与 `selectedAction.mappings` 在 Go 里被初始化成
//!    空切片 ⇒ 恒为 `[]`，**不能**也写成 null。
//! 2. **Go `json.MarshalIndent` 默认做 HTML 转义**：`<`/`>`/`&` → `\u003c`/`\u003e`/`\u0026`，
//!    另有 U+2028/U+2029。serde_json **不转义** ⇒ 必须后处理（语料里就有 `&&`：
//!    synthetic 的 `#HotIf 'WinActive("A") && GetKeyState("Shift")'`）。
//! 3. 字段顺序 = Go 结构体声明顺序（serde 按声明顺序序列化，逐字段 `rename` 对齐）。

use std::io;
use std::path::Path;

use serde::Serialize;

use crate::generator::behaviors::{self, Catalog};
use crate::generator::model::{Action, Config, MatchType, REMAP_KEY, WindowGroup};
use crate::generator::text::{contains_only_modifier, divide, to_ahk_func_arg};

/// Go `generators.PlanVersion`。
pub const PLAN_VERSION: i32 = 1;

/// Go `generators.selectedActionKeyCap`（与 `script.maxEntriesPerMapping` 同口径）。
/// `pub(crate)`：`actions::selected_action_code` 与 plan 投影共用同一口径。
pub(crate) const SELECTED_ACTION_KEY_CAP: usize = 9;

#[derive(Debug, Serialize)]
pub struct Plan {
    #[serde(rename = "planVersion")]
    pub plan_version: i32,
    pub keymaps: Option<Vec<PlanKeymap>>,
    pub abbr: PlanAbbr,
    #[serde(rename = "selectedAction")]
    pub selected_action: PlanSelectedAction,
    #[serde(rename = "windowGroups")]
    pub window_groups: Option<Vec<WindowGroup>>,
    #[serde(rename = "matchTypes")]
    pub match_types: Option<Vec<MatchType>>,
}

#[derive(Debug, Serialize)]
pub struct PlanKeymap {
    pub id: i32,
    pub name: String,
    pub hotkey: String,
    #[serde(rename = "parentID")]
    pub parent_id: i32,
    #[serde(rename = "delaySec")]
    pub delay_sec: String,
    #[serde(rename = "disableAt")]
    pub disable_at: String,
    pub entries: Option<Vec<PlanEntry>>,
}

#[derive(Debug, Serialize)]
pub struct PlanEntry {
    pub hotkey: String,
    #[serde(rename = "typeID")]
    pub type_id: i32,
    #[serde(rename = "valueID")]
    pub value_id: i32,
    #[serde(rename = "windowGroupID")]
    pub window_group_id: i32,
    #[serde(rename = "conditionType")]
    pub condition_type: i32,
    #[serde(rename = "winTitle")]
    pub win_title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

#[derive(Debug, Serialize)]
pub struct PlanAbbr {
    #[serde(rename = "capslockEnabled")]
    pub capslock_enabled: bool,
    #[serde(rename = "capslockKeys")]
    pub capslock_keys: String,
    pub capslock: Vec<PlanAbbrEntry>,
    #[serde(rename = "semicolonEnabled")]
    pub semicolon_enabled: bool,
    #[serde(rename = "semicolonKeys")]
    pub semicolon_keys: String,
    pub semicolon: Vec<PlanAbbrEntry>,
}

#[derive(Debug, Serialize)]
pub struct PlanAbbrEntry {
    pub abbr: String,
    pub actions: Vec<PlanEntry>,
}

#[derive(Debug, Serialize)]
pub struct PlanSelectedAction {
    pub hotkey: String,
    pub enable: bool,
    pub mappings: Vec<PlanSelectedMapping>,
}

#[derive(Debug, Serialize)]
pub struct PlanSelectedMapping {
    #[serde(rename = "matchType")]
    pub match_type: String,
    #[serde(rename = "matchValue")]
    pub match_value: String,
    pub entries: Vec<PlanSelectedEntry>,
}

#[derive(Debug, Serialize)]
pub struct PlanSelectedEntry {
    pub key: usize,
    pub behavior: String,
    pub action: String,
    #[serde(rename = "actionValue")]
    pub action_value: String,
    #[serde(rename = "workingDir")]
    pub working_dir: String,
    pub name: String,
}

/// Go `generators.BuildPlan`。调用方**必须**先执行 `Preprocess`（注入 `!f17`），
/// 与 `GenerateAHK` / `GenerateScripts` 路径保持一致。
pub fn build_plan(config: &mut Config, catalog: Option<&Catalog>) -> Plan {
    let window_groups = if config.options.window_groups.is_empty() {
        None
    } else {
        Some(config.options.window_groups.clone())
    };
    let match_types = if config.match_types.is_empty() {
        None
    } else {
        Some(config.match_types.clone())
    };
    Plan {
        plan_version: PLAN_VERSION,
        keymaps: plan_keymaps(config),
        abbr: plan_abbr(config),
        selected_action: plan_selected_action(config, catalog),
        window_groups,
        match_types,
    }
}

/// Go `generators.WritePlan`：2 空格缩进 + 末尾换行 + **Go 口径的 HTML 转义**。
pub fn write_plan(
    config: &mut Config,
    catalog: Option<&Catalog>,
    output_file: &Path,
) -> io::Result<()> {
    let plan = build_plan(config, catalog);
    let json = serde_json::to_string_pretty(&plan)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let json = escape_json_html(&json);
    std::fs::write(output_file, format!("{json}\n"))
}

/// 复刻 Go `encoding/json` 的默认 HTML 转义。
fn escape_json_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

fn plan_keymaps(config: &mut Config) -> Option<Vec<PlanKeymap>> {
    let mut result = Vec::new();
    for keymap in config.enabled_keymaps() {
        // 与 renderKeymap 一致：空白热键的模式不渲染
        if keymap.hotkey.trim().is_empty() {
            continue;
        }
        let hotkey = if contains_only_modifier(&keymap.hotkey) {
            "customHotkeys".to_string()
        } else {
            keymap.hotkey.clone()
        };
        result.push(PlanKeymap {
            id: keymap.id,
            name: keymap.name.clone(),
            hotkey,
            parent_id: keymap.parent_id,
            delay_sec: divide(keymap.delay as i64, 1000),
            disable_at: config.get_keymap_disable_at(keymap.id),
            entries: plan_entries(config, &keymap),
        });
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn plan_entries(
    config: &Config,
    keymap: &crate::generator::model::Keymap,
) -> Option<Vec<PlanEntry>> {
    let mut entries = Vec::new();
    for mut action in deterministic_sort(sort_hotkeys(&keymap.hotkeys)) {
        // 与 renderKeymap 一致：纯修饰键模式跳过 singlePress，其余拼接触发键
        if contains_only_modifier(&keymap.hotkey) {
            if action.hotkey == "singlePress" {
                continue;
            }
            action.hotkey = format!("{}{}", keymap.hotkey, action.hotkey);
        }
        // 与 ActionToHotkey 一致：未注册的 TypeID 不产生注册（TypeID 1..=9 全集）
        if !(1..=9).contains(&action.type_id) {
            continue;
        }
        entries.push(to_plan_entry(config, &action));
    }
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn plan_abbr(config: &Config) -> PlanAbbr {
    let capslock_enabled = config.capslock_abbr_enabled();
    let semicolon_enabled = config.semicolon_abbr_enabled();
    let mut result = PlanAbbr {
        capslock_enabled,
        capslock_keys: String::new(),
        capslock: Vec::new(),
        semicolon_enabled,
        semicolon_keys: String::new(),
        semicolon: Vec::new(),
    };
    if capslock_enabled {
        result.capslock_keys = config.capslock_abbr_keys();
        result.capslock = plan_abbr_entries(config, &config.capslock_abbr());
    }
    if semicolon_enabled {
        result.semicolon_keys = config.semicolon_abbr_keys();
        result.semicolon = plan_abbr_entries(config, &config.semicolon_abbr());
    }
    result
}

fn plan_abbr_entries(
    config: &Config,
    abbr_map: &std::collections::HashMap<String, Vec<Action>>,
) -> Vec<PlanAbbrEntry> {
    // 与 AbbrRegistryCode 一致：按缩写字典序，动作经 sortActions，跳过未注册 TypeID
    let mut abbr_list: Vec<(String, Vec<Action>)> = abbr_map
        .iter()
        .map(|(abbr, actions)| (abbr.clone(), sort_actions(actions)))
        .collect();
    abbr_list.sort_by(|a, b| a.0.cmp(&b.0));

    let mut result = Vec::with_capacity(abbr_list.len());
    for (abbr, actions) in abbr_list {
        let mut entry = PlanAbbrEntry {
            abbr,
            actions: Vec::new(),
        };
        for action in actions {
            if !(1..=9).contains(&action.type_id) {
                continue;
            }
            entry.actions.push(to_plan_entry(config, &action));
        }
        result.push(entry);
    }
    result
}

fn plan_selected_action(config: &Config, catalog: Option<&Catalog>) -> PlanSelectedAction {
    let mut result = PlanSelectedAction {
        hotkey: String::new(),
        enable: false,
        mappings: Vec::new(),
    };
    let Some(selected) = config.selected_action.as_ref() else {
        return result;
    };
    if !selected.enable || selected.hotkey.is_empty() {
        return result;
    }
    result.hotkey = selected.hotkey.clone();
    result.enable = selected.enable;

    for mapping in &selected.mappings {
        let mut plan_mapping = PlanSelectedMapping {
            match_type: mapping.match_type.clone(),
            match_value: mapping.match_value.clone(),
            entries: Vec::new(),
        };
        for (index, entry) in mapping.entries.iter().enumerate() {
            // 与 selectedActionCode 同口径：超 key cap 的 entry 不进计划
            if index + 1 > SELECTED_ACTION_KEY_CAP {
                continue;
            }
            let (action, action_value, working_dir) = behaviors::resolve_rule_action(
                catalog,
                &entry.behavior,
                &entry.action_value,
                &entry.working_dir,
            );
            plan_mapping.entries.push(PlanSelectedEntry {
                key: index + 1,
                behavior: entry.behavior.clone(),
                action,
                action_value,
                working_dir,
                name: behaviors::behavior_name(catalog, &entry.behavior),
            });
        }
        result.mappings.push(plan_mapping);
    }
    if result.mappings.is_empty() {
        // 零 mappings 与渲染器/AHK 端「空串不注册」口径统一
        result.hotkey = String::new();
    }
    result
}

fn to_plan_entry(config: &Config, action: &Action) -> PlanEntry {
    let (win_title, condition_type) = config.get_win_title(action);
    // 与 AbbrRegistryCode 一致：conditionType 5 的表达式去掉包裹的单引号
    let win_title = if condition_type == 5 {
        win_title.trim_matches('\'').to_string()
    } else {
        win_title
    };
    PlanEntry {
        hotkey: action.hotkey.clone(),
        type_id: action.type_id,
        value_id: action.value_id,
        window_group_id: action.window_group_id,
        condition_type,
        win_title,
        comment: action.comment.clone(),
    }
}

/// Go `generators.sortHotkeys`：摊平 `hotkey -> actions`，用 **非稳定**排序按
/// (TypeID, 热键字节长度, 热键字典序) 排序。热键名取自 `ToAHKFuncArg` 去掉首末引号。
///
/// `pub(crate)`：`actions::render_keymap` 复用（与 Go 同源，避免第二份实现漂移）。
pub(crate) fn sort_hotkeys(
    hotkey_map: &std::collections::HashMap<String, Vec<Action>>,
) -> Vec<Action> {
    let mut result: Vec<Action> = Vec::new();
    for (hotkey, actions) in hotkey_map {
        let stripped = strip_quotes(&to_ahk_func_arg(hotkey));
        for action in actions {
            let mut action = action.clone();
            action.hotkey = stripped.clone();
            result.push(action);
        }
    }
    // Rust `sort_unstable_by` 与 Go `sort.Slice` 都是非稳定排序（语料须规避并列）
    result.sort_unstable_by(|a, b| {
        a.type_id
            .cmp(&b.type_id)
            .then_with(|| a.hotkey.len().cmp(&b.hotkey.len()))
            .then_with(|| a.hotkey.cmp(&b.hotkey))
    });
    result
}

/// Go `substr(s, 1, -1)`：按 rune 去掉首末各一个字符。
fn strip_quotes(text: &str) -> String {
    crate::generator::text::substr(text, 1, -1)
}

/// Go `generators.deterministicSort`：在 `sortHotkeys` 基础上追加 windowGroupID 兜底
/// （`sort.SliceStable` ⇒ Rust 稳定 `sort_by`）。
fn deterministic_sort(mut actions: Vec<Action>) -> Vec<Action> {
    actions.sort_by(|a, b| {
        a.type_id
            .cmp(&b.type_id)
            .then_with(|| a.hotkey.len().cmp(&b.hotkey.len()))
            .then_with(|| a.hotkey.cmp(&b.hotkey))
            .then_with(|| a.window_group_id.cmp(&b.window_group_id))
    });
    actions
}

/// Go `generators.sortActions`：把 `window_group_id == 0`（默认生效、优先级最低）挪到最后，
/// 其余保持原有相对顺序（稳定分区）。
///
/// `pub(crate)`：`actions::abbr_registry_code` 复用（Go 侧 `sortActions` 同时服务两者）。
pub(crate) fn sort_actions(actions: &[Action]) -> Vec<Action> {
    let mut result = Vec::with_capacity(actions.len());
    let mut suffix = Vec::new();
    for action in actions {
        if action.window_group_id == 0 {
            suffix.push(action.clone());
        } else {
            result.push(action.clone());
        }
    }
    result.extend(suffix);
    result
}

/// 供外部（bin）复用的常量视图。
pub const REMAP_TYPE_ID: i32 = REMAP_KEY;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn action(type_id: i32, window_group: i32) -> Action {
        Action {
            type_id,
            window_group_id: window_group,
            ..Default::default()
        }
    }

    #[test]
    fn sort_actions_moves_default_group_last() {
        let actions = vec![action(6, 0), action(6, 2), action(6, 1)];
        // window_group_id != 0 的保持原序在前，== 0 的挪到最后
        let sorted: Vec<i32> = sort_actions(&actions)
            .iter()
            .map(|a| a.window_group_id)
            .collect();
        assert_eq!(sorted, vec![2, 1, 0]);
    }

    #[test]
    fn sort_hotkeys_strips_quotes_and_orders_by_type_then_length() {
        let mut map: HashMap<String, Vec<Action>> = HashMap::new();
        map.insert("bb".to_string(), vec![action(6, 0)]);
        map.insert("a".to_string(), vec![action(6, 0)]);
        map.insert("z".to_string(), vec![action(1, 0)]);

        let sorted = sort_hotkeys(&map);
        let view: Vec<(i32, String)> = sorted
            .iter()
            .map(|a| (a.type_id, a.hotkey.clone()))
            .collect();
        // TypeID 小的先；同 TypeID 按长度（"a" 先于 "bb"）
        assert_eq!(
            view,
            vec![(1, "z".into()), (6, "a".into()), (6, "bb".into())]
        );
    }

    #[test]
    fn escape_json_html_matches_go_escaping() {
        assert_eq!(escape_json_html("a && b"), "a \\u0026\\u0026 b");
        assert_eq!(escape_json_html("<x>"), "\\u003cx\\u003e");
        assert_eq!(escape_json_html("&#"), "\\u0026#");
    }

    #[test]
    fn empty_collections_serialize_as_null_but_abbr_as_array() {
        let mut config = Config::default();
        let plan = build_plan(&mut config, None);
        let json = escape_json_html(&serde_json::to_string_pretty(&plan).unwrap());
        // Go: keymaps/windowGroups/matchTypes 是 nil ⇒ null
        assert!(json.contains("\"keymaps\": null"), "{json}");
        assert!(json.contains("\"windowGroups\": null"), "{json}");
        assert!(json.contains("\"matchTypes\": null"), "{json}");
        // Go: abbr 的两个列表与 mappings 被初始化为空切片 ⇒ []（不能是 null）
        assert!(json.contains("\"capslock\": []"), "{json}");
        assert!(json.contains("\"semicolon\": []"), "{json}");
        assert!(json.contains("\"mappings\": []"), "{json}");
    }
}
