using System.Text.Json.Serialization;

namespace KeyFlux.Settings.Models;

// ============================================================================
// KeyFlux 配置数据模型 (C# 复刻)
//
// 权威蓝本: config-server/internal/script/model/types.go
// 约定:
//   1. 每个类的属性声明顺序与对应 Go 结构体的字段顺序严格一致;
//   2. [JsonPropertyName] 与 Go json tag 逐字对齐 (含大小写);
//   3. Go 侧 `json:"-"` 的字段 (Config.KeyMapping / Action.RemapInHotIf)
//      不参与 JSON 序列化, 故本模型不定义;
//   4. Go 侧带 omitempty 的字段 (如 fileGroups
//      以及 Action 的大部分字段) 在 JSON 中可能缺失, 反序列化时落回
//      C# 属性默认值 (空集合 / 空串 / 0 / false), 模型一律以非空类型容忍缺失;
//   5. selectedAction 在 Go 侧为指针 + omitempty, 但 ParseConfig 读时保证非 nil,
//      GET/PUT /config 全链路恒为对象 —— C# 侧以非空引用 + 默认 new() 对齐。
// ============================================================================

/// <summary>对应 Go struct Config。顶层配置, GET/PUT /config 的载荷。</summary>
public sealed class Config
{
    [JsonPropertyName("keymaps")]
    public List<Keymap> Keymaps { get; set; } = [];

    [JsonPropertyName("options")]
    public Options Options { get; set; } = new();

    // 选中动作单键分发 (方案 D, 2026-09 重构): GET/PUT /config 全链路携带, 恒对象;
    // 旧 actionSchemes 多方案结构已由 Go ParseConfig 读时一次性迁移 (save 不再输出),
    // C# 侧不再建模 (迁移仅发生在后端读路径, 前端永远只见 selectedAction)。
    [JsonPropertyName("selectedAction")]
    public SelectedAction SelectedAction { get; set; } = new();

    // omitempty: 文件分组快捷填充数据, 缺失时为空列表
    [JsonPropertyName("fileGroups")]
    public List<FileGroup> FileGroups { get; set; } = [];

    // omitempty: 自定义总览页 Markdown, 缺失时为 ""; 非空时总览页优先展示自定义内容
    [JsonPropertyName("overviewDocMd")]
    public string OverviewDocMd { get; set; } = "";
}

/// <summary>对应 Go struct Keymap。单个键盘映射 (一页按键矩阵)。</summary>
public sealed class Keymap
{
    [JsonPropertyName("id")]
    public int Id { get; set; }

    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("enable")]
    public bool Enable { get; set; }

    [JsonPropertyName("hotkey")]
    public string Hotkey { get; set; } = "";

    [JsonPropertyName("parentID")]
    public int ParentId { get; set; }

    [JsonPropertyName("delay")]
    public int Delay { get; set; }

    [JsonPropertyName("disableAt")]
    public string DisableAt { get; set; } = "";

    /// <summary>
    /// 前端专用标记 (对应 Vue Keymap.isNew, 不参与序列化):
    /// 新增的 keymap 首次选中 singlePress 时自动填入默认动作 (任务 #10 页面使用)。
    /// </summary>
    [JsonIgnore]
    public bool IsNew { get; set; }

    /// <summary>按键 -> 动作列表。键为按键名 (如 "a"、"*1"), 值为该按键绑定的动作序列。</summary>
    [JsonPropertyName("hotkeys")]
    public Dictionary<string, List<Action>> Hotkeys { get; set; } = new();
}

/// <summary>
/// 对应 Go struct SelectedAction。选中动作单键分发 (方案 D):
/// 选中内容后按单一热键触发, 按 mappings 顺序匹配第一个命中的 mapping,
/// 弹出其 entries 菜单 (最多 9 项) 由用户按数字键选择行为。
/// </summary>
public sealed class SelectedAction
{
    [JsonPropertyName("hotkey")]
    public string Hotkey { get; set; } = "";

    [JsonPropertyName("enable")]
    public bool Enable { get; set; }

    /// <summary>有序映射列表 (分区组内行序 = 匹配优先级); 后端契约空时恒 []。</summary>
    [JsonPropertyName("mappings")]
    public List<SelectedMapping> Mappings { get; set; } = [];
}

