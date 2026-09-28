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

use windows_reactor::*;

use crate::models::SelectedEntry;
use crate::models::{
    Action, Config, Keymap, PluginListResponse, PluginSetting, PluginSettingsResponse,
    QuickSwitchOption,
};
use crate::platform::{self, WindowSpec};
use crate::services::abbr;
use crate::services::api::{ApiResponse, HttpSettingsApi, MessageBody, SettingsApi};
use crate::services::backend::{BackendSession, BackendSessionOptions, resolve_settings_exe};
use crate::services::market::{self, MarketEntry};
use crate::services::selected_action::{self as sa, MATCH_FILE_EXT, MATCH_TEXT_TYPE};
use crate::services::{action_editor, i18n, keymap, markdown, plugins, settings, store};
use crate::theme;
use crate::ui::{
    abbr_view, action_editor as action_editor_view, keymap_view, markdown_view, plugins_view,
    selected_action_view, settings_view,
};

/// 页面种类（由 `build_nav` 依配置推导，对齐 `MainViewModel.PageForKeymap`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageKind {
    /// 使用指南。
    Guide,
    /// 选中动作。
    SelectedAction,
    /// 插件。
    Plugins,
    /// 设置/选项（keymap id = 4）。
    Settings,
    /// 缩写页（keymap id = 2 或 3）。
    Abbr(i32),
    /// 按键矩阵页（其余 keymap）。
    Keymap(i32),
}

impl PageKind {
    /// 页面标题（Fluent：页面用 Title 档）。
    fn title(self) -> String {
        match self {
            Self::Guide => i18n::t("913"),
            Self::SelectedAction => i18n::t("914"),
            Self::Plugins => i18n::t("2418"),
            Self::Settings => i18n::t("2581"),
            Self::Abbr(_) | Self::Keymap(_) => String::new(),
        }
    }
}

/// 导航条目。
#[derive(Clone, Debug)]
struct NavEntry {
    /// 稳定 tag（`NavigationView` 选中匹配 + 重建后保持选中）。
    tag: String,
    label: String,
    kind: PageKind,
}

/// 导航构建（复刻 `MainViewModel.BuildNav`）：
/// 使用指南 + 选中动作 + 插件 + **所有 `enable && id != 1` 的 keymap**。
///
/// `id == 1`（自定义热键）不入导航 —— 2026-09-08 起迁入设置页「其他设置」卡片。
fn build_nav(config: &Config) -> Vec<NavEntry> {
    let mut entries = vec![
        NavEntry {
            tag: "home".to_string(),
            label: i18n::t("913"),
            kind: PageKind::Guide,
        },
        NavEntry {
            tag: "action".to_string(),
            label: i18n::t("914"),
            kind: PageKind::SelectedAction,
        },
        NavEntry {
            tag: "plugins".to_string(),
            label: i18n::t("2418"),
            kind: PageKind::Plugins,
        },
    ];

    for keymap in config.keymaps.iter().filter(|km| km.enable && km.id != 1) {
        let label = if keymap.id == 4 {
            // id=4（设置页入口）标题固定取 i18n：config 里的 name 是数据字段
            // （会被 generators.go 写进生成的 AHK），改它会连带 golden/oracle 与用户 live config。
            i18n::t("2581")
        } else if keymap.name.is_empty() {
            // TODO(Phase 3b): 移植 `NavBadge.EffectiveHotkey`（含窗口分组/修饰键推导），
            // 当前退化用 keymap.hotkey。
            keymap.hotkey.clone()
        } else {
            keymap.name.clone()
        };

        let kind = match keymap.id {
            4 => PageKind::Settings,
            2 | 3 => PageKind::Abbr(keymap.id),
            other => PageKind::Keymap(other),
        };

        entries.push(NavEntry {
            tag: format!("keymap-{}", keymap.id),
            label,
            kind,
        });
    }

    entries
}

/// 组件持有的后端会话槽（`BackendSession` 非 `Clone`，经 `Arc` 旁路移交并在组件 drop 时终止子进程）。
type SessionSlot = Arc<Mutex<Option<BackendSession>>>;

#[derive(Clone)]
pub enum Message {
    /// 导航切换（`None` = 列表重建导致的瞬时清空 ⇒ 忽略，对齐旧版防闪白逻辑）。
    Nav(Option<String>),
    /// 点选键格 / 缩写条目（复刻 `Key.vue` click；禁用键已在状态层拦住）。
    SelectKey(String),
    /// 缩写页命令框输入。
    CmdText(String),
    /// 缩写页命令框回车执行（`del <缩写>` / `rn <新名>` / 其余视为选中）。
    RunCmd,
    /// 空操作（单选取消选中、下拉未命中索引等场景）。
    Noop,
    /// 切换窗口分组（复刻 `windowGroupID` 联动）。
    SelectWindowGroup(i32),
    /// 切换动作类型（复刻 `actionTypeID` 联动；会清空除分组/类型外全部字段）。
    SelectActionType(i32),
    /// 编辑动作字段。
    EditField(ActionField),
    /// 选中单选项（复刻 `SelectRadio`）。
    SelectRadio {
        value_id: i32,
        label_key: &'static str,
    },
    /// 窗口侦探（`POST /server/command/2`）。
    WindowSpy,
    /// 非保存类命令的结果提示（与 `SaveFinished` 同处理）。
    Notice(Result<String, String>),

