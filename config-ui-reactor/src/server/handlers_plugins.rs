//! 插件 REST API —— Go `internal/server/plugins.go`（197 行）与
//! `internal/plugins/plugins.go` 服务端消费面（`Manifest`/`Setting` wire 结构、
//! `InstallFromZip`/`extractZip`/`safeZipName`、`Remove`）的移植。
//!
//! 路由（server.go:58-64）：`GET /api/plugins`、`POST /api/plugins/import`、
//! `DELETE /api/plugins/:id`、`GET/PUT /api/plugins/:id/settings`。
//!
//! wire 口径（对照基线逐字节钉死）：
//! * `Manifest` 结构体序列化，字段序 = Go 声明序（id/name/nameEn/version/
//!   specVersion/description/author/entry/permissions/settings）；omitempty ⇒
//!   `nameEn`/`version`/`description`/`author`/`permissions`/`settings` 空值
//!   不出场（`settings: []` 与 nil 同样省略 —— len==0 语义）。
//! * `Setting` 字段序：key/type/label/labelEn/default/filter/hint/hintEn/min/
//!   max/maxLength；`min`/`max` 是 `*float64`（nil 才省略），Go float64 整数
//!   形态输出 `1` 而非 `1.0`（校验强制整数，故恒走整数路径）。
//! * `GET /api/plugins` 是 gin.H ⇒ `{"errors":null,"plugins":[…]}`（字典序）。
//! * `pluginSettingsDTO` 结构体序：id/settings/values；`values` 是 map ⇒ 键
//!   字典序；`settings` 无 omitempty ⇒ manifest 未声明时输出 `null`。
//!
//! 错误口径：FormFile 缺失/zip 校验失败/ID 非法 → 400 `{"message":…}`；保存
//! 失败 → 500；绑定失败 panic → 500 空 body。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::generator::plugins as gplugins;

use super::dto::marshal_go_json;
use super::{HttpReply, ServerContext, ServerPaths};

// --------------------------------------------------------------------------- wire 结构

/// Go `plugins.Entry` 的 wire 投影。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WireEntry {
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub func: String,
}

/// Go `plugins.Setting` 的 wire 投影。字段序 = Go 声明序。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WireSetting {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(rename = "labelEn", skip_serializing_if = "String::is_empty")]
    pub label_en: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub filter: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hint: String,
    #[serde(rename = "hintEn", skip_serializing_if = "String::is_empty")]
    pub hint_en: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_go_f64_opt"
    )]
    pub min: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_go_f64_opt"
    )]
    pub max: Option<f64>,
    #[serde(rename = "maxLength", skip_serializing_if = "is_zero_i32")]
    pub max_length: i32,
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

/// Go `*float64` 的序列化：整数值输出整数形态（`1` 而非 `1.0`）。
fn serialize_go_f64_opt<S: serde::Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        // validateSettings 强制 min/max 为整数，恒走整数路径；非整数兜底 serde 形态
        Some(number) if number.fract() == 0.0 && number.abs() < 9.007_199_254_740_992e15 => {
            serializer.serialize_some(&(*number as i64))
        }
        Some(number) => serializer.serialize_some(number),
        None => serializer.serialize_none(),
    }
}

/// Go `plugins.Manifest` 的 wire 投影。`settings` omitempty（len==0 不出场）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct WireManifest {
    pub id: String,
    pub name: String,
    #[serde(rename = "nameEn", skip_serializing_if = "String::is_empty")]
    pub name_en: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(rename = "specVersion")]
    pub spec_version: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    pub entry: WireEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Vec<WireSetting>>,
}

impl WireSetting {
    fn to_generator(&self) -> gplugins::Setting {
        gplugins::Setting {
            key: self.key.clone(),
            kind: self.kind.clone(),
            label: self.label.clone(),
            label_en: self.label_en.clone(),
            default: self.default.clone(),
            filter: self.filter.clone(),
            hint: self.hint.clone(),
            hint_en: self.hint_en.clone(),
            min: self.min,
            max: self.max,
            max_length: self.max_length,
        }
    }
}

