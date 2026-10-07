//! guide —— `Shell::update` 的 使用指南文档就地编辑 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_guide`。

use super::super::*;

impl Shell {
    /// `guide` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_guide(
        &mut self,
        message: Message,
        _context: &ComponentContext<Self>,
    ) {
        match message {
            Message::GuideEditOpen => {
                // 🔴 WinUI TextBox 不认裸 LF 换行 (文档源全是 LF) —— 弹窗里整篇
                // 塌成一行 (2026-10-05 用户报障)。喂框前归一为 CRLF; 渲染侧
                // markdown::parse 自带归一, 编辑保存后的文本渲染不受影响。
                self.guide_edit_text = self.doc_md.replace("\r\n", "\n").replace('\n', "\r\n");
                self.guide_edit_open = true;
            }
            Message::GuideEditValue(value) => self.guide_edit_text = value,
            Message::GuideEditReset => {
                // 复刻旧 `OverviewEditWindow`：清空 = 恢复默认文档 —— 立即回落出厂
                // 站内 config_doc.md（读不到回退后端）。🔴 不得用「清空渲染态」代替：
                // 那会让指南页落进「暂不可用」降级态（2026-10-05 用户报障）。
                // 落盘统一走页脚「保存配置」（config 置空后，下次 assemble 走同一回退链）。
                self.guide_edit_text.clear();
                if let Some(config) = self.config.as_mut() {
                    config.overview_doc_md = String::new();
                }
                self.doc_md = self.load_default_doc();
                self.guide_edit_open = false;
            }
            Message::GuideEditClose => self.guide_edit_open = false,
            Message::GuideEditSave => {
                let text = std::mem::take(&mut self.guide_edit_text);
                // 「清空内容并保存可恢复默认文档」（i18n 111 承诺）：空文本 = 立即回落
                // 出厂文档（与 Reset 同链）；config 照常写空串 —— 落盘后下次 assemble
                // 走同一回退链，语义自洽。
                self.doc_md = if text.trim().is_empty() {
                    self.load_default_doc()
                } else {
                    text.clone()
                };
                self.guide_edit_open = false;
                // 「保存」= 应用到内存 config（对话框草稿 → 工作副本）；落盘统一走
                // 页脚「保存配置」（保存策略：唯一落盘入口）
                if let Some(config) = self.config.as_mut() {
                    config.overview_doc_md = text;
                }
            }
            // ---------------------------------------------------------- 自定义热键动作编辑
            _ => {}
        }
    }
}
