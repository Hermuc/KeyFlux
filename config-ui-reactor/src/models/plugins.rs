//! 插件相关 DTO（对齐 `config-ui-avalonia/Models/PluginModels.cs`）。
//!
//! ⚠️ 字段名必须与 Go 契约逐字一致（`nameEn` / `specVersion` 等为驼峰）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 插件 manifest（`GET /api/plugins` 的条目；内置卡亦用同一结构合成）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// 英文显示名（英文界面优先使用）。
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// 规格版本号。
    #[serde(default)]
    pub spec_version: i32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub entry: PluginEntry,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    /// 声明式设置项：**非空**即表示该插件可配置。
    #[serde(default)]
    pub settings: Option<Vec<PluginSetting>>,
}

/// 插件入口声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub func: Option<String>,
}

impl Default for PluginEntry {
    fn default() -> Self {
        Self {
            kind: default_kind(),
            file: None,
            func: None,
        }
    }
}

fn default_kind() -> String {
    "script".to_string()
}

/// `GET /api/plugins` 响应：用户插件列表（ID 字典序）+ 逐包加载告警。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginListResponse {
    #[serde(default)]
    pub plugins: Vec<PluginManifest>,
    /// 后端逐包错误隔离汇总；`None` = 无告警。
    #[serde(default)]
    pub errors: Option<Vec<String>>,
}

/// 插件市场目录（发布侧 `plugins/marketplace.json`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketCatalog {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plugins: Vec<MarketPluginEntry>,
}

/// 市场目录条目（`url` 指向插件包 zip）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPluginEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// 插件包 zip 下载地址。
    #[serde(default)]
    pub url: String,
}

/// 插件声明式设置项（manifest.settings 条目；服务端 `GET /api/plugins/:id/settings` 同构）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSetting {
    #[serde(default)]
    pub key: String,
    /// 编辑器类型：`char` / `text` / `number` / `file`（`type` 与 Rust 关键字冲突，显式重命名）。
    #[serde(rename = "type", default = "default_setting_type")]
    pub setting_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub label_en: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    /// 文件类型的选择器过滤（后端声明，前端透传给文件对话框）。
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub hint_en: Option<String>,
    /// 文本长度上限（`0` = 未声明，前端回退 1024）。
    #[serde(default)]
    pub max_length: i32,
    /// 数字项下限。
    #[serde(default)]
    pub min: Option<i64>,
    /// 数字项上限。
    #[serde(default)]
    pub max: Option<i64>,
}

fn default_setting_type() -> String {
    "text".to_string()
}

/// `GET /api/plugins/:id/settings` 响应：声明 + 默认值合并后的完整值表。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub settings: Vec<PluginSetting>,
    /// 当前值表（缺项 = 用声明默认值）。
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

/// `PUT /api/plugins/:id/settings` 请求体。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsRequest {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_catalog_deserializes_published_payload() {
        let raw = r#"{
            "name": "KeyFlux 插件市场",
            "plugins": [
                { "id": "everything_search", "name": "Everything 搜索", "nameEn": "Everything Search",
                  "version": "1.0.1", "description": "描述", "author": "KeyFlux",
                  "url": "https://example.com/everything.zip" }
            ]
        }"#;
        let catalog: MarketCatalog = serde_json::from_str(raw).expect("应能反序列化");
        assert_eq!(catalog.plugins.len(), 1);
        let entry = &catalog.plugins[0];
        assert_eq!(entry.id, "everything_search");
        assert_eq!(entry.name_en.as_deref(), Some("Everything Search"));
        assert!(entry.url.ends_with(".zip"));
    }

    #[test]
    fn market_catalog_tolerates_minimal_payload() {
        let catalog: MarketCatalog =
            serde_json::from_str(r#"{"plugins":[{"id":"a","name":"A","url":"u"}]}"#)
                .expect("最简载荷也应可解析");
        assert_eq!(catalog.name, "");
        assert_eq!(catalog.plugins[0].version, None);
    }

    #[test]
    fn manifest_deserializes_go_payload() {
        let raw = r#"{
            "id": "my_plug",
            "name": "我的插件",
            "nameEn": "My Plug",
            "version": "1.2.0",
            "specVersion": 1,
            "description": "描述",
            "author": "me",
            "entry": { "kind": "script", "file": "main.ahk" },
            "permissions": ["clipboard"],
            "settings": [{ "key": "x" }]
        }"#;
        let manifest: PluginManifest = serde_json::from_str(raw).expect("应能反序列化");
        assert_eq!(manifest.id, "my_plug");
        assert_eq!(manifest.name_en.as_deref(), Some("My Plug"));
        assert_eq!(manifest.spec_version, 1);
        assert_eq!(manifest.entry.kind, "script");
        assert_eq!(manifest.entry.file.as_deref(), Some("main.ahk"));
        assert_eq!(manifest.settings.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn plugin_setting_uses_literal_type_key() {
        let raw = r#"{"key":"sep","type":"char","label":"分隔符","default":" ",
            "maxLength":1,"min":-5,"max":9}"#;
        let setting: PluginSetting = serde_json::from_str(raw).expect("应能反序列化");
        assert_eq!(setting.setting_type, "char");
        assert_eq!(setting.max_length, 1);
        assert_eq!(setting.min, Some(-5));
        assert_eq!(setting.max, Some(9));

        // 序列化必须写回 "type"（而非 settingType）
        let v = serde_json::to_value(&setting).unwrap();
        assert_eq!(v["type"], "char");
        assert!(v.get("settingType").is_none());
    }

    #[test]
    fn settings_round_trip_covers_declared_fields() {
        let raw = r#"{"id":"everything_search","settings":[{"key":"sep","type":"char"}],
            "values":{"sep":" "}}"#;
        let response: PluginSettingsResponse = serde_json::from_str(raw).expect("应能反序列化");
        assert_eq!(response.settings[0].key, "sep");
        assert_eq!(response.values.get("sep").map(String::as_str), Some(" "));

        let request = PluginSettingsRequest {
            values: [("sep".to_string(), String::new())].into_iter().collect(),
        };
        let v = serde_json::to_value(&request).unwrap();
        assert_eq!(v["values"]["sep"], "");
    }

    #[test]
    fn manifest_tolerates_missing_optionals() {
        let manifest: PluginManifest =
            serde_json::from_str(r#"{"id":"a","name":"A"}"#).expect("最简载荷也应可解析");
        assert_eq!(manifest.version, None);
        assert_eq!(manifest.entry.kind, "script", "entry 缺省 kind=script");
        assert_eq!(manifest.settings, None);
    }

    #[test]
    fn list_response_carries_errors() {
        let list: PluginListResponse =
            serde_json::from_str(r#"{"plugins":[],"errors":["bad.zip: 缺少 manifest"]}"#)
                .expect("应能反序列化");
        assert!(list.plugins.is_empty());
        assert_eq!(list.errors.as_ref().map(Vec::len), Some(1));

        // errors 为 null（后端 omitempty）时容忍
        let list: PluginListResponse =
            serde_json::from_str(r#"{"plugins":[]}"#).expect("errors 缺失也应可解析");
        assert_eq!(list.errors, None);
    }
}