    Ready {
        config: Box<Config>,
        port: u16,
        doc_md: String,
        shortcuts: Vec<String>,
        /// 部署根（`<deploy>`）；`None` = 未能解析（清空历史等运行期文件操作将不可用）。
        data_root: Option<std::path::PathBuf>,
    },
    Failed(String),
    Retry,
    Save,
    SaveFinished(Result<String, String>),
    // ------------------------------------------------------------- 选中动作页
    /// 主热键输入（AHK 格式，手输）。
    SaHotkey(String),
    /// 启用开关（立即保存，对齐旧版语义）。
    SaEnable(bool),
    /// 聚合卡内点选类型 toggle（`match_type` = `"textType"` / `"fileExt"`）。
    SaSelectToggle {
        match_type: &'static str,
        id: String,
    },
    /// 删除当前点亮类型的映射（立即保存；⚠️ 本轮无确认弹窗，见 16 文档差异表）。
    SaDeleteType {
        match_type: &'static str,
    },
    /// 「添加行为」下拉选择变化。
    SaPickBehavior {
        match_type: &'static str,
        pick: Option<usize>,
    },
    /// 「添加行为」：把下拉选中的行为追加到当前类型的映射（未配置类型同时创建映射）。
    SaAddBehavior {
        match_type: &'static str,
    },
    /// 删除映射内第 `index` 个行为（至少保留一个，对齐旧版 1108 语义）。
    SaRemoveEntry {
        match_type: &'static str,
        index: usize,
    },
    /// 编辑映射内第 `index` 个行为的命令模板。
    SaEntryValue {
        match_type: &'static str,
        index: usize,
        value: String,
    },
    /// 行为目录快照到达（Ready 后台拉取 `GET /api/behaviors`）。
    BehaviorsLoaded(Result<Box<sa::Catalog>, String>),
    // ------------------------------------------------------------- 插件页
    /// 重新拉取插件目录（进入页面 / 导入 / 删除后）。
    PluginsReload,
    /// 插件目录到达。
    PluginsLoaded(Result<Box<PluginListResponse>, String>),
    /// 启停开关（内置卡直通 collectEnabled；用户卡直通注册表）。
    PluginToggle {
        id: String,
        is_builtin: bool,
        enabled: bool,
    },
    /// 删除用户插件。
    PluginDelete(String),
    /// 删除完成（Ok = 后端已删除目录，需清注册表 + 重载）。
    PluginDeleted {
        id: String,
        result: Result<(), String>,
    },
    /// 打开「插件市场」。
    PluginsMarket,
    /// 导入本地插件包（弹文件选择 → `POST /api/plugins/import`）。
    PluginImport,
    /// 导入完成（Ok = 已导入插件的显示名）。
    PluginImported(Result<String, String>),
    /// 打开插件配置（内置 QuickSwitch，或声明了 settings 的用户插件）。
    PluginConfigure(String),
    // ------------------------------------------------------------- QuickSwitch 对话框
    /// 编辑草稿字段（开关/数值）。
    QsEdit(QsField),
    /// 追加一行「排除目录」。
    QsAddPrefix,
    /// 删除第 N 行「排除目录」。
    QsRemovePrefix(usize),
    /// 编辑第 N 行「排除目录」文本。
    QsPrefix(usize, String),
    /// 清空历史（一次性动作，非持久字段）。
    QsClearHistory,
    /// 对话框关闭（Primary = 保存；其余 = 放弃草稿）。
    QsClosed(ContentDialogResult),
    // ------------------------------------------------------------- 插件市场
    /// 重新拉取市场目录。
    MarketReload,
    /// 市场目录 + 本地已装集合到达。
    MarketLoaded(Result<Vec<MarketEntry>, String>),
    /// 安装市场条目（下载 zip → 本地导入链路）。
    MarketInstall {
        id: String,
        url: String,
    },
    /// 安装结果（Ok = 已导入插件名）。
    MarketInstalled {
        id: String,
        result: Result<String, String>,
    },
    /// 关闭市场对话框。
    MarketClosed,
    // ------------------------------------------------------------- 插件设置对话框
    /// 设置行编辑。
    PsValue(usize, String),
    /// 「选择文件」（`file` 类型行）。
    PsPickFile(usize),
    /// 设置加载完成（携带 id 防串台）。
    PsLoaded {
        id: String,
        result: Result<PluginSettingsResponse, String>,
    },
    /// 对话框关闭（Primary = 校验并保存；校验失败则原样重开）。
    PsClosed(ContentDialogResult),
    /// 保存结果（Err = 后端拒绝，原样重开）。
    PsSaved(Result<(), String>),
    // ------------------------------------------------------------- 选项页
    /// 开合一个分区（一次只展开一张，复刻旧版手风琴）。
    SettingsSection(&'static str),
    /// 开机自启开关（即时 `POST /server/command/3|4`，不走保存链路）。
    StartupToggle(bool),
    /// 开机自启命令完成（Err = 失败原因，进选项页提示条）。
    StartupDone(Result<(), String>),
    /// 「触发延时」分区当前选中的方案（渲染态）。
    DelayScheme(usize),
    /// 「命令框字体」浏览按钮（Win32 文件对话框，UI 线程同步弹出）。
    FontBrowse,
    /// 选项页字段编辑（直接写入内存 config，随页脚保存链路持久化）。
    Opt(OptEdit),
}

/// 选项页字段编辑载荷（`Message::Opt`）。
///
/// 下标语义：`Scheme*` = `config.keymaps` 中 **id > 4** 方案的下标（渲染序）；
/// `CustomHotkey*` = keymap id=1 的热键行下标；`Skin(i)` = [`settings::SKIN_FIELDS`] 下标；
/// `Group*` = `options.window_groups` 中 **id > 0** 分组的下标（哨兵 Exclude/Global 不进编辑器）。
#[derive(Clone)]
pub enum OptEdit {
    SchemeName(usize, String),
    SchemeHotkey(usize, String),
    SchemeEnable(usize, bool),
    SchemeAdd,
    SchemeDelay(usize, String),
    HideMatrix(bool),
    Language(usize),
    CustomHotkey(usize, String),
    CustomHotkeyAdd,
    MouseDelay1(String),
    MouseDelay2(String),
    MouseFastSingle(String),
    MouseFastRepeat(String),
    MouseSlowSingle(String),
    MouseSlowRepeat(String),
    MouseTipSymbol(String),
    MouseKeepMode(bool),
    MouseShowTip(bool),
    ScrollDelay1(String),
    ScrollDelay2(String),
    ScrollOnceLine(String),
    LayoutPreset(&'static str),
    KeyboardLayoutSet(String),
    Skin(usize, String),
    FontSource(String),
    FontWeight(usize),
    FontReset,
    PathVarName(usize, String),
    PathVarValue(usize, String),
    PathVarAdd,
    PathVarRemove(usize),
    GroupName(usize, String),
    GroupValue(usize, String),
    GroupCondition(usize, usize),
    GroupAdd,
    GroupRemove(usize),
}

/// QuickSwitch 配置对话框的可编辑字段（消息载荷）。
#[derive(Clone)]
pub enum QsField {
    CollectEnabled(bool),
    AutoShow(bool),
    AutoJumpOpen(bool),
    AutoJumpSave(bool),
    MaxHistory(i32),
    OverlayRows(i32),
    OverlayRowsCompact(i32),
    PollIntervalMs(i32),
}

/// 可编辑的动作字段（消息载荷；`Clone` 以满足 `Component::Message` 约束）。
#[derive(Clone)]
pub enum ActionField {
    WinTitle(String),
    Target(String),
    Args(String),
    WorkingDir(String),
    Comment(String),
    KeysToSend(String),
    AhkCode(String),
    RemapToKey(String),
    RunAsAdmin(bool),
    RunInBackground(bool),
    DetectHiddenWindow(bool),
}

pub struct Shell {
    nav: Vec<NavEntry>,
    page_index: usize,
    loading: bool,
    error: Option<String>,
    notice: Option<String>,
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
    /// 插件目录加载告警（逐包错误汇总）。
    plugins_error: Option<String>,
    /// 插件页一次性操作回显（导入/删除成功提示）。
    plugin_status: Option<String>,
    /// QuickSwitch 配置对话框的编辑草稿（`None` = 对话框关闭）。
    qs_draft: Option<QuickSwitchOption>,
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
            session,
            port: None,
            config: None,
            doc_md: String::new(),
            selected_hotkey: None,
            cmd_text: String::new(),
            plugin_catalog: None,
            plugins_loading: false,
            plugins_error: None,
            plugin_status: None,
            qs_draft: None,
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
            settings_open: None,
            delay_scheme: 0,
            settings_notice: None,
            data_root: None,
            catalog: sa::Catalog::default(),
            sa_text_sel: None,
            sa_file_sel: None,
            sa_text_pick: None,
            sa_file_pick: None,
            hotkey_pending_save: false,
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
            // 禁用键的判断已在状态层完成，这里只记录选中项
            Message::SelectKey(hotkey) => {
                self.selected_hotkey = Some(hotkey);
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
                        let api = HttpSettingsApi::new(port);
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
            } => {
                self.nav = build_nav(&config);
                self.config = Some(*config);
                self.port = Some(port);
                self.doc_md = doc_md;
                self.shortcuts = shortcuts;
                self.data_root = data_root;
                self.page_index = 0;
                self.loading = false;
                self.error = None;
                // 行为目录快照（选中动作页消费；失败时目录为空 = 下拉空，不阻断页面）
                let _ = context.spawn_background(move |_token| {
                    let api = HttpSettingsApi::new(port);
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
                self.notice = None;
                let _ = context
                    .spawn_background(move |_token| Message::SaveFinished(save(port, &config)));
            }
            Message::SaveFinished(result) | Message::Notice(result) => match result {
                Ok(notice) => {
                    // 保存成功 ⇒ 热键未保存提示复位（对齐旧版 OnConfigSaved）
                    self.hotkey_pending_save = false;
                    self.notice = Some(notice);
                }
                Err(reason) => {
                    self.notice = None;
                    self.error = Some(reason);
                }
            },
            // ------------------------------------------------------------- 选中动作页
            Message::SaHotkey(text) => {
                if let Some(config) = self.config.as_mut() {
                    config.selected_action.hotkey = text;
                    self.hotkey_pending_save = true;
                }
            }
            Message::SaEnable(enabled) => {
                if let Some(config) = self.config.as_mut() {
                    config.selected_action.enable = enabled;
                }
                // 启用开关 = 立即保存（对齐旧版 SaveEnableAsync；不做失败回滚，见 16 文档差异表）
                self.save_now(context);
            }
            Message::SaSelectToggle { match_type, id } => match match_type {
                MATCH_TEXT_TYPE => self.sa_text_sel = Some(id),
                _ => self.sa_file_sel = Some(id),
            },
            Message::SaDeleteType { match_type } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if id.is_empty() {
                    return;
                }
                let mut removed = false;
                if let Some(config) = self.config.as_mut()
                    && let Some(position) = sa::find_mapping_index_for_type(config, match_type, &id)
                {
                    config.selected_action.mappings.remove(position);
                    removed = true;
                }
                if removed {
                    // 删除即保存（对齐旧版「确认后立即保存」；⚠️ 本轮无确认弹窗）
                    self.reset_sa_selection(match_type);
                    self.save_now(context);
                }
            }
            Message::SaPickBehavior { match_type, pick } => match match_type {
                MATCH_TEXT_TYPE => self.sa_text_pick = pick,
                _ => self.sa_file_pick = pick,
            },
            Message::SaAddBehavior { match_type } => {
                self.add_behavior(match_type);
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
                    }
                    Err(reason) => {
                        self.plugins_error = Some(reason);
                        self.plugin_catalog = None;
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
                        let api = HttpSettingsApi::new(port);
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
                    self.plugins_error = Some(format!("{}: {reason}", i18n::t("2436")));
                }
            },
            Message::PluginImport => {
                let Some(port) = self.port else {
                    return;
                };
                // 文件选择是**同步模态**对话（必须在 UI 线程弹出）；选定后再把字节交给后台 POST
                let picked = platform::file_dialog::pick_open_file(
                    &i18n::t("2427"),
                    platform::file_dialog::ZIP_FILTER,
                );
                let Some(path) = picked else {
                    return;
                };
                let bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        self.plugins_error = Some(format!("{}: {error}", i18n::t("2432")));
                        return;
                    }
                };
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "plugin.zip".to_string());

                let _ = context.spawn_background(move |_token| {
                    let api = HttpSettingsApi::new(port);
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
                    self.plugin_status = Some(i18n::t_fmt("2431", &[&name]));
                    self.reload_plugins(context);
                }
                Err(reason) => {
                    self.plugins_error = Some(format!("{}: {reason}", i18n::t("2432")));
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
                        let api = HttpSettingsApi::new(port);
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
                        self.reload_plugins(context);
                    }
                    Err(reason) => {
                        self.market_error = Some(format!("{}: {reason}", i18n::t("2432")));
                    }
                }
            }
            Message::MarketClosed => {
                self.market_open = false;
                self.market_installing = None;
            }
            // ---------------------------------------------------------- 插件设置对话框
            Message::PsValue(index, value) => {
                if let Some(slot) = self.ps_rows.get_mut(index) {
                    slot.1 = value;
                }
            }
            Message::PsPickFile(index) => {
                // 过滤器来自后端声明；未声明则不限类型
                let filter = self
                    .ps_rows
                    .get(index)
                    .and_then(|(setting, _)| setting.filter.clone())
                    .filter(|filter| !filter.is_empty())
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
                    return;
                }
                // 本地即时校验（后端仍是权威；失败 → 原样重开供修正）
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
                self.ps_open = false;
                self.ps_error = None;
                let _ = context.spawn_background(move |_token| {
                    let api = HttpSettingsApi::new(port);
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
            Message::PsSaved(result) => match result {
                Ok(()) => {
                    self.ps_open = false;
                    self.ps_error = None;
                }
                Err(reason) => {
                    self.ps_error = Some(format!("{}: {reason}", i18n::t("2585")));
                    self.ps_open = true; // 后端拒绝 → 重开供修正
                }
            },
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
                            let api = HttpSettingsApi::new(port);
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
                // 一次性动作：截断 <部署根>/data/quickswitch/history.tsv（文件保留）
                let outcome = match &self.data_root {
                    Some(root) => plugins::clear_history(root),
                    None => Err("未确定部署根目录".to_string()),
                };
                if let Err(reason) = outcome {
                    self.plugins_error = Some(format!("{}: {reason}", i18n::t("2417")));
                }
            }
            Message::QsClosed(result) => {
                let draft = self.qs_draft.take();
                if result == ContentDialogResult::Primary
                    && let Some(draft) = draft
                {
                    let changed = self
                        .config
                        .as_mut()
                        .map(|config| plugins::commit_draft(config, &draft))
                        .unwrap_or(false);
                    if changed {
                        self.save_now(context);
                    }
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
                        let api = HttpSettingsApi::new(port);
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
                Err(reason) => {
                    // 目录拉取失败：页面仍可用（下拉为空），仅提示
                    self.notice = Some(reason);
                }
            },
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        let spec = WindowSpec::default();
        context.window_visuals(spec.visuals());
        context.window_title(&spec.title);

        // 自绘标题栏：`grid_row` 必须在收尾（`.into()`）之前设置。
        let title_bar: View = TitleBar::new()
            .grid_row(0)
            .title("KeyFlux 设置面板")
            .preferred_height(WindowTitleBarHeight::Tall)
            .into();

        // 导航项：tag 驱动选中匹配，文案来自配置/ i18n。
        let menu: Vec<(String, View)> = self
            .nav
            .iter()
            .map(|entry| {
                let item: View = NavigationViewItem::new().tag(entry.tag.clone()).slot(
                    NavigationViewItemSlot::Content,
                    TextBlock::new().text(entry.label.clone()),
                );
                (entry.tag.clone(), item)
            })
            .collect();

        let nav: View = NavigationView::new()
            .grid_row(1)
            .pane_title("KeyFlux")
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
            ))
    }
}

