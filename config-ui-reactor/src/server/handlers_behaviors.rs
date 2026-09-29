//! 行为包 REST API —— Go `internal/server/behaviors.go`（145 行全文件）与
//! `internal/behaviors/behaviors.go` 中服务端消费面（`Pack`/`Entry`/`EntryParams`/
//! `AppliesToEntry` wire 结构、`LoadCatalog`/`readPack`、`ValidateManifest`、
//! `WriteUserPack`/`RemoveUserPack`、`ResolveRuleAction`）的移植。
//!
//! 路由（server.go:52-56）：`GET/POST /api/behaviors`、`PUT/DELETE /api/behaviors/:id`、
//! `POST /api/behaviors/apply`。变更不自动重启引擎（前端显式 apply）。
//!
//! wire 口径（对照基线逐字节钉死）：
//! * `Pack` 是**结构体序列化**（字段按 Go 声明序：id/name/nameEn/version/
//!   description/specVersion/appliesTo/entry/permissions/boundTypeId/source），
//!   omitempty ⇒ `nameEn`/`version`/`description`/`permissions`/`boundTypeId`/
//!   `source` 空值不出场；`appliesTo` **无** omitempty ⇒ nil → `null`；
//!   `entry.params` 是非指针结构体 ⇒ `omitempty` 无效，恒输出（`"params":{}`）。
//! * `GET` 响应是 gin.H map ⇒ 键按字典序：builtin < errors < user；`errors`
//!   是 nil slice ⇒ `null`，builtin/user 归一为空数组（Go 手工 `[]`）。
//! * 错误路径：ShouldBindJSON 失败 / RemoveUserPack 失败 → panic → gin Recovery
//!   → **500 空 body**；校验失败 → 400 `{"message":…}`。
//!
//! 错误文案逐字对照 Go（`%q` ⇒ Rust `{:?}`，`%d`/`%s` 同形）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::generator::config::parse_config;
use crate::generator::model::strip_null_fields;

use super::dto::marshal_go_json;
use super::validate::{
    RawAppliesToEntry, RawPack, RuleRef, ValidationCatalog, catalog_from_packs, find_text_feature,
    is_builtin_action, is_custom_ref, normalize_ext, text_feature_hint,
};
use super::{HttpReply, ServerContext, VERSION};

/// Go `behaviors.SpecVersion`。
const SPEC_VERSION: i32 = 1;

// --------------------------------------------------------------------------- wire 结构

/// Go `behaviors.AppliesToEntry`。`default` omitempty。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WireAppliesToEntry {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exts: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(skip_serializing_if = "is_false")]
    pub default: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Go `behaviors.EntryParams`：两字段均 omitempty（结构体本身恒出场）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WireEntryParams {
    #[serde(rename = "actionValue", skip_serializing_if = "String::is_empty")]
    pub action_value: String,
    #[serde(rename = "workingDir", skip_serializing_if = "String::is_empty")]
    pub working_dir: String,
}

/// Go `behaviors.Entry`。注意 `params` 带 omitempty 但是非指针结构体 ⇒ 恒输出。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WireEntry {
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub action: String,
    pub params: WireEntryParams,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub func: String,
}

/// Go `behaviors.Pack` 的 wire 全字段投影。字段序 = Go 声明序。
/// `appliesTo` 无 omitempty：`None`（nil）→ `null`、`Some(vec![])` → `[]`。
/// `permissions`/`source`/`boundTypeId` omitempty（`Some(空)` 归一为 `None`，
/// 与 Go len==0 出场语义一致 —— 绑定后由 [`WirePack::canonicalize`] 处理）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WirePack {
    pub id: String,
    pub name: String,
    #[serde(rename = "nameEn", skip_serializing_if = "String::is_empty")]
    pub name_en: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(rename = "specVersion")]
    pub spec_version: i32,
    #[serde(rename = "appliesTo")]
    pub applies_to: Option<Vec<WireAppliesToEntry>>,
    pub entry: WireEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(rename = "boundTypeId", skip_serializing_if = "String::is_empty")]
    pub bound_type_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
}

