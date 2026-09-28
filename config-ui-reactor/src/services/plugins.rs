//! 插件页的**纯逻辑**（零 UI 依赖）。
//!
//! 逐项复刻 `config-ui-avalonia/ViewModels/PluginsPageViewModel.cs` 的可判定部分：
//! 统一卡片列表（首卡恒为内置 QuickSwitch）+ 启停开关分流 + 状态/显示派生 + 空态判定。
//!
//! 开关语义（与旧版一致）：
//! * **内置卡**（`quick_switch`）直通 `options.quickSwitch.collectEnabled`；
//! * **用户卡**直通注册表 `options.plugins.disabled`（在表内 = 已停用）。
//!
//! 两者都走「保存链路」（`PUT /config` 会重生成脚本并重启引擎）。

use crate::models::{
    Config, PluginListResponse, PluginManifest, PluginSetting, PluginSettingsResponse,
    QuickSwitchOption,
};
use crate::services::i18n;

/// 内置插件 ID（与 Go `internal/plugins.BuiltinPluginIDs` 对应）。
pub const BUILTIN_PLUGIN_IDS: [&str; 1] = ["quick_switch"];

/// 内置 QuickSwitch 的固定 ID。
pub const QUICK_SWITCH_ID: &str = "quick_switch";

/// 统一插件卡（内置 + 用户同一模型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCard {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    pub version: Option<String>,
    pub description: String,
    pub author: String,
    /// 内置插件（首卡 QuickSwitch；不可删除，开关直通 `collectEnabled`）。
    pub is_builtin: bool,
    /// 是否可删除（仅用户插件）。
    pub can_delete: bool,
    /// 是否可打开配置对话框（内置卡恒可；用户卡需声明了 settings）。
    pub can_configure: bool,
    pub enabled: bool,
}

impl PluginCard {
    /// 显示名（复刻 `DisplayName`：英文界面优先 `nameEn`）。
    pub fn display_name(&self, english: bool) -> String {
        if english
            && let Some(name_en) = &self.name_en
            && !name_en.is_empty()
        {
            return name_en.clone();
        }
        self.name.clone()
    }

    /// 版本徽标（复刻 `VersionText`：无版本为空串，否则 `v{version}`）。
    pub fn version_text(&self) -> String {
        match &self.version {
            Some(version) if !version.is_empty() => format!("v{version}"),
            _ => String::new(),
        }
    }

    /// 开关下方的状态文字（i18n 2423 已启用 / 2424 已停用）。
    pub fn status_text(&self) -> String {
        i18n::t(if self.enabled { "2423" } else { "2424" })
    }
}

/// 判断是否内置插件。
pub fn is_builtin(id: &str) -> bool {
    BUILTIN_PLUGIN_IDS.contains(&id)
}

/// 合成内置 QuickSwitch 卡（复刻 `CreateQuickSwitchCard`）。
pub fn quick_switch_card(config: &Config) -> PluginCard {
    PluginCard {
        id: QUICK_SWITCH_ID.to_string(),
        name: i18n::t("2408"),
        name_en: Some("Quick Switch".to_string()),
        version: None,
        description: i18n::t("2422"),
        author: String::new(),
        is_builtin: true,
        can_delete: false,
        can_configure: true,
        enabled: config.options.quick_switch.collect_enabled,
    }
}

/// 由目录响应 + 当前配置构建统一卡片列表（首卡恒为内置 QuickSwitch）。
pub fn build_cards(config: &Config, catalog: &PluginListResponse) -> Vec<PluginCard> {
    let mut cards = vec![quick_switch_card(config)];

    cards.extend(
        catalog
            .plugins
            .iter()
            .map(|manifest| card_from(config, manifest)),
    );
    cards
}

