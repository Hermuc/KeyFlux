//! `data/config.json` 的 DTO。
//!
//! **权威蓝本**：Go `config-server/internal/script/model/types.go`
//! 与 C# `config-ui-avalonia/Models/ConfigModels.cs`（561 行，已逐字核对）。
//!
//! 铁律：
//! 1. 字段顺序与 json 名**不得改动**；`*ID` 类键（`parentID` / `actionTypeID` /
//!    `actionValueID` / `windowGroupID`）与 camelCase 推导不同，必须显式 `rename`。
//! 2. 新增字段须 **Go / C# / Rust 三端同批**，否则 Go 全量落盘会静默剥掉它。
//! 3. `[JsonIgnore]` 的前端专用字段（`IsNew` / `IsEmpty`）在 Rust 用 `#[serde(skip)]`。
//! 4. C# 用「代理属性 + WhenWritingNull」实现 Go 的 `omitempty`：仅
//!    `SelectedEntry.actionValue` / `workingDir` 两处空串需**省略键**，用
//!    `skip_serializing_if = "String::is_empty"` 对齐。

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

/// 容忍显式 `null`：映射为 `T::default()`（对应 C# 的 `?? new()` 归一）。
fn de_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// 对应 Go `struct Config`；`GET/PUT /config` 的载荷。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub keymaps: Vec<Keymap>,

    #[serde(default)]
    pub options: Options,

    /// 选中动作单键分发（方案 D）：全链路恒对象。
    #[serde(default)]
    pub selected_action: SelectedAction,

    /// omitempty：文件分组快捷填充数据，缺失时为空列表。
    #[serde(default)]
    pub file_groups: Vec<FileGroup>,

    /// 自定义匹配类型（方案 C7），json tag 与 Go `MatchTypeDTO` 逐字一致。
    #[serde(default)]
    pub match_types: Vec<MatchType>,

    /// omitempty：自定义使用指南页 Markdown，缺失时为 `""`。
    #[serde(default)]
    pub overview_doc_md: String,
}

/// 对应 Go `struct Keymap`。单个键盘映射（一页按键矩阵）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keymap {
    #[serde(default)]
    pub id: i32,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub enable: bool,

    #[serde(default)]
    pub hotkey: String,

    /// json tag 为 `parentID`（非 `parentId`）。
    #[serde(default, rename = "parentID")]
    pub parent_id: i32,

    #[serde(default)]
    pub delay: i32,

    #[serde(default)]
    pub disable_at: String,

    /// 前端专用标记（对应 Vue `Keymap.isNew`，不参与序列化）。
    #[serde(skip)]
    pub is_new: bool,

    /// 按键 -> 动作列表（键如 `"a"`、`"*1"`）。
    ///
    /// ⚠️ Rust 侧用 `BTreeMap`（有序）而非 C# 的哈希表：Go 侧反序列化到 `map`、
    /// 生成期迭代顺序本就随机，故语义等价，但能让我们产出的 JSON **逐字节可复现**。
    #[serde(default)]
    pub hotkeys: BTreeMap<String, Vec<Action>>,
}

/// 对应 Go `struct SelectedAction`。选中动作单键分发（方案 D）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedAction {
    #[serde(default)]
    pub hotkey: String,

    #[serde(default)]
    pub enable: bool,

    /// 有序映射列表（行序 = 匹配优先级）。
    #[serde(default)]
    pub mappings: Vec<SelectedMapping>,
}

/// 对应 Go `struct SelectedMapping`。一个匹配前提桶。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedMapping {
    /// `"fileExt"` | `"textType"`。
    #[serde(default)]
    pub match_type: String,

    #[serde(default)]
    pub match_value: String,

    /// 1..9 项，顺序即菜单序号。
    #[serde(default)]
    pub entries: Vec<SelectedEntry>,
}

/// 对应 Go `struct SelectedEntry`。菜单项。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedEntry {
    /// 行为库 ID（内置 11 个基础动作 ID 或用户行为包 ID）。
    #[serde(default)]
    pub behavior: String,

    /// 命令模板 / 目标值；空串 = 用行为包默认（**空串时省略该键**，对齐 Go omitempty）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action_value: String,

    /// 工作目录；空串 = 不设置（**空串时省略该键**）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub working_dir: String,

    #[serde(default)]
    pub options: RuleOptions,
}