/// <summary>
/// 对应 Go struct SelectedMapping。一个匹配前提桶: 同 matchType+matchValue 的行为菜单。
/// 语义对齐旧 ActionRule 的 matchType/matchValue (fileExt=逗号分隔后缀, textType=url/path/magnet/plain)。
/// </summary>
public sealed class SelectedMapping
{
    [JsonPropertyName("matchType")]
    public string MatchType { get; set; } = ""; // "fileExt" | "textType"

    [JsonPropertyName("matchValue")]
    public string MatchValue { get; set; } = "";

    /// <summary>1..9 项, 顺序即菜单序号; 后端契约空时恒 []。</summary>
    [JsonPropertyName("entries")]
    public List<SelectedEntry> Entries { get; set; } = [];
}

/// <summary>
/// 对应 Go struct SelectedEntry。菜单项: behavior = 行为库 ID (内置 11 个基础动作 ID 或用户行为包 ID)。
/// actionValue/workingDir 非空时覆盖行为包默认模板, 空则用包默认 (复用 behaviors.ResolveRuleAction 语义);
/// 空串经 *Json 代理属性省略 (null + WhenWritingNull 整属性跳过), 与 Go json omitempty 缺键字节对齐。
/// (JsonIgnoreCondition.WhenWritingDefault 对 string 无效: "" 非 default(string)=null)
/// </summary>
public sealed class SelectedEntry
{
    [JsonPropertyName("behavior")]
    public string Behavior { get; set; } = "";

    /// <summary>命令模板 / 目标值; 空串 = 用行为包默认。</summary>
    [JsonIgnore]
    public string ActionValue { get; set; } = "";

