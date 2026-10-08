//! keymap —— `Shell::update` 的 键位网格 / 导航 / 全局开关 / 保存流水线 / 后端生命周期 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_keymap`。
//!
//! 2026-10-09 结构优化（批 W 手法）：**纯臂逻辑**抽为本文件的私有 `fn`，`handle_keymap`
//! 变薄壳（match → 方法）；单行赋值臂保持原样不抽。含 `context` 的臂（导航、
//! `WindowSpy` / `Ready` / `Retry` / `Save` / `SaveFinished` / `PickWindow` /
//! `StartupToggle`）显式接收 `context`。

use super::super::*;

impl Shell {
    /// `keymap` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_keymap(
        &mut self,
        message: Message,
        context: &ComponentContext<Self>,
    ) {
        match message {
            Message::Nav(tag) => self.nav_to(tag, context),
            // 禁用键的判断已在状态层完成，这里再兜一道（复刻 `KeymapEditorCore.SelectKey` 的
            // 空值/禁用键 guard：键盘网格页与未来入口共用该消息）
            Message::ToggleComments => self.comments_collapsed = !self.comments_collapsed,
            Message::PaneOverlay(open) => self.pane_overlay_open = open,
            Message::AcrylicToggle(value) => self.acrylic_toggle(value),
            Message::SelectKey(hotkey) => self.select_key(hotkey),
            Message::CmdText(text) => self.cmd_text(text),
            Message::Noop => {}
            Message::SelectWindowGroup(id) => self.window_group_id = id,
            Message::SelectActionType(type_id) => self.select_action_type(type_id),
            Message::EditField(field) => self.apply_field(field),
            Message::SelectRadio {
                value_id,
                label_key,
            } => self.select_radio(value_id, label_key),
            Message::SelectPluginAction { action_id } => self.select_plugin_action(action_id),
            Message::WindowSpy => self.window_spy(context),
            Message::Ready {
                config,
                port,
                doc_md,
                data_root,
                ui_prefs,
            } => self.on_ready(config, port, doc_md, data_root, ui_prefs, context),
            Message::Failed(reason) => {
                self.loading = false;
                self.error = Some(reason);
            }
            Message::Retry => self.retry(context),
            Message::Save => self.save_now(context),
            Message::SaveFinished(result) => self.save_finished(result, context),
            Message::PickWindow => self.pick_window(context),
            Message::WindowPicked(outcome) => self.window_picked(outcome),
            Message::SettingsSection(section) => self.toggle_settings_section(section),
            Message::StartupToggle(enabled) => self.startup_toggle(enabled, context),
            Message::Opt(edit) => self.apply_opt(edit),
            Message::DelayScheme(index) => self.delay_scheme = index,
            _ => {}
        }
    }

    // ------------------------------------------------------- 抽取的纯逻辑（可测）

    fn nav_to(&mut self, tag: Option<String>, context: &ComponentContext<Self>) {
        if let Some(tag) = tag
            && let Some(index) = self.nav.iter().position(|entry| entry.tag == tag)
        {
            self.page_index = index;
            // 切页清空键位选中与命令框（旧版每页各有独立 `SelectedHotkey`/`CmdText`）
            self.selected_hotkey = None;
            self.cmd_text.clear();
            // 进入插件页即拉取目录（复刻旧版导航回调里的 `ReloadAsync`）
            if self.nav[index].kind == PageKind::Plugins {
                self.reload_plugins(context);
            }
        }
    }

    fn acrylic_toggle(&mut self, value: bool) {
        // 视觉即时生效（玻璃策略随渲染派生）；持久化入队 —— 只有
        // 「保存配置」成功后才写 `data/ui-prefs.json`（保存策略统一咽喉）。
        self.acrylic = value;
        self.pending.push(save_pipeline::PendingChange::UiPrefs(
            crate::services::ui_prefs::UiPrefs { acrylic: value },
        ));
    }

    fn select_key(&mut self, hotkey: String) {
        if hotkey.is_empty() {
            return;
        }
        let disabled = self
            .config
            .as_ref()
            .map(keymap::disabled_keys)
            .unwrap_or_default();
        let guard_ok = self
            .current_keymap()
            .map(|keymap| !keymap::is_disabled(&disabled, keymap.id, &hotkey))
            .unwrap_or(true);
        if guard_ok {
            self.selected_hotkey = Some(hotkey);
        }
    }

    fn cmd_text(&mut self, text: String) {
        // 回车执行：0.100.0 无键盘事件 API ⇒ 命令框用 `accepts_return(true)`，
        // 回车插入的尾部换行即执行信号（见 `ui::abbr_view` 模块说明）。
        // Win32 Edit 可能插入 LF 或 CR (取决于实现), 两者都算执行信号
        if text.ends_with('\n') || text.ends_with('\r') {
            // 2026-10-04 修复: 多行文本(粘贴/输入法组合)在中间也含换行,
            // trim_end 只删尾部 => 中间换行残留在命令里。全部移除后再执行。
            let cleaned: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
            self.cmd_text = cleaned.trim().to_string();
            self.run_abbr_command();
        } else {
            self.cmd_text = text;
        }
    }

    fn select_action_type(&mut self, type_id: i32) {
        // 先取变更信息（借用随之结束），再做可能的缩写联动
        let change = self
            .current_action_mut()
            .and_then(|action| action_editor::apply_type_change(action, type_id));
        if let Some(change) = change {
            self.maybe_refresh_abbr_enable(change.old_type, change.new_type, change.old_value, 0);
        }
    }

    fn select_radio(&mut self, value_id: i32, label_key: &'static str) {
        let item = action_editor::RadioItem {
            value_id,
            label_key,
            hide_in_abbr: false,
        };
        let (old_value, new_value) = self
            .current_action_mut()
            .map(|action| action_editor::select_radio(action, &item))
            .unwrap_or((-1, -1));
        if old_value >= 0 {
            // 复刻 `MaybeRefreshAbbrEnable(typeId, typeId, oldValue, newValue)`
            if let Some(type_id) = self.current_action().map(|action| action.type_id) {
                self.maybe_refresh_abbr_enable(type_id, type_id, old_value, new_value);
            }
        }
    }

    fn select_plugin_action(&mut self, action_id: String) {
        let old_value = self
            .current_action_mut()
            .map(|action| action_editor::select_plugin_action(action, &action_id))
            .unwrap_or(-1);
        if old_value >= 0
            && let Some(type_id) = self.current_action().map(|action| action.type_id)
        {
            self.maybe_refresh_abbr_enable(type_id, type_id, old_value, 0);
        }
    }

    fn window_spy(&mut self, context: &ComponentContext<Self>) {
        if let Some(port) = self.port {
            let _ = context.spawn_background(move |_token| {
                let api = crate::services::transport::new_settings_api(port);
                let response: ApiResponse<crate::services::api::EmptyJson> =
                    api.send_server_command(2);
                Message::Notice(if response.success {
                    Ok(i18n::t("309"))
                } else {
                    Err(response
                        .error_message
                        .unwrap_or_else(|| "窗口侦探启动失败".to_string()))
                })
            });
        }
    }

    fn on_ready(
        &mut self,
        config: Box<crate::models::Config>,
        port: u16,
        doc_md: String,
        data_root: Option<std::path::PathBuf>,
        ui_prefs: crate::services::ui_prefs::UiPrefs,
        context: &ComponentContext<Self>,
    ) {
        self.acrylic = ui_prefs.acrylic;
        self.nav = build_nav(&config);
        self.config = Some(*config);
        self.port = Some(port);
        self.doc_md = doc_md;
        self.data_root = data_root;
        // CLI 传输没有端口 ⇒ 登记本地静态站目录：指南图片走直读 + `source_data`、
        // 内部链接走 `file:///`。HTTP 模式**不登记** ⇒ 行为与改动前完全一致。
        if crate::services::transport::transport() == crate::services::transport::Transport::Cli {
            crate::ui::doc_assets::set_site_dir(
                self.data_root
                    .as_ref()
                    .map(|root| root.join("bin").join("site")),
            );
        }
        self.page_index = 0;
        self.loading = false;
        self.error = None;
        // 插件目录预取（P7b）：type 9 动作编辑器的插件动作动态组消费
        // `plugin_catalog`；启动即取，不等用户进插件页。
        self.reload_plugins(context);
        // 行为目录快照（选中动作页消费；失败时目录为空 = 下拉空，不阻断页面）
        let _ = context.spawn_background(move |_token| {
            let api = crate::services::transport::new_settings_api(port);
            let response = api.get_behaviors();
            if let Some(value) = response.value {
                Message::BehaviorsLoaded(Ok(Box::new(sa::SaCatalog {
                    builtin: value.builtin,
                    user: value.user,
                })))
            } else {
                Message::BehaviorsLoaded(Err(response
                    .error_message
                    .unwrap_or_else(|| format!("HTTP {}", response.status))))
            }
        });
    }

    fn retry(&mut self, context: &ComponentContext<Self>) {
        self.loading = true;
        self.error = None;
        self.notice = None;
        let slot = Arc::clone(&self.session);
        let _ = context.spawn_background(move |_token| load_backend(slot));
    }

    fn save_finished(
        &mut self,
        result: Result<String, save_pipeline::SaveFailure>,
        context: &ComponentContext<Self>,
    ) {
        match result {
            Ok(text) => {
                self.hotkey_pending_save = false;
                self.notice_error = false;
                self.notice = Some(text);
                // 保存成功 ⇒ 导航即时重建（方案启停等变化反映到侧栏，复刻 BuildNav）
                self.rebuild_nav();
                // 暂存变更已提交 ⇒ 从后端重拉目录，两份快照与服务端最终态对齐
                self.reload_catalog(context);
                self.refresh_plugins(context);
                self.schedule_notice_clear(context);
            }
            Err(failure) => {
                self.notice_error = true;
                self.notice = Some(failure.reason);
                // 未执行的余项原序退回队列（已执行项不回滚，重试由再次保存承担）
                self.pending.restore(failure.remaining);
            }
        }
    }

    /// 窗口拾取准星 (动作编辑面板 301 行): 会话跑在后台线程, 阻塞至
    /// Esc/右键/左键提交; 防重入由 picking 标志 (按钮置灰) + 平台层 BUSY 双保险。
    fn pick_window(&mut self, context: &ComponentContext<Self>) {
        if self.picking {
            return;
        }
        self.picking = true;
        // 高亮框颜色 = 品牌强调色 (theme 令牌直取, 与 UI 同源)。
        let accent = theme::ACCENT;
        let highlight = crate::platform::window_picker::Rgb(accent.r, accent.g, accent.b);
        let _ = context.spawn_background(move |_token| {
            Message::WindowPicked(crate::platform::window_picker::pick(highlight))
        });
    }

    fn window_picked(&mut self, outcome: crate::platform::window_picker::PickOutcome) {
        self.picking = false;
        match outcome.status {
            crate::platform::window_picker::PickStatus::Success => {
                // 写回与手工输入同一条 apply_field 通路 (空值联动等语义一致)。
                self.apply_field(ActionField::WinTitle(outcome.text));
            }
            crate::platform::window_picker::PickStatus::AccessDenied => {
                self.notice_error = true;
                self.notice = Some(i18n::t("1081"));
            }
            crate::platform::window_picker::PickStatus::NoWindow => {
                self.notice_error = true;
                self.notice = Some(i18n::t("1082"));
            }
            crate::platform::window_picker::PickStatus::Cancelled => {}
        }
    }

    fn toggle_settings_section(&mut self, section: &'static str) {
        // 一次只展开一张（复刻旧版手风琴）：再点已展开的则收起
        self.settings_open = if self.settings_open == Some(section) {
            None
        } else {
            Some(section)
        };
    }

    fn startup_toggle(&mut self, enabled: bool, context: &ComponentContext<Self>) {
        // 回显态写内存 config；真实生效 = 计划任务（服务端命令 3/4），
        // 入队待「保存配置」统一发送（保存策略：无即时副作用）。
        if let Some(config) = self.config.as_mut() {
            settings::set_startup(config, enabled);
        }
        self.pending
            .push(save_pipeline::PendingChange::StartupCommand(
                settings::startup_command_id(enabled),
            ));
        self.notice = Some(i18n::t("2595"));
        self.notice_error = false;
        self.schedule_notice_clear(context);
    }
}
