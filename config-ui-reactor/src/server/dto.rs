//! HTTP DTO 层 —— Go `internal/server/dto.go`（652 行）的逐字段移植。
//!
//! 契约（与 Go `encoding/json` 的 wire 口径逐字对齐）：
//! * **字段序**：serde 按结构体声明序输出 = Go 按结构体字段声明序输出，故各 DTO
//!   字段声明顺序必须与 dto.go 逐字一致（gin `c.JSON` 不缩进、map 键按字典序 ——
//!   `gin.H` 两处键序 message < restartFailed 天然满足）。
//! * **omitempty** ⇒ `skip_serializing_if`：`String::is_empty` / `Vec::is_empty` /
//!   `is_zero`（int 0）/ `is_false`（bool false），与 Go omitempty 判定一致。
//!   例外：Go 对**结构体**的 omitempty 不生效（`ConfigDTO.Options` 恒输出），Rust
//!   直接用非 Option 字段即可。
//! * **空集合恒 `[]`**：`ConfigToDTO` 保证 keymaps/fileGroups/matchTypes/mappings
//!   永不缺键（dto.go:208-235）。`options.windowGroups` / `pathVariables` /
//!   `quickSwitch.excludedPrefixes` 在 Go 侧 nil 时输出 `null`，Rust 模型
//!   （`Vec`）无法区分 nil 与空，恒输出 `[]`（真实配置无 null 形态，见下）。
//! * **`hotkeys` map**：Go nil map 输出 `null`、空 map 输出 `{}`；Rust 模型
//!   （`BTreeMap`）同样无法区分，恒输出 `{}`（真实 config.json 16 个 keymap 中
//!   0 个 null、1 个 `{}`，按真实语料取 `{}` 口径）。map 键序：Go 按字节字典序，
//!   两端均用 `BTreeMap` 对齐（`String` 的 `Ord` 即字节序）—— generator 模型
//!   `Keymap.hotkeys` 亦为 `BTreeMap`，故 DTO 与落盘同序。
//! * **HTML 转义**：gin `c.JSON` 走 `json.Marshal` 默认口径，`<` `>` `&`
//!   （及 U+2028/2029）转义为 `\u003c` 等 —— 见 [`marshal_go_json`]。
//!   与落盘相反：`SaveConfigFile` 是 `SetEscapeHTML(false)`（generator 侧已实现）。
//!
//! 加载（PUT 绑定）口径：Go `ShouldBindJSON` 把 `null` 解为零值、缺失解零值、
//! 忽略未知字段、**只解首个 JSON 值**（`json.Decoder.Decode`）—— 由
//! [`bind_config_dto`] 统一模拟（null 剥除复用 `model::strip_null_fields`）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::generator::model as m;

// --------------------------------------------------------------------------- DTO 类型

/// Go `ConfigDTO`。字段序 = dto.go:11-18。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigDto {
    pub keymaps: Vec<KeymapDto>,
    /// Go 标签带 `omitempty` 但对象类型不生效，恒输出。
    pub options: OptionsDto,
    #[serde(rename = "selectedAction")]
    pub selected_action: SelectedActionDto,
    #[serde(rename = "fileGroups")]
    pub file_groups: Vec<FileGroupDto>,
    #[serde(rename = "matchTypes")]
    pub match_types: Vec<MatchTypeDto>,
    #[serde(rename = "overviewDocMd", skip_serializing_if = "String::is_empty")]
    pub overview_doc_md: String,
}

/// Go `SelectedActionDTO`（dto.go:22-26）：mappings 空恒 `[]`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectedActionDto {
    pub hotkey: String,
    pub enable: bool,
    pub mappings: Vec<SelectedMappingDto>,
}

/// Go `SelectedMappingDTO`：entries 空恒 `[]`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectedMappingDto {
    #[serde(rename = "matchType")]
    pub match_type: String,
    #[serde(rename = "matchValue")]
    pub match_value: String,
    pub entries: Vec<SelectedEntryDto>,
}

/// Go `SelectedEntryDTO`（dto.go:34-39）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectedEntryDto {
    pub behavior: String,
    #[serde(rename = "actionValue", skip_serializing_if = "String::is_empty")]
    pub action_value: String,
    #[serde(rename = "workingDir", skip_serializing_if = "String::is_empty")]
    pub working_dir: String,
    pub options: RuleOptionsDto,
}

/// Go `KeymapDTO`（dto.go:41-50）。`hotkeys` 用 `BTreeMap` 对齐 Go 的 map 键字典序。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapDto {
    pub id: i32,
    pub name: String,
    pub enable: bool,
    pub hotkey: String,
    #[serde(rename = "parentID")]
    pub parent_id: i32,
    pub delay: i32,
    #[serde(rename = "disableAt")]
    pub disable_at: String,
    pub hotkeys: BTreeMap<String, Vec<ActionDto>>,
}