/// 单个用户插件卡（复刻 `PluginCardVm` 的派生字段）。
pub fn card_from(config: &Config, manifest: &PluginManifest) -> PluginCard {
    let builtin = is_builtin(&manifest.id);
    PluginCard {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        name_en: manifest.name_en.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone().unwrap_or_default(),
        author: manifest.author.clone().unwrap_or_default(),
        is_builtin: builtin,
        can_delete: !builtin,
        can_configure: builtin
            || manifest
                .settings
                .as_ref()
                .map(|settings| !settings.is_empty())
                .unwrap_or(false),
        // 缺省启用：仅当登记在 `disabled` 中才算停用
        enabled: !config
            .options
            .plugins
            .disabled
            .iter()
            .any(|id| id == &manifest.id),
    }
}

/// 写回开关（复刻 `OnCardEnabledChanged` 的配置写入部分，返回**是否真的发生变更**）。
///
/// 调用方在返回 `true` 时应走保存链路（`force: true`）。
pub fn apply_enabled(config: &mut Config, card_id: &str, is_builtin: bool, enabled: bool) -> bool {
    if is_builtin {
        if config.options.quick_switch.collect_enabled == enabled {
            return false;
        }
        config.options.quick_switch.collect_enabled = enabled;
        return true;
    }

    let registry = &mut config.options.plugins.disabled;
    let registered = registry.iter().any(|id| id == card_id);
    if enabled {
        // 启用 = 从停用表移除
        if !registered {
            return false;
        }
        registry.retain(|id| id != card_id);
        return true;
    }
    if registered {
        return false;
    }
    registry.push(card_id.to_string());
    true
}

/// 删除插件后清理注册表孤儿项（复刻 `DeletePlugin` 的收尾），返回是否发生变更。
pub fn remove_from_registry(config: &mut Config, card_id: &str) -> bool {
    let registry = &mut config.options.plugins.disabled;
    let before = registry.len();
    registry.retain(|id| id != card_id);
    before != registry.len()
}

/// 页面空态判定（复刻 `ShowEmptyState`：加载完成、无告警、且无用户插件）。
pub fn show_empty_state(loading: bool, load_error: Option<&str>, cards: &[PluginCard]) -> bool {
    !loading && load_error.is_none() && !cards.iter().any(|card| !card.is_builtin)
}

/// 加载失败可见性（复刻 `ShowLoadError`）。
pub fn show_load_error(loading: bool, load_error: Option<&str>) -> bool {
    !loading && load_error.is_some()
}

/// 逐包告警合并为单串（复刻 `string.Join("\n", Errors)`）。
pub fn join_errors(errors: Option<&Vec<String>>) -> Option<String> {
    match errors {
        Some(list) if !list.is_empty() => Some(list.join("\n")),
        _ => None,
    }
}

// ---------------------------------------------------------------- QuickSwitch 配置对话框

/// `history.tsv` 相对于部署根的路径（复刻 `QuickSwitchDialogViewModel.ClearHistory`）。
pub const HISTORY_RELATIVE_PATH: &str = "data/quickswitch/history.tsv";

/// 从真源深拷贝出编辑草稿（复刻 `QuickSwitchDialogViewModel` 构造：副本编辑，取消不影响真源）。
pub fn draft_from(config: &Config) -> QuickSwitchOption {
    config.options.quick_switch.clone()
}

/// 把草稿写回真源（复刻 `SaveAsync`）：`maxHistory` 下限钳到 1，逐字段写回；返回是否发生变更。
///
/// 返回 `true` 时调用方应走保存链路（`force: true`）。
pub fn commit_draft(config: &mut Config, draft: &QuickSwitchOption) -> bool {
    let mut draft = draft.clone();
    draft.max_history = draft.max_history.max(1);

    if config.options.quick_switch == draft {
        return false;
    }
    config.options.quick_switch = draft;
    true
}

/// 历史条数下限（复刻 `Math.Max(1, MaxHistory)`）。
pub const MIN_HISTORY: i32 = 1;

/// 清空 QuickSwitch 历史：把 `<部署根>/data/quickswitch/history.tsv` 截断为空文件（文件保留）。
///
/// 引擎在每次对话框实例切换时经 `HistLoad` 重读该文件，故截断后历史立即呈空态。
/// `deployment_root` = 部署根（`backend_dir` 的父级）。
pub fn clear_history(deployment_root: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let path = deployment_root.join(HISTORY_RELATIVE_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&path, b"").map_err(|error| error.to_string())?;
    Ok(path)
}