impl WireManifest {
    /// 绑定后归一：`permissions`/`settings` 的 `Some(空)` → `None`
    ///（omitempty len==0 同形）。
    fn canonicalize(&mut self) {
        if self.permissions.as_ref().is_some_and(Vec::is_empty) {
            self.permissions = None;
        }
        if self.settings.as_ref().is_some_and(Vec::is_empty) {
            self.settings = None;
        }
    }

    fn settings_or_empty(&self) -> &[WireSetting] {
        self.settings.as_deref().unwrap_or(&[])
    }

    /// 转换为 generator 侧 Manifest（校验面复用）。
    fn to_generator(&self) -> gplugins::Manifest {
        gplugins::Manifest {
            id: self.id.clone(),
            name: self.name.clone(),
            name_en: self.name_en.clone(),
            version: self.version.clone(),
            spec_version: self.spec_version,
            description: self.description.clone(),
            author: self.author.clone(),
            entry: gplugins::Entry {
                kind: self.entry.kind.clone(),
                file: self.entry.file.clone(),
                func: self.entry.func.clone(),
            },
            permissions: self.permissions.clone().unwrap_or_default(),
            settings: self
                .settings_or_empty()
                .iter()
                .map(|setting| gplugins::Setting {
                    key: setting.key.clone(),
                    kind: setting.kind.clone(),
                    label: setting.label.clone(),
                    label_en: setting.label_en.clone(),
                    default: setting.default.clone(),
                    filter: setting.filter.clone(),
                    hint: setting.hint.clone(),
                    hint_en: setting.hint_en.clone(),
                    min: setting.min,
                    max: setting.max,
                    max_length: setting.max_length,
                })
                .collect(),
        }
    }
}

// --------------------------------------------------------------------------- 目录加载

/// Go `plugins.Catalog`：wire manifest + 错误累积（nil → 响应 `null`）。
pub(crate) struct PluginCatalog {
    pub plugins: Vec<WireManifest>,
    pub errors: Vec<String>,
}

/// Go `plugins.readManifest`：读 `plugin.json` → 解析（校验）→ 目录名一致。
/// 校验复用 generator::plugins（与 Go 同一错误文案口径）。
fn read_manifest(dir: &Path) -> Result<WireManifest, String> {
    let raw = std::fs::read(dir.join("plugin.json")).map_err(|error| error.to_string())?;
    let manifest = parse_manifest_wire(&raw)?;
    let dir_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if manifest.id != dir_name {
        return Err(format!(
            "目录名 {dir_name:?} 与 manifest id {:?} 不一致",
            manifest.id
        ));
    }
    Ok(manifest)
}

/// Go `parseManifest`：剥 BOM → 解析 → 校验（经 generator 面复用）。
/// 🔴 走 [`gplugins::validate_manifest_body`]（**不含内置 ID 检查**）——目录加载
/// 与 zip 安装共用本解析，随包内置插件 quick_switch 以标准插件形态存在于
/// `data/plugins/`；内置 ID 冒名拦截由调用方按路径决定：zip 安装在下方补
/// [`gplugins::validate_manifest`] 严格版，目录列表天然放行（2026-10-02 修：
/// 之前在此误用严格版 ⇒ 面板列表页报「与内置插件冲突」）。
pub(crate) fn parse_manifest_wire(raw: &[u8]) -> Result<WireManifest, String> {
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    let mut value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|error| format!("plugin.json 解析失败: {error}"))?;
    crate::generator::model::strip_null_fields(&mut value);
    let mut wire: WireManifest =
        serde_json::from_value(value).map_err(|error| format!("plugin.json 解析失败: {error}"))?;
    wire.canonicalize();
    gplugins::validate_manifest_body(&wire.to_generator())?;
    Ok(wire)
}