/// Go `FileGroupDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileGroupDto {
    pub name: String,
    pub label: String,
    pub exts: Vec<String>,
}

/// Go `MatchTypeDTO`（dto.go:61-69）：rules/exts/order 带 omitempty。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchTypeDto {
    pub id: String,
    pub label: String,
    #[serde(rename = "labelEn", skip_serializing_if = "String::is_empty")]
    pub label_en: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<MatchRuleDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exts: Vec<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub order: i32,
}

/// Go `MatchRuleDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchRuleDto {
    pub op: String,
    pub value: String,
}

/// Go `RuleOptionsDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleOptionsDto {
    #[serde(rename = "copyToClipboard")]
    pub copy_to_clipboard: bool,
    #[serde(rename = "clearSelection")]
    pub clear_selection: bool,
    pub confirm: bool,
}

/// Go `ActionDTO`（dto.go:82-99）：除 windowGroupID/actionTypeID 外全 omitempty。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionDto {
    #[serde(rename = "windowGroupID")]
    pub window_group_id: i32,
    #[serde(rename = "actionTypeID")]
    pub type_id: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
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
    #[serde(skip_serializing_if = "is_false")]
    pub run_as_admin: bool,
    #[serde(rename = "runInBackground", skip_serializing_if = "is_false")]
    pub run_in_background: bool,
    #[serde(rename = "detectHiddenWindow", skip_serializing_if = "is_false")]
    pub detect_hidden_window: bool,
    #[serde(rename = "ahkCode", skip_serializing_if = "String::is_empty")]
    pub ahk_code: String,
}

/// Go `OptionsDTO`（dto.go:101-116）。字段序逐字对齐。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OptionsDto {
    #[serde(rename = "hideMatrix")]
    pub hide_matrix: bool,
    #[serde(rename = "keyfluxVersion")]
    pub keyflux_version: String,
    #[serde(rename = "windowGroups")]
    pub window_groups: Vec<WindowGroupDto>,
    pub mouse: MouseDto,
    pub scroll: ScrollDto,
    #[serde(rename = "commandInputSkin")]
    pub command_input_skin: CommandInputSkinDto,
    #[serde(rename = "pathVariables")]
    pub path_variables: Vec<PathVariableDto>,
    pub startup: bool,
    pub language: String,
    #[serde(rename = "keyMapping")]
    pub key_mapping: String,
    #[serde(rename = "keyboardLayout")]
    pub keyboard_layout: String,
    #[serde(rename = "quickSwitch")]
    pub quick_switch: QuickSwitchOptionDto,
    pub plugins: PluginsOptionDto,
    #[serde(rename = "commandFont")]
    pub command_font: CommandFontOptionDto,
}

/// Go `CommandFontOptionDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandFontOptionDto {
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    pub weight: String,
}

/// Go `PluginsOptionDTO`：disabled 空恒 `[]`（dto.go:409 契约）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsOptionDto {
    pub disabled: Vec<String>,
}

/// Go `QuickSwitchOptionDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickSwitchOptionDto {
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

/// Go `WindowGroupDTO`（value/conditionType omitempty）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGroupDto {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(rename = "conditionType", skip_serializing_if = "is_zero")]
    pub condition_type: i32,
}

/// Go `MouseDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseDto {
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

/// Go `ScrollDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollDto {
    pub delay1: String,
    pub delay2: String,
    #[serde(rename = "onceLineCount")]
    pub once_line_count: String,
}

/// Go `PathVariableDTO`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PathVariableDto {
    pub name: String,
    pub value: String,
}

/// Go `CommandInputSkinDTO`：18 字段，无 omitempty，字段序逐字对齐。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandInputSkinDto {
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

fn is_zero(value: &i32) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

// --------------------------------------------------------------------------- 序列化口径

/// gin `c.JSON` 口径的 JSON 序列化：compact + **HTML 转义**（`json.Marshal` 默认
/// `SetEscapeHTML(true)`：`<`→`\u003c`、`>`→`\u003e`、`&`→`\u0026`，字符串内的
/// U+2028/U+2029 同样转义）。serde_json 不做这些转义，故对序列化产物做
/// **字符串字面量感知**的后处理（逐字节扫描，追踪 in-string 状态与 `\` 转义）。
pub(crate) fn marshal_go_json<T: Serialize>(value: &T) -> String {
    let raw = serde_json::to_string(value).expect("DTO 序列化不应失败");
    go_html_escape_json(&raw)
}

