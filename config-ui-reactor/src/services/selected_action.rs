//! 选中动作页（`SelectedActionPageViewModel` / `TypeCardVm` / `BehaviorCatalog` 的纯逻辑等价物）。
//!
//! 权威来源：
//! * `ViewModels/SelectedActionPageViewModel.cs`（页面编排）
//! * `ViewModels/TypeCardVm.cs`（聚合卡 toggle 构建 / transient 详情 / 删除）
//! * `Services/BehaviorCatalog.cs`（行为目录覆盖/默认/显示名推导 —— Rust 侧改为
//!   显式传 `&Catalog` 的纯函数，不再用 C# 的静态全局）
//! * `Services/ActionSchemeCatalog.cs`（TextTypes 镜像 / NormalizeExts / SameExts）
//!
//! ⚠️ 与 C# 的口径差异（均已在文档登记）：
//! * 行为目录在组件 state 里持有快照（C# 是静态 `BehaviorCatalog`），页面重开即重拉。
//! * 覆盖比较全部**忽略大小写**（对齐 C# `OrdinalIgnoreCase`）。

use crate::models::{BehaviorAppliesTo, BehaviorPack, Config, SelectedMapping};
use crate::services::i18n;

/// matchType 分区值：文件后缀。
pub const MATCH_FILE_EXT: &str = "fileExt";
/// matchType 分区值：文本特征。
pub const MATCH_TEXT_TYPE: &str = "textType";

/// 内置文本特征 —— 注册表真源（2026-10-06 Go 后端退役后由本镜像承载；
/// 原 Go 真源 `textfeatures.go` 的 value/顺序由此继承。自定义走 `Config.MatchTypes`。
/// 与 AHK TextFeatureSpecs 的对账由 tools/texttype_conformance.py 强制）。
/// 顺序 = 界面 toggle 顺序，**兜底特征 plain 恒居末位**（TextFeatureRegistryConsistencyTests 钉死）。
pub const TEXT_TYPES: [(&str, &str); 5] = [
    ("url", "1059"),
    ("path", "1060"),
    ("magnet", "1061"),
    ("bilibili", "2580"),
    ("plain", "1062"),
];

/// 兜底特征值（注册表末位）。
pub const FALLBACK_TEXT_TYPE: &str = "plain";

/// 引擎级无参基础动作集（对齐 AHK `ExecuteActionRule` 分支；不随用户包增减）。
pub const BASE_ACTION_NO_VALUE: [&str; 4] =
    ["open_url", "open_path", "open_folder", "magnet_download"];

// ---------------------------------------------------------------- 行为目录快照

/// 行为目录快照（内置在前、用户在后；各自 ID 字典序由后端保证 —— 决定 `default_for` 稳定序）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub builtin: Vec<BehaviorPack>,
    pub user: Vec<BehaviorPack>,
}

impl Catalog {
    /// 合并视图（内置在前）。
    pub fn packs(&self) -> impl Iterator<Item = &BehaviorPack> {
        self.builtin.iter().chain(self.user.iter())
    }

    pub fn find(&self, id: &str) -> Option<&BehaviorPack> {
        self.packs().find(|pack| pack.id == id)
    }

