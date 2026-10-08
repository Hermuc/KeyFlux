//! plugin —— `Shell::update` 的 插件卡: 开关/配置/导入/删除/市场入口 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 2026-10-08 批 W3b：**纯臂逻辑**抽为本文件的 `fn`（测试经子模块直测），
//! `handle_plugin` 变薄壳。例外（不可脱离窗口测试，保持原样）：`PluginsReload` /
//! `PluginsMarket`（`spawn_background`）、`PluginImport`（同步模态文件对话框 +
//! `fs::read`）。

use super::super::*;

impl Shell {
    /// `plugin` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_plugin(
        &mut self,
        message: Message,
        context: &ComponentContext<Self>,
    ) {
        match message {
            Message::PluginsReload => {
                self.reload_plugins(context);
            }
            Message::PluginsLoaded(result) => self.plugins_loaded(result),
            Message::PluginToggle {
                id,
                is_builtin,
                enabled,
            } => self.plugin_toggle(id, is_builtin, enabled),
            Message::PluginDelete(id) => {
                self.plugin_delete(id);
                self.schedule_notice_clear(context);
            }
            Message::PluginImport => {
                // 文件选择是**同步模态**对话（必须在 UI 线程弹出）；字节此刻读出并校验
                // 可读（与旧链路同一失败口径），上传入队 —— 由「保存配置」统一提交。
                // 过滤器显示名走 i18n 2437（旧版 `KeyFlux 插件包`），不再硬编码中文；
                // 双零编码交给 platform::file_dialog::single_filter（业务层不得出现原始 NUL）。
                let selected = platform::file_dialog::pick_open_file(
                    &i18n::t("2427"),
                    &platform::file_dialog::single_filter(&i18n::t("2437"), "*.zip"),
                );
                let Some(path) = selected else {
                    return;
                };
                let bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        self.plugins_action_error = Some(format!("{}: {error}", i18n::t("2432")));
                        return;
                    }
                };
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "plugin.zip".to_string());
                self.pending
                    .push(save_pipeline::PendingChange::PluginImport { bytes, file_name });
                self.plugins_error = None;
                self.plugins_action_error = None;
                self.plugin_status = Some(i18n::t("2595"));
            }
            // ---------------------------------------------------------- 插件市场
            Message::PluginsMarket => {
                self.market_open = true;
                self.market_status = None;
                self.reload_market(context);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------- 抽取的纯逻辑（可测）

    fn plugins_loaded(&mut self, result: Result<Box<PluginListResponse>, String>) {
        self.plugins_loading = false;
        match result {
            Ok(catalog) => {
                self.plugins_error = plugins::join_errors(catalog.errors.as_ref());
                self.plugin_catalog = Some(*catalog);
                self.plugins_action_error = None;
            }
            Err(reason) => {
                // 目录加载失败**保留既有卡片**只加告警（复刻 `PluginsPageViewModel`：
                // 失败早退、旧列表不清）
                self.plugins_error = Some(reason);
            }
        }
    }

    /// 只改内存态（保存策略：唯一落盘入口 = 页脚「保存配置」；旧版
    /// "开关即保存 + 重启引擎" 已随自动保存一起退役，启用在保存后生效）。
    fn plugin_toggle(&mut self, id: String, is_builtin: bool, enabled: bool) {
        let _ = self
            .config
            .as_mut()
            .map(|config| plugins::apply_enabled(config, &id, is_builtin, enabled));
    }

    /// 删除主体（自动清除提示由调用侧 `schedule_notice_clear` 承担）：
    /// bundled 判定以删除前目录快照里的 manifest 标记为准（墓碑只该打在
    /// 随包分发的插件上；此刻目录仍在，快照未失效）。
    /// 保存策略：注册表清理只改内存 config，目录删除入队，
    /// 由「保存配置」统一提交（提交后 refresh_plugins 重拉目录）。
    pub(in crate::app) fn plugin_delete(&mut self, id: String) {
        let bundled = self
            .plugin_catalog
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .plugins
                    .iter()
                    .find(|manifest| manifest.id == id)
                    .map(|manifest| manifest.bundled)
            })
            .unwrap_or(false);
        let _ = self
            .config
            .as_mut()
            .map(|config| plugins::remove_from_registry(config, &id, bundled));
        self.pending
            .push(save_pipeline::PendingChange::PluginDelete(id));
        self.plugins_action_error = None;
        self.notice = Some(i18n::t("2595"));
        self.notice_error = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::plugins::PluginManifest;

    /// 目录到达契约：Ok 覆盖快照 + 汇总逐包错误；Err **保留既有卡片**只加告警
    /// （复刻 `PluginsPageViewModel`：失败早退、旧列表不清）。
    #[test]
    fn plugins_loaded_ok_replaces_err_keeps_existing() {
        let old_response = PluginListResponse {
            plugins: vec![PluginManifest {
                id: "old".to_string(),
                ..Default::default()
            }],
            errors: None,
        };
        let mut shell = Shell {
            plugins_loading: true,
            plugin_catalog: Some(old_response),
            ..Shell::default()
        };

        shell.plugins_loaded(Err("网络失败".to_string()));
        assert!(!shell.plugins_loading);
        assert_eq!(shell.plugins_error.as_deref(), Some("网络失败"));
        assert!(
            shell.plugin_catalog.as_ref().unwrap().plugins.len() == 1,
            "Err 不得清掉既有卡片"
        );

        let response = PluginListResponse {
            plugins: vec![
                PluginManifest {
                    id: "a".to_string(),
                    ..Default::default()
                },
                PluginManifest {
                    id: "b".to_string(),
                    ..Default::default()
                },
            ],
            errors: Some(vec!["包 b 坏了".to_string()]),
        };
        shell.plugins_loaded(Ok(Box::new(response)));
        assert!(shell.plugins_error.is_some(), "逐包错误应汇总进告警");
        assert_eq!(shell.plugin_catalog.as_ref().unwrap().plugins.len(), 2);
        assert!(shell.plugins_action_error.is_none());
    }

    /// 开关 = 停用注册表增删（内存 config）：停用未注册的 → push；启用已注册的 → 移除。
    #[test]
    fn plugin_toggle_updates_disabled_registry() {
        let mut shell = Shell {
            config: Some(crate::models::config::Config::default()),
            ..Shell::default()
        };

        shell.plugin_toggle("demo".to_string(), false, false);
        let registry = &shell.config.as_ref().unwrap().options.plugins.disabled;
        assert!(registry.contains(&"demo".to_string()), "停用应写入注册表");

        shell.plugin_toggle("demo".to_string(), false, true);
        let registry = &shell.config.as_ref().unwrap().options.plugins.disabled;
        assert!(!registry.contains(&"demo".to_string()), "启用应移出注册表");
    }

    /// 删除 = 注册表清理（bundled 追加墓碑）+ 入队 + 非红提示；自动清除由调用侧承担。
    #[test]
    fn plugin_delete_cleans_registry_and_enqueues() {
        let mut config = crate::models::config::Config::default();
        config.options.plugins.disabled.push("doomed".to_string());
        let mut shell = Shell {
            config: Some(config),
            plugin_catalog: Some(PluginListResponse {
                plugins: vec![PluginManifest {
                    id: "doomed".to_string(),
                    bundled: true,
                    ..Default::default()
                }],
                errors: None,
            }),
            ..Shell::default()
        };

        shell.plugin_delete("doomed".to_string());

        let options = &shell.config.as_ref().unwrap().options.plugins;
        assert!(!options.disabled.contains(&"doomed".to_string()));
        assert!(
            options.removed.contains(&"doomed".to_string()),
            "bundled 插件删除应追加墓碑（防随包重装复活）"
        );
        assert!(!shell.pending.is_empty());
        assert!(shell.notice.is_some());
        assert!(!shell.notice_error);
        assert!(shell.plugins_action_error.is_none());
    }
}
