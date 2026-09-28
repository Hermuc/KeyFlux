//! 行为库编辑（复刻 `BehaviorLibraryViewModel` / `BehaviorEditViewModel` 的纯逻辑面）。
//!
//! 权威契约：行为包格式冻结（`CONTRACTS.md` §3.9，specVersion 1）——manifest 字段与
//! Go/C# 逐字一致（见 `models::behavior`）。深度校验在后端（`POST/PUT /api/behaviors`
//! 校验失败 400，文案经 `error_message` 回显）；此处只保留草稿模型与轻量检查。

use crate::models::{BehaviorAppliesTo, BehaviorEntry, BehaviorEntryParams, BehaviorPack};
use crate::services::selected_action::Catalog;

/// 新建草稿标记（无对应目录项）。
pub const NEW_INDEX: usize = usize::MAX;

/// 一条生效前提的编辑态（exts 串 / 特征值共用 `value`，按 `kind` 解释）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppliesDraft {
    /// `"fileExt"` | `"textType"`。
    pub kind: String,
    /// fileExt = 逗号分隔后缀串；textType = 特征值（`url`/`plain`/…）。
    pub value: String,
    /// 该前提桶默认推荐。
    pub is_default: bool,
}

/// 行为包编辑草稿。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BehaviorDraft {
    /// 目录合并视图（内置在前）中的下标；[`NEW_INDEX`] = 新建。
    pub index: usize,
    /// 包 ID（新建可改；编辑态只读——ID 即目录名）。
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub description: String,
    /// 生效前提（≥1 条）。
    pub applies: Vec<AppliesDraft>,
    /// 基础动作（builtin entry 的 `action`；如 open_url / copy / run）。
    pub base_action: String,
    /// 默认命令模板（entry.params.actionValue）。
    pub template: String,
    /// 默认工作目录。
    pub working_dir: String,
}

impl BehaviorDraft {
    /// 从既有包载入（编辑态；内置包仅展示）。
    pub fn from_pack(pack: &BehaviorPack) -> Self {
        Self {
            index: NEW_INDEX,
            id: pack.id.clone(),
            name: pack.name.clone(),
            name_en: pack.name_en.clone().unwrap_or_default(),
            description: pack.description.clone().unwrap_or_default(),
            applies: pack
                .applies_to
                .iter()
                .map(|entry| AppliesDraft {
                    kind: entry.kind.clone(),
                    value: entry
                        .exts
                        .as_ref()
                        .map(|exts| exts.join(","))
                        .or_else(|| entry.value.clone())
                        .unwrap_or_default(),
                    is_default: entry.is_default,
                })
                .collect(),
            base_action: if pack.entry.kind.eq_ignore_ascii_case("builtin") {
                pack.entry.action.clone().unwrap_or_default()
            } else {
                String::new()
            },
            template: pack
                .entry
                .params
                .as_ref()
                .and_then(|params| params.action_value.clone())
                .unwrap_or_default(),
            working_dir: pack
                .entry
                .params
                .as_ref()
                .and_then(|params| params.working_dir.clone())
                .unwrap_or_default(),
        }
        .with_index(NEW_INDEX)
    }

    fn with_index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    /// 新建草稿：一条空前提 + 默认基础动作。
    pub fn new_draft() -> Self {
        Self {
            index: NEW_INDEX,
            applies: vec![AppliesDraft {
                kind: "textType".to_string(),
                value: String::new(),
                is_default: false,
            }],
            ..Default::default()
        }
    }