    /// 行为显示名：en 语境优先 `nameEn`；未知 ID 回退原值（脏值恒可见口径）。
    pub fn label_for(&self, id: &str) -> String {
        match self.find(id) {
            None => id.to_string(),
            Some(pack) => {
                if i18n::language() == i18n::Lang::En {
                    pack.name_en
                        .as_deref()
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| pack.name.clone())
                } else {
                    pack.name.clone()
                }
            }
        }
    }

    /// 行为提示（description；缺失回退空串）。
    pub fn hint_for(&self, id: &str) -> String {
        self.find(id)
            .and_then(|pack| pack.description.clone())
            .unwrap_or_default()
    }

    /// 是否无参行为：内置语义集，或 builtin 包未声明默认命令模板。
    pub fn is_no_value(&self, id: &str) -> bool {
        if BASE_ACTION_NO_VALUE.contains(&id) {
            return true;
        }
        match self.find(id) {
            None => false,
            Some(pack) => {
                pack.entry.kind.eq_ignore_ascii_case("builtin")
                    && pack
                        .entry
                        .params
                        .as_ref()
                        .and_then(|params| params.action_value.as_deref())
                        .map(str::is_empty)
                        .unwrap_or(true)
            }
        }
    }

    /// 切换到该行为时的默认命令模板（包声明；无则空串）。
    pub fn default_template_for(&self, id: &str) -> String {
        self.find(id)
            .and_then(|pack| pack.entry.params.as_ref())
            .and_then(|params| params.action_value.clone())
            .unwrap_or_default()
    }

    /// 行为展开后的基础动作 ID（内置 ID 直通；用户包取 entry.action；未知原样）。
    pub fn base_action_of(&self, id: &str) -> String {
        if BASE_ACTION_NO_VALUE.contains(&id) || self.find(id).is_none() {
            return id.to_string();
        }
        match self.find(id) {
            Some(pack) if pack.entry.kind.eq_ignore_ascii_case("builtin") => {
                pack.entry.action.clone().unwrap_or_else(|| id.to_string())
            }
            _ => id.to_string(),
        }
    }
}

// ---------------------------------------------------------------- 覆盖推导

/// 规则值集展开：textType 单值（trim + 小写）；fileExt 逗号分隔（`normalize_exts` 语义）。
/// 条件值为空时按通用文件集处理（与旧 `FileActions` 回退口径一致，仅用于展示过滤）。
pub fn rule_values(match_type: &str, match_value: &str) -> Vec<String> {
    if match_type.eq_ignore_ascii_case(MATCH_TEXT_TYPE) {
        let value = match_value.trim().to_lowercase();
        return if value.is_empty() {
            Vec::new()
        } else {
            vec![value]
        };
    }
    let exts = normalize_exts(match_value);
    if exts.is_empty() {
        vec!["*".to_string()]
    } else {
        exts
    }
}

/// 是否 `"type:"` 自定义引用（与 Go `behaviors.IsCustomRef` 同约定；仅纯前缀判断）。
fn is_custom_ref(value: &str) -> bool {
    value.starts_with("type:")
}

fn trimmed_eq(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b)
}

fn ext_hit(exts: &[String], value: &str) -> bool {
    exts.iter()
        .any(|ext| trimmed_eq(ext.trim_matches('.'), value))
}

fn entry_covers(entry: &BehaviorAppliesTo, match_type: &str, values: &[String]) -> bool {
    if !entry.kind.eq_ignore_ascii_case(match_type) {
        return false;
    }
    if entry.kind.eq_ignore_ascii_case(MATCH_TEXT_TYPE) {
        if values.len() != 1 {
            return false;
        }
        let value = &values[0];
        // 精确匹配（专属前提），或 plain 通配前提覆盖任意自定义文本类型引用（两段式覆盖，
        // 留桩不死胡同 —— 对齐 Go behaviors.entryCovers 的 plain && IsCustomRef 规则）。
        return entry
            .value
            .as_deref()
            .map(|v| trimmed_eq(v, value))
            .unwrap_or(false)
            || entry
                .value
                .as_deref()
                .map(|v| trimmed_eq(v, FALLBACK_TEXT_TYPE) && is_custom_ref(value))
                .unwrap_or(false);
    }
    let exts = entry.exts.as_deref().unwrap_or_default();
    let pack_wildcard = exts.iter().any(|ext| ext.trim() == "*");
    for value in values {
        if value == "*" {
            if !pack_wildcard {
                return false; // 任意文件规则只能依赖通配前提
            }
            continue;
        }
        if !pack_wildcard && !ext_hit(exts, value) {
            return false;
        }
    }
    true
}

/// 精确覆盖（不含 plain 继承 / `*` 通配）：用于「专属 / 继承」分段。
fn entry_covers_exact(entry: &BehaviorAppliesTo, match_type: &str, values: &[String]) -> bool {
    if !entry.kind.eq_ignore_ascii_case(match_type) {
        return false;
    }
    if entry.kind.eq_ignore_ascii_case(MATCH_TEXT_TYPE) {
        return values.len() == 1
            && entry
                .value
                .as_deref()
                .map(|v| trimmed_eq(v, &values[0]))
                .unwrap_or(false);
    }
    let exts = entry.exts.as_deref().unwrap_or_default();
    if exts.iter().any(|ext| ext.trim() == "*") {
        return false;
    }
    for value in values {
        if value == "*" {
            return false;
        }
        if !ext_hit(exts, value) {
            return false;
        }
    }
    true
}

