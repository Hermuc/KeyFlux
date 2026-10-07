//! ps —— `Shell::update` 的 单个插件的设置对话框 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_ps`。

use super::super::*;

impl Shell {
    /// `ps` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_ps(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::PsValue(index, value) => {
                if let Some(slot) = self.ps_rows.get_mut(index) {
                    slot.1 = value;
                }
            }
            Message::PsPickFile(index) => {
                // 过滤器来自后端声明；转成 Win32 双 NUL 串（复刻 ExtOf：`"everything.exe"`
                // → 描述 + `*.exe`，此前原样直传导致对话框异常）
                let filter = self
                    .ps_rows
                    .get(index)
                    .and_then(|(setting, _)| setting.filter.clone())
                    .map(|declared| plugins::file_dialog_filter(&declared))
                    .unwrap_or_else(|| platform::file_dialog::ALL_FILES_FILTER.to_string());
                if let Some(path) = platform::file_dialog::pick_open_file(&i18n::t("2583"), &filter)
                    && let Some(slot) = self.ps_rows.get_mut(index)
                {
                    slot.1 = path.to_string_lossy().into_owned();
                }
            }
            Message::PsLoaded { id, result } => {
                if self.ps_id.as_deref() != Some(id.as_str()) {
                    return; // 已切换或关闭 ⇒ 丢弃过期结果
                }
                self.ps_loading = false;
                match result {
                    Ok(response) => {
                        // 服务端空声明时退回本地 manifest（窗口期兜底）
                        let manifest = self
                            .plugin_catalog
                            .as_ref()
                            .and_then(|catalog| catalog.plugins.iter().find(|m| m.id == id))
                            .cloned()
                            .unwrap_or_default();
                        self.ps_rows = plugins::settings_rows(&response, &manifest);
                        self.ps_error = None;
                    }
                    Err(reason) => {
                        self.ps_error = Some(format!("{}: {reason}", i18n::t("2584")));
                        self.ps_rows.clear();
                    }
                }
            }
            Message::PsClosed(result) => {
                if result != ContentDialogResult::Primary {
                    self.ps_open = false;
                    self.ps_error = None;
                    return;
                }
                // 本地即时校验（保存管线提交时后端仍是权威；失败 → 弹窗保持打开供修正）
                let english = matches!(i18n::language(), i18n::Lang::En);
                for (setting, value) in &self.ps_rows {
                    if let Some(reason) = plugins::validate_setting(setting, value) {
                        let label = plugins::setting_label(setting, english);
                        self.ps_error = Some(format!("{}: {label}: {reason}", i18n::t("2585")));
                        self.ps_open = true;
                        return;
                    }
                }
                let Some(id) = self.ps_id.clone() else {
                    return;
                };
                let values: BTreeMap<String, String> = self
                    .ps_rows
                    .iter()
                    .map(|(setting, value)| (setting.key.clone(), value.clone()))
                    .collect();
                // 保存策略：「保存」= 入队（整表写回），由「保存配置」统一提交
                // （提交时后端按声明校验，失败原因经页脚提示条回显）。
                self.pending
                    .push(save_pipeline::PendingChange::PluginSettings { id, values });
                self.ps_open = false;
                self.ps_error = None;
                self.notice = Some(i18n::t("2595"));
                self.notice_error = false;
                self.schedule_notice_clear(context);
            }
            Message::PluginConfigure(id) => {
                // 声明式设置（P6 起 QuickSwitch 亦走此路 —— 专用对话框已删）：
                // 打开即拉取「声明 + 默认值合并后的完整值表」
                {
                    let Some(config) = self.config.as_ref() else {
                        return;
                    };
                    let Some(catalog) = self.plugin_catalog.as_ref() else {
                        return;
                    };
                    let Some(manifest) = catalog.plugins.iter().find(|m| m.id == id) else {
                        return;
                    };
                    if !plugins::card_from(config, manifest).can_configure {
                        return;
                    }
                    let english = matches!(i18n::language(), i18n::Lang::En);
                    self.ps_id = Some(manifest.id.clone());
                    self.ps_title = plugins::card_from(config, manifest).display_name(english);
                    self.ps_rows.clear();
                    self.ps_error = None;
                    self.ps_open = true;
                    self.ps_loading = true;
                    if let Some(port) = self.port {
                        let plugin_id = manifest.id.clone();
                        let _ = context.spawn_background(move |_token| {
                            let api = crate::services::transport::new_settings_api(port);
                            let response = api.get_plugin_settings(&plugin_id);
                            Message::PsLoaded {
                                id: plugin_id,
                                result: match response.value {
                                    Some(value) => Ok(value),
                                    None => Err(response
                                        .error_message
                                        .unwrap_or_else(|| format!("HTTP {}", response.status))),
                                },
                            }
                        });
                    }
                }
            }
            // ---------------------------------------------------------- 选项页
            _ => {}
        }
    }
}
