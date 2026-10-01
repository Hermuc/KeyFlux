//! 根组件与外壳（迁移自 `MainWindow.axaml` + `MainViewModel`）。
//!
//! 旧结构映射：
//! * 无边框窗口 + 自绘标题栏 ⇒ `TitleBar` 控件（Fluent：标题栏可承载导航，三键系统原生）
//! * 左 `ListBox` 导航 ⇒ `NavigationView::MenuItems` 槽（**配置驱动**，见 [`build_nav`]）
//! * 底部保存按钮 + 保存提示 ⇒ `NavigationView::PaneFooter` 槽
//! * 内容区 + 加载/错误遮罩 ⇒ `Content` 槽按状态三态互斥
//! * `MainViewModel.InitializeAsync`（连后端 + 拉配置）⇒ `create` 里 `spawn_background`
//!
//! ⚠️ 0.100.0 实测约束（勿照抄 master 文档）：
//! * `View` **不**实现 `LayoutControl` ⇒ `grid_row` 只能设在**未收尾的 builder** 上。
//! * `Component::Message` 必须 `Clone` ⇒ 后端子进程经 `Arc<Mutex<Option<BackendSession>>>` 旁路移交。
//! * 弹窗用 `ComponentContext::open_window`（无 `run_window`）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_reactor::*;

use crate::glass;
use crate::models::SelectedEntry;
use crate::models::{
    Action, Config, Keymap, PluginListResponse, PluginSetting, PluginSettingsResponse,
    QuickSwitchOption,
};
use crate::platform::{self, WindowSpec};
use crate::services::abbr;
use crate::services::api::{ApiResponse, MessageBody, SettingsApi};
use crate::services::backend::{BackendSession, BackendSessionOptions, resolve_settings_exe};
use crate::services::market::{self, MarketEntry};
use crate::services::selected_action::{self as sa, MATCH_FILE_EXT, MATCH_TEXT_TYPE};
use crate::services::{
    action_editor, behaviors_edit, i18n, keymap, markdown, match_types_edit, plugins, settings,
    store,
};
use crate::theme;
use crate::ui::{
    abbr_view, action_editor as action_editor_view, keymap_view, markdown_view, plugins_view,
    selected_action_view, settings_view,
};

// 拆分后的子模块（原 app.rs 的 impl Shell 与类型定义按职责外移，见各文件头注）。
mod dialogs;
mod glue;
mod messages;
mod state;
mod views;

use glue::*;
use messages::*;

pub struct Shell {
    nav: Vec<NavEntry>,
    page_index: usize,
    loading: bool,
    error: Option<String>,
    notice: Option<String>,
    /// 提示条是否为**错误**（红）：保存失败不再踢出整页（对齐旧版「弹窗报错、现场保留」）。
    notice_error: bool,
    /// 上次页脚保存发起时刻（1 秒节流，复刻 `MainViewModel.SaveCommand` 的 `useThrottleFn`）。
    last_save: Option<Instant>,
    session: SessionSlot,
    port: Option<u16>,
    config: Option<Config>,
    doc_md: String,
    /// 键位图页当前选中的热键（切换页面时清空，对齐旧版每页独立的 `SelectedHotkey`）。
    selected_hotkey: Option<String>,
    /// 缩写页命令框内容（切换页面时清空，对齐旧版每页独立的 `CmdText`）。
    cmd_text: String,
    /// 插件目录快照（`GET /api/plugins`；`None` = 尚未拉取成功）。
    plugin_catalog: Option<PluginListResponse>,
    /// 插件目录加载中。
    plugins_loading: bool,
    /// 插件目录加载告警（逐包错误汇总；**带重试按钮**，语义 = 重新拉目录）。
    plugins_error: Option<String>,
    /// 插件页一次性操作失败（导入/删除；纯文字横幅，**无重试**——复刻旧版分流）。
    plugins_action_error: Option<String>,
    /// 插件页一次性操作回显（导入/删除成功提示）。
    plugin_status: Option<String>,
    /// QuickSwitch 配置对话框的编辑草稿（`None` = 对话框关闭）。
    qs_draft: Option<QuickSwitchOption>,
    /// QuickSwitch 对话框内错误（清空历史失败等；渲染在弹窗**内部**，不再写页面横幅被遮蔽）。
    qs_error: Option<String>,
    /// 插件市场对话框是否打开。
    market_open: bool,
    /// 市场条目（目录序）。
    market_entries: Vec<MarketEntry>,
    /// 市场目录拉取中。
    market_loading: bool,
    /// 市场目录拉取失败原因。
    market_error: Option<String>,
    /// 正在安装的市场条目 id。
    market_installing: Option<String>,
    /// 安装成功回显。
    market_status: Option<String>,
    /// 插件设置对话框：当前插件 id。
    ps_id: Option<String>,
    /// 插件设置对话框：标题（插件显示名）。
    ps_title: String,
    /// 插件设置对话框：设置行（声明序）。
    ps_rows: Vec<(PluginSetting, String)>,
    /// 插件设置对话框：加载中。
    ps_loading: bool,
    /// 插件设置对话框：加载/校验失败原因（非空时以红字呈现）。
    ps_error: Option<String>,
    /// 插件设置对话框是否打开（校验失败重开时用）。
    ps_open: bool,
    /// 插件设置对话框保存进行中（防在结果返回前重复提交）。
    ps_saving: bool,
    /// 选中动作页启用开关的**保存前值**（保存失败时回滚显示态，复刻 `SaveEnableAsync`）。
    sa_enable_prev: Option<bool>,
    /// 「删除映射」确认框是否打开（1109 文案）。
    sa_delete_confirm: bool,
    /// 选中动作页页内状态条（▶ 执行失败等；`(文本, 是否错误)`）。
    sa_status: Option<(String, bool)>,
    /// 「添加映射」弹窗草稿（`None` = 关闭）。
    sa_add: Option<SaAddDraft>,
    /// 行为编辑尾随保存的代际计数（防抖窗口内新编辑使旧定时器失效）。
    sa_save_gen: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// 键位/缩写页右侧备注汇总是否折叠（默认展开；折叠以优先保证键盘网格完整显示）。
    comments_collapsed: bool,
    /// 选项页「亚克力毛玻璃效果」状态（持久化于面板私有偏好文件 `data/ui-prefs.json`）。
    acrylic: bool,
    /// 导航窗格是否展开（true = Left 模式带文字并推开内容；false = LeftCompact 图标窄轨）。
    /// 默认 true（保持原有「展开带文字」的默认观感），汉堡点击后由事件回写翻转。
    pane_overlay_open: bool,
    /// 「管理匹配类型」对话框是否打开。
    mt_dialog: bool,
    /// 匹配类型编辑草稿（`None` = 未进入编辑/已清）。
    mt_draft: Option<match_types_edit::MatchTypeDraft>,
    /// 匹配类型对话框内状态条。
    mt_status: Option<(String, bool)>,
    /// 「试一下」示例内容。
    mt_test: String,
    /// 「试一下」结果（Ok = 命中预览文本；Err = 失败原因）。
    mt_test_result: Option<Result<String, String>>,
    /// 「管理行为」对话框是否打开。
    bh_dialog: bool,
    /// 行为目录选中项（内置在前；`None` = 未选）。
    bh_pick: Option<usize>,
    /// 行为编辑草稿（`None` = 展示只读详情占位）。
    bh_draft: Option<behaviors_edit::BehaviorDraft>,
    /// 行为对话框内状态条。
    bh_status: Option<(String, bool)>,
    /// 行为保存/删除进行中（防重复提交）。
    bh_saving: bool,
    /// 「编辑使用指南」对话框是否打开。
    guide_edit_open: bool,
    /// 指南编辑框内容。
    guide_edit_text: String,
    /// 自定义热键动作编辑对话框：当前编辑的行（`Some(row)` = 打开，keymap id=1）。
    hotkey_editor_row: Option<usize>,
    /// 选项页当前展开的分区（`None` = 全部收起；一次只展开一张，复刻旧版手风琴）。
    settings_open: Option<&'static str>,
    /// 选项页「触发延时」分区当前选中的方案（`nav` 中 id>4 方案的下标）。
    delay_scheme: usize,
    /// 选项页一次性提示（开机自启结果 / 保存校验失败原因）。
    settings_notice: Option<String>,
    /// 部署根路径（`<deploy>`；用于「清空历史」定位 `data/quickswitch/history.tsv`）。
    data_root: Option<std::path::PathBuf>,
    /// 行为目录快照（选中动作页；`GET /api/behaviors`）。
    catalog: sa::Catalog,
    /// 选中动作页：文本卡当前点亮的 toggle id。
    sa_text_sel: Option<String>,
    /// 选中动作页：文件卡当前点亮的 toggle id。
    sa_file_sel: Option<String>,
    /// 选中动作页：文本卡「添加行为」下拉当前索引。
    sa_text_pick: Option<usize>,
    /// 选中动作页：文件卡「添加行为」下拉当前索引。
    sa_file_pick: Option<usize>,
    /// 选中动作页：热键已改未保存（保存成功后复位，对齐旧版 `HotkeyPendingSave`）。
    hotkey_pending_save: bool,
    /// 选中动作页热键与既有占用集冲突（1025 红字提示）。
    sa_hotkey_conflict: bool,
    /// 当前窗口分组（复刻 `store.windowGroupID`）。
    window_group_id: i32,
    /// 快捷方式下拉数据（`GET /shortcuts`，用于「启动程序或激活窗口」的目标选择）。
    shortcuts: Vec<String>,
}