// ---------------------------------------------------------------- 插件声明式设置

/// 单字符输入（`type: "char"`）。
pub const SETTING_CHAR: &str = "char";
/// 数字输入（`type: "number"`）。
pub const SETTING_NUMBER: &str = "number";
/// 文件选择（`type: "file"`）。
pub const SETTING_FILE: &str = "file";

/// 未声明 `maxLength` 时的回退（与后端 `Setting.ValueLimit` 口径一致）。
pub const DEFAULT_MAX_LENGTH: usize = 1024;

pub fn is_char(setting: &PluginSetting) -> bool {
    setting.setting_type == SETTING_CHAR
}

pub fn is_number(setting: &PluginSetting) -> bool {
    setting.setting_type == SETTING_NUMBER
}

pub fn is_file(setting: &PluginSetting) -> bool {
    setting.setting_type == SETTING_FILE
}

/// 输入框字符上限（`char` → 1；`maxLength > 0` → 声明值；否则 1024）。
pub fn max_length(setting: &PluginSetting) -> usize {
    if is_char(setting) {
        return 1;
    }
    if setting.max_length > 0 {
        return setting.max_length as usize;
    }
    DEFAULT_MAX_LENGTH
}

/// Pick 语义（复刻 `PluginSettingRowVm.Pick`）：英文界面优先英文；中文缺失回退英文；再回退空串。
fn pick(zh: &str, en: &Option<String>, english: bool) -> String {
    if english
        && let Some(en) = en
        && !en.is_empty()
    {
        return en.clone();
    }
    if !zh.is_empty() {
        return zh.to_string();
    }
    en.clone().unwrap_or_default()
}

/// 标签（数据驱动，非 i18n 键）。
pub fn setting_label(setting: &PluginSetting, english: bool) -> String {
    pick(&setting.label, &setting.label_en, english)
}

/// 提示。
pub fn setting_hint(setting: &PluginSetting, english: bool) -> String {
    pick(
        setting.hint.as_deref().unwrap_or(""),
        &setting.hint_en,
        english,
    )
}

/// 数字项的上下限说明（无边界为空；复刻 `RangeHint`）。
pub fn range_hint(setting: &PluginSetting) -> String {
    if !is_number(setting) {
        return String::new();
    }
    match (setting.min, setting.max) {
        (Some(min), Some(max)) => format!("{min} – {max}"),
        (Some(min), None) => format!(">= {min}"),
        (None, Some(max)) => format!("<= {max}"),
        (None, None) => String::new(),
    }
}

/// 本地即时校验（复刻 `PluginSettingRowVm.Validate`；后端仍是权威）。
///
/// 返回 `None` = 通过；`Some(reason)` = 可直接展示的原因。
pub fn validate_setting(setting: &PluginSetting, value: &str) -> Option<String> {
    if value.is_empty() {
        return None; // 空 = 清除覆盖（回落默认值），永远合法
    }
    let limit = max_length(setting);
    if value.chars().count() > limit {
        return Some(format!("≤ {limit}"));
    }
    if value.contains('\0') {
        return Some("NUL".to_string());
    }

    let characters: Vec<char> = value.chars().collect();
    if is_char(setting) {
        let first = characters[0];
        if first < '\u{20}' || first == '\u{7f}' {
            return Some("printable only".to_string());
        }
    }
    if is_number(setting) {
        let parsed: Result<i64, _> = value.parse();
        let Ok(number) = parsed else {
            return Some("integer".to_string());
        };
        if let Some(min) = setting.min
            && number < min
        {
            return Some(format!(">= {min}"));
        }
        if let Some(max) = setting.max
            && number > max
        {
            return Some(format!("<= {max}"));
        }
    }
    None
}