/// 行为是否覆盖规则前提：fileExt 要求规则值集 ⊆ 前提集（`"*"` 覆盖任意）。
pub fn covers(pack: &BehaviorPack, match_type: &str, values: &[String]) -> bool {
    pack.applies_to
        .iter()
        .any(|entry| entry_covers(entry, match_type, values))
}

fn covers_dedicated(pack: &BehaviorPack, match_type: &str, values: &[String]) -> bool {
    pack.applies_to
        .iter()
        .any(|entry| entry_covers_exact(entry, match_type, values))
}

/// 覆盖某前提的行为列表，展示序：专属前提（非通配）在前、通配在后，各自保持目录序。
pub fn covering<'a>(
    catalog: &'a Catalog,
    match_type: &str,
    match_value: &str,
) -> Vec<&'a BehaviorPack> {
    let values = rule_values(match_type, match_value);
    let mut specific = Vec::new();
    let mut generic = Vec::new();
    for pack in catalog.packs() {
        if !covers(pack, match_type, &values) {
            continue;
        }
        if covers_dedicated(pack, match_type, &values) {
            specific.push(pack);
        } else {
            generic.push(pack);
        }
    }
    specific.extend(generic);
    specific
}

/// 某前提是否拥有「专属」行为（精确 appliesTo 命中；自定义文本类型引用由 plain 继承段兜底）。
pub fn has_dedicated_behavior_for(catalog: &Catalog, match_type: &str, match_value: &str) -> bool {
    let values = rule_values(match_type, match_value);
    catalog
        .packs()
        .any(|pack| covers_dedicated(pack, match_type, &values))
}

/// 前提桶默认行为：default 标记优先（目录序），回退第一条覆盖包。
pub fn default_for(catalog: &Catalog, match_type: &str, match_value: &str) -> Option<String> {
    let values = rule_values(match_type, match_value);
    let mut first: Option<&BehaviorPack> = None;
    for pack in catalog.packs() {
        for entry in &pack.applies_to {
            if !entry_covers(entry, match_type, &values) {
                continue;
            }
            if entry.is_default {
                return Some(pack.id.clone());
            }
            first.get_or_insert(pack);
            break;
        }
    }
    first.map(|pack| pack.id.clone())
}

// ---------------------------------------------------------------- 后缀归一化

/// 后缀列表归一化：逗号/分号（中英文）分隔 → trim + 两端去点 → 去重保序。
pub fn normalize_exts(match_value: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    if match_value.trim().is_empty() {
        return result;
    }
    let mut seen: Vec<String> = Vec::new(); // 大小写不敏感去重（保序）
    for token in match_value
        .split([',', '，', '、', ';', '；'])
        .map(str::trim)
        .map(|token| token.trim_matches('.'))
    {
        if token.is_empty() {
            continue;
        }
        let key = token.to_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            result.push(token.to_string());
        }
    }
    result
}

/// 两个后缀集合是否等价（两侧先各自归一化再比较，忽略大小写/顺序/两端点）。
pub fn same_exts(a: &[String], b: &[String]) -> bool {
    let norm = |exts: &[String]| -> Vec<String> {
        let mut set: Vec<String> = normalize_exts(&exts.join(","))
            .into_iter()
            .map(|ext| ext.to_lowercase())
            .collect();
        set.sort();
        set
    };
    norm(a) == norm(b)
}

// ---------------------------------------------------------------- 聚合卡 toggles

/// 聚合卡内的一行类型 toggle。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeToggle {
    /// 类型标识（内置特征值 / `group:<Name>` / `type:<id>` / `orphan:<n>`）。
    pub id: String,
    /// 显示文案。
    pub label: String,
}

