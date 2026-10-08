//! DTO 层：字段名必须与 Go / C# 契约**逐字一致**。
//!
//! 为什么逐字一致：Go 后端保存时会**全量落盘** config.json，任何它不认识的字段会被静默剥掉，
//! 于是「前端改了名但后端不认识」会表现为**静默丢数据**。
//!
//! 移植起点（**均已退役** —— Go 后端 `36ccb83`、Avalonia 客户端 `1f3dc9f`；现行契约 = 本模块 + `generator/model.rs`）：
//! * `config-ui-avalonia/Models/ConfigModels.cs`（561 行）
//! * Go 蓝本 `config-server/internal/script/model/types.go`
//! * 端点点清单：`config-ui-avalonia/Services/SettingsApiClient.cs`
//!   溯源：`git show 36ccb83^:config-server/internal/script/model/types.go`。
//!
//! 本模块的单元测试即旧版 `ModelUnitTests` 的等价物（键名/键集合断言）。

pub mod behavior;
pub mod config;
pub mod plugins;

pub use behavior::*;
pub use config::*;
pub use plugins::*;

// 三套模型（models / generator::model / server::dto）的 wire 字段契约对账。
// 仅测试期编译；见文件头注（模块化审查报告 §5.1 / 问题 #2 的护栏部分）。
#[cfg(test)]
mod contract;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn key_set(v: &Value) -> Vec<String> {
        let mut keys: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    #[test]
    fn config_top_level_keys_match_contract() {
        let v = serde_json::to_value(Config::default()).unwrap();
        assert_eq!(
            key_set(&v),
            vec![
                "fileGroups",
                "keymaps",
                "matchTypes",
                "options",
                "overviewDocMd",
                "selectedAction"
            ]
        );
    }

    #[test]
    fn keymap_uses_literal_id_keys() {
        let km = Keymap {
            id: 1,
            parent_id: 2,
            ..Default::default()
        };
        let v = serde_json::to_value(&km).unwrap();
        assert_eq!(v["parentID"], 2, "必须是 parentID");
        assert!(v.get("parentId").is_none(), "不得出现 camelCase 推导名");
        assert!(v.get("isNew").is_none(), "is_new 不参与序列化");
    }

    #[test]
    fn action_uses_literal_id_keys() {
        let a = Action {
            window_group_id: 1,
            type_id: 2,
            value_id: 3,
            ..Default::default()
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["windowGroupID"], 1);
        assert_eq!(v["actionTypeID"], 2);
        assert_eq!(v["actionValueID"], 3);
        assert!(v.get("typeID").is_none());
        assert!(v.get("isEmpty").is_none(), "is_empty 不参与序列化");
    }

    #[test]
    fn options_keys_match_contract() {
        let v = serde_json::to_value(Options::default()).unwrap();
        assert_eq!(
            key_set(&v),
            vec![
                "commandFont",
                "commandInputSkin",
                "hideMatrix",
                "keyMapping",
                "keyboardLayout",
                "keyfluxVersion",
                "language",
                "mouse",
                "pathVariables",
                "plugins",
                "scroll",
                "startup",
                "windowGroups",
            ]
        );
    }

    #[test]
    fn command_input_skin_has_18_string_fields() {
        let v = serde_json::to_value(CommandInputSkin::default()).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 18, "CommandInputSkin 必须恰好 18 个字段");
        assert!(obj.values().all(|x| x.is_string()), "全部为字符串");
    }

    #[test]
    fn selected_entry_omits_empty_optional_strings() {
        let empty = serde_json::to_value(SelectedEntry::default()).unwrap();
        assert!(
            empty.get("actionValue").is_none(),
            "空串 actionValue 必须省略键"
        );
        assert!(
            empty.get("workingDir").is_none(),
            "空串 workingDir 必须省略键"
        );

        let filled = SelectedEntry {
            action_value: "calc".to_string(),
            working_dir: "C:\\".to_string(),
            ..Default::default()
        };
        let v = serde_json::to_value(&filled).unwrap();
        assert_eq!(v["actionValue"], "calc");
        assert_eq!(v["workingDir"], "C:\\");
    }

    #[test]
    fn nullable_option_sections_are_tolerated() {
        let o: Options = serde_json::from_str(r#"{"plugins":null,"commandFont":null}"#).unwrap();
        assert_eq!(o.plugins, PluginsOption::default());
        assert_eq!(o.command_font, CommandFontOption::default());
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn sample_round_trip_keeps_contract_values() {
        let raw = r#"{
          "keymaps":[{"id":1,"name":"F","enable":true,"hotkey":"f","parentID":0,"delay":0,
            "disableAt":"","hotkeys":{"a":[{"windowGroupID":-1,"actionTypeID":2,"comment":"",
            "hotkey":"","keysToSend":"x","remapToKey":"","actionValueID":0,"winTitle":"",
            "target":"","args":"","workingDir":"","runAsAdmin":false,"runInBackground":false,
            "detectHiddenWindow":false,"ahkCode":""}]}}],
          "options":{"language":"zh"},"selectedAction":{},"fileGroups":[],"matchTypes":[],
          "overviewDocMd":""}"#;
        let c: Config = serde_json::from_str(raw).unwrap();
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["keymaps"][0]["parentID"], 0);
        assert_eq!(v["keymaps"][0]["hotkeys"]["a"][0]["actionTypeID"], 2);
        assert_eq!(v["keymaps"][0]["hotkeys"]["a"][0]["windowGroupID"], -1);
        assert_eq!(v["options"]["language"], "zh");
    }
}