impl Component for Shell {
    type Input = ();
    type Message = Message;

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let session: SessionSlot = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&session);
        let _ = context.spawn_background(move |_token| load_backend(slot));
        Self {
            nav: Vec::new(),
            page_index: 0,
            loading: true,
            error: None,
            notice: None,
            notice_error: false,
            last_save: None,
            session,
            port: None,
            config: None,
            doc_md: String::new(),
            selected_hotkey: None,
            cmd_text: String::new(),
            plugin_catalog: None,
            plugins_loading: false,
            plugins_error: None,
            plugins_action_error: None,
            plugin_status: None,
            qs_draft: None,
            qs_error: None,
            market_open: false,
            market_entries: Vec::new(),
            market_loading: false,
            market_error: None,
            market_installing: None,
            market_status: None,
            ps_id: None,
            ps_title: String::new(),
            ps_rows: Vec::new(),
            ps_loading: false,
            ps_error: None,
            ps_open: false,
            ps_saving: false,
            sa_enable_prev: None,
            sa_delete_confirm: false,
            sa_status: None,
            sa_add: None,
            sa_save_gen: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            comments_collapsed: false,
            acrylic: false,
            pane_overlay_open: true,
            mt_dialog: false,
            mt_draft: None,
            mt_status: None,
            mt_test: String::new(),
            mt_test_result: None,
            bh_dialog: false,
            bh_pick: None,
            bh_draft: None,
            bh_status: None,
            bh_saving: false,
            guide_edit_open: false,
            guide_edit_text: String::new(),
            hotkey_editor_row: None,
            settings_open: Some("delay"),
            delay_scheme: 0,
            settings_notice: None,
            data_root: None,
            catalog: sa::Catalog::default(),
            sa_text_sel: None,
            sa_file_sel: None,
            sa_text_pick: None,
            sa_file_pick: None,
            hotkey_pending_save: false,
            sa_hotkey_conflict: false,
            window_group_id: 0,
            shortcuts: Vec::new(),
        }
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            // 防闪白：导航列表重建会把选中项瞬时空掉，忽略不可解析的 tag。
            Message::Nav(tag) => {
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
            // 禁用键的判断已在状态层完成，这里再兜一道（复刻 `KeymapEditorCore.SelectKey` 的
            // 空值/禁用键 guard：键盘网格页与未来入口共用该消息）
            Message::ToggleComments => {
                self.comments_collapsed = !self.comments_collapsed;
            }
            Message::PaneOverlay(open) => {
                self.pane_overlay_open = open;
            }
            Message::AcrylicToggle(value) => {
                self.acrylic = value;
                // 持久化失败不阻断交互：状态已生效，仅以页内提示告知无法记住选择。
                if let Some(root) = self.data_root.as_deref()
                    && let Err(error) = crate::services::ui_prefs::save(
                        root,
                        crate::services::ui_prefs::UiPrefs { acrylic: value },
                    )
                {
                    self.settings_notice = Some(format!("{}: {error}", i18n::t("2594")));
                }
            }
            Message::SelectKey(hotkey) => {
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
            Message::CmdText(text) => {
                // 回车执行：0.100.0 无键盘事件 API ⇒ 命令框用 `accepts_return(true)`，
                // 回车插入的尾部换行即执行信号（见 `ui::abbr_view` 模块说明）。
                if text.ends_with('\n') {
                    self.cmd_text = text.trim_end_matches(['\n', '\r']).to_string();
                    self.run_abbr_command();
                } else {
                    self.cmd_text = text;
                }
            }
            Message::RunCmd => {
                self.run_abbr_command();
            }
            Message::Noop => {}
            Message::SelectWindowGroup(id) => {
                self.window_group_id = id;
            }
            Message::SelectActionType(type_id) => {
                // 先取变更信息（借用随之结束），再做可能的缩写联动
                let change = self
                    .current_action_mut()
                    .and_then(|action| action_editor::apply_type_change(action, type_id));
                if let Some(change) = change {
                    self.maybe_refresh_abbr_enable(
                        change.old_type,
                        change.new_type,
                        change.old_value,
                        0,
                    );
                }
            }
            Message::EditField(field) => {
                self.apply_field(field);
            }
            Message::SelectRadio {
                value_id,
                label_key,
            } => {
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
            Message::WindowSpy => {
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
            Message::Ready {
                config,
                port,
                doc_md,
                shortcuts,
                data_root,
                ui_prefs,
            } => {
                self.acrylic = ui_prefs.acrylic;
                self.nav = build_nav(&config);
                self.config = Some(*config);
                self.port = Some(port);
                self.doc_md = doc_md;
                self.shortcuts = shortcuts;
                self.data_root = data_root;
                // CLI 传输没有端口 ⇒ 登记本地静态站目录：指南图片走直读 + `source_data`、
                // 内部链接走 `file:///`。HTTP 模式**不登记** ⇒ 行为与改动前完全一致。
                if crate::services::transport::transport()
                    == crate::services::transport::Transport::Cli
                {
                    crate::ui::doc_assets::set_site_dir(
                        self.data_root
                            .as_ref()
                            .map(|root| root.join("bin").join("site")),
                    );
                }
                self.page_index = 0;
                self.loading = false;
                self.error = None;
                // 行为目录快照（选中动作页消费；失败时目录为空 = 下拉空，不阻断页面）
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.get_behaviors();
                    if let Some(value) = response.value {
                        Message::BehaviorsLoaded(Ok(Box::new(sa::Catalog {
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
            Message::Failed(reason) => {
                self.loading = false;
                self.error = Some(reason);
            }
            Message::Retry => {
                self.loading = true;
                self.error = None;
                self.notice = None;
                let slot = Arc::clone(&self.session);
                let _ = context.spawn_background(move |_token| load_backend(slot));
            }
            Message::Save => {
                let (Some(config), Some(port)) = (self.config.clone(), self.port) else {
                    return;
                };
                // 1 秒节流（复刻 `SaveCommand` 的 useThrottleFn(1000)）；开关等即时保存走
                // `save_now` 不受限流
                if let Some(last) = self.last_save
                    && last.elapsed() < Duration::from_secs(1)
                {
                    return;
                }
                self.last_save = Some(Instant::now());
                self.notice = None;
                let _ = context
                    .spawn_background(move |_token| Message::SaveFinished(save(port, &config)));
            }
            Message::ClearNotice => {
                if !self.notice_error {
                    self.notice = None;
                }
            }
            // 保存成功/失败都**停留在当前页**（旧版失败走模态弹窗、现场保留；此处以
            // 红色页脚提示等价承载），不再把用户踢到全页错误态。
            Message::SaveFinished(result) => match result {
                Ok(text) => {
                    self.hotkey_pending_save = false;
                    self.notice_error = false;
                    self.notice = Some(text);
                    // 保存成功 ⇒ 导航即时重建（方案启停等变化反映到侧栏，复刻 BuildNav）
                    self.rebuild_nav();
                    // QuickSwitch 草稿在保存确认后关闭弹窗（保存失败则保持打开供修正，
                    // 复刻旧 `QuickSwitchDialogWindow` 的「失败窗口不关」语义）
                    self.qs_draft = None;
                    self.qs_error = None;
                    self.sa_enable_prev = None;
                    self.schedule_notice_clear(context);
                }
                Err(reason) => {
                    self.notice_error = true;
                    self.notice = Some(reason);
                    // 启用开关保存失败 ⇒ 回滚显示态（复刻 `SaveEnableAsync`）
                    if let Some(previous) = self.sa_enable_prev.take()
                        && let Some(config) = self.config.as_mut()
                    {
                        config.selected_action.enable = previous;
                    }
                }
            },
            Message::Notice(result) => match result {
                Ok(text) => {
                    self.notice_error = false;
                    self.notice = Some(text);
                    self.schedule_notice_clear(context);
                }
                Err(reason) => {
                    self.notice_error = true;
                    self.notice = Some(reason);
                }
            },
            // ------------------------------------------------------------- 选中动作页
            Message::SaHotkey(text) => {
                if let Some(config) = self.config.as_mut() {
                    // UsedHotkeys 冲突校验（复刻 `SelectedActionPageViewModel`）：
                    // 输入与全部 keymap 的热键/触发键比对（不含选中动作自身旧值）
                    let mut occupied: Vec<String> = config
                        .keymaps
                        .iter()
                        .flat_map(|keymap| {
                            keymap
                                .hotkeys
                                .keys()
                                .cloned()
                                .chain(std::iter::once(keymap.hotkey.clone()))
                        })
                        .collect();
                    occupied.retain(|existing| !existing.is_empty());
                    let previous = config.selected_action.hotkey.clone();
                    let conflict = !text.is_empty()
                        && occupied.iter().any(|existing| {
                            existing.eq_ignore_ascii_case(&text)
                                && !existing.eq_ignore_ascii_case(&previous)
                        });
                    self.sa_hotkey_conflict = conflict;
                    config.selected_action.hotkey = text;
                    self.hotkey_pending_save = true;
                }
            }
            Message::SaEnable(enabled) => {
                // 记录保存前值：失败时回滚显示态（复刻 `SaveEnableAsync`）
                self.sa_enable_prev = self
                    .config
                    .as_ref()
                    .map(|config| config.selected_action.enable);
                if let Some(config) = self.config.as_mut() {
                    config.selected_action.enable = enabled;
                }
                // 启用开关 = 立即保存（对齐旧版 SaveEnableAsync 语义）
                self.save_now(context);
            }
            Message::SaSelectToggle { match_type, id } => match match_type {
                MATCH_TEXT_TYPE => self.sa_text_sel = Some(id),
                _ => self.sa_file_sel = Some(id),
            },
            Message::SaDeleteAsk => {
                let id = self
                    .sa_selected_id(MATCH_TEXT_TYPE)
                    .or_else(|| self.sa_selected_id(MATCH_FILE_EXT))
                    .unwrap_or_default();
                if id.is_empty() {
                    return;
                }
                // 仅已配置（存在 mapping）的类型可删；打开确认框（1109，确认才落盘）
                let has_mapping = self.config.as_ref().is_some_and(|config| {
                    [MATCH_TEXT_TYPE, MATCH_FILE_EXT].iter().any(|match_type| {
                        sa::find_mapping_for_type(config, match_type, &id).is_some()
                    })
                });
                self.sa_delete_confirm = has_mapping;
            }
            Message::SaDeleteCancelled => self.sa_delete_confirm = false,
            Message::SaDeleteConfirmed => {
                self.sa_delete_confirm = false;
                let mut removed = false;
                for match_type in [MATCH_TEXT_TYPE, MATCH_FILE_EXT] {
                    let id = self.sa_selected_id(match_type).unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    if let Some(config) = self.config.as_mut()
                        && let Some(position) =
                            sa::find_mapping_index_for_type(config, match_type, &id)
                    {
                        config.selected_action.mappings.remove(position);
                        removed = true;
                        self.reset_sa_selection(match_type);
                    }
                }
                if removed {
                    // 删除即保存（复刻旧版「确认后立即保存」）
                    self.save_now(context);
                }
            }
            Message::SaPlaySample => {
                let Some(port) = self.port else {
                    return;
                };
                // typeId = toggle id（内置特征值 / group:<name> / type:<id>，后端白名单同构）
                let Some(type_id) = self
                    .sa_selected_id(MATCH_TEXT_TYPE)
                    .or_else(|| self.sa_selected_id(MATCH_FILE_EXT))
                    .filter(|id| !id.is_empty())
                else {
                    return;
                };
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.play_selected_action(&type_id);
                    Message::SaPlayDone(if response.success {
                        Ok(())
                    } else {
                        Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status)))
                    })
                });
            }
            Message::SaPickBehavior { match_type, pick } => match match_type {
                MATCH_TEXT_TYPE => self.sa_text_pick = pick,
                _ => self.sa_file_pick = pick,
            },
            Message::SaAddBehavior { match_type } => {
                self.add_behavior(match_type);
                self.request_sa_throttled_save(context);
            }
            Message::SaRemoveEntry { match_type, index } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                    && mapping.entries.len() > 1
                {
                    // 至少保留一个行为（旧版 1108 语义；空 entries 会被后端 400 拒绝）
                    mapping.entries.remove(index);
                }
                self.save_now(context);
            }
            Message::SaEntrySwitch {
                match_type,
                index,
                pick,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                // 覆盖推导（catalog 不可变借用）→ 写回（可变借用）
                let replacement = {
                    let match_value = self.config.as_ref().and_then(|config| {
                        sa::find_mapping_for_type(config, match_type, &id).map(|mapping| {
                            (
                                mapping.match_value.clone(),
                                mapping.entries.get(index).cloned(),
                            )
                        })
                    });
                    match (self.config.as_ref(), match_value) {
                        (_, Some((match_value, Some(current)))) => {
                            let covering = sa::covering(&self.catalog, match_type, &match_value);
                            covering.get(pick).map(|pack| {
                                let behavior = pack.id.clone();
                                SelectedEntry {
                                    action_value: if self.catalog.is_no_value(&behavior) {
                                        String::new()
                                    } else {
                                        // 切换行为 ⇒ 重置为该行为默认模板（复刻 OnBehaviorChanged）
                                        self.catalog.default_template_for(&behavior)
                                    },
                                    ..current
                                }
                            })
                        }
                        _ => None,
                    }
                };
                if let Some(replacement) = replacement
                    && let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                {
                    mapping.entries[index] = replacement;
                }
                self.request_sa_throttled_save(context);
            }
            Message::SaEntryMove {
                match_type,
                index,
                delta,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                {
                    let target = index as i64 + i64::from(delta);
                    if target >= 0 && (target as usize) < mapping.entries.len() {
                        mapping.entries.swap(index, target as usize);
                    }
                }
                self.request_sa_throttled_save(context);
            }
            Message::SaEntryValue {
                match_type,
                index,
                value,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                    && let Some(entry) = mapping.entries.get_mut(index)
                {
                    entry.action_value = value;
                }
                self.request_sa_throttled_save(context);
            }
            Message::SaEntryWorkingDir {
                match_type,
                index,
                value,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                    && let Some(entry) = mapping.entries.get_mut(index)
                {
                    entry.working_dir = value;
                }
                self.request_sa_throttled_save(context);
            }
            Message::SaSaveThrottled => self.save_now(context),
            Message::SaPlayDone(result) => {
                self.sa_status = match result {
                    // 成功：引擎侧可见执行，不打扰（旧版 StatusText 仅承载失败）
                    Ok(()) => None,
                    Err(reason) => Some((reason, true)),
                };
            }
            Message::SaAddOpen => {
                self.sa_add = Some(SaAddDraft::default());
            }
            Message::SaHotkeyClear => {
                self.sa_hotkey_conflict = false;
                if let Some(config) = self.config.as_mut() {
                    config.selected_action.hotkey = String::new();
                }
                self.hotkey_pending_save = true;
            }
            Message::SaNewType { kind } => {
                self.mt_dialog = true;
                self.mt_status = None;
                self.mt_test_result = None;
                self.mt_test.clear();
                self.mt_draft = self
                    .config
                    .as_ref()
                    .map(|config| match_types_edit::MatchTypeDraft::new_draft(config, kind));
            }
            Message::SaAddCancel => self.sa_add = None,
            Message::SaAddType(pick) => {
                if let Some(draft) = self.sa_add.as_mut() {
                    draft.type_pick = pick;
                    draft.checked.clear();
                    draft.error = None;
                }
            }
            Message::SaAddToggle(index, checked) => {
                let Some(draft) = self.sa_add.as_mut() else {
                    return;
                };
                let Some(pick) = draft.type_pick else {
                    return;
                };
                let Some(config) = self.config.as_ref() else {
                    return;
                };
                let options = sa::add_type_options(config);
                let Some(option) = options.get(pick) else {
                    return;
                };
                let (match_type, match_value) = sa::add_target(config, &option.id);
                let covering = sa::covering(&self.catalog, &match_type, &match_value);
                let Some(pack) = covering.get(index) else {
                    return;
                };
                let behavior = pack.id.clone();
                if checked {
                    // 勾选序 = 菜单序；9 上限（复刻 BehaviorPickVm）
                    if draft.checked.len() < 9 && !draft.checked.contains(&behavior) {
                        draft.checked.push(behavior);
                    }
                } else {
                    draft.checked.retain(|existing| existing != &behavior);
                }
            }
            Message::SaAddConfirm => {
                let Some(draft) = self.sa_add.as_mut() else {
                    return;
                };
                let Some(pick) = draft.type_pick else {
                    return;
                };
                let Some(config) = self.config.as_ref() else {
                    return;
                };
                let options = sa::add_type_options(config);
                let Some(option) = options.get(pick) else {
                    return;
                };
                if draft.checked.is_empty() {
                    // CanConfirm：至少勾一个行为（1104_any = 「任意」文案即缺位提示）
                    draft.error = Some(i18n::t("1104_any"));
                    return;
                }
                let (match_type, match_value) = sa::add_target(config, &option.id);
                // 重复条件拦截（1115）：同 (matchType, matchValue) 已配置
                if sa::mapping_exists(config, &match_type, &match_value) {
                    draft.error = Some(i18n::t("1115"));
                    return;
                }
                let entries: Vec<SelectedEntry> = draft
                    .checked
                    .iter()
                    .map(|behavior| SelectedEntry {
                        behavior: behavior.clone(),
                        action_value: self.catalog.default_template_for(behavior),
                        ..Default::default()
                    })
                    .collect();
                let Some(config) = self.config.as_mut() else {
                    return;
                };
                config
                    .selected_action
                    .mappings
                    .push(crate::models::SelectedMapping {
                        match_type: match_type.clone(),
                        match_value,
                        entries,
                    });
                self.sa_add = None;
                self.save_now(context);
            }
            // ---------------------------------------------------------- 匹配类型管理
            Message::MatchTypesOpen => {
                self.mt_dialog = true;
                self.mt_status = None;
                self.mt_test_result = None;
                if self.mt_draft.is_none() {
                    self.mt_draft = self
                        .config
                        .as_ref()
                        .map(|config| match_types_edit::MatchTypeDraft::new_draft(config, "text"));
                }
            }
            Message::MatchTypesClose => {
                self.mt_dialog = false;
                self.mt_draft = None;
                self.mt_status = None;
                self.mt_test_result = None;
                self.mt_test.clear();
            }
            Message::MatchTypesPick(pick) => {
                let draft = pick.and_then(|index| {
                    self.config
                        .as_ref()
                        .and_then(|config| config.match_types.get(index))
                        .map(|mt| match_types_edit::MatchTypeDraft::from_existing(index, mt))
                });
                self.mt_pick_set(draft);
            }
            Message::MtNew => {
                let draft = self
                    .config
                    .as_ref()
                    .map(|config| match_types_edit::MatchTypeDraft::new_draft(config, "text"));
                self.mt_draft = draft;
                self.mt_status = None;
                self.mt_test_result = None;
                self.mt_test.clear();
            }
            Message::MtLabel(value) => self.mt_edit_draft(|draft| draft.label = value),
            Message::MtLabelEn(value) => self.mt_edit_draft(|draft| draft.label_en = value),
            Message::MtKind(index) => {
                let kind = if index == 1 { "fileExt" } else { "text" };
                self.mt_edit_draft(|draft| {
                    draft.kind = kind.to_string();
                    if kind == "fileExt" {
                        draft.rules.clear();
                    } else if draft.rules.is_empty() {
                        draft.rules.push(("contains".to_string(), String::new()));
                    }
                });
            }
            Message::MtRuleOp(rule, op_index) => {
                let ops = ["equals", "prefix", "suffix", "contains"];
                self.mt_edit_draft(|draft| {
                    if let Some(slot) = draft.rules.get_mut(rule)
                        && let Some(op) = ops.get(op_index)
                    {
                        slot.0 = (*op).to_string();
                    }
                });
            }
            Message::MtRuleValue(rule, value) => {
                self.mt_edit_draft(|draft| {
                    if let Some(slot) = draft.rules.get_mut(rule) {
                        slot.1 = value;
                    }
                });
            }
            Message::MtRuleAdd => self.mt_edit_draft(|draft| {
                draft.rules.push(("contains".to_string(), String::new()));
            }),
            Message::MtRuleRemove(rule) => {
                self.mt_edit_draft(|draft| {
                    if draft.rules.len() > 1 {
                        draft.rules.remove(rule);
                    }
                });
            }
            Message::MtExts(value) => self.mt_edit_draft(|draft| draft.exts = value),
            Message::MtSave(with_behavior) => {
                let Some(draft) = self.mt_draft.clone() else {
                    return;
                };
                if let Err(reason) = match_types_edit::validate(&draft) {
                    self.mt_status = Some((reason, true));
                    return;
                }
                let (match_type, match_value) = {
                    if self.config.is_none() {
                        return;
                    }
                    // 以写回后的 kind/引用推导目标（fileExt → 后缀串；text → type: 引用）
                    let kind_is_file = draft.kind == "fileExt";
                    let value = if kind_is_file {
                        sa::normalize_exts(&draft.exts).join(",")
                    } else {
                        draft.type_ref()
                    };
                    (
                        if kind_is_file {
                            sa::MATCH_FILE_EXT
                        } else {
                            sa::MATCH_TEXT_TYPE
                        },
                        value,
                    )
                };
                let mut config_holder = self.config.clone();
                let Some(config) = config_holder.as_mut() else {
                    return;
                };
                match_types_edit::apply(config, &draft);
                self.config = config_holder;
                self.mt_status = None;
                self.save_now(context);

                if with_behavior {
                    // 「保存并创建专属行为」：建一个强绑定本类型的行为包
                    // （基础动作取当前覆盖集首个的基础动作，模板留空由用户后续在行为库补）
                    let base_action = sa::covering(&self.catalog, match_type, &match_value)
                        .first()
                        .map(|pack| self.catalog.base_action_of(&pack.id))
                        .unwrap_or_else(|| "run".to_string());
                    let applies = if match_type == sa::MATCH_TEXT_TYPE {
                        crate::models::BehaviorAppliesTo {
                            kind: sa::MATCH_TEXT_TYPE.to_string(),
                            value: if draft.kind == "text" {
                                Some("plain".to_string())
                            } else {
                                Some(match_value.clone())
                            },
                            exts: None,
                            is_default: false,
                        }
                    } else {
                        crate::models::BehaviorAppliesTo {
                            kind: sa::MATCH_FILE_EXT.to_string(),
                            value: None,
                            exts: Some(sa::normalize_exts(&match_value)),
                            is_default: false,
                        }
                    };
                    let pack = crate::models::BehaviorPack {
                        id: draft.id.clone(),
                        name: draft.label.clone(),
                        spec_version: 1,
                        applies_to: vec![applies],
                        entry: crate::models::BehaviorEntry {
                            kind: "builtin".to_string(),
                            action: Some(base_action),
                            ..Default::default()
                        },
                        bound_type_id: Some(draft.type_ref()),
                        source: Some("user".to_string()),
                        ..Default::default()
                    };
                    if let Some(port) = self.port {
                        let _ = context.spawn_background(move |_token| {
                            let api = crate::services::transport::new_settings_api(port);
                            let response = api.create_behavior(&pack);
                            match response.success {
                                true => Message::BhReloaded(Ok(())),
                                false => Message::BhReloaded(Err(response
                                    .error_message
                                    .unwrap_or_else(|| format!("HTTP {}", response.status)))),
                            }
                        });
                    }
                }
                // 重新载入草稿为「编辑既有」态（新建后 id 已落库）
                let index = self
                    .config
                    .as_ref()
                    .and_then(|config| config.match_types.iter().position(|mt| mt.id == draft.id));
                self.mt_pick_set(index.map(|index| {
                    match_types_edit::MatchTypeDraft::from_existing(
                        index,
                        &self.config.as_ref().unwrap().match_types[index],
                    )
                }));
            }
            Message::MtDelete => {
                let Some(draft) = self.mt_draft.clone() else {
                    return;
                };
                if draft.index == match_types_edit::NEW_INDEX {
                    return;
                }
                let mut config_holder = self.config.clone();
                let deleted = config_holder
                    .as_mut()
                    .and_then(|config| match_types_edit::delete(config, draft.index));
                let Some(type_id) = deleted else {
                    return;
                };
                self.config = config_holder;
                // 级联删同名专属行为包（bound_type_id 命中）
                if let Some(port) = self.port {
                    let type_ref = format!("type:{type_id}");
                    let bound = self
                        .catalog
                        .user
                        .iter()
                        .find(|pack| pack.bound_type_id.as_deref() == Some(type_ref.as_str()))
                        .map(|pack| pack.id.clone());
                    if let Some(behavior_id) = bound {
                        let _ = context.spawn_background(move |_token| {
                            let api = crate::services::transport::new_settings_api(port);
                            let _ = api.delete_behavior(&behavior_id);
                            let _ = api.apply_behaviors();
                            Message::Noop
                        });
                    }
                }
                self.mt_draft = None;
                self.save_now(context);
            }
            Message::MtTest(content) => {
                self.mt_test = content;
            }
            Message::MtTestRun => {
                let Some(port) = self.port else {
                    return;
                };
                if self.mt_test.trim().is_empty() {
                    self.mt_test_result = Some(Err(i18n::t("2570")));
                    return;
                }
                // 编辑中快照：把草稿 apply 进克隆配置（未保存的类型也能参与匹配，
                // 复刻 Go 端 selectedActionTestRequest 的 snapshot-priority 语义）
                let Some(draft) = self.mt_draft.clone() else {
                    return;
                };
                let mut snapshot = self.config.clone().unwrap_or_default();
                match_types_edit::apply(&mut snapshot, &draft);
                let is_file = draft.kind == "fileExt";
                let match_types = snapshot.match_types.clone();
                let selected_action = Some(snapshot.selected_action.clone());
                let content = self.mt_test.clone();
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.test_selected_action(
                        &content,
                        is_file,
                        selected_action.as_ref(),
                        Some(&match_types),
                    );
                    Message::MtTestDone(match (response.success, response.value) {
                        (true, Some(value)) if value.matched => {
                            Ok(value.preview.unwrap_or_default())
                        }
                        (true, _) => Ok(String::new()),
                        (_, _) => Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status))),
                    })
                });
            }
            Message::MtTestDone(result) => {
                self.mt_test_result = Some(result);
            }
            // ---------------------------------------------------------- 行为库
            Message::BehaviorsOpen => {
                self.bh_dialog = true;
                self.bh_status = None;
                self.bh_pick = None;
                self.bh_draft = None;
            }
            Message::BehaviorsClose => {
                self.bh_dialog = false;
                self.bh_pick = None;
                self.bh_draft = None;
                self.bh_status = None;
            }
            Message::BhPick(pick) => {
                self.bh_pick = pick;
                let merged: Vec<&crate::models::BehaviorPack> = self.catalog.packs().collect();
                self.bh_draft = pick.and_then(|index| {
                    merged.get(index).map(|pack| {
                        let mut draft = behaviors_edit::BehaviorDraft::from_pack(pack);
                        draft.index = index;
                        draft
                    })
                });
                self.bh_status = None;
            }
            Message::BhNew => {
                self.bh_draft = Some(behaviors_edit::BehaviorDraft::new_draft());
                self.bh_pick = None;
                self.bh_status = None;
            }
            Message::BhName(value) => self.bh_edit_draft(|draft| draft.name = value),
            Message::BhId(value) => self.bh_edit_draft(|draft| draft.id = value),
            Message::BhDescription(value) => self.bh_edit_draft(|draft| draft.description = value),
            Message::BhAppliesKind(row, index) => {
                let kind = if index == 1 { "fileExt" } else { "textType" };
                self.bh_edit_draft(|draft| {
                    if let Some(slot) = draft.applies.get_mut(row) {
                        slot.kind = kind.to_string();
                    }
                });
            }
            Message::BhAppliesValue(row, value) => {
                self.bh_edit_draft(|draft| {
                    if let Some(slot) = draft.applies.get_mut(row) {
                        slot.value = value;
                    }
                });
            }
            Message::BhAppliesDefault(row, value) => {
                self.bh_edit_draft(|draft| {
                    if let Some(slot) = draft.applies.get_mut(row) {
                        slot.is_default = value;
                    }
                });
            }
            Message::BhAppliesAdd => self.bh_edit_draft(|draft| {
                draft.applies.push(behaviors_edit::AppliesDraft {
                    kind: "textType".to_string(),
                    ..Default::default()
                });
            }),
            Message::BhAppliesRemove(row) => self.bh_edit_draft(|draft| {
                if draft.applies.len() > 1 {
                    draft.applies.remove(row);
                }
            }),
            Message::BhBaseAction(index) => {
                let options = behaviors_edit::base_action_options(&self.catalog);
                self.bh_edit_draft(|draft| {
                    if let Some(action) = options.get(index) {
                        draft.base_action = action.clone();
                    }
                });
            }
            Message::BhTemplate(value) => self.bh_edit_draft(|draft| draft.template = value),
            Message::BhWorkingDir(value) => self.bh_edit_draft(|draft| draft.working_dir = value),
            Message::BhSave => {
                let Some(draft) = self.bh_draft.clone() else {
                    return;
                };
                let is_new = draft.index == behaviors_edit::NEW_INDEX;
                if !is_new {
                    // 内置包只读（1103_only；后端也会 404 拒绝）
                    let is_builtin = self.catalog.builtin.iter().any(|pack| pack.id == draft.id);
                    if is_builtin {
                        self.bh_status = Some((i18n::t("1103_only"), true));
                        return;
                    }
                }
                if let Err(reason) = behaviors_edit::validate(&draft) {
                    self.bh_status = Some((reason, true));
                    return;
                }
                let Some(port) = self.port else {
                    return;
                };
                let pack = draft.to_pack();
                let id = pack.id.clone();
                self.bh_saving = true;
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = if is_new {
                        api.create_behavior(&pack)
                    } else {
                        api.update_behavior(&id, &pack)
                    };
                    Message::BhSaved(if response.success {
                        Ok(())
                    } else {
                        Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status)))
                    })
                });
            }
            Message::BhSaved(result) => {
                self.bh_saving = false;
                match result {
                    Ok(()) => {
                        self.bh_status = None;
                        self.reload_catalog(context);
                    }
                    Err(reason) => self.bh_status = Some((reason, true)),
                }
            }
            Message::BhDelete => {
                let Some(draft) = self.bh_draft.clone() else {
                    return;
                };
                let Some(port) = self.port else {
                    return;
                };
                let id = draft.id.clone();
                self.bh_saving = true;
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.delete_behavior(&id);
                    Message::BhSaved(if response.success {
                        Ok(())
                    } else {
                        Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status)))
                    })
                });
            }
            Message::BhApplyNow => {
                let Some(port) = self.port else {
                    return;
                };
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.apply_behaviors();
                    Message::BhApplied(if response.success {
                        Ok(())
                    } else {
                        Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status)))
                    })
                });
            }
            Message::BhApplied(result) => match result {
                // 1101_applied 已含「若未恢复请托盘重载」引导
                Ok(()) => self.bh_status = Some((i18n::t("1101_applied"), false)),
                Err(reason) => self.bh_status = Some((reason, true)),
            },
            Message::BhReloaded(result) => {
                if let Err(reason) = result {
                    self.bh_status = Some((reason, true));
                }
                self.reload_catalog(context);
            }
            // ---------------------------------------------------------- 指南编辑
            Message::GuideEditOpen => {
                self.guide_edit_text = self.doc_md.clone();
                self.guide_edit_open = true;
            }
            Message::GuideEditValue(value) => self.guide_edit_text = value,
            Message::GuideEditReset => {
                // 复刻旧 `OverviewEditWindow`：清空 = 恢复默认文档（引擎回落站内 config_doc.md）
                self.guide_edit_text.clear();
                if let Some(config) = self.config.as_mut() {
                    config.overview_doc_md = String::new();
                }
                self.doc_md.clear();
                self.guide_edit_open = false;
                self.save_now(context);
            }
            Message::GuideEditClose => self.guide_edit_open = false,
            Message::GuideEditSave => {
                let text = std::mem::take(&mut self.guide_edit_text);
                self.doc_md = text.clone();
                self.guide_edit_open = false;
                if let Some(config) = self.config.as_mut() {
                    config.overview_doc_md = text;
                }
                self.save_now(context);
            }
            // ---------------------------------------------------------- 自定义热键动作编辑
            Message::CustomHotkeyEdit(row) => {
                // 编辑目标切换为 keymap id=1 的第 row 行（current_keymap_id 的 override）
                let hotkey = self
                    .config
                    .as_ref()
                    .and_then(|config| config.keymaps.iter().find(|km| km.id == 1))
                    .and_then(|keymap| keymap.hotkeys.keys().nth(row).cloned());
                let Some(hotkey) = hotkey else {
                    return;
                };
                self.hotkey_editor_row = Some(row);
                self.selected_hotkey = Some(hotkey);
                self.window_group_id = 0;
            }
            Message::CustomHotkeyEditClose => {
                self.hotkey_editor_row = None;
                self.selected_hotkey = None;
            }
            // ---------------------------------------------------------- 插件页
            Message::PluginsReload => {
                self.reload_plugins(context);
            }
            Message::PluginsLoaded(result) => {
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
            Message::PluginToggle {
                id,
                is_builtin,
                enabled,
            } => {
                let changed = self
                    .config
                    .as_mut()
                    .map(|config| plugins::apply_enabled(config, &id, is_builtin, enabled))
                    .unwrap_or(false);
                if changed {
                    // 开关即保存（旧版 `SaveAsync(force: true)`：PUT /config 会重生成脚本并重启引擎）
                    self.save_now(context);
                }
            }
            Message::PluginDelete(id) => {
                if let Some(port) = self.port {
                    let _ = context.spawn_background(move |_token| {
                        let api = crate::services::transport::new_settings_api(port);
                        let response = api.delete_plugin(&id);
                        Message::PluginDeleted {
                            id,
                            result: if response.success {
                                Ok(())
                            } else {
                                Err(response
                                    .error_message
                                    .unwrap_or_else(|| format!("HTTP {}", response.status)))
                            },
                        }
                    });
                }
            }
            Message::PluginDeleted { id, result } => match result {
                Ok(()) => {
                    // 注册表孤儿项清理（旧版行为包先例：config 变更统一走保存链路）
                    let cleaned = self
                        .config
                        .as_mut()
                        .map(|config| plugins::remove_from_registry(config, &id))
                        .unwrap_or(false);
                    if cleaned {
                        self.save_now(context);
                    }
                    self.reload_plugins(context);
                }
                Err(reason) => {
                    self.plugins_action_error = Some(format!("{}: {reason}", i18n::t("2436")));
                }
            },
            Message::PluginImport => {
                let Some(port) = self.port else {
                    return;
                };
                // 文件选择是**同步模态**对话（必须在 UI 线程弹出）；选定后再把字节交给后台 POST。
                // 过滤器显示名走 i18n 2437（旧版 `KeyFlux 插件包`），不再硬编码中文。
                let picked = platform::file_dialog::pick_open_file(
                    &i18n::t("2427"),
                    &format!("{}\0*.zip\0\0", i18n::t("2437")),
                );
                let Some(path) = picked else {
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

                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.import_plugin(&bytes, &file_name);
                    Message::PluginImported(match response.value {
                        Some(manifest) => Ok(manifest.name),
                        None => Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status))),
                    })
                });
            }
            Message::PluginImported(result) => match result {
                Ok(name) => {
                    self.plugins_error = None;
                    self.plugins_action_error = None;
                    self.plugin_status = Some(i18n::t_fmt("2431", &[&name]));
                    // 刷新**保留**刚设的成功回显（此前 reload 先清 status ⇒ 横幅永远看不到）
                    self.refresh_plugins(context);
                }
                Err(reason) => {
                    self.plugins_action_error = Some(format!("{}: {reason}", i18n::t("2432")));
                }
            },
            // ---------------------------------------------------------- 插件市场
            Message::PluginsMarket => {
                self.market_open = true;
                self.market_status = None;
                self.reload_market(context);
            }
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
                let Some(port) = self.port else {
                    return;
                };
                self.market_installing = Some(id.clone());
                self.market_error = None;
                let _ = context.spawn_background(move |_token| {
                    // 客户端下载 zip → 复用本地导入链路（后端不出网）
                    let outcome = market::download_zip(&url).and_then(|bytes| {
                        let api = crate::services::transport::new_settings_api(port);
                        let response = api.import_plugin(&bytes, &format!("{id}.zip"));
                        match response.value {
                            Some(manifest) => Ok(manifest.name),
                            None => Err(response
                                .error_message
                                .unwrap_or_else(|| format!("HTTP {}", response.status))),
                        }
                    });
                    Message::MarketInstalled {
                        id,
                        result: outcome,
                    }
                });
            }
            Message::MarketInstalled { id, result } => {
                self.market_installing = None;
                match result {
                    Ok(name) => {
                        self.market_status = Some(i18n::t_fmt("2431", &[&name]));
                        if let Some(entry) = self.market_entries.iter_mut().find(|e| e.id == id) {
                            entry.is_installed = true;
                        }
                        // 插件页同步刷新（新装的插件应出现在列表里）
                        self.refresh_plugins(context);
                    }
                    Err(reason) => {
                        self.market_error = Some(format!("{}: {reason}", i18n::t("2432")));
                    }
                }
            }
            Message::MarketClosed => {
                self.market_open = false;
                self.market_installing = None;
                // 关闭市场后无条件刷新插件页（复刻 `OnMarketClosed` 的 ReloadAsync）
                self.refresh_plugins(context);
            }
            // ---------------------------------------------------------- 插件设置对话框
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
                if self.ps_saving {
                    return; // 保存进行中，防重复提交
                }
                // 本地即时校验（后端仍是权威；失败 → 弹窗保持打开供修正）
                let english = matches!(i18n::language(), i18n::Lang::En);
                for (setting, value) in &self.ps_rows {
                    if let Some(reason) = plugins::validate_setting(setting, value) {
                        let label = plugins::setting_label(setting, english);
                        self.ps_error = Some(format!("{}: {label}: {reason}", i18n::t("2585")));
                        self.ps_open = true;
                        return;
                    }
                }
                let Some(port) = self.port else {
                    return;
                };
                let Some(id) = self.ps_id.clone() else {
                    return;
                };
                let values: BTreeMap<String, String> = self
                    .ps_rows
                    .iter()
                    .map(|(setting, value)| (setting.key.clone(), value.clone()))
                    .collect();
                // 复刻旧版「保存期间窗口保持打开，后端拒绝也不关」：弹窗保持，
                // `PsSaved(Ok)` 才关闭（Err 时 ps_error 已在弹窗内显示）
                self.ps_saving = true;
                self.ps_error = None;
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.save_plugin_settings(&id, &values);
                    Message::PsSaved(if response.success {
                        Ok(())
                    } else {
                        Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status)))
                    })
                });
            }
            Message::PsSaved(result) => {
                self.ps_saving = false;
                match result {
                    Ok(()) => {
                        self.ps_open = false;
                        self.ps_error = None;
                    }
                    Err(reason) => {
                        // 后端拒绝 → 弹窗保持打开供修正（值未丢）
                        self.ps_error = Some(format!("{}: {reason}", i18n::t("2585")));
                        self.ps_open = true;
                    }
                }
            }
            Message::PluginConfigure(id) => {
                if id == plugins::QUICK_SWITCH_ID {
                    // 打开即深拷贝出草稿（副本编辑，取消不影响真源）
                    self.qs_draft = self.config.as_ref().map(plugins::draft_from);
                    self.plugin_status = None;
                } else {
                    // 声明式设置：打开即拉取「声明 + 默认值合并后的完整值表」
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
            // ---------------------------------------------------------- QuickSwitch 对话框
            Message::QsEdit(field) => {
                if let Some(draft) = self.qs_draft.as_mut() {
                    match field {
                        QsField::CollectEnabled(value) => draft.collect_enabled = value,
                        QsField::AutoShow(value) => draft.auto_show = value,
                        QsField::AutoJumpOpen(value) => draft.auto_jump_open = value,
                        QsField::AutoJumpSave(value) => draft.auto_jump_save = value,
                        QsField::MaxHistory(value) => draft.max_history = value,
                        QsField::OverlayRows(value) => draft.overlay_rows = value,
                        QsField::OverlayRowsCompact(value) => draft.overlay_rows_compact = value,
                        QsField::PollIntervalMs(value) => draft.poll_interval_ms = value,
                    }
                }
            }
            Message::QsAddPrefix => {
                if let Some(draft) = self.qs_draft.as_mut() {
                    draft.excluded_prefixes.push(String::new());
                }
            }
            Message::QsRemovePrefix(index) => {
                if let Some(draft) = self.qs_draft.as_mut()
                    && index < draft.excluded_prefixes.len()
                {
                    draft.excluded_prefixes.remove(index);
                }
            }
            Message::QsPrefix(index, value) => {
                if let Some(draft) = self.qs_draft.as_mut()
                    && let Some(slot) = draft.excluded_prefixes.get_mut(index)
                {
                    *slot = value;
                }
            }
            Message::QsClearHistory => {
                // 一次性动作：截断 <部署根>/data/quickswitch/history.tsv（文件保留）。
                // 错误写进**弹窗内部**（此前写页面横幅，被打开中的 ContentDialog 遮蔽）
                let outcome = match &self.data_root {
                    Some(root) => plugins::clear_history(root),
                    None => Err("未确定部署根目录".to_string()),
                };
                if let Err(reason) = outcome {
                    self.qs_error = Some(format!("{}: {reason}", i18n::t("2417")));
                }
            }
            Message::QsClosed(result) => {
                if result != ContentDialogResult::Primary {
                    self.qs_draft = None;
                    self.qs_error = None;
                    return;
                }
                if self.qs_draft.is_none() {
                    return;
                }
                // 复刻旧版语义：保存期间**草稿保留、弹窗保持打开**，`SaveFinished(Ok)`
                // 才关闭（见其 Ok 分支清除 `qs_draft`）；无变更时直接丢弃草稿关闭。
                let changed = self
                    .qs_draft
                    .as_ref()
                    .and_then(|draft| {
                        self.config
                            .as_mut()
                            .map(|config| plugins::commit_draft(config, draft))
                    })
                    .unwrap_or(false);
                if changed {
                    self.qs_error = None;
                    self.save_now(context);
                } else {
                    self.qs_draft = None;
                    self.qs_error = None;
                }
            }
            // ---------------------------------------------------------- 选项页
            Message::SettingsSection(section) => {
                // 一次只展开一张（复刻旧版手风琴）：再点已展开的则收起
                self.settings_open = if self.settings_open == Some(section) {
                    None
                } else {
                    Some(section)
                };
            }
            Message::StartupToggle(enabled) => {
                if let Some(config) = self.config.as_mut() {
                    settings::set_startup(config, enabled);
                }
                if let Some(port) = self.port {
                    let command_id = settings::startup_command_id(enabled);
                    let _ = context.spawn_background(move |_token| {
                        let api = crate::services::transport::new_settings_api(port);
                        let response = api.send_server_command(command_id);
                        Message::StartupDone(if response.success {
                            Ok(())
                        } else {
                            Err(response
                                .error_message
                                .unwrap_or_else(|| format!("HTTP {}", response.status)))
                        })
                    });
                }
            }
            Message::StartupDone(result) => match result {
                Ok(()) => self.settings_notice = None,
                Err(reason) => self.settings_notice = Some(reason),
            },
            Message::Opt(edit) => self.apply_opt(edit),
            Message::DelayScheme(index) => self.delay_scheme = index,
            Message::FontBrowse => {
                let picked = platform::file_dialog::pick_open_file(
                    &i18n::t("2504"),
                    platform::file_dialog::FONT_FILTER,
                );
                if let Some(path) = picked
                    && let Some(config) = self.config.as_mut()
                {
                    config.options.command_font.source_path = path.to_string_lossy().into_owned();
                }
            }
            Message::BehaviorsLoaded(result) => match result {
                Ok(catalog) => {
                    self.catalog = *catalog;
                    // 目录到达后重置两卡选中（covering 列表会变）
                }
                Err(_reason) => {
                    // 目录拉取失败：页面仍可用（下拉为空）；对话框内失败经 bh_status 呈现
                }
            },
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        // 毛玻璃策略同步（唯一写入点）：开启时自绘表面按 alpha 稀释，材质透出。
        glass::set_enabled(self.acrylic);
        // 窗口视觉随状态派生（背板材质的 select 语义收在 `platform::WindowSpec`）。
        let spec = WindowSpec::default().with_glass(self.acrylic);
        context.window_visuals(spec.visuals());
        context.window_title(&spec.title);

        // 自绘标题栏：`grid_row` 必须在收尾（`.into()`）之前设置。
        // ⚠️ 高度用 Standard（32px）：Tall（48px）的三键会超出 TitleBar 行的分隔线
        // （用户实测截图）；Standard 与行内分隔线对齐，且与常规桌面应用观感一致。
        let title_bar: View = TitleBar::new()
            .grid_row(0)
            .title("KeyFlux 设置面板")
            .preferred_height(WindowTitleBarHeight::Standard)
            .into();

        // 导航项：tag 驱动选中匹配，文案来自配置/ i18n。
        let menu: Vec<(String, View)> = self
            .nav
            .iter()
            .map(|entry| {
                let item: View = NavigationViewItem::new().tag(entry.tag.clone()).slots([
                    SlotView::new(
                        NavigationViewItemSlot::Icon,
                        FontIcon::new().glyph(entry.kind.glyph()),
                    ),
                    SlotView::new(
                        NavigationViewItemSlot::Content,
                        TextBlock::new().text(entry.label.clone()),
                    ),
                ]);
                (entry.tag.clone(), item)
            })
            .collect();

        let nav: View = NavigationView::new()
            .grid_row(1)
            .pane_title("KeyFlux")
            // 侧栏宽度：WinUI NavigationView 默认 OpenPaneLength=320，明显宽于旧设计
            // （旧 Avalonia `ColumnDefinitions="264,*"` ⇒ theme::SIDEBAR_WIDTH=264）⇒ 显式收紧。
            .open_pane_length(theme::SIDEBAR_WIDTH)
            // 展开/折叠由**状态驱动显示模式切换**（实测口径，2026-09-29/30）：
            //  * 展开态 = Left + IsPaneOpen(true)：带文字并推开内容（原默认观感）；
            //  * 折叠态 = LeftCompact + IsPaneOpen(false)：~48px 图标窄轨、内容区铺满。
            //  为什么不只用 Left + IsPaneOpen(false)：该路径只隐藏条目文字、宽度不缩、
            //  内容不左移；为什么不只用 LeftCompact + IsPaneOpen(true)：本环境**不会**
            //  展开浮层（探针构建初始 true 仍渲染为窄轨）。
            //  ⚠️ is_pane_open 必须受控于状态：写死会在事件回写后的重渲染中把原生刚置上
            //  的值打回，表现为「点汉堡毫无反应」。
            .pane_display_mode(if self.pane_overlay_open {
                NavigationViewPaneDisplayMode::Left
            } else {
                NavigationViewPaneDisplayMode::LeftCompact
            })
            .is_pane_open(self.pane_overlay_open)
            // 亚克力：覆盖 NavigationView 的三处主题资源（内容区透明 + 窗格轻玻璃）——
            // 框架自带的内容区不透明底会盖住系统背板材质（MS Learn 官方口径：
            // 「不要给 Window / NavigationView / 页面 Grid 设不透明背景」）。
            .resource_overrides(self.acrylic.then(glass::navigation_glass_resources))
            .on_is_pane_open_changed(context.callback(Message::PaneOverlay))
            .is_settings_visible(false)
            .on_selected_tag_changed(context.callback(|tag: Option<String>| Message::Nav(tag)))
            .slots([
                SlotView::collection(NavigationViewSlot::MenuItems, menu),
                SlotView::new(NavigationViewSlot::PaneFooter, self.pane_footer(context)),
                SlotView::new(NavigationViewSlot::Content, self.content(context)),
            ]);

        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((
                title_bar,
                nav,
                self.quick_switch_dialog(context),
                self.market_dialog(context),
                self.plugin_settings_dialog(context),
                self.sa_delete_dialog(context),
                self.sa_add_dialog(context),
                self.sa_match_types_dialog(context),
                self.sa_behaviors_dialog(context),
                self.guide_edit_dialog(context),
                self.custom_hotkey_dialog(context),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Keymap, Options};

    fn keymap(id: i32, name: &str, hotkey: &str, enable: bool) -> Keymap {
        Keymap {
            id,
            name: name.to_string(),
            hotkey: hotkey.to_string(),
            enable,
            ..Default::default()
        }
    }

    #[test]
    fn nav_starts_with_three_fixed_entries() {
        let config = Config::default();
        let nav = build_nav(&config);
        assert_eq!(nav.len(), 3);
        assert_eq!(nav[0].tag, "home");
        assert_eq!(nav[1].tag, "action");
        assert_eq!(nav[2].tag, "plugins");
        assert_eq!(nav[0].kind, PageKind::Guide);
        assert_eq!(nav[1].kind, PageKind::SelectedAction);
        assert_eq!(nav[2].kind, PageKind::Plugins);
    }

    #[test]
    fn nav_includes_enabled_keymaps_except_id_one() {
        let config = Config {
            options: Options::default(),
            keymaps: vec![
                keymap(1, "Custom", "f1", true),   // id=1 不入导航
                keymap(5, "我的模式", "j", true),  // 自定义 name
                keymap(6, "", "k", true),          // name 空 ⇒ 用 hotkey
                keymap(7, "禁用模式", "l", false), // 未启用 ⇒ 不入导航
            ],
            ..Default::default()
        };
        let nav = build_nav(&config);
        let tags: Vec<&str> = nav.iter().map(|entry| entry.tag.as_str()).collect();
        assert_eq!(
            tags,
            vec!["home", "action", "plugins", "keymap-5", "keymap-6"]
        );
        assert_eq!(nav[3].label, "我的模式");
        assert_eq!(nav[4].label, "k", "name 为空时回退 hotkey");
        assert_eq!(nav[3].kind, PageKind::Keymap(5));
    }

    #[test]
    fn nav_routes_keymap_ids_to_page_kinds() {
        let config = Config {
            keymaps: vec![
                keymap(2, "Command", "caps", true),
                keymap(3, "Abbreviation", "tab", true),
                keymap(4, "Settings", "f12", true),
            ],
            ..Default::default()
        };
        let nav = build_nav(&config);
        assert_eq!(nav[3].kind, PageKind::Abbr(2));
        assert_eq!(nav[4].kind, PageKind::Abbr(3));
        assert_eq!(nav[5].kind, PageKind::Settings);
        // id=4 的标题固定取 i18n（不吃 config name）
        assert_ne!(nav[5].label, "Settings");
    }

    #[test]
    fn nav_is_empty_of_keymaps_when_all_disabled() {
        let config = Config {
            keymaps: vec![keymap(5, "A", "j", false), keymap(6, "B", "k", false)],
            ..Default::default()
        };
        assert_eq!(build_nav(&config).len(), 3);
    }
}
