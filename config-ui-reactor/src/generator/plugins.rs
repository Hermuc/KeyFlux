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
///
/// `bool`（2026-10-01 新增）：**开关类字段插件化的硬前置**。原
/// `options.quickSwitch` 是 4 个 bool + 4 个 int + 1 个字符串数组，而声明式设置此前
/// 只有 `char/text/number/file` —— 表达不了"开关"。值域 = `"true"` / `"false"`
/// （字符串承载，与 `ConfigProvider` 的扁平字符串存储同构 ⇒ **AHK 侧零改动**）。
const SETTING_TYPES: [&str; 5] = ["char", "text", "number", "file", "bool"];

/// Go `plugins.Entry`：插件入口声明（当前仅 `script` 形态）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub kind: String,
    pub file: String,
    pub func: String,
    /// 晚初始化函数名（可选；P7a）。生成端渲染进 `PLUGIN_LATE_INIT` 扩展点为
    /// **无参**调用（配置由插件运行时自取，P5 定式）；空 = 无晚初始化，空块零字节。
    pub late: String,
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
    /// 仅 type=text：多行编辑器 + 换行分隔值（2026-10-02 P5，首个消费方
    /// = quick_switch.excludedPrefixes；提案 §5 D2）。
    #[serde(rename = "multiline")]
    pub multiline: bool,
}

/// Go `plugins.ProvidedActionKindPlugin`：动作 kind 词表（当前仅 "plugin"）。
pub const PROVIDED_ACTION_KIND_PLUGIN: &str = "plugin";

/// Go `plugins.MaxActionsPerPlugin`：单插件动作声明上限（防 manifest 失控）。
pub const MAX_ACTIONS_PER_PLUGIN: usize = 32;

/// Go `plugins.ProvidedAction`：插件对外提供的一等动作
/// （`manifest.provides.actions[]`；全局动作 ID = `<pluginId>.<actionId>`）。
/// P7a 仅声明 + 校验（零消费 ⇒ 产物零漂移）；消费切换 = P7b。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ProvidedAction {
    pub id: String,
    pub label: String,
    #[serde(rename = "labelEn")]
    pub label_en: String,
    pub kind: String,
}

/// Go `plugins.Provides`：manifest 的能力提供块。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Provides {
    pub actions: Vec<ProvidedAction>,
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
    /// 能力提供块（可选；缺省 = 无 = wire 不出场，存量插件零漂移）。
    pub provides: Option<Provides>,
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

