//! `app` 的消息与界面枚举**类型定义**（自原 `app.rs` 拆分）。
//!
//! 本文件只放类型/枚举与 `build_nav`，不含行为逻辑。

use super::*;

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
    pub(super) fn title(self) -> String {
        match self {
            Self::Guide => i18n::t("913"),
            Self::SelectedAction => i18n::t("914"),
            Self::Plugins => i18n::t("2418"),
            Self::Settings => i18n::t("2581"),
            Self::Abbr(_) | Self::Keymap(_) => String::new(),
        }
    }

    /// 导航图标（Segoe Fluent Icons 字形；折叠窄轨仅显示图标，展开浮层图标+文字）。
    pub(super) fn glyph(self) -> &'static str {
        match self {
            Self::Guide => "\u{E8E9}",          // Read（使用指南）
            Self::SelectedAction => "\u{E73E}", // CheckMark（选中动作）
            Self::Plugins => "\u{E8C8}",        // Puzzle（插件）
            Self::Settings => "\u{E713}",       // Setting（选项）
            Self::Abbr(_) => "\u{E8C1}",        // Link（缩写）
            Self::Keymap(_) => "\u{E765}",      // Keyboard（按键矩阵）
        }
    }
}

/// 导航条目。
#[derive(Clone, Debug)]
pub struct NavEntry {
    /// 稳定 tag（`NavigationView` 选中匹配 + 重建后保持选中）。
    pub(super) tag: String,
    pub(super) label: String,
    pub(super) kind: PageKind,
}

