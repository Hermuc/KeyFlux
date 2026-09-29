//! PUT /config 保存链路校验 —— Go `internal/script/selectedaction.go`
//! （`ValidateSelectedAction` / `ResolveMappingValues`）与
//! `internal/script/actionscheme.go`（`ValidateFileGroups`）的移植，
//! 以及校验所需的 `behaviors` 依赖面（`RefValues` / `Covers` / 文本特征词表）。
//!
//! 边界说明：生成器侧 `generator::behaviors` 的 `Pack` 未携带 `appliesTo`
//! （生成端不消费），而覆盖判定需要它；生成器模块冻结不动，故此处以**校验专用**
//! 的最小 manifest 读取（两来源目录：内置 = exe 同级 `behaviors/`、用户 =
//! config 同级 `behaviors/` —— 与 Go `server.loadBehaviorCatalog` 一致，
//! **不含**插件贡献目录）。
//!
//! 错误文案逐字对照 Go（面向用户的中文提示），HTTP 400 响应体依赖这些文案。

use std::path::Path;

use serde::Deserialize;

use crate::generator::model::{Config, FileGroup, SelectedAction, SelectedMapping};

/// 单个 mapping 的 entries 上限（菜单序号 1-9）。Go `maxEntriesPerMapping`。
const MAX_ENTRIES_PER_MAPPING: usize = 9;

// --------------------------------------------------------------------------- 内置文本特征词表

/// Go `behaviors/textfeatures.go` 注册表的校验面投影（值 / 中文名，顺序即表序）。
/// 仅供词表判定与错误文案拼接使用；正则命中不在保存链路（`MatchTextFeature`
/// 不被 ValidateSelectedAction 消费），故不移植。
const TEXT_FEATURES: [(&str, &str); 5] = [
    ("url", "链接"),
    ("path", "路径"),
    ("magnet", "磁力链接"),
    ("bilibili", "B 站"),
    ("plain", "纯文本"),
];

/// Go `behaviors.FindTextFeature`：归一化（去空白 + 小写）后精确匹配。
fn find_text_feature(value: &str) -> Option<&'static str> {
    let v = value.trim().to_lowercase();
    TEXT_FEATURES
        .iter()
        .find(|(known, _)| *known == v)
        .map(|(value, _)| *value)
}

/// Go `behaviors.IsKnownTextType`（含 plain）。
fn is_known_text_type(value: &str) -> bool {
    find_text_feature(value).is_some()
}

/// Go `behaviors.TextFeatureHint`："链接 / 路径 / 磁力链接 / B 站 / 纯文本"。
fn text_feature_hint() -> String {
    let labels: Vec<&str> = TEXT_FEATURES.iter().map(|(_, label)| *label).collect();
    labels.join(" / ")
}

// --------------------------------------------------------------------------- 校验专用行为目录

/// Go `behaviors.AppliesToEntry` 的校验面投影。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawAppliesToEntry {
    #[serde(rename = "type")]
    kind: String,
    exts: Vec<String>,
    value: String,
}

/// Go `behaviors.Pack` 的校验面投影（只取 Covers 所需字段）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawPack {
    id: String,
    name: String,
    #[serde(rename = "appliesTo")]
    applies_to: Vec<RawAppliesToEntry>,
}

/// Go `behaviors.Catalog` 的校验面投影：`Get` + `Covers`。
#[derive(Debug, Clone, Default)]
pub(crate) struct ValidationCatalog {
    packs: Vec<RawPack>,
}

impl ValidationCatalog {
    /// Go `(*Catalog).Get`：线性查找，先到者胜（builtin 在前）。
    fn get(&self, id: &str) -> Option<&RawPack> {
        self.packs.iter().find(|pack| pack.id == id)
    }

    /// Go `behaviors.containsFold`：后缀集成员判定（归一化后忽略大小写）。
    fn contains_fold(list: &[String], v: &str) -> bool {
        list.iter()
            .any(|item| normalize_ext(item).eq_ignore_ascii_case(&normalize_ext(v)))
    }