/// 对应 Go `struct RuleOptions`。行为执行三开关。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleOptions {
    #[serde(default)]
    pub copy_to_clipboard: bool,

    #[serde(default)]
    pub clear_selection: bool,

    #[serde(default)]
    pub confirm: bool,
}

/// 对应 Go `struct FileGroup`。文件分组（前端快捷填充数据，引擎不感知）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileGroup {
    /// 分组标识（英文，如 `image`）。
    #[serde(default)]
    pub name: String,

    /// 中文显示名（如 `图片`）。
    #[serde(default)]
    pub label: String,

    /// 后缀列表（不含点，如 `["jpg","jpeg"]`）。
    #[serde(default)]
    pub exts: Vec<String>,
}

/// 对应 Go `struct MatchRule`：自定义文本类型的判定谓词。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRule {
    /// 算子 `equals`/`prefix`/`suffix`/`contains`（仅 ASCII 大小写不敏感）。
    #[serde(default)]
    pub op: String,

    /// 算子右值（Trim 非空，长度 ≤ 256）。
    #[serde(default)]
    pub value: String,
}

/// 对应 Go `struct MatchType`：自定义匹配类型（`kind = text | fileExt`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchType {
    /// 稳定标识 `^[a-z][a-z0-9_]{0,23}$`；引用命名空间 `type:<id>`。
    #[serde(default)]
    pub id: String,

    /// 用户数据（不进 i18n）；缺省回退显示 id。
    #[serde(default)]
    pub label: String,

    /// 可选英文显示名。
    #[serde(default)]
    pub label_en: String,

    /// `"text"` | `"fileExt"`。
    #[serde(default)]
    pub kind: String,

    /// `kind = text` 必填：判定谓词清单（≥1 条，OR 语义）。
    #[serde(default)]
    pub rules: Vec<MatchRule>,

    /// `kind = fileExt` 时使用。
    #[serde(default)]
    pub exts: Vec<String>,

    /// 展示序。
    #[serde(default)]
    pub order: i32,
}

/// 对应 Go `struct Action`。按键绑定的单个动作。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// json tag 为 `windowGroupID`。
    #[serde(default, rename = "windowGroupID")]
    pub window_group_id: i32,

    /// json tag 为 `actionTypeID`（C# 属性名为 TypeId）。
    #[serde(default, rename = "actionTypeID")]
    pub type_id: i32,

    #[serde(default)]
    pub comment: String,

    #[serde(default)]
    pub hotkey: String,

    #[serde(default)]
    pub keys_to_send: String,

    #[serde(default)]
    pub remap_to_key: String,

    /// json tag 为 `actionValueID`。
    #[serde(default, rename = "actionValueID")]
    pub value_id: i32,

    #[serde(default)]
    pub win_title: String,

    #[serde(default)]
    pub target: String,

    #[serde(default)]
    pub args: String,

    #[serde(default)]
    pub working_dir: String,

    #[serde(default)]
    pub run_as_admin: bool,

    #[serde(default)]
    pub run_in_background: bool,

    #[serde(default)]
    pub detect_hidden_window: bool,

    #[serde(default)]
    pub ahk_code: String,

    /// 前端专用「未配置」哨兵（对应 Vue `Action.isEmpty`，Go 无此字段）。
    #[serde(skip)]
    pub is_empty: bool,
}

/// 对应 Go `struct Options`。全局选项。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    #[serde(default)]
    pub hide_matrix: bool,

    #[serde(default)]
    pub keyflux_version: String,

    #[serde(default)]
    pub window_groups: Vec<WindowGroup>,

    #[serde(default)]
    pub mouse: Mouse,

    #[serde(default)]
    pub scroll: Scroll,

    #[serde(default)]
    pub command_input_skin: CommandInputSkin,

    #[serde(default)]
    pub path_variables: Vec<PathVariable>,

    #[serde(default)]
    pub startup: bool,

    #[serde(default)]
    pub language: String,

    #[serde(default)]
    pub key_mapping: String,

    #[serde(default)]
    pub keyboard_layout: String,

    #[serde(default)]
    pub quick_switch: QuickSwitchOption,

    /// Go 侧为指针 + omitempty ⇒ 容忍缺失与显式 `null`。
    #[serde(default, deserialize_with = "de_null_default")]
    pub plugins: PluginsOption,

    /// 同上。
    #[serde(default, deserialize_with = "de_null_default")]
    pub command_font: CommandFontOption,
}