impl Shell {
    /// 侧栏底部：分隔线 + 保存提示 + 保存按钮（旧 `DockPanel.Dock="Bottom"` 区）。
    fn pane_footer(&self, context: &mut ViewContext<Self>) -> View {
        let notice: View = match &self.notice {
            Some(text) => TextBlock::new()
                .text(text.clone())
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::MUTED_GREEN))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            None => View::empty(),
        };

        StackPanel::new()
            .spacing(8.0)
            .margin(theme::pad_md())
            .children((
                Border::new()
                    .height(1.0)
                    .background(theme::border_cream())
                    .content(TextBlock::new().text("")),
                notice,
                Button::new()
                    .on_click(context.message(Message::Save))
                    .content(
                        TextBlock::new()
                            .text(i18n::t("507"))
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ),
            ))
    }

    /// 内容区三态互斥：加载中 / 错误 / 页面。
    fn content(&self, context: &mut ViewContext<Self>) -> View {
        if self.loading {
            return StackPanel::new()
                .spacing(14.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    ProgressRing::new().is_active(true).width(48.0).height(48.0),
                    TextBlock::new()
                        .text(i18n::t("917"))
                        .foreground(theme::stone_gray())
                        .horizontal_alignment(HorizontalAlignment::Center),
                ));
        }

        if let Some(error) = &self.error {
            return StackPanel::new()
                .spacing(12.0)
                .max_width(560.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(i18n::t("919"))
                        .font_size(theme::FONT_SUBTITLE)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::near_black())
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(error.clone())
                        .foreground(theme::stone_gray())
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_wrapping(TextWrapping::Wrap),
                    Button::new()
                        .on_click(context.message(Message::Retry))
                        .content(i18n::t("920")),
                ));
        }

        self.page_view(context)
    }

    /// 当前页面标题（keymap 页用配置里的 name/hotkey）。
    fn current_title(&self) -> String {
        match self.nav.get(self.page_index) {
            Some(entry) => {
                if entry.kind.title().is_empty() {
                    entry.label.clone()
                } else {
                    entry.kind.title()
                }
            }
            None => String::new(),
        }
    }

    fn page_view(&self, context: &mut ViewContext<Self>) -> View {
        let Some(entry) = self.nav.get(self.page_index) else {
            return TextBlock::new().text("（无导航项）").into();
        };

        // 使用指南：消费 services::markdown 的块模型 → 原生控件（含链接/图片）。
        if entry.kind == PageKind::Guide {
            return self.guide_view();
        }

        // 插件页：统一插件卡（内置 QuickSwitch + 用户插件）
        if entry.kind == PageKind::Plugins {
            return self.plugins_page(context);
        }

        // 选项页（keymap id=4）：方案卡 + 手风琴分区
        if entry.kind == PageKind::Settings {
            return self.settings_page(context);
        }

        let hint = match entry.kind {
            PageKind::SelectedAction => {
                return self.selected_action_page(context);
            }
            PageKind::Plugins => "内置 QuickSwitch 卡 + 用户插件卡 + zip 导入 + 市场入口",
            PageKind::Settings => "快捷键方案 / 外观材质 / 语言 / 路径变量 / 其他设置",
            // 缩写页与矩阵页共享同一编辑核心（旧 `KeymapEditorCore`）：左侧换成 chips + 命令框。
            PageKind::Abbr(id) => {
                return self.abbr_page(context, id);
            }
            PageKind::Keymap(id) => {
                return self.keymap_page(context, id);
            }
            PageKind::Guide => unreachable!("Guide 已提前返回"),
        };
        self.placeholder_page(hint, &self.current_title())
    }

    // ---------------------------------------------------------- 选中动作页

    /// 选中动作页：页头 + 热键卡 + 文本/文件两张聚合卡（toggle + 详情编辑器）。
    fn selected_action_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let sa_config = &config.selected_action;

        let header: View = keymap_view::page_header(&i18n::t("914"), Some(&i18n::t("960")));

        // 热键提示条：未保存（1077）优先于空热键警示（976）
        let hint = if self.hotkey_pending_save {
            i18n::t("1077")
        } else if sa_config.hotkey.is_empty() {
            i18n::t("976")
        } else {
            String::new()
        };
        let hotkey: View = selected_action_view::hotkey_card(
            &sa_config.hotkey,
            sa_config.enable,
            &hint,
            context.callback(|text: String| Message::SaHotkey(text)),
            context.callback(|enabled: bool| Message::SaEnable(enabled)),
        );

        let text_card = self.sa_type_card(context, MATCH_TEXT_TYPE);
        let file_card = self.sa_type_card(context, MATCH_FILE_EXT);

        // 旧页面容器：`Grid Margin="8,20,20,20"` + `StackPanel Spacing="14" MaxWidth="860"`
        ScrollViewer::new().content(
            StackPanel::new()
                .margin(Thickness::new(8.0, 20.0, 20.0, 20.0))
                .spacing(14.0)
                .max_width(860.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .children((header, hotkey, text_card, file_card)),
        )
    }

    /// 一张聚合卡（`match_type` 分区）：卡头 + toggle 行 + 详情编辑器。
    fn sa_type_card(&self, context: &mut ViewContext<Self>, match_type: &'static str) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("").into();
        };
        let toggles = sa::build_toggles(config, match_type);
        if toggles.is_empty() {
            return selected_action_view::type_card(
                TextBlock::new()
                    .text(sa::match_type_label(match_type))
                    .font_size(theme::FONT_CARD_TITLE)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(theme::terracotta())
                    .into(),
            );
        }

        // 选择恢复规则（复刻 RebuildToggles）：保持原选（仍存在）→ 首个已配置 → 首个
        let stored = self.sa_selected_id(match_type).unwrap_or_default();
        let sel_id = if toggles.iter().any(|toggle| toggle.id == stored) {
            stored
        } else {
            toggles
                .iter()
                .find(|toggle| sa::find_mapping_for_type(config, match_type, &toggle.id).is_some())
                .map(|toggle| toggle.id.clone())
                .or_else(|| toggles.first().map(|toggle| toggle.id.clone()))
                .unwrap_or_default()
        };

        let mapping = sa::find_mapping_for_type(config, match_type, &sel_id);
        let header: View = selected_action_view::card_header(
            &sa::match_type_label(match_type),
            mapping.is_some(),
            context.message(Message::SaDeleteType { match_type }),
        );
        let toggles_area: View = selected_action_view::toggles_row(&toggles, &sel_id, |id| {
            context.message(Message::SaSelectToggle { match_type, id })
        });

        // 详情面板
        let detail: View = if let Some(mapping) = mapping {
            let mut rows: Vec<(String, View)> = Vec::new();
            for (index, entry) in mapping.entries.iter().enumerate() {
                let behavior = entry.behavior.clone();
                rows.push((
                    format!("entry-{index}"),
                    // 旧行编辑器套 `rowEditor` 子卡（Ivory 面 + 圆角 4 + Padding 10）
                    selected_action_view::row_editor(selected_action_view::entry_row(
                        index,
                        &self.catalog.label_for(&behavior),
                        &entry.action_value,
                        sa::BadgeColor::for_base_action(&self.catalog.base_action_of(&behavior)),
                        self.catalog.is_no_value(&behavior),
                        context.callback(move |value: String| Message::SaEntryValue {
                            match_type,
                            index,
                            value,
                        }),
                        context.message(Message::SaRemoveEntry { match_type, index }),
                    )),
                ));
            }

            let covering = sa::covering(&self.catalog, match_type, &mapping.match_value);
            let picked = self.sa_picked(match_type, covering.len());
            let can_add = mapping.entries.len() < 9 && picked.is_some();
            rows.push((
                "add".to_string(),
                selected_action_view::add_behavior_row(
                    self.sa_covering_labels(match_type, &mapping.match_value),
                    picked,
                    can_add,
                    context.callback(move |pick: Option<usize>| Message::SaPickBehavior {
                        match_type,
                        pick,
                    }),
                    context.message(Message::SaAddBehavior { match_type }),
                ),
            ));

            StackPanel::new()
                .spacing(8.0)
                .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                .keyed_children(rows)
        } else {
            let match_value = sa::transient_match_value(config, match_type, &sel_id);
            let covering = sa::covering(&self.catalog, match_type, &match_value);
            let picked = self.sa_picked(match_type, covering.len());
            let can_add = picked.is_some();

            StackPanel::new()
                .spacing(8.0)
                .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                .children((
                    selected_action_view::pending_hint(sa::has_dedicated_behavior_for(
                        &self.catalog,
                        match_type,
                        &match_value,
                    )),
                    selected_action_view::add_behavior_row(
                        self.sa_covering_labels(match_type, &match_value),
                        picked,
                        can_add,
                        context.callback(move |pick: Option<usize>| Message::SaPickBehavior {
                            match_type,
                            pick,
                        }),
                        context.message(Message::SaAddBehavior { match_type }),
                    ),
                ))
        };

        selected_action_view::type_card(StackPanel::new().spacing(8.0).children((
            header,
            toggles_area,
            detail,
        )))
    }

    /// 当前卡「添加行为」下拉的有效选中（越界折叠为 `None`）。
    fn sa_picked(&self, match_type: &str, covering_len: usize) -> Option<usize> {
        let stored = match match_type {
            MATCH_TEXT_TYPE => self.sa_text_pick,
            _ => self.sa_file_pick,
        };
        stored.filter(|pick| *pick < covering_len)
    }

    /// 覆盖行为的显示名列表（下拉源）。
    fn sa_covering_labels(&self, match_type: &str, match_value: &str) -> Vec<String> {
        sa::covering(&self.catalog, match_type, match_value)
            .iter()
            .map(|pack| self.catalog.label_for(&pack.id))
            .collect()
    }

    /// 某卡当前点亮的 toggle id。
    fn sa_selected_id(&self, match_type: &str) -> Option<String> {
        match match_type {
            MATCH_TEXT_TYPE => self.sa_text_sel.clone(),
            _ => self.sa_file_sel.clone(),
        }
    }

    /// 删除后清空该卡选中与下拉选择（视图层下次渲染按恢复规则重选）。
    fn reset_sa_selection(&mut self, match_type: &str) {
        match match_type {
            MATCH_TEXT_TYPE => {
                self.sa_text_sel = None;
                self.sa_text_pick = None;
            }
            _ => {
                self.sa_file_sel = None;
                self.sa_file_pick = None;
            }
        }
    }

    /// 立即保存（启用开关 / 删除映射；复刻旧版「立即保存」语义）。
    fn save_now(&mut self, context: &ComponentContext<Self>) {
        let (Some(config), Some(port)) = (self.config.clone(), self.port) else {
            return;
        };

        // 选项页皮肤字段校验（颜色 #RRGGBB / 数值 >= 0；旧版为自由文本框，此处收紧）
        for field in settings::SKIN_FIELDS {
            let value = settings::skin_get(&config.options.command_input_skin, field.key)
                .unwrap_or_default();
            if let Some(reason) = settings::validate_skin_field(&field, value) {
                self.settings_notice =
                    Some(format!("{} ({}): {}", i18n::t("741"), field.key, reason));
                return;
            }
        }

        self.notice = None;
        let _ = context.spawn_background(move |_token| Message::SaveFinished(save(port, &config)));
    }

    /// 「添加行为」：把下拉选中的行为追加到当前类型的映射（未配置类型同时创建映射）。
    fn add_behavior(&mut self, match_type: &'static str) {
        let Some(id) = self.sa_selected_id(match_type) else {
            return;
        };
        if id.is_empty() {
            return;
        }
        let pick = match match_type {
            MATCH_TEXT_TYPE => self.sa_text_pick,
            _ => self.sa_file_pick,
        };
        let Some(pick) = pick else {
            return;
        };

        // 1) matchValue（不可变借用阶段）
        let match_value = match self.config.as_ref() {
            Some(config) => sa::find_mapping_for_type(config, match_type, &id)
                .map(|mapping| mapping.match_value.clone())
                .unwrap_or_else(|| sa::transient_match_value(config, match_type, &id)),
            None => return,
        };

        // 2) 目录推导（catalog 不可变借用）
        let covering = sa::covering(&self.catalog, match_type, &match_value);
        let Some(pack) = covering.get(pick) else {
            return;
        };
        let behavior = pack.id.clone();
        let action_value = if self.catalog.is_no_value(&behavior) {
            String::new()
        } else {
            self.catalog.default_template_for(&behavior)
        };

        // 3) 写入（可变借用；transient → 真实 mapping 转正）
        let Some(config) = self.config.as_mut() else {
            return;
        };
        if let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id) {
            if mapping.entries.len() >= 9 {
                return; // 每条规则最多 9 个行为（旧版 1107 语义）
            }
            if mapping
                .entries
                .iter()
                .any(|entry| entry.behavior == behavior)
            {
                return; // 已添加过（旧版 1119 语义）
            }
            mapping.entries.push(SelectedEntry {
                behavior,
                action_value,
                ..Default::default()
            });
        } else {
            config
                .selected_action
                .mappings
                .push(crate::models::SelectedMapping {
                    match_type: match_type.to_string(),
                    match_value,
                    entries: vec![SelectedEntry {
                        behavior,
                        action_value,
                        ..Default::default()
                    }],
                });
        }
    }

    // ---------------------------------------------------------- 动作编辑（键位图系共享）

    /// 当前页面对应的 keymap id（仅键位图系页面有）。
    fn current_keymap_id(&self) -> Option<i32> {
        match self.nav.get(self.page_index)?.kind {
            PageKind::Keymap(id) | PageKind::Abbr(id) => Some(id),
            _ => None,
        }
    }

    /// 当前页面对应的 keymap。
    fn current_keymap(&self) -> Option<&Keymap> {
        let id = self.current_keymap_id()?;
        self.config
            .as_ref()?
            .keymaps
            .iter()
            .find(|keymap| keymap.id == id)
    }

    /// 当前编辑的动作（只读）。
    fn current_action(&self) -> Option<&Action> {
        let keymap = self.current_keymap()?;
        keymap::find_action(
            keymap,
            self.selected_hotkey.as_deref()?,
            self.window_group_id,
        )
    }

    /// 当前编辑的动作（可变；不存在则惰性初始化，复刻 `_getAction`）。
    fn current_action_mut(&mut self) -> Option<&mut Action> {
        let keymap_id = self.current_keymap_id()?;
        let hotkey = self.selected_hotkey.clone()?;
        let group = self.window_group_id;
        let config = self.config.as_mut()?;
        keymap::ensure_action(config, keymap_id, &hotkey, group)
    }

    /// 对当前 keymap 做可变访问（无配置 / 无该 keymap 时静默跳过）。
    fn with_current_keymap<F: FnOnce(&mut Keymap)>(&mut self, apply: F) {
        let Some(id) = self.current_keymap_id() else {
            return;
        };
        let Some(config) = self.config.as_mut() else {
            return;
        };
        if let Some(keymap) = config.keymaps.iter_mut().find(|km| km.id == id) {
            apply(keymap);
        }
    }

    // ---------------------------------------------------------- 插件页

    /// 插件声明式设置对话框（`ContentDialog`；表单按 manifest 声明渲染）。
    fn plugin_settings_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.ps_open {
            return View::empty();
        }

        let english = matches!(i18n::language(), i18n::Lang::En);
        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some(error) = &self.ps_error {
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(error.clone())
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::ERROR_CRIMSON))
                    .text_wrapping(TextWrapping::Wrap)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
                    .into(),
            ));
        }

        if self.ps_loading {
            rows.push((rows.len(), plugins_view::loading()));
        } else if self.ps_rows.is_empty() {
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(i18n::t("2586"))
                    .foreground(theme::stone_gray())
                    .into(),
            ));
        }

        for (index, (setting, value)) in self.ps_rows.iter().enumerate() {
            rows.push((
                rows.len(),
                plugins_view::setting_row(
                    setting,
                    value,
                    english,
                    context.callback(move |value: String| Message::PsValue(index, value)),
                    context.message(Message::PsPickFile(index)),
                ),
            ));
        }

        // ⚠️ `content()` 收尾 ⇒ 其余 builder 在前
        ContentDialog::new()
            .title(self.ps_title.clone())
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| Message::PsClosed(result)))
            .content(
                ScrollViewer::new()
                    .max_height(430.0)
                    .min_width(480.0)
                    .content(StackPanel::new().spacing(0.0).keyed_children(rows)),
            )
    }

    /// 后台拉取市场目录 + 本地已装集合（复刻 `PluginMarketViewModel.LoadAsync`）。
    fn reload_market(&mut self, context: &ComponentContext<Self>) {
        let Some(port) = self.port else {
            return;
        };
        self.market_loading = true;
        self.market_error = None;
        let _ = context.spawn_background(move |_token| {
            // 已装集合取本地后端目录；目录本体走外部网络（后端不出网）
            let installed: Vec<String> = HttpSettingsApi::new(port)
                .get_plugins()
                .value
                .map(|catalog| {
                    catalog
                        .plugins
                        .into_iter()
                        .map(|plugin| plugin.id)
                        .collect()
                })
                .unwrap_or_default();

            match market::fetch_catalog() {
                Ok(catalog) => {
                    Message::MarketLoaded(Ok(market::build_entries(&catalog, &installed)))
                }
                Err(reason) => Message::MarketLoaded(Err(reason)),
            }
        });
    }

    /// 插件市场对话框（`ContentDialog`；列表可滚动）。
    ///
    /// 与旧 `PluginMarketWindow` 的差异：旧版是**独立窗口**，新版用 `ContentDialog`
    /// （Fluent 一致的模态交互，且省去第二窗口的生命周期管理）。
    fn market_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.market_open {
            return View::empty();
        }

        let english = matches!(i18n::language(), i18n::Lang::En);
        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some(status) = &self.market_status {
            rows.push((rows.len(), plugins_view::status_banner(status)));
        }

        if self.market_loading {
            rows.push((rows.len(), plugins_view::loading()));
        } else if let Some(error) = &self.market_error {
            rows.push((
                rows.len(),
                plugins_view::load_error(error, context.message(Message::MarketReload)),
            ));
        } else if self.market_entries.is_empty() {
            rows.push((rows.len(), plugins_view::empty_state()));
        }

        for entry in &self.market_entries {
            let installing = self.market_installing.as_deref() == Some(entry.id.as_str());
            rows.push((
                rows.len(),
                plugins_view::market_entry(
                    entry,
                    english,
                    installing,
                    context.message(Message::MarketInstall {
                        id: entry.id.clone(),
                        url: entry.url.clone(),
                    }),
                ),
            ));
        }

        ContentDialog::new()
            .title(i18n::t("2428"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|_result: ContentDialogResult| Message::MarketClosed))
            .content(
                ScrollViewer::new()
                    .max_height(460.0)
                    .min_width(520.0)
                    .content(StackPanel::new().spacing(0.0).keyed_children(rows)),
            )
    }

    /// 选项页字段编辑（`Message::Opt` 的落地）。
    ///
    /// 下标语义见 [`OptEdit`] 文档；全部**直接写入内存 config**，随页脚保存链路持久化
    /// （与键位图页/缩写页的编辑模式一致）。
    fn apply_opt(&mut self, edit: OptEdit) {
        let Some(config) = self.config.as_mut() else {
            return;
        };
        match edit {
            OptEdit::SchemeName(index, value) => {
                if let Some(keymap) = config.keymaps.iter_mut().filter(|km| km.id > 4).nth(index) {
                    keymap.name = value;
                }
            }
            OptEdit::SchemeHotkey(index, value) => {
                if let Some(keymap) = config.keymaps.iter_mut().filter(|km| km.id > 4).nth(index) {
                    keymap.hotkey = value;
                }
            }
            OptEdit::SchemeEnable(index, value) => {
                let changed = config
                    .keymaps
                    .iter_mut()
                    .filter(|km| km.id > 4)
                    .nth(index)
                    .map(|keymap| {
                        let changed = keymap.enable != value;
                        keymap.enable = value;
                        changed
                    })
                    .unwrap_or(false);
                // 启停改变导航构成 ⇒ 重建导航（与保存后 BuildNav 同语义）
                if changed {
                    let next = config.clone();
                    self.nav = build_nav(&next);
                }
            }
            OptEdit::SchemeAdd => {
                let mut next_id = 5;
                while config.keymaps.iter().any(|km| km.id == next_id) {
                    next_id += 1;
                }
                config.keymaps.push(Keymap {
                    id: next_id,
                    name: format!("{} {}", i18n::t("1118"), next_id),
                    enable: false,
                    ..Default::default()
                });
                self.rebuild_nav();
            }
            OptEdit::SchemeDelay(index, value) => {
                if let (Some(keymap), Ok(delay)) = (
                    config.keymaps.iter_mut().filter(|km| km.id > 4).nth(index),
                    value.trim().parse::<i32>(),
                ) {
                    keymap.delay = delay;
                }
            }
            OptEdit::HideMatrix(value) => config.options.hide_matrix = value,
            OptEdit::Language(index) => {
                let value = ["", "zh", "en"][index.min(2)];
                config.options.language = value.to_string();
                i18n::apply_config_language(value);
                self.rebuild_nav();
            }
            OptEdit::CustomHotkey(index, value) => {
                if let Some(keymap) = config.keymaps.iter_mut().find(|km| km.id == 1) {
                    let old = keymap.hotkeys.keys().nth(index).cloned();
                    if let Some(old) = old {
                        keymap::change_hotkey(keymap, &old, &value);
                    }
                }
            }
            OptEdit::CustomHotkeyAdd => {
                // 占位热键：动作为空 ⇒ `clean_for_save` 在保存时整体丢弃，不会生成无效 AHK
                let mut next = 1;
                let placeholder = loop {
                    let candidate = format!("ctrl+alt+shift+f{next}");
                    let taken = config
                        .keymaps
                        .iter()
                        .find(|km| km.id == 1)
                        .map(|km| km.hotkeys.contains_key(&candidate))
                        .unwrap_or(true);
                    if !taken {
                        break candidate;
                    }
                    next += 1;
                };
                let _ = keymap::ensure_action(config, 1, &placeholder, -1);
            }
            OptEdit::MouseDelay1(value) => config.options.mouse.delay1 = value,
            OptEdit::MouseDelay2(value) => config.options.mouse.delay2 = value,
            OptEdit::MouseFastSingle(value) => config.options.mouse.fast_single = value,
            OptEdit::MouseFastRepeat(value) => config.options.mouse.fast_repeat = value,
            OptEdit::MouseSlowSingle(value) => config.options.mouse.slow_single = value,
            OptEdit::MouseSlowRepeat(value) => config.options.mouse.slow_repeat = value,
            OptEdit::MouseTipSymbol(value) => config.options.mouse.tip_symbol = value,
            OptEdit::MouseKeepMode(value) => config.options.mouse.keep_mouse_mode = value,
            OptEdit::MouseShowTip(value) => config.options.mouse.show_tip = value,
            OptEdit::ScrollDelay1(value) => config.options.scroll.delay1 = value,
            OptEdit::ScrollDelay2(value) => config.options.scroll.delay2 = value,
            OptEdit::ScrollOnceLine(value) => config.options.scroll.once_line_count = value,
            OptEdit::LayoutPreset(kind) => {
                let current = config.options.keyboard_layout.clone();
                if let Some(layout) = settings::keyboard_layout_preset(kind, &current) {
                    config.options.keyboard_layout = layout;
                }
            }
            OptEdit::KeyboardLayoutSet(value) => config.options.keyboard_layout = value,
            OptEdit::Skin(index, value) => {
                if let Some(field) = settings::SKIN_FIELDS.get(index) {
                    settings::skin_set(&mut config.options.command_input_skin, field.key, &value);
                }
            }
            OptEdit::FontSource(value) => config.options.command_font.source_path = value,
            OptEdit::FontWeight(index) => {
                if let Some(weight) = settings::FONT_WEIGHTS.get(index) {
                    config.options.command_font.weight = (*weight).to_string();
                }
            }
            OptEdit::FontReset => settings::font_reset(config),
            OptEdit::PathVarName(index, value) => {
                if let Some(row) = config.options.path_variables.get_mut(index) {
                    row.name = value;
                }
            }
            OptEdit::PathVarValue(index, value) => {
                if let Some(row) = config.options.path_variables.get_mut(index) {
                    row.value = value;
                }
            }
            OptEdit::PathVarAdd => {
                settings::add_path_variable(config);
            }
            OptEdit::PathVarRemove(index) => {
                settings::remove_path_variable(config, index);
            }
            OptEdit::GroupName(row, value) => {
                if let Some(group) = config
                    .options
                    .window_groups
                    .iter_mut()
                    .filter(|group| group.id > 0)
                    .nth(row)
                {
                    group.name = value;
                }
            }
            OptEdit::GroupValue(row, value) => {
                if let Some(group) = config
                    .options
                    .window_groups
                    .iter_mut()
                    .filter(|group| group.id > 0)
                    .nth(row)
                {
                    group.value = value;
                }
            }
            OptEdit::GroupCondition(row, index) => {
                if let Some(group) = config
                    .options
                    .window_groups
                    .iter_mut()
                    .filter(|group| group.id > 0)
                    .nth(row)
                {
                    group.condition_type = index as i32 + 1;
                }
            }
            OptEdit::GroupAdd => {
                let mut next_id = 1;
                while config
                    .options
                    .window_groups
                    .iter()
                    .any(|group| group.id == next_id)
                {
                    next_id += 1;
                }
                config
                    .options
                    .window_groups
                    .push(crate::models::WindowGroup {
                        id: next_id,
                        name: format!("{} {}", i18n::t("1118"), next_id),
                        ..Default::default()
                    });
            }
            OptEdit::GroupRemove(row) => {
                let keep: Vec<usize> = config
                    .options
                    .window_groups
                    .iter()
                    .enumerate()
                    .filter(|(_, group)| group.id > 0)
                    .map(|(index, _)| index)
                    .collect();
                if let Some(&index) = keep.get(row) {
                    config.options.window_groups.remove(index);
                }
            }
        }
    }

    // ---------------------------------------------------------- 选项页

    /// 手风琴分区卡包装：展开时才构建内容（一次只展开一张，复刻旧版）。
    fn section(
        &self,
        context: &mut ViewContext<Self>,
        id: &'static str,
        title_key: &str,
        body: impl FnOnce(&Self, &mut ViewContext<Self>) -> View,
    ) -> View {
        let open = self.settings_open == Some(id);
        let content = if open {
            body(self, context)
        } else {
            View::empty()
        };
        settings_view::section_card(
            i18n::t(title_key),
            open,
            context.message(Message::SettingsSection(id)),
            content,
        )
    }

    /// 选项页（keymap id=4）：左列「快捷键方案」卡 + 右列手风琴分区栈。
    ///
    /// 分区顺序对齐旧 `SettingsPageView.axaml`：其他设置 / 程序分组 / 自定义热键 /
    /// 鼠标参数 / 滚轮 / 键盘布局 / 触发延时 / 命令框皮肤 / 命令框字体 / 路径变量。
    fn settings_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let schemes: Vec<&Keymap> = config.keymaps.iter().filter(|km| km.id > 4).collect();

        // 左列：快捷键方案（915）——行内直接编辑名称/触发键/开关
        let mut scheme_rows: Vec<(usize, View)> = Vec::new();
        for (index, keymap) in schemes.iter().enumerate() {
            scheme_rows.push((
                index,
                settings_view::scheme_row(
                    &keymap.name,
                    &keymap.hotkey,
                    keymap.enable,
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::SchemeName(index, value))
                    }),
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::SchemeHotkey(index, value))
                    }),
                    context.callback(move |value: bool| {
                        Message::Opt(OptEdit::SchemeEnable(index, value))
                    }),
                ),
            ));
        }

        // 左列卡：旧 `Border.leftPanel`（Ivory 面 + cream 边 2px + 圆角 14 + Padding 16）；
        // 标题 915 旧 16 SemiBold + 底距 12
        let left: View = Border::new()
            .padding(theme::pad_md())
            .margin(Thickness::new(0.0, 0.0, 16.0, 0.0))
            .background(theme::ivory())
            .border_brush(theme::border_cream())
            .border_thickness(theme::card_border())
            .corner_radius(theme::radius_card())
            .vertical_alignment(VerticalAlignment::Top)
            .content(
                StackPanel::new().spacing(10.0).children((
                    TextBlock::new()
                        .text(i18n::t("915"))
                        .font_size(theme::FONT_SECTION_TITLE)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .foreground(theme::near_black())
                        .margin(Thickness::new(0.0, 0.0, 0.0, 12.0)),
                    settings_view::scheme_header(),
                    StackPanel::new().keyed_children(scheme_rows),
                    Button::new()
                        .margin(Thickness::new(0.0, 10.0, 0.0, 0.0))
                        .on_click(context.message(Message::Opt(OptEdit::SchemeAdd)))
                        .content(TextBlock::new().text(i18n::t("405"))),
                )),
            );

        // 右列：分区栈
        let mut sections: Vec<(usize, View)> = Vec::new();

        // 一次性提示（自启命令失败 / 保存校验失败）
        if let Some(notice) = &self.settings_notice {
            sections.push((
                sections.len(),
                TextBlock::new()
                    .text(notice.clone())
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::ERROR_CRIMSON))
                    .text_wrapping(TextWrapping::Wrap)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
                    .into(),
            ));
        }

        // 505 其他设置：506 开机自启（即时生效）/ 901 隐藏矩阵 / 781 语言
        sections.push((
            sections.len(),
            self.section(context, "other", "505", |this, context| {
                let languages = vec![
                    "跟随系统".to_string(),
                    "中文".to_string(),
                    "English".to_string(),
                ];
                let language_index = match this
                    .config
                    .as_ref()
                    .map(|config| config.options.language.as_str())
                {
                    Some("zh") => 1,
                    Some("en") => 2,
                    _ => 0,
                };
                StackPanel::new().children((
                    settings_view::toggle_row(
                        i18n::t("506"),
                        this.config
                            .as_ref()
                            .map(|c| c.options.startup)
                            .unwrap_or(false),
                        context.callback(|value: bool| Message::StartupToggle(value)),
                    ),
                    settings_view::check_row(
                        i18n::t("902"),
                        this.config
                            .as_ref()
                            .map(|c| c.options.hide_matrix)
                            .unwrap_or(false),
                        context.callback(|value: bool| Message::Opt(OptEdit::HideMatrix(value))),
                    ),
                    settings_view::combo_row(
                        i18n::t("781"),
                        &languages,
                        language_index,
                        context.callback(|value: Option<usize>| {
                            Message::Opt(OptEdit::Language(value.unwrap_or(0)))
                        }),
                    ),
                ))
            }),
        ));

        // 601 编辑程序分组（哨兵 Exclude/Global 不进编辑器）
        sections.push((
            sections.len(),
            self.section(context, "groups", "601", |this, context| {
                let groups: Vec<&crate::models::WindowGroup> = this
                    .config
                    .as_ref()
                    .map(|config| {
                        config
                            .options
                            .window_groups
                            .iter()
                            .filter(|group| group.id > 0)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut rows: Vec<(usize, View)> = Vec::new();
                for (row, group) in groups.iter().enumerate() {
                    let name_cb = context.callback(move |value: String| {
                        Message::Opt(OptEdit::GroupName(row, value))
                    });
                    let value_cb = context.callback(move |value: String| {
                        Message::Opt(OptEdit::GroupValue(row, value))
                    });
                    let condition_cb = context.callback(move |value: Option<usize>| {
                        Message::Opt(OptEdit::GroupCondition(row, value.unwrap_or(0)))
                    });
                    let delete_cb = context.message(Message::Opt(OptEdit::GroupRemove(row)));
                    rows.push((
                        row,
                        settings_view::group_row(
                            &group.name,
                            &group.value,
                            group.condition_type.saturating_sub(1).max(0) as usize,
                            name_cb,
                            value_cb,
                            condition_cb,
                            delete_cb,
                        ),
                    ));
                }
                StackPanel::new().children((
                    StackPanel::new().keyed_children(rows),
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::GroupAdd)))
                        .content(TextBlock::new().text(i18n::t("609"))),
                    settings_view::hint_row(i18n::t("612")),
                ))
            }),
        ));

        // 1116 自定义热键（keymap id=1；动作详情在动作编辑面板体系内）
        sections.push((
            sections.len(),
            self.section(context, "customhotkeys", "1116", |this, context| {
                let rows: Vec<(String, String)> = this
                    .config
                    .as_ref()
                    .and_then(|config| config.keymaps.iter().find(|km| km.id == 1))
                    .map(|keymap| {
                        keymap
                            .hotkeys
                            .iter()
                            .map(|(hotkey, actions)| {
                                let function = actions
                                    .iter()
                                    .find(|action| !action.comment.is_empty())
                                    .map(|action| action.comment.clone())
                                    .unwrap_or_default();
                                (hotkey.clone(), function)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut list: Vec<(usize, View)> = Vec::new();
                for (row, (hotkey, function)) in rows.iter().enumerate() {
                    list.push((
                        row,
                        settings_view::hotkey_row(
                            hotkey,
                            function,
                            context.callback(move |value: String| {
                                Message::Opt(OptEdit::CustomHotkey(row, value))
                            }),
                        ),
                    ));
                }
                StackPanel::new().children((
                    StackPanel::new().keyed_children(list),
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::CustomHotkeyAdd)))
                        .content(TextBlock::new().text(i18n::t("1118"))),
                ))
            }),
        ));

        // 701 修改鼠标参数（9 字段；字符串型数值原样透传，与旧版自由文本框一致）
        sections.push((
            sections.len(),
            self.section(context, "mouse", "701", |this, context| {
                let mouse = this
                    .config
                    .as_ref()
                    .map(|config| config.options.mouse.clone())
                    .unwrap_or_default();
                fn text<C: Fn(String) -> Message + 'static>(
                    callback: C,
                ) -> impl Fn(String) -> Message + 'static {
                    move |value: String| callback(value)
                }
                StackPanel::new().children((
                    settings_view::hint_row(i18n::t("702")),
                    settings_view::text_field(
                        i18n::t("703"),
                        &mouse.delay1,
                        context.callback(text(|value| Message::Opt(OptEdit::MouseDelay1(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("704"),
                        &mouse.delay2,
                        context.callback(text(|value| Message::Opt(OptEdit::MouseDelay2(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("705"),
                        &mouse.fast_single,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseFastSingle(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("706"),
                        &mouse.fast_repeat,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseFastRepeat(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("707"),
                        &mouse.slow_single,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseSlowSingle(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("708"),
                        &mouse.slow_repeat,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseSlowRepeat(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("709"),
                        &mouse.tip_symbol,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseTipSymbol(value)))),
                    ),
                    settings_view::check_row(
                        i18n::t("710"),
                        mouse.show_tip,
                        context.callback(|value: bool| Message::Opt(OptEdit::MouseShowTip(value))),
                    ),
                    settings_view::check_row(
                        i18n::t("711"),
                        mouse.keep_mouse_mode,
                        context.callback(|value: bool| Message::Opt(OptEdit::MouseKeepMode(value))),
                    ),
                ))
            }),
        ));

        // 712 滚轮相关参数（3 字段）
        sections.push((
            sections.len(),
            self.section(context, "scroll", "712", |this, context| {
                let scroll = this
                    .config
                    .as_ref()
                    .map(|config| config.options.scroll.clone())
                    .unwrap_or_default();
                StackPanel::new().children((
                    settings_view::text_field(
                        i18n::t("713"),
                        &scroll.delay1,
                        context
                            .callback(|value: String| Message::Opt(OptEdit::ScrollDelay1(value))),
                    ),
                    settings_view::text_field(
                        i18n::t("714"),
                        &scroll.delay2,
                        context
                            .callback(|value: String| Message::Opt(OptEdit::ScrollDelay2(value))),
                    ),
                    settings_view::text_field(
                        i18n::t("715"),
                        &scroll.once_line_count,
                        context
                            .callback(|value: String| Message::Opt(OptEdit::ScrollOnceLine(value))),
                    ),
                ))
            }),
        ));

        // 721 修改键盘布局：多行文本 + 四个预设按钮
        sections.push((
            sections.len(),
            self.section(context, "layout", "721", |this, context| {
                let layout = this
                    .config
                    .as_ref()
                    .map(|config| config.options.keyboard_layout.clone())
                    .unwrap_or_default();
                StackPanel::new().spacing(8.0).children((
                    TextBox::new()
                        .text(layout)
                        .accepts_return(true)
                        .min_height(180.0)
                        .on_text_changed(context.callback(|value: String| {
                            Message::Opt(OptEdit::KeyboardLayoutSet(value))
                        })),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("0"))))
                                .content(TextBlock::new().text(i18n::t("723"))),
                            Button::new()
                                .on_click(
                                    context.message(Message::Opt(OptEdit::LayoutPreset("74"))),
                                )
                                .content(TextBlock::new().text(i18n::t("724"))),
                            Button::new()
                                .on_click(
                                    context.message(Message::Opt(OptEdit::LayoutPreset("104"))),
                                )
                                .content(TextBlock::new().text(i18n::t("725"))),
                            Button::new()
                                .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("1"))))
                                .content(TextBlock::new().text(i18n::t("726"))),
                        )),
                ))
            }),
        ));

        // 761 设置触发延时：方案下拉 + 毫秒数（提示 763）
        sections.push((
            sections.len(),
            self.section(context, "delay", "761", |this, context| {
                let schemes: Vec<String> = this
                    .config
                    .as_ref()
                    .map(|config| {
                        config
                            .keymaps
                            .iter()
                            .filter(|km| km.id > 4)
                            .map(|km| km.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let index = this.delay_scheme.min(schemes.len().saturating_sub(1));
                let delay = this
                    .config
                    .as_ref()
                    .and_then(|config| config.keymaps.iter().filter(|km| km.id > 4).nth(index))
                    .map(|keymap| keymap.delay.to_string())
                    .unwrap_or_default();
                StackPanel::new().children((
                    settings_view::combo_row(
                        i18n::t("915"),
                        &schemes,
                        index,
                        context.callback(|value: Option<usize>| {
                            Message::DelayScheme(value.unwrap_or(0))
                        }),
                    ),
                    settings_view::text_field(
                        i18n::t("762"),
                        &delay,
                        context.callback(move |value: String| {
                            Message::Opt(OptEdit::SchemeDelay(index, value))
                        }),
                    ),
                    settings_view::hint_row(i18n::t("763")),
                ))
            }),
        ));

        // 741 命令框皮肤（18 字段；748 透明度共用文案）
        sections.push((
            sections.len(),
            self.section(context, "skin", "741", |this, context| {
                let mut rows: Vec<(usize, View)> = Vec::new();
                for (index, field) in settings::SKIN_FIELDS.iter().enumerate() {
                    let value = this
                        .config
                        .as_ref()
                        .and_then(|config| {
                            settings::skin_get(&config.options.command_input_skin, field.key)
                        })
                        .unwrap_or("")
                        .to_string();
                    rows.push((
                        index,
                        settings_view::skin_row(
                            i18n::t(field.label_key),
                            &value,
                            context.callback(move |value: String| {
                                Message::Opt(OptEdit::Skin(index, value))
                            }),
                        ),
                    ));
                }
                StackPanel::new().keyed_children(rows)
            }),
        ));

        // 2503 命令框字体：路径 + 浏览 + 字重 + 恢复默认
        sections.push((
            sections.len(),
            self.section(context, "font", "2503", |this, context| {
                let (source, weight_index) = this
                    .config
                    .as_ref()
                    .map(|config| {
                        let index = settings::FONT_WEIGHTS
                            .iter()
                            .position(|weight| *weight == config.options.command_font.weight)
                            .unwrap_or(2);
                        (config.options.command_font.source_path.clone(), index)
                    })
                    .unwrap_or_default();
                let weights: Vec<String> = settings::FONT_WEIGHTS
                    .iter()
                    .map(|w| i18n::t(settings::font_weight_label_key(w)))
                    .collect();
                StackPanel::new().children((
                    settings_view::text_field(
                        i18n::t("2504"),
                        &source,
                        context.callback(|value: String| Message::Opt(OptEdit::FontSource(value))),
                    ),
                    settings_view::button_row(
                        "",
                        i18n::t("2582"),
                        context.message(Message::FontBrowse),
                    ),
                    settings_view::combo_row(
                        i18n::t("2508"),
                        &weights,
                        weight_index,
                        context.callback(|value: Option<usize>| {
                            Message::Opt(OptEdit::FontWeight(value.unwrap_or(2)))
                        }),
                    ),
                    settings_view::button_row(
                        "",
                        i18n::t("2507"),
                        context.message(Message::Opt(OptEdit::FontReset)),
                    ),
                ))
            }),
        ));

        // 907 编辑路径变量：行编辑 + 新增（933）
        sections.push((
            sections.len(),
            self.section(context, "pathvars", "907", |this, context| {
                let variables = this
                    .config
                    .as_ref()
                    .map(|config| config.options.path_variables.clone())
                    .unwrap_or_default();
                let mut rows: Vec<(usize, View)> = Vec::new();
                for (row, variable) in variables.iter().enumerate() {
                    rows.push((
                        row,
                        settings_view::pathvar_row(
                            &variable.name,
                            &variable.value,
                            context.callback(move |value: String| {
                                Message::Opt(OptEdit::PathVarName(row, value))
                            }),
                            context.callback(move |value: String| {
                                Message::Opt(OptEdit::PathVarValue(row, value))
                            }),
                            context.message(Message::Opt(OptEdit::PathVarRemove(row))),
                        ),
                    ));
                }
                StackPanel::new().children((
                    StackPanel::new().keyed_children(rows),
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::PathVarAdd)))
                        .content(TextBlock::new().text(i18n::t("933"))),
                    settings_view::hint_row(i18n::t("911")),
                ))
            }),
        ));

        // 右列：旧 `StackPanel Width="460" Spacing="16" Margin="24,0,24,24"`（卡间距由
        // section_card 自带底距 16 承担）；整页容器 = 旧 `Margin="24,20,24,28"`
        let right: View = ScrollViewer::new().content(
            StackPanel::new()
                .width(460.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .margin(Thickness::new(24.0, 0.0, 24.0, 24.0))
                .keyed_children(sections),
        );

        Grid::new()
            .margin(Thickness::new(24.0, 20.0, 24.0, 28.0))
            .columns([GridLength::Pixel(560.0), GridLength::STAR])
            .children((left, Border::new().grid_column(1).content(right)))
    }

    /// QuickSwitch 配置对话框（`ContentDialog`；草稿存在即打开）。
    ///
    /// 字段与旧 `QuickSwitchDialogWindow.axaml` 一致：4 个开关 + 历史条数 + 排除目录表 + 清空历史。
    /// （`pollIntervalMs` / `overlayRows` / `overlayRowsCompact` 旧版也未在对话框内暴露，
    /// 但保存时同样逐字段写回 —— 此处保持一致。）
    fn quick_switch_dialog(&self, context: &mut ViewContext<Self>) -> View {
        let Some(draft) = self.qs_draft.as_ref() else {
            return View::empty();
        };

        let mut rows: Vec<(usize, View)> = Vec::new();
        rows.push((
            rows.len(),
            plugins_view::qs_check(
                i18n::t("2409"),
                draft.collect_enabled,
                context.callback(|value: bool| Message::QsEdit(QsField::CollectEnabled(value))),
            ),
        ));
        rows.push((
            rows.len(),
            plugins_view::qs_check(
                i18n::t("2410"),
                draft.auto_show,
                context.callback(|value: bool| Message::QsEdit(QsField::AutoShow(value))),
            ),
        ));
        rows.push((
            rows.len(),
            plugins_view::qs_check(
                i18n::t("2411"),
                draft.auto_jump_open,
                context.callback(|value: bool| Message::QsEdit(QsField::AutoJumpOpen(value))),
            ),
        ));
        rows.push((
            rows.len(),
            plugins_view::qs_check(
                i18n::t("2412"),
                draft.auto_jump_save,
                context.callback(|value: bool| Message::QsEdit(QsField::AutoJumpSave(value))),
            ),
        ));
        rows.push((
            rows.len(),
            plugins_view::qs_number(
                i18n::t("2413"),
                f64::from(draft.max_history),
                f64::from(plugins::MIN_HISTORY),
                9999.0,
                context.callback(|value: Option<f64>| {
                    Message::QsEdit(QsField::MaxHistory(value.unwrap_or(1.0) as i32))
                }),
            ),
        ));

        // 排除目录表（行编辑即时回写草稿）
        rows.push((rows.len(), plugins_view::qs_section(i18n::t("2414"))));
        for (index, prefix) in draft.excluded_prefixes.iter().enumerate() {
            let remove: View = Button::new()
                .on_click(context.message(Message::QsRemovePrefix(index)))
                .content(TextBlock::new().text(i18n::t("912")));
            rows.push((
                rows.len(),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
                    .children((
                        plugins_view::qs_text(
                            prefix,
                            "",
                            context.callback(move |value: String| Message::QsPrefix(index, value)),
                        ),
                        remove,
                    )),
            ));
        }
        rows.push((
            rows.len(),
            Button::new()
                .on_click(context.message(Message::QsAddPrefix))
                .content(TextBlock::new().text(i18n::t("2415"))),
        ));
        rows.push((
            rows.len(),
            Button::new()
                .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                .on_click(context.message(Message::QsClearHistory))
                .content(TextBlock::new().text(i18n::t("2416"))),
        ));

        // ⚠️ `content()` 是收尾方法（返回 `View`）⇒ 其余 builder 必须写在它之前
        ContentDialog::new()
            .title(i18n::t("2408"))
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| Message::QsClosed(result)))
            .content(
                ScrollViewer::new()
                    .max_height(420.0)
                    .content(StackPanel::new().spacing(0.0).keyed_children(rows)),
            )
    }

    /// 后台拉取插件目录（复刻 `PluginsPageViewModel.ReloadAsync`）。
    fn reload_plugins(&mut self, context: &ComponentContext<Self>) {
        let Some(port) = self.port else {
            return;
        };
        self.plugins_loading = true;
        self.plugins_error = None;
        self.plugin_status = None;
        let _ = context.spawn_background(move |_token| {
            let api = HttpSettingsApi::new(port);
            let response = api.get_plugins();
            match response.value {
                Some(catalog) => Message::PluginsLoaded(Ok(Box::new(catalog))),
                None => Message::PluginsLoaded(Err(response
                    .error_message
                    .unwrap_or_else(|| format!("HTTP {}", response.status)))),
            }
        });
    }

    /// 插件页：页头入口 + 分区说明 + 统一卡片列表 + 三态。
    ///
    /// 卡片由「当前配置 + 目录快照」**每次渲染即时派生** ⇒ 开关状态天然与配置同步
    /// （无需额外的双向同步标记，旧版的 `_syncingFromConfig` 因此省去）。
    fn plugins_page(&self, context: &mut ViewContext<Self>) -> View {
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
                plugins_view::load_error(error, context.message(Message::PluginsReload)),
            ));
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

    /// 缩写页命令框执行（复刻 `AbbrPageViewModel.RunCmd`）：
    /// `del <缩写>` 删除 · `rn <新名>` 重命名当前选中 · 其余视为选中（不存在时由 `ensure_action` 惰性创建）。
    fn run_abbr_command(&mut self) {
        let selected = self.selected_hotkey.clone().unwrap_or_default();
        let input = self.cmd_text.clone();
        let outcome = abbr::parse_command(&input, &selected);

        if outcome.clear_input {
            self.cmd_text.clear();
        }

        match outcome.command {
            abbr::AbbrCommand::None => {}
            abbr::AbbrCommand::Delete { hotkey } => {
                self.with_current_keymap(|keymap| {
                    keymap::remove_hotkey(keymap, &hotkey);
                });
            }
            abbr::AbbrCommand::Rename { from, to } => {
                self.with_current_keymap(|keymap| {
                    keymap::change_hotkey(keymap, &from, &to);
                });
            }
            abbr::AbbrCommand::Select { .. } => {}
        }

        // `del` 分支把选中置空（C# 的 `SelectedHotkey = ""`）；其余为新的目标键
        self.selected_hotkey = outcome.next_selection.filter(|next| !next.is_empty());
    }

    /// 写入动作字段（复刻各类型编辑器的 setter 与 `isEmpty` 规则）。
    fn apply_field(&mut self, field: ActionField) {
        let Some(action) = self.current_action_mut() else {
            return;
        };
        match field {
            ActionField::WinTitle(value) => {
                action.win_title = value;
                action_editor::refresh_empty_activate_or_run(action);
            }
            ActionField::Target(value) => {
                action.target = value;
                action_editor::refresh_empty_activate_or_run(action);
            }
            ActionField::Args(value) => action.args = value,
            ActionField::WorkingDir(value) => action.working_dir = value,
            ActionField::Comment(value) => action.comment = value,
            ActionField::KeysToSend(value) => action_editor::apply_keys_to_send(action, &value),
            ActionField::AhkCode(value) => action_editor::apply_ahk_code(action, &value),
            ActionField::RemapToKey(value) => action_editor::apply_remap(action, &value),
            ActionField::RunAsAdmin(value) => action.run_as_admin = value,
            ActionField::RunInBackground(value) => action.run_in_background = value,
            ActionField::DetectHiddenWindow(value) => action.detect_hidden_window = value,
        }
    }

    /// 复刻 `MaybeRefreshAbbrEnable`：类型 9 与取值 5/6 相互变化时重算缩写/命令 keymap 的启用态。
    fn maybe_refresh_abbr_enable(
        &mut self,
        old_type: i32,
        new_type: i32,
        old_value: i32,
        new_value: i32,
    ) {
        let type_involved = old_type == 9 || new_type == 9;
        let value_involved = matches!(old_value, 5 | 6) || matches!(new_value, 5 | 6);
        if !(type_involved && value_involved) {
            return;
        }
        if let Some(config) = self.config.as_mut() {
            store::change_abbr_enable(config);
        }
        self.rebuild_nav();
    }

    /// 重建导航并**保持当前选中项**（启用态变化会增删导航项）。
    fn rebuild_nav(&mut self) {
        let Some(config) = self.config.as_ref() else {
            return;
        };
        let nav = build_nav(config);
        let current_tag = self.nav.get(self.page_index).map(|entry| entry.tag.clone());
        self.nav = nav;
        if let Some(tag) = current_tag
            && let Some(index) = self.nav.iter().position(|entry| entry.tag == tag)
        {
            self.page_index = index;
        }
    }

    /// 动作编辑面板：两级下拉 + 按类型分发的编辑器。
    fn action_editor_panel(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return action_editor_view::frame(action_editor_view::hint("配置未加载"));
        };

        // 窗口分组下拉（复刻 `windowGroups.filter(id >= 0)`）
        let groups = action_editor::window_group_options(config);
        let group_ids: Vec<i32> = groups.iter().map(|group| group.id).collect();
        let group_index = group_ids.iter().position(|id| *id == self.window_group_id);
        let group_combo = action_editor_view::combo(
            groups.iter().map(|group| group.name.clone()).collect(),
            group_index,
            true,
            context.callback({
                let ids = group_ids.clone();
                move |index: Option<usize>| match index.and_then(|i| ids.get(i).copied()) {
                    Some(id) => Message::SelectWindowGroup(id),
                    None => Message::Noop,
                }
            }),
        );

        // 动作类型下拉（未选键时禁用；缩写语境隐藏 4/5）
        let is_abbr = self.current_keymap().map(keymap::is_abbr).unwrap_or(false);
        let types = action_editor::type_options(is_abbr);
        let type_ids: Vec<i32> = types.iter().map(|option| option.id).collect();
        let has_hotkey = self.selected_hotkey.is_some();
        let action = self.current_action();
        let type_index = match action {
            Some(action) => type_ids.iter().position(|id| *id == action.type_id),
            None => Some(0),
        };
        let type_combo = action_editor_view::combo(
            types.iter().map(|option| option.label()).collect(),
            type_index,
            has_hotkey,
            context.callback({
                let ids = type_ids.clone();
                move |index: Option<usize>| match index.and_then(|i| ids.get(i).copied()) {
                    Some(id) => Message::SelectActionType(id),
                    None => Message::Noop,
                }
            }),
        );

        // 编辑器主体（按 typeId 分发；复刻 `RebuildEditor`）
        let body: View = match action {
            None => action_editor_view::hint(if has_hotkey {
                "该键在当前窗口分组下尚无动作，请选择动作类型"
            } else {
                // 措辞需同时适配「模式页键盘网格」与「缩写页 chips」两种宿主
                "请先从左侧选中一个键或缩写条目"
            }),
            Some(action) => match action.type_id {
                0 => action_editor_view::hint("未配置"),
                1 => self.editor_activate_or_run(action, context),
                2 | 3 | 4 | 7 | 9 => self.editor_radio_group(action, context),
                5 => self.editor_remap(action, context),
                6 => self.editor_send_keys(action, context),
                8 => self.editor_ahk_code(action, context),
                _ => action_editor_view::hint("未知动作类型"),
            },
        };

        // 顶部两个下拉**无标签**（忠实复刻旧面板：靠选项内容自解释）
        let header: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(20.0)
            .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
            .children((group_combo, type_combo));

        let rows: Vec<(usize, View)> =
            vec![(0, header), (1, action_editor_view::divider()), (2, body)];

        action_editor_view::frame(StackPanel::new().spacing(0.0).keyed_children(rows))
    }

    /// 类型 1：启动程序或激活窗口。
    fn editor_activate_or_run(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let win_title = action_editor_view::text_box(
            &action.win_title,
            false,
            context.callback(|value: String| Message::EditField(ActionField::WinTitle(value))),
        );
        let args = action_editor_view::text_box(
            &action.args,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Args(value))),
        );
        let working_dir = action_editor_view::text_box(
            &action.working_dir,
            false,
            context.callback(|value: String| Message::EditField(ActionField::WorkingDir(value))),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Comment(value))),
        );

        // 目标：文本框 + 快捷方式下拉（选中即填入目标，复刻旧版的 shortcuts 下拉）
        let target = action_editor_view::text_box(
            &action.target,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Target(value))),
        );
        let shortcuts: View = if self.shortcuts.is_empty() {
            View::empty()
        } else {
            let paths = self.shortcuts.clone();
            action_editor_view::combo(
                paths.clone(),
                None,
                true,
                context.callback(move |index: Option<usize>| {
                    match index.and_then(|i| paths.get(i).cloned()) {
                        Some(path) => Message::EditField(ActionField::Target(path)),
                        None => Message::Noop,
                    }
                }),
            )
        };

        let error: View = match action_editor::evaluate_win_title_error(&action.win_title) {
            Some(message) => action_editor_view::field_error(message),
            None => View::empty(),
        };
        let hint: View = TextBlock::new()
            .text(i18n::t("301hint"))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::stone_gray())
            .text_wrapping(TextWrapping::Wrap)
            .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
            .into();
        let spy: View = Button::new()
            .on_click(context.message(Message::WindowSpy))
            .content(i18n::t("309"));
        let target_rows: Vec<(usize, View)> = vec![(0, target), (1, shortcuts)];

        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("301"), win_title)),
            (1, hint),
            (2, error),
            (
                3,
                action_editor_view::field(
                    i18n::t("302"),
                    StackPanel::new().spacing(6.0).keyed_children(target_rows),
                ),
            ),
            (4, action_editor_view::field(i18n::t("303"), args)),
            (5, action_editor_view::field(i18n::t("304"), working_dir)),
            (6, action_editor_view::field(i18n::t("305"), comment)),
            (
                7,
                action_editor_view::toggle(
                    i18n::t("306"),
                    action.run_as_admin,
                    context
                        .callback(|value: bool| Message::EditField(ActionField::RunAsAdmin(value))),
                ),
            ),
            (
                8,
                action_editor_view::toggle(
                    i18n::t("307"),
                    action.run_in_background,
                    context.callback(|value: bool| {
                        Message::EditField(ActionField::RunInBackground(value))
                    }),
                ),
            ),
            (
                9,
                action_editor_view::toggle(
                    i18n::t("308"),
                    action.detect_hidden_window,
                    context.callback(|value: bool| {
                        Message::EditField(ActionField::DetectHiddenWindow(value))
                    }),
                ),
            ),
            (10, spy),
        ];

        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 类型 2/3/4/7/9：枚举单选（两两一行）。
    fn editor_radio_group(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let is_abbr = self.current_keymap().map(keymap::is_abbr).unwrap_or(false);
        let rows = action_editor::radio_rows(action.type_id, is_abbr);
        let group_name = format!("kf-radio-{}-{}", action.type_id, self.window_group_id);
        action_editor_view::radio_rows(&rows, &group_name, action.value_id, |item| {
            context.callback(move |checked: bool| {
                if checked {
                    Message::SelectRadio {
                        value_id: item.value_id,
                        label_key: item.label_key,
                    }
                } else {
                    Message::Noop
                }
            })
        })
    }

    /// 类型 5：重映射按键。
    fn editor_remap(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let single_press = self.selected_hotkey.as_deref() == Some("singlePress");
        let value = action_editor_view::text_box(
            &action.remap_to_key,
            false,
            context.callback(|v: String| Message::EditField(ActionField::RemapToKey(v))),
        );
        let candidates = action_editor::REMAP_ITEMS.to_vec();
        let picker = action_editor_view::combo(
            candidates.iter().map(|key| (*key).to_string()).collect(),
            None,
            !single_press,
            context.callback({
                let keys: Vec<String> = candidates.iter().map(|key| (*key).to_string()).collect();
                move |index: Option<usize>| match index.and_then(|i| keys.get(i).cloned()) {
                    Some(key) => Message::EditField(ActionField::RemapToKey(key)),
                    None => Message::Noop,
                }
            }),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|v: String| Message::EditField(ActionField::Comment(v))),
        );

        let mut rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("401"), value)),
            (1, picker),
        ];
        if single_press {
            rows.push((2, action_editor_view::field_error(i18n::t("954"))));
        }
        rows.push((3, action_editor_view::field(i18n::t("305"), comment)));
        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 类型 6：输入按键或文本。
    fn editor_send_keys(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let keys = action_editor_view::text_box(
            &action.keys_to_send,
            true,
            context.callback(|v: String| Message::EditField(ActionField::KeysToSend(v))),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|v: String| Message::EditField(ActionField::Comment(v))),
        );
        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("402"), keys)),
            (1, action_editor_view::field(i18n::t("305"), comment)),
        ];
        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 类型 8：自定义函数。
    fn editor_ahk_code(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let code = action_editor_view::text_box(
            &action.ahk_code,
            true,
            context.callback(|v: String| Message::EditField(ActionField::AhkCode(v))),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|v: String| Message::EditField(ActionField::Comment(v))),
        );

        // 示例下拉：选中即写入代码框（复刻 `BuiltinFunction.vue` 的 items）
        let examples: Vec<String> = action_editor::AHK_EXAMPLES
            .iter()
            .map(|item| (*item).to_string())
            .collect();
        let example_picker = action_editor_view::combo(
            examples.clone(),
            None,
            true,
            context.callback(move |index: Option<usize>| {
                match index.and_then(|i| examples.get(i).cloned()) {
                    Some(code) => Message::EditField(ActionField::AhkCode(code)),
                    None => Message::Noop,
                }
            }),
        );
        let tips: View = StackPanel::new()
            .spacing(2.0)
            .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
            .children((
                TextBlock::new()
                    .text(i18n::t("955"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(i18n::t("956"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
            ));

        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("403"), code)),
            (1, example_picker),
            (2, action_editor_view::field(i18n::t("305"), comment)),
            (3, tips),
        ];
        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 键位图页：页头 + 键盘网格 + 动作编辑面板 + 右侧备注汇总。
    fn keymap_page(&self, context: &mut ViewContext<Self>, keymap_id: i32) -> View {
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

        // 左列三段式：页头 / 键盘网格（占满剩余高度 ⇒ ScrollViewer 才有界可滚）/ 动作面板
        // ⚠️ `View` 不实现 `LayoutControl` ⇒ `grid_row` 只能设在未收尾的 builder 上；
        //    已构建的 `View` 用 `Border` 包裹后再定位。
        let left: View = Border::new().grid_column(0).content(
            Grid::new()
                .rows([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .margin(Thickness::new(24.0, 20.0, 12.0, 28.0))
                .children((
                    Border::new().grid_row(0).content(keymap_view::page_header(
                        &keymap::header_title(keymap),
                        keymap::parent_info(keymap, config).as_deref(),
                    )),
                    Border::new()
                        .grid_row(1)
                        .content(ScrollViewer::new().content(grid)),
                    Border::new()
                        .grid_row(2)
                        .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                        .content(self.action_editor_panel(context)),
                )),
        );

        let entries = keymap::build_comment_entries(keymap, config);
        let comments: View = if entries.is_empty() {
            keymap_view::comment_empty_hint()
        } else {
            keymap_view::comment_summary(&entries)
        };

        let right: View = Border::new()
            .grid_column(1)
            .margin(Thickness::new(20.0, 20.0, 4.0, 28.0))
            .content(comments);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((left, right))
    }

    /// 缩写页（id 2/3）：页头 + chips 网格 + 命令框 + 动作编辑面板 + 右侧备注汇总。
    fn abbr_page(&self, context: &mut ViewContext<Self>, keymap_id: i32) -> View {
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
        let command: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .keyed_children(vec![
                (
                    0usize,
                    abbr_view::command_box(
                        &self.cmd_text,
                        &i18n::t("406"),
                        context.callback(|value: String| Message::CmdText(value)),
                    ),
                ),
                (
                    1usize,
                    abbr_view::run_button(i18n::t("920"), context.message(Message::RunCmd)),
                ),
            ]);

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
        let comments: View = if entries.is_empty() {
            keymap_view::comment_empty_hint()
        } else {
            keymap_view::comment_summary(&entries)
        };

        let right: View = Border::new()
            .grid_column(1)
            .margin(Thickness::new(20.0, 20.0, 4.0, 28.0))
            .content(comments);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((left, right))
    }

    /// 指南页：`config.overviewDocMd` 优先，为空时已在后台拉取 `/config_doc.md`。
    fn guide_view(&self) -> View {
        let Some(port) = self.port else {
            return TextBlock::new().text("后端未连接").into();
        };

        let body: View = if self.doc_md.trim().is_empty() {
            TextBlock::new()
                .text("（未找到使用指南文档：config.overviewDocMd 为空且 /config_doc.md 不可读）")
                .foreground(theme::stone_gray())
                .text_wrapping(TextWrapping::Wrap)
                .into()
        } else {
            let blocks = markdown::parse(&self.doc_md);
            let rendered = markdown_view::render(&blocks, port);
            let content: View = StackPanel::new()
                .spacing(10.0)
                .margin(Thickness::new(28.0, 20.0, 28.0, 28.0))
                .children((
                    TextBlock::new()
                        .text(self.current_title())
                        .font_size(28.0)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::near_black()),
                    rendered,
                ));
            // 文档较长 ⇒ 纵向滚动（Fluent：内容区可滚动，页面不整体滚动）
            ScrollViewer::new().content(content)
        };

        body
    }

    fn placeholder_page(&self, hint: &str, title: &str) -> View {
        let keymap_count = self
            .config
            .as_ref()
            .map(|config| config.keymaps.len())
            .unwrap_or(0);

        StackPanel::new()
            .spacing(10.0)
            .margin(theme::pad_lg())
            .children((
                TextBlock::new()
                    .text(title.to_string())
                    .font_size(28.0)
                    .font_weight(FontWeight::BOLD)
                    .foreground(theme::near_black()),
                TextBlock::new()
                    .text(hint.to_string())
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(format!("后端已连接 · keymap {keymap_count} 个"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::OLIVE_GRAY)),
                Border::new()
                    .padding(theme::pad_md())
                    .background(theme::card_background())
                    .border_brush(theme::card_stroke())
                    .border_thickness(theme::hairline())
                    .corner_radius(theme::radius_md())
                    .content(
                        TextBlock::new()
                            .text("Phase 3 进行中：本页内容待迁移。")
                            .opacity(0.75),
                    ),
            ))
    }
}

// ------------------------------------------------------------- 后台工作（非 UI 线程）

/// 连接后端（直连或子进程）、拉取配置与使用指南文档；会话移交到组件持有的槽。
fn load_backend(slot: SessionSlot) -> Message {
    let args: Vec<String> = std::env::args().collect();
    let options = BackendSessionOptions::parse(&args);

    let session = match connect(&options) {
        Ok(session) => session,
        Err(reason) => return Message::Failed(reason),
    };

    let response = session.api().get_config();
    let Some(config) = response.value.clone() else {
        return Message::Failed(
            response
                .error_message
                .unwrap_or_else(|| format!("读取配置失败 (HTTP {})", response.status)),
        );
    };
    let port = session.port();

    // 使用指南文档：自定义内容优先，为空时取后端静态站的默认文档
    // （对齐旧 `HomePageViewModel.LoadAsync`）。
    let mut doc_md = config.overview_doc_md.clone();
    if doc_md.trim().is_empty() {
        doc_md = session
            .api()
            .get_raw_text("/config_doc.md")
            .value
            .unwrap_or_default();
    }

    // 快捷方式列表（`GET /shortcuts`；空目录后端返回 null ⇒ 容忍为空）
    let shortcuts = session
        .api()
        .get_shortcuts()
        .value
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.path)
        .collect::<Vec<_>>();

    if let Ok(mut guard) = slot.lock() {
        *guard = Some(session);
    }

    Message::Ready {
        config: Box::new(config),
        port,
        doc_md,
        shortcuts,
        data_root: resolve_deployment_root(&options),
    }
}

