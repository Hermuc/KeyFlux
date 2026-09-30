//! 配置数据模型 —— Go `internal/script/model`（`types.go` 256 行 + `methods.go` 385 行）的同名移植。
//!
//! 字段名 / JSON 标签 / 默认值必须与 Go 逐字一致：这些结构体既用于读 `data/config.json`，
//! 也用于产出 `plan.json`（对账闸门内）与 AHK 代码。
//!
//! 关键口径（易错，均来自 Go 源码）：
//! * `Action` 的 JSON 标签是 **`actionTypeID` / `actionValueID`**（不是 `typeID` / `valueID`）；
//! * Go 缺省 JSON 字段 = 零值 ⇒ 每个结构体都 `#[serde(default)]` + `Default`；
//! * `Action.RemapInHotIf` 在 Go 侧是 `json:"-"`（不落盘，渲染期由 `handle_key_remapping` 置位）；
//! * `Config.KeyMapping` 同理（渲染期由 `handle_key_remapping` 生成）。
//!
//! 未在此实现的 Go 成员（留给后续单元）：`ParseConfig` 的默认值/迁移、渲染用字符串方法
//! （`window_groups` / `path_variables` / `custom_match_types` 的 AHK 片段）。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use crate::generator::text::{ahk_string, not_blank_lines, to_ahk_func_arg};

/// Go `model.RemapKey`：重映射按键动作的 TypeID（生成端与渲染端共用）。
pub const REMAP_KEY: i32 = 5;

