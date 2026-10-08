//! `app::pages::plugins` —— 插件页：内置 QuickSwitch 卡 + 用户插件卡 + zip 导入 + 市场入口。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。

use super::super::*;

impl Shell {
    /// 插件页：页头入口 + 分区说明 + 统一卡片列表 + 三态。
    ///
    /// 卡片由「当前配置 + 目录快照」**每次渲染即时派生** ⇒ 开关状态天然与配置同步
    /// （无需额外的双向同步标记，旧版的 `_syncingFromConfig` 因此省去）。
    pub(in crate::app) fn plugins_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let english = matches!(i18n::language(), i18n::Lang::En);
        let catalog = self.plugin_catalog.clone().unwrap_or_default();
        let cards = plugins::build_cards(config, &catalog);

        let mut rows: Vec<(usize, View)> = Vec::new();
        rows.push((
            rows.len(),
            plugins_view::page_header(
                context.message(Message::PluginsMarket),
                context.message(Message::PluginImport),
            ),
        ));

        if let Some(status) = &self.plugin_status {
            rows.push((rows.len(), plugins_view::status_banner(status)));
        }

        // 统一插件列表（旧版顺序：卡列表在前，加载/告警/空态随后）
        for card in &cards {
            let toggle_id = card.id.clone();
            let is_builtin = card.is_builtin;
            let delete_id = card.id.clone();
            let configure_id = card.id.clone();
            rows.push((
                rows.len(),
                plugins_view::plugin_card(
                    card,
                    english,
                    context.callback(move |enabled: bool| Message::PluginToggle {
                        id: toggle_id.clone(),
                        is_builtin,
                        enabled,
                    }),
                    context.message(Message::PluginDelete(delete_id)),
                    context.callback(move |_info: PointerEventInfo| {
                        Message::PluginConfigure(configure_id.clone())
                    }),
                ),
            ));
        }

        if self.plugins_loading {
            rows.push((rows.len(), plugins_view::loading()));
        } else if let Some(error) = &self.plugins_error {
            rows.push((
                rows.len(),
                plugins_view::load_error(error, None, context.message(Message::PluginsReload)),
            ));
        } else if let Some(error) = &self.plugins_action_error {
            // 一次性操作失败：纯文字横幅（无重试按钮，重试语义只属于目录加载）
            rows.push((rows.len(), plugins_view::action_error(error)));
        } else if plugins::show_empty_state(false, None, &cards) {
            rows.push((rows.len(), plugins_view::empty_state()));
        }

        // 页尾：运行时边界说明 + 配置引导（旧版在列表之后）
        rows.push((rows.len(), plugins_view::footer_notes()));

        // 旧 `StackPanel Margin="36,32,36,40" Spacing="16" MaxWidth="820"`（左对齐）
        ScrollViewer::new().content(
            StackPanel::new()
                .spacing(16.0)
                .max_width(820.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .margin(Thickness::new(36.0, 32.0, 36.0, 40.0))
                .keyed_children(rows),
        )
    }
}