/// 导航构建（复刻 `MainViewModel.BuildNav`）：
/// 使用指南 + 选中动作 + 插件 + **所有 `enable && id != 1` 的 keymap**。
///
/// `id == 1`（自定义热键）不入导航 —— 2026-09-08 起迁入设置页「其他设置」卡片。
pub fn build_nav(config: &Config) -> Vec<NavEntry> {
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
pub type SessionSlot = Arc<Mutex<Option<BackendSession>>>;

#[derive(Clone)]
pub enum Message {
    /// 导航切换（`None` = 列表重建导致的瞬时清空 ⇒ 忽略，对齐旧版防闪白逻辑）。
    Nav(Option<String>),
    /// 点选键格 / 缩写条目（复刻 `Key.vue` click；禁用键已在状态层拦住）。
    SelectKey(String),
    /// 键位/缩写页右侧备注汇总折叠 ⇄ 展开（布局优先保证键盘网格完整显示）。
    ToggleComments,
    /// 导航窗格浮层展开态回写（LeftCompact 模式下汉堡切换；驱动页脚窄轨/完整切换）。
    PaneOverlay(bool),
    /// 选项页「亚克力毛玻璃效果」开关：更新状态并持久化到面板私有偏好文件。
    AcrylicToggle(bool),
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
        /// 面板私有 UI 偏好（`<deploy>/data/ui-prefs.json`，与共享 config 解耦）。
        ui_prefs: crate::services::ui_prefs::UiPrefs,
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
    /// 行为行：切换行为（pick = 覆盖序；切换重置默认模板，复刻 `EntryRowVm`）。
    SaEntrySwitch {
        match_type: &'static str,
        index: usize,
        pick: usize,
    },
    /// 行为行上移/下移（`delta` = -1 / +1；顺序即菜单数字键）。
    SaEntryMove {
        match_type: &'static str,
        index: usize,
        delta: i32,
    },
    /// 编辑映射内第 `index` 个行为的命令模板。
    SaEntryValue {
        match_type: &'static str,
        index: usize,
        value: String,
    },
    /// 编辑映射内第 `index` 个行为的工作目录。
    SaEntryWorkingDir {
        match_type: &'static str,
        index: usize,
        value: String,
    },
    /// ▶ 真实执行当前点亮类型（`POST /api/selected-action/play`，白名单 typeId）。
    SaPlaySample,
    /// ▶ 执行结果（Err = 失败原因；成功不打扰）。
    SaPlayDone(Result<(), String>),
    /// 打开「删除映射」确认框（1109 文案；确认后才删除 + 保存）。
    SaDeleteAsk,
    /// 确认删除当前点亮类型的映射。
    SaDeleteConfirmed,
    /// 取消删除确认框。
    SaDeleteCancelled,
    /// 打开「添加映射」弹窗。
    SaAddOpen,
    /// 清除选中动作热键（输入行尾 ✕，复刻旧 HotkeyCapture 的清除钮）。
    SaHotkeyClear,
    /// 卡内「＋ 新建匹配类型」（2553）：直接打开匹配类型对话框并预置对应 kind 的草稿。
    SaNewType {
        kind: &'static str,
    },
    /// 「添加映射」类型下拉选择（`None` = 未选）。
    SaAddType(Option<usize>),
    /// 「添加映射」行为勾选/取消（`checked` 保序 = 菜单序）。
    SaAddToggle(usize, bool),
    /// 「添加映射」确认（重复条件 1115 拦截）。
    SaAddConfirm,
    /// 「添加映射」取消。
    SaAddCancel,
    /// 行为编辑后的尾随保存（800ms 防抖到期且仍是最新一代 ⇒ 真正落盘）。
    SaSaveThrottled,
    // ---------------------------------------------------------- 匹配类型管理
    /// 打开「管理匹配类型」对话框（2519）。
    MatchTypesOpen,
    /// 关闭（丢弃草稿）。
    MatchTypesClose,
    /// 选中既有类型（`config.match_types` 下标；`None` = 回到新建草稿）。
    MatchTypesPick(Option<usize>),
    /// 新建草稿。
    MtNew,
    /// 名称（2523）。
    MtLabel(String),
    /// 英文名称（2567）。
    MtLabelEn(String),
    /// 类型（0 = text 2556 / 1 = fileExt 2551；仅草稿态可改）。
    MtKind(usize),
    /// 规则算子（2512 equals / 2513 prefix / 2514 suffix / 2515 contains）。
    MtRuleOp(usize, usize),
    /// 规则值。
    MtRuleValue(usize, String),
    /// 追加一条规则。
    MtRuleAdd,
    /// 删除一条规则。
    MtRuleRemove(usize),
    /// 后缀串（2528）。
    MtExts(String),
    /// 保存：`true` = 同时创建专属行为并强绑定（2529）。
    MtSave(bool),
    /// 删除类型（级联删 `type:` 引用映射 + 同名专属行为包）。
    MtDelete,
    /// 「试一下」示例内容（`POST /api/selected-action/test`）。
    MtTest(String),
    /// 提交「试一下」（携带编辑中快照：未保存的类型草稿也参与匹配）。
    MtTestRun,
    /// 「试一下」结果（Ok = 命中预览；空串 = 未命中；Err = 请求失败）。
    MtTestDone(Result<String, String>),
    // ---------------------------------------------------------- 行为库
    /// 打开「管理行为」对话框（1083）。
    BehaviorsOpen,
    /// 关闭。
    BehaviorsClose,
    /// 选中目录项（内置在前；`None` = 未选）。
    BhPick(Option<usize>),
    /// 新建行为草稿。
    BhNew,
    /// 名称。
    BhName(String),
    /// 包 ID（新建可改）。
    BhId(String),
    /// 描述。
    BhDescription(String),
    /// 前提类型（0 = textType / 1 = fileExt）。
    BhAppliesKind(usize, usize),
    /// 前提值（后缀串 / 特征值）。
    BhAppliesValue(usize, String),
    /// 前提默认推荐开关。
    BhAppliesDefault(usize, bool),
    /// 追加前提行。
    BhAppliesAdd,
    /// 删除前提行。
    BhAppliesRemove(usize),
    /// 基础动作下拉。
    BhBaseAction(usize),
    /// 默认模板。
    BhTemplate(String),
    /// 默认工作目录。
    BhWorkingDir(String),
    /// 保存（新建 POST / 编辑 PUT；内置包后端拒绝）。
    BhSave,
    /// 删除（仅用户包）。
    BhDelete,
    /// 立即生效（`POST /api/behaviors/apply`，重启引擎）。
    BhApplyNow,
    /// 行为目录刷新完成（保存/删除后重拉）。
    BhReloaded(Result<(), String>),
    /// 保存结果。
    BhSaved(Result<(), String>),
    /// 应用结果（Err = 失败原因；Ok 忽略 `restartFailed` 差异——引擎重启失败由 1079 引导）。
    BhApplied(Result<(), String>),
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
    /// 打开插件配置（声明了 settings 的插件；P6 起 QuickSwitch 亦走此路）。
    PluginConfigure(String),
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
    /// 清除页脚提示（928 成功提示 2 秒自动消失，复刻旧版 `Task.Delay(2000)`）。
    ClearNotice,
    // ---------------------------------------------------------- 指南编辑（OverviewEdit）
    /// 打开「编辑使用指南」对话框（指南页底部编辑入口，复刻 `EditZoneHint`）。
    GuideEditOpen,
    /// 总览 markdown 编辑。
    GuideEditValue(String),
    /// 恢复默认文档（2404：清空 overviewDocMd ⇒ 引擎回落到站内 config_doc.md）。
    GuideEditReset,
    /// 保存指南文档（overviewDocMd + 立即保存，复刻旧 `SaveAsync(force:true)`）。
    GuideEditSave,
    /// 关闭编辑对话框（丢弃未保存修改）。
    GuideEditClose,
    // ---------------------------------------------------------- 自定义热键动作编辑
    /// 打开第 `row` 行自定义热键的动作编辑（复刻 `ActionEditorWindow`，keymap id=1）。
    CustomHotkeyEdit(usize),
    /// 关闭动作编辑对话框。
    CustomHotkeyEditClose,
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
    CustomHotkeyRemove(usize),
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

/// 「添加映射」弹窗草稿（复刻 `AddMappingVm` + `BehaviorPickVm`：类型下拉 + 勾选序）。
#[derive(Debug, Clone, Default)]
pub struct SaAddDraft {
    /// 类型下拉下标（`None` = 未选；候选项由 [`sa_add_type_options`] 生成）。
    pub(super) type_pick: Option<usize>,
    /// 已勾选行为 ID（**保序** = 菜单数字键序）。
    pub(super) checked: Vec<String>,
    /// 弹窗内错误（重复条件 1115 等）。
    pub(super) error: Option<String>,
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
