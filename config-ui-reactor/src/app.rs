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
//! * 弹窗现统一走 `ContentDialog`（旧「会崩溃」结论已被 2026-10 的生产使用推翻）。
//! * `open_window` 通道保留但全仓零调用。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use windows_reactor::*;

use crate::models::SelectedEntry;
use crate::models::{
    Action, Config, Keymap, PluginListResponse, PluginSetting, PluginSettingsResponse,
};
use crate::platform::glass;
use crate::platform::{self, WindowSpec};
use crate::services::abbr;
use crate::services::api::{ApiResponse, SettingsApi};
use crate::services::backend::{BackendSession, BackendSessionOptions, resolve_settings_exe};
use crate::services::market::{self, MarketEntry};
use crate::services::selected_action::{self as sa, MATCH_FILE_EXT, MATCH_TEXT_TYPE};
use crate::services::{
    action_editor, behaviors_edit, i18n, keymap, markdown, match_types_edit, plugins,
    save_pipeline, settings, store,
};
use crate::theme;
use crate::ui::{
    abbr_view, action_editor as action_editor_view, keymap_view, markdown_view, plugins_view,
    selected_action_view, settings_view,
};

// 拆分后的子模块（原 app.rs 的 impl Shell 与类型定义按职责外移，见各文件头注）。
mod dialogs;
mod glue;
mod handlers;
mod messages;
// 页面装配（2026-10-08 自 views.rs 再拆；一个页面一个文件，见 pages/mod.rs 头注）。
mod pages;
mod state;
mod views;

use glue::*;
use messages::*;

