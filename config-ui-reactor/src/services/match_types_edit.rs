//! 自定义匹配类型编辑（复刻 `MatchTypesDialogViewModel` 的纯逻辑面）。
//!
//! 旧实现：`ViewModels/MatchTypesDialogViewModel.cs`（836 行）。此处只保留**可判定**
//! 的部分：草稿模型、代号唯一化（2542 冲突加后缀）、写回/删除级联；其余（表单交互）
//! 在 `ui`/`app` 侧。深度校验交后端（`PUT /config` 的 `ValidateMatchTypes`，400 文案
//! 经保存失败链路回显）。

use crate::models::{Config, MatchRule, MatchType, SelectedMapping};

/// 「新建」时无对应下标。
pub const NEW_INDEX: usize = usize::MAX;

/// 编辑草稿（对应旧 `DraftRow` + 表单字段）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MatchTypeDraft {
    /// `config.match_types` 下标；[`NEW_INDEX`] = 新建。
    pub index: usize,
    /// 代号 `^[a-z][a-z0-9_]{0,23}$`（新建时自动生成，编辑态只读展示）。
    pub id: String,
    /// 显示名（2523）。
    pub label: String,
    /// 英文显示名（2567，可选）。
    pub label_en: String,
    /// `"text"` | `"fileExt"`（新建可选，编辑态锁定——kind 决定引用语义）。
    pub kind: String,
    /// 判定规则（仅 `kind = text`）：`(op, value)`，op ∈ equals/prefix/suffix/contains。
    pub rules: Vec<(String, String)>,
    /// 后缀串（仅 `kind = fileExt`，逗号分隔原文）。
    pub exts: String,
}

impl MatchTypeDraft {
    /// 从既有类型载入（编辑态）。
    pub fn from_existing(index: usize, mt: &MatchType) -> Self {
        Self {
            index,
            id: mt.id.clone(),
            label: mt.label.clone(),
            label_en: mt.label_en.clone(),
            kind: mt.kind.clone(),
            rules: mt
                .rules
                .iter()
                .map(|rule| (rule.op.clone(), rule.value.clone()))
                .collect(),
            exts: mt.exts.join(","),
        }
    }

    /// 新建草稿：代号自动生成（2542 冲突加后缀）。
    pub fn new_draft(config: &Config, kind: &str) -> Self {
        Self {
            index: NEW_INDEX,
            id: unique_id(config, "custom"),
            label: String::new(),
            label_en: String::new(),
            kind: kind.to_string(),
            rules: vec![("contains".to_string(), String::new())],
            exts: String::new(),
        }
    }

    /// 引用命名空间（`type:<id>`）。
    pub fn type_ref(&self) -> String {
        format!("type:{}", self.id)
    }
}

