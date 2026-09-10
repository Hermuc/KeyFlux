using System.Collections.ObjectModel;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

// ============================================================================
// 动作编辑面板 (复刻 components/actions/Action.vue 与各动作编辑组件)
//
// 结构对照:
//   Action.vue          -> ActionEditorViewModel (窗口分组下拉 + 动作类型下拉 + 动态编辑器)
//   ActivateOrRun.vue   -> ActivateOrRunEditorVm   (类型 1)
//   System/Window/Mouse/
//   Text/KeyFlux.vue   -> RadioGroupEditorVm      (类型 2/3/4/7/9, 枚举按 RadioCatalog 对齐)
//   RemapKey.vue        -> RemapEditorVm           (类型 5)
//   SendKey.vue         -> SendKeysEditorVm        (类型 6)
//   BuiltinFunction.vue -> AhkCodeEditorVm         (类型 8)
//
// isEmpty 语义逐类型复刻各组件的 watchEffect (见各 VM 属性写入处)。
// ============================================================================

/// <summary>动作类型下拉项 (label 为文案键, 展示时经 I18n 翻译)。</summary>
public sealed record ActionTypeOption(int Id, string Label)
{
    public string Display => I18n.T(Label);
    public override string ToString() => Display;
}

/// <summary>窗口分组下拉项 (复刻 windowGroups.filter(x => x.id >= 0))。</summary>
public sealed record WindowGroupOption(int Id, string Name)
{
    public override string ToString() => Name;
}

/// <summary>
/// 动作编辑面板 VM: 窗口分组/动作类型两级选择 + 按 actionTypeID 分发的编辑器。
/// 当前编辑对象是内存 Config 里的 Action 实例引用 (编辑器直接写入, 保存走既有链路)。
/// </summary>
public sealed partial class ActionEditorViewModel : ObservableObject
{
    private readonly KeymapEditorCore _core;
    private bool _suppressTypeChange;

    public ActionEditorViewModel(KeymapEditorCore core)
    {
        _core = core;
        GroupOptions = BuildGroupOptions();
        TypeOptions = BuildTypeOptions();
        _selectedGroup = GroupOptions.FirstOrDefault();
        _selectedType = TypeOptions[0];
    }

    public KeymapEditorCore Core => _core;
    public Models.Action? CurrentAction { get; private set; }

    public IReadOnlyList<WindowGroupOption> GroupOptions { get; private set; }
    public IReadOnlyList<ActionTypeOption> TypeOptions { get; private set; }

    /// <summary>窗口分组选择 (复刻 Action.vue 的 windowGroupID 下拉)。</summary>
    [ObservableProperty]
    private WindowGroupOption? _selectedGroup;

    /// <summary>动作类型选择 (复刻 actionTypeID 下拉; 未选键时禁用)。</summary>
    [ObservableProperty]
    private ActionTypeOption? _selectedType;

    /// <summary>当前类型的编辑器 VM (DataTemplate 分发; 类型 0 时为 null)。</summary>
    [ObservableProperty]
    private object? _editor;

    /// <summary>是否已选中键 (复刻 :disabled="!hotkey")。</summary>
    [ObservableProperty]
    private bool _hasHotkey;

    /// <summary>语言切换刻度: 面板内 ConverterParameter 文案 (watermark/标签) 重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    private List<WindowGroupOption> BuildGroupOptions()
        => _core.Config.Options.WindowGroups.Where(w => w.Id >= 0)
            .Select(w => new WindowGroupOption(w.Id, w.Name)).ToList();

    private List<ActionTypeOption> BuildTypeOptions()
    {
        // 复刻 Action.vue actionTypes (hideInAbbr: 4/5)
        var all = new (int Id, string Label, bool HideInAbbr)[]
        {
            (0, "200", false), (1, "201", false), (2, "202", false), (3, "203", false),
            (4, "204", true), (5, "205", true), (6, "206", false),
            (7, "207", false), (8, "208", false), (9, "209", false),
        };
        return all.Where(t => !_core.IsAbbr || !t.HideInAbbr)
            .Select(t => new ActionTypeOption(t.Id, t.Label)).ToList();
    }

