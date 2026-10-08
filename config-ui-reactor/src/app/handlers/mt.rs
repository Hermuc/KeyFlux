//! mt —— `Shell::update` 的 文本特征/匹配类型注册表编辑 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_mt`。

use super::super::*;

impl Shell {
    /// `mt` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_mt(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::MatchTypesOpen => {
                self.mt_dialog = true;
                self.mt_status = None;
                self.mt_test_result = None;
                if self.mt_draft.is_none() {
                    self.mt_draft = self
                        .config
                        .as_ref()
                        .map(|config| match_types_edit::MatchTypeDraft::new_draft(config, "text"));
                }
            }
            Message::MatchTypesClose => {
                self.mt_dialog = false;
                self.mt_draft = None;
                self.mt_status = None;
                self.mt_test_result = None;
                self.mt_test.clear();
            }
            Message::MatchTypesSelect(selected) => {
                let draft = selected.and_then(|index| {
                    self.config
                        .as_ref()
                        .and_then(|config| config.match_types.get(index))
                        .map(|mt| match_types_edit::MatchTypeDraft::from_existing(index, mt))
                });
                self.mt_pick_set(draft);
            }
            Message::MtNew => {
                let draft = self
                    .config
                    .as_ref()
                    .map(|config| match_types_edit::MatchTypeDraft::new_draft(config, "text"));
                self.mt_draft = draft;
                self.mt_status = None;
                self.mt_test_result = None;
                self.mt_test.clear();
            }
            Message::MtLabel(value) => self.mt_edit_draft(|draft| draft.label = value),
            Message::MtLabelEn(value) => self.mt_edit_draft(|draft| draft.label_en = value),
            Message::MtKind(index) => {
                let kind = if index == 1 { "fileExt" } else { "text" };
                self.mt_edit_draft(|draft| {
                    draft.kind = kind.to_string();
                    if kind == "fileExt" {
                        draft.rules.clear();
                    } else if draft.rules.is_empty() {
                        draft.rules.push(("contains".to_string(), String::new()));
                    }
                });
            }
            Message::MtRuleOp(rule, op_index) => {
                let ops = ["equals", "prefix", "suffix", "contains"];
                self.mt_edit_draft(|draft| {
                    if let Some(slot) = draft.rules.get_mut(rule)
                        && let Some(op) = ops.get(op_index)
                    {
                        slot.0 = (*op).to_string();
                    }
                });
            }
            Message::MtRuleValue(rule, value) => {
                self.mt_edit_draft(|draft| {
                    if let Some(slot) = draft.rules.get_mut(rule) {
                        slot.1 = value;
                    }
                });
            }
            Message::MtRuleAdd => self.mt_edit_draft(|draft| {
                draft.rules.push(("contains".to_string(), String::new()));
            }),
            Message::MtRuleRemove(rule) => {
                self.mt_edit_draft(|draft| {
                    if draft.rules.len() > 1 {
                        draft.rules.remove(rule);
                    }
                });
            }
            Message::MtExts(value) => self.mt_edit_draft(|draft| draft.exts = value),
            Message::MtSave(with_behavior) => {
                let Some(draft) = self.mt_draft.clone() else {
                    return;
                };
                if let Err(reason) = match_types_edit::validate(&draft) {
                    self.mt_status = Some((reason, true));
                    return;
                }
                let (match_type, match_value) = {
                    if self.config.is_none() {
                        return;
                    }
                    // 以写回后的 kind/引用推导目标（fileExt → 后缀串；text → type: 引用）
                    let kind_is_file = draft.kind == "fileExt";
                    let value = if kind_is_file {
                        sa::normalize_exts(&draft.exts).join(",")
                    } else {
                        draft.type_ref()
                    };
                    (
                        if kind_is_file {
                            sa::MATCH_FILE_EXT
                        } else {
                            sa::MATCH_TEXT_TYPE
                        },
                        value,
                    )
                };
                let mut config_holder = self.config.clone();
                let Some(config) = config_holder.as_mut() else {
                    return;
                };
                match_types_edit::apply(config, &draft);
                self.config = config_holder;
                self.mt_status = None;

                if with_behavior {
                    // 「保存并创建专属行为」：建一个强绑定本类型的行为包
                    // （基础动作取当前覆盖集首个的基础动作，模板留空由用户后续在行为库补）
                    let base_action = sa::covering(&self.catalog, match_type, &match_value)
                        .first()
                        .map(|pack| self.catalog.base_action_of(&pack.id))
                        .unwrap_or_else(|| "run".to_string());
                    // 保存策略：行为包先入内存目录（下拉即刻可见）并入队，
                    // 「保存配置」时统一提交（此处不再直连后端）
                    let staged_id = draft.id.clone();
                    let applies = if match_type == sa::MATCH_TEXT_TYPE {
                        crate::models::BehaviorAppliesTo {
                            kind: sa::MATCH_TEXT_TYPE.to_string(),
                            value: if draft.kind == "text" {
                                Some("plain".to_string())
                            } else {
                                Some(match_value.clone())
                            },
                            exts: None,
                            is_default: false,
                        }
                    } else {
                        crate::models::BehaviorAppliesTo {
                            kind: sa::MATCH_FILE_EXT.to_string(),
                            value: None,
                            exts: Some(sa::normalize_exts(&match_value)),
                            is_default: false,
                        }
                    };
                    let pack = crate::models::BehaviorPack {
                        id: draft.id.clone(),
                        name: draft.label.clone(),
                        spec_version: 1,
                        applies_to: vec![applies],
                        entry: crate::models::BehaviorEntry {
                            kind: "builtin".to_string(),
                            action: Some(base_action),
                            ..Default::default()
                        },
                        bound_type_id: Some(draft.type_ref()),
                        source: Some("user".to_string()),
                        ..Default::default()
                    };
                    if let Some(slot) = self
                        .catalog
                        .user
                        .iter_mut()
                        .find(|pack| pack.id == staged_id)
                    {
                        *slot = pack.clone();
                    } else {
                        self.catalog.user.push(pack.clone());
                    }
                    self.pending
                        .push(save_pipeline::PendingChange::BehaviorCreate(pack));
                }
                // 重新载入草稿为「编辑既有」态（新建后 id 已落库）
                let index = self
                    .config
                    .as_ref()
                    .and_then(|config| config.match_types.iter().position(|mt| mt.id == draft.id));
                self.mt_pick_set(index.map(|index| {
                    match_types_edit::MatchTypeDraft::from_existing(
                        index,
                        &self.config.as_ref().unwrap().match_types[index],
                    )
                }));
            }
            Message::MtDelete => {
                let Some(draft) = self.mt_draft.clone() else {
                    return;
                };
                if draft.index == match_types_edit::NEW_INDEX {
                    return;
                }
                let mut config_holder = self.config.clone();
                let deleted = config_holder
                    .as_mut()
                    .and_then(|config| match_types_edit::delete(config, draft.index));
                let Some(type_id) = deleted else {
                    return;
                };
                self.config = config_holder;
                // 级联删同名专属行为包（bound_type_id 命中）：内存目录即时移除 +
                // 入队，由「保存配置」统一提交（保存策略：无即时副作用）。
                let type_ref = format!("type:{type_id}");
                let bound = self
                    .catalog
                    .user
                    .iter()
                    .find(|pack| pack.bound_type_id.as_deref() == Some(type_ref.as_str()))
                    .map(|pack| pack.id.clone());
                if let Some(behavior_id) = bound {
                    self.catalog.user.retain(|pack| pack.id != behavior_id);
                    self.pending
                        .push(save_pipeline::PendingChange::BehaviorDelete(behavior_id));
                }
                self.mt_draft = None;
            }
            Message::MtTest(content) => {
                self.mt_test = content;
            }
            Message::MtTestRun => {
                let Some(port) = self.port else {
                    return;
                };
                if self.mt_test.trim().is_empty() {
                    self.mt_test_result = Some(Err(i18n::t("2570")));
                    return;
                }
                // 编辑中快照：把草稿 apply 进克隆配置（未保存的类型也能参与匹配，
                // 复刻 Go 端 selectedActionTestRequest 的 snapshot-priority 语义）
                let Some(draft) = self.mt_draft.clone() else {
                    return;
                };
                let mut snapshot = self.config.clone().unwrap_or_default();
                match_types_edit::apply(&mut snapshot, &draft);
                let is_file = draft.kind == "fileExt";
                let match_types = snapshot.match_types.clone();
                let selected_action = Some(snapshot.selected_action.clone());
                let content = self.mt_test.clone();
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.test_selected_action(
                        &content,
                        is_file,
                        selected_action.as_ref(),
                        Some(&match_types),
                    );
                    Message::MtTestDone(match (response.success, response.value) {
                        (true, Some(value)) if value.matched => {
                            Ok(value.preview.unwrap_or_default())
                        }
                        (true, _) => Ok(String::new()),
                        (_, _) => Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status))),
                    })
                });
            }
            Message::MtTestDone(result) => {
                self.mt_test_result = Some(result);
            }
            // ---------------------------------------------------------- 行为库
            _ => {}
        }
    }
}
