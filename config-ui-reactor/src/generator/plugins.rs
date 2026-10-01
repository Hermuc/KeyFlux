//! 插件体系 —— Go **两处**源码的同名移植，合并在一个模块里:
//!
//! 1. `internal/plugins/plugins.go`（管理面）: 目录加载 [`load_catalog`] / manifest 校验
//!    [`validate_manifest`] / 目录快照 [`Catalog`]（仅移植生成端真正消费的部分）；
//! 2. `internal/script/generators/plugins.go`（注入面）: 模板注入块
//!    [`render_plugin_blocks`]（`PLUGIN_INCLUDES` / `PLUGIN_BOOTSTRAP`）。
//!
//! 目录口径（与 [`crate::generator::behaviors`] 的插件贡献包一致）：插件根 = **config.json
//! 同级** `plugins/`，每个子目录 = 一个插件包（目录名必须等于 manifest 的 `id`）。
//! Go 的 `SetPluginsDir` 注入的正是 `<config.json 目录>/plugins`。
//!
//! ⚠️ 与 Go 的**有意差异**：Go 用包级全局 `generators.PluginsDir` + 进程级缓存块 +
//! `generators.Cfg` 读停用表；本移植一律**显式传参**（`plugins_dir` + `disabled`），
//! 不引入进程级可变状态（迁移纪律）。
//!
//! 字节口径（易踩坑）：两块均以 `\n` **前导**（行尾拼接约定）—— 模板写作
//! `...Plugins.ahk{{ PLUGIN_INCLUDES }}`，零插件时返回空串即可保证产物字节不变。
//! 与 Go 的 `%q` / `%s` / `%d` 打印逐字对齐。

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

/// Go `plugins.SpecVersion`：当前插件包格式版本（manifest `specVersion` 不等即拒绝）。
pub const SPEC_VERSION: i32 = 1;

/// Go `plugins.BuiltinPluginIDs`：内置插件保留 ID 集（用户包不得占用）。
pub const BUILTIN_PLUGIN_IDS: [&str; 1] = ["quick_switch"];

/// Go `plugins.MaxSettingsPerPlugin`。
pub const MAX_SETTINGS_PER_PLUGIN: usize = 32;

/// Go `plugins.MaxSettingValueLen`。
pub const MAX_SETTING_VALUE_LEN: i64 = 1024;

/// Go `plugins.SettingsPermission`。
pub const SETTINGS_PERMISSION: &str = "settings";

/// Go `plugins.settingTypes`：合法设置项类型词表（协议一部分，两端必须一致）。
const SETTING_TYPES: [&str; 4] = ["char", "text", "number", "file"];

/// Go `plugins.Entry`：插件入口声明（当前仅 `script` 形态）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub kind: String,
    pub file: String,
    pub func: String,
}

/// Go `plugins.Setting`：声明式设置项（仅生成端用到的字段；`min`/`max` 为可空）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Setting {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(rename = "labelEn")]
    pub label_en: String,
    pub default: String,
    pub filter: String,
    pub hint: String,
    #[serde(rename = "hintEn")]
    pub hint_en: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    #[serde(rename = "maxLength")]
    pub max_length: i32,
}

/// Go `plugins.Manifest`（`plugin.json`）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(rename = "nameEn")]
    pub name_en: String,
    pub version: String,
    #[serde(rename = "specVersion")]
    pub spec_version: i32,
    pub description: String,
    pub author: String,
    pub entry: Entry,
    pub permissions: Vec<String>,
    pub settings: Vec<Setting>,
}

impl Manifest {
    /// Go `(*Manifest).HasPermission`。
    pub fn has_permission(&self, name: &str) -> bool {
        self.permissions.iter().any(|p| p == name)
    }
}

/// Go `plugins.Catalog`：用户插件目录快照（按 ID 字典序 + 逐包错误隔离）。
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub plugins: Vec<Manifest>,
    pub errors: Vec<String>,
}

// --------------------------------------------------------------- 校验（Go plugins.go）