// --------------------------------------------------------------------------- 顶层

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub keymaps: Vec<Keymap>,
    pub options: Options,
    #[serde(rename = "selectedAction", skip_serializing_if = "Option::is_none")]
    pub selected_action: Option<SelectedAction>,
    #[serde(rename = "actionSchemes", skip_serializing_if = "Vec::is_empty")]
    pub action_schemes: Vec<ActionScheme>,
    #[serde(rename = "fileGroups", skip_serializing_if = "Vec::is_empty")]
    pub file_groups: Vec<FileGroup>,
    #[serde(rename = "matchTypes", skip_serializing_if = "Vec::is_empty")]
    pub match_types: Vec<MatchType>,
    #[serde(rename = "overviewDocMd", skip_serializing_if = "String::is_empty")]
    pub overview_doc_md: String,
    /// Go `json:"-"`：渲染期由 [`Config::handle_key_remapping`] 生成，不落盘。
    #[serde(skip)]
    pub key_mapping: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Keymap {
    pub id: i32,
    pub name: String,
    pub enable: bool,
    pub hotkey: String,
    #[serde(rename = "parentID")]
    pub parent_id: i32,
    pub delay: i32,
    #[serde(rename = "disableAt")]
    pub disable_at: String,
    /// ⚠️ 必须是 `BTreeMap`（不可换回 `HashMap`）：Go `encoding/json` 对 **map** 键按
    /// 字典序（UTF-8 字节序）输出，`HashMap` 的随机迭代序会让同一份配置两次落盘字节不同，
    /// 也与 Go 的产出不一致（`save_config_file` 的确定性由此保证，有单测守护）。
    pub hotkeys: BTreeMap<String, Vec<Action>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Action {
    #[serde(rename = "windowGroupID")]
    pub window_group_id: i32,
    #[serde(rename = "actionTypeID")]
    pub type_id: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// 渲染期由 `sort_hotkeys` 覆写（不是配置字段）。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hotkey: String,
    #[serde(rename = "keysToSend", skip_serializing_if = "String::is_empty")]
    pub keys_to_send: String,
    #[serde(rename = "remapToKey", skip_serializing_if = "String::is_empty")]
    pub remap_to_key: String,
    #[serde(rename = "actionValueID", skip_serializing_if = "is_zero")]
    pub value_id: i32,
    #[serde(rename = "winTitle", skip_serializing_if = "String::is_empty")]
    pub win_title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub args: String,
    #[serde(rename = "workingDir", skip_serializing_if = "String::is_empty")]
    pub working_dir: String,
    #[serde(rename = "runAsAdmin", skip_serializing_if = "is_false")]
    pub run_as_admin: bool,
    #[serde(rename = "runInBackground", skip_serializing_if = "is_false")]
    pub run_in_background: bool,
    #[serde(rename = "detectHiddenWindow", skip_serializing_if = "is_false")]
    pub detect_hidden_window: bool,
    #[serde(rename = "ahkCode", skip_serializing_if = "String::is_empty")]
    pub ahk_code: String,
    /// Go `json:"-"`：渲染期由 [`Config::handle_key_remapping`] 置位。
    #[serde(skip)]
    pub remap_in_hot_if: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    #[serde(rename = "hideMatrix")]
    pub hide_matrix: bool,
    #[serde(rename = "keyfluxVersion")]
    pub keyflux_version: String,
    #[serde(rename = "windowGroups")]
    pub window_groups: Vec<WindowGroup>,
    pub mouse: Mouse,
    pub scroll: Scroll,
    #[serde(rename = "commandInputSkin")]
    pub command_input_skin: CommandInputSkin,
    #[serde(rename = "pathVariables")]
    pub path_variables: Vec<PathVariable>,
    pub startup: bool,
    pub language: String,
    #[serde(rename = "keyMapping")]
    pub key_mapping: String,
    #[serde(rename = "keyboardLayout")]
    pub keyboard_layout: String,
    #[serde(rename = "quickSwitch")]
    pub quick_switch: QuickSwitchOption,
    pub plugins: PluginsOption,
    #[serde(rename = "commandFont")]
    pub command_font: CommandFontOption,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Mouse {
    #[serde(rename = "keepMouseMode")]
    pub keep_mouse_mode: bool,
    #[serde(rename = "showTip")]
    pub show_tip: bool,
    #[serde(rename = "tipSymbol")]
    pub tip_symbol: String,
    pub delay1: String,
    pub delay2: String,
    #[serde(rename = "fastSingle")]
    pub fast_single: String,
    #[serde(rename = "fastRepeat")]
    pub fast_repeat: String,
    #[serde(rename = "slowSingle")]
    pub slow_single: String,
    #[serde(rename = "slowRepeat")]
    pub slow_repeat: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scroll {
    pub delay1: String,
    pub delay2: String,
    #[serde(rename = "onceLineCount")]
    pub once_line_count: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandInputSkin {
    #[serde(rename = "backgroundColor")]
    pub background_color: String,
    #[serde(rename = "backgroundOpacity")]
    pub background_opacity: String,
    #[serde(rename = "borderWidth")]
    pub border_width: String,
    #[serde(rename = "borderColor")]
    pub border_color: String,
    #[serde(rename = "borderOpacity")]
    pub border_opacity: String,
    #[serde(rename = "borderRadius")]
    pub border_radius: String,
    #[serde(rename = "cornerColor")]
    pub corner_color: String,
    #[serde(rename = "cornerOpacity")]
    pub corner_opacity: String,
    #[serde(rename = "gridlineColor")]
    pub gridline_color: String,
    #[serde(rename = "gridlineOpacity")]
    pub gridline_opacity: String,
    #[serde(rename = "keyColor")]
    pub key_color: String,
    #[serde(rename = "keyOpacity")]
    pub key_opacity: String,
    #[serde(rename = "hideAnimationDuration")]
    pub hide_animation_duration: String,
    #[serde(rename = "windowYPos")]
    pub window_y_pos: String,
    #[serde(rename = "windowWidth")]
    pub window_width: String,
    #[serde(rename = "windowShadowColor")]
    pub window_shadow_color: String,
    #[serde(rename = "windowShadowOpacity")]
    pub window_shadow_opacity: String,
    #[serde(rename = "windowShadowSize")]
    pub window_shadow_size: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickSwitchOption {
    #[serde(rename = "collectEnabled")]
    pub collect_enabled: bool,
    #[serde(rename = "autoShow")]
    pub auto_show: bool,
    #[serde(rename = "autoJumpOpen")]
    pub auto_jump_open: bool,
    #[serde(rename = "autoJumpSave")]
    pub auto_jump_save: bool,
    #[serde(rename = "pollIntervalMs")]
    pub poll_interval_ms: i32,
    #[serde(rename = "maxHistory")]
    pub max_history: i32,
    #[serde(rename = "overlayRows")]
    pub overlay_rows: i32,
    #[serde(rename = "overlayRowsCompact")]
    pub overlay_rows_compact: i32,
    #[serde(rename = "excludedPrefixes")]
    pub excluded_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsOption {
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandFontOption {
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    pub weight: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PathVariable {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGroup {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(rename = "conditionType", skip_serializing_if = "is_zero")]
    pub condition_type: i32,
}

// --------------------------------------------------------------------------- 选中动作

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectedAction {
    pub hotkey: String,
    pub enable: bool,
    pub mappings: Vec<SelectedMapping>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectedMapping {
    #[serde(rename = "matchType")]
    pub match_type: String,
    #[serde(rename = "matchValue")]
    pub match_value: String,
    pub entries: Vec<SelectedEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectedEntry {
    pub behavior: String,
    #[serde(rename = "actionValue", skip_serializing_if = "String::is_empty")]
    pub action_value: String,
    #[serde(rename = "workingDir", skip_serializing_if = "String::is_empty")]
    pub working_dir: String,
    pub options: RuleOptions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleOptions {
    #[serde(rename = "copyToClipboard")]
    pub copy_to_clipboard: bool,
    #[serde(rename = "clearSelection")]
    pub clear_selection: bool,
    pub confirm: bool,
}

// --------------------------------------------------------------------------- 匹配类型 / 文件分组

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileGroup {
    pub name: String,
    pub label: String,
    pub exts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchType {
    pub id: String,
    pub label: String,
    #[serde(rename = "labelEn", skip_serializing_if = "String::is_empty")]
    pub label_en: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<MatchRule>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exts: Vec<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub order: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchRule {
    pub op: String,
    pub value: String,
}

/// Go `model.ActionScheme`：旧多方案结构，仅存量迁移读取（P3 不渲染）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionScheme {
    pub id: i32,
    pub name: String,
    pub hotkey: String,
    pub enable: bool,
    #[serde(rename = "restartFailed", skip_serializing_if = "is_false")]
    pub restart_failed: bool,
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// 从 `config.json` 文本解析配置 —— **必须**经由本函数而非直接 `serde_json::from_str`。
///
/// ⚠️ 保真点：Go 的 `encoding/json` 把 `null` 解成**零值**（`null` 落进 slice/map/string
/// 都不报错，只留 nil/""），serde 却会以 `invalid type: null, expected a sequence` 失败。
/// 真实配置里就有这种字段（实测 `options.plugins.disabled: null`）。
///
/// 故先递归剔除**对象字段**里的 `null`（等价于"字段缺失" ⇒ 走 `#[serde(default)]` ⇒ 零值），
/// 从而与 Go 的容错口径一致。数组元素里的 `null` 未处理（Go 同样会把它解成元素零值，
/// 但真实配置未出现；若将来遇到，需在此处按字段类型补齐）。
pub fn config_from_json(raw: &str) -> serde_json::Result<Config> {
    let mut value: serde_json::Value = serde_json::from_str(raw)?;
    strip_null_fields(&mut value);
    serde_json::from_value(value)
}

/// 递归剔除**对象字段**里的 `null`（等价于"字段缺失" ⇒ 走 `#[serde(default)]` ⇒ 零值）。
///
/// `pub(crate)`：`actions` 的夹具单测复用同一容错口径（Go 的 nil map/slice ⇒ JSON `null`）。
pub(crate) fn strip_null_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, field| !field.is_null());
            for field in map.values_mut() {
                strip_null_fields(field);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                strip_null_fields(item);
            }
        }
        _ => {}
    }
}

// --------------------------------------------------------------------------- 方法（Go methods.go）

impl Config {
    /// Go `(*Config).EnabledKeymaps`：`ID==1 && Enable` 与 `ID>=5 && Enable` 入选，
    /// 再按 `ParentID` 把子模式紧随其父插入。**副作用**：ID==1 时调用
    /// [`Config::handle_key_remapping`]（与 Go 一致）。
    pub fn enabled_keymaps(&mut self) -> Vec<Keymap> {
        // Go 侧 `km.Hotkeys` 是 map ⇒ `handleKeyRemapping` 的改写对 `enabled` 里的同一个
        // Keymap 可见。故这里先拍快照用于遍历（规避借用冲突），改动后再**从 self 重取**，
        // 保证返回的 Keymap 携带改写后的 Map（与 Go 的共享引用语义等价）。
        let snapshot: Vec<Keymap> = self.keymaps.clone();
        let mut enabled: Vec<Keymap> = Vec::new();
        for (index, km) in snapshot.iter().enumerate() {
            if km.id == 1 && km.enable {
                self.handle_key_remapping(index);
                enabled.push(self.keymaps[index].clone());
            }
            if km.id >= 5 && km.enable {
                enabled.push(self.keymaps[index].clone());
            }
        }

        let mut groups: HashMap<i32, Vec<Keymap>> = HashMap::new();
        for km in &enabled {
            if km.parent_id != 0 {
                groups.entry(km.parent_id).or_default().push(km.clone());
            }
        }

        let mut result = Vec::new();
        for km in &enabled {
            if km.parent_id == 0 {
                result.push(km.clone());
                if let Some(sub) = groups.get(&km.id) {
                    result.extend(sub.iter().cloned());
                }
            }
        }
        result
    }

    /// Go `(*Config).handleKeyRemapping`：收集 TypeID==5 的动作，按 `WindowGroupID`
    /// **稳定**排序后渲染成 `a::b` 段（内含 `#HotIf` 头），并写回两个副作用：
    /// `KeyMapping` 与各动作的 `remap_in_hot_if`。
    ///
    /// 逐字复刻 Go 的写回顺序（`a.Hotkey = hk` → 副本进 list → `a.RemapInHotIf = true`
    /// → 写回 map），故 list 里的副本**保留原始** `remap_in_hot_if`，而 map 项两处都被改。
    pub fn handle_key_remapping(&mut self, index: usize) {
        let mut list: Vec<Action> = Vec::new();
        if let Some(km) = self.keymaps.get_mut(index) {
            for (hotkey, actions) in km.hotkeys.iter_mut() {
                for action in actions.iter_mut() {
                    if action.type_id == REMAP_KEY {
                        action.hotkey = hotkey.clone();
                        list.push(action.clone());
                        action.remap_in_hot_if = true;
                    }
                }
            }
        }

        // Go 用 sort.SliceStable 按 WindowGroupID 升序（并列项保持原始=map 随机序）
        list.sort_by_key(|action| action.window_group_id);

        let mut out = String::new();
        let mut last_group = -1;
        for action in &list {
            if last_group != action.window_group_id {
                out.push('\n');
                out.push_str(&self.hotif_header(action));
                out.push('\n');
                last_group = action.window_group_id;
            }
            let _ = writeln!(
                out,
                "{}::{}",
                action.hotkey.trim_start_matches('*'),
                action.remap_to_key
            );
        }
        out.push_str("\n#HotIf");
        self.key_mapping = out;
    }

    /// Go `(*Config).GetWinTitle`：`WindowGroupID==0` ⇒ `(""", 0)`；
    /// 命中窗口组时 `ConditionType==5` 走表达式分支（值加单引号）。
    pub fn get_win_title(&self, action: &Action) -> (String, i32) {
        if action.window_group_id == 0 {
            return (r#""""#.to_string(), 0);
        }
        for group in &self.options.window_groups {
            if group.id == action.window_group_id {
                if group.condition_type == 5 {
                    return (format!("'{}'", group.value), 5);
                }
                return (group_to_win_tile(group), group.condition_type);
            }
        }
        (r#""""#.to_string(), 0)
    }

    /// Go `(*Config).GetHotkeyContext`。
    pub fn get_hotkey_context(&self, action: &Action) -> String {
        let (win_title, condition_type) = self.get_win_title(action);
        if win_title == r#""""# && condition_type == 0 {
            return String::new();
        }
        format!(", , {win_title}, {condition_type}")
    }

    /// Go `hotifHeader`（非导出；被 `handle_key_remapping` 调用）。
    fn hotif_header(&self, action: &Action) -> String {
        let (win_title, condition_type) = self.get_win_title(action);
        if win_title == r#""""# && condition_type == 0 {
            return "#HotIf".to_string();
        }
        match condition_type {
            1 => format!("#HotIf WinActive({win_title})"),
            2 => format!("#HotIf WinExist({win_title})"),
            3 => format!("#HotIf !WinActive({win_title})"),
            4 => format!("#HotIf !WinExist({win_title})"),
            5 => format!("#HotIf {win_title} "),
            _ => String::new(),
        }
    }

    /// Go `(*Config).GetKeymapDisableAt`：0 行 ⇒ ""；1 行 ⇒ 该行；多行 ⇒ 组名。
    pub fn get_keymap_disable_at(&self, keymap_id: i32) -> String {
        for km in &self.keymaps {
            if km.id == keymap_id {
                let lines = not_blank_lines(&km.disable_at);
                return match lines.len() {
                    0 => String::new(),
                    1 => lines[0].clone(),
                    _ => format!("ahk_group GROUP_DISABLE_KEYMAP_{keymap_id}"),
                };
            }
        }
        String::new()
    }

    /// Go `(*Config).CapslockAbbr`：`Hotkey == "capslockAbbr"` 的模式的热键表。
    pub fn capslock_abbr(&self) -> BTreeMap<String, Vec<Action>> {
        for km in &self.keymaps {
            if km.hotkey == "capslockAbbr" {
                return km.hotkeys.clone();
            }
        }
        BTreeMap::new()
    }

    /// Go `(*Config).SemicolonAbbr`。
    pub fn semicolon_abbr(&self) -> BTreeMap<String, Vec<Action>> {
        for km in &self.keymaps {
            if km.hotkey == "semicolonAbbr" {
                return km.hotkeys.clone();
            }
        }
        BTreeMap::new()
    }

    /// Go `(*Config).CapslockAbbrEnabled`：任一启用模式里存在 TypeID9/ValueID6。
    pub fn capslock_abbr_enabled(&self) -> bool {
        has_abbr_trigger(&self.keymaps, 6)
    }

    /// Go `(*Config).SemicolonAbbrEnabled`：任一启用模式里存在 TypeID9/ValueID5。
    pub fn semicolon_abbr_enabled(&self) -> bool {
        has_abbr_trigger(&self.keymaps, 5)
    }

    /// Go `(*Config).CapslockAbbrKeys`：缩写键去重排序，并把 `,` 转义为 `,,`。
    pub fn capslock_abbr_keys(&self) -> String {
        abbr_keys(&self.capslock_abbr())
    }

    /// Go `(*Config).SemicolonAbbrKeys`。
    pub fn semicolon_abbr_keys(&self) -> String {
        abbr_keys(&self.semicolon_abbr())
    }

    /// Go `(*Config).FindMatchType`。
    pub fn find_match_type(&self, id: &str) -> Option<&MatchType> {
        self.match_types.iter().find(|mt| mt.id == id)
    }

    /// Go `(*Config).FileGroupExts`。
    pub fn file_group_exts(&self, name: &str) -> Option<&Vec<String>> {
        self.file_groups
            .iter()
            .find(|g| g.name == name)
            .map(|g| &g.exts)
    }

    /// Go `(*Config).PathVariables`（AHK 片段）。
    pub fn path_variables(&self) -> String {
        let mut out = String::new();
        for variable in &self.options.path_variables {
            if variable.name.trim().is_empty() {
                continue;
            }
            out.push_str("  ");
            out.push_str(&variable.name);
            out.push_str(" := ");
            out.push_str(&to_ahk_func_arg(&variable.value));
            out.push('\n');
        }
        out
    }

    /// Go `(*Config).WindowGroups`（AHK 片段）。
    pub fn window_groups(&self) -> String {
        let mut out = String::new();
        for group in &self.options.window_groups {
            add_group(&mut out, &group.value, group.id, None);
        }
        for km in &self.keymaps {
            add_group(
                &mut out,
                &km.disable_at,
                km.id,
                Some("GROUP_DISABLE_KEYMAP_"),
            );
        }
        out
    }

    /// Go `(*Config).CustomMatchTypes`（AHK 片段）；两者皆空 ⇒ 空串（零 golden 影响的落点）。
    pub fn custom_match_types(&self) -> String {
        if self.match_types.is_empty() && self.file_groups.is_empty() {
            return String::new();
        }
        let mut out = String::from("global CustomMatchTypes := Map(\n");
        let mut first = true;
        let mut write_entry =
            |out: &mut String, id: &str, kind: &str, exts: &[String], rules: &[MatchRule]| {
                if !first {
                    out.push_str(",\n");
                }
                first = false;
                out.push_str("  ");
                out.push_str(&ahk_string(id));
                out.push_str(", {kind: ");
                out.push_str(&ahk_string(kind));
                if kind == "fileExt" {
                    out.push_str(", exts: [");
                    for (index, ext) in exts.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&ahk_string(ext));
                    }
                    out.push_str("]}");
                    return;
                }
                out.push_str(", rules: [");
                for (index, rule) in rules.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str("{op: ");
                    out.push_str(&ahk_string(&rule.op));
                    out.push_str(", value: ");
                    out.push_str(&ahk_string(&rule.value));
                    out.push('}');
                }
                out.push_str("]}");
            };

        for mt in &self.match_types {
            write_entry(&mut out, &mt.id, &mt.kind, &mt.exts, &mt.rules);
        }
        for group in &self.file_groups {
            if self.find_match_type(&group.name).is_some() {
                continue; // 同名已在 matchTypes 出现（防御手改配置）
            }
            write_entry(&mut out, &group.name, "fileExt", &group.exts, &[]);
        }
        out.push_str("\n)");
        out
    }
}

/// Go `hasAbbrTrigger` 的等价内联（两条 `CapslockAbbrEnabled` / `SemicolonAbbrEnabled` 共用）。
fn has_abbr_trigger(keymaps: &[Keymap], value_id: i32) -> bool {
    for km in keymaps {
        if !km.enable {
            continue;
        }
        for actions in km.hotkeys.values() {
            for action in actions {
                if action.type_id == 9 && action.value_id == value_id {
                    return true;
                }
            }
        }
    }
    false
}

/// Go `CapslockAbbrKeys` / `SemicolonAbbrKeys` 的公共实现。
fn abbr_keys(abbr: &BTreeMap<String, Vec<Action>>) -> String {
    let mut keys: Vec<String> = abbr.keys().map(|key| key.replace(',', ",,")).collect();
    keys.sort();
    keys.join(",")
}

/// Go `model.GroupName`：默认前缀 `MY_WINDOW_GROUP_`；`id < 0` 时多一个下划线。
pub fn group_name(id: i32, prefix: Option<&str>) -> String {
    let prefix = prefix.unwrap_or("MY_WINDOW_GROUP_");
    if id < 0 {
        format!("{prefix}_{}", -id)
    } else {
        format!("{prefix}{id}")
    }
}

/// Go `model.GroupToWinTile`：多行 ⇒ `"ahk_group <名>"`；单行 ⇒ 去空白后的值。
pub fn group_to_win_tile(group: &WindowGroup) -> String {
    if not_blank_lines(&group.value).len() > 1 {
        format!("\"ahk_group {}\"", group_name(group.id, None))
    } else {
        format!("\"{}\"", group.value.trim())
    }
}

/// Go `addGroup`：多行时逐行 `GroupAdd`（表达式直出的 `""` 跳过）。
fn add_group(out: &mut String, value: &str, id: i32, prefix: Option<&str>) {
    let lines = not_blank_lines(value);
    if lines.len() > 1 {
        for line in &lines {
            let arg = to_ahk_func_arg(line);
            if arg != r#""""# {
                let _ = writeln!(out, "  GroupAdd(\"{}\", {})", group_name(id, prefix), arg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(path: &str) -> Config {
        let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {path} 失败: {e}"));
        config_from_json(&raw).unwrap_or_else(|e| panic!("解析 {path} 失败: {e}"))
    }

    /// Go `encoding/json` 容忍 `null`（解为零值），serde 不容忍 —— 走 `config_from_json` 后一致。
    #[test]
    fn config_from_json_tolerates_nulls_like_go() {
        let config = config_from_json(
            r#"{"options":{"plugins":{"disabled":null}},"keymaps":null,"matchTypes":null}"#,
        )
        .expect("null 字段应被容错");
        assert!(config.options.plugins.disabled.is_empty());
        assert!(config.keymaps.is_empty());
        assert!(config.match_types.is_empty());
    }

    /// 能直接吃下真实配置（字段名/标签一致，且 Unknown 字段不致命）。
    #[test]
    fn parses_real_corpus_configs() {
        for path in [
            "../tools/parity/corpus/factory/config.json",
            "../tools/parity/corpus/synthetic/config.json",
        ] {
            let config = load(path);
            assert!(!config.keymaps.is_empty(), "{path} 应含 keymaps");
            // Action 的标签是 actionTypeID / actionValueID（不是 typeID / valueID）
            let has_action = config
                .keymaps
                .iter()
                .any(|km| km.hotkeys.values().any(|actions| !actions.is_empty()));
            assert!(has_action, "{path} 应含动作");
        }
    }

    /// Go: `ID==1 && Enable` 与 `ID>=5 && Enable` 入选；子模式紧随其父。
    #[test]
    fn enabled_keymaps_filters_and_groups() {
        let mut config = Config {
            keymaps: vec![
                Keymap {
                    id: 2,
                    enable: true,
                    ..Default::default()
                },
                Keymap {
                    id: 5,
                    enable: true,
                    ..Default::default()
                },
                Keymap {
                    id: 6,
                    enable: true,
                    parent_id: 5,
                    ..Default::default()
                },
                Keymap {
                    id: 7,
                    enable: false,
                    ..Default::default()
                },
                Keymap {
                    id: 1,
                    enable: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let ids: Vec<i32> = config.enabled_keymaps().iter().map(|km| km.id).collect();
        // ID2 不入选；ID6 是 ID5 的子模式，紧随其后；禁用的 ID7 不入选。
        assert_eq!(ids, vec![5, 6, 1]);
    }

    /// Go `GetWinTitle` 三分支：无窗口组 / 普通条件 / conditionType 5 表达式。
    #[test]
    fn get_win_title_branches() {
        let config = Config {
            options: Options {
                window_groups: vec![
                    WindowGroup {
                        id: 1,
                        name: "steam".into(),
                        value: "steam.exe".into(),
                        condition_type: 1,
                    },
                    WindowGroup {
                        id: 2,
                        name: "expr".into(),
                        value: "A && B".into(),
                        condition_type: 5,
                    },
                    WindowGroup {
                        id: 3,
                        name: "multi".into(),
                        value: "a.exe\nb.exe".into(),
                        condition_type: 2,
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        let action = |wg: i32| Action {
            window_group_id: wg,
            ..Default::default()
        };

        assert_eq!(config.get_win_title(&action(0)), (r#""""#.to_string(), 0));
        assert_eq!(
            config.get_win_title(&action(1)),
            ("\"steam.exe\"".to_string(), 1)
        );
        assert_eq!(
            config.get_win_title(&action(2)),
            ("'A && B'".to_string(), 5)
        );
        // 多行 ⇒ ahk_group
        assert_eq!(
            config.get_win_title(&action(3)),
            ("\"ahk_group MY_WINDOW_GROUP_3\"".to_string(), 2)
        );
        // 未命中 ⇒ 与 WindowGroupID==0 同形
        assert_eq!(config.get_win_title(&action(99)), (r#""""#.to_string(), 0));
        // GetHotkeyContext 在 ("" ,0) 时为空
        assert_eq!(config.get_hotkey_context(&action(0)), "");
        assert_eq!(
            config.get_hotkey_context(&action(1)),
            ", , \"steam.exe\", 1"
        );
    }

    /// Go `GetKeymapDisableAt` 三分支。
    #[test]
    fn keymap_disable_at_branches() {
        let config = Config {
            keymaps: vec![
                Keymap {
                    id: 1,
                    disable_at: String::new(),
                    ..Default::default()
                },
                Keymap {
                    id: 5,
                    disable_at: " steam.exe \n\n".into(),
                    ..Default::default()
                },
                Keymap {
                    id: 6,
                    disable_at: "a.exe\nb.exe".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(config.get_keymap_disable_at(1), "");
        assert_eq!(config.get_keymap_disable_at(5), "steam.exe"); // 单行 ⇒ trim 后的行
        assert_eq!(
            config.get_keymap_disable_at(6),
            "ahk_group GROUP_DISABLE_KEYMAP_6"
        );
        assert_eq!(config.get_keymap_disable_at(42), "");
    }

    /// Go `CapslockAbbrEnabled` / `SemicolonAbbrEnabled` 只看启用模式里的 TypeID9 触发动作。
    #[test]
    fn abbr_enabled_detects_typeid9_triggers() {
        let mut hotkeys = BTreeMap::new();
        hotkeys.insert(
            "*CapsLock".to_string(),
            vec![Action {
                type_id: 9,
                value_id: 6,
                ..Default::default()
            }],
        );
        let disabled = Config {
            keymaps: vec![Keymap {
                id: 5,
                enable: false,
                hotkeys: hotkeys.clone(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(!disabled.capslock_abbr_enabled(), "禁用的模式不应触发");

        let enabled = Config {
            keymaps: vec![Keymap {
                id: 5,
                enable: true,
                hotkeys,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(enabled.capslock_abbr_enabled());
        assert!(!enabled.semicolon_abbr_enabled());
    }

    /// Go `CapslockAbbrKeys`：排序 + `,` → `,,`。
    #[test]
    fn abbr_keys_sorts_and_escapes_comma() {
        let config = Config {
            keymaps: vec![Keymap {
                id: 2,
                hotkey: "capslockAbbr".into(),
                hotkeys: BTreeMap::from([
                    ("web".to_string(), vec![]),
                    (",".to_string(), vec![]),
                    ("jk".to_string(), vec![]),
                ]),
                ..Default::default()
            }],
            ..Default::default()
        };
        // `,` 先被转义为 `,,`，再参与排序并以 `,` 连接 ⇒ 三段拼出 ",,,jk,web"
        assert_eq!(config.capslock_abbr_keys(), ",,,jk,web");
        assert_eq!(config.semicolon_abbr_keys(), "");
    }

    /// Go `GroupName`：负数多一个下划线；前缀可覆盖。
    #[test]
    fn group_name_handles_negative_ids() {
        assert_eq!(group_name(3, None), "MY_WINDOW_GROUP_3");
        assert_eq!(group_name(-1, None), "MY_WINDOW_GROUP__1");
        assert_eq!(group_name(3, Some("PREFIX_")), "PREFIX_3");
        assert_eq!(group_name(-1, Some("PREFIX_")), "PREFIX__1");
    }

    /// Go `handleKeyRemapping` 的副作用：`remap_in_hot_if` 置位 + `key_mapping` 前缀 `\n`。
    #[test]
    fn handle_key_remapping_sets_flags_and_renders() {
        let mut keymaps_hotkeys: BTreeMap<String, Vec<Action>> = BTreeMap::new();
        keymaps_hotkeys.insert(
            "a".to_string(),
            vec![Action {
                type_id: REMAP_KEY,
                remap_to_key: "b".into(),
                window_group_id: 0,
                ..Default::default()
            }],
        );
        keymaps_hotkeys.insert(
            "*q".to_string(),
            vec![Action {
                type_id: REMAP_KEY,
                remap_to_key: "w".into(),
                window_group_id: 1,
                ..Default::default()
            }],
        );

        let mut config = Config {
            keymaps: vec![Keymap {
                id: 1,
                enable: true,
                hotkeys: keymaps_hotkeys,
                ..Default::default()
            }],
            ..Default::default()
        };
        config.options.window_groups = vec![WindowGroup {
            id: 1,
            name: "g".into(),
            value: "steam.exe".into(),
            condition_type: 1,
        }];

        config.handle_key_remapping(0);

        // 副作用 1：所有重映射动作被标记
        let flags: Vec<bool> = config.keymaps[0]
            .hotkeys
            .values()
            .flat_map(|actions| actions.iter().map(|a| a.remap_in_hot_if))
            .collect();
        assert!(flags.iter().all(|flag| *flag), "所有重映射动作都应置位");

        // 副作用 2：KeyMapping 段（WindowGroupID 0 在前，`*` 被 trim）
        let mapping = &config.key_mapping;
        assert!(
            mapping.starts_with("\n#HotIf\n"),
            "首组前写入 HotIf 头: {mapping:?}"
        );
        assert!(mapping.contains("a::b\n"), "{mapping:?}");
        assert!(
            mapping.contains("#HotIf WinActive(\"steam.exe\")"),
            "{mapping:?}"
        );
        assert!(
            mapping.contains("q::w\n"),
            "`*q` 应被 trim 成 `q`: {mapping:?}"
        );
        assert!(mapping.ends_with("\n#HotIf"), "{mapping:?}");
    }

    /// Go `CustomMatchTypes`：空 ⇒ ""（零 golden 影响）；文件名与 matchTypes 重名时跳过。
    #[test]
    fn custom_match_types_empty_and_dedupe() {
        let empty = Config::default();
        assert_eq!(empty.custom_match_types(), "");

        let config = Config {
            match_types: vec![MatchType {
                id: "netdisk".into(),
                kind: "text".into(),
                rules: vec![MatchRule {
                    op: "contains".into(),
                    value: "pan.baidu.com".into(),
                }],
                ..Default::default()
            }],
            file_groups: vec![
                FileGroup {
                    name: "image".into(),
                    label: "图片".into(),
                    exts: vec!["jpg".into(), "png".into()],
                },
                FileGroup {
                    name: "netdisk".into(),
                    label: "重名".into(),
                    exts: vec!["x".into()],
                },
            ],
            ..Default::default()
        };
        let out = config.custom_match_types();
        assert!(
            out.starts_with("global CustomMatchTypes := Map(\n"),
            "{out}"
        );
        assert!(out.contains("\"netdisk\", {kind: \"text\", rules: [{op: \"contains\", value: \"pan.baidu.com\"}]}"), "{out}");
        assert!(
            out.contains("\"image\", {kind: \"fileExt\", exts: [\"jpg\", \"png\"]}"),
            "{out}"
        );
        assert!(
            !out.contains("重名"),
            "与 matchTypes 同名的分组应跳过: {out}"
        );
        assert!(out.ends_with("\n)"), "{out}");
    }
}