/// Go `plugins.LoadCatalog`：缺目录 = 空（NotFound 不记错）；点前缀目录跳过
/// （导入临时目录残留）；按 ID 稳定排序。
pub(crate) fn load_plugin_catalog(paths: &ServerPaths) -> PluginCatalog {
    let mut catalog = PluginCatalog {
        plugins: Vec::new(),
        errors: Vec::new(),
    };
    let entries = match std::fs::read_dir(&paths.user_plugins) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                catalog.errors.push(format!("读取插件目录失败: {error}"));
            }
            return catalog;
        }
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) || name.starts_with('.') {
            continue;
        }
        match read_manifest(&entry.path()) {
            Ok(manifest) => catalog.plugins.push(manifest),
            Err(error) => catalog.errors.push(format!("{name}: {error}")),
        }
    }
    catalog.plugins.sort_by(|a, b| a.id.cmp(&b.id));
    catalog
}

// --------------------------------------------------------------------------- 导入（zip）

/// zip 导入防护上限：防解压炸弹（数量/总量/单文件）。Go `maxZipEntries` 等。
const MAX_ZIP_ENTRIES: usize = 500;
const MAX_ZIP_TOTAL: u64 = 64 << 20; // 64 MB
const MAX_ZIP_FILE: u64 = 32 << 20; // 32 MB

/// Go `safeZipName`：拒绝绝对路径、盘符与 `..` 穿越（仅接受包内相对路径）。
fn safe_zip_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\') {
        return false;
    }
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return false; // Windows 盘符 (C:/...)
    }
    !name.split('/').any(|segment| segment == "..")
}

/// Go `extractZip`：解压到临时目录，拒绝 zip-slip、限量防炸弹。
/// `data` 已带 64MB+1 截断检查。
fn extract_zip(data: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|error| format!("不是有效的 zip 文件: {error}"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!("插件包文件数超过上限 ({MAX_ZIP_ENTRIES})"));
    }
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let raw_name = file.name().to_string();
        let name = raw_name.replace('\\', "/"); // Go filepath.ToSlash
        if !safe_zip_name(&name) {
            return Err(format!("插件包含不安全路径: {raw_name:?}"));
        }
        if file.is_dir() {
            continue;
        }
        if file.size() > MAX_ZIP_FILE {
            return Err(format!(
                "插件包内单文件超过上限 ({} MB): {}",
                MAX_ZIP_FILE >> 20,
                raw_name
            ));
        }
        let target = dest.join(
            name.split('/')
                .fold(std::path::PathBuf::new(), |acc, segment| acc.join(segment)),
        );
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|error| error.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|error| error.to_string())?;
        total += file.size();
        if total > MAX_ZIP_TOTAL {
            return Err(format!(
                "插件包解压总量超过上限 ({} MB)",
                MAX_ZIP_TOTAL >> 20
            ));
        }
    }
    Ok(())
}

/// Go `os.MkdirTemp(userDir, ".import-")` 等价：父目录缺失先建再重试一次。
fn create_import_tmp(user_dir: &Path) -> Result<std::path::PathBuf, String> {
    let attempt = |base: &Path| -> Option<std::path::PathBuf> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = base.join(format!(".import-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).ok()?;
        Some(dir)
    };
    attempt(user_dir)
        .or_else(|| {
            std::fs::create_dir_all(user_dir).ok()?;
            attempt(user_dir)
        })
        .ok_or_else(|| "创建导入临时目录失败".to_string())
}

/// Go `plugins.InstallFromZip`：解压到临时目录 → 校验 manifest → 按 id 原子
/// rename 落盘到 `userDir/<id>`。zip 根可直接是包内容，也可带唯一顶层目录。
pub(crate) fn install_from_zip(data: &[u8], user_dir: &Path) -> Result<WireManifest, String> {
    if data.len() as u64 > MAX_ZIP_TOTAL {
        return Err(format!("插件包超过大小上限 ({} MB)", MAX_ZIP_TOTAL >> 20));
    }
    let tmp_dir = create_import_tmp(user_dir)?;
    let result = (|| {
        extract_zip(data, &tmp_dir)?;

        // 定位 plugin.json：优先解压根，其次唯一子目录（GitHub 源码 zip 形态）
        let mut root = tmp_dir.clone();
        if !root.join("plugin.json").exists() {
            let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&root)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .collect();
            if entries.len() != 1 || !entries[0].file_type().map(|k| k.is_dir()).unwrap_or(false) {
                return Err("zip 中找不到 plugin.json (应在压缩包根或唯一顶层目录下)".to_string());
            }
            root = entries[0].path();
            if !root.join("plugin.json").exists() {
                return Err("zip 中找不到 plugin.json (应在压缩包根或唯一顶层目录下)".to_string());
            }
        }

        let raw = std::fs::read(root.join("plugin.json")).map_err(|error| error.to_string())?;
        let manifest = parse_manifest_wire(&raw)?;
        // 导入路径走严格版：第三方包不得冒用内置 ID (目录列表路径不受此限)。
        gplugins::validate_manifest(&manifest.to_generator())?;

        let dest = user_dir.join(&manifest.id);
        if dest.exists() {
            return Err(format!(
                "插件「{}」(ID {}) 已存在, 如需覆盖请先删除",
                manifest.name, manifest.id
            ));
        }
        std::fs::rename(&root, &dest).map_err(|error| error.to_string())?;
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp_dir); // 失败即清空临时目录；成功路径已 rename 走
    }
    result
}