    /// <summary>切换窗口分组: 按新分组重新解析当前键的动作 (复刻 windowGroupID 联动)。</summary>
    partial void OnSelectedGroupChanged(WindowGroupOption? value)
    {
        if (value is null) return;
        _core.SelectedWindowGroupId = value.Id;
    }

    /// <summary>
    /// 切换动作类型 (复刻 onActionTypeChange): 清除除 windowGroupID/actionTypeID 外的全部字段;
    /// 类型 0 置 isEmpty。
    /// </summary>
    partial void OnSelectedTypeChanged(ActionTypeOption? value)
    {
        if (_suppressTypeChange || value is null || CurrentAction is null) return;
        var a = CurrentAction;
        if (a.TypeId == value.Id) return;

        var oldType = a.TypeId;
        var oldValue = a.ValueId;
        var keepGroup = a.WindowGroupId;

        a.Comment = "";
        a.Hotkey = "";
        a.KeysToSend = "";
        a.RemapToKey = "";
        a.ValueId = 0;
        a.WinTitle = "";
        a.Target = "";
        a.Args = "";
        a.WorkingDir = "";
        a.RunAsAdmin = false;
        a.RunInBackground = false;
        a.DetectHiddenWindow = false;
        a.AhkCode = "";
        a.WindowGroupId = keepGroup;
        a.TypeId = value.Id;
        a.IsEmpty = value.Id == 0;

        _core.MaybeRefreshAbbrEnable(oldType, value.Id, oldValue, 0);
        RebuildEditor();
        _core.NotifyDataChanged();
    }

    /// <summary>绑定到解析出的动作 (来自 core 的选中态变化)。</summary>
    public void BindTo(Models.Action? action)
    {
        CurrentAction = action;
        HasHotkey = action is not null;
        _suppressTypeChange = true;
        SelectedType = action is null
            ? TypeOptions[0]
            : TypeOptions.FirstOrDefault(o => o.Id == action.TypeId) ?? TypeOptions[0];
        _suppressTypeChange = false;
        RebuildEditor();
    }

    private void RebuildEditor()
    {
        var a = CurrentAction;
        Editor = a is null ? null : a.TypeId switch
        {
            1 => new ActivateOrRunEditorVm(this, a),
            2 or 3 or 4 or 7 or 9 => new RadioGroupEditorVm(this, a),
            5 => new RemapEditorVm(this, a),
            6 => new SendKeysEditorVm(this, a),
            8 => new AhkCodeEditorVm(this, a),
            _ => null,
        };
    }

    /// <summary>语言切换: 重建下拉文案与当前编辑器 (单选标签等预翻译内容)。</summary>
    public void OnLanguageChanged()
    {
        LanguageTick++;
        var typeId = SelectedType?.Id ?? 0;
        var groupId = SelectedGroup?.Id ?? 0;
        _suppressTypeChange = true;
        TypeOptions = BuildTypeOptions();
        SelectedType = TypeOptions.FirstOrDefault(o => o.Id == typeId) ?? TypeOptions[0];
        GroupOptions = BuildGroupOptions();
        SelectedGroup = GroupOptions.FirstOrDefault(o => o.Id == groupId);
        _suppressTypeChange = false;
        RebuildEditor();
    }
}

// ============================================================================
// 类型 1: 启动程序或激活窗口 (复刻 ActivateOrRun.vue)
// ============================================================================

/// <summary>类型 1 编辑器: winTitle / target(shortcuts 下拉) / args / workingDir / 备注 / 三个开关 + 窗口侦探。</summary>
public sealed partial class ActivateOrRunEditorVm : ObservableObject
{
    private readonly ActionEditorViewModel _editor;
    private readonly Models.Action _a;

