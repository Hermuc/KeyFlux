//! `app::pages::abbr` —— 缩写页（与矩阵页共享同一编辑核心）。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。

use super::super::*;

impl Shell {
    /// 缩写页（id 2/3）：页头 + chips 网格 + 命令框 + 动作编辑面板 + 右侧备注汇总。
    pub(in crate::app) fn abbr_page(
        &self,
        context: &mut ViewContext<Self>,
        keymap_id: i32,
    ) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let Some(keymap) = config.keymaps.iter().find(|km| km.id == keymap_id) else {
            return TextBlock::new()
                .text(format!("未找到 keymap id={keymap_id}"))
                .foreground(theme::stone_gray())
                .into();
        };

        let disabled = keymap::disabled_keys(config);
        let chips = abbr::build_chips(keymap, &disabled, self.selected_hotkey.as_deref());
        let chips_area: View = if chips.is_empty() {
            abbr_view::empty_hint("（暂无缩写条目：在下方命令框输入内容并回车即可新增）")
        } else {
            // 统一格宽：依最长标签自适应（`VariableSizedWrapGrid` 的硬性要求）
            let item_width = abbr::chip_item_width(chips.iter().map(|chip| chip.label.as_str()));
            abbr_view::chip_grid(&chips, item_width, |chip| {
                context.message(Message::SelectKey(chip.hotkey))
            })
        };
        let command: View = abbr_view::command_box(
            &self.cmd_text,
            &i18n::t("406"),
            context.callback(|value: String| Message::CmdText(value)),
        );

        let header: View = keymap_view::page_header(
            &keymap::header_title(keymap),
            keymap::parent_info(keymap, config).as_deref(),
        );

        // 左列四段式：页头 / chips / 命令框 / 动作面板（STAR 行 ⇒ 面板有界可滚）
        let left: View = Border::new().grid_column(0).content(
            Grid::new()
                .rows([
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::STAR,
                ])
                .margin(Thickness::new(24.0, 20.0, 12.0, 28.0))
                .children((
                    Border::new().grid_row(0).content(header),
                    Border::new().grid_row(1).content(chips_area),
                    Border::new()
                        .grid_row(2)
                        .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                        .content(command),
                    Border::new()
                        .grid_row(3)
                        .margin(Thickness::new(0.0, 18.0, 0.0, 0.0))
                        .content(ScrollViewer::new().content(self.action_editor_panel(context))),
                )),
        );

        // 缩写页备注用 `format_space` 口径（原样键 + 尾部空格可见）
        let entries = abbr::build_comment_entries(keymap, config);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((left, self.comments_column(context, &entries)))
    }
}