/// 对已序列化的 JSON 文本补做 Go 的 HTML 转义。仅在字符串字面量内部替换，
/// JSON 结构字符（键/标点）不受影响；UTF-8 多字节序列原样保留（Go 输出非 ASCII
/// 为原始 UTF-8，除 U+2028/2029 外不转义）。
pub(crate) fn go_html_escape_json(json: &str) -> String {
    let bytes = json.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() + 16);
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if !in_string {
            if b == b'"' {
                in_string = true;
            }
            out.push(b);
            i += 1;
            continue;
        }
        match b {
            // serde_json 的转义产物（\" \\ \n \uXXXX …）全部是 ASCII：连同转义符
            // 一起原样拷贝，避免把 \" 误判为字符串结束。
            b'\\' => {
                out.push(b);
                if i + 1 < bytes.len() {
                    i += 1;
                    out.push(bytes[i]);
                }
                i += 1;
            }
            b'"' => {
                in_string = false;
                out.push(b);
                i += 1;
            }
            b'<' => {
                out.extend_from_slice(b"\\u003c");
                i += 1;
            }
            b'>' => {
                out.extend_from_slice(b"\\u003e");
                i += 1;
            }
            b'&' => {
                out.extend_from_slice(b"\\u0026");
                i += 1;
            }
            // U+2028 = E2 80 A8 / U+2029 = E2 80 A9（Go json.Marshal 在字符串内转义）
            0xE2 if i + 2 < bytes.len()
                && bytes[i + 1] == 0x80
                && (bytes[i + 2] == 0xA8 || bytes[i + 2] == 0xA9) =>
            {
                if bytes[i + 2] == 0xA8 {
                    out.extend_from_slice(b"\\u2028");
                } else {
                    out.extend_from_slice(b"\\u2029");
                }
                i += 3;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).expect("输入本就是合法 UTF-8")
}

// --------------------------------------------------------------------------- model → DTO

/// Go `ConfigToDTO`（dto.go:195-238）：空集合恒 `[]`。
pub fn config_to_dto(cfg: &m::Config) -> ConfigDto {
    let mut dto = ConfigDto {
        overview_doc_md: cfg.overview_doc_md.clone(),
        options: options_to_dto(&cfg.options),
        ..Default::default()
    };
    dto.keymaps = cfg.keymaps.iter().map(keymap_to_dto).collect();
    dto.file_groups = cfg
        .file_groups
        .iter()
        .map(|fg| FileGroupDto {
            name: fg.name.clone(),
            label: fg.label.clone(),
            exts: fg.exts.clone(),
        })
        .collect();
    dto.match_types = cfg.match_types.iter().map(match_type_to_dto).collect();
    dto.selected_action = selected_action_to_dto(cfg.selected_action.as_ref());
    dto
}

/// Go `keymapToDTO`（dto.go:252-276）。Go 对 nil `Hotkeys` 输出 `null`、空 map 输出
/// `{}`；Rust 模型无法区分，恒输出 `{}`（真实语料 0 null / 1 `{}`，见模块头注释）。
fn keymap_to_dto(km: &m::Keymap) -> KeymapDto {
    let mut hotkeys = BTreeMap::new();
    for (key, actions) in &km.hotkeys {
        hotkeys.insert(key.clone(), actions.iter().map(action_to_dto).collect());
    }
    KeymapDto {
        id: km.id,
        name: km.name.clone(),
        enable: km.enable,
        hotkey: km.hotkey.clone(),
        parent_id: km.parent_id,
        delay: km.delay,
        disable_at: km.disable_at.clone(),
        hotkeys,
    }
}

/// Go `actionToDTO`（dto.go:278-296）。
fn action_to_dto(a: &m::Action) -> ActionDto {
    ActionDto {
        window_group_id: a.window_group_id,
        type_id: a.type_id,
        comment: a.comment.clone(),
        hotkey: a.hotkey.clone(),
        keys_to_send: a.keys_to_send.clone(),
        remap_to_key: a.remap_to_key.clone(),
        value_id: a.value_id,
        win_title: a.win_title.clone(),
        target: a.target.clone(),
        args: a.args.clone(),
        working_dir: a.working_dir.clone(),
        run_as_admin: a.run_as_admin,
        run_in_background: a.run_in_background,
        detect_hidden_window: a.detect_hidden_window,
        ahk_code: a.ahk_code.clone(),
    }
}

/// Go `selectedActionToDTO`（dto.go:298-312）：nil → 空结构 + `mappings: []`。
fn selected_action_to_dto(sa: Option<&m::SelectedAction>) -> SelectedActionDto {
    let Some(sa) = sa else {
        return SelectedActionDto::default();
    };
    SelectedActionDto {
        hotkey: sa.hotkey.clone(),
        enable: sa.enable,
        mappings: sa.mappings.iter().map(selected_mapping_to_dto).collect(),
    }
}

