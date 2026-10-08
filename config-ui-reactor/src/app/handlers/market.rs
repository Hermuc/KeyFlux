//! market —— `Shell::update` 的 插件市场对话框 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 2026-10-08 批 W3b：**纯臂逻辑**抽为本文件的 `fn`（测试经子模块直测），
//! `handle_market` 变薄壳。`MarketReload` / `MarketClosed` 两臂经
//! `reload_market` / `refresh_plugins` 触发 `context.spawn_background` ⇒ 不可脱离
//! 窗口测试，保持原样（本域仅 4 臂中的 2 臂可抽）。

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
            Message::MarketLoaded(result) => self.market_loaded(result),
            Message::MarketInstall { id, url } => self.market_install(id, url),
            Message::MarketClosed => {
                self.market_open = false;
                // 关闭市场后无条件刷新插件页（复刻 `OnMarketClosed` 的 ReloadAsync）
                self.refresh_plugins(context);
            }
            // ---------------------------------------------------------- 插件设置对话框
            _ => {}
        }
    }

    // ------------------------------------------------------- 抽取的纯逻辑（可测）

    fn market_loaded(&mut self, result: Result<Vec<MarketEntry>, String>) {
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

    /// 安装（下载 zip + 导入）入队，由「保存配置」统一提交；
    /// is_installed 保持 false（此刻确实未装，保存成功后刷新列表即到位）。
    pub(in crate::app) fn market_install(&mut self, id: String, url: String) {
        self.pending
            .push(save_pipeline::PendingChange::MarketInstall { id, url });
        self.market_error = None;
        self.market_status = Some(i18n::t("2595"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MarketEntry 无 Default ⇒ 测试用最小合法条目构造。
    fn entry(id: &str) -> MarketEntry {
        MarketEntry {
            id: id.to_string(),
            name: format!("插件 {id}"),
            name_en: None,
            version: None,
            description: String::new(),
            author: String::new(),
            url: format!("https://example.com/{id}.zip"),
            is_installed: false,
        }
    }

    /// 目录到达契约：Ok 覆盖 entries 并清错误；Err 置错误**并清空旧列表**
    /// （与 PluginsLoaded 的「失败保留旧卡片」策略刻意不同——市场是只读目录）。
    #[test]
    fn market_loaded_ok_replaces_err_keeps_placeholder_free() {
        let mut shell = Shell {
            market_loading: true,
            market_entries: vec![entry("old")],
            ..Shell::default()
        };
        shell.market_loaded(Ok(vec![entry("demo")]));
        assert!(!shell.market_loading);
        assert!(shell.market_error.is_none());
        assert_eq!(shell.market_entries.len(), 1);
        assert_eq!(shell.market_entries[0].id, "demo");

        shell.market_loaded(Err("拉取失败".to_string()));
        assert!(!shell.market_loading);
        assert_eq!(shell.market_error.as_deref(), Some("拉取失败"));
        assert!(shell.market_entries.is_empty(), "Err 应清空旧列表");
    }

    /// 安装 = 入队 + 清错误 + 状态条回显；不改 is_installed（保存成功后刷新才到位）。
    #[test]
    fn market_install_enqueues_and_flashes_status() {
        let mut shell = Shell {
            market_error: Some("旧错误".to_string()),
            ..Shell::default()
        };

        shell.market_install("demo".to_string(), "https://example.com/a.zip".to_string());

        assert!(!shell.pending.is_empty());
        assert!(shell.market_error.is_none());
        assert!(shell.market_status.is_some());
    }
}