    public ActivateOrRunEditorVm(ActionEditorViewModel editor, Models.Action a)
    {
        _editor = editor;
        _a = a;
        _ = LoadShortcutsAsync();
    }

    /// <summary>shortcuts 下拉数据 (GET /shortcuts; 空目录后端返回 null, 容忍为空列表)。</summary>
    public ObservableCollection<string> Shortcuts { get; } = [];

    private async Task LoadShortcutsAsync()
    {
        var api = _editor.Core.Main.Session.Api;
        if (api is null) return;
        var resp = await api.GetShortcutsAsync();
        var paths = resp.Value?.Select(s => s.Path).ToList() ?? [];
        Dispatcher.UIThread.Post(() =>
        {
            Shortcuts.Clear();
            foreach (var p in paths) Shortcuts.Add(p);
        });
    }

    public string WinTitle
    {
        get => _a.WinTitle;
        set
        {
            if (_a.WinTitle == value) return;
            _a.WinTitle = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(WinTitleError));
            RefreshEmpty();
        }
    }

    /// <summary>纯函数校验 (供单测): 空 / 以 ahk_ 或 ahk-expression: 开头 / 含 " ahk_" 组合串 → 放行; 否则裸 xxx.exe 结尾 → 301err。</summary>
    public static string? EvaluateWinTitleError(string? winTitle)
    {
        if (string.IsNullOrEmpty(winTitle)) return null;
        if (winTitle.StartsWith("ahk_") || winTitle.StartsWith("ahk-expression:") || winTitle.Contains(" ahk_")) return null;
        return winTitle.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) ? I18n.T("301err") : null;
    }

    /// <summary>复刻 winTitleRules: 裸写 xxx.exe 永远匹配失败, 提示用 ahk_exe; 组合串 "标题 ahk_exe 名.exe" 放行。</summary>
    public string? WinTitleError => EvaluateWinTitleError(_a.WinTitle);

    public string Target
    {
        get => _a.Target;
        set
        {
            if (_a.Target == value) return;
            _a.Target = value;
            OnPropertyChanged();
            RefreshEmpty();
        }
    }

    public string Args
    {
        get => _a.Args;
        set { if (_a.Args != value) { _a.Args = value; OnPropertyChanged(); } }
    }

    public string WorkingDir
    {
        get => _a.WorkingDir;
        set { if (_a.WorkingDir != value) { _a.WorkingDir = value; OnPropertyChanged(); } }
    }

    public string Comment
    {
        get => _a.Comment;
        set { if (_a.Comment != value) { _a.Comment = value; OnPropertyChanged(); _editor.Core.NotifyDataChanged(); } }
    }

    public bool RunAsAdmin
    {
        get => _a.RunAsAdmin;
        set { if (_a.RunAsAdmin != value) { _a.RunAsAdmin = value; OnPropertyChanged(); } }
    }

    public bool RunInBackground
    {
        get => _a.RunInBackground;
        set { if (_a.RunInBackground != value) { _a.RunInBackground = value; OnPropertyChanged(); } }
    }

    public bool DetectHiddenWindow
    {
        get => _a.DetectHiddenWindow;
        set { if (_a.DetectHiddenWindow != value) { _a.DetectHiddenWindow = value; OnPropertyChanged(); } }
    }

    /// <summary>复刻 watchEffect: isEmpty = !winTitle && !target。</summary>
    private void RefreshEmpty()
    {
        _a.IsEmpty = string.IsNullOrEmpty(_a.WinTitle) && string.IsNullOrEmpty(_a.Target);
        _editor.Core.NotifyDataChanged();
    }

    /// <summary>窗口侦探按钮 (label:309) -> POST /server/command/2。</summary>
    [RelayCommand]
    private Task RunWindowSpyAsync() => _editor.Core.RunWindowSpyAsync();
}