    /// Go `behaviors.entryCovers`：单条前提是否覆盖规则值集。
    /// fileExt 要求规则值集 ⊆ 前提值集（`*` 覆盖任意）；textType 要求特征值相等，
    /// 自定义引用（type:<id>）被 `plain` 前提覆盖（两段式覆盖，方案 C7）。
    fn entry_covers(entry: &RawAppliesToEntry, match_type: &str, values: &[String]) -> bool {
        if entry.kind != match_type {
            return false;
        }
        match entry.kind.as_str() {
            "fileExt" => {
                let pack_wildcard = Self::contains_fold(&entry.exts, "*");
                for v in values {
                    if v == "*" {
                        if !pack_wildcard {
                            return false; // 任意文件规则只能依赖通配前提
                        }
                        continue;
                    }
                    if !pack_wildcard && !Self::contains_fold(&entry.exts, v) {
                        return false;
                    }
                }
                true
            }
            "textType" => {
                if values.len() != 1 {
                    return false;
                }
                if entry.value.eq_ignore_ascii_case(&values[0]) {
                    return true;
                }
                is_custom_ref(&values[0]) && entry.value.eq_ignore_ascii_case("plain")
            }
            _ => false,
        }
    }

    /// Go `(*Catalog).Covers`。
    pub(crate) fn covers(&self, id: &str, match_type: &str, values: &[String]) -> bool {
        let Some(pack) = self.get(id) else {
            return false;
        };
        pack.applies_to
            .iter()
            .any(|entry| Self::entry_covers(entry, match_type, values))
    }

    /// Go `server.behaviorDisplayName`：优先包名，缺失回退原始 ID。
    fn display_name(&self, id: &str) -> String {
        self.get(id)
            .map(|pack| pack.name.clone())
            .unwrap_or_else(|| id.to_string())
    }
}

/// 内置基础动作保留 ID 集。Go `behaviors.BuiltinActionIDs`（逐字搬运；
/// 生成器侧 `BUILTIN_ACTION_IDS` 同源，此处独立成表以保持校验层自包含）。
const BUILTIN_ACTION_IDS: [&str; 10] = [
    "open_url",
    "open_path",
    "open_folder",
    "magnet_download",
    "open",
    "search",
    "run",
    "send_keys",
    "script",
    "copy",
];

fn is_builtin_action(id: &str) -> bool {
    BUILTIN_ACTION_IDS.contains(&id)
}

/// 读单个包目录（Go `server` 场景下经 `LoadCatalog→readPack` 的容错口径）：
/// 缺 `behavior.json` / 解析失败 / 目录名与 id 不一致 ⇒ 跳过该包（错误隔离）。
fn read_validation_pack(dir: &Path) -> Option<RawPack> {
    let raw = std::fs::read(dir.join("behavior.json")).ok()?;
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&raw);
    let pack: RawPack = serde_json::from_slice(raw).ok()?;
    if pack.id != dir.file_name()?.to_string_lossy() {
        return None;
    }
    Some(pack)
}

/// 扫描一个来源目录：缺失/不可读 ⇒ 空（正常场景）。
fn load_validation_dir(dir: &Path) -> Vec<RawPack> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut packs: Vec<RawPack> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter_map(|entry| read_validation_pack(&entry.path()))
        .collect();
    packs.sort_by(|a, b| a.id.cmp(&b.id)); // 稳定排序，同 Go sortPacks
    packs
}

/// Go `server.loadBehaviorCatalog`（behaviors.go:41-47）：内置 = exe 同级
/// `behaviors/`，用户 = `../data/behaviors`。两来源拼接，先到者胜。
pub(crate) fn load_validation_catalog(builtin_dir: &Path, user_dir: &Path) -> ValidationCatalog {
    let mut packs = load_validation_dir(builtin_dir);
    packs.extend(load_validation_dir(user_dir));
    ValidationCatalog { packs }
}

// --------------------------------------------------------------------------- 引用解析

/// Go `behaviors.IsCustomRef`：`type:` 前缀 = 自定义类型引用。
fn is_custom_ref(v: &str) -> bool {
    v.starts_with("type:")
}

/// Go `behaviors.normalizeExt`：去空白、去两端点。
fn normalize_ext(v: &str) -> String {
    v.trim().trim_matches('.').to_string()
}

