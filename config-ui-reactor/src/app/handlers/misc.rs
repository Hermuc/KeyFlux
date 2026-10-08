//! misc —— `Shell::update` 的 杂项: 提示条 / 自定义热键 / 字体浏览 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_misc`。

use super::super::*;

impl Shell {
    /// `misc` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_misc(
        &mut self,
        message: Message,
        context: &ComponentContext<Self>,
    ) {
        match message {
            Message::ClearNotice => {
                if !self.notice_error {
                    self.notice = None;
                }
            }
            // 保存成功/失败都**停留在当前页**（旧版失败走模态弹窗、现场保留；此处以
            // 红色页脚提示等价承载），不再把用户踢到全页错误态。
            Message::Notice(result) => match result {
                Ok(text) => {
                    self.notice_error = false;
                    self.notice = Some(text);
                    self.schedule_notice_clear(context);
                }
                Err(reason) => {
                    self.notice_error = true;
                    self.notice = Some(reason);
                }
            },
            // ------------------------------------------------------------- 选中动作页
            Message::CustomHotkeyEdit(row) => {
                // 编辑目标切换为 keymap id=1 的第 row 行（current_keymap_id 的 override）
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
            Message::CustomHotkeyEditClose => {
                self.hotkey_editor_row = None;
                self.selected_hotkey = None;
            }
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
}
