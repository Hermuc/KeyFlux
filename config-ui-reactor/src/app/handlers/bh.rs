//! bh —— `Shell::update` 的 动作库 (behavior) 编辑 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_bh`。

use super::super::*;

impl Shell {
    /// `bh` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_bh(
        &mut self,
        message: Message,
        _context: &ComponentContext<Self>,
    ) {
        match message {
            Message::BehaviorsOpen => {
                self.bh_dialog = true;
                self.bh_status = None;
                self.bh_selected = None;
                self.bh_draft = None;
            }
            Message::BehaviorsClose => {
                self.bh_dialog = false;
                self.bh_selected = None;
                self.bh_draft = None;
                self.bh_status = None;
            }
            Message::BhSelect(selected) => {
                self.bh_selected = selected;
                let merged: Vec<&crate::models::BehaviorPack> = self.catalog.packs().collect();
                self.bh_draft = selected.and_then(|index| {
                    merged.get(index).map(|pack| {
                        let mut draft = behaviors_edit::BehaviorDraft::from_pack(pack);
                        draft.index = index;
                        draft
                    })
                });
                self.bh_status = None;
            }
            Message::BhNew => {
                self.bh_draft = Some(behaviors_edit::BehaviorDraft::new_draft());
                self.bh_selected = None;
                self.bh_status = None;
            }
            Message::BhName(value) => self.bh_edit_draft(|draft| draft.name = value),
            Message::BhId(value) => self.bh_edit_draft(|draft| draft.id = value),
            Message::BhDescription(value) => self.bh_edit_draft(|draft| draft.description = value),
            Message::BhAppliesKind(row, index) => {
                let kind = if index == 1 { "fileExt" } else { "textType" };
                self.bh_edit_draft(|draft| {
                    if let Some(slot) = draft.applies.get_mut(row) {
                        slot.kind = kind.to_string();
                    }
                });
            }
            Message::BhAppliesValue(row, value) => {
                self.bh_edit_draft(|draft| {
                    if let Some(slot) = draft.applies.get_mut(row) {
                        slot.value = value;
                    }
                });
            }
            Message::BhAppliesDefault(row, value) => {
                self.bh_edit_draft(|draft| {
                    if let Some(slot) = draft.applies.get_mut(row) {
                        slot.is_default = value;
                    }
                });
            }
            Message::BhAppliesAdd => self.bh_edit_draft(|draft| {
                draft.applies.push(behaviors_edit::AppliesDraft {
                    kind: "textType".to_string(),
                    ..Default::default()
                });
            }),
            Message::BhAppliesRemove(row) => self.bh_edit_draft(|draft| {
                if draft.applies.len() > 1 {
                    draft.applies.remove(row);
                }
            }),
            Message::BhBaseAction(index) => {
                let options = behaviors_edit::base_action_options(&self.catalog);
                self.bh_edit_draft(|draft| {
                    if let Some(action) = options.get(index) {
                        draft.base_action = action.clone();
                    }
                });
            }
            Message::BhTemplate(value) => self.bh_edit_draft(|draft| draft.template = value),
            Message::BhWorkingDir(value) => self.bh_edit_draft(|draft| draft.working_dir = value),
            Message::BhSave => {
                let Some(draft) = self.bh_draft.clone() else {
                    return;
                };
                let is_new = draft.index == behaviors_edit::NEW_INDEX;
                if !is_new {
                    // 内置包只读（1103_only；后端也会 404 拒绝）
                    let is_builtin = self.catalog.builtin.iter().any(|pack| pack.id == draft.id);
                    if is_builtin {
                        self.bh_status = Some((i18n::t("1103_only"), true));
                        return;
                    }
                }
                if let Err(reason) = behaviors_edit::validate(&draft) {
                    self.bh_status = Some((reason, true));
                    return;
                }
                // 保存策略：「保存」= 应用到**内存目录**（下拉即刻反映）并入队；
                // 行为库落盘 + 引擎重启由「保存配置」统一提交。
                let pack = draft.to_pack();
                let id = pack.id.clone();
                if let Some(slot) = self.catalog.user.iter_mut().find(|pack| pack.id == id) {
                    *slot = pack.clone();
                } else {
                    self.catalog.user.push(pack.clone());
                }
                let change = if is_new {
                    save_pipeline::PendingChange::BehaviorCreate(pack)
                } else {
                    save_pipeline::PendingChange::BehaviorUpdate { id, pack }
                };
                self.pending.push(change);
                self.bh_status = Some((i18n::t("2595"), false));
            }
            Message::BhDelete => {
                let Some(draft) = self.bh_draft.clone() else {
                    return;
                };
                // 保存策略：内存目录即时移除 + 入队，由「保存配置」统一提交
                // （本会话内建又删会在队列里相互抵消，见 PendingQueue::push）。
                let id = draft.id.clone();
                self.catalog.user.retain(|pack| pack.id != id);
                self.pending
                    .push(save_pipeline::PendingChange::BehaviorDelete(id));
                self.bh_status = Some((i18n::t("2595"), false));
            }
            // ---------------------------------------------------------- 指南编辑
            Message::BehaviorsLoaded(result) => match result {
                Ok(catalog) => {
                    self.catalog = *catalog;
                    // 目录到达后重置两卡选中（covering 列表会变）
                }
                Err(_reason) => {
                    // 目录拉取失败：页面仍可用（下拉为空）；对话框内失败经 bh_status 呈现
                }
            },
            _ => {}
        }
    }
}