/// 自定义匹配类型的显示标签：en 语境优先 labelEn，否则 label；二者皆空回退 id（用户数据不进 i18n）。
fn custom_type_label(id: &str, label: &str, label_en: &str) -> String {
    if i18n::language() == i18n::Lang::En && !label_en.is_empty() {
        label_en.to_string()
    } else if !label.is_empty() {
        label.to_string()
    } else {
        id.to_string()
    }
}

/// 内置文本特征的显示名（走 i18n）；未知值返回 `None`（自定义/孤儿由调用方处理）。
pub fn builtin_text_type_label(value: &str) -> Option<String> {
    TEXT_TYPES
        .iter()
        .find(|(candidate, _)| *candidate == value)
        .map(|(_, key)| i18n::t(key))
}

/// 分区标题（「文件后缀」/「文本特征」）。
pub fn match_type_label(match_type: &str) -> String {
    let key = match match_type {
        MATCH_FILE_EXT => "1031",
        MATCH_TEXT_TYPE => "1032",
        _ => return match_type.to_string(),
    };
    i18n::t(key)
}

/// 构建卡内类型 toggle 集合：
/// * 文本卡 = 内置 `TextTypes` + `match_types` 中 `kind == "text"`（引用 `type:<id>`）；
/// * 文件卡 = 全部 `file_groups`（引用 `group:<name>`）+ `kind == "fileExt"` 的自定义类型；
/// * 任何已配置但不被上述规则覆盖的 mapping（孤儿）也补一个 toggle，避免存量数据被隐藏。
pub fn build_toggles(config: &Config, match_type: &str) -> Vec<TypeToggle> {
    let mut toggles = Vec::new();
    if match_type == MATCH_TEXT_TYPE {
        for (value, key) in TEXT_TYPES {
            toggles.push(TypeToggle {
                id: value.to_string(),
                label: i18n::t(key),
            });
        }
        for mt in config.match_types.iter().filter(|mt| mt.kind == "text") {
            toggles.push(TypeToggle {
                id: format!("type:{}", mt.id),
                label: custom_type_label(&mt.id, &mt.label, &mt.label_en),
            });
        }
    } else {
        for group in &config.file_groups {
            toggles.push(TypeToggle {
                id: format!("group:{}", group.name),
                label: group.label.clone(),
            });
        }
        for mt in config.match_types.iter().filter(|mt| mt.kind == "fileExt") {
            toggles.push(TypeToggle {
                id: format!("type:{}", mt.id),
                label: custom_type_label(&mt.id, &mt.label, &mt.label_en),
            });
        }
    }

    // 孤儿：已配置但不被上述规则覆盖的 mapping
    let mut orphan = 0;
    for mapping in config
        .selected_action
        .mappings
        .iter()
        .filter(|mapping| mapping.match_type == match_type)
    {
        if is_covered(config, match_type, mapping) {
            continue;
        }
        toggles.push(TypeToggle {
            id: format!("orphan:{orphan}"),
            label: mapping.match_value.clone(),
        });
        orphan += 1;
    }
    toggles
}

/// mapping 是否被 toggle 规则覆盖（文本卡：内置特征或自定义引用；文件卡：自定义引用或分组等价后缀）。
fn is_covered(config: &Config, match_type: &str, mapping: &SelectedMapping) -> bool {
    if match_type == MATCH_TEXT_TYPE {
        if TEXT_TYPES
            .iter()
            .any(|(value, _)| *value == mapping.match_value)
        {
            return true;
        }
        return mapping.match_value.starts_with("type:")
            && config
                .match_types
                .iter()
                .any(|mt| mt.kind == "text" && format!("type:{}", mt.id) == mapping.match_value);
    }
    // fileExt
    if mapping.match_value.starts_with("type:") {
        return config
            .match_types
            .iter()
            .any(|mt| mt.kind == "fileExt" && format!("type:{}", mt.id) == mapping.match_value);
    }
    config
        .file_groups
        .iter()
        .any(|group| same_exts(&normalize_exts(&mapping.match_value), &group.exts))
}

/// 类型标识 → 已配置的 mapping（组内行序 = 优先级）。
pub fn find_mapping_for_type<'a>(
    config: &'a Config,
    match_type: &str,
    id: &str,
) -> Option<&'a SelectedMapping> {
    find_mapping_index_for_type(config, match_type, id)
        .map(|index| &config.selected_action.mappings[index])
}