/// Go `behaviors.RefValues`（behaviors.go:224-241）：把规则匹配前提展开为值集。
/// fileExt 按逗号拆分并归一化（`*` 保留）；其余非空即单值。
fn ref_values(match_type: &str, match_value: &str) -> Vec<String> {
    if match_type == "fileExt" {
        if match_value.trim() == "*" {
            return vec!["*".to_string()];
        }
        return match_value
            .split(',')
            .map(normalize_ext)
            .filter(|v| !v.is_empty())
            .collect();
    }
    let v = match_value.trim();
    if v.is_empty() {
        Vec::new()
    } else {
        vec![v.to_string()]
    }
}

/// Go `script.ResolveMappingValues`（selectedaction.go:144-177）：
/// 解析映射匹配前提为 `(matchType, values)`；解析失败 ⇒ `None`。
/// `type:` 引用查 config 的 matchTypes（text）/ fileGroups（fileExt）。
pub(crate) fn resolve_mapping_values(
    cfg: &Config,
    mapping: &SelectedMapping,
) -> Option<(String, Vec<String>)> {
    if is_custom_ref(&mapping.match_value) {
        let id = mapping.match_value.strip_prefix("type:").unwrap_or("");
        return match mapping.match_type.as_str() {
            "textType" => cfg
                .find_match_type(id)
                .filter(|mt| mt.kind == "text")
                .map(|_| ("textType".to_string(), vec![mapping.match_value.clone()])),
            "fileExt" => cfg
                .file_group_exts(id)
                .filter(|exts| !exts.is_empty())
                .map(|exts| ("fileExt".to_string(), exts.clone())),
            _ => None,
        };
    }
    match mapping.match_type.as_str() {
        "fileExt" => {
            let v = ref_values("fileExt", &mapping.match_value);
            if v.is_empty() {
                None
            } else {
                Some(("fileExt".to_string(), v))
            }
        }
        "textType" => {
            let v = mapping.match_value.trim().to_lowercase();
            if is_known_text_type(&v) {
                Some(("textType".to_string(), vec![v]))
            } else {
                None
            }
        }
        _ => None,
    }
}

// --------------------------------------------------------------------------- 校验器

/// Go `script.matchTypeName`（错误提示用）。
fn match_type_name(match_type: &str) -> &'static str {
    if match_type == "textType" {
        "文本特征"
    } else {
        "文件后缀"
    }
}

/// Go `script.hasControlChar`（actionscheme.go:62-64）。
fn has_control_char(s: &str) -> bool {
    s.contains('\r') || s.contains('\n')
}

/// Go `script.ValidateFileGroups`（actionscheme.go:31-58）：
/// 名称/显示名非空、名称合法、后缀列表非空且不含控制字符。
pub(crate) fn validate_file_groups(groups: &[FileGroup]) -> Result<(), String> {
    // Go: ^[a-z][a-z0-9_]*$（Name 升格为 type:<Name> 引用的稳定标识）
    let name_re = regex::Regex::new(r"^[a-z][a-z0-9_]*$").expect("内置正则不应编译失败");
    for (index, group) in groups.iter().enumerate() {
        if group.name.trim().is_empty() {
            return Err(format!("文件分组第 {} 项缺少名称 (name)", index + 1));
        }
        if !name_re.is_match(&group.name) {
            return Err(format!(
                "文件分组「{}」名称不合法 (须以小写字母开头, 仅含小写字母/数字/下划线)",
                group.name
            ));
        }
        if group.label.trim().is_empty() {
            return Err(format!("文件分组「{}」缺少显示名 (label)", group.name));
        }
        if group.exts.is_empty() {
            return Err(format!("文件分组「{}」的后缀列表 (exts) 为空", group.name));
        }
        for (ext_index, ext) in group.exts.iter().enumerate() {
            if has_control_char(ext) {
                return Err(format!(
                    "文件分组「{}」第 {} 个后缀含换行等控制字符",
                    group.name,
                    ext_index + 1
                ));
            }
        }
    }
    Ok(())
}

