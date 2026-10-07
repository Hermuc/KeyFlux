//! market —— `Shell::update` 的 插件市场对话框 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_market`。

use super::super::*;

impl Shell {
    /// `market` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_market(
        &mut self,
        message: Message,
        context: &ComponentContext<Self>,
    ) {
        match message {
            Message::MarketReload => {
                self.reload_market(context);
            }
            Message::MarketLoaded(result) => {
                self.market_loading = false;
                match result {
                    Ok(entries) => {
                        self.market_error = None;
                        self.market_entries = entries;
                    }
                    Err(reason) => {
                        self.market_error = Some(reason);
                        self.market_entries.clear();
                    }
                }
            }
            Message::MarketInstall { id, url } => {
                // 保存策略：安装（下载 zip + 导入）入队，由「保存配置」统一提交；
                // is_installed 保持 false（此刻确实未装，保存成功后刷新列表即到位）。
                self.pending
                    .push(save_pipeline::PendingChange::MarketInstall {
                        id: id.clone(),
                        url,
                    });
                self.market_error = None;
                self.market_status = Some(i18n::t("2595"));
            }
            Message::MarketClosed => {
                self.market_open = false;
                // 关闭市场后无条件刷新插件页（复刻 `OnMarketClosed` 的 ReloadAsync）
                self.refresh_plugins(context);
            }
            // ---------------------------------------------------------- 插件设置对话框
            _ => {}
        }
    }
}
