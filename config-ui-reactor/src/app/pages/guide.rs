//! `app::pages::guide` —— 使用指南页（`services::markdown` 块模型 -> 原生控件）。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。

use super::super::*;

impl Shell {
    /// 指南页：`config.overviewDocMd` 优先，为空时已在后台拉取 `/config_doc.md`。
    ///
    /// 底部有「编辑指南」入口（复刻旧 `EditZoneHint` 虚线编辑区 → `OverviewEditWindow`）。
    pub(in crate::app) fn guide_view(&self, context: &mut ViewContext<Self>) -> View {
        let Some(port) = self.port else {
            return TextBlock::new().text("后端未连接").into();
        };

        let body: View = if self.doc_md.trim().is_empty() {
            // 文档不可达空态：内置快速上手引导（复刻旧 `HomePageView.axaml:42-56` 的
            // 932 标题 + 934-938 文案，不再是一行硬编码中文）
            ScrollViewer::new().content(
                StackPanel::new()
                    .spacing(10.0)
                    .max_width(720.0)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .margin(Thickness::new(28.0, 20.0, 28.0, 28.0))
                    .children((
                        TextBlock::new()
                            .text(self.current_title())
                            .font_size(28.0)
                            .font_weight(FontWeight::BOLD)
                            .foreground(theme::near_black()),
                        TextBlock::new()
                            .text(i18n::t("932"))
                            .font_size(theme::FONT_CARD_TITLE)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .foreground(theme::near_black()),
                        TextBlock::new()
                            .text(i18n::t("934"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("935"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("936"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("937"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("938"))
                            .foreground(theme::stone_gray())
                            .text_wrapping(TextWrapping::Wrap),
                        Self::guide_edit_entry(context),
                    )),
            )
        } else {
            let blocks = markdown::parse(&self.doc_md);
            let rendered = markdown_view::render(&blocks, port);
            let content: View = StackPanel::new()
                .spacing(10.0)
                .margin(Thickness::new(28.0, 20.0, 28.0, 28.0))
                .children((
                    TextBlock::new()
                        .text(self.current_title())
                        .font_size(28.0)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::near_black()),
                    rendered,
                    // 页脚来源说明（旧 `HomePageView.axaml:33-34` 的 931，WarmSilver 12px）
                    TextBlock::new()
                        .text(i18n::t("931"))
                        .font_size(theme::FONT_CAPTION)
                        .foreground(theme::stone_gray())
                        .text_wrapping(TextWrapping::Wrap),
                    Self::guide_edit_entry(context),
                ));
            // 文档较长 ⇒ 纵向滚动（Fluent：内容区可滚动，页面不整体滚动）
            ScrollViewer::new().content(content)
        };

        body
    }
}
