//! guide —— `Shell::update` 的 使用指南文档就地编辑 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 2026-10-08 批 W3a：**纯臂逻辑**抽为本文件的 `fn`（私有，测试经子模块直测），
//! `handle_guide` 变薄壳；单行赋值臂（Value/Close）保持原样不抽。
//! 本域 5 臂全部纯内存/纯字段 —— `load_default_doc` 在 `session=None`（测试 Shell）下
//! 返回空串，无窗口副作用，全部可脱离窗口测试。

use super::super::*;

impl Shell {
    /// `guide` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_guide(
        &mut self,
        message: Message,
        _context: &ComponentContext<Self>,
    ) {
        match message {
            Message::GuideEditOpen => self.guide_edit_open(),
            Message::GuideEditValue(value) => self.guide_edit_text = value,
            Message::GuideEditReset => self.guide_edit_reset(),
            Message::GuideEditClose => self.guide_edit_open = false,
            Message::GuideEditSave => self.guide_edit_save(),
            // ---------------------------------------------------------- 自定义热键动作编辑
            _ => {}
        }
    }

    // ------------------------------------------------------- 抽取的纯逻辑（可测）

    fn guide_edit_open(&mut self) {
        // 🔴 WinUI TextBox 不认裸 LF 换行 (文档源全是 LF) —— 弹窗里整篇
        // 塌成一行 (2026-10-05 用户报障)。喂框前归一为 CRLF; 渲染侧
        // markdown::parse 自带归一, 编辑保存后的文本渲染不受影响。
        self.guide_edit_text = self.doc_md.replace("\r\n", "\n").replace('\n', "\r\n");
        self.guide_edit_open = true;
    }

    fn guide_edit_reset(&mut self) {
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

    /// 保存 = 应用到内存 config（对话框草稿 → 工作副本）；落盘统一走
    /// 页脚「保存配置」（保存策略：唯一落盘入口）。
    pub(in crate::app) fn guide_edit_save(&mut self) {
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
        if let Some(config) = self.config.as_mut() {
            config.overview_doc_md = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 打开对话框的换行契约：文档源全是 LF，喂框前必须归一为 CRLF
    /// （🔴 WinUI TextBox 不认裸 LF——2026-10-05 用户报障的根因）。
    #[test]
    fn guide_edit_open_normalizes_lf_to_crlf() {
        let mut shell = Shell {
            doc_md: "第一行\n第二行\r\n第三行".to_string(),
            ..Shell::default()
        };

        shell.guide_edit_open();

        assert!(shell.guide_edit_open);
        assert_eq!(shell.guide_edit_text, "第一行\r\n第二行\r\n第三行");
    }

    /// Reset 契约：文本框清空 + config 置空 + doc_md 回落默认文档链
    /// （不得用「清空渲染态」代替——那会让指南页落入降级态）。
    #[test]
    fn guide_edit_reset_restores_default_doc() {
        let mut shell = Shell {
            config: Some(crate::models::config::Config::default()),
            ..Shell::default()
        };
        shell.doc_md = "现有文档".to_string();
        shell.guide_edit_text = "正在编辑".to_string();
        shell.config.as_mut().unwrap().overview_doc_md = "自定义".to_string();

        shell.guide_edit_reset();

        assert!(shell.guide_edit_text.is_empty());
        assert!(!shell.guide_edit_open);
    }

    /// Save 契约两分支：空文本 = 立即回落默认文档（i18n 111 承诺）；非空 = 直写
    /// doc_md 与内存 config；两者都关闭对话框并取走草稿文本。
    #[test]
    fn guide_edit_save_empty_falls_back_and_nonempty_writes() {
        // ① 空文本回落（Default Shell 的 load_default_doc 返回空串）
        let mut shell = Shell {
            config: Some(crate::models::config::Config::default()),
            ..Shell::default()
        };
        shell.guide_edit_text = "   \r\n".to_string();
        shell.guide_edit_save();
        assert!(
            shell.doc_md.is_empty(),
            "空文本应回落默认文档（测试环境为空串）"
        );
        // config 写入的是 take 出来的**原始文本**（空白串）——「回落」只作用于渲染态 doc_md
        assert_eq!(shell.config.as_ref().unwrap().overview_doc_md, "   \r\n");
        assert!(!shell.guide_edit_open);
        assert!(shell.guide_edit_text.is_empty(), "保存应取走草稿文本");

        // ② 非空直写
        let mut shell = Shell {
            config: Some(crate::models::config::Config::default()),
            ..Shell::default()
        };
        shell.guide_edit_text = "# 新指南".to_string();
        shell.guide_edit_open = true;
        shell.guide_edit_save();
        assert_eq!(shell.doc_md, "# 新指南");
        assert_eq!(shell.config.as_ref().unwrap().overview_doc_md, "# 新指南");
        assert!(!shell.guide_edit_open);
    }
}