/// Go `selectedMappingToDTO`：entries 空恒 `[]`。
fn selected_mapping_to_dto(map: &m::SelectedMapping) -> SelectedMappingDto {
    SelectedMappingDto {
        match_type: map.match_type.clone(),
        match_value: map.match_value.clone(),
        entries: map.entries.iter().map(selected_entry_to_dto).collect(),
    }
}

/// Go `selectedEntryToDTO`。
fn selected_entry_to_dto(e: &m::SelectedEntry) -> SelectedEntryDto {
    SelectedEntryDto {
        behavior: e.behavior.clone(),
        action_value: e.action_value.clone(),
        working_dir: e.working_dir.clone(),
        options: RuleOptionsDto {
            copy_to_clipboard: e.options.copy_to_clipboard,
            clear_selection: e.options.clear_selection,
            confirm: e.options.confirm,
        },
    }
}

/// Go `matchRulesToDTO` / MatchType 分支（dto.go:222-232, 241-250）。
fn match_type_to_dto(mt: &m::MatchType) -> MatchTypeDto {
    MatchTypeDto {
        id: mt.id.clone(),
        label: mt.label.clone(),
        label_en: mt.label_en.clone(),
        kind: mt.kind.clone(),
        rules: mt
            .rules
            .iter()
            .map(|r| MatchRuleDto {
                op: r.op.clone(),
                value: r.value.clone(),
            })
            .collect(),
        exts: mt.exts.clone(),
        order: mt.order,
    }
}

/// Go `optionsToDTO`（dto.go:342-429）。
fn options_to_dto(o: &m::Options) -> OptionsDto {
    OptionsDto {
        hide_matrix: o.hide_matrix,
        keyflux_version: o.keyflux_version.clone(),
        window_groups: o
            .window_groups
            .iter()
            .map(|wg| WindowGroupDto {
                id: wg.id,
                name: wg.name.clone(),
                value: wg.value.clone(),
                condition_type: wg.condition_type,
            })
            .collect(),
        mouse: MouseDto {
            keep_mouse_mode: o.mouse.keep_mouse_mode,
            show_tip: o.mouse.show_tip,
            tip_symbol: o.mouse.tip_symbol.clone(),
            delay1: o.mouse.delay1.clone(),
            delay2: o.mouse.delay2.clone(),
            fast_single: o.mouse.fast_single.clone(),
            fast_repeat: o.mouse.fast_repeat.clone(),
            slow_single: o.mouse.slow_single.clone(),
            slow_repeat: o.mouse.slow_repeat.clone(),
        },
        scroll: ScrollDto {
            delay1: o.scroll.delay1.clone(),
            delay2: o.scroll.delay2.clone(),
            once_line_count: o.scroll.once_line_count.clone(),
        },
        command_input_skin: CommandInputSkinDto {
            background_color: o.command_input_skin.background_color.clone(),
            background_opacity: o.command_input_skin.background_opacity.clone(),
            border_width: o.command_input_skin.border_width.clone(),
            border_color: o.command_input_skin.border_color.clone(),
            border_opacity: o.command_input_skin.border_opacity.clone(),
            border_radius: o.command_input_skin.border_radius.clone(),
            corner_color: o.command_input_skin.corner_color.clone(),
            corner_opacity: o.command_input_skin.corner_opacity.clone(),
            gridline_color: o.command_input_skin.gridline_color.clone(),
            gridline_opacity: o.command_input_skin.gridline_opacity.clone(),
            key_color: o.command_input_skin.key_color.clone(),
            key_opacity: o.command_input_skin.key_opacity.clone(),
            hide_animation_duration: o.command_input_skin.hide_animation_duration.clone(),
            window_y_pos: o.command_input_skin.window_y_pos.clone(),
            window_width: o.command_input_skin.window_width.clone(),
            window_shadow_color: o.command_input_skin.window_shadow_color.clone(),
            window_shadow_opacity: o.command_input_skin.window_shadow_opacity.clone(),
            window_shadow_size: o.command_input_skin.window_shadow_size.clone(),
        },
        path_variables: o
            .path_variables
            .iter()
            .map(|pv| PathVariableDto {
                name: pv.name.clone(),
                value: pv.value.clone(),
            })
            .collect(),
        startup: o.startup,
        language: o.language.clone(),
        key_mapping: o.key_mapping.clone(),
        keyboard_layout: o.keyboard_layout.clone(),
        quick_switch: QuickSwitchOptionDto {
            collect_enabled: o.quick_switch.collect_enabled,
            auto_show: o.quick_switch.auto_show,
            auto_jump_open: o.quick_switch.auto_jump_open,
            auto_jump_save: o.quick_switch.auto_jump_save,
            poll_interval_ms: o.quick_switch.poll_interval_ms,
            max_history: o.quick_switch.max_history,
            overlay_rows: o.quick_switch.overlay_rows,
            overlay_rows_compact: o.quick_switch.overlay_rows_compact,
            excluded_prefixes: o.quick_switch.excluded_prefixes.clone(),
        },
        plugins: PluginsOptionDto {
            disabled: o.plugins.disabled.clone(),
        },
        command_font: CommandFontOptionDto {
            source_path: o.command_font.source_path.clone(),
            weight: o.command_font.weight.clone(),
        },
    }
}