impl WirePack {
    /// 绑定后归一：`permissions` 的 `Some(空)` → `None`（omitempty len==0 同形）。
    pub(crate) fn canonicalize(&mut self) {
        if self.permissions.as_ref().is_some_and(Vec::is_empty) {
            self.permissions = None;
        }
    }

    /// Go 直接遍历 `p.AppliesTo` 的口径（nil 安全）。
    fn applies(&self) -> &[WireAppliesToEntry] {
        self.applies_to.as_deref().unwrap_or(&[])
    }
}

/// Go `ShouldBindJSON` 的等价绑定：剥 BOM → 解析（`null` ⇒ 零值包，字段级
/// `null` 经 [`strip_null_fields`] 归零 —— Go json.Unmarshal 同语义）→ 类型
/// 不匹配/尾随内容报错。失败由调用方映射为 500 空 body（Go panic → Recovery）。
pub(crate) fn bind_wire_pack(body: &[u8]) -> Result<WirePack, ()> {
    let raw = body.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(body);
    let mut value: serde_json::Value = serde_json::from_slice(raw).map_err(|_| ())?;
    if value.is_null() {
        return Ok(WirePack::default());
    }
    strip_null_fields(&mut value);
    let mut pack: WirePack = serde_json::from_value(value).map_err(|_| ())?;
    pack.canonicalize();
    Ok(pack)
}

// --------------------------------------------------------------------------- 校验（behaviors.go:366-436）