// ============================================================================
// 类型 2/3/4/7/9: 枚举单选 (复刻 RadioGroup.vue + System/Window/Mouse/Text/KeyFlux.vue)
// ============================================================================

/// <summary>单选项 (labelKey 为文案键; 点击时同时把备注写为 label:NNN, 复刻 changeActionComment)。</summary>
public sealed partial class RadioOptionVm : ObservableObject
{
    private readonly RadioGroupEditorVm _owner;
    private bool _suppress;

    public RadioOptionVm(RadioGroupEditorVm owner, int valueId, string labelKey)
    {
        _owner = owner;
        ValueId = valueId;
        LabelKey = labelKey;
    }

    public int ValueId { get; }
    public string LabelKey { get; }
    public string Label => I18n.T(LabelKey);

    [ObservableProperty]
    private bool _isChecked;

    partial void OnIsCheckedChanged(bool value)
    {
        if (value && !_suppress) _owner.SelectRadio(this);
    }

    /// <summary>程序化同步选中态 (不触发回写)。</summary>
    public void SyncChecked(bool value)
    {
        _suppress = true;
        IsChecked = value;
        _suppress = false;
    }
}

/// <summary>
/// 枚举单选编辑器。分组与取值的权威对照: System.vue / Window.vue / Mouse.vue /
/// Text.vue / KeyFlux.vue (含 hideInAbbr 过滤); 布局复刻 RadioGroup.vue 的
/// groups 规则 (普通: [[g1,g2],[g3,g4]]; Text 为 horizontal 单行四列)。
/// </summary>
public sealed partial class RadioGroupEditorVm : ObservableObject
{
    private readonly ActionEditorViewModel _editor;
    private readonly Models.Action _a;

    public RadioGroupEditorVm(ActionEditorViewModel editor, Models.Action a)
    {
        _editor = editor;
        _a = a;
        Rows = BuildRows();
        foreach (var opt in AllOptions) opt.SyncChecked(opt.ValueId == _a.ValueId);
    }

    /// <summary>行 -> 列 -> 选项 (渲染为横向行内的纵向单选列)。</summary>
    public IReadOnlyList<IReadOnlyList<IReadOnlyList<RadioOptionVm>>> Rows { get; }

    private IEnumerable<RadioOptionVm> AllOptions => Rows.SelectMany(r => r).SelectMany(c => c);

    private List<List<List<RadioOptionVm>>> BuildRows()
    {
        var groups = RadioCatalog.GroupsFor(_a.TypeId);
        var isAbbr = _editor.Core.IsAbbr;

        var built = groups
            .Select(g => g.Where(o => !isAbbr || !o.HideInAbbr)
                .Select(o => new RadioOptionVm(this, o.ValueId, o.LabelKey)).ToList())
            .Where(c => c.Count > 0)
            .ToList();

        // 分行: 两两一行 (2026-09-08: 类型 7 原为单行全部 4 组, 总宽约 590 > 面板内容宽
        // 584, 第 4 组被横向裁剪只剩选项圆圈左弧; 改两两一行后任意语言/字体下不再溢出,
        // 高度由面板 Height=430 + 内部 ScrollViewer 兜底)
        var rows = new List<List<List<RadioOptionVm>>>();
        for (var i = 0; i < built.Count; i += 2)
        {
            rows.Add(built.Skip(i).Take(2).ToList());
        }
        return rows;
    }

    /// <summary>选中单选项 (复刻 v-radio 绑定 + @click changeActionComment(item.label))。</summary>
    public void SelectRadio(RadioOptionVm opt)
    {
        var oldValue = _a.ValueId;
        _a.ValueId = opt.ValueId;
        _a.Comment = opt.LabelKey;
        _a.IsEmpty = false;
        foreach (var o in AllOptions) o.SyncChecked(ReferenceEquals(o, opt));
        _editor.Core.MaybeRefreshAbbrEnable(_a.TypeId, _a.TypeId, oldValue, opt.ValueId);
        _editor.Core.NotifyDataChanged();
    }
}