/// Go `idPattern` `^[a-z][a-z0-9_]{0,31}$`（手写匹配，避免引入编译期正则）。
fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// Go `settingKeyPattern` `^[A-Za-z][A-Za-z0-9_]{0,31}$`。
fn is_valid_setting_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// Go `(*Setting).ValueLimit`。
pub(crate) fn value_limit(setting: &Setting) -> i64 {
    if setting.kind == "char" {
        return 1;
    }
    if setting.kind == "text"
        && setting.max_length > 0
        && (setting.max_length as i64) < MAX_SETTING_VALUE_LEN
    {
        return setting.max_length as i64;
    }
    MAX_SETTING_VALUE_LEN
}

/// Go `plugins.ValidateSettingValue`：空串一律合法；非空按类型校验。
pub(crate) fn validate_setting_value(setting: &Setting, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Ok(());
    }
    if value.chars().count() as i64 > value_limit(setting) {
        return Err(format!("{:?} 超过长度上限 {}", value, value_limit(setting)));
    }
    if value.contains('\0') {
        return Err("值不能包含 NUL 字符".to_string());
    }
    match setting.kind.as_str() {
        "char" => {
            let r = value.chars().next().unwrap_or('\0');
            let code = r as u32;
            if code < 0x20 || code == 0x7F {
                return Err(format!("{value:?} 不是可打印字符"));
            }
        }
        "number" => {
            let n: i64 = value.parse().map_err(|_| format!("{value:?} 不是整数"))?;
            if let Some(min) = setting.min
                && (n as f64) < min
            {
                return Err(format!("{n} 小于下限 {min}"));
            }
            if let Some(max) = setting.max
                && (n as f64) > max
            {
                return Err(format!("{n} 大于上限 {max}"));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Go `plugins.validateSettings`（设置块自洽校验；坏包在加载期即隔离）。
fn validate_settings(manifest: &Manifest) -> Result<(), String> {
    if manifest.settings.is_empty() {
        return Ok(());
    }
    if manifest.settings.len() > MAX_SETTINGS_PER_PLUGIN {
        return Err(format!(
            "插件「{}」声明的设置项过多 ({} > {})",
            manifest.id,
            manifest.settings.len(),
            MAX_SETTINGS_PER_PLUGIN
        ));
    }
    if !manifest.has_permission(SETTINGS_PERMISSION) {
        return Err(format!(
            "插件「{}」声明了 settings 但未申请 {:?} 权限",
            manifest.id, SETTINGS_PERMISSION
        ));
    }
    let mut seen: HashSet<String> = HashSet::new();
    for (index, setting) in manifest.settings.iter().enumerate() {
        if !is_valid_setting_key(&setting.key) {
            return Err(format!(
                "插件「{}」第 {} 个设置项的 key {:?} 不合法 (须匹配 ^[A-Za-z][A-Za-z0-9_]{{0,31}}$)",
                manifest.id,
                index + 1,
                setting.key
            ));
        }
        if !seen.insert(setting.key.clone()) {
            return Err(format!(
                "插件「{}」设置项 key {:?} 重复",
                manifest.id, setting.key
            ));
        }
        if !SETTING_TYPES.contains(&setting.kind.as_str()) {
            return Err(format!(
                "插件「{}」设置项 {:?} 的 type {:?} 不合法 (仅支持 char/text/number/file)",
                manifest.id, setting.key, setting.kind
            ));
        }
        if setting.label.trim().is_empty() {
            return Err(format!(
                "插件「{}」设置项 {:?} 缺少 label",
                manifest.id, setting.key
            ));
        }
        if let (Some(min), Some(max)) = (setting.min, setting.max)
            && min > max
        {
            return Err(format!(
                "插件「{}」设置项 {:?} 的 min 大于 max",
                manifest.id, setting.key
            ));
        }
        if setting.kind != "number" && (setting.min.is_some() || setting.max.is_some()) {
            return Err(format!(
                "插件「{}」设置项 {:?} 不是 number 类型, 不应带 min/max",
                manifest.id, setting.key
            ));
        }
        if setting.kind != "file" && !setting.filter.is_empty() {
            return Err(format!(
                "插件「{}」设置项 {:?} 不是 file 类型, 不应带 filter",
                manifest.id, setting.key
            ));
        }
        for v in [setting.min, setting.max].into_iter().flatten() {
            if v != (v as i64 as f64) {
                return Err(format!(
                    "插件「{}」设置项 {:?} 的 min/max 必须是整数",
                    manifest.id, setting.key
                ));
            }
        }
        if let Err(error) = validate_setting_value(setting, &setting.default) {
            return Err(format!(
                "插件「{}」设置项 {:?} 的默认值不合法: {}",
                manifest.id, setting.key, error
            ));
        }
    }
    Ok(())
}

/// Go `plugins.ValidateManifest`（加载期与导入 API 共用）。
pub fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if !is_valid_id(&manifest.id) {
        return Err(format!(
            "插件 ID {:?} 不合法 (须匹配 ^[a-z][a-z0-9_]{{0,31}}$)",
            manifest.id
        ));
    }
    if BUILTIN_PLUGIN_IDS.contains(&manifest.id.as_str()) {
        return Err(format!("插件 ID {:?} 与内置插件冲突", manifest.id));
    }
    if manifest.spec_version != SPEC_VERSION {
        return Err(format!(
            "specVersion 必须为 {} (当前 {})",
            SPEC_VERSION, manifest.spec_version
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err(format!("插件「{}」缺少名称 (name)", manifest.id));
    }
    match manifest.entry.kind.as_str() {
        "script" => {
            if manifest.entry.file.trim().is_empty() || manifest.entry.func.trim().is_empty() {
                return Err(format!(
                    "插件「{}」的 script entry 缺少 file 或 func",
                    manifest.id
                ));
            }
        }
        _ => {
            return Err(format!(
                "插件「{}」的 entry.kind {:?} 不合法 (当前仅支持 script)",
                manifest.id, manifest.entry.kind
            ));
        }
    }
    validate_settings(manifest)
}

// --------------------------------------------------------------- 加载（Go plugins.go）

/// Go `parseManifest`：剥 BOM → 解析 → `ValidateManifest`。
fn parse_manifest(raw: &[u8]) -> Result<Manifest, String> {
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    let manifest: Manifest =
        serde_json::from_slice(raw).map_err(|error| format!("plugin.json 解析失败: {error}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Go `readManifest`：读 `plugin.json`，目录名必须等于 manifest 的 `id`。
fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let raw = std::fs::read(dir.join("plugin.json")).map_err(|error| error.to_string())?;
    let manifest = parse_manifest(&raw)?;
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

/// Go `plugins.LoadCatalog`：扫描用户插件目录（缺目录 = 空；点前缀目录跳过；按 ID 稳定排序）。
pub fn load_catalog(user_dir: &Path) -> Catalog {
    let mut catalog = Catalog::default();
    let entries = match std::fs::read_dir(user_dir) {
        Ok(entries) => entries,
        Err(error) => {
            // Go: 只有**非 NotExist** 才记错误（目录缺失是正常场景）。
            if error.kind() != std::io::ErrorKind::NotFound {
                catalog.errors.push(format!("读取插件目录失败: {error}"));
            }
            return catalog;
        }
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) || name.starts_with('.') {
            continue; // 点前缀 = 导入临时目录（中断残留），不入目录
        }
        match read_manifest(&entry.path()) {
            Ok(manifest) => catalog.plugins.push(manifest),
            Err(error) => catalog.errors.push(format!("{name}: {error}")),
        }
    }
    // Go sort.SliceStable: Rust `sort_by` 同为稳定排序。
    catalog.plugins.sort_by(|a, b| a.id.cmp(&b.id));
    catalog
}

// --------------------------------------------------------------- 注入块（Go generators/plugins.go）

/// Go `generators.safePluginRelFile`：入口相对路径安全校验（拒绝绝对路径 / 盘符 / `.`、`..` 段）。
fn safe_plugin_rel_file(file: &str) -> bool {
    if file.is_empty() || file.starts_with('/') || file.starts_with('\\') || file.contains(':') {
        return false;
    }
    for segment in file.split(['/', '\\']) {
        if segment == ".." || segment == "." {
            return false;
        }
    }
    true
}

/// Go `generators.ahkStringLit`：渲染 AHK v2 双引号字符串字面量。
///
/// 替换顺序不可调换：先拍平换行（`\r\n` → `\n` → `\r` → 空格），再转义反引号，最后转义双引号。
fn ahk_string_lit(s: &str) -> String {
    let s = s
        .replace("\r\n", " ")
        .replace(['\n', '\r'], " ")
        .replace('`', "``")
        .replace('"', "`\"");
    format!("\"{s}\"")
}

/// Go `generators.ahkManifestLiteral`：把 manifest 渲染为 AHK `Map(...)` 原生字面量。
fn ahk_manifest_literal(manifest: &Manifest) -> String {
    let mut out = String::new();
    out.push_str("Map(\"id\", ");
    out.push_str(&ahk_string_lit(&manifest.id));
    out.push_str(", \"name\", ");
    out.push_str(&ahk_string_lit(&manifest.name));
    if !manifest.name_en.is_empty() {
        out.push_str(", \"nameEn\", ");
        out.push_str(&ahk_string_lit(&manifest.name_en));
    }
    if !manifest.version.is_empty() {
        out.push_str(", \"version\", ");
        out.push_str(&ahk_string_lit(&manifest.version));
    }
    out.push_str(&format!(", \"specVersion\", {}", manifest.spec_version));
    if !manifest.description.is_empty() {
        out.push_str(", \"description\", ");
        out.push_str(&ahk_string_lit(&manifest.description));
    }
    if !manifest.author.is_empty() {
        out.push_str(", \"author\", ");
        out.push_str(&ahk_string_lit(&manifest.author));
    }
    out.push_str(", \"entry\", Map(\"kind\", ");
    out.push_str(&ahk_string_lit(&manifest.entry.kind));
    out.push_str(", \"file\", ");
    out.push_str(&ahk_string_lit(&manifest.entry.file));
    out.push_str(", \"func\", ");
    out.push_str(&ahk_string_lit(&manifest.entry.func));
    out.push(')');
    if !manifest.permissions.is_empty() {
        let perms: Vec<String> = manifest
            .permissions
            .iter()
            .map(|p| ahk_string_lit(p))
            .collect();
        out.push_str(", \"permissions\", [");
        out.push_str(&perms.join(", "));
        out.push(']');
    }
    out.push(')');
    out
}

/// Go `generators.renderPluginBlocks`：产出 `(includes, bootstrap)` 两块。
///
/// 单插件失败只产注释行，不影响其他插件（错误隔离）；入口文件在生成期做存在性与路径
/// 安全校验（AHK 的 `#Include` 指向缺失文件会让整个脚本加载失败）。
pub fn render_plugin_blocks(plugins_dir: &Path, disabled: &HashSet<String>) -> (String, String) {
    let mut includes = String::new();
    let mut bootstrap = String::new();
    let catalog = load_catalog(plugins_dir);
    for manifest in &catalog.plugins {
        if disabled.contains(&manifest.id) {
            // 启停持久化（config.options.plugins.disabled）：停用插件不注入不注册。
            bootstrap.push_str(&format!(
                "\n; [插件] {} 已在配置中停用, 跳过加载",
                manifest.id
            ));
            continue;
        }
        if manifest.entry.kind != "script" || manifest.entry.file.is_empty() {
            bootstrap.push_str(&format!(
                "\n; [插件警告] {}: 仅支持 script 入口 (entry.file), 已跳过",
                manifest.id
            ));
            continue;
        }
        if !safe_plugin_rel_file(&manifest.entry.file) {
            bootstrap.push_str(&format!(
                "\n; [插件警告] {}: entry.file 非法 {:?}, 已跳过",
                manifest.id, manifest.entry.file
            ));
            continue;
        }
        // Go: filepath.Join(dir, m.ID, filepath.FromSlash(m.Entry.File))
        let abs = plugins_dir.join(&manifest.id).join(
            manifest
                .entry
                .file
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        if !abs.exists() {
            bootstrap.push_str(&format!(
                "\n; [插件警告] {}: 入口文件缺失 ({}), 已跳过",
                manifest.id, manifest.entry.file
            ));
            continue;
        }
        // 生成脚本位于 bin/，用户插件在 ../data/plugins/<id>/（目录名 = manifest.id）。
        includes.push_str(&format!(
            "\n#Include ../data/plugins/{}/{}",
            manifest.id, manifest.entry.file
        ));
        bootstrap.push_str(&format!(
            "\nPluginManager.Register({})",
            ahk_manifest_literal(manifest)
        ));
        bootstrap.push_str(&format!(
            "\nPluginManager.LoadEntry({})",
            ahk_string_lit(&manifest.id)
        ));
    }
    for error in &catalog.errors {
        bootstrap.push_str(&format!("\n; [插件错误] {error}"));
    }
    (includes, bootstrap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn examples_dir() -> PathBuf {
        PathBuf::from("../plugins/examples")
    }

    /// 目录扫描：读到 everything_search，且字段/权限/设置解析正确。
    #[test]
    fn loads_example_plugin() {
        let catalog = load_catalog(&examples_dir());
        assert!(
            catalog.errors.is_empty(),
            "示例插件应无错误: {:?}",
            catalog.errors
        );
        assert_eq!(catalog.plugins.len(), 1);
        let plugin = &catalog.plugins[0];
        assert_eq!(plugin.id, "everything_search");
        assert_eq!(plugin.entry.kind, "script");
        assert_eq!(plugin.entry.file, "main.ahk");
        assert_eq!(plugin.entry.func, "EverythingSearchMain");
        assert!(plugin.has_permission("settings"));
        assert_eq!(plugin.settings.len(), 4);
    }

    /// 缺目录 ⇒ 空目录、无错误（Go: NotExist 不记为错误）。
    #[test]
    fn missing_dir_is_empty_and_not_an_error() {
        let catalog = load_catalog(std::path::Path::new("../data/no-such-plugins-dir"));
        assert!(catalog.plugins.is_empty());
        assert!(catalog.errors.is_empty());
    }

    /// 注入块：示例插件产出 1 行 Include + Register/LoadEntry。
    #[test]
    fn renders_include_and_bootstrap_for_example() {
        let (includes, bootstrap) = render_plugin_blocks(&examples_dir(), &HashSet::new());
        assert_eq!(
            includes,
            "\n#Include ../data/plugins/everything_search/main.ahk"
        );
        assert!(bootstrap.contains(
            "\nPluginManager.Register(Map(\"id\", \"everything_search\", \"name\", \"Everything 搜索\""
        ), "{bootstrap}");
        assert!(
            bootstrap.ends_with("\nPluginManager.LoadEntry(\"everything_search\")"),
            "{bootstrap}"
        );
    }

    /// 空目录 / 不存在的目录 ⇒ 两块皆空串（模板行尾拼接下产物字节不变）。
    #[test]
    fn empty_when_no_plugins() {
        let (includes, bootstrap) =
            render_plugin_blocks(std::path::Path::new("../data/nope"), &HashSet::new());
        assert!(includes.is_empty());
        assert!(bootstrap.is_empty());
    }

    /// 停用插件只产注释行、不注入。
    #[test]
    fn disabled_plugin_is_skipped_with_comment() {
        let disabled: HashSet<String> = ["everything_search".to_string()].into_iter().collect();
        let (includes, bootstrap) = render_plugin_blocks(&examples_dir(), &disabled);
        assert!(includes.is_empty());
        assert_eq!(
            bootstrap,
            "\n; [插件] everything_search 已在配置中停用, 跳过加载"
        );
    }

    /// BOM 剥离 + 目录名不一致的拒绝口径。
    #[test]
    fn parse_manifest_strips_bom_and_checks_id() {
        let raw = b"\xEF\xBB\xBF{\"id\":\"x\",\"name\":\"X\",\"specVersion\":1,\"entry\":{\"kind\":\"script\",\"file\":\"a.ahk\",\"func\":\"F\"}}";
        let manifest = parse_manifest(raw).expect("应解析成功");
        assert_eq!(manifest.id, "x");
    }
}