/// `find_mapping_for_type` 的索引版（调用方随后要 `remove` 时避免借用纠缠）。
pub fn find_mapping_index_for_type(config: &Config, match_type: &str, id: &str) -> Option<usize> {
    let mappings = &config.selected_action.mappings;
    if match_type == MATCH_TEXT_TYPE {
        return mappings.iter().position(|mapping| {
            mapping.match_type == MATCH_TEXT_TYPE && mapping.match_value == id
        });
    }
    if let Some(name) = id.strip_prefix("group:") {
        let group = config.file_groups.iter().find(|group| group.name == name)?;
        return mappings.iter().position(|mapping| {
            mapping.match_type == MATCH_FILE_EXT
                && same_exts(&normalize_exts(&mapping.match_value), &group.exts)
        });
    }
    // type:<id> 或 orphan: 直接按 matchValue 命中
    mappings
        .iter()
        .position(|mapping| mapping.match_type == MATCH_FILE_EXT && mapping.match_value == id)
}

/// `find_mapping_for_type` 的可变版（编辑 entries / 条件值）。
pub fn find_mapping_for_type_mut<'a>(
    config: &'a mut Config,
    match_type: &str,
    id: &str,
) -> Option<&'a mut SelectedMapping> {
    let index = find_mapping_index_for_type(config, match_type, id)?;
    config.selected_action.mappings.get_mut(index)
}

/// 未配置类型的 transient matchValue：文本特征 = 特征值本身；group = 分组后缀逗号串；其余原样。
pub fn transient_match_value(config: &Config, match_type: &str, id: &str) -> String {
    if match_type == MATCH_TEXT_TYPE {
        return id.to_string();
    }
    if let Some(name) = id.strip_prefix("group:") {
        return match config.file_groups.iter().find(|group| group.name == name) {
            Some(group) => group.exts.join(","),
            None => String::new(),
        };
    }
    id.to_string() // type:<id> / orphan: 原样
}

// ---------------------------------------------------------------- 添加映射弹窗

/// 「添加映射」类型下拉的一项（复刻 `AddMappingVm` 的候选项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTypeOption {
    /// 分区：[`MATCH_FILE_EXT`] / [`MATCH_TEXT_TYPE`]。
    pub match_type: String,
    /// 类型标识（`group:<name>` / 内置特征值 / `type:<id>`）。
    pub id: String,
    /// 显示文案。
    pub label: String,
}

/// 「添加映射」候选类型全集，顺序复刻 `AddMappingVm`：
/// 文件分组 → 内置文本特征 5 个 → 自定义类型（text → fileExt）。
/// （旧版内置特征后有一个下拉分隔项；reactor ComboBox 无分隔项能力，按同序平铺。）
pub fn add_type_options(config: &Config) -> Vec<AddTypeOption> {
    let mut options = Vec::new();
    for group in &config.file_groups {
        options.push(AddTypeOption {
            match_type: MATCH_FILE_EXT.to_string(),
            id: format!("group:{}", group.name),
            label: group.label.clone(),
        });
    }
    for (value, key) in TEXT_TYPES {
        options.push(AddTypeOption {
            match_type: MATCH_TEXT_TYPE.to_string(),
            id: value.to_string(),
            label: i18n::t(key),
        });
    }
    for mt in config.match_types.iter().filter(|mt| mt.kind == "text") {
        options.push(AddTypeOption {
            match_type: MATCH_TEXT_TYPE.to_string(),
            id: format!("type:{}", mt.id),
            label: custom_type_label(&mt.id, &mt.label, &mt.label_en),
        });
    }
    for mt in config.match_types.iter().filter(|mt| mt.kind == "fileExt") {
        options.push(AddTypeOption {
            match_type: MATCH_FILE_EXT.to_string(),
            id: format!("type:{}", mt.id),
            label: custom_type_label(&mt.id, &mt.label, &mt.label_en),
        });
    }
    options
}