// --------------------------------------------------------------------------- multipart

/// gin `c.FormFile("file")` 的最小解析：从 Content-Type 取 boundary，取第一个
/// **带非空 filename** 的 `name="file"` part（无 filename 的 part 进 Value，
/// 不算文件）。解析失败 ⇒ Err（400）。
pub(crate) fn multipart_file(
    content_type: &str,
    body: &[u8],
    field: &str,
) -> Result<(String, Vec<u8>), ()> {
    // boundary 参数提取（参数名不区分大小写；值可带引号）
    let lower = content_type.to_ascii_lowercase();
    if !lower.starts_with("multipart/form-data") {
        return Err(());
    }
    let mut boundary: Option<String> = None;
    for param in content_type.split(';').map(str::trim) {
        let Some(eq) = param.find('=') else {
            continue;
        };
        if param[..eq].eq_ignore_ascii_case("boundary") {
            boundary = Some(param[eq + 1..].trim_matches('"').to_string());
        }
    }
    let boundary = boundary.filter(|value| !value.is_empty()).ok_or(())?;

    let dash_boundary = format!("--{boundary}");
    let mut cursor = 0usize;
    while let Some(start) = find(&body[cursor..], dash_boundary.as_bytes()) {
        let mut pos = cursor + start + dash_boundary.len();
        // 结尾哨兵 "--boundary--" ⇒ 无更多 part
        if body.get(pos..pos + 2) == Some(b"--") {
            break;
        }
        // 跳过 \r\n
        if body.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        }
        let Some(header_end) = find(&body[pos..], b"\r\n\r\n") else {
            break;
        };
        let headers = String::from_utf8_lossy(&body[pos..pos + header_end]).to_string();
        let body_start = pos + header_end + 4;
        // part 体到下一个 --boundary 前的 \r\n
        let Some(next) = find(&body[body_start..], dash_boundary.as_bytes()) else {
            break;
        };
        let mut part_end = body_start + next;
        if body[body_start..part_end].ends_with(b"\r\n") {
            part_end -= 2;
        }
        cursor = body_start + next;

        // Content-Disposition 解析：name 精确匹配 + filename 非空
        let mut name_ok = false;
        let mut filename = String::new();
        for line in headers.split("\r\n") {
            let lower_line = line.to_ascii_lowercase();
            if !lower_line.starts_with("content-disposition:") {
                continue;
            }
            for piece in line.split(';').map(str::trim) {
                if let Some(value) = piece.strip_prefix("name=") {
                    name_ok = value.trim_matches('"') == field;
                } else if let Some(value) = piece.strip_prefix("filename=") {
                    filename = value.trim_matches('"').to_string();
                }
            }
        }
        if name_ok && !filename.is_empty() {
            return Ok((filename, body[body_start..part_end].to_vec()));
        }
    }
    Err(())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// --------------------------------------------------------------------------- handlers