/// 批 W（2026-10-08）：测试专用 `Default` —— `Shell` 是纯数据（无窗口句柄；
/// `session` 为 `Arc<Mutex<Option<_>>>`，None 即可），测试可先构造空白实例再按需
/// 置字段。`cfg_attr` 保证 release 构建零影响。
#[cfg_attr(test, derive(Default))]
pub struct Shell {
    nav: Vec<NavEntry>,
    page_index: usize,
    loading: bool,
    error: Option<String>,
    notice: Option<String>,
    /// 窗口拾取会话进行中（准星按钮防重入 + 置灰；会话经 spawn_background 阻塞线程）。
    picking: bool,
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
    /// 插件市场对话框是否打开。
    market_open: bool,
    /// 市场条目（目录序）。
    market_entries: Vec<MarketEntry>,
    /// 市场目录拉取中。
    market_loading: bool,
    /// 市场目录拉取失败原因。
    market_error: Option<String>,
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
    /// 「删除映射」确认框是否打开（1109 文案）。
    sa_delete_confirm: bool,
    /// 选中动作页页内状态条（▶ 执行失败等；`(文本, 是否错误)`）。
    sa_status: Option<(String, bool)>,
    /// 「添加映射」弹窗草稿（`None` = 关闭）。
    sa_add: Option<SaAddDraft>,
    /// 键位/缩写页右侧备注汇总是否折叠（默认展开；折叠以优先保证键盘网格完整显示）。
    comments_collapsed: bool,
    /// 选项页「亚克力毛玻璃效果」状态（持久化于面板私有偏好文件 `data/ui-prefs.json`）。
    acrylic: bool,
    /// 导航窗格是否展开（true = Left 模式带文字并推开内容；false = LeftCompact 图标窄轨）。
    /// 默认 true（保持原有「展开带文字」的默认观感），汉堡点击后由事件回写翻转。
    pane_overlay_open: bool,
    /// 「管理匹配类型」对话框是否打开。
    mt_dialog: bool,
    /// 文件后缀卡内联后缀编辑器：(分组下标, 未落盘文本)——None = 干净态。
    exts_edit: Option<(usize, String)>,
    /// 尾随归一代际（800ms 防抖只做**内存**归一，落盘统一走页脚保存）。
    exts_save_gen: std::sync::Arc<std::sync::atomic::AtomicU64>,
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
    bh_selected: Option<usize>,
    /// 行为编辑草稿（`None` = 展示只读详情占位）。
    bh_draft: Option<behaviors_edit::BehaviorDraft>,
    /// 行为对话框内状态条。
    bh_status: Option<(String, bool)>,
    /// 「编辑使用指南」对话框是否打开。
    guide_edit_open: bool,
    /// 指南编辑框内容。
    guide_edit_text: String,
    /// 自定义热键动作编辑对话框：当前编辑的行（`Some(row)` = 打开，keymap id=1）。
    hotkey_editor_row: Option<usize>,
    /// 暂存变更队列（保存策略：一切修改先入队，页脚「保存配置」统一提交）。
    pending: save_pipeline::PendingQueue,
    /// 选项页当前展开的分区（`None` = 全部收起；一次只展开一张，复刻旧版手风琴）。
    settings_open: Option<&'static str>,
    /// 选项页「触发延时」分区当前选中的方案（`nav` 中 id>4 方案的下标）。
    delay_scheme: usize,
    /// 选项页一次性提示（开机自启结果 / 保存校验失败原因）。
    settings_notice: Option<String>,
    /// 部署根路径（`<deploy>`；用于 acrylic 偏好落盘（ui-prefs.json）与 CLI 模式静态站目录定位）。
    data_root: Option<std::path::PathBuf>,
    /// 行为目录快照（选中动作页；`GET /api/behaviors`）。
    catalog: sa::SaCatalog,
    /// 选中动作页：文本卡当前点亮的 toggle id。
    sa_text_sel: Option<String>,
    /// 选中动作页：文件卡当前点亮的 toggle id。
    sa_file_sel: Option<String>,
    /// 选中动作页：文本卡「添加行为」下拉当前索引。
    sa_text_selected: Option<usize>,
    /// 选中动作页：文件卡「添加行为」下拉当前索引。
    sa_file_selected: Option<usize>,
    /// 选中动作页：热键已改未保存（保存成功后复位，对齐旧版 `HotkeyPendingSave`）。
    hotkey_pending_save: bool,
    /// 选中动作页热键与既有占用集冲突（1025 红字提示）。
    sa_hotkey_conflict: bool,
    /// 当前窗口分组（复刻 `store.windowGroupID`）。
    window_group_id: i32,
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
            picking: false,
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
            market_open: false,
            market_entries: Vec::new(),
            market_loading: false,
            market_error: None,
            market_status: None,
            ps_id: None,
            ps_title: String::new(),
            ps_rows: Vec::new(),
            ps_loading: false,
            ps_error: None,
            ps_open: false,
            sa_delete_confirm: false,
            sa_status: None,
            sa_add: None,
            comments_collapsed: false,
            acrylic: false,
            pane_overlay_open: true,
            mt_dialog: false,
            exts_edit: None,
            exts_save_gen: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mt_draft: None,
            mt_status: None,
            mt_test: String::new(),
            mt_test_result: None,
            bh_dialog: false,
            bh_selected: None,
            bh_draft: None,
            bh_status: None,
            guide_edit_open: false,
            guide_edit_text: String::new(),
            hotkey_editor_row: None,
            settings_open: Some("delay"),
            delay_scheme: 0,
            settings_notice: None,
            data_root: None,
            catalog: sa::SaCatalog::default(),
            pending: save_pipeline::PendingQueue::default(),
            sa_text_sel: None,
            sa_file_sel: None,
            sa_text_selected: None,
            sa_file_selected: None,
            hotkey_pending_save: false,
            sa_hotkey_conflict: false,
            window_group_id: 0,
        }
    }

    /// 面板消息总分发: 按变体域转发到 `app::handlers::*` 的 `handle_*` (2026-10-07 拆分)。
    ///
    /// 本函数只做**路由**, 不含业务逻辑: 各域臂体在 `app/handlers/<域>.rs`,
    /// 由脚本按变体前缀机械迁移、逐字未改。仍 match 全部变体 ⇒ 穷尽性由编译器保证。
    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        use Message::*;
        match message {
            // 键位 / 选项 / 导航域
            Nav(..)
            | ToggleComments
            | PaneOverlay(..)
            | AcrylicToggle(..)
            | SelectKey(..)
            | CmdText(..)
            | Noop
            | SelectWindowGroup(..)
            | SelectActionType(..)
            | EditField(..)
            | SelectRadio { .. }
            | SelectPluginAction { .. }
            | WindowSpy
            | Ready { .. }
            | Failed(..)
            | Retry
            | Save
            | SaveFinished(..)
            | PickWindow
            | WindowPicked(..)
            | SettingsSection(..)
            | StartupToggle(..)
            | Opt(..)
            | DelayScheme(..) => self.handle_keymap(message, context),
            // 杂项域（通知 / 自定义热键编辑 / 字体浏览 / 查看引擎日志）
            ClearNotice
            | Notice(..)
            | CustomHotkeyEdit(..)
            | CustomHotkeyEditClose
            | FontBrowse
            | OpenEngineLog => self.handle_misc(message, context),
            // 选中动作域
            SaHotkey(..)
            | SaEnable(..)
            | SaSelectToggle { .. }
            | SaDeleteAsk
            | SaDeleteCancelled
            | SaDeleteConfirmed
            | SaPlaySample
            | SaSelectBehavior { .. }
            | SaAddBehavior { .. }
            | SaRemoveEntry { .. }
            | SaEntrySwitch { .. }
            | SaEntryMove { .. }
            | SaEntryValue { .. }
            | SaEntryWorkingDir { .. }
            | SaPlayDone(..)
            | SaAddOpen
            | SaHotkeyClear
            | SaNewType { .. }
            | SaAddCancel
            | SaAddType(..)
            | SaAddToggle(..)
            | SaAddConfirm
            | SaExtsEditValue(..)
            | SaExtsEditCommit(..) => self.handle_sa(message, context),
            // 匹配类型域
            MatchTypesOpen | MatchTypesClose | MatchTypesSelect(..) | MtNew | MtLabel(..)
            | MtLabelEn(..) | MtKind(..) | MtRuleOp(..) | MtRuleValue(..) | MtRuleAdd
            | MtRuleRemove(..) | MtExts(..) | MtSave(..) | MtDelete | MtTest(..) | MtTestRun
            | MtTestDone(..) => self.handle_mt(message, context),
            // 行为库域
            BehaviorsOpen | BehaviorsClose | BhSelect(..) | BhNew | BhName(..) | BhId(..)
            | BhDescription(..) | BhAppliesKind(..) | BhAppliesValue(..) | BhAppliesDefault(..)
            | BhAppliesAdd | BhAppliesRemove(..) | BhBaseAction(..) | BhTemplate(..)
            | BhWorkingDir(..) | BhSave | BhDelete | BehaviorsLoaded(..) => {
                self.handle_bh(message, context)
            }
            // 使用指南域
            GuideEditOpen | GuideEditValue(..) | GuideEditReset | GuideEditClose
            | GuideEditSave => self.handle_guide(message, context),
            // 插件域
            PluginsReload
            | PluginsLoaded(..)
            | PluginToggle { .. }
            | PluginDelete(..)
            | PluginImport
            | PluginsMarket => self.handle_plugin(message, context),
            // 插件市场域
            MarketReload | MarketLoaded(..) | MarketInstall { .. } | MarketClosed => {
                self.handle_market(message, context)
            }
            // 插件设置域
            PsValue(..) | PsPickFile(..) | PsLoaded { .. } | PsClosed(..) | PluginConfigure(..) => {
                self.handle_ps(message, context)
            }
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
            .title("设置面板")
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
            // 关掉 WinUI 控件默认的左上角返回箭头占位（Auto 模式渲染；KeyFlux
            // 是单层导航无历史栈，且未接任何 BackRequested 处理 ⇒ 纯占位）。
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
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
        let nav = build_nav(&config.keymaps);
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
        let nav = build_nav(&config.keymaps);
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
        let nav = build_nav(&config.keymaps);
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
        assert_eq!(build_nav(&config.keymaps).len(), 3);
    }
}