/// <summary>枚举分组静态目录 (逐项复刻 Vue 五个枚举组件的 actionValueID/label/hideInAbbr)。</summary>
public static class RadioCatalog
{
    public sealed record RadioItem(int ValueId, string LabelKey, bool HideInAbbr = false);

    public static IReadOnlyList<RadioItem[]> GroupsFor(int typeId) => typeId switch
    {
        // System.vue
        2 =>
        [
            [I(1, "17"), I(2, "18"), I(3, "19"), I(4, "20"), I(5, "21"), I(9, "2401"), I(6, "22")],
            [I(7, "23"), I(8, "24"), I(10, "2402")],
        ],
        // Window.vue
        3 =>
        [
            [I(1, "1"), I(2, "2"), I(3, "3"), I(4, "4", true), I(5, "5"), I(6, "6"), I(7, "7"), I(15, "8"), I(16, "9")],
            [I(8, "10"), I(9, "11"), I(10, "12"), I(11, "13"), I(12, "14"), I(13, "15"), I(14, "16", true)],
        ],
        // Mouse.vue
        4 =>
        [
            [I(1, "25"), I(2, "26"), I(3, "27"), I(4, "28")],
            [I(5, "29"), I(6, "30"), I(7, "31"), I(8, "32")],
            [I(9, "33"), I(10, "34"), I(11, "35"), I(12, "36")],
            [I(13, "37")],
        ],
        // Text.vue (horizontal)
        7 =>
        [
            [I(1, "38", true), I(2, "39", true), I(3, "40", true), I(4, "41", true),
             I(5, "42", true), I(6, "43", true), I(7, "44", true), I(8, "45", true)],
            [I(9, "46", true), I(10, "47", true), I(11, "48", true), I(12, "49", true),
             I(13, "50", true), I(14, "51", true), I(15, "52", true), I(16, "53", true)],
            [I(17, "54", true), I(33, "55", true), I(18, "56", true), I(19, "57", true),
             I(30, "58", true), I(31, "59", true), I(32, "60", true), I(29, "61")],
            [I(20, "62", true), I(21, "63", true), I(22, "64", true), I(23, "65", true),
             I(24, "66", true), I(25, "67", true), I(26, "68", true), I(27, "69", true), I(28, "70", true)],
        ],
        // KeyFlux.vue
        9 =>
        [
            [I(1, "71", true), I(2, "72"), I(3, "73"), I(4, "74")],
            [I(5, "75", true), I(6, "76", true), I(7, "77"), I(8, "78", true)],
            [I(9, "2408")],
        ],
        _ => [],
    };

    private static RadioItem I(int valueId, string labelKey, bool hideInAbbr = false)
        => new(valueId, labelKey, hideInAbbr);

    // (原 Text.vue horizontal 单行布局已废: 4 组单行宽约 590 > 面板内容宽 584 会裁剪,
    //  2026-09-08 起统一两两一行, 本方法保留供目录语义查询)
    public static bool IsHorizontal(int typeId) => typeId == 7;
}

// ============================================================================
// 类型 5: 重映射按键 (复刻 RemapKey.vue)
// ============================================================================

/// <summary>类型 5 编辑器: remapToKey 可编辑下拉 (singlePress 键不支持重映射, 提示改用输入按键)。</summary>
public sealed partial class RemapEditorVm : ObservableObject
{
    private readonly ActionEditorViewModel _editor;
    private readonly Models.Action _a;

    public RemapEditorVm(ActionEditorViewModel editor, Models.Action a)
    {
        _editor = editor;
        _a = a;
    }

