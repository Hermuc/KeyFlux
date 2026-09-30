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

use crate::generator::model::{Config, FileGroup, MatchType, SelectedAction, SelectedMapping};

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
pub(crate) fn find_text_feature(value: &str) -> Option<&'static str> {
    let v = value.trim().to_lowercase();
    TEXT_FEATURES
        .iter()
        .find(|(known, _)| *known == v)
        .map(|(value, _)| *value)
}

/// Go `behaviors.IsKnownTextType`（含 plain）。
pub(crate) fn is_known_text_type(value: &str) -> bool {
    find_text_feature(value).is_some()
}

/// Go `behaviors.TextFeatureHint`："链接 / 路径 / 磁力链接 / B 站 / 纯文本"。
pub(crate) fn text_feature_hint() -> String {
    let labels: Vec<&str> = TEXT_FEATURES.iter().map(|(_, label)| *label).collect();
    labels.join(" / ")
}

// --------------------------------------------------------------------------- 校验专用行为目录

/// Go `behaviors.AppliesToEntry` 的校验面投影。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct RawAppliesToEntry {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) exts: Vec<String>,
    pub(crate) value: String,
}

/// Go `behaviors.Pack` 的校验面投影（只取 Covers / ValidateDelete 所需字段）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct RawPack {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "appliesTo")]
    pub(crate) applies_to: Vec<RawAppliesToEntry>,
    /// Go `Pack.Source`：由加载目录决定（builtin = exe 同级、user = config 同级），
    /// manifest 里不写（同 Go `json:"-"`），故 serde 跳过。
    #[serde(skip)]
    pub(crate) source: String,
}

/// Go `behaviors.RuleRef`（behaviors.go:438-443）：删除校验所需的规则投影。
#[derive(Debug, Clone)]
pub(crate) struct RuleRef {
    pub(crate) match_type: String,
    pub(crate) match_value: String,
    pub(crate) action_type: String,
}

/// Go `behaviors.Catalog` 的校验面投影：`Get` + `Covers`。
#[derive(Debug, Clone, Default)]
pub(crate) struct ValidationCatalog {
    packs: Vec<RawPack>,
}