fn json_message(status: u16, message: &str) -> HttpReply {
    HttpReply::json(
        status,
        marshal_go_json(&serde_json::json!({ "message": message })),
    )
}

/// Go `findUserPlugin`：用户插件目录里按 ID 找 manifest。
fn find_user_plugin<'a>(catalog: &'a PluginCatalog, id: &str) -> Option<&'a WireManifest> {
    catalog.plugins.iter().find(|manifest| manifest.id == id)
}

/// Go `GetPluginsHandler`：`{"errors":…,"plugins":[…]}`（gin.H 字典序）。
pub(crate) fn get_plugins(ctx: &ServerContext) -> HttpReply {
    let catalog = load_plugin_catalog(&ctx.paths);
    // 外层 gin.H（errors<plugins 字典序）+ 内层 Manifest 结构体声明序 ——
    // 分段序列化后手工拼装（经 Value 中转会全量重排）。
    let errors_json = if catalog.errors.is_empty() {
        "null".to_string() // Go nil slice → null
    } else {
        marshal_go_json(&catalog.errors)
    };
    let body = format!(
        "{{\"errors\":{},\"plugins\":{}}}",
        errors_json,
        marshal_go_json(&catalog.plugins)
    );
    HttpReply::json(200, body)
}

/// Go `ImportPluginHandler`：multipart 字段 file（zip）→ 校验安装 → 200 manifest。
pub(crate) fn import_plugin(ctx: &ServerContext, content_type: &str, body: &[u8]) -> HttpReply {
    let Ok((_filename, zip_bytes)) = multipart_file(content_type, body, "file") else {
        return json_message(400, "缺少上传文件 (multipart 字段 file)");
    };
    match install_from_zip(&zip_bytes, &ctx.paths.user_plugins) {
        Ok(manifest) => HttpReply::json(200, marshal_go_json(&manifest)),
        Err(message) => json_message(400, &message),
    }
}

/// Go `DeletePluginHandler`：删除用户插件目录；**刻意不清理**
/// plugin-settings.json（卸载重装场景保留用户已填设置）。
pub(crate) fn delete_plugin(ctx: &ServerContext, id: &str) -> HttpReply {
    match remove_plugin(&ctx.paths.user_plugins, id) {
        Ok(()) => json_message(200, "ok"),
        Err(message) => json_message(400, &message),
    }
}

/// Go `plugins.Remove`：非法 ID / 不存在均拒绝。
/// 2026-10-02 P4 放行内置 ID：随包内置插件可删除（目录移除 + 面板写墓碑
/// `config.options.plugins.removed`，与本函数同构 —— 墓碑属 config 状态源）。
fn remove_plugin(user_dir: &Path, id: &str) -> Result<(), String> {
    if !super::settings_store::is_valid_plugin_id(id) {
        return Err(format!("插件 ID {id:?} 不合法"));
    }
    let dir = user_dir.join(id);
    if !dir.exists() {
        return Err(format!("插件「{id}」不存在"));
    }
    std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())
}

// ---------------- 插件设置（声明式 schema + 值） ----------------

/// Go `mergedValues`：manifest 默认值与已存值合并（只含声明的键，未存回落默认）。
fn merged_values(
    manifest: &WireManifest,
    stored: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for setting in manifest.settings_or_empty() {
        match stored.get(&setting.key) {
            Some(value) => {
                out.insert(setting.key.clone(), value.clone());
            }
            None => {
                out.insert(setting.key.clone(), setting.default.clone());
            }
        }
    }
    out
}

/// Go `pluginSettingsDTO`：结构体字段序 id/settings/values；settings 无
/// omitempty（manifest 未声明 ⇒ null）；values 是 map ⇒ 字典序。
#[derive(Serialize)]
struct PluginSettingsDto<'a> {
    id: &'a str,
    settings: Option<&'a Vec<WireSetting>>,
    values: &'a BTreeMap<String, String>,
}