// --------------------------------------------------------------------------- DTO → model

/// Go `DTOToConfig`（dto.go:433-467）。
pub fn dto_to_config(dto: &ConfigDto) -> m::Config {
    m::Config {
        keymaps: dto.keymaps.iter().map(dto_to_keymap).collect(),
        options: dto_to_options(&dto.options),
        selected_action: Some(dto_to_selected_action(&dto.selected_action)),
        action_schemes: Vec::new(),
        file_groups: dto
            .file_groups
            .iter()
            .map(|fg| m::FileGroup {
                name: fg.name.clone(),
                label: fg.label.clone(),
                exts: fg.exts.clone(),
            })
            .collect(),
        match_types: dto
            .match_types
            .iter()
            .map(|mt| m::MatchType {
                id: mt.id.clone(),
                label: mt.label.clone(),
                label_en: mt.label_en.clone(),
                kind: mt.kind.clone(),
                rules: mt
                    .rules
                    .iter()
                    .map(|r| m::MatchRule {
                        op: r.op.clone(),
                        value: r.value.clone(),
                    })
                    .collect(),
                exts: mt.exts.clone(),
                order: mt.order,
            })
            .collect(),
        overview_doc_md: dto.overview_doc_md.clone(),
        key_mapping: String::new(),
    }
}

/// Go `dtoToKeymap`。DTO 的 `hotkeys` 为空 ⇒ 模型空 map（Go nil map 的落盘形态
/// 差异见模块头注释）。
fn dto_to_keymap(km: &KeymapDto) -> m::Keymap {
    let mut hotkeys = std::collections::BTreeMap::new();
    for (key, actions) in &km.hotkeys {
        hotkeys.insert(key.clone(), actions.iter().map(dto_to_action).collect());
    }
    m::Keymap {
        id: km.id,
        name: km.name.clone(),
        enable: km.enable,
        hotkey: km.hotkey.clone(),
        parent_id: km.parent_id,
        delay: km.delay,
        disable_at: km.disable_at.clone(),
        hotkeys,
    }
}

/// Go `dtoToAction`。
fn dto_to_action(a: &ActionDto) -> m::Action {
    m::Action {
        window_group_id: a.window_group_id,
        type_id: a.type_id,
        comment: a.comment.clone(),
        hotkey: a.hotkey.clone(),
        keys_to_send: a.keys_to_send.clone(),
        remap_to_key: a.remap_to_key.clone(),
        value_id: a.value_id,
        win_title: a.win_title.clone(),
        target: a.target.clone(),
        args: a.args.clone(),
        working_dir: a.working_dir.clone(),
        run_as_admin: a.run_as_admin,
        run_in_background: a.run_in_background,
        detect_hidden_window: a.detect_hidden_window,
        ahk_code: a.ahk_code.clone(),
        remap_in_hot_if: false,
    }
}

/// Go `dtoToSelectedAction`（dto.go:527-552）：恒非 nil，mappings 空恒空 Vec。
fn dto_to_selected_action(sa: &SelectedActionDto) -> m::SelectedAction {
    m::SelectedAction {
        hotkey: sa.hotkey.clone(),
        enable: sa.enable,
        mappings: sa
            .mappings
            .iter()
            .map(|md| m::SelectedMapping {
                match_type: md.match_type.clone(),
                match_value: md.match_value.clone(),
                entries: md.entries.iter().map(dto_to_selected_entry).collect(),
            })
            .collect(),
    }
}

/// Go `dtoToSelectedEntry`。
fn dto_to_selected_entry(e: &SelectedEntryDto) -> m::SelectedEntry {
    m::SelectedEntry {
        behavior: e.behavior.clone(),
        action_value: e.action_value.clone(),
        working_dir: e.working_dir.clone(),
        options: m::RuleOptions {
            copy_to_clipboard: e.options.copy_to_clipboard,
            clear_selection: e.options.clear_selection,
            confirm: e.options.confirm,
        },
    }
}

