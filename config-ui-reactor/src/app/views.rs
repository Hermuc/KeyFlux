//! `app` 的 views：**外壳 + 页面路由**（把 Shell 状态 + ViewContext 翻译成 View）。
//!
//! 2026-10-07 自原 `app.rs` 的 `impl Shell` 拆出；2026-10-08 再把 7 个页面装配方法
//! 迁到 [`crate::app::pages`]（模块化审查 §3.2 / 问题 #4：本文件曾达 1801 行、
//! 单个 `impl Shell`、22 个方法承担 7 个页面，其中 `settings_page` 单函数长达数百行，
//! 体量超过 `ui/` 下任何一个完整文件；2026-10-09 又按页面内分区边界把各超长装配函数拆小）。
//!
//! 本文件只留**外壳**（`pane_footer` / 三态内容区 `content`）与**路由**
//! （`page_view` / `current_title` / `placeholder_page`）；页面本身见 `app::pages`。
//! 迁移为纯代码搬移，方法体逐字未改 —— 由脚本执行并以「方法集合 + 逐方法 SHA256」
//! 双重对账（与 2026-10-07 拆 `update` 时同一手法）。

use super::*;

impl Shell {
    /// 侧栏底部：分隔线 + 保存提示（成功绿 / 失败红）+ 保存按钮（旧 `DockPanel.Dock="Bottom"` 区）。
    pub(in crate::app) fn pane_footer(&self, context: &mut ViewContext<Self>) -> View {
        // 紧凑窄轨（浮层未展开）：只放得下图标按钮 —— 完整页脚（分隔线/提示/文字按钮）
        // 在 48px 轨内会被裁成窄条（2026-09-29 实测）。
        if !self.pane_overlay_open {
            return Button::new()
                .on_click(context.message(Message::Save))
                .horizontal_alignment(HorizontalAlignment::Center)
                .margin(theme::pad_md())
                .content(FontIcon::new().glyph("\u{E74E}")); // Save
        }
        let notice: View = match &self.notice {
            Some(text) => TextBlock::new()
                .text(text.clone())
                .font_size(theme::FONT_CAPTION)
                .foreground(if self.notice_error {
                    theme::solid(theme::ERROR_CRIMSON)
                } else {
                    theme::solid(theme::MUTED_GREEN)
                })
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            None => View::empty(),
        };

        StackPanel::new()
            .spacing(8.0)
            .margin(theme::pad_md())
            .children((
                Border::new()
                    .height(1.0)
                    .background(theme::border_faint())
                    .content(TextBlock::new().text("")),
                notice,
                Button::new()
                    .on_click(context.message(Message::Save))
                    .content(
                        TextBlock::new()
                            .text(i18n::t("507"))
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ),
            ))
    }

    /// 内容区三态互斥：加载中 / 错误 / 页面。
    pub(in crate::app) fn content(&self, context: &mut ViewContext<Self>) -> View {
        if self.loading {
            return StackPanel::new()
                .spacing(14.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    ProgressRing::new().is_active(true).width(48.0).height(48.0),
                    TextBlock::new()
                        .text(i18n::t("917"))
                        .foreground(theme::stone_gray())
                        .horizontal_alignment(HorizontalAlignment::Center),
                ));
        }

        if let Some(error) = &self.error {
            return StackPanel::new()
                .spacing(12.0)
                .max_width(560.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    // 旧错误页的 42px ⚠ 图标（`MainWindow.axaml:208-212`）
                    TextBlock::new()
                        .text("⚠")
                        .font_size(42.0)
                        .horizontal_alignment(HorizontalAlignment::Center),
                    TextBlock::new()
                        .text(i18n::t("919"))
                        .font_size(theme::FONT_SUBTITLE)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::near_black())
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(error.clone())
                        .foreground(theme::stone_gray())
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_wrapping(TextWrapping::Wrap),
                    Button::new()
                        .on_click(context.message(Message::Retry))
                        .content(i18n::t("920")),
                ));
        }

        self.page_view(context)
    }

    /// 当前页面标题（keymap 页用配置里的 name/hotkey）。
    pub(in crate::app) fn current_title(&self) -> String {
        match self.nav.get(self.page_index) {
            Some(entry) => {
                if entry.kind.title().is_empty() {
                    entry.label.clone()
                } else {
                    entry.kind.title()
                }
            }
            None => String::new(),
        }
    }

    pub(in crate::app) fn page_view(&self, context: &mut ViewContext<Self>) -> View {
        let Some(entry) = self.nav.get(self.page_index) else {
            return TextBlock::new().text("（无导航项）").into();
        };

        // 使用指南：消费 services::markdown 的块模型 → 原生控件（含链接/图片）。
        if entry.kind == PageKind::Guide {
            return self.guide_view(context);
        }

        // 插件页：统一插件卡（内置 QuickSwitch + 用户插件）
        if entry.kind == PageKind::Plugins {
            return self.plugins_page(context);
        }

        // 选项页（keymap id=4）：方案卡 + 手风琴分区
        if entry.kind == PageKind::Settings {
            return self.settings_page(context);
        }

        let hint = match entry.kind {
            PageKind::SelectedAction => {
                return self.selected_action_page(context);
            }
            PageKind::Plugins => "内置 QuickSwitch 卡 + 用户插件卡 + zip 导入 + 市场入口",
            PageKind::Settings => "快捷键方案 / 外观材质 / 语言 / 路径变量 / 其他设置",
            // 缩写页与矩阵页共享同一编辑核心（旧 `KeymapEditorCore`）：左侧换成 chips + 命令框。
            PageKind::Abbr(id) => {
                return self.abbr_page(context, id);
            }
            PageKind::Keymap(id) => {
                return self.keymap_page(context, id);
            }
            PageKind::Guide => unreachable!("Guide 已提前返回"),
        };
        self.placeholder_page(hint, &self.current_title())
    }

    pub(in crate::app) fn placeholder_page(&self, hint: &str, title: &str) -> View {
        let keymap_count = self
            .config
            .as_ref()
            .map(|config| config.keymaps.len())
            .unwrap_or(0);

        StackPanel::new()
            .spacing(10.0)
            .margin(theme::pad_lg())
            .children((
                TextBlock::new()
                    .text(title.to_string())
                    .font_size(28.0)
                    .font_weight(FontWeight::BOLD)
                    .foreground(theme::near_black()),
                TextBlock::new()
                    .text(hint.to_string())
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(format!("后端已连接 · keymap {keymap_count} 个"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::SLATE_GRAY)),
                Border::new()
                    .padding(theme::pad_md())
                    .background(theme::card_background())
                    .border_brush(theme::card_stroke())
                    .border_thickness(theme::hairline())
                    .corner_radius(theme::radius_md())
                    .content(
                        TextBlock::new()
                            .text("Phase 3 进行中：本页内容待迁移。")
                            .opacity(0.75),
                    ),
            ))
    }
}