/// 解析部署根（`<deploy>`）：`<deploy>/bin/settings.exe` ⇒ 其祖父目录。
///
/// 直连模式（`--port`）下无从推断，退回 `--backend-dir` 指定的工作目录。
fn resolve_deployment_root(options: &BackendSessionOptions) -> Option<std::path::PathBuf> {
    if options.direct_port.is_some() {
        return options.working_directory.clone();
    }
    let exe_dir = std::env::current_exe()
        .ok()?
        .parent()
        .map(std::path::Path::to_path_buf)?;
    let exe = resolve_settings_exe(options, &exe_dir)?;
    exe.parent()?.parent().map(std::path::Path::to_path_buf)
}

fn connect(options: &BackendSessionOptions) -> Result<BackendSession, String> {
    if let Some(port) = options.direct_port {
        return BackendSession::connect_direct(port, options).map_err(|error| error.to_string());
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .ok_or_else(|| "无法确定当前可执行文件目录".to_string())?;
    let exe = resolve_settings_exe(options, &exe_dir).ok_or_else(|| {
        let searched = crate::services::backend::settings_exe_candidates(&exe_dir)
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("; ");
        format!("未找到 settings.exe (查找位置: {searched})")
    })?;

    BackendSession::spawn(&exe, options.working_directory.as_deref(), options)
        .map_err(|error| error.to_string())
}

/// 清洗后 PUT 配置（`ConfigSaver` 语义），返回保存提示文案。
fn save(port: u16, config: &Config) -> Result<String, String> {
    let payload = store::clean_for_save(config);
    let api = HttpSettingsApi::new(port);
    let response: ApiResponse<MessageBody> = api.save_config(&payload);

    if !response.success {
        return Err(response
            .error_message
            .unwrap_or_else(|| format!("保存失败 (HTTP {})", response.status)));
    }

    // `restartFailed`：保存已落盘但引擎重启失败 ⇒ 引导用户经托盘「重载」手动生效。
    if response.value.as_ref().and_then(|body| body.restart_failed) == Some(true) {
        return Ok(format!(
            "{}（引擎重启失败，请在托盘点「重载」）",
            i18n::t("928")
        ));
    }

    Ok(i18n::t("928"))
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