/// Go `dtoToOptions`（dto.go:567-652）。
fn dto_to_options(o: &OptionsDto) -> m::Options {
    m::Options {
        hide_matrix: o.hide_matrix,
        keyflux_version: o.keyflux_version.clone(),
        window_groups: o
            .window_groups
            .iter()
            .map(|wg| m::WindowGroup {
                id: wg.id,
                name: wg.name.clone(),
                value: wg.value.clone(),
                condition_type: wg.condition_type,
            })
            .collect(),
        mouse: m::Mouse {
            keep_mouse_mode: o.mouse.keep_mouse_mode,
            show_tip: o.mouse.show_tip,
            tip_symbol: o.mouse.tip_symbol.clone(),
            delay1: o.mouse.delay1.clone(),
            delay2: o.mouse.delay2.clone(),
            fast_single: o.mouse.fast_single.clone(),
            fast_repeat: o.mouse.fast_repeat.clone(),
            slow_single: o.mouse.slow_single.clone(),
            slow_repeat: o.mouse.slow_repeat.clone(),
        },
        scroll: m::Scroll {
            delay1: o.scroll.delay1.clone(),
            delay2: o.scroll.delay2.clone(),
            once_line_count: o.scroll.once_line_count.clone(),
        },
        command_input_skin: m::CommandInputSkin {
            background_color: o.command_input_skin.background_color.clone(),
            background_opacity: o.command_input_skin.background_opacity.clone(),
            border_width: o.command_input_skin.border_width.clone(),
            border_color: o.command_input_skin.border_color.clone(),
            border_opacity: o.command_input_skin.border_opacity.clone(),
            border_radius: o.command_input_skin.border_radius.clone(),
            corner_color: o.command_input_skin.corner_color.clone(),
            corner_opacity: o.command_input_skin.corner_opacity.clone(),
            gridline_color: o.command_input_skin.gridline_color.clone(),
            gridline_opacity: o.command_input_skin.gridline_opacity.clone(),
            key_color: o.command_input_skin.key_color.clone(),
            key_opacity: o.command_input_skin.key_opacity.clone(),
            hide_animation_duration: o.command_input_skin.hide_animation_duration.clone(),
            window_y_pos: o.command_input_skin.window_y_pos.clone(),
            window_width: o.command_input_skin.window_width.clone(),
            window_shadow_color: o.command_input_skin.window_shadow_color.clone(),
            window_shadow_opacity: o.command_input_skin.window_shadow_opacity.clone(),
            window_shadow_size: o.command_input_skin.window_shadow_size.clone(),
        },
        path_variables: o
            .path_variables
            .iter()
            .map(|pv| m::PathVariable {
                name: pv.name.clone(),
                value: pv.value.clone(),
            })
            .collect(),
        startup: o.startup,
        language: o.language.clone(),
        key_mapping: o.key_mapping.clone(),
        keyboard_layout: o.keyboard_layout.clone(),
        quick_switch: m::QuickSwitchOption {
            collect_enabled: o.quick_switch.collect_enabled,
            auto_show: o.quick_switch.auto_show,
            auto_jump_open: o.quick_switch.auto_jump_open,
            auto_jump_save: o.quick_switch.auto_jump_save,
            poll_interval_ms: o.quick_switch.poll_interval_ms,
            max_history: o.quick_switch.max_history,
            overlay_rows: o.quick_switch.overlay_rows,
            overlay_rows_compact: o.quick_switch.overlay_rows_compact,
            excluded_prefixes: o.quick_switch.excluded_prefixes.clone(),
        },
        plugins: m::PluginsOption {
            disabled: o.plugins.disabled.clone(),
        },
        command_font: m::CommandFontOption {
            source_path: o.command_font.source_path.clone(),
            weight: o.command_font.weight.clone(),
        },
    }
}

// --------------------------------------------------------------------------- PUT 绑定