    /// 组装 wire 格式包（builtin entry + params；specVersion 1）。
    pub fn to_pack(&self) -> BehaviorPack {
        let has_params = !self.template.is_empty() || !self.working_dir.is_empty();
        BehaviorPack {
            id: self.id.trim().to_string(),
            name: self.name.trim().to_string(),
            name_en: Some(self.name_en.trim().to_string()).filter(|text| !text.is_empty()),
            description: Some(self.description.trim().to_string()).filter(|text| !text.is_empty()),
            spec_version: 1,
            applies_to: self
                .applies
                .iter()
                .filter(|applies| !applies.value.trim().is_empty())
                .map(|applies| {
                    if applies.kind == "fileExt" {
                        BehaviorAppliesTo {
                            kind: "fileExt".to_string(),
                            exts: Some(crate::services::selected_action::normalize_exts(
                                &applies.value,
                            )),
                            value: None,
                            is_default: applies.is_default,
                        }
                    } else {
                        BehaviorAppliesTo {
                            kind: "textType".to_string(),
                            exts: None,
                            value: Some(applies.value.trim().to_string()),
                            is_default: applies.is_default,
                        }
                    }
                })
                .collect(),
            entry: BehaviorEntry {
                kind: "builtin".to_string(),
                action: Some(self.base_action.trim().to_string()).filter(|text| !text.is_empty()),
                params: has_params.then(|| BehaviorEntryParams {
                    action_value: Some(self.template.clone()).filter(|text| !text.is_empty()),
                    working_dir: Some(self.working_dir.clone()).filter(|text| !text.is_empty()),
                }),
                ..Default::default()
            },
            permissions: None,
            bound_type_id: None,
            source: None,
            ..Default::default()
        }
    }
}

/// 轻量校验：ID/名称必填、ID 字符集 `^[A-Za-z][A-Za-z0-9_-]{0,63}$`、至少一条非空前提、
/// 基础动作必选。深度校验（覆盖合法性等）在后端。
pub fn validate(draft: &BehaviorDraft) -> Result<(), String> {
    let id = draft.id.trim();
    if id.is_empty() {
        return Err(crate::services::i18n::t("1103_only"));
    }
    let id_ok = id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !id_ok {
        return Err(crate::services::i18n::t("1103_only"));
    }
    if draft.name.trim().is_empty() {
        return Err(crate::services::i18n::t("1103_only"));
    }
    if !draft
        .applies
        .iter()
        .any(|applies| !applies.value.trim().is_empty())
    {
        return Err(crate::services::i18n::t("1104_any"));
    }
    if draft.base_action.trim().is_empty() {
        return Err(crate::services::i18n::t("1103_only"));
    }
    Ok(())
}

/// 基础动作下拉候选：内置包的 entry.action 全集 ∪ 无参基础动作集（去重排序）。
pub fn base_action_options(catalog: &Catalog) -> Vec<String> {
    let mut options: Vec<String> = Vec::new();
    for pack in catalog.builtin.iter() {
        if pack.entry.kind.eq_ignore_ascii_case("builtin")
            && let Some(action) = &pack.entry.action
            && !options.contains(action)
        {
            options.push(action.clone());
        }
    }
    for action in crate::services::selected_action::BASE_ACTION_NO_VALUE {
        if !options.contains(&action.to_string()) {
            options.push(action.to_string());
        }
    }
    options.sort();
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_pack_preserves_semantics() {
        let mut draft = BehaviorDraft::new_draft();
        draft.id = "es_search".to_string();
        draft.name = "Everything 搜索".to_string();
        draft.base_action = "run".to_string();
        draft.template = "es.exe {selected}".to_string();
        draft.applies[0] = AppliesDraft {
            kind: "textType".to_string(),
            value: "plain".to_string(),
            is_default: false,
        };
        let pack = draft.to_pack();
        assert_eq!(pack.id, "es_search");
        assert_eq!(pack.spec_version, 1);
        assert_eq!(pack.entry.kind, "builtin");
        assert_eq!(
            pack.entry.params.as_ref().unwrap().action_value.as_deref(),
            Some("es.exe {selected}")
        );
        assert_eq!(pack.applies_to[0].value.as_deref(), Some("plain"));

        let reloaded = BehaviorDraft::from_pack(&pack);
        assert_eq!(reloaded.id, "es_search");
        assert_eq!(reloaded.template, "es.exe {selected}");
        assert_eq!(reloaded.applies[0].value, "plain");
    }

    #[test]
    fn validate_rejects_bad_id_and_empty_applies() {
        let mut draft = BehaviorDraft::new_draft();
        assert!(validate(&draft).is_err(), "ID 必填");
        draft.id = "9bad".to_string();
        assert!(validate(&draft).is_err(), "ID 首字符须字母");
        draft.id = "ok_id".to_string();
        assert!(validate(&draft).is_err(), "前提值必填");
        draft.applies[0].value = "url".to_string();
        assert!(validate(&draft).is_err(), "基础动作必选");
        draft.name = "示例行为".to_string();
        draft.base_action = "open_url".to_string();
        assert!(validate(&draft).is_ok());
    }
}