    /// <summary>actionValue 序列化代理: 空串 -> null (整属性省略), 读侧 null 归一 ""。</summary>
    [JsonPropertyName("actionValue")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? ActionValueJson
    {
        get => ActionValue.Length == 0 ? null : ActionValue;
        set => ActionValue = value ?? "";
    }

    /// <summary>工作目录; 空串 = 不设置。</summary>
    [JsonIgnore]
    public string WorkingDir { get; set; } = "";

    /// <summary>workingDir 序列化代理: 空串 -> null (整属性省略), 读侧 null 归一 ""。</summary>
    [JsonPropertyName("workingDir")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? WorkingDirJson
    {
        get => WorkingDir.Length == 0 ? null : WorkingDir;
        set => WorkingDir = value ?? "";
    }

    [JsonPropertyName("options")]
    public RuleOptions Options { get; set; } = new();
}

/// <summary>
/// 对应 Go struct RuleOptions。行为执行三开关 (复制到剪贴板 / 清除选区 / 执行前确认),
/// 方案 D 前挂于旧 ActionRule.Options, 现由 SelectedEntry 携带, 语义与 json tag 不变。
/// </summary>
public sealed class RuleOptions
{
    [JsonPropertyName("copyToClipboard")]
    public bool CopyToClipboard { get; set; }

    [JsonPropertyName("clearSelection")]
    public bool ClearSelection { get; set; }

    [JsonPropertyName("confirm")]
    public bool Confirm { get; set; }
}

/// <summary>
/// 对应 Go struct FileGroup。文件分组: 前端「文件后缀」条件值的快捷填充数据
/// (选择分组 -> 展开为逗号分隔后缀列表), 引擎层不感知分组概念。
/// </summary>
public sealed class FileGroup
{
    /// <summary>分组标识 (英文, 如 image)。</summary>
    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    /// <summary>中文显示名 (如 图片)。</summary>
    [JsonPropertyName("label")]
    public string Label { get; set; } = "";

    /// <summary>后缀列表 (不含点, 如 ["jpg","jpeg"])。</summary>
    [JsonPropertyName("exts")]
    public List<string> Exts { get; set; } = [];
}

/// <summary>对应 Go struct Action。按键绑定的单个动作, 部分字段因动作类型而异。</summary>
public sealed class Action
{
    [JsonPropertyName("windowGroupID")]
    public int WindowGroupId { get; set; }

    // json tag 为 actionTypeID (非 typeID), 与 Go 侧一致
    [JsonPropertyName("actionTypeID")]
    public int TypeId { get; set; }

    // omitempty
    [JsonPropertyName("comment")]
    public string Comment { get; set; } = "";

    // omitempty
    [JsonPropertyName("hotkey")]
    public string Hotkey { get; set; } = "";

    // ---- 下面的字段因动作类型而异 (均 omitempty) ----

    [JsonPropertyName("keysToSend")]
    public string KeysToSend { get; set; } = "";

    [JsonPropertyName("remapToKey")]
    public string RemapToKey { get; set; } = "";

    // json tag 为 actionValueID
    [JsonPropertyName("actionValueID")]
    public int ValueId { get; set; }

    [JsonPropertyName("winTitle")]
    public string WinTitle { get; set; } = "";

    [JsonPropertyName("target")]
    public string Target { get; set; } = "";

    [JsonPropertyName("args")]
    public string Args { get; set; } = "";

    [JsonPropertyName("workingDir")]
    public string WorkingDir { get; set; } = "";

    [JsonPropertyName("runAsAdmin")]
    public bool RunAsAdmin { get; set; }

    [JsonPropertyName("runInBackground")]
    public bool RunInBackground { get; set; }

    [JsonPropertyName("detectHiddenWindow")]
    public bool DetectHiddenWindow { get; set; }

    [JsonPropertyName("ahkCode")]
    public string AhkCode { get; set; } = "";

    /// <summary>
    /// 前端专用的「未配置」哨兵标记 (对应 Vue Action.isEmpty, Go 无此字段, 不参与序列化):
    /// 动作编辑页新建插槽时置 true; 保存清洗 (ConfigSaver) 会剔除 isEmpty 的动作。
    /// </summary>
    [JsonIgnore]
    public bool IsEmpty { get; set; }
}

/// <summary>对应 Go struct Options。全局选项。</summary>
public sealed class Options
{
    [JsonPropertyName("hideMatrix")]
    public bool HideMatrix { get; set; }

    [JsonPropertyName("keyfluxVersion")]
    public string KeyfluxVersion { get; set; } = "";

    [JsonPropertyName("windowGroups")]
    public List<WindowGroup> WindowGroups { get; set; } = [];

    [JsonPropertyName("mouse")]
    public Mouse Mouse { get; set; } = new();

    [JsonPropertyName("scroll")]
    public Scroll Scroll { get; set; } = new();

    [JsonPropertyName("commandInputSkin")]
    public CommandInputSkin CommandInputSkin { get; set; } = new();

    [JsonPropertyName("pathVariables")]
    public List<PathVariable> PathVariables { get; set; } = [];

    [JsonPropertyName("startup")]
    public bool Startup { get; set; }

    [JsonPropertyName("language")]
    public string Language { get; set; } = "";

    [JsonPropertyName("keyMapping")]
    public string KeyMapping { get; set; } = "";

    [JsonPropertyName("keyboardLayout")]
    public string KeyboardLayout { get; set; } = "";

    [JsonPropertyName("quickSwitch")]
    public QuickSwitchOption QuickSwitch { get; set; } = new();

    [JsonPropertyName("plugins")]
    public PluginsOption? Plugins { get; set; } = new();
}

/// <summary>对应 Go struct PluginsOption。插件注册表 (第三方插件的启用状态)。</summary>
public sealed class PluginsOption
{
    /// <summary>
    /// 已停用的插件 ID 列表 (缺省 = 启用)。只记「已停用」而非「已启用」:
    /// 新导入的插件默认启用, 不需要写注册表; 引擎运行时就绪后按此表过滤加载。
    /// </summary>
    [JsonPropertyName("disabled")]
    public List<string> Disabled { get; set; } = [];
}

/// <summary>对应 Go struct QuickSwitchOption。快速切换 (QuickSwitch) 配置段。</summary>
public sealed class QuickSwitchOption
{
    [JsonPropertyName("collectEnabled")]
    public bool CollectEnabled { get; set; }

    [JsonPropertyName("autoShow")]
    public bool AutoShow { get; set; }

    [JsonPropertyName("autoJumpOpen")]
    public bool AutoJumpOpen { get; set; }

    [JsonPropertyName("autoJumpSave")]
    public bool AutoJumpSave { get; set; }

    [JsonPropertyName("pollIntervalMs")]
    public int PollIntervalMs { get; set; }

    [JsonPropertyName("maxHistory")]
    public int MaxHistory { get; set; }

    [JsonPropertyName("overlayRows")]
    public int OverlayRows { get; set; }

    [JsonPropertyName("overlayRowsCompact")]
    public int OverlayRowsCompact { get; set; }

    [JsonPropertyName("excludedPrefixes")]
    public List<string> ExcludedPrefixes { get; set; } = [];
}

/// <summary>对应 Go struct WindowGroup。窗口分组 (动作生效的窗口条件)。</summary>
public sealed class WindowGroup
{
    [JsonPropertyName("id")]
    public int Id { get; set; }

    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    // omitempty
    [JsonPropertyName("value")]
    public string Value { get; set; } = "";

    // omitempty
    [JsonPropertyName("conditionType")]
    public int ConditionType { get; set; }
}

/// <summary>对应 Go struct Mouse。鼠标模式参数。</summary>
public sealed class Mouse
{
    [JsonPropertyName("keepMouseMode")]
    public bool KeepMouseMode { get; set; }

    [JsonPropertyName("showTip")]
    public bool ShowTip { get; set; }

    [JsonPropertyName("tipSymbol")]
    public string TipSymbol { get; set; } = "";

    [JsonPropertyName("delay1")]
    public string Delay1 { get; set; } = "";

    [JsonPropertyName("delay2")]
    public string Delay2 { get; set; } = "";

    [JsonPropertyName("fastSingle")]
    public string FastSingle { get; set; } = "";

    [JsonPropertyName("fastRepeat")]
    public string FastRepeat { get; set; } = "";

    [JsonPropertyName("slowSingle")]
    public string SlowSingle { get; set; } = "";

    [JsonPropertyName("slowRepeat")]
    public string SlowRepeat { get; set; } = "";
}

/// <summary>对应 Go struct Scroll。滚动模式参数。</summary>
public sealed class Scroll
{
    [JsonPropertyName("delay1")]
    public string Delay1 { get; set; } = "";

    [JsonPropertyName("delay2")]
    public string Delay2 { get; set; } = "";

    [JsonPropertyName("onceLineCount")]
    public string OnceLineCount { get; set; } = "";
}

/// <summary>对应 Go struct PathVariable。路径变量 (命令中可引用)。</summary>
public sealed class PathVariable
{
    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("value")]
    public string Value { get; set; } = "";
}

/// <summary>对应 Go struct CommandInputSkin。命令输入窗口外观 (数值均为字符串)。</summary>
public sealed class CommandInputSkin
{
    [JsonPropertyName("backgroundColor")]
    public string BackgroundColor { get; set; } = "";