    /// <summary>候选键列表 (逐项复刻 RemapKey.vue items; 也允许自由输入)。</summary>
    public IReadOnlyList<string> Items { get; } =
    [
        "Up", "Down", "Left", "Right", "Home", "End", "Backspace", "Delete",
        "Space", "Tab", "Enter", "Escape", "Insert", "CapsLock", "AppsKey", "PgUp", "PgDn",
        "LWin", "RWin", "LControl", "RControl", "LAlt", "RAlt", "LShift", "RShift", "PrintScreen",
        "Volume_Mute", "Volume_Up", "Volume_Down", "Media_Next", "Media_Prev", "Media_Stop", "Media_Play_Pause",
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    ];

    /// <summary>复刻 :disabled="hotkey == 'singlePress'"。</summary>
    public bool IsSinglePress => _editor.Core.SelectedHotkey == "singlePress";

    public string RemapToKey
    {
        get => _a.RemapToKey;
        set
        {
            if (_a.RemapToKey == value) return;
            _a.RemapToKey = value;
            OnPropertyChanged();
            // 复刻 changeComment: 备注 = 「重映射为 X」(当前语言)
            _a.Comment = string.IsNullOrEmpty(value) ? "" : I18n.T("401") + " " + value;
            // 复刻 watchEffect: isEmpty = !remapToKey
            _a.IsEmpty = string.IsNullOrEmpty(value);
            _editor.Core.NotifyDataChanged();
        }
    }
}

// ============================================================================
// 类型 6: 输入按键或文本 (复刻 SendKey.vue)
// ============================================================================

/// <summary>类型 6 编辑器: keysToSend 多行文本 + 备注。</summary>
public sealed partial class SendKeysEditorVm : ObservableObject
{
    private readonly ActionEditorViewModel _editor;
    private readonly Models.Action _a;

    public SendKeysEditorVm(ActionEditorViewModel editor, Models.Action a)
    {
        _editor = editor;
        _a = a;
    }

    public string KeysToSend
    {
        get => _a.KeysToSend;
        set
        {
            if (_a.KeysToSend == value) return;
            _a.KeysToSend = value;
            OnPropertyChanged();
            // 复刻 watchEffect: isEmpty = !keysToSend
            _a.IsEmpty = string.IsNullOrEmpty(value);
            _editor.Core.NotifyDataChanged();
        }
    }

    public string Comment
    {
        get => _a.Comment;
        set { if (_a.Comment != value) { _a.Comment = value; OnPropertyChanged(); _editor.Core.NotifyDataChanged(); } }
    }
}

// ============================================================================
// 类型 8: 自定义函数 (复刻 BuiltinFunction.vue)
// ============================================================================

/// <summary>类型 8 编辑器: ahkCode 编辑 (示例下拉 + 多行编辑框) + 备注 + Tips。</summary>
public sealed partial class AhkCodeEditorVm : ObservableObject
{
    private readonly ActionEditorViewModel _editor;
    private readonly Models.Action _a;

    public AhkCodeEditorVm(ActionEditorViewModel editor, Models.Action a)
    {
        _editor = editor;
        _a = a;
    }

    /// <summary>示例函数 (复刻 BuiltinFunction.vue items)。</summary>
    public IReadOnlyList<string> Examples { get; } =
    [
        "CenterAndResizeWindow(1600, 1000)",
        "ProcessExistSendKeyOrRun(\"WeChat.exe\", \"^!w\", \"shortcuts\\微信.lnk\")",
    ];

    public string AhkCode
    {
        get => _a.AhkCode;
        set
        {
            if (_a.AhkCode == value) return;
            _a.AhkCode = value;
            OnPropertyChanged();
            // 复刻 watchEffect: isEmpty = !ahkCode
            _a.IsEmpty = string.IsNullOrEmpty(value);
            _editor.Core.NotifyDataChanged();
        }
    }

    public string Comment
    {
        get => _a.Comment;
        set { if (_a.Comment != value) { _a.Comment = value; OnPropertyChanged(); _editor.Core.NotifyDataChanged(); } }
    }
}
