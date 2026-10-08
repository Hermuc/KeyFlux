//! bh —— `Shell::update` 的 动作库 (behavior) 编辑 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 2026-10-08 批 W3a：**纯臂逻辑**抽为本文件的 `fn`（私有，测试经子模块直测），
//! `handle_bh` 变薄壳（match → 方法）；单行 `bh_edit_draft` 臂保持原样不抽。
//! 含 `context.spawn_background` 的臂不存在于本域 —— 18 臂全部可脱离窗口测试。

use super::super::*;

impl Shell {
    /// `bh` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_bh(
        &mut self,
        message: Message,
        _context: &ComponentContext<Self>,
    ) {
        match message {
            Message::BehaviorsOpen => self.bh_open_dialog(),
            Message::BehaviorsClose => self.bh_close_dialog(),
            Message::BhSelect(selected) => self.bh_select(selected),
            Message::BhNew => self.bh_new_draft(),
            Message::BhName(value) => self.bh_edit_draft(|draft| draft.name = value),
            Message::BhId(value) => self.bh_edit_draft(|draft| draft.id = value),
            Message::BhDescription(value) => self.bh_edit_draft(|draft| draft.description = value),
            Message::BhAppliesKind(row, index) => self.bh_applies_set_kind(row, index),
            Message::BhAppliesValue(row, value) => self.bh_applies_set_value(row, value),
            Message::BhAppliesDefault(row, value) => self.bh_applies_set_default(row, value),
            Message::BhAppliesAdd => self.bh_applies_add(),
            Message::BhAppliesRemove(row) => self.bh_applies_remove(row),
            Message::BhBaseAction(index) => self.bh_set_base_action(index),
            Message::BhTemplate(value) => self.bh_edit_draft(|draft| draft.template = value),
            Message::BhWorkingDir(value) => self.bh_edit_draft(|draft| draft.working_dir = value),
            Message::BhSave => self.bh_save(),
            Message::BhDelete => self.bh_delete(),
            Message::BehaviorsLoaded(result) => self.bh_catalog_loaded(result),
            _ => {}
        }
    }

    // ------------------------------------------------------- 抽取的纯逻辑（可测）

    fn bh_open_dialog(&mut self) {
        self.bh_dialog = true;
        self.bh_status = None;
        self.bh_selected = None;
        self.bh_draft = None;
    }

    fn bh_close_dialog(&mut self) {
        self.bh_dialog = false;
        self.bh_selected = None;
        self.bh_draft = None;
        self.bh_status = None;
    }

    fn bh_select(&mut self, selected: Option<usize>) {
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

    fn bh_new_draft(&mut self) {
        self.bh_draft = Some(behaviors_edit::BehaviorDraft::new_draft());
        self.bh_selected = None;
        self.bh_status = None;
    }

    fn bh_applies_set_kind(&mut self, row: usize, index: usize) {
        let kind = if index == 1 { "fileExt" } else { "textType" };
        self.bh_edit_draft(|draft| {
            if let Some(slot) = draft.applies.get_mut(row) {
                slot.kind = kind.to_string();
            }
        });
    }

    fn bh_applies_set_value(&mut self, row: usize, value: String) {
        self.bh_edit_draft(|draft| {
            if let Some(slot) = draft.applies.get_mut(row) {
                slot.value = value;
            }
        });
    }

    fn bh_applies_set_default(&mut self, row: usize, value: bool) {
        self.bh_edit_draft(|draft| {
            if let Some(slot) = draft.applies.get_mut(row) {
                slot.is_default = value;
            }
        });
    }

    fn bh_applies_add(&mut self) {
        self.bh_edit_draft(|draft| {
            draft.applies.push(behaviors_edit::AppliesDraft {
                kind: "textType".to_string(),
                ..Default::default()
            });
        });
    }

    fn bh_applies_remove(&mut self, row: usize) {
        self.bh_edit_draft(|draft| {
            if draft.applies.len() > 1 {
                draft.applies.remove(row);
            }
        });
    }

    fn bh_set_base_action(&mut self, index: usize) {
        let options = behaviors_edit::base_action_options(&self.catalog);
        self.bh_edit_draft(|draft| {
            if let Some(action) = options.get(index) {
                draft.base_action = action.clone();
            }
        });
    }

    /// 保存 = 应用到**内存目录**并入队；行为库落盘 + 引擎重启由「保存配置」统一提交。
    pub(in crate::app) fn bh_save(&mut self) {
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

    /// 删除 = 内存目录即时移除 + 入队，由「保存配置」统一提交
    /// （本会话内建又删会在队列里相互抵消，见 PendingQueue::push）。
    pub(in crate::app) fn bh_delete(&mut self) {
        let Some(draft) = self.bh_draft.clone() else {
            return;
        };
        let id = draft.id.clone();
        self.catalog.user.retain(|pack| pack.id != id);
        self.pending
            .push(save_pipeline::PendingChange::BehaviorDelete(id));
        self.bh_status = Some((i18n::t("2595"), false));
    }

    fn bh_catalog_loaded(&mut self, result: Result<Box<sa::SaCatalog>, String>) {
        match result {
            Ok(catalog) => {
                self.catalog = *catalog;
                // 目录到达后重置两卡选中（covering 列表会变）
            }
            Err(_reason) => {
                // 目录拉取失败：页面仍可用（下拉为空）；对话框内失败经 bh_status 呈现
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::behaviors_edit;
    use super::*;

    /// 空白 Shell + 一条通过 validate 的合法新草稿
    /// （validate 契约：id 字母开头且限 `[A-Za-z0-9_-]`、name 非空、
    ///   applies ≥1 条且 value 非空、base_action 非空）。
    fn fresh_draft(id: &str) -> behaviors_edit::BehaviorDraft {
        behaviors_edit::BehaviorDraft {
            index: behaviors_edit::NEW_INDEX,
            id: id.to_string(),
            name: format!("名称 {id}"),
            applies: vec![behaviors_edit::AppliesDraft {
                kind: "textType".to_string(),
                value: "https://example.com".to_string(),
                ..Default::default()
            }],
            base_action: "copy".to_string(),
            ..Default::default()
        }
    }

    /// 批 W3a：对话框开/关是四字段的成对联动（开 = 进入干净编辑态，关 = 全清）。
    #[test]
    fn bh_dialog_open_close_roundtrip() {
        let mut shell = Shell {
            bh_status: Some(("旧".to_string(), true)),
            bh_selected: Some(2),
            bh_draft: Some(fresh_draft("stale")),
            ..Shell::default()
        };
        shell.bh_open_dialog();
        assert!(shell.bh_dialog);
        assert!(shell.bh_status.is_none());
        assert!(shell.bh_selected.is_none());
        assert!(shell.bh_draft.is_none());

        shell.bh_close_dialog();
        assert!(!shell.bh_dialog);
        assert!(shell.bh_draft.is_none());
    }

    /// 新建 = 干净草稿 + 清选中/状态条；草稿编辑在无草稿时必须是无害 no-op。
    #[test]
    fn bh_new_draft_and_edit_noop_without_draft() {
        let mut shell = Shell::default();
        shell.bh_new_draft();
        assert!(shell.bh_selected.is_none());
        assert!(shell.bh_status.is_none());
        let draft = shell.bh_draft.clone().expect("new_draft 应置草稿");
        assert_eq!(draft.index, behaviors_edit::NEW_INDEX);

        // 无草稿时编辑闭包不应被调用（也不得 panic）
        let mut empty = Shell::default();
        empty.bh_edit_draft(|draft| draft.name = "不应落盘".to_string());
        assert!(empty.bh_draft.is_none());
    }

    /// applies 增删契约：Add 追加 textType 槽；Remove 在**仅剩 1 条时必须拒删**
    /// （草稿契约 = 生效前提 ≥1 条）。
    #[test]
    fn bh_applies_add_and_remove_keeps_at_least_one() {
        let mut shell = Shell {
            bh_draft: Some(fresh_draft("pkg")),
            ..Shell::default()
        };

        shell.bh_applies_add();
        assert_eq!(shell.bh_draft.as_ref().unwrap().applies.len(), 2);

        shell.bh_applies_remove(0);
        assert_eq!(shell.bh_draft.as_ref().unwrap().applies.len(), 1);

        shell.bh_applies_remove(0);
        assert_eq!(
            shell.bh_draft.as_ref().unwrap().applies.len(),
            1,
            "仅剩 1 条时 Remove 必须是 no-op"
        );
    }

    /// 内置包只读（1103_only）+ 校验失败都不得触碰内存目录与队列。
    #[test]
    fn bh_save_rejects_builtin_and_invalid_without_side_effects() {
        let builtin = crate::models::BehaviorPack {
            id: "builtin_x".to_string(),
            ..Default::default()
        };
        let mut shell = Shell {
            catalog: sa::SaCatalog {
                builtin: vec![builtin],
                user: Vec::new(),
            },
            ..Shell::default()
        };

        // ① 编辑既有内置包（index != NEW_INDEX 且 id 命中 builtin）。
        //    草稿其余字段必须**满足 validate** —— 否则红灯来自 validate 而非内置拦截，
        //    测试就测空了（负向对照实测揭示过这一点：applies 为空时红来自 1104）。
        shell.bh_draft = Some(behaviors_edit::BehaviorDraft {
            index: 0,
            id: "builtin_x".to_string(),
            name: "改内置".to_string(),
            applies: vec![behaviors_edit::AppliesDraft {
                kind: "textType".to_string(),
                value: "https://example.com".to_string(),
                ..Default::default()
            }],
            base_action: "copy".to_string(),
            ..Default::default()
        });
        shell.bh_save();
        assert!(shell.bh_status.as_ref().is_some_and(|(_, is_err)| *is_err));
        assert!(shell.catalog.user.is_empty());
        assert!(shell.pending.is_empty());

        // ② 合法新草稿但 id 非法（空 id → validate 拦截）
        let mut invalid = fresh_draft("pkg");
        invalid.id = String::new();
        shell.bh_draft = Some(invalid);
        shell.bh_save();
        assert!(shell.bh_status.as_ref().is_some_and(|(_, is_err)| *is_err));
        assert!(shell.catalog.user.is_empty());
        assert!(shell.pending.is_empty());
    }

    /// 合法新建 → 写内存目录 + 入队 BehaviorCreate；index 指向 user 槽再存 →
    /// Update 分支原地更新（不重复追加）。
    #[test]
    fn bh_save_creates_then_updates_in_place() {
        let mut shell = Shell {
            bh_draft: Some(fresh_draft("my_pack")),
            ..Shell::default()
        };

        shell.bh_save();
        assert_eq!(shell.catalog.user.len(), 1);
        assert_eq!(shell.catalog.user[0].id, "my_pack");
        assert!(!shell.pending.is_empty());
        assert!(shell.bh_status.as_ref().is_some_and(|(_, is_err)| !*is_err));

        // 同 id、index 指向 user 槽（0）⇒ 非新建 ⇒ Update 分支
        let mut draft = fresh_draft("my_pack");
        draft.index = 0;
        shell.bh_draft = Some(draft);
        shell.bh_save();
        assert_eq!(shell.catalog.user.len(), 1, "Update 分支应原地更新而非追加");
    }

    /// 删除 = 内存目录 retain 掉目标 + 入队 BehaviorDelete。
    /// ⚠ 不可「先 save 再 delete」构造场景 —— 本会话内建又删会在队列里**相互抵消**
    /// （PendingQueue::push 的相消语义），故直接预置 user 包。
    #[test]
    fn bh_delete_removes_from_catalog_and_enqueues() {
        let pack = crate::models::BehaviorPack {
            id: "doomed".to_string(),
            ..Default::default()
        };
        let mut draft = fresh_draft("doomed");
        draft.index = 0;
        let mut shell = Shell {
            catalog: sa::SaCatalog {
                builtin: Vec::new(),
                user: vec![pack],
            },
            bh_draft: Some(draft),
            ..Shell::default()
        };

        shell.bh_delete();
        assert!(shell.catalog.user.is_empty());
        assert!(!shell.pending.is_empty(), "非「建删相消」路径应真实入队");
        assert!(shell.bh_status.as_ref().is_some_and(|(_, is_err)| !*is_err));

        // 无草稿时删除是 no-op
        let mut empty = Shell::default();
        empty.bh_delete();
        assert!(empty.pending.is_empty());
    }

    /// 目录到达 = 整体替换（Ok）；拉取失败 = 保持现状（页面仍可用）。
    #[test]
    fn bh_catalog_loaded_replaces_on_ok_and_keeps_on_err() {
        let mut shell = Shell::default();
        let mut catalog = sa::SaCatalog::default();
        catalog.user.push(crate::models::BehaviorPack::default());
        shell.bh_catalog_loaded(Ok(Box::new(catalog)));
        assert_eq!(shell.catalog.user.len(), 1);

        shell.bh_catalog_loaded(Err("网络失败".to_string()));
        assert_eq!(shell.catalog.user.len(), 1, "Err 分支不得清空现有目录");
    }
}