/// 候选类型的落盘目标 `(matchType, matchValue)`（复刻 `AddMappingVm` 构造 SelectedMapping）。
pub fn add_target(config: &Config, id: &str) -> (String, String) {
    if let Some(name) = id.strip_prefix("group:") {
        let match_value = config
            .file_groups
            .iter()
            .find(|group| group.name == name)
            .map(|group| group.exts.join(","))
            .unwrap_or_default();
        return (MATCH_FILE_EXT.to_string(), match_value);
    }
    if id.starts_with("type:") {
        let kind = config
            .match_types
            .iter()
            .find(|mt| format!("type:{}", mt.id) == id)
            .map(|mt| mt.kind.clone())
            .unwrap_or_else(|| MATCH_TEXT_TYPE.to_string());
        let match_type = if kind == "fileExt" {
            MATCH_FILE_EXT
        } else {
            MATCH_TEXT_TYPE
        };
        return (match_type.to_string(), id.to_string());
    }
    (MATCH_TEXT_TYPE.to_string(), id.to_string())
}

/// 是否已存在同 `(matchType, matchValue)` 的映射（文本特征 trim+大小写不敏感；
/// 文件后缀走归一化集合比较，复刻 `AddMappingVm.ConfirmAsync` 的去重提示前提）。
pub fn mapping_exists(config: &Config, match_type: &str, match_value: &str) -> bool {
    config.selected_action.mappings.iter().any(|mapping| {
        if mapping.match_type != match_type {
            return false;
        }
        if match_type == MATCH_TEXT_TYPE {
            mapping
                .match_value
                .trim()
                .eq_ignore_ascii_case(match_value.trim())
        } else {
            same_exts(
                &normalize_exts(&mapping.match_value),
                &normalize_exts(match_value),
            )
        }
    })
}

/// 行为徽章配色键：链接深灰 / 路径暖绿 / 磁力珊瑚 / 其余橄榄（对齐 `BehaviorBadgeColors`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeColor {
    LinkDarkWarm,
    PathGreen,
    MagnetCoral,
    PlainOlive,
}