/// Go `c.ShouldBindJSON(&dto)` 的口径模拟：
/// * 只解**首个** JSON 值（gin 用 `json.Decoder.Decode`，尾随垃圾被忽略）；
/// * `null` 字段 = 缺失（Go 把 null 解为零值 —— 复用生成器侧 null 剥除）；
/// * 未知字段忽略（serde 默认）；
/// * 类型不匹配/语法错误 ⇒ `Err`（Go 侧 panic → gin Recovery → 500）。
pub(crate) fn bind_config_dto(body: &[u8]) -> Result<ConfigDto, serde_json::Error> {
    let mut iter = serde_json::Deserializer::from_slice(body).into_iter::<serde_json::Value>();
    let Some(value) = iter.next().transpose()? else {
        // 空请求体：Go 的 Decode 会报 EOF 错误 → panic → 500
        return Err(serde_json::from_str::<serde_json::Value>("").unwrap_err());
    };
    let mut value = value;
    m::strip_null_fields(&mut value);
    serde_json::from_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::model::Config;

    /// 空（零值）配置的 GET 响应逐字节锁定：字段序、空集合恒 `[]`、
    /// omitempty（overviewDocMd 缺席）、plugins.disabled 恒 `[]`。
    /// Go 侧对应 `ConfigToDTO` 的输出（差异仅 nil-slice → `null` vs `[]`，见模块头）。
    #[test]
    fn empty_config_dto_serializes_byte_exact() {
        let json = marshal_go_json(&config_to_dto(&Config::default()));
        assert_eq!(
            json,
            "{\"keymaps\":[],\"options\":{\"hideMatrix\":false,\"keyfluxVersion\":\"\",\
             \"windowGroups\":[],\"mouse\":{\"keepMouseMode\":false,\"showTip\":false,\
             \"tipSymbol\":\"\",\"delay1\":\"\",\"delay2\":\"\",\"fastSingle\":\"\",\
             \"fastRepeat\":\"\",\"slowSingle\":\"\",\"slowRepeat\":\"\"},\
             \"scroll\":{\"delay1\":\"\",\"delay2\":\"\",\"onceLineCount\":\"\"},\
             \"commandInputSkin\":{\"backgroundColor\":\"\",\"backgroundOpacity\":\"\",\
             \"borderWidth\":\"\",\"borderColor\":\"\",\"borderOpacity\":\"\",\
             \"borderRadius\":\"\",\"cornerColor\":\"\",\"cornerOpacity\":\"\",\
             \"gridlineColor\":\"\",\"gridlineOpacity\":\"\",\"keyColor\":\"\",\
             \"keyOpacity\":\"\",\"hideAnimationDuration\":\"\",\"windowYPos\":\"\",\
             \"windowWidth\":\"\",\"windowShadowColor\":\"\",\"windowShadowOpacity\":\"\",\
             \"windowShadowSize\":\"\"},\"pathVariables\":[],\"startup\":false,\
             \"language\":\"\",\"keyMapping\":\"\",\"keyboardLayout\":\"\",\
             \"quickSwitch\":{\"collectEnabled\":false,\"autoShow\":false,\
             \"autoJumpOpen\":false,\"autoJumpSave\":false,\"pollIntervalMs\":0,\
             \"maxHistory\":0,\"overlayRows\":0,\"overlayRowsCompact\":0,\
             \"excludedPrefixes\":[]},\"plugins\":{\"disabled\":[]},\
             \"commandFont\":{\"sourcePath\":\"\",\"weight\":\"\"}},\
             \"selectedAction\":{\"hotkey\":\"\",\"enable\":false,\"mappings\":[]},\
             \"fileGroups\":[],\"matchTypes\":[]}"
        );
    }

    /// gin c.JSON 的 HTML 转义：字符串内的 `<` `>` `&` 逐字节替换为 `\u003c` 等；
    /// 非 ASCII（含 emoji）保持原始 UTF-8；JSON 结构与键名不受影响。
    #[test]
    fn go_html_escape_matches_gin_c_json() {
        let mut config = Config::default();
        config.keymaps.push(m::Keymap {
            id: 1,
            name: "狗🐶<b>&\"x\"".into(),
            ..Default::default()
        });
        config.keymaps[0].hotkeys.insert(
            "*a".into(),
            vec![m::Action {
                type_id: 3,
                comment: "<a&b>c".into(),
                ..Default::default()
            }],
        );
        let json = marshal_go_json(&config_to_dto(&config));
        assert!(
            json.contains("\"name\":\"狗🐶\\u003cb\\u003e\\u0026\\\"x\\\"\""),
            "{json}"
        );
        assert!(
            json.contains("\"comment\":\"\\u003ca\\u0026b\\u003ec\""),
            "{json}"
        );
        // omitempty：零值动作只保留两个恒输出键
        assert!(
            json.contains("{\"windowGroupID\":0,\"actionTypeID\":3,\"comment\":"),
            "{json}"
        );
    }

    /// 转义器对已转义序列（`\"`）与 U+2028/2029 的处理。
    #[test]
    fn html_escape_keeps_quote_escapes_and_handles_line_separators() {
        let json = r#"{"a":"x\u2028y","b":"say \"hi\" <z>"}"#;
        assert_eq!(
            go_html_escape_json(json),
            r#"{"a":"x\u2028y","b":"say \"hi\" \u003cz\u003e"}"#
        );
    }

    /// omitempty 全谱：零值 Action / MatchType / WindowGroup / SelectedEntry 的键集。
    #[test]
    fn omitempty_omits_zero_fields() {
        let mut config = Config::default();
        config.match_types.push(m::MatchType {
            id: "t".into(),
            label: "T".into(),
            kind: "text".into(),
            ..Default::default()
        });
        config.options.window_groups.push(m::WindowGroup {
            id: 7,
            name: "g".into(),
            ..Default::default()
        });
        config.selected_action = Some(m::SelectedAction {
            hotkey: "^!s".into(),
            enable: true,
            mappings: vec![m::SelectedMapping {
                match_type: "textType".into(),
                match_value: "url".into(),
                entries: vec![m::SelectedEntry {
                    behavior: "copy".into(),
                    options: m::RuleOptions {
                        confirm: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }],
            }],
        });
        let json = marshal_go_json(&config_to_dto(&config));
        assert!(
            json.contains("\"matchTypes\":[{\"id\":\"t\",\"label\":\"T\",\"kind\":\"text\"}]"),
            "{json}"
        );
        assert!(
            json.contains("\"windowGroups\":[{\"id\":7,\"name\":\"g\"}]"),
            "{json}"
        );
        assert!(
            json.contains(
                "\"entries\":[{\"behavior\":\"copy\",\"options\":\
                           {\"copyToClipboard\":false,\"clearSelection\":false,\"confirm\":true}}]"
            ),
            "{json}"
        );
    }

    /// DTO→model→DTO 往返稳定（含 omitempty 字段、BTreeMap 键序、selectedAction）。
    #[test]
    fn dto_model_roundtrip_stable() {
        let raw = r##"{
            "keymaps":[{"id":1,"name":"主","enable":true,"hotkey":"*a","parentID":0,
                "delay":5,"disableAt":"x.exe","hotkeys":{"b":[{"windowGroupID":2,
                "actionTypeID":6,"remapToKey":"c","actionValueID":9}]}},
                {"id":2,"name":"空热键表","hotkeys":{}}],
            "options":{"hideMatrix":true,"keyfluxVersion":"9.9","windowGroups":[
                {"id":1,"name":"g","value":"a.exe","conditionType":1}],
                "pathVariables":[{"name":"P","value":"V"}],
                "quickSwitch":{"collectEnabled":true,"excludedPrefixes":["a","b"]},
                "plugins":{"disabled":["p1"]},
                "commandFont":{"sourcePath":"C:/f.ttf","weight":"bold"}},
            "selectedAction":{"hotkey":"^!s","enable":true,"mappings":[
                {"matchType":"fileExt","matchValue":"jpg,png","entries":[
                    {"behavior":"open","actionValue":"%selected%","workingDir":"D:/",
                     "options":{"copyToClipboard":true}}]}]},
            "fileGroups":[{"name":"image","label":"图片","exts":["jpg","png"]}],
            "matchTypes":[{"id":"netdisk","label":"网盘","labelEn":"Netdisk",
                "kind":"text","rules":[{"op":"contains","value":"pan"}],"order":3}],
            "overviewDocMd":"# 指南"
        }"##;
        let dto: ConfigDto = serde_json::from_str(raw).unwrap();
        let config = dto_to_config(&dto);
        let dto2 = config_to_dto(&config);
        let config2 = dto_to_config(&dto2);
        let json1 = serde_json::to_string(&dto).unwrap();
        let json2 = serde_json::to_string(&dto2).unwrap();
        assert_eq!(json1, json2, "DTO 序列化往返应稳定");
        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            serde_json::to_string(&config2).unwrap()
        );
        // hotkeys 键按字典序（BTreeMap）输出 —— Go map 键字典序契约
        let out = marshal_go_json(&dto2);
        let pos_b = out.find("\"keymaps\":[{\"id\":1").unwrap();
        let _ = pos_b;
        assert!(out.contains("\"hotkeys\":{\"b\":["), "{out}");
        // selectedAction 恒输出（模型侧恒 Some）
        assert!(config.selected_action.is_some());
        assert!(
            config.selected_action.as_ref().unwrap().mappings[0]
                .entries
                .first()
                .unwrap()
                .options
                .copy_to_clipboard
        );
    }

    /// PUT 绑定口径：null = 缺失（零值）、未知字段忽略、尾随垃圾忽略（gin Decode 语义）。
    #[test]
    fn bind_config_dto_tolerates_null_and_trailing_garbage() {
        let dto = bind_config_dto(
            br#"{"keymaps":null,"options":null,"selectedAction":null,
                 "unknownField":123,"fileGroups":null} {"trailing":1}"#,
        )
        .expect("null 与尾随垃圾都应容忍");
        assert!(dto.keymaps.is_empty());
        assert!(dto.file_groups.is_empty());
        assert!(dto.selected_action.mappings.is_empty());

        // 语法错误 → Err（Go: panic → 500）
        assert!(bind_config_dto(br#"{"keymaps":"oops"}"#).is_err());
        // 空 body → Err
        assert!(bind_config_dto(b"").is_err());
        // 数字类型不匹配 → Err
        assert!(bind_config_dto(br#"{"keymaps":[{"id":"x"}]}"#).is_err());
    }

    /// Go json 对字符串内 U+2028 之外的裸 UTF-8 不转义（中文/emoji 直出）。
    #[test]
    fn non_ascii_stays_raw() {
        let json = go_html_escape_json("{\"a\":\"链接 🐶\"}");
        assert_eq!(json, "{\"a\":\"链接 🐶\"}");
    }
}