/// Go `behaviors.idPattern` `^[a-z][a-z0-9_]{0,31}$`。
fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// Go `pluginActionPattern` `^plugin:[a-z][a-z0-9_]{0,31}:[A-Za-z0-9_\-]{1,64}$`
/// （手写等价判定，语义逐字对齐；正确性由单测钉住）。
fn is_plugin_action(action: &str) -> bool {
    let Some(rest) = action.strip_prefix("plugin:") else {
        return false;
    };
    let Some((plugin_id, action_name)) = rest.split_once(':') else {
        return false;
    };
    let id_bytes = plugin_id.as_bytes();
    if id_bytes.is_empty() || id_bytes.len() > 32 || !id_bytes[0].is_ascii_lowercase() {
        return false;
    }
    if !id_bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
    {
        return false;
    }
    let name = action_name.as_bytes();
    !name.is_empty()
        && name.len() <= 64
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

/// Go `behaviors.ValidateManifest`（**就地**校验 + 归一化：boundTypeId 与
/// appliesTo[].value 会被改写为小写/剥前缀 —— WriteUserPack 落盘的是归一化后的
/// manifest，响应体亦然）。`known_text` 为 `None` 时容忍悬空引用（加载链路）。
pub(crate) fn validate_manifest(
    pack: &mut WirePack,
    known_text: Option<&dyn Fn(&str) -> bool>,
) -> Result<(), String> {
    if !is_valid_id(&pack.id) {
        return Err(format!(
            "行为 ID {:?} 不合法 (须匹配 ^[a-z][a-z0-9_]{{0,31}}$)",
            pack.id
        ));
    }
    if pack.spec_version != SPEC_VERSION {
        return Err(format!(
            "specVersion 必须为 {} (当前 {})",
            SPEC_VERSION, pack.spec_version
        ));
    }
    if pack.name.trim().is_empty() {
        return Err(format!("行为「{}」缺少名称 (name)", pack.id));
    }
    if pack.applies().is_empty() {
        return Err(format!("行为「{}」缺少生效前提 (appliesTo)", pack.id));
    }
    if is_custom_ref(pack.bound_type_id.trim()) {
        // 容错: 允许写 "type:<id>" 或裸 id 两种形态, 归一后校验存在性
        pack.bound_type_id = pack
            .bound_type_id
            .trim()
            .strip_prefix("type:")
            .unwrap_or(pack.bound_type_id.trim())
            .to_string();
    }
    if !pack.bound_type_id.is_empty()
        && let Some(resolve) = known_text
        && !resolve(&pack.bound_type_id)
    {
        return Err(format!(
            "行为「{}」绑定的匹配类型「{}」不存在",
            pack.id, pack.bound_type_id
        ));
    }
    for (index, entry) in pack
        .applies_to
        .get_or_insert_with(Vec::new)
        .iter_mut()
        .enumerate()
    {
        match entry.kind.as_str() {
            "fileExt" => {
                if entry.exts.is_empty() {
                    return Err(format!(
                        "行为「{}」第 {} 条匹配条件缺少文件扩展名",
                        pack.id,
                        index + 1
                    ));
                }
                for (ext_index, ext) in entry.exts.iter().enumerate() {
                    if normalize_ext(ext).is_empty() {
                        return Err(format!(
                            "行为「{}」第 {} 条匹配条件的第 {} 个扩展名为空",
                            pack.id,
                            index + 1,
                            ext_index + 1
                        ));
                    }
                }
            }
            "textType" => {
                let v = entry.value.trim().to_lowercase();
                if is_custom_ref(&v) {
                    // 自定义匹配类型引用 (方案 C7): 存在性只在保存链路裁决
                    if let Some(resolve) = known_text
                        && !resolve(v.strip_prefix("type:").unwrap_or(&v))
                    {
                        return Err(format!(
                            "行为「{}」第 {} 条匹配条件引用的匹配类型「{}」不存在",
                            pack.id,
                            index + 1,
                            entry.value
                        ));
                    }
                    entry.value = v;
                    continue;
                }
                if find_text_feature(&v).is_none() {
                    return Err(format!(
                        "行为「{}」第 {} 条匹配条件的文本特征「{}」无效（可选：{}，或已自定义的匹配类型）",
                        pack.id,
                        index + 1,
                        entry.value,
                        text_feature_hint()
                    ));
                }
                entry.value = v;
            }
            _ => {
                return Err(format!(
                    "行为「{}」第 {} 条匹配条件的分类「{}」无效（可选：文本内容 / 文件类型）",
                    pack.id,
                    index + 1,
                    entry.kind
                ));
            }
        }
    }
    match pack.entry.kind.as_str() {
        "builtin" => {
            // 内置基础动作白名单, 或插件运行时动作引用 (plugin:<id>:<name>)
            if !is_builtin_action(&pack.entry.action) && !is_plugin_action(&pack.entry.action) {
                return Err(format!(
                    "行为「{}」的 entry.action {:?} 不是内置基础动作或插件动作引用",
                    pack.id, pack.entry.action
                ));
            }
        }
        "script" => {
            if pack.entry.file.trim().is_empty() || pack.entry.func.trim().is_empty() {
                return Err(format!(
                    "行为「{}」的 script entry 缺少 file 或 func",
                    pack.id
                ));
            }
        }
        _ => {
            return Err(format!(
                "行为「{}」的 entry.kind {:?} 不合法 (可选: builtin / script)",
                pack.id, pack.entry.kind
            ));
        }
    }
    Ok(())
}

// --------------------------------------------------------------------------- 目录加载（behaviors.go:106-208）

/// Go `behaviors.Catalog` 的完整投影（wire 包 + 加载错误累积）。
pub(crate) struct FullCatalog {
    pub packs: Vec<WirePack>,
    /// Go `Catalog.Errors`：nil（无错误）→ 响应 `null`。
    pub errors: Vec<String>,
}

/// Go `behaviors.readPack`：读 `behavior.json` → BOM 剥除 → 解析 → 目录名比对 →
/// `ValidateManifest`（加载链路不传 knownText —— 容忍遗留数据的悬空引用）。
fn read_pack(dir: &Path) -> Result<WirePack, String> {
    let raw = std::fs::read(dir.join("behavior.json")).map_err(|error| error.to_string())?;
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&raw);
    let mut pack: WirePack =
        serde_json::from_slice(raw).map_err(|error| format!("behavior.json 解析失败: {error}"))?;
    pack.canonicalize();
    let dir_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if pack.id != dir_name {
        return Err(format!(
            "目录名 {dir_name:?} 与 manifest id {:?} 不一致",
            pack.id
        ));
    }
    validate_manifest(&mut pack, None)?;
    Ok(pack)
}