    [JsonPropertyName("backgroundOpacity")]
    public string BackgroundOpacity { get; set; } = "";

    [JsonPropertyName("borderWidth")]
    public string BorderWidth { get; set; } = "";

    [JsonPropertyName("borderColor")]
    public string BorderColor { get; set; } = "";

    [JsonPropertyName("borderOpacity")]
    public string BorderOpacity { get; set; } = "";

    [JsonPropertyName("borderRadius")]
    public string BorderRadius { get; set; } = "";

    [JsonPropertyName("cornerColor")]
    public string CornerColor { get; set; } = "";

    [JsonPropertyName("cornerOpacity")]
    public string CornerOpacity { get; set; } = "";

    [JsonPropertyName("gridlineColor")]
    public string GridlineColor { get; set; } = "";

    [JsonPropertyName("gridlineOpacity")]
    public string GridlineOpacity { get; set; } = "";

    [JsonPropertyName("keyColor")]
    public string KeyColor { get; set; } = "";

    [JsonPropertyName("keyOpacity")]
    public string KeyOpacity { get; set; } = "";

    [JsonPropertyName("hideAnimationDuration")]
    public string HideAnimationDuration { get; set; } = "";

    [JsonPropertyName("windowYPos")]
    public string WindowYPos { get; set; } = "";

    [JsonPropertyName("windowWidth")]
    public string WindowWidth { get; set; } = "";

    [JsonPropertyName("windowShadowColor")]
    public string WindowShadowColor { get; set; } = "";

    [JsonPropertyName("windowShadowOpacity")]
    public string WindowShadowOpacity { get; set; } = "";

    [JsonPropertyName("windowShadowSize")]
    public string WindowShadowSize { get; set; } = "";
}