/// Go `GetPluginSettingsHandler`：声明 + 合并默认值后的完整值。
pub(crate) fn get_plugin_settings(ctx: &ServerContext, id: &str) -> HttpReply {
    let catalog = load_plugin_catalog(&ctx.paths);
    let Some(manifest) = find_user_plugin(&catalog, id) else {
        return json_message(404, &format!("插件「{id}」不存在"));
    };
    let stored = ctx.plugin_settings.load_for(id);
    let values = merged_values(manifest, &stored);
    let dto = PluginSettingsDto {
        id: &manifest.id,
        settings: manifest.settings.as_ref(),
        values: &values,
    };
    HttpReply::json(200, marshal_go_json(&dto))
}

/// Go `saveSettingsRequest` 绑定：`{"values": {…}}`。null 值 ⇒ 空串（Go
/// Unmarshal null 进 string 的零值语义）；`values` 缺失/null ⇒ 空表。
fn bind_save_request(body: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let raw = body.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(body);
    let mut value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|error| format!("请求体不是合法 JSON: {error}"))?;
    crate::generator::model::strip_null_fields(&mut value);
    let Some(map) = value.get("values").and_then(|v| v.as_object()) else {
        // "values":null / 缺失 ⇒ nil map（无键可写）
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (key, value) in map {
        match value {
            serde_json::Value::String(text) => {
                out.insert(key.clone(), text.clone());
            }
            serde_json::Value::Null => {
                out.insert(key.clone(), String::new()); // Go null → 零值 ""
            }
            _ => {
                return Err(format!(
                    "请求体不是合法 JSON: json: cannot unmarshal {value} into Go value of type string"
                ));
            }
        }
    }
    Ok(out)
}

/// Go `SavePluginSettingsHandler`：键必须在 manifest 声明过、值过
/// ValidateSettingValue；按键排序校验保证同一错误输入恒得同一条错误；
/// 校验失败整单拒绝（不做部分写入）。
pub(crate) fn save_plugin_settings(ctx: &ServerContext, id: &str, body: &[u8]) -> HttpReply {
    let catalog = load_plugin_catalog(&ctx.paths);
    let Some(manifest) = find_user_plugin(&catalog, id) else {
        return json_message(404, &format!("插件「{id}」不存在"));
    };
    let values = match bind_save_request(body) {
        Ok(values) => values,
        Err(message) => return json_message(400, &message),
    };
    // 先按 key 排序校验（BTreeMap 天然有序），保证同输入同错误
    let settings = manifest.settings_or_empty();
    for (key, value) in &values {
        let Some(declared) = settings.iter().find(|setting| &setting.key == key) else {
            return json_message(400, &format!("设置项「{key}」不在插件的声明里"));
        };
        if let Err(error) = gplugins::validate_setting_value(&declared.to_generator(), value) {
            return json_message(
                400,
                &format!("设置项「{}」的值不合法: {}", declared.label, error),
            );
        }
    }
    if let Err(message) = ctx.plugin_settings.save(id, &values) {
        return json_message(500, &format!("保存设置失败: {message}"));
    }
    let stored = ctx.plugin_settings.load_for(id);
    let merged = merged_values(manifest, &stored);
    let dto = PluginSettingsDto {
        id: &manifest.id,
        settings: manifest.settings.as_ref(),
        values: &merged,
    };
    HttpReply::json(200, marshal_go_json(&dto))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归 (2026-10-02): 目录列表路径必须放行内置 ID —— 随包内置插件
    /// quick_switch 以标准插件形态存在于 data/plugins/, 面板列表此前误走
    /// 严格版校验而报「与内置插件冲突」。冒名拦截只属导入 API (zip 安装处)。
    #[test]
    fn parse_manifest_wire_allows_builtin_id() {
        let raw = r#"{
            "id": "quick_switch", "name": "快速切换", "nameEn": "Quick Switch",
            "version": "1.0.0", "specVersion": 1, "description": "内置",
            "entry": {"kind": "script", "file": "main.ahk", "func": "QuickSwitchMain"},
            "permissions": ["window"]
        }"#
        .as_bytes();
        let manifest = parse_manifest_wire(raw)
            .unwrap_or_else(|error| panic!("目录解析应放行内置 ID: {error}"));
        assert_eq!(manifest.id, "quick_switch");
    }
}