/// Go `behaviors.loadDir`：目录缺失 = 空；单个坏包跳过并记错误（错误隔离）。
fn load_dir(dir: &Path, source: &str) -> (Vec<WirePack>, Vec<String>, Option<String>) {
    let mut packs = Vec::new();
    let mut errors = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return (packs, errors, None); // 目录缺失 = 无该来源包 (正常场景)
            }
            return (packs, errors, Some(error.to_string()));
        }
    };
    for entry in entries.filter_map(Result::ok) {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        match read_pack(&entry.path()) {
            Ok(mut pack) => {
                pack.source = source.to_string();
                packs.push(pack);
            }
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    (packs, errors, None)
}

/// Go `server.loadBehaviorCatalog`（behaviors.go:41-47）：内置 = exe 同级
/// `behaviors/`，用户 = `../data/behaviors`。各来源按 ID 稳定排序，builtin 在前。
pub(crate) fn load_behavior_catalog(paths: &super::ServerPaths) -> FullCatalog {
    let mut catalog = FullCatalog {
        packs: Vec::new(),
        errors: Vec::new(),
    };
    for (dir, source) in [
        (&paths.builtin_behaviors, "builtin"),
        (&paths.user_behaviors, "user"),
    ] {
        let (mut packs, mut errors, fatal) = load_dir(dir, source);
        if let Some(fatal) = fatal {
            catalog.errors.push(format!("{source}: {fatal}"));
        }
        catalog.errors.append(&mut errors);
        // Go sortPacks：按 ID 稳定排序
        packs.sort_by(|a, b| a.id.cmp(&b.id));
        catalog.packs.append(&mut packs);
    }
    catalog
}

/// 从 [`FullCatalog`] 投影校验目录（复用覆盖/删除校验的单一实现）。
pub(crate) fn validation_catalog(catalog: &FullCatalog) -> ValidationCatalog {
    catalog_from_packs(
        catalog
            .packs
            .iter()
            .map(|pack| RawPack {
                id: pack.id.clone(),
                name: pack.name.clone(),
                applies_to: pack
                    .applies()
                    .iter()
                    .map(|entry| RawAppliesToEntry {
                        kind: entry.kind.clone(),
                        exts: entry.exts.clone(),
                        value: entry.value.clone(),
                    })
                    .collect(),
                source: pack.source.clone(),
            })
            .collect(),
    )
}

/// 服务端共享入口：加载完整目录并投影为校验目录（selected-action test 的
/// 覆盖校验与删除校验同源 —— 单一真源，不各读一遍目录）。
pub(crate) fn behavior_validation_catalog(ctx: &ServerContext) -> ValidationCatalog {
    validation_catalog(&load_behavior_catalog(&ctx.paths))
}

// --------------------------------------------------------------------------- 写盘（behaviors.go:522-548）

/// Go `behaviors.WriteUserPack`：校验（**保存链路传 knownText 钩子**）→
/// MarshalIndent 2 空格（HTML 转义开）+ 尾换行、无 BOM → `userDir/<id>/behavior.json`。
pub(crate) fn write_user_pack(
    user_dir: &Path,
    pack: &mut WirePack,
    known_text: Option<&dyn Fn(&str) -> bool>,
) -> Result<(), String> {
    if pack.source == "builtin" || is_builtin_action(&pack.id) {
        return Err(format!("行为标识「{}」与内置行为冲突", pack.id));
    }
    validate_manifest(pack, known_text)?;
    // MarshalIndent 2 空格 + HTML 转义（Go json.MarshalIndent 默认转义）。
    // 必须直接对 WirePack 做 to_string_pretty（声明序），经 Value 中转会被
    // BTreeMap 全量重排。
    let pretty = serde_json::to_string_pretty(pack).map_err(|error| error.to_string())?;
    let escaped = super::dto::go_html_escape_json(&pretty);
    let dir = user_dir.join(&pack.id);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    std::fs::write(dir.join("behavior.json"), escaped + "\n").map_err(|error| error.to_string())
}

/// Go `behaviors.RemoveUserPack`：内置 ID 拒绝；目录不存在亦视为成功（RemoveAll）。
pub(crate) fn remove_user_pack(user_dir: &Path, id: &str) -> Result<(), String> {
    if is_builtin_action(id) {
        return Err(format!("行为标识「{id}」与内置行为冲突"));
    }
    match std::fs::remove_dir_all(user_dir.join(id)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

// --------------------------------------------------------------------------- 钩子与引用投影

/// Go `matchTypeResolver`（behaviors.go:27-39）：`type:` 引用存在性钩子。
/// 配置读取失败 ⇒ `None`：退化为不校验存在性（容忍口径）。textType 引用查
/// matchTypes[]，fileExt 引用查 fileGroups[] —— 共用同一 "type:" 命名空间。
fn match_type_resolver(paths: &super::ServerPaths) -> Option<impl Fn(&str) -> bool + '_> {
    let cfg = parse_config(&paths.config_file, VERSION).ok()?;
    Some(move |id: &str| cfg.find_match_type(id).is_some() || cfg.file_group_exts(id).is_some())
}

/// Go `behaviorRuleRefs`（behaviors.go:52-63）：selectedAction.mappings 的
/// entries 投影为删除校验引用。
fn behavior_rule_refs(config: &crate::generator::model::Config) -> Vec<RuleRef> {
    let mut refs = Vec::new();
    if let Some(sa) = &config.selected_action {
        for mapping in &sa.mappings {
            for entry in &mapping.entries {
                refs.push(RuleRef {
                    match_type: mapping.match_type.clone(),
                    match_value: mapping.match_value.clone(),
                    action_type: entry.behavior.clone(),
                });
            }
        }
    }
    refs
}

// --------------------------------------------------------------------------- handlers

fn json_message(status: u16, message: &str) -> HttpReply {
    HttpReply::json(
        status,
        marshal_go_json(&serde_json::json!({ "message": message })),
    )
}

/// Go `GetBehaviorsHandler`：`{"builtin":[…],"errors":…,"user":[…]}`（gin.H
/// 键字典序；errors nil → null）。
pub(crate) fn get_behaviors(ctx: &ServerContext) -> HttpReply {
    let catalog = load_behavior_catalog(&ctx.paths);
    let mut builtin = Vec::new();
    let mut user = Vec::new();
    for pack in &catalog.packs {
        if pack.source == "builtin" {
            builtin.push(pack);
        } else {
            user.push(pack);
        }
    }
    // 外层是 gin.H（键字典序 builtin<errors<user），内层 Pack 是结构体序列化
    //（声明序）—— 不能经 serde_json::Value 中转（会全量重排），故分段序列化
    // 后手工拼装。
    let errors_json = if catalog.errors.is_empty() {
        "null".to_string() // Go nil slice → null
    } else {
        marshal_go_json(&catalog.errors)
    };
    let body = format!(
        "{{\"builtin\":{},\"errors\":{},\"user\":{}}}",
        marshal_go_json(&builtin),
        errors_json,
        marshal_go_json(&user)
    );
    HttpReply::json(200, body)
}

/// Go `rejectIfScriptEntry`：一期仅开放基础动作组合。
fn reject_if_script_entry(pack: &WirePack) -> Option<HttpReply> {
    if pack.entry.kind == "script" {
        return Some(json_message(
            400,
            "脚本行为将在后续版本支持, 当前请选择基础动作组合",
        ));
    }
    None
}

/// Go `CreateBehaviorHandler`：绑定 → Source=user → script 拒绝 → WriteUserPack
/// （带 knownText 钩子）→ 200 回显落盘后的包（含校验归一化）。
pub(crate) fn create_behavior(ctx: &ServerContext, body: &[u8]) -> HttpReply {
    let Ok(mut pack) = bind_wire_pack(body) else {
        return HttpReply::empty(500); // Go panic → Recovery
    };
    pack.source = "user".to_string();
    if let Some(reply) = reject_if_script_entry(&pack) {
        return reply;
    }
    let resolver = match_type_resolver(&ctx.paths);
    let known_text = resolver.as_ref().map(|r| r as &dyn Fn(&str) -> bool);
    match write_user_pack(&ctx.paths.user_behaviors, &mut pack, known_text) {
        Ok(()) => HttpReply::json(200, marshal_go_json(&pack)),
        Err(message) => json_message(400, &message),
    }
}

/// Go `UpdateBehaviorHandler`：绑定 → 路径 id 覆写 → 已存在且 Source=="user"
/// 否则 404 → WriteUserPack → 200 回显。
pub(crate) fn update_behavior(ctx: &ServerContext, id: &str, body: &[u8]) -> HttpReply {
    let Ok(mut pack) = bind_wire_pack(body) else {
        return HttpReply::empty(500);
    };
    pack.id = id.to_string();
    pack.source = "user".to_string();
    if let Some(reply) = reject_if_script_entry(&pack) {
        return reply;
    }
    let catalog = load_behavior_catalog(&ctx.paths);
    let existing = catalog.packs.iter().find(|pack| pack.id == id);
    let existing_ok = existing.is_some_and(|existing| existing.source == "user");
    if !existing_ok {
        return json_message(404, "仅可编辑用户自定义行为");
    }
    let resolver = match_type_resolver(&ctx.paths);
    let known_text = resolver.as_ref().map(|r| r as &dyn Fn(&str) -> bool);
    match write_user_pack(&ctx.paths.user_behaviors, &mut pack, known_text) {
        Ok(()) => HttpReply::json(200, marshal_go_json(&pack)),
        Err(message) => json_message(400, &message),
    }
}

/// Go `DeleteBehaviorHandler`：ValidateDelete（引用检查）→ RemoveUserPack →
/// `{"message":"ok"}`。删除失败 panic → 500。
pub(crate) fn delete_behavior(ctx: &ServerContext, id: &str) -> HttpReply {
    let catalog = load_behavior_catalog(&ctx.paths);
    let refs = match parse_config(&ctx.paths.config_file, VERSION) {
        Ok(config) => behavior_rule_refs(&config),
        // Go: loadSelectedAction panic → 500 空 body
        Err(_) => return HttpReply::empty(500),
    };
    if let Err(message) = validation_catalog(&catalog).validate_delete(id, &refs) {
        return json_message(400, &message);
    }
    match remove_user_pack(&ctx.paths.user_behaviors, id) {
        Ok(()) => json_message(200, "ok"),
        // Go: panic(err) → Recovery → 500 空 body
        Err(_) => HttpReply::empty(500),
    }
}

/// Go `ApplyBehaviorsHandler`：重启 KeyFlux 使行为变更生效。
pub(crate) fn apply_behaviors(ctx: &ServerContext) -> HttpReply {
    let restart_failed = !(ctx.hooks.restart_engine)();
    HttpReply::json(
        200,
        marshal_go_json(&serde_json::json!({ "restartFailed": restart_failed })),
    )
}