impl ValidationCatalog {
    /// Go `(*Catalog).Get`：线性查找，先到者胜（builtin 在前）。
    pub(crate) fn get(&self, id: &str) -> Option<&RawPack> {
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

    /// Go `behaviors.coveredBy`（behaviors.go:501-510）：其他包任一前提
    /// 覆盖单值即真。
    fn covered_by(packs: &[&RawPack], match_type: &str, v: &str) -> bool {
        packs.iter().any(|pack| {
            pack.applies_to.iter().any(|entry| {
                Self::entry_covers(entry, match_type, std::slice::from_ref(&v.to_string()))
            })
        })
    }

    /// Go `behaviors.ValidateDelete`（behaviors.go:450-499）逐字移植：
    ///  1. 内置包不可删除；
    ///  2. 被任何规则 actionType 引用 → 拒绝（引用会悬空）；
    ///  3. 值级覆盖检查：删除后，若该包前提覆盖的某个值仍被引用（其他规则或
    ///     其他行为前提）且再无任何启用行为覆盖 → 拒绝（防空前提桶）。
    pub(crate) fn validate_delete(&self, id: &str, refs: &[RuleRef]) -> Result<(), String> {
        let Some(p) = self.get(id) else {
            return Err(format!("行为「{id}」不存在"));
        };
        if p.source != "user" {
            return Err(format!("内置行为「{}」不可删除", p.name));
        }
        let mut ref_count = 0usize;
        // Go `referenced map[[2]string]bool`：(matchType, value) 有序对集合
        let mut referenced: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for r in refs {
            if r.action_type == id {
                ref_count += 1;
            }
            for v in ref_values(&r.match_type, &r.match_value) {
                referenced.insert((r.match_type.clone(), v));
            }
        }
        if ref_count > 0 {
            return Err(format!(
                "行为「{}」仍被 {} 条映射引用，请先修改或删除相应映射",
                p.name, ref_count
            ));
        }
        let others: Vec<&RawPack> = self.packs.iter().filter(|o| o.id != id).collect();
        for o in &others {
            for e in &o.applies_to {
                for v in entry_values(e) {
                    referenced.insert((e.kind.clone(), v));
                }
            }
        }
        let mut uncovered: Vec<String> = Vec::new();
        for e in &p.applies_to {
            for v in entry_values(e) {
                if !referenced.contains(&(e.kind.clone(), v.clone())) {
                    continue; // 无任何引用的前提值随包一并消失, 不构成空桶
                }
                if !Self::covered_by(&others, &e.kind, &v) {
                    uncovered.push(display_value(&e.kind, &v));
                }
            }
        }
        if !uncovered.is_empty() {
            return Err(format!(
                "删除「{}」后以下匹配条件将没有可用行为：{}",
                p.name,
                uncovered.join("、")
            ));
        }
        Ok(())
    }
}

/// Go `behaviors.entryValues`（behaviors.go）：fileExt 取扩展名表，其余取单值。
fn entry_values(e: &RawAppliesToEntry) -> Vec<String> {
    if e.kind == "fileExt" {
        e.exts.clone()
    } else {
        vec![e.value.clone()]
    }
}

/// Go `behaviors.displayValue`（behaviors.go:512-520）。
fn display_value(match_type: &str, v: &str) -> String {
    if match_type == "textType" {
        return format!("文本特征 {v}");
    }
    if v == "*" {
        return "任意文件".to_string();
    }
    format!("后缀 .{}", v.strip_prefix('.').unwrap_or(v))
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

pub(crate) fn is_builtin_action(id: &str) -> bool {
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

/// 扫描一个来源目录：缺失/不可读 ⇒ 空（正常场景）。`source` 标记包来源
/// （同 Go `LoadCatalog` 按目录赋 `Pack.Source`）。
fn load_validation_dir(dir: &Path, source: &str) -> Vec<RawPack> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut packs: Vec<RawPack> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter_map(|entry| read_validation_pack(&entry.path()))
        .collect();
    for pack in &mut packs {
        pack.source = source.to_string();
    }
    packs.sort_by(|a, b| a.id.cmp(&b.id)); // 稳定排序，同 Go sortPacks
    packs
}

/// Go `server.loadBehaviorCatalog`（behaviors.go:41-47）：内置 = exe 同级
/// `behaviors/`，用户 = `../data/behaviors`。两来源拼接，先到者胜。
pub(crate) fn load_validation_catalog(builtin_dir: &Path, user_dir: &Path) -> ValidationCatalog {
    let mut packs = load_validation_dir(builtin_dir, "builtin");
    packs.extend(load_validation_dir(user_dir, "user"));
    ValidationCatalog { packs }
}

/// 从完整 manifest 投影构建校验目录（服务端 CRUD 链路复用同一覆盖/删除语义；
/// Go 侧 behaviors.Pack 本就全字段，此处按需投影到校验面）。
pub(crate) fn catalog_from_packs(packs: Vec<RawPack>) -> ValidationCatalog {
    ValidationCatalog { packs }
}

// --------------------------------------------------------------------------- 引用解析

/// Go `behaviors.IsCustomRef`：`type:` 前缀 = 自定义类型引用。
pub(crate) fn is_custom_ref(v: &str) -> bool {
    v.starts_with("type:")
}

/// Go `behaviors.normalizeExt`：去空白、去两端点。
pub(crate) fn normalize_ext(v: &str) -> String {
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

/// Go `script.validMatchOps`：封闭 4 算子。
const VALID_MATCH_OPS: [&str; 4] = ["equals", "prefix", "suffix", "contains"];

/// Go `script.reservedTextTypeNames`：内置文本特征名，自定义类型不得占用。
/// 由 [`TEXT_FEATURES`] 派生，与 Go `behaviors.TextFeatureValues()` 同源。
fn is_reserved_text_type(id: &str) -> bool {
    TEXT_FEATURES.iter().any(|(value, _)| *value == id)
}

/// Go `script.ValidateMatchTypes`（actionscheme.go:266-327）逐字移植：自定义匹配类型表
/// 的结构校验（保存期严格）。规则：id 合法/唯一/不与内置名及文件分组名冲突；label 非空；
/// kind ∈ {text,fileExt}；kind=text 时 rules ≥1 且每条 op 合法、value 非空且 ≤256 字符；
/// kind=fileExt 时 exts 归一化后非空。**错误文案与 Go 逐字节一致**（400 响应体依赖）。
pub(crate) fn validate_match_types(
    types: &[MatchType],
    groups: &[FileGroup],
) -> Result<(), String> {
    // Go: ^[a-z][a-z0-9_]{0,23}$（长度 1-24，非多行模式下 $ = 文本末尾，与 Go 同口径）
    let id_re = regex::Regex::new(r"^[a-z][a-z0-9_]{0,23}$").expect("内置正则不应编译失败");
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let group_names: std::collections::HashSet<&str> =
        groups.iter().map(|group| group.name.as_str()).collect();

    for mt in types {
        let id = mt.id.trim();
        if !id_re.is_match(id) {
            // 文案取**原文**（未 trim），与 Go 一致
            return Err(format!(
                "内部标识「{}」格式不正确（需以小写字母开头，仅含小写字母、数字与下划线，长度 1-24）",
                mt.id
            ));
        }
        if is_reserved_text_type(id) {
            // 注意：括注仅列 4 项（未含「B 站」）系 Go 侧既有文案，逐字保留
            return Err(format!(
                "内部标识「{id}」与内置文本特征（链接／路径／磁力链接／纯文本）冲突，请更换"
            ));
        }
        if !seen.insert(id) {
            return Err(format!("内部标识「{id}」重复，同一类型的标识必须唯一"));
        }
        if group_names.contains(id) {
            return Err(format!("内部标识「{id}」与文件分组同名，请更换"));
        }
        if mt.label.trim().is_empty() {
            return Err(format!("匹配类型「{id}」缺少名称"));
        }
        if mt.kind != "text" && mt.kind != "fileExt" {
            return Err(format!(
                "匹配类型「{id}」的分类无效（可选：文本内容 / 文件类型）"
            ));
        }
        if mt.kind == "text" {
            if mt.rules.is_empty() {
                return Err(format!("文本类型「{id}」缺少匹配条件，至少需要 1 条"));
            }
            for (index, rule) in mt.rules.iter().enumerate() {
                let n = index + 1;
                if !VALID_MATCH_OPS.contains(&rule.op.as_str()) {
                    return Err(format!(
                        "匹配类型「{id}」第 {n} 条匹配条件的匹配方式无效「{}」（可选：包含该文字 / 完全相同 / 以该文字开头 / 以该文字结尾）",
                        rule.op
                    ));
                }
                if rule.value.trim().is_empty() {
                    return Err(format!("匹配类型「{id}」第 {n} 条匹配内容为空"));
                }
                if rule.value.chars().count() > 256 {
                    return Err(format!(
                        "匹配类型「{id}」第 {n} 条匹配内容过长（最多 256 个字符）"
                    ));
                }
                if has_control_char(&rule.value) {
                    return Err(format!("匹配类型「{id}」第 {n} 条匹配内容不能包含换行符"));
                }
            }
        } else {
            let has_ext = mt.exts.iter().any(|ext| !normalize_ext(ext).is_empty());
            if !has_ext {
                return Err(format!("文件类型「{id}」缺少文件扩展名（如 psd, ai）"));
            }
            for (index, ext) in mt.exts.iter().enumerate() {
                if has_control_char(ext) {
                    return Err(format!(
                        "匹配类型「{id}」第 {} 个文件扩展名不能包含换行符",
                        index + 1
                    ));
                }
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
            source: "user".into(),
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

    /// Go `script.TestValidateMatchTypes` 的镜像：三类非法（重复 id / label 空 / op 非法）
    /// 逐一断言**逐字节一致**的错误文案，另加合法形态放行 + fileExt/kind/超长等分支。
    #[test]
    fn validate_match_types_rejects_invalid_with_go_identical_messages() {
        let groups: Vec<FileGroup> = vec![FileGroup {
            name: "design".into(),
            label: "设计".into(),
            exts: vec!["psd".into()],
        }];
        let text_type = |id: &str, label: &str, op: &str, value: &str| MatchType {
            id: id.into(),
            label: label.into(),
            kind: "text".into(),
            rules: vec![MatchRule {
                op: op.into(),
                value: value.into(),
            }],
            ..Default::default()
        };

        // 合法（文本 / 文件）放行
        assert!(
            validate_match_types(
                &[text_type("netdisk", "网盘", "contains", "pan.baidu.com")],
                &groups
            )
            .is_ok()
        );
        assert!(
            validate_match_types(
                &[MatchType {
                    id: "imgs".into(),
                    label: "图片".into(),
                    kind: "fileExt".into(),
                    exts: vec!["jpg".into(), "png".into()],
                    ..Default::default()
                }],
                &groups
            )
            .is_ok()
        );

        // ── 三类要求用例（与 Go 400 文案逐字节相同）──
        // ① 重复 id（取重复的第二个，与 Go 一致）
        assert_eq!(
            validate_match_types(
                &[
                    text_type("dup", "甲", "contains", "a"),
                    text_type("dup", "乙", "contains", "b"),
                ],
                &groups
            )
            .unwrap_err(),
            "内部标识「dup」重复，同一类型的标识必须唯一"
        );
        // ② label 为空（仅空白）
        assert_eq!(
            validate_match_types(&[text_type("empty", "   ", "contains", "a")], &groups)
                .unwrap_err(),
            "匹配类型「empty」缺少名称"
        );
        // ③ op 非法
        assert_eq!(
            validate_match_types(&[text_type("badop", "算子", "regex", "a")], &groups).unwrap_err(),
            "匹配类型「badop」第 1 条匹配条件的匹配方式无效「regex」（可选：包含该文字 / 完全相同 / 以该文字开头 / 以该文字结尾）"
        );

        // 其余分支的文案对齐（Go 侧 matchtypes_test.go 覆盖同一集合）
        assert_eq!(
            validate_match_types(&[text_type("NetDisk", "x", "contains", "a")], &groups)
                .unwrap_err(),
            "内部标识「NetDisk」格式不正确（需以小写字母开头，仅含小写字母、数字与下划线，长度 1-24）"
        );
        assert_eq!(
            validate_match_types(&[text_type("url", "x", "contains", "a")], &groups).unwrap_err(),
            "内部标识「url」与内置文本特征（链接／路径／磁力链接／纯文本）冲突，请更换"
        );
        assert_eq!(
            validate_match_types(&[text_type("design", "x", "contains", "a")], &groups)
                .unwrap_err(),
            "内部标识「design」与文件分组同名，请更换"
        );
        assert_eq!(
            validate_match_types(
                &[MatchType {
                    id: "a".into(),
                    label: "x".into(),
                    kind: "weird".into(),
                    ..Default::default()
                }],
                &groups
            )
            .unwrap_err(),
            "匹配类型「a」的分类无效（可选：文本内容 / 文件类型）"
        );
        assert_eq!(
            validate_match_types(
                &[MatchType {
                    id: "a".into(),
                    label: "x".into(),
                    kind: "text".into(),
                    ..Default::default()
                }],
                &groups
            )
            .unwrap_err(),
            "文本类型「a」缺少匹配条件，至少需要 1 条"
        );
        assert_eq!(
            validate_match_types(&[text_type("a", "x", "contains", "   ")], &groups).unwrap_err(),
            "匹配类型「a」第 1 条匹配内容为空"
        );
        assert_eq!(
            validate_match_types(
                &[text_type("a", "x", "contains", &"x".repeat(257))],
                &groups
            )
            .unwrap_err(),
            "匹配类型「a」第 1 条匹配内容过长（最多 256 个字符）"
        );
        assert_eq!(
            validate_match_types(&[text_type("a", "x", "contains", "x\ny")], &groups).unwrap_err(),
            "匹配类型「a」第 1 条匹配内容不能包含换行符"
        );
        assert_eq!(
            validate_match_types(
                &[MatchType {
                    id: "a".into(),
                    label: "x".into(),
                    kind: "fileExt".into(),
                    exts: vec![".".into(), " ".into()],
                    ..Default::default()
                }],
                &groups
            )
            .unwrap_err(),
            "文件类型「a」缺少文件扩展名（如 psd, ai）"
        );
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

    // --------------------------------------------------------------- ValidateDelete

    fn pack(id: &str, name: &str, source: &str, kind: &str, value: &str) -> RawPack {
        RawPack {
            id: id.into(),
            name: name.into(),
            applies_to: vec![RawAppliesToEntry {
                kind: kind.into(),
                exts: if kind == "fileExt" {
                    value.split(',').map(|s| s.to_string()).collect()
                } else {
                    vec![]
                },
                value: if kind == "fileExt" {
                    String::new()
                } else {
                    value.into()
                },
            }],
            source: source.into(),
        }
    }

    fn rref(match_type: &str, match_value: &str, action_type: &str) -> RuleRef {
        RuleRef {
            match_type: match_type.into(),
            match_value: match_value.into(),
            action_type: action_type.into(),
        }
    }

    #[test]
    fn validate_delete_rejects_missing_builtin_and_referenced() {
        let mut cat = ValidationCatalog::default();
        cat.packs
            .push(pack("p1", "包一", "user", "textType", "url"));
        cat.packs
            .push(pack("sys", "系统", "builtin", "textType", "path"));

        assert_eq!(
            cat.validate_delete("nope", &[]).unwrap_err(),
            "行为「nope」不存在"
        );
        assert_eq!(
            cat.validate_delete("sys", &[]).unwrap_err(),
            "内置行为「系统」不可删除"
        );
        assert_eq!(
            cat.validate_delete(
                "p1",
                &[
                    rref("textType", "url", "p1"),
                    rref("textType", "path", "p1")
                ]
            )
            .unwrap_err(),
            "行为「包一」仍被 2 条映射引用，请先修改或删除相应映射"
        );
    }

    /// 值级覆盖三态：引用值仍被引用且无他人覆盖 → 拒绝；引用但他人覆盖 → 放行；
    /// 前提值无任何引用 → 随包消失不构成空桶。
    /// 注意映射引用 actionType 指向被删包本身时先被「仍被引用」拦截（前一测试），
    /// 值级覆盖场景的引用 actionType 指向其他行为。
    #[test]
    fn validate_delete_value_level_coverage() {
        let mut cat = ValidationCatalog::default();
        cat.packs
            .push(pack("img", "图片", "user", "fileExt", "jpg,png"));
        cat.packs
            .push(pack("img2", "图片2", "user", "fileExt", "png"));
        let refs = [rref("fileExt", "jpg,png", "img2")];

        // jpg/png 均被引用；png 有 img2 前提覆盖，jpg 无 → uncovered = ["后缀 .jpg"]
        assert_eq!(
            cat.validate_delete("img", &refs).unwrap_err(),
            "删除「图片」后以下匹配条件将没有可用行为：后缀 .jpg"
        );

        // img3 前提覆盖 jpg → 放行
        cat.packs
            .push(pack("img3", "图片3", "user", "fileExt", "jpg"));
        assert!(cat.validate_delete("img", &refs).is_ok());

        // 前提值无任何引用（值随包消失）→ 放行
        let mut cat2 = ValidationCatalog::default();
        cat2.packs
            .push(pack("only", "孤包", "user", "textType", "magnet"));
        assert!(cat2.validate_delete("only", &[]).is_ok());

        // 通配值展示形态：任意文件（映射引用指向 w2，包 w 前提的 * 无他人覆盖）
        let mut cat3 = ValidationCatalog::default();
        cat3.packs.push(pack("w", "通配", "user", "fileExt", "*"));
        cat3.packs
            .push(pack("w2", "通配2", "builtin", "fileExt", "jpg"));
        let refs3 = [rref("fileExt", "*", "w2")];
        assert_eq!(
            cat3.validate_delete("w", &refs3).unwrap_err(),
            "删除「通配」后以下匹配条件将没有可用行为：任意文件"
        );

        // 文本特征展示形态
        let mut cat4 = ValidationCatalog::default();
        cat4.packs
            .push(pack("t", "文本包", "user", "textType", "url"));
        let refs4 = [rref("textType", "url", "other")];
        assert_eq!(
            cat4.validate_delete("t", &refs4).unwrap_err(),
            "删除「文本包」后以下匹配条件将没有可用行为：文本特征 url"
        );
    }
}