/// 对应 Go `struct CommandFontOption`。命令输入框字体配置段。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFontOption {
    /// 用户选择的字体文件绝对路径（空 = 未自定义）。
    #[serde(default)]
    pub source_path: String,

    /// 字重档位 `thin`/`light`/`regular`/`semibold`/`bold`。
    #[serde(default)]
    pub weight: String,
}

/// 对应 Go `struct PluginsOption`。插件注册表（只记「已停用」+ 内置墓碑）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginsOption {
    #[serde(default)]
    pub disabled: Vec<String>,
    /// 用户主动删除的**随包内置**插件 ID 墓碑（2026-10-02 P4）。
    /// 🔴 `skip_serializing_if` 与 Go `omitempty` 同构：空（常态）时字段消失，
    /// GET /config 产物对旧基线逐字节等价（裸字段会产生 null vs [] 分歧）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

/// 对应 Go `struct QuickSwitchOption`。
///
/// 🔴 2026-10-02 P5 起 **deprecated**（同 Go 侧注释）：配置已迁
/// `plugin-settings.json`，本段仅保留序列化与读取兼容（回滚安全）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSwitchOption {
    #[serde(default)]
    pub collect_enabled: bool,

    #[serde(default)]
    pub auto_show: bool,

    #[serde(default)]
    pub auto_jump_open: bool,

    #[serde(default)]
    pub auto_jump_save: bool,

    #[serde(default)]
    pub poll_interval_ms: i32,

    #[serde(default)]
    pub max_history: i32,

    #[serde(default)]
    pub overlay_rows: i32,

    #[serde(default)]
    pub overlay_rows_compact: i32,

    #[serde(default)]
    pub excluded_prefixes: Vec<String>,
}

/// 对应 Go `struct WindowGroup`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowGroup {
    #[serde(default)]
    pub id: i32,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub value: String,

    #[serde(default)]
    pub condition_type: i32,
}

/// 对应 Go `struct Mouse`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mouse {
    #[serde(default)]
    pub keep_mouse_mode: bool,

    #[serde(default)]
    pub show_tip: bool,

    #[serde(default)]
    pub tip_symbol: String,

    #[serde(default)]
    pub delay1: String,

    #[serde(default)]
    pub delay2: String,

    #[serde(default)]
    pub fast_single: String,

    #[serde(default)]
    pub fast_repeat: String,

    #[serde(default)]
    pub slow_single: String,

    #[serde(default)]
    pub slow_repeat: String,
}

/// 对应 Go `struct Scroll`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scroll {
    #[serde(default)]
    pub delay1: String,

    #[serde(default)]
    pub delay2: String,

    #[serde(default)]
    pub once_line_count: String,
}

/// 对应 Go `struct PathVariable`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathVariable {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub value: String,
}

/// 对应 Go `struct CommandInputSkin`（18 个字段均为字符串）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInputSkin {
    #[serde(default)]
    pub background_color: String,
    #[serde(default)]
    pub background_opacity: String,
    #[serde(default)]
    pub border_width: String,
    #[serde(default)]
    pub border_color: String,
    #[serde(default)]
    pub border_opacity: String,
    #[serde(default)]
    pub border_radius: String,
    #[serde(default)]
    pub corner_color: String,
    #[serde(default)]
    pub corner_opacity: String,
    #[serde(default)]
    pub gridline_color: String,
    #[serde(default)]
    pub gridline_opacity: String,
    #[serde(default)]
    pub key_color: String,
    #[serde(default)]
    pub key_opacity: String,
    #[serde(default)]
    pub hide_animation_duration: String,
    #[serde(default)]
    pub window_y_pos: String,
    #[serde(default)]
    pub window_width: String,
    #[serde(default)]
    pub window_shadow_color: String,
    #[serde(default)]
    pub window_shadow_opacity: String,
    #[serde(default)]
    pub window_shadow_size: String,
}
