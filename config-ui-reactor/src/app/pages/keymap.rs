//! `app::pages::keymap` —— 键位矩阵页。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。

use super::super::*;

impl Shell {
    /// 键位图页：页头 + 键盘网格 + 动作编辑面板 + 右侧备注汇总。
    pub(in crate::app) fn keymap_page(
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

        let rows = keymap::build_rows(&config.options.keyboard_layout, &keymap.hotkey);
        let font_size = keymap::key_font_size(keymap::small_font(&rows));
        let disabled = keymap::disabled_keys(config);
        let states = keymap_view::compute_states(
            &rows,
            keymap,
            &disabled,
            self.selected_hotkey.as_deref(),
            self.window_group_id,
        );

        let grid = keymap_view::keyboard_grid(&rows, &states, font_size, |hotkey| {
            context.message(Message::SelectKey(hotkey))
        });

        // 左列三段式：页头（Auto）/ 键盘网格（**Auto = 自然高度**）/ 动作面板（STAR）。
        // ⚠️ `View` 不实现 `LayoutControl` ⇒ `grid_row` 只能设在未收尾的 builder 上；
        //    已构建的 `View` 用 `Border` 包裹后再定位。
        //
        // ⚠️ 2026-10-01 修「点已绑定键后动作面板铺满整页、键盘网格被挤没」：
        //    旧结构 = 网格行 STAR + 面板行 **Auto**。Grid 的 Auto 行按**无限高度**测量子级 ⇒
        //    面板内容要多少给多少（实测单选表 ≈900 DIP），网格行被压到 0 高度；面板底部还会
        //    溢出窗口被裁，且**滚不动**（ScrollViewer 自认拿到全额高度，无滚动区间）。
        //    新结构 = **网格行 Auto**（自然高度，`max_height` 封顶防超大自定义布局）+
        //    **面板行 STAR**：① 网格永远拿满自己的自然高度 ⇒ 不被面板挤压、整张可见；
        //    ② 面板拿到「剩余高度」这个**有界**视口 ⇒ 内容再高也只滚不溢出；
        //    ③ 面板内容不足时**贴底**（`VerticalAlignment::Bottom`）⇒ 观感与旧版一致
        //       （小卡片仍在页面底部），内容一多就填满该区并滚轮浏览。
        //    两者都是纯**布局约束**：两个滚动区各自独立（指针落在哪个区就滚哪个），
        //    网格的显示与交互不受任何影响。
        let left: View = Border::new().grid_column(0).content(
            Grid::new()
                .rows([GridLength::Auto, GridLength::Auto, GridLength::STAR])
                .margin(Thickness::new(24.0, 20.0, 12.0, 28.0))
                .children((
                    Border::new().grid_row(0).content(keymap_view::page_header(
                        &keymap::header_title(keymap),
                        keymap::parent_info(keymap, config).as_deref(),
                    )),
                    Border::new().grid_row(1).content(
                        // 横向兜底：低逻辑宽（DPI 缩放/备注栏展开）下底行自然宽可能
                        // 超出可用宽，SinglePress 等行尾键会被裁 —— 允许横向滚动保底。
                        ScrollViewer::new()
                            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
                            .max_height(keymap_view::GRID_MAX_HEIGHT)
                            .content(grid),
                    ),
                    Border::new()
                        .grid_row(2)
                        .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                        .content(
                            ScrollViewer::new().content(
                                Border::new()
                                    .vertical_alignment(VerticalAlignment::Bottom)
                                    .content(self.action_editor_panel(context)),
                            ),
                        ),
                )),
        );

        let entries = keymap::build_comment_entries(keymap, config);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((left, self.comments_column(context, &entries)))
    }
}
