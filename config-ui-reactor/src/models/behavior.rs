//! 行为包 manifest DTO —— wire 格式 = 文件格式 = 后端 CONTRACTS §3.9（specVersion 1）。
//!
//! 字段名与 Go `behaviors.Pack/AppliesToEntry/Entry/EntryParams`（`internal/behaviors/behaviors.go`）
//! 及 C# `Models/BehaviorModels.cs` **逐字一致**（Go 全量落盘会静默剥掉未知字段，见迁移铁律）。

use serde::{Deserialize, Serialize};

/// 一个行为包（内置 11 个 + 用户包）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorPack {
    pub id: String,

    pub name: String,

    #[serde(default)]
    pub name_en: Option<String>,

    #[serde(default)]
    pub version: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub spec_version: i32,

    #[serde(default)]
    pub applies_to: Vec<BehaviorAppliesTo>,

    #[serde(default)]
    pub entry: BehaviorEntry,

    #[serde(default)]
    pub permissions: Option<Vec<String>>,

    /// 强绑定的自定义匹配类型 id（仅供 UI 展示，不参与覆盖判定）。
    #[serde(default)]
    pub bound_type_id: Option<String>,

    /// 来源标记（builtin/user），后端加载期附加，不属于包文件本身。
    #[serde(default)]
    pub source: Option<String>,
}

/// 生效前提条目：fileExt 用显式后缀集（`"*"` = 任意文件），textType 用特征枚举值。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BehaviorAppliesTo {
    /// `"fileExt"` | `"textType"`。
    #[serde(default, rename = "type")]
    pub kind: String,

    #[serde(default)]
    pub exts: Option<Vec<String>>,

    /// textType 前提的特征值（`"plain"` 通配覆盖任意自定义文本类型引用）。
    #[serde(default)]
    pub value: Option<String>,

    /// 该前提桶的默认推荐行为。
    #[serde(default, rename = "default")]
    pub is_default: bool,
}

/// 包入口：builtin = 基础动作组合（一期）；script = 自定义脚本（二期）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BehaviorEntry {
    #[serde(default)]
    pub kind: String,

    #[serde(default)]
    pub action: Option<String>,

    #[serde(default)]
    pub params: Option<BehaviorEntryParams>,

    #[serde(default)]
    pub file: Option<String>,

    #[serde(default)]
    pub func: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorEntryParams {
    #[serde(default)]
    pub action_value: Option<String>,

    #[serde(default)]
    pub working_dir: Option<String>,
}

/// `GET /api/behaviors` 响应：内置 + 用户两组（各自 ID 字典序）+ 加载告警。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BehaviorCatalogResponse {
    #[serde(default)]
    pub builtin: Vec<BehaviorPack>,

    #[serde(default)]
    pub user: Vec<BehaviorPack>,

    #[serde(default)]
    pub errors: Option<Vec<String>>,
}

// wire 格式的键名校验（`type`/`default` 是关键字面量，必须显式 rename）。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_to_uses_literal_keys() {
        let a = BehaviorAppliesTo {
            kind: "textType".to_string(),
            value: Some("plain".to_string()),
            is_default: true,
            ..Default::default()
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["type"], "textType");
        assert_eq!(v["value"], "plain");
        assert_eq!(v["default"], true);
        assert!(v.get("kind").is_none());
        assert!(v.get("is_default").is_none());
    }

    #[test]
    fn entry_params_use_camel_case_keys() {
        let p = BehaviorEntryParams {
            action_value: Some("%selected%".to_string()),
            working_dir: Some("C:\\".to_string()),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["actionValue"], "%selected%");
        assert_eq!(v["workingDir"], "C:\\");
    }

    #[test]
    fn catalog_response_parses_backend_shape() {
        // 真实后端（bin/behaviors）响应的最小切片
        let raw = r#"{
          "builtin": [{
            "id": "open_url", "name": "打开链接", "nameEn": "Open URL",
            "specVersion": 1,
            "appliesTo": [{"type": "textType", "value": "url", "default": true}],
            "entry": {"kind": "builtin", "action": "open_url", "params": {"actionValue": ""}},
            "source": "builtin"
          }],
          "user": [],
          "errors": null
        }"#;
        let c: BehaviorCatalogResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(c.builtin.len(), 1);
        assert_eq!(c.builtin[0].id, "open_url");
        assert_eq!(c.builtin[0].applies_to[0].value.as_deref(), Some("url"));
        assert!(c.builtin[0].applies_to[0].is_default);
        assert_eq!(c.builtin[0].entry.action.as_deref(), Some("open_url"));
        assert_eq!(
            c.builtin[0].entry.params.as_ref().unwrap().action_value,
            Some(String::new())
        );
    }
}