/// 代号唯一化：`base`、`base_2`、`base_3`…（复刻 `UniqueId` 的冲突加后缀，2542 文案场景）。
pub fn unique_id(config: &Config, base: &str) -> String {
    let base = base.to_lowercase();
    let taken = |candidate: &str| {
        config
            .match_types
            .iter()
            .any(|mt| mt.id.eq_ignore_ascii_case(candidate))
    };
    if !taken(&base) {
        return base;
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{base}_{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// 轻量客户端校验（深度校验在后端 `PUT /config`）：名称必填；text 至少一条**非空**规则值；
/// fileExt 后缀串非空。`Err` = 直接可展示的文案键或文本。
pub fn validate(draft: &MatchTypeDraft) -> Result<(), String> {
    if draft.label.trim().is_empty() {
        return Err(crate::services::i18n::t("2523"));
    }
    if draft.kind == "text" {
        let has_value = draft
            .rules
            .iter()
            .any(|(_, value)| !value.trim().is_empty());
        if !has_value {
            return Err(crate::services::i18n::t("1104_any"));
        }
    } else if draft.exts.trim().is_empty() {
        return Err(crate::services::i18n::t("2528"));
    }
    Ok(())
}

/// 草稿落盘：新建追加（`order` = 追加到末位）、编辑就地写回。
pub fn apply(config: &mut Config, draft: &MatchTypeDraft) {
    let mt = MatchType {
        id: draft.id.clone(),
        label: draft.label.trim().to_string(),
        label_en: draft.label_en.trim().to_string(),
        kind: draft.kind.clone(),
        rules: draft
            .rules
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(op, value)| MatchRule {
                op: op.clone(),
                value: value.trim().to_string(),
            })
            .collect(),
        exts: crate::services::selected_action::normalize_exts(&draft.exts),
        order: i32::try_from(config.match_types.len()).unwrap_or(i32::MAX),
    };
    if draft.index == NEW_INDEX || draft.index >= config.match_types.len() {
        config.match_types.push(mt);
    } else {
        config.match_types[draft.index] = mt;
    }
}

/// 删除类型并级联：删掉引用 `type:<id>` 的 mapping（1:1 复刻旧对话框的删除语义；
/// 旧版还会删同名专属行为包——该半侧由调用方走行为库 API）。
pub fn delete(config: &mut Config, index: usize) -> Option<String> {
    let mt = config.match_types.get(index)?;
    let type_ref = format!("type:{}", mt.id);
    config
        .selected_action
        .mappings
        .retain(|mapping: &SelectedMapping| mapping.match_value != type_ref);
    let id = mt.id.clone();
    config.match_types.remove(index);
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SelectedEntry;

    fn config_with_types(ids: &[&str]) -> Config {
        Config {
            match_types: ids
                .iter()
                .map(|id| MatchType {
                    id: (*id).to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn unique_id_appends_suffix_on_conflict() {
        let config = config_with_types(&["custom"]);
        assert_eq!(unique_id(&config, "custom"), "custom_2");
        assert_eq!(unique_id(&config, "other"), "other");
        let config = config_with_types(&["custom", "custom_2"]);
        assert_eq!(unique_id(&config, "CUSTOM"), "custom_3", "大小写不敏感");
    }

    #[test]
    fn validate_requires_label_and_rules_or_exts() {
        let mut draft = MatchTypeDraft::new_draft(&config_with_types(&[]), "text");
        assert!(validate(&draft).is_err(), "名称必填");
        draft.label = "我的类型".to_string();
        assert!(validate(&draft).is_err(), "text 至少一条非空规则值");
        draft.rules[0].1 = "https://".to_string();
        assert!(validate(&draft).is_ok());

        let mut file_draft = MatchTypeDraft::new_draft(&config_with_types(&[]), "fileExt");
        file_draft.label = "设计文件".to_string();
        assert!(validate(&file_draft).is_err(), "fileExt 后缀串必填");
        file_draft.exts = "psd, ai".to_string();
        assert!(validate(&file_draft).is_ok());
    }

    #[test]
    fn apply_appends_new_and_updates_existing() {
        let mut config = config_with_types(&["a"]);
        let mut draft = MatchTypeDraft::new_draft(&config, "text");
        draft.label = "B".to_string();
        draft.rules[0] = ("prefix".to_string(), "mailto:".to_string());
        apply(&mut config, &draft);
        assert_eq!(config.match_types.len(), 2);
        assert_eq!(config.match_types[1].id, draft.id);
        assert_eq!(config.match_types[1].order, 1, "order = 追加序");

        draft.index = 1;
        draft.label = "B2".to_string();
        apply(&mut config, &draft);
        assert_eq!(config.match_types[1].label, "B2");
        assert_eq!(config.match_types.len(), 2, "编辑不追加");
    }

    #[test]
    fn delete_cascades_type_ref_mappings() {
        let mut config = config_with_types(&["a", "b"]);
        config.selected_action.mappings.push(SelectedMapping {
            match_type: "textType".to_string(),
            match_value: "type:a".to_string(),
            entries: vec![SelectedEntry::default()],
        });
        config.selected_action.mappings.push(SelectedMapping {
            match_type: "textType".to_string(),
            match_value: "url".to_string(),
            entries: vec![SelectedEntry::default()],
        });
        assert_eq!(delete(&mut config, 0).as_deref(), Some("a"));
        assert_eq!(config.match_types.len(), 1);
        assert_eq!(config.selected_action.mappings.len(), 1, "引用级联删除");
        assert_eq!(config.selected_action.mappings[0].match_value, "url");
        assert_eq!(delete(&mut config, 9), None, "越界安全");
    }
}