/// Go `lateInitPattern` `^[A-Za-z_][A-Za-z0-9_]{0,63}$`（手写匹配，同上）。
fn is_valid_late_init(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !(bytes[0] == b'_' || bytes[0].is_ascii_alphabetic()) {
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
    if setting.kind == "bool" {
        return 5; // "false" —— 最长的合法取值（不接受 yes/1/True 等变体）
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
        // 空串已在上方提前放行（= 未设置，回落 manifest 默认值）。
        // 大小写敏感：引擎侧只做 `= "true"` 比较，放宽会让两端判定分叉。
        "bool" if value != "true" && value != "false" => {
            return Err(format!("{value:?} 不是布尔值 (仅接受 true/false)"));
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
                "插件「{}」设置项 {:?} 的 type {:?} 不合法 (仅支持 char/text/number/file/bool)",
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
        if setting.kind != "text" && setting.multiline {
            return Err(format!(
                "插件「{}」设置项 {:?} 不是 text 类型, 不应带 multiline",
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

/// Go `plugins.ValidateManifest`（导入 API 共用：结构校验 + 内置 ID 保留集）。
/// 🔴 目录加载路径不走本函数（走 [`validate_manifest_body`]）：随包内置插件
/// quick_switch 自 2026-10-01 P2 插件化起以标准插件形态分发
/// （`data/plugins/quick_switch/`），必须经目录扫描正常加载；本函数仅供导入 API
/// 调用 —— 第三方包不得冒用内置 ID。
pub fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if BUILTIN_PLUGIN_IDS.contains(&manifest.id.as_str()) {
        return Err(format!("插件 ID {:?} 与内置插件冲突", manifest.id));
    }
    validate_manifest_body(manifest)
}

/// Go `plugins.validateManifestBody`（目录加载路径；不含内置 ID 检查）。
pub(crate) fn validate_manifest_body(manifest: &Manifest) -> Result<(), String> {
    if !is_valid_id(&manifest.id) {
        return Err(format!(
            "插件 ID {:?} 不合法 (须匹配 ^[a-z][a-z0-9_]{{0,31}}$)",
            manifest.id
        ));
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
            if !manifest.entry.late.is_empty() && !is_valid_late_init(&manifest.entry.late) {
                return Err(format!(
                    "插件「{}」的 entry.late {:?} 不合法 (须匹配 ^[A-Za-z_][A-Za-z0-9_]{{0,63}}$)",
                    manifest.id, manifest.entry.late
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
    validate_settings(manifest)?;
    validate_provides(manifest)
}

/// Go `plugins.validateProvides`（P7a）：加载期拦坏声明，坏包在目录加载时即隔离。
fn validate_provides(manifest: &Manifest) -> Result<(), String> {
    let Some(provides) = &manifest.provides else {
        return Ok(());
    };
    let actions = &provides.actions;
    if actions.is_empty() {
        return Err(format!(
            "插件「{}」声明了 provides 但没有任何 action",
            manifest.id
        ));
    }
    if actions.len() > MAX_ACTIONS_PER_PLUGIN {
        return Err(format!(
            "插件「{}」声明的动作过多 ({} > {})",
            manifest.id,
            actions.len(),
            MAX_ACTIONS_PER_PLUGIN
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(actions.len());
    for (index, action) in actions.iter().enumerate() {
        if !is_valid_setting_key(&action.id) {
            return Err(format!(
                "插件「{}」第 {} 个动作的 id {:?} 不合法 (须匹配 ^[A-Za-z][A-Za-z0-9_]{{0,31}}$)",
                manifest.id,
                index + 1,
                action.id
            ));
        }
        if !seen.insert(action.id.as_str()) {
            return Err(format!(
                "插件「{}」动作 id {:?} 重复",
                manifest.id, action.id
            ));
        }
        if action.label.trim().is_empty() {
            return Err(format!(
                "插件「{}」动作 {:?} 缺少 label",
                manifest.id, action.id
            ));
        }
        if action.kind != PROVIDED_ACTION_KIND_PLUGIN {
            return Err(format!(
                "插件「{}」动作 {:?} 的 kind {:?} 不合法 (当前仅支持 {:?})",
                manifest.id, action.id, action.kind, PROVIDED_ACTION_KIND_PLUGIN
            ));
        }
    }
    Ok(())
}

// --------------------------------------------------------------- 加载（Go plugins.go）

/// Go `parseManifest`：剥 BOM → 解析 → `validateManifestBody`。
/// 🔴 目录加载路径走**宽松**校验（不含内置 ID 检查）：随包内置插件 quick_switch
/// 须经目录扫描正常加载；导入 API 另行调 [`validate_manifest`] 补上保留集检查。
fn parse_manifest(raw: &[u8]) -> Result<Manifest, String> {
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    let manifest: Manifest =
        serde_json::from_slice(raw).map_err(|error| format!("plugin.json 解析失败: {error}"))?;
    validate_manifest_body(&manifest)?;
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

/// Go `generators.PluginLateInit`：模板函数 `{{ PLUGIN_LATE_INIT }}`（插件晚初始化
/// 扩展点，位于 `InitKeymap()` 与 `OnExit` 之间）。产出「必须晚于 InitKeymap」的插件
/// 初始化行，行尾拼接约定（非空时自带前导 `\n`），空块 = 零字节。
///
/// 当前唯一消费方 = quick_switch 的 `InitQuickSwitch()` 无参调用行（2026-10-02 P5
/// 起：代码随插件搬入 `data/plugins/quick_switch/`，配置经 ConfigProvider 运行时
/// 自取 `plugin-settings.json`，生成器与 `options.quickSwitch` 已解耦；提案
/// `docs/contracts-proposals/quickswitch-pluginization.md`）。存在性/禁用判定与
/// [`render_plugin_blocks`] 完全同构（同一 catalog + 同一 disabled/removed 集 + 同一入口
/// 存在性校验），保证「插件不可用 ⇒ 初始化行不产出」（可删除性保证：AHK v2 直调
/// 未定义函数是加载期致命错误）。特判泛化（manifest 声明晚初始化函数）属 P7。
pub fn render_late_init(
    plugins_dir: &Path,
    disabled: &HashSet<String>,
    removed: &HashSet<String>,
) -> String {
    let catalog = load_catalog(plugins_dir);
    for manifest in &catalog.plugins {
        if removed.contains(&manifest.id) || disabled.contains(&manifest.id) {
            continue;
        }
        if manifest.entry.kind != "script" || manifest.entry.file.is_empty() {
            continue;
        }
        if !safe_plugin_rel_file(&manifest.entry.file) {
            continue;
        }
        let abs = plugins_dir.join(&manifest.id).join(
            manifest
                .entry
                .file
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        if !abs.exists() {
            continue;
        }
        if manifest.id == "quick_switch" {
            // P5 (2026-10-02): 无参调用 —— 配置已迁 plugin-settings.json, 插件经
            // ConfigProvider 自取; 生成器不再读 options.quickSwitch。
            // （P7 将以 manifest 声明泛化此特判。）
            return "\nInitQuickSwitch()".to_string();
        }
    }
    String::new()
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
/// `removed` = 墓碑集（`config.options.plugins.removed`，2026-10-02 P4）：用户主动
/// 删除的随包内置插件 —— 目录被同步带回时不复活（文案与 Go 同构，parity 产物一致）。
pub fn render_plugin_blocks(
    plugins_dir: &Path,
    disabled: &HashSet<String>,
    removed: &HashSet<String>,
) -> (String, String) {
    let mut includes = String::new();
    let mut bootstrap = String::new();
    let catalog = load_catalog(plugins_dir);
    for manifest in &catalog.plugins {
        if removed.contains(&manifest.id) {
            bootstrap.push_str(&format!(
                "\n; [插件] {} 已被用户移除 (墓碑), 跳过加载",
                manifest.id
            ));
            continue;
        }
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

    /// 目录扫描：读到 everything_search + quick_switch（2026-10-01 P2 起
    /// quick_switch 以标准插件形态随包分发），且字段/权限/设置解析正确。
    #[test]
    fn loads_example_plugin() {
        let catalog = load_catalog(&examples_dir());
        assert!(
            catalog.errors.is_empty(),
            "示例插件应无错误: {:?}",
            catalog.errors
        );
        assert_eq!(catalog.plugins.len(), 2);
        // ID 字典序: everything_search 在前
        let plugin = &catalog.plugins[0];
        assert_eq!(plugin.id, "everything_search");
        assert_eq!(plugin.entry.kind, "script");
        assert_eq!(plugin.entry.file, "main.ahk");
        assert_eq!(plugin.entry.func, "EverythingSearchMain");
        assert!(plugin.has_permission("settings"));
        assert_eq!(plugin.settings.len(), 4);
        // 内置 ID 经目录加载放行（导入路径仍拒绝冒名，见 validate_manifest 拆分）
        let builtin = &catalog.plugins[1];
        assert_eq!(builtin.id, "quick_switch");
        assert_eq!(builtin.entry.func, "QuickSwitchMain");
        assert!(builtin.has_permission("window"));
        // P5 (2026-10-02)：9 项声明式设置 + settings 能力位。
        assert!(builtin.has_permission("settings"));
        assert_eq!(builtin.settings.len(), 9);
        let excluded = builtin
            .settings
            .iter()
            .find(|setting| setting.key == "excludedPrefixes")
            .expect("excludedPrefixes 声明存在");
        assert_eq!(excluded.kind, "text");
        assert!(excluded.multiline, "排除前缀 = 多行文本 (换行分隔)");
        // P7a (2026-10-02)：示例插件尚未声明 provides / late（零漂移前提）。
        assert!(builtin.provides.is_none());
        assert!(builtin.entry.late.is_empty());
    }

    /// P7a 校验矩阵：provides.actions[] + entry.late（与 Go TestValidateProvidesAndLateInit 同源）。
    #[test]
    fn validates_provides_and_late_init() {
        let base = || Manifest {
            id: "hello_world".to_string(),
            name: "Hello".to_string(),
            spec_version: SPEC_VERSION,
            entry: Entry {
                kind: "script".to_string(),
                file: "main.ahk".to_string(),
                func: "PluginMain".to_string(),
                late: String::new(),
            },
            ..Default::default()
        };

        // entry.late：合法放行；非法标识符拒绝。
        let mut m = base();
        m.entry.late = "InitHelloWorld".to_string();
        assert!(validate_manifest_body(&m).is_ok());
        for bad in ["1Init", "Init X", "Init-X", "A".repeat(65).as_str()] {
            m.entry.late = bad.to_string();
            let error = validate_manifest_body(&m).unwrap_err();
            assert!(error.contains("entry.late"), "late {bad:?}: {error}");
        }

        // provides：合法声明放行。
        let mut m = base();
        m.provides = Some(Provides {
            actions: vec![
                ProvidedAction {
                    id: "goto".to_string(),
                    label: "跳转".to_string(),
                    label_en: "Go".to_string(),
                    kind: "plugin".to_string(),
                },
                ProvidedAction {
                    id: "back".to_string(),
                    label: "返回".to_string(),
                    label_en: String::new(),
                    kind: "plugin".to_string(),
                },
            ],
        });
        assert!(validate_manifest_body(&m).is_ok());

        let mut m2 = base();
        m2.provides = Some(Provides { actions: vec![] });
        let error = validate_manifest_body(&m2).unwrap_err();
        assert!(error.contains("没有任何 action"), "{error}");

        let two_actions = || Provides {
            actions: vec![
                ProvidedAction {
                    id: "goto".to_string(),
                    label: "跳转".to_string(),
                    label_en: String::new(),
                    kind: "plugin".to_string(),
                },
                ProvidedAction {
                    id: "back".to_string(),
                    label: "返回".to_string(),
                    label_en: String::new(),
                    kind: "plugin".to_string(),
                },
            ],
        };
        let expect_reject = |m: &Manifest, want: &str| {
            let error = validate_manifest_body(m).unwrap_err();
            assert!(error.contains(want), "期望含 {want:?}, 实际: {error}");
        };

        // id 词表
        let mut m = base();
        m.provides = Some(two_actions());
        m.provides.as_mut().unwrap().actions[0].id = "Bad-Id".to_string();
        expect_reject(&m, "不合法");

        // 插件内重复
        let mut m = base();
        m.provides = Some(two_actions());
        let id = m.provides.as_ref().unwrap().actions[0].id.clone();
        m.provides.as_mut().unwrap().actions[1].id = id;
        expect_reject(&m, "重复");

        // 缺 label
        let mut m = base();
        m.provides = Some(two_actions());
        m.provides.as_mut().unwrap().actions[0].label = " ".to_string();
        expect_reject(&m, "缺少 label");

        // kind 词表
        let mut m = base();
        m.provides = Some(two_actions());
        m.provides.as_mut().unwrap().actions[0].kind = "builtin".to_string();
        expect_reject(&m, "kind");
    }

    /// 缺目录 ⇒ 空目录、无错误（Go: NotExist 不记为错误）。
    #[test]
    fn missing_dir_is_empty_and_not_an_error() {
        let catalog = load_catalog(std::path::Path::new("../data/no-such-plugins-dir"));
        assert!(catalog.plugins.is_empty());
        assert!(catalog.errors.is_empty());
    }

    /// 注入块：示例插件各产出 1 行 Include + Register/LoadEntry。
    #[test]
    fn renders_include_and_bootstrap_for_example() {
        let (includes, bootstrap) =
            render_plugin_blocks(&examples_dir(), &HashSet::new(), &HashSet::new());
        assert_eq!(
            includes,
            "\n#Include ../data/plugins/everything_search/main.ahk\n\
             #Include ../data/plugins/quick_switch/main.ahk"
        );
        assert!(bootstrap.contains(
            "\nPluginManager.Register(Map(\"id\", \"everything_search\", \"name\", \"Everything 搜索\""
        ), "{bootstrap}");
        assert!(
            bootstrap.contains("\nPluginManager.LoadEntry(\"everything_search\")"),
            "{bootstrap}"
        );
        assert!(
            bootstrap.ends_with("\nPluginManager.LoadEntry(\"quick_switch\")"),
            "{bootstrap}"
        );
    }

    /// 空目录 / 不存在的目录 ⇒ 两块皆空串（模板行尾拼接下产物字节不变）。
    #[test]
    fn empty_when_no_plugins() {
        let (includes, bootstrap) = render_plugin_blocks(
            std::path::Path::new("../data/nope"),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert!(includes.is_empty());
        assert!(bootstrap.is_empty());
    }

    /// 停用插件只产注释行、不注入（另一插件照常）。
    #[test]
    fn disabled_plugin_is_skipped_with_comment() {
        let disabled: HashSet<String> = ["everything_search".to_string()].into_iter().collect();
        let (includes, bootstrap) =
            render_plugin_blocks(&examples_dir(), &disabled, &HashSet::new());
        assert_eq!(includes, "\n#Include ../data/plugins/quick_switch/main.ahk");
        assert!(bootstrap.contains("\n; [插件] everything_search 已在配置中停用, 跳过加载"));
        assert!(
            !bootstrap.contains("\nPluginManager.Register(Map(\"id\", \"everything_search\""),
            "{bootstrap}"
        );
        assert!(
            bootstrap.contains("\nPluginManager.Register(Map(\"id\", \"quick_switch\""),
            "{bootstrap}"
        );
    }

    /// 墓碑（2026-10-02 P4）：removed 中的随包内置插件不注入、只产注释行 ——
    /// 目录被 sync-plugins 带回时不复活（文案与 Go 同构）。
    #[test]
    fn removed_plugin_tombstone_skips_injection() {
        let removed: HashSet<String> = ["quick_switch".to_string()].into_iter().collect();
        let (includes, bootstrap) =
            render_plugin_blocks(&examples_dir(), &HashSet::new(), &removed);
        assert_eq!(
            includes,
            "\n#Include ../data/plugins/everything_search/main.ahk"
        );
        assert!(bootstrap.contains("\n; [插件] quick_switch 已被用户移除 (墓碑), 跳过加载"));
        assert!(
            !bootstrap.contains("\nPluginManager.Register(Map(\"id\", \"quick_switch\""),
            "{bootstrap}"
        );
        assert!(
            bootstrap.contains("\nPluginManager.LoadEntry(\"everything_search\")"),
            "{bootstrap}"
        );
    }

    /// BOM 剥离 + 目录名不一致的拒绝口径。
    #[test]
    fn parse_manifest_strips_bom_and_checks_id() {
        let raw = b"\xEF\xBB\xBF{\"id\":\"x\",\"name\":\"X\",\"specVersion\":1,\"entry\":{\"kind\":\"script\",\"file\":\"a.ahk\",\"func\":\"F\"}}";
        let manifest = parse_manifest(raw).expect("应解析成功");
        assert_eq!(manifest.id, "x");
    }

    /// `bool` 类型（2026-10-01 协议扩展）：值域严格 = `true`/`false`；空串合法（= 未设置，
    /// 回落 manifest 默认值）；`yes`/`1`/`True` 一律拒绝（大小写敏感 —— 与 `ConfigProvider`
    /// 的字符串存储口径一致，引擎侧只做 `= "true"` 比较，放宽大小写会让两边判定分叉）。
    #[test]
    fn bool_setting_value_domain_is_strict() {
        let manifest: Manifest = serde_json::from_str(
            r#"{"id":"x","name":"X","specVersion":1,
                "entry":{"kind":"script","file":"a.ahk","func":"F"},
                "permissions":["settings"],
                "settings":[{"key":"autoShow","type":"bool","label":"自动弹出","default":"true"}]}"#,
        )
        .expect("应能反序列化");
        assert!(validate_manifest(&manifest).is_ok(), "{manifest:?}");

        let setting = &manifest.settings[0];
        assert_eq!(value_limit(setting), 5, "上限 = \"false\" 的长度");
        assert!(validate_setting_value(setting, "").is_ok(), "空串 = 未设置");
        assert!(validate_setting_value(setting, "true").is_ok());
        assert!(validate_setting_value(setting, "false").is_ok());
        for bad in ["yes", "1", "True", "FALSE", "on"] {
            assert!(
                validate_setting_value(setting, bad).is_err(),
                "{bad:?} 应被拒绝"
            );
        }
    }

    /// `bool` 不得携带 `number`/`file` 专属字段（`min`/`max`/`filter`）——
    /// 否则 manifest 会写出永远不会生效的声明。
    #[test]
    fn bool_setting_rejects_number_and_file_only_fields() {
        let with_extra = |extra: &str| {
            format!(
                r#"{{"id":"x","name":"X","specVersion":1,
                    "entry":{{"kind":"script","file":"a.ahk","func":"F"}},
                    "permissions":["settings"],
                    "settings":[{{"key":"k","type":"bool","label":"L"{extra}}}]}}"#
            )
        };
        for (extra, what) in [(r#","min":0"#, "min"), (r#","filter":"*.exe""#, "filter")] {
            let manifest: Manifest = serde_json::from_str(&with_extra(extra)).unwrap();
            assert!(
                validate_manifest(&manifest).is_err(),
                "bool 不应带 {what}: {manifest:?}"
            );
        }
    }

    /// `bool` 的**默认值**本身也要过校验 —— 坏包必须在加载期隔离，而不是等用户点开设置框。
    #[test]
    fn bool_setting_with_bad_default_is_rejected() {
        let manifest: Manifest = serde_json::from_str(
            r#"{"id":"x","name":"X","specVersion":1,
                "entry":{"kind":"script","file":"a.ahk","func":"F"},
                "permissions":["settings"],
                "settings":[{"key":"k","type":"bool","label":"L","default":"yes"}]}"#,
        )
        .unwrap();
        let error = validate_manifest(&manifest).expect_err("应拒绝");
        assert!(error.contains("默认值不合法"), "{error}");
    }

    /// 词表扩容后，`bool` 必须被 `SETTING_TYPES` 接纳（两端同步的机械闸门：
    /// 这条挂了说明 Rust 加了 `bool` 而词表没加，或反之）。
    #[test]
    fn bool_is_in_setting_types_vocabulary() {
        assert!(SETTING_TYPES.contains(&"bool"));
        assert_eq!(SETTING_TYPES.len(), 5);
    }
}
