//! misc —— `Shell::update` 的 杂项: 提示条 / 自定义热键 / 字体浏览 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 2026-10-08 批 W3b：**纯臂逻辑**抽为本文件的 `fn`（测试经子模块直测），
//! `handle_misc` 变薄壳。例外（不可脱离窗口测试，保持原样）：`Notice-Ok`
//! （`schedule_notice_clear` 含 `spawn_background` + sleep）、`FontBrowse`
//! （同步模态文件对话框）。

use super::super::*;

impl Shell {
    /// `misc` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_misc(
        &mut self,
        message: Message,
        context: &ComponentContext<Self>,
    ) {
        match message {
            Message::ClearNotice => self.clear_notice(),
            // 保存成功/失败都**停留在当前页**（旧版失败走模态弹窗、现场保留；此处以
            // 红色页脚提示等价承载），不再把用户踢到全页错误态。
            Message::Notice(result) => match result {
                Ok(text) => {
                    self.notice_error = false;
                    self.notice = Some(text);
                    self.schedule_notice_clear(context);
                }
                Err(reason) => self.notice_show_error(reason),
            },
            // ------------------------------------------------------------- 选中动作页
            Message::CustomHotkeyEdit(row) => self.custom_hotkey_edit(row),
            Message::CustomHotkeyEditClose => self.custom_hotkey_edit_close(),
            // ---------------------------------------------------------- 插件页
            Message::FontBrowse => {
                let selected = platform::file_dialog::pick_open_file(
                    &i18n::t("2504"),
                    platform::file_dialog::FONT_FILTER,
                );
                if let Some(path) = selected
                    && let Some(config) = self.config.as_mut()
                {
                    config.options.command_font.source_path = path.to_string_lossy().into_owned();
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------- 抽取的纯逻辑（可测）

    /// 错误提示条**不被自动清除**（红色常驻，用户须自行关闭）；仅普通提示可清。
    fn clear_notice(&mut self) {
        if !self.notice_error {
            self.notice = None;
        }
    }

    /// 错误分支：红条常驻（不走 2 秒自动清除）。
    fn notice_show_error(&mut self, reason: String) {
        self.notice_error = true;
        self.notice = Some(reason);
    }

    /// 编辑目标切换为 keymap id=1 的第 row 行（current_keymap_id 的 override）。
    fn custom_hotkey_edit(&mut self, row: usize) {
        let hotkey = self
            .config
            .as_ref()
            .and_then(|config| config.keymaps.iter().find(|km| km.id == 1))
            .and_then(|keymap| keymap.hotkeys.keys().nth(row).cloned());
        let Some(hotkey) = hotkey else {
            return;
        };
        self.hotkey_editor_row = Some(row);
        self.selected_hotkey = Some(hotkey);
        self.window_group_id = 0;
    }

    fn custom_hotkey_edit_close(&mut self) {
        self.hotkey_editor_row = None;
        self.selected_hotkey = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误提示条是**常驻**的：ClearNotice 不得清掉红色提示（防「报错一闪而过」）；
    /// 普通提示才可清。
    #[test]
    fn clear_notice_only_when_not_error() {
        let mut shell = Shell {
            notice: Some("错误信息".to_string()),
            notice_error: true,
            ..Shell::default()
        };
        shell.clear_notice();
        assert!(shell.notice.is_some(), "错误提示不得被 ClearNotice 清除");

        shell.notice_error = false;
        shell.clear_notice();
        assert!(shell.notice.is_none(), "普通提示应可被清除");
    }

    /// 错误分支置红条；Ok 分支置非红（Ok 的自动清除属 spawn 侧，不在本测试范围）。
    #[test]
    fn notice_error_branch_sets_sticky_notice() {
        let mut shell = Shell::default();
        shell.notice_show_error("保存失败".to_string());
        assert!(shell.notice_error);
        assert_eq!(shell.notice.as_deref(), Some("保存失败"));
    }

    /// 自定义热键编辑 = 按 row 取 keymap id=1 的第 row 个热键名 + 重置窗口分组；
    /// 越界 row 必须 no-op；Close 清编辑态。
    #[test]
    fn custom_hotkey_edit_maps_row_and_close_clears() {
        let keymap = crate::models::config::Keymap {
            id: 1,
            hotkeys: [
                ("*a".to_string(), Vec::new()),
                ("*b".to_string(), Vec::new()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let config = crate::models::config::Config {
            keymaps: vec![keymap],
            ..Default::default()
        };
        let mut shell = Shell {
            config: Some(config),
            ..Shell::default()
        };

        shell.custom_hotkey_edit(1);
        assert_eq!(shell.hotkey_editor_row, Some(1));
        assert_eq!(shell.selected_hotkey.as_deref(), Some("*b"));
        assert_eq!(shell.window_group_id, 0);

        shell.custom_hotkey_edit_close();
        assert!(shell.hotkey_editor_row.is_none());
        assert!(shell.selected_hotkey.is_none());

        // 越界 row：no-op（不得打开编辑器）
        shell.custom_hotkey_edit(99);
        assert!(shell.hotkey_editor_row.is_none());
    }
}