/// Go `script.ValidateSelectedAction`（selectedaction.go:81-136）：
/// PUT /config 保存链路的组合合法性校验。`cat` 为 `None` 时跳过覆盖检查
/// （镜像 Go 对目录缺失/异常部署的容忍）。
pub(crate) fn validate_selected_action(
    selected_action: Option<&SelectedAction>,
    catalog: Option<&ValidationCatalog>,
    cfg: &Config,
) -> Result<(), String> {
    let Some(sa) = selected_action else {
        return Ok(());
    };
    // 热键：仅启用态要求非空（禁用/空热键方案不注册热键，不把存量空配置锁死）
    if sa.enable && sa.hotkey.trim().is_empty() {
        return Err("选中动作已启用但热键为空, 请设置热键或关闭启用".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    for mapping in &sa.mappings {
        // 同 (matchType, matchValue) 的重复 mapping 拒绝（归一化后比较）
        let key = format!(
            "{}|{}",
            mapping.match_type.to_lowercase(),
            mapping.match_value.trim().to_lowercase()
        );
        if !seen.insert(key) {
            return Err(format!(
                "{}「{}」的映射条件重复, 请合并为同一映射",
                match_type_name(&mapping.match_type),
                mapping.match_value
            ));
        }
        if mapping.entries.is_empty() {
            return Err(format!(
                "{}「{}」没有行为, 请至少添加一个行为",
                match_type_name(&mapping.match_type),
                mapping.match_value
            ));
        }
        if mapping.entries.len() > MAX_ENTRIES_PER_MAPPING {
            return Err(format!(
                "{}「{}」的行为超过 {} 个 (菜单序号仅支持 1-{})",
                match_type_name(&mapping.match_type),
                mapping.match_value,
                MAX_ENTRIES_PER_MAPPING,
                MAX_ENTRIES_PER_MAPPING
            ));
        }
        // 解析匹配前提值集（含 type: 引用）：悬空引用或非法条件即拒绝
        let Some((match_type, values)) = resolve_mapping_values(cfg, mapping) else {
            if is_custom_ref(&mapping.match_value) {
                return Err(format!(
                    "未找到引用的匹配类型「{}」，请先在「匹配类型」中创建",
                    mapping
                        .match_value
                        .strip_prefix("type:")
                        .unwrap_or(&mapping.match_value)
                ));
            }
            if mapping.match_type == "textType" {
                return Err(format!(
                    "未知的文本特征「{}」，可选：{}（或在「匹配类型」中自定义）",
                    mapping.match_value,
                    text_feature_hint()
                ));
            }
            return Err(format!(
                "文件扩展名「{}」无效，请填写如 jpg,png（或 * 表示任意文件）",
                mapping.match_value
            ));
        };
        // 归一化后为空拒绝（空串/纯点/纯逗号）
        if values.is_empty() {
            return Err(format!(
                "文件扩展名「{}」无效，请填写如 jpg,png（或 * 表示任意文件）",
                mapping.match_value
            ));
        }
        for (entry_index, entry) in mapping.entries.iter().enumerate() {
            // 内置基础动作在目录缺失时跳过覆盖检查（异常部署/纯 CLI 场景的容忍口径）
            let Some(cat) = catalog else {
                continue;
            };
            if is_builtin_action(&entry.behavior) && cat.get(&entry.behavior).is_none() {
                continue;
            }
            if !cat.covers(&entry.behavior, &match_type, &values) {
                return Err(format!(
                    "{}「{}」第 {} 项: 动作「{}」与该匹配条件不匹配",
                    match_type_name(&mapping.match_type),
                    mapping.match_value,
                    entry_index + 1,
                    cat.display_name(&entry.behavior)
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::model::{MatchRule, MatchType, SelectedEntry};

    fn config_with_refs() -> Config {
        Config {
            match_types: vec![MatchType {
                id: "netdisk".into(),
                label: "网盘".into(),
                kind: "text".into(),
                rules: vec![MatchRule {
                    op: "contains".into(),
                    value: "pan.baidu.com".into(),
                }],
                ..Default::default()
            }],
            file_groups: vec![FileGroup {
                name: "image".into(),
                label: "图片".into(),
                exts: vec!["jpg".into(), "png".into()],
            }],
            ..Default::default()
        }
    }

    fn entry(behavior: &str) -> SelectedEntry {
        SelectedEntry {
            behavior: behavior.into(),
            ..Default::default()
        }
    }

    #[test]
    fn file_groups_rejects_bad_name_label_exts_and_control_chars() {
        let group = |name: &str, label: &str, exts: &[&str]| FileGroup {
            name: name.into(),
            label: label.into(),
            exts: exts.iter().map(|s| s.to_string()).collect(),
        };
        assert_eq!(
            validate_file_groups(&[group("", "L", &["jpg"])]).unwrap_err(),
            "文件分组第 1 项缺少名称 (name)"
        );
        assert_eq!(
            validate_file_groups(&[group("Image", "L", &["jpg"])]).unwrap_err(),
            "文件分组「Image」名称不合法 (须以小写字母开头, 仅含小写字母/数字/下划线)"
        );
        assert_eq!(
            validate_file_groups(&[group("img", "", &["jpg"])]).unwrap_err(),
            "文件分组「img」缺少显示名 (label)"
        );
        assert_eq!(
            validate_file_groups(&[group("img", "图", &[])]).unwrap_err(),
            "文件分组「img」的后缀列表 (exts) 为空"
        );
        assert_eq!(
            validate_file_groups(&[group("img", "图", &["j\npg"])]).unwrap_err(),
            "文件分组「img」第 1 个后缀含换行等控制字符"
        );
        assert!(validate_file_groups(&[group("img", "图", &["jpg", "png"])]).is_ok());
    }

    #[test]
    fn selected_action_rejects_empty_hotkey_dup_and_empty_entries() {
        let cfg = Config::default();
        let mut sa = SelectedAction {
            hotkey: "  ".into(),
            enable: true,
            mappings: vec![],
        };
        assert_eq!(
            validate_selected_action(Some(&sa), None, &cfg).unwrap_err(),
            "选中动作已启用但热键为空, 请设置热键或关闭启用"
        );
        // 禁用态不要求热键
        sa.enable = false;
        assert!(validate_selected_action(Some(&sa), None, &cfg).is_ok());

        sa.mappings = vec![
            SelectedMapping {
                match_type: "textType".into(),
                match_value: "url".into(),
                entries: vec![entry("copy")],
            },
            SelectedMapping {
                match_type: "texttype".into(),
                match_value: " URL ".into(),
                entries: vec![entry("copy")],
            },
        ];
        // Go 文案取当前（第二个）mapping 的原文：非内置名 "texttype" 回退「文件后缀」
        assert_eq!(
            validate_selected_action(Some(&sa), None, &cfg).unwrap_err(),
            "文件后缀「 URL 」的映射条件重复, 请合并为同一映射"
        );

        sa.mappings = vec![SelectedMapping {
            match_type: "textType".into(),
            match_value: "url".into(),
            entries: vec![],
        }];
        assert_eq!(
            validate_selected_action(Some(&sa), None, &cfg).unwrap_err(),
            "文本特征「url」没有行为, 请至少添加一个行为"
        );
    }

    #[test]
    fn selected_action_rejects_dangling_refs_and_unknown_text_types() {
        let cfg = config_with_refs();
        let sa = |match_type: &str, match_value: &str| SelectedAction {
            hotkey: "^!s".into(),
            enable: false,
            mappings: vec![SelectedMapping {
                match_type: match_type.into(),
                match_value: match_value.into(),
                entries: vec![entry("copy")],
            }],
        };
        assert_eq!(
            validate_selected_action(Some(&sa("textType", "type:missing")), None, &cfg)
                .unwrap_err(),
            "未找到引用的匹配类型「missing」，请先在「匹配类型」中创建"
        );
        assert_eq!(
            validate_selected_action(Some(&sa("fileExt", "type:missing")), None, &cfg).unwrap_err(),
            "未找到引用的匹配类型「missing」，请先在「匹配类型」中创建"
        );
        assert_eq!(
            validate_selected_action(Some(&sa("textType", "nope")), None, &cfg).unwrap_err(),
            "未知的文本特征「nope」，可选：链接 / 路径 / 磁力链接 / B 站 / 纯文本（或在「匹配类型」中自定义）"
        );
        assert_eq!(
            validate_selected_action(Some(&sa("fileExt", " , . ")), None, &cfg).unwrap_err(),
            "文件扩展名「 , . 」无效，请填写如 jpg,png（或 * 表示任意文件）"
        );
        // 合法形态：type 引用命中 + 内置词表 + 通配后缀
        assert!(
            validate_selected_action(Some(&sa("textType", "type:netdisk")), None, &cfg).is_ok()
        );
        assert!(validate_selected_action(Some(&sa("fileExt", "type:image")), None, &cfg).is_ok());
        assert!(validate_selected_action(Some(&sa("fileExt", "*")), None, &cfg).is_ok());
        assert!(validate_selected_action(Some(&sa("textType", "MAGNET")), None, &cfg).is_ok());
    }

    /// Covers 语义：目录缺失时内置动作跳过检查；用户包前提不覆盖即拒绝。
    #[test]
    fn covers_check_skips_missing_builtin_and_rejects_uncovered_pack() {
        let cfg = Config::default();
        let catalog = ValidationCatalog::default(); // 空目录
        let sa = SelectedAction {
            hotkey: "^!s".into(),
            enable: false,
            mappings: vec![
                // 内置动作 + 空目录 ⇒ 跳过覆盖检查
                SelectedMapping {
                    match_type: "textType".into(),
                    match_value: "url".into(),
                    entries: vec![entry("copy")],
                },
                // 未知用户包 + 空目录 ⇒ Covers false ⇒ 拒绝
                SelectedMapping {
                    match_type: "textType".into(),
                    match_value: "path".into(),
                    entries: vec![entry("my_pack")],
                },
            ],
        };
        assert_eq!(
            validate_selected_action(Some(&sa), Some(&catalog), &cfg).unwrap_err(),
            "文本特征「path」第 1 项: 动作「my_pack」与该匹配条件不匹配"
        );
        // cat == None ⇒ 全部跳过
        assert!(validate_selected_action(Some(&sa), None, &cfg).is_ok());

        // 有目录且前提被 plain 覆盖（两段式：type: 引用被 plain 前提覆盖）
        let mut with_pack = ValidationCatalog::default();
        with_pack.packs.push(RawPack {
            id: "my_pack".into(),
            name: "我的包".into(),
            applies_to: vec![RawAppliesToEntry {
                kind: "textType".into(),
                exts: vec![],
                value: "plain".into(),
            }],
        });
        let mut sa2 = SelectedAction {
            hotkey: "^!s".into(),
            enable: false,
            mappings: vec![SelectedMapping {
                match_type: "textType".into(),
                match_value: "type:netdisk".into(),
                entries: vec![entry("my_pack")],
            }],
        };
        let mut cfg2 = cfg.clone();
        cfg2.match_types = vec![MatchType {
            id: "netdisk".into(),
            label: "网盘".into(),
            kind: "text".into(),
            rules: vec![MatchRule {
                op: "contains".into(),
                value: "pan".into(),
            }],
            ..Default::default()
        }];
        assert!(validate_selected_action(Some(&sa2), Some(&with_pack), &cfg2).is_ok());
        // 换成不覆盖的 url 前提（非引用）⇒ 拒绝
        sa2.mappings[0].match_value = "url".into();
        assert!(validate_selected_action(Some(&sa2), Some(&with_pack), &cfg2).is_err());
    }

    #[test]
    fn ref_values_normalizes_ext_list() {
        assert_eq!(ref_values("fileExt", " jpg , .png ,, "), vec!["jpg", "png"]);
        assert_eq!(ref_values("fileExt", "*"), vec!["*"]);
        assert!(ref_values("fileExt", " , ").is_empty());
        assert_eq!(ref_values("textType", " URL "), vec!["URL"]);
        assert!(ref_values("textType", "  ").is_empty());
    }

    #[test]
    fn entries_over_limit_is_rejected() {
        let cfg = Config::default();
        let sa = SelectedAction {
            hotkey: "^!s".into(),
            enable: false,
            mappings: vec![SelectedMapping {
                match_type: "textType".into(),
                match_value: "url".into(),
                entries: (0..10).map(|_| entry("copy")).collect(),
            }],
        };
        assert_eq!(
            validate_selected_action(Some(&sa), None, &cfg).unwrap_err(),
            "文本特征「url」的行为超过 9 个 (菜单序号仅支持 1-9)"
        );
    }

    /// RuleOptions 不参与校验（渲染数据数组暂不消费），但字段存在性由编译保证。
    #[test]
    fn rule_options_is_consumed_by_mapping_resolution() {
        let e = entry("copy");
        assert!(!e.options.copy_to_clipboard);
        assert!(!e.options.clear_selection);
        assert!(!e.options.confirm);
    }
}