impl BadgeColor {
    /// 由行为的基础动作 ID 推导（基础动作 = `Catalog::base_action_of` 的结果）。
    pub fn for_base_action(base_action: &str) -> Self {
        match base_action {
            "open_url" => Self::LinkDarkWarm,
            "open_path" | "open_folder" => Self::PathGreen,
            "magnet_download" => Self::MagnetCoral,
            _ => Self::PlainOlive,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileGroup, MatchType, SelectedAction, SelectedEntry};

    fn pack(
        id: &str,
        applies_to: Vec<BehaviorAppliesTo>,
        entry_action: Option<&str>,
    ) -> BehaviorPack {
        BehaviorPack {
            id: id.to_string(),
            name: format!("行为 {id}"),
            name_en: Some(format!("Behavior {id}")),
            spec_version: 1,
            applies_to,
            entry: crate::models::BehaviorEntry {
                kind: "builtin".to_string(),
                action: entry_action.map(str::to_string),
                params: None,
                ..Default::default()
            },
            source: Some("builtin".to_string()),
            ..Default::default()
        }
    }

    fn applies(
        kind: &str,
        value: Option<&str>,
        exts: Option<Vec<&str>>,
        is_default: bool,
    ) -> BehaviorAppliesTo {
        BehaviorAppliesTo {
            kind: kind.to_string(),
            value: value.map(str::to_string),
            exts: exts.map(|list| list.into_iter().map(str::to_string).collect()),
            is_default,
        }
    }

    fn sample_catalog() -> Catalog {
        Catalog {
            builtin: vec![
                pack(
                    "open_url",
                    vec![applies("textType", Some("url"), None, true)],
                    Some("open_url"),
                ),
                pack(
                    "copy_text",
                    vec![applies("textType", Some("plain"), None, false)],
                    Some("copy"),
                ),
                pack(
                    "run_any",
                    vec![applies("fileExt", None, Some(vec!["*"]), true)],
                    Some("run"),
                ),
                pack(
                    "open_image",
                    vec![applies("fileExt", None, Some(vec!["png", "jpg"]), false)],
                    Some("open_path"),
                ),
            ],
            user: vec![],
        }
    }

    fn config_with(mapping: SelectedMapping) -> Config {
        Config {
            selected_action: SelectedAction {
                mappings: vec![mapping],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn rule_values_split_exts_and_lowercase_text() {
        assert_eq!(rule_values("textType", " URL "), vec!["url"]);
        assert!(rule_values("textType", "  ").is_empty());
        assert_eq!(rule_values("fileExt", "txt,md"), vec!["txt", "md"]);
        assert_eq!(rule_values("fileExt", ""), vec!["*"]);
    }

    #[test]
    fn covering_orders_dedicated_before_generic() {
        let catalog = sample_catalog();
        // url：open_url 专属在前（也带 default）
        let list = covering(&catalog, "textType", "url");
        assert_eq!(list[0].id, "open_url");
        assert!(list.iter().all(|p| p.id != "copy_text" || true));

        // 自定义文本引用：无专属包 ⇒ 只有 plain 继承的 copy_text
        let list = covering(&catalog, "textType", "type:abc");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "copy_text");
        assert!(!has_dedicated_behavior_for(
            &catalog, "textType", "type:abc"
        ));

        // png：open_image 专属在前，run_any（* 通配）在后
        let list = covering(&catalog, "fileExt", "png");
        assert_eq!(
            list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["open_image", "run_any"]
        );
    }

    #[test]
    fn default_for_prefers_default_flag_in_catalog_order() {
        let catalog = sample_catalog();
        assert_eq!(
            default_for(&catalog, "textType", "url").as_deref(),
            Some("open_url")
        );
        assert_eq!(
            default_for(&catalog, "fileExt", "png").as_deref(),
            Some("run_any"),
            "* 通配包带 default"
        );
        // 无覆盖包 ⇒ None
        assert_eq!(default_for(&catalog, "textType", "nonsense"), None);
    }

    #[test]
    fn is_no_value_and_template_follow_manifest() {
        let mut catalog = sample_catalog();
        // 用户包：script 语义 + 带模板 ⇒ 有参
        catalog.user.push(BehaviorPack {
            id: "es_search".to_string(),
            name: "Everything 搜索".to_string(),
            entry: crate::models::BehaviorEntry {
                kind: "script".to_string(),
                params: Some(crate::models::BehaviorEntryParams {
                    action_value: Some("es.exe {selected}".to_string()),
                    working_dir: None,
                }),
                ..Default::default()
            },
            ..Default::default()
        });
        // builtin 包 + 空模板 ⇒ 无参（open_url 本身也在基础集）
        assert!(catalog.is_no_value("open_url"));
        assert!(catalog.is_no_value("run_any"), "builtin + 空 params ⇒ 无参");
        assert!(!catalog.is_no_value("es_search"));
        assert_eq!(
            catalog.default_template_for("es_search"),
            "es.exe {selected}"
        );
        assert_eq!(catalog.default_template_for("open_url"), "");
        // base_action_of：builtin 用户包取 entry.action
        assert_eq!(catalog.base_action_of("es_search"), "es_search");
        assert_eq!(catalog.base_action_of("open_url"), "open_url");
        let catalog = Catalog::default();
        assert_eq!(catalog.base_action_of("ghost_id"), "ghost_id", "未知原样");
    }

    #[test]
    fn label_for_respects_language_and_falls_back_to_raw() {
        let catalog = sample_catalog();
        // 当前语言（zh）：取 name
        assert_eq!(catalog.label_for("open_url"), "行为 open_url");
        // 未知 ID：原样返回
        assert_eq!(catalog.label_for("ghost"), "ghost");
    }

    #[test]
    fn normalize_and_same_exts_ignore_case_order_and_dots() {
        assert_eq!(
            normalize_exts(".txt,md，doc；png"),
            vec!["txt", "md", "doc", "png"]
        );
        assert_eq!(
            normalize_exts("txt,txt, TXT"),
            vec!["txt"],
            "大小写不敏感去重"
        );
        assert!(same_exts(
            &["jpg".into(), ".png".into()],
            &["PNG".into(), "jpg".into()]
        ));
        assert!(!same_exts(&["txt".into()], &["txt".into(), "md".into()]));
    }

    #[test]
    fn build_toggles_covers_builtin_groups_custom_and_orphans() {
        let config = Config {
            file_groups: vec![FileGroup {
                name: "pics".to_string(),
                label: "图片".to_string(),
                exts: vec!["png".into(), "jpg".into()],
            }],
            match_types: vec![
                MatchType {
                    id: "t1".to_string(),
                    label: "我的文本".to_string(),
                    label_en: String::new(),
                    kind: "text".to_string(),
                    ..Default::default()
                },
                MatchType {
                    id: "f1".to_string(),
                    label: String::new(),
                    label_en: String::new(),
                    kind: "fileExt".to_string(),
                    ..Default::default()
                },
            ],
            selected_action: SelectedAction {
                mappings: vec![SelectedMapping {
                    match_type: "fileExt".to_string(),
                    match_value: "zip".to_string(), // 孤儿：不属于任何分组
                    entries: vec![SelectedEntry::default()],
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let text = build_toggles(&config, MATCH_TEXT_TYPE);
        let ids: Vec<&str> = text.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            ["url", "path", "magnet", "bilibili", "plain", "type:t1"]
        );
        assert_eq!(text[5].label, "我的文本", "自定义标签：label 空才回退 id");

        let file = build_toggles(&config, MATCH_FILE_EXT);
        let ids: Vec<&str> = file.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["group:pics", "type:f1", "orphan:0"]);
        assert_eq!(file[0].label, "图片");
        assert_eq!(file[2].label, "zip", "孤儿标签 = 原始 matchValue");
    }

    #[test]
    fn find_mapping_and_transient_value_resolve_each_id_kind() {
        let config = config_with(SelectedMapping {
            match_type: "fileExt".to_string(),
            match_value: "png,jpg".to_string(),
            entries: vec![SelectedEntry {
                behavior: "open_image".to_string(),
                ..Default::default()
            }],
        });
        // group:pics 分组（等价后缀）命中该 mapping
        let mut config = config;
        config.file_groups.push(FileGroup {
            name: "pics".to_string(),
            label: "图片".to_string(),
            exts: vec!["jpg".into(), ".PNG".into()],
        });

        let found = find_mapping_for_type(&config, MATCH_FILE_EXT, "group:pics");
        assert!(found.is_some(), "后缀等价（含两端点/大小写）命中");
        assert!(find_mapping_for_type(&config, MATCH_FILE_EXT, "group:none").is_none());

        // 文本卡：按特征值直接命中
        let mut config2 = config_with(SelectedMapping {
            match_type: "textType".to_string(),
            match_value: "url".to_string(),
            entries: vec![],
        });
        assert!(find_mapping_for_type(&config2, MATCH_TEXT_TYPE, "url").is_some());
        assert!(find_mapping_for_type(&config2, MATCH_TEXT_TYPE, "path").is_none());

        // transient：分组取后缀串
        config2.file_groups.push(FileGroup {
            name: "g".to_string(),
            label: "g".to_string(),
            exts: vec!["txt".into(), "md".into()],
        });
        assert_eq!(
            transient_match_value(&config2, MATCH_FILE_EXT, "group:g"),
            "txt,md"
        );
        assert_eq!(
            transient_match_value(&config2, MATCH_TEXT_TYPE, "url"),
            "url"
        );
        assert_eq!(
            transient_match_value(&config2, MATCH_FILE_EXT, "orphan:0"),
            "orphan:0"
        );
    }

    #[test]
    fn badge_color_follows_base_action() {
        assert_eq!(
            BadgeColor::for_base_action("open_url"),
            BadgeColor::LinkDarkWarm
        );
        assert_eq!(
            BadgeColor::for_base_action("open_folder"),
            BadgeColor::PathGreen
        );
        assert_eq!(
            BadgeColor::for_base_action("magnet_download"),
            BadgeColor::MagnetCoral
        );
        assert_eq!(BadgeColor::for_base_action("copy"), BadgeColor::PlainOlive);
    }

    #[test]
    fn match_type_label_uses_i18n_keys() {
        assert_eq!(
            match_type_label(MATCH_FILE_EXT),
            crate::services::i18n::t("1031")
        );
        assert_eq!(
            match_type_label(MATCH_TEXT_TYPE),
            crate::services::i18n::t("1032")
        );
        assert_eq!(match_type_label("other"), "other");
    }
}