/// 由服务端响应 + 本地 manifest 构建设置行（声明序；初值 = 当前值 → 声明默认值 → 空串）。
///
/// 服务端返回空声明时退回本地 manifest（复刻 `LoadAsync` 的窗口期兜底）。
pub fn settings_rows(
    response: &PluginSettingsResponse,
    manifest: &PluginManifest,
) -> Vec<(PluginSetting, String)> {
    let schema: &[PluginSetting] = if response.settings.is_empty() {
        manifest.settings.as_deref().unwrap_or(&[])
    } else {
        &response.settings
    };

    schema
        .iter()
        .map(|setting| {
            let initial = response
                .values
                .get(&setting.key)
                .cloned()
                .or_else(|| setting.default.clone())
                .unwrap_or_default();
            (setting.clone(), initial)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PluginsOption;

    fn manifest(id: &str, name: &str, version: Option<&str>, settings: usize) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: name.to_string(),
            name_en: Some(format!("{name}-EN")),
            version: version.map(str::to_string),
            description: Some("描述".to_string()),
            author: Some("me".to_string()),
            settings: if settings == 0 {
                None
            } else {
                Some(vec![
                    PluginSetting {
                        key: "x".to_string(),
                        ..Default::default()
                    };
                    settings
                ])
            },
            ..Default::default()
        }
    }

    #[test]
    fn cards_start_with_builtin_quick_switch() {
        let mut config = Config::default();
        config.options.quick_switch.collect_enabled = true;
        let cards = build_cards(&config, &PluginListResponse::default());

        assert_eq!(cards.len(), 1, "无用户插件时仅内置卡");
        let card = &cards[0];
        assert_eq!(card.id, QUICK_SWITCH_ID);
        assert!(card.is_builtin);
        assert!(!card.can_delete, "内置卡不可删除");
        assert!(card.can_configure, "内置卡可配置");
        assert!(card.enabled, "直通 collectEnabled");
        assert_eq!(card.name_en.as_deref(), Some("Quick Switch"));
    }

    #[test]
    fn user_plugins_follow_and_default_enabled() {
        let mut config = Config::default();
        config.options.quick_switch.collect_enabled = false;
        config.options.plugins = PluginsOption {
            disabled: vec!["b".to_string()],
        };
        let catalog = PluginListResponse {
            plugins: vec![
                manifest("a", "A", Some("1.0"), 0),
                manifest("b", "B", None, 2),
            ],
            errors: None,
        };

        let cards = build_cards(&config, &catalog);
        assert_eq!(cards.len(), 3, "内置 + 2 用户");
        assert!(cards[1].enabled, "未登记 disabled ⇒ 默认启用");
        assert!(!cards[2].enabled, "登记在 disabled ⇒ 停用");
        assert!(cards[1].can_delete);
        assert!(!cards[1].can_configure, "无 settings 声明的用户卡不可配置");
        assert!(cards[2].can_configure, "有 settings 声明则可配置");
        assert_eq!(cards[1].version_text(), "v1.0");
        assert_eq!(cards[2].version_text(), "", "无版本 ⇒ 空徽标");
    }

    #[test]
    fn builtin_id_in_catalog_is_not_deletable() {
        let config = Config::default();
        let card = card_from(&config, &manifest(QUICK_SWITCH_ID, "QS", None, 0));
        assert!(card.is_builtin, "目录里出现内置 ID 也按内置判定");
        assert!(!card.can_delete);
        assert!(card.can_configure);
    }

    #[test]
    fn display_name_prefers_english_only_when_english() {
        let config = Config::default();
        let card = card_from(&config, &manifest("a", "中文名", None, 0));
        assert_eq!(card.display_name(false), "中文名");
        assert_eq!(card.display_name(true), "中文名-EN");

        let mut no_en = card.clone();
        no_en.name_en = None;
        assert_eq!(no_en.display_name(true), "中文名", "无 nameEn 时回退原名");
        no_en.name_en = Some(String::new());
        assert_eq!(no_en.display_name(true), "中文名", "空 nameEn 同样回退");
    }

    #[test]
    fn enabling_builtin_writes_collect_enabled_only_on_change() {
        let mut config = Config::default();
        config.options.quick_switch.collect_enabled = false;

        assert!(apply_enabled(&mut config, QUICK_SWITCH_ID, true, true));
        assert!(config.options.quick_switch.collect_enabled);
        assert!(
            !apply_enabled(&mut config, QUICK_SWITCH_ID, true, true),
            "无变化 ⇒ 不必保存"
        );
    }

    #[test]
    fn enabling_user_plugin_toggles_registry() {
        let mut config = Config::default();

        assert!(apply_enabled(&mut config, "a", false, false), "停用 = 入表");
        assert_eq!(config.options.plugins.disabled, vec!["a".to_string()]);
        assert!(
            !apply_enabled(&mut config, "a", false, false),
            "重复停用无变化"
        );

        assert!(apply_enabled(&mut config, "a", false, true), "启用 = 出表");
        assert!(config.options.plugins.disabled.is_empty());
        assert!(
            !apply_enabled(&mut config, "a", false, true),
            "重复启用无变化"
        );
    }

    #[test]
    fn removing_from_registry_reports_change() {
        let mut config = Config::default();
        config.options.plugins.disabled = vec!["a".to_string(), "b".to_string()];
        assert!(remove_from_registry(&mut config, "a"));
        assert_eq!(config.options.plugins.disabled, vec!["b".to_string()]);
        assert!(!remove_from_registry(&mut config, "zzz"), "不存在 ⇒ 无变化");
    }

    #[test]
    fn empty_and_error_states_follow_legacy() {
        let builtin_only = vec![quick_switch_card(&Config::default())];
        assert!(show_empty_state(false, None, &builtin_only));
        assert!(
            !show_empty_state(true, None, &builtin_only),
            "加载中不显示空态"
        );
        assert!(
            !show_empty_state(false, Some("warn"), &builtin_only),
            "有告警不显示空态"
        );

        let with_user = {
            let config = Config::default();
            vec![
                quick_switch_card(&config),
                card_from(&config, &manifest("a", "A", None, 0)),
            ]
        };
        assert!(!show_empty_state(false, None, &with_user));

        assert!(show_load_error(false, Some("boom")));
        assert!(!show_load_error(true, Some("boom")), "加载中不显示错误");
        assert!(!show_load_error(false, None));
    }

    #[test]
    fn draft_is_deep_copy_and_commit_clamps_history() {
        let mut config = Config::default();
        config.options.quick_switch.max_history = 5;
        config.options.quick_switch.excluded_prefixes = vec!["C:\\tmp".to_string()];

        let mut draft = draft_from(&config);
        draft.max_history = 0; // 期望被钳到 1
        draft.excluded_prefixes.push("D:\\x".to_string());
        draft.auto_show = true;

        // 副本编辑不影响真源
        assert_eq!(config.options.quick_switch.max_history, 5);
        assert_eq!(config.options.quick_switch.excluded_prefixes.len(), 1);

        assert!(commit_draft(&mut config, &draft), "有变更 ⇒ 需保存");
        assert_eq!(config.options.quick_switch.max_history, 1, "下限钳到 1");
        assert_eq!(config.options.quick_switch.excluded_prefixes.len(), 2);
        assert!(config.options.quick_switch.auto_show);

        assert!(!commit_draft(&mut config, &draft), "再次提交无变更");
    }

    #[test]
    fn clear_history_truncates_file_and_keeps_it() {
        let root = std::env::temp_dir().join("keyflux-qs-history-test");
        let path = root.join(HISTORY_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().expect("有父目录")).expect("建目录");
        std::fs::write(&path, "C:\\a\nC:\\b\n").expect("写入历史");

        let written = clear_history(&root).expect("清空应成功");
        assert_eq!(written, path);
        assert!(path.exists(), "文件本身保留");
        assert_eq!(std::fs::read(&path).expect("读取").len(), 0, "内容被截断");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn setting_validation_matches_legacy() {
        let number = |min: Option<i64>, max: Option<i64>, setting_type: &str| PluginSetting {
            key: "k".to_string(),
            setting_type: setting_type.to_string(),
            min,
            max,
            ..Default::default()
        };

        let integer = number(Some(1), Some(9), "number");
        assert_eq!(
            validate_setting(&integer, ""),
            None,
            "空 = 清除覆盖，永远合法"
        );
        assert_eq!(validate_setting(&integer, "5"), None);
        assert_eq!(
            validate_setting(&integer, "abc").as_deref(),
            Some("integer")
        );
        assert_eq!(validate_setting(&integer, "0").as_deref(), Some(">= 1"));
        assert_eq!(validate_setting(&integer, "10").as_deref(), Some("<= 9"));

        let character = number(None, None, "char");
        assert_eq!(max_length(&character), 1, "char 上限恒为 1");
        assert_eq!(validate_setting(&character, " "), None, "空格合法");
        assert_eq!(
            validate_setting(&character, "\t").as_deref(),
            Some("printable only")
        );
        assert_eq!(validate_setting(&character, "ab").as_deref(), Some("≤ 1"));

        let mut text = number(None, None, "text");
        assert_eq!(max_length(&text), DEFAULT_MAX_LENGTH, "未声明回退 1024");
        text.max_length = 7;
        assert_eq!(max_length(&text), 7);
        assert_eq!(validate_setting(&text, "12345678").as_deref(), Some("≤ 7"));
    }

    #[test]
    fn labels_prefer_english_and_fall_back() {
        let setting = PluginSetting {
            key: "k".to_string(),
            label: "中文".to_string(),
            label_en: Some("English".to_string()),
            hint_en: Some("Hint".to_string()),
            ..Default::default()
        };
        assert_eq!(setting_label(&setting, false), "中文");
        assert_eq!(setting_label(&setting, true), "English");
        assert_eq!(setting_hint(&setting, false), "Hint", "中文缺失回退英文");

        let bare = PluginSetting {
            key: "k".to_string(),
            label: "中文".to_string(),
            ..Default::default()
        };
        assert_eq!(setting_hint(&bare, true), "", "双语皆缺为空串");
    }

    #[test]
    fn range_hint_covers_all_boundary_shapes() {
        let make = |min: Option<i64>, max: Option<i64>| PluginSetting {
            key: "n".to_string(),
            setting_type: "number".to_string(),
            min,
            max,
            ..Default::default()
        };
        assert_eq!(range_hint(&make(Some(1), Some(9))), "1 – 9");
        assert_eq!(range_hint(&make(Some(1), None)), ">= 1");
        assert_eq!(range_hint(&make(None, Some(9))), "<= 9");
        assert_eq!(range_hint(&make(None, None)), "");

        let text = PluginSetting {
            key: "t".to_string(),
            setting_type: "text".to_string(),
            ..Default::default()
        };
        assert_eq!(range_hint(&text), "", "非数字项无范围提示");
    }

    #[test]
    fn rows_merge_server_values_with_manifest_fallback() {
        let manifest = PluginManifest {
            id: "p".to_string(),
            name: "P".to_string(),
            settings: Some(vec![PluginSetting {
                key: "local".to_string(),
                default: Some("d".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let response = PluginSettingsResponse {
            id: "p".to_string(),
            settings: vec![
                PluginSetting {
                    key: "a".to_string(),
                    default: Some("A".to_string()),
                    ..Default::default()
                },
                PluginSetting {
                    key: "b".to_string(),
                    ..Default::default()
                },
            ],
            values: [("b".to_string(), "vb".to_string())].into_iter().collect(),
        };

        let rows = settings_rows(&response, &manifest);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0.key, "a");
        assert_eq!(rows[0].1, "A", "无当前值 → 声明默认值");
        assert_eq!(rows[1].1, "vb", "当前值优先于默认值");

        // 服务端空声明 → 退回本地 manifest（窗口期兜底）
        let empty = PluginSettingsResponse {
            id: "p".to_string(),
            ..Default::default()
        };
        let rows = settings_rows(&empty, &manifest);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.key, "local");
        assert_eq!(rows[0].1, "d");
    }

    #[test]
    fn errors_join_ignores_empty_list() {
        assert_eq!(join_errors(None), None);
        assert_eq!(join_errors(Some(&vec![])), None);
        assert_eq!(
            join_errors(Some(&vec!["a".to_string(), "b".to_string()])),
            Some("a\nb".to_string())
        );
    }
}
