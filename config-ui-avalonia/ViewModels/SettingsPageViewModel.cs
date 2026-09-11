using KeyFlux.Settings.Theming;
using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>语言下拉条目 (复刻 language-map.ts 的 languageList)。</summary>
public sealed record LanguageOption(string Title, string Value);

/// <summary>
/// 命令框皮肤字段条目 (数据驱动渲染 18 个字段; 标签预翻译, 语言变化时刷新)。
/// </summary>
public sealed class SkinFieldViewModel : ObservableObject
{
    private readonly string _labelKey;
    private readonly Func<string> _get;
    private readonly Action<string> _set;

    public SkinFieldViewModel(string labelKey, Func<string> get, Action<string> set)
    {
        _labelKey = labelKey;
        _get = get;
        _set = set;
    }

    public string Label => I18n.T(_labelKey);

    public string Value
    {
        get => _get();
        set
        {
            _set(value);
            OnPropertyChanged(nameof(Value));
        }
    }

    public void RefreshLabel() => OnPropertyChanged(nameof(Label));
}

/// <summary>
/// Settings 页左列「快捷键方案」表行: 包装 Keymap 模型, 提供上层下拉、
/// 开关级联、删除约束等计算属性 (复刻 Settings.vue 表格逻辑)。
/// </summary>
public sealed partial class KeymapRowViewModel : ObservableObject
{
    private readonly SettingsPageViewModel _owner;

    public KeymapRowViewModel(SettingsPageViewModel owner, Keymap model)
    {
        _owner = owner;
        Model = model;
    }

    public Keymap Model { get; }

    public string Name
    {
        get => Model.Name;
        set => SetProperty(Model.Name, value, Model, (m, v) => m.Name = v);
    }

    public string Hotkey
    {
        get => Model.Hotkey;
        set => SetProperty(Model.Hotkey, value, Model, (m, v) => m.Hotkey = v);
    }

    /// <summary>开关: 写入时走级联逻辑 (父键联动开启 / 子键联动关闭 / 缩写状态重算)。</summary>
    public bool Enable
    {
        get => Model.Enable;
        set
        {
            if (Model.Enable != value) _owner.ToggleKeymapEnable(this);
        }
    }

    /// <summary>上层选择: 对象形式供 ComboBox 使用, 映射回 Model.ParentId。</summary>
    public Keymap? ParentSelection
    {
        get => ParentOptions.FirstOrDefault(o => o.Id == Model.ParentId);
        set
        {
            if (value is null || value.Id == Model.ParentId || HasSubKeymap) return;
            Model.ParentId = value.Id;
            _owner.RefreshKeymapSection();
        }
    }

    /// <summary>候选上层列表 (哨兵 "-" + 其余自定义父键, 排除自身)。</summary>
    public ObservableCollection<Keymap> ParentOptions { get; } = [];

    /// <summary>是否被其它 keymap 作为上层引用。</summary>
    public bool HasSubKeymap =>
        _owner.Config.Keymaps.Any(k => k.Id > 4 && k.ParentId == Model.Id);

    /// <summary>复刻 disabledKeymapOption: 启用中或被依赖时禁止删除。</summary>
    public bool CanDelete => !Model.Enable && !HasSubKeymap;

    /// <summary>复刻 deleteBtnTip。</summary>
    public string DeleteTip =>
        Model.Enable ? I18n.T("950") : HasSubKeymap ? I18n.T("951") : "";

    /// <summary>模型侧变更后刷新行上的计算属性。</summary>
    public void RefreshComputed()
    {
        OnPropertyChanged(nameof(Enable));
        OnPropertyChanged(nameof(ParentSelection));
        OnPropertyChanged(nameof(HasSubKeymap));
        OnPropertyChanged(nameof(CanDelete));
        OnPropertyChanged(nameof(DeleteTip));
    }
}

/// <summary>
/// Settings 选项页 (逐项复刻 Settings.vue):
/// 左列 = 快捷键方案表 (名称/触发键/上层/开关/删除 + 新增);
/// 右列 = 其他设置 (开机自启、隐藏矩阵、语言、自定义热键、鼠标/滚轮参数、键盘布局、
/// 命令框皮肤、触发延时、路径变量), 分区互斥展开 (复刻 resetOtherToFalse)。
/// 自定义热键分区 (2026-09-08 自独立页迁入): 复用 CustomHotkeyPageViewModel,
/// 编辑动作经 ActionEditorWindow 弹窗 (原页右侧内嵌面板)。
/// </summary>
public sealed partial class SettingsPageViewModel : ObservableObject
{
    private readonly MainViewModel _main;
    private static readonly Keymap ParentSentinel = new() { Id = 0, Name = "-", Enable = true };

    public SettingsPageViewModel(MainViewModel main)
    {
        _main = main;
        Config = main.Config ?? throw new InvalidOperationException("Config 未加载");
        var customKeymap = Config.Keymaps.FirstOrDefault(k => k.Id == 1);
        if (customKeymap is not null) CustomHotkeys = new CustomHotkeyPageViewModel(main, customKeymap);
        BuildSkinFields();
        LoadAcrylic();
        foreach (var pv in Options.PathVariables) PathVariables.Add(pv);
        RefreshKeymapSection();
    }

    public Config Config { get; }
    public Options Options => Config.Options;
    public CommandInputSkin Skin => Options.CommandInputSkin;
    public Mouse MouseOpts => Options.Mouse;
    public Scroll ScrollOpts => Options.Scroll;
    private ISettingsApi? Api => _main.Session.Api;

    /// <summary>
    /// 自定义热键分区 (keymap/1, 原 Custom Hotkeys 页迁入; keymap 缺失时为 null 整卡隐藏)。
    /// </summary>
    public CustomHotkeyPageViewModel? CustomHotkeys { get; }

    /// <summary>主 VM (视图打开窗口组对话框等场景使用)。</summary>
    public MainViewModel Main => _main;

    /// <summary>语言切换递增, 驱动 ConverterParameter 式文案绑定重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    // ------------------------------------------------------------- 分区显隐
    // 复刻 Vue: resetOtherToFalse —— 同一时刻只展开一个分区 (默认展示触发延时)

    [ObservableProperty] private bool _showMouseOption;
    [ObservableProperty] private bool _showLanguageOption;
    [ObservableProperty] private bool _showKeyboardLayout;
    [ObservableProperty] private bool _showKeymapDelay = true;
    [ObservableProperty] private bool _showSkin;
    [ObservableProperty] private bool _showPathVariables;
    [ObservableProperty] private bool _showCustomHotkeys;

    [RelayCommand]
    private void ToggleSection(string? which)
    {
        var wasOpen = which switch
        {
            "mouse" => ShowMouseOption,
            "language" => ShowLanguageOption,
            "layout" => ShowKeyboardLayout,
            "delay" => ShowKeymapDelay,
            "skin" => ShowSkin,
            "pathvars" => ShowPathVariables,
            "customhotkeys" => ShowCustomHotkeys,
            _ => false,
        };
        ShowMouseOption = ShowLanguageOption = ShowKeyboardLayout = false;
        ShowKeymapDelay = ShowSkin = ShowPathVariables = ShowCustomHotkeys = false;
        if (wasOpen) return;
        switch (which)
        {
            case "mouse": ShowMouseOption = true; break;
            case "language": ShowLanguageOption = true; break;
            case "layout": ShowKeyboardLayout = true; break;
            case "delay": ShowKeymapDelay = true; break;
            case "skin": ShowSkin = true; break;
            case "pathvars": ShowPathVariables = true; break;
            case "customhotkeys": ShowCustomHotkeys = true; break;
        }
    }

    // ------------------------------------------------------------- 开机自启

    /// <summary>开机自启开关: 读 config.options.startup, 切换时调 POST /server/command/3|4。</summary>
    public bool Startup
    {
        get => Options.Startup;
        set
        {
            if (Options.Startup == value) return;
            Options.Startup = value;
            OnPropertyChanged();
            _ = SendStartupCommandAsync(value);
        }
    }

    private async Task SendStartupCommandAsync(bool enable)
    {
        if (Api is null) return;
        var resp = await Api.SendServerCommandAsync(enable ? 3 : 4);
        if (!resp.Success)
        {
            _main.ShowMessage(I18n.T("506"), resp.ErrorMessage ?? $"command {(enable ? 3 : 4)} failed");
        }
    }

    // ------------------------------------------------------------- 语言选择

    public IReadOnlyList<LanguageOption> Languages { get; } =
    [
        new LanguageOption("简体中文", "zh"),
        new LanguageOption("English", "en"),
    ];

    public LanguageOption? SelectedLanguage
    {
        get => Languages.FirstOrDefault(l => l.Value == Options.Language) ?? Languages[0];
        set
        {
            if (value is null || Options.Language == value.Value) return;
            Options.Language = value.Value;
            I18n.Language = value.Value; // 触发全局文案刷新
            OnPropertyChanged();
        }
    }

    // ------------------------------------------------------------- 键盘布局

    [RelayCommand]
    private void ResetKeyboardLayout(string? kind)
    {
        Options.KeyboardLayout = kind switch
        {
            "0" => ConfigReadDefaults.DefaultKeyboardLayout,
            "74" => ConfigReadDefaults.KeyboardLayout74,
            "104" => ConfigReadDefaults.KeyboardLayout104,
            "1" => Options.KeyboardLayout + "\n" + ConfigReadDefaults.MouseButtons,
            _ => Options.KeyboardLayout,
        };
        OnPropertyChanged(nameof(Options));
    }

    // ------------------------------------------------------------- 窗口亚克力(毛玻璃)

    /// <summary>是否启用窗口毛玻璃。关闭时底色为实心 Parchment。</summary>
    [ObservableProperty] private bool _acrylicEnabled;

    /// <summary>透明度 0..100。0=完全不透明, 100=尽量透明 (内部仍夹最小不透明度)。</summary>
    [ObservableProperty] private int _acrylicTransparency;

    /// <summary>从配置载入亚克力设置 (构造期调用一次)。</summary>
    private void LoadAcrylic()
    {
        var a = Options.Acrylic;
        AcrylicEnabled = a?.Enabled ?? true;
        AcrylicTransparency = a is null ? 30 : Math.Clamp(a.Transparency, 0, 100);
        WindowSurface.Apply(CurrentAcrylic());
    }

    /// <summary>把 UI 上的两个值写回配置段, 并立即应用到底色 (配置段缺失时按需新建)。</summary>
    private AcrylicOption? CurrentAcrylic()
    {
        var a = Options.Acrylic;
        if (a is null)
        {
            a = new AcrylicOption();
            Options.Acrylic = a;
        }
        a.Enabled = AcrylicEnabled;
        a.Transparency = Math.Clamp(AcrylicTransparency, 0, 100);
        return a;
    }

    private void ApplyAcrylicChange() => WindowSurface.Apply(CurrentAcrylic());

    partial void OnAcrylicEnabledChanged(bool value) => ApplyAcrylicChange();
    partial void OnAcrylicTransparencyChanged(int value) => ApplyAcrylicChange();

    // ------------------------------------------------------------- 命令框皮肤

    public ObservableCollection<SkinFieldViewModel> SkinFields { get; } = [];

    private void BuildSkinFields()
    {
        SkinFields.Clear();
        void Add(string labelKey, Func<string> get, Action<string> set)
            => SkinFields.Add(new SkinFieldViewModel(labelKey, get, set));

        // 行 1
        Add("743", () => Skin.WindowWidth, v => Skin.WindowWidth = v);
        Add("744", () => Skin.WindowYPos, v => Skin.WindowYPos = v);
        Add("745", () => Skin.BorderRadius, v => Skin.BorderRadius = v);
        Add("746", () => Skin.HideAnimationDuration, v => Skin.HideAnimationDuration = v);
        // 行 2
        Add("747", () => Skin.BackgroundColor, v => Skin.BackgroundColor = v);
        Add("748", () => Skin.BackgroundOpacity, v => Skin.BackgroundOpacity = v);
        Add("749", () => Skin.GridlineColor, v => Skin.GridlineColor = v);
        Add("748", () => Skin.GridlineOpacity, v => Skin.GridlineOpacity = v);
        // 行 3
        Add("750", () => Skin.BorderWidth, v => Skin.BorderWidth = v);
        Add("751", () => Skin.BorderColor, v => Skin.BorderColor = v);
        Add("748", () => Skin.BorderOpacity, v => Skin.BorderOpacity = v);
        // 行 4
        Add("752", () => Skin.KeyColor, v => Skin.KeyColor = v);
        Add("748", () => Skin.KeyOpacity, v => Skin.KeyOpacity = v);
        Add("753", () => Skin.CornerColor, v => Skin.CornerColor = v);
        Add("748", () => Skin.CornerOpacity, v => Skin.CornerOpacity = v);
        // 行 5
        Add("754", () => Skin.WindowShadowSize, v => Skin.WindowShadowSize = v);
        Add("755", () => Skin.WindowShadowColor, v => Skin.WindowShadowColor = v);
        Add("748", () => Skin.WindowShadowOpacity, v => Skin.WindowShadowOpacity = v);
    }

    // ------------------------------------------------------------- 触发延时

    /// <summary>自定义 keymap 列表 (触发延时分区的逐 keymap 输入)。</summary>
    public ObservableCollection<Keymap> CustomKeymaps { get; } = [];

    // ------------------------------------------------------------- 路径变量

    /// <summary>与 Options.PathVariables 共享实例的双向同步集合。</summary>
    public ObservableCollection<PathVariable> PathVariables { get; } = [];

    [RelayCommand]
    private void AddPathVariable()
    {
        var pv = new PathVariable();
        PathVariables.Add(pv);
        Options.PathVariables.Add(pv);
    }

    [RelayCommand]
    private void RemovePathVariable(PathVariable? pv)
    {
        if (pv is null) return;
        PathVariables.Remove(pv);
        Options.PathVariables.Remove(pv);
    }

    // ------------------------------------------------------------- keymap 表

    public ObservableCollection<KeymapRowViewModel> KeymapRows { get; } = [];

    /// <summary>重建表行/候选上层/自定义列表, 并通知主窗口刷新导航。</summary>
    public void RefreshKeymapSection()
    {
        var customs = Config.Keymaps.Where(k => k.Id > 4).ToList();

        // 自定义父键列表 (parentID==0) + 哨兵 "-"
        var parentBase = new List<Keymap> { ParentSentinel };
        parentBase.AddRange(customs.Where(k => k.ParentId == 0));

        KeymapRows.Clear();
        foreach (var km in customs)
        {
            var row = new KeymapRowViewModel(this, km);
            foreach (var p in parentBase.Where(p => p.Id != km.Id))
            {
                row.ParentOptions.Add(p);
            }
            KeymapRows.Add(row);
        }

        CustomKeymaps.Clear();
        foreach (var km in customs) CustomKeymaps.Add(km);

        _main.OnNavInvalidated();
    }

    /// <summary>复刻 toggleKeymapEnable: 父键联动开启、子键联动关闭、缩写状态重算。</summary>
    public void ToggleKeymapEnable(KeymapRowViewModel row)
    {
        var km = row.Model;
        if (!km.Enable && km.ParentId != 0)
        {
            var parent = Config.Keymaps.FirstOrDefault(k => k.Id == km.ParentId && k.Id > 4);
            if (parent is not null) parent.Enable = true;
        }
        if (km.Enable)
        {
            foreach (var son in Config.Keymaps.Where(k => k.Id > 4 && k.ParentId == km.Id))
            {
                son.Enable = false;
            }
        }
        km.Enable = !km.Enable;
        ConfigActions.ChangeAbbrEnable(Config);
        RefreshKeymapSection();
    }

    [RelayCommand]
    private void AddKeymap()
    {
        var customs = Config.Keymaps.Where(k => k.Id > 4).ToList();
        var newId = customs.Count == 0 ? 5 : customs[^1].Id + 1;
        var km = new Keymap
        {
            Id = newId,
            Name = "",
            Enable = false,
            Hotkey = "",
            ParentId = 0,
            Delay = 0,
            IsNew = true,
        };
        // 插入位置 = 自定义 keymap 之后、内置 (1,2,3,4) 之前 (复刻 splice(customKeymaps.length, 0, ...))
        Config.Keymaps.Insert(customs.Count, km);
        RefreshKeymapSection();
    }

    [RelayCommand]
    private void RemoveKeymapRow(KeymapRowViewModel? row)
    {
        if (row is null || !row.CanDelete) return;
        RemoveKeymapById(row.Model.Id);
        RefreshKeymapSection();
    }

    private void RemoveKeymapById(int id)
    {
        // 复刻 removeKeymap: findLastIndex(id) 后 splice
        for (var i = Config.Keymaps.Count - 1; i >= 0; i--)
        {
            if (Config.Keymaps[i].Id == id)
            {
                Config.Keymaps.RemoveAt(i);
                return;
            }
        }
    }

    /// <summary>
    /// 复刻 checkKeymapData (名称/触发键失焦时):
    /// 触发键与同上层键重复时删除当前行; 规范化非标准键名 (bs->Backspace 等)。
    /// </summary>
    public void CommitKeymapEdit(KeymapRowViewModel row)
    {
        var km = row.Model;
        var f = Config.Keymaps.FirstOrDefault(k => k.Hotkey == km.Hotkey && k.ParentId == km.ParentId) ?? km;
        if (f.Id != km.Id && !string.IsNullOrEmpty(km.Hotkey))
        {
            RemoveKeymapById(km.Id);
        }
        km.Hotkey = ConfigActions.NormalizeKeyName(km.Hotkey);
        RefreshKeymapSection();
    }

    // ------------------------------------------------------------- 语言刷新

    /// <summary>全局语言变化时由 MainViewModel 调用。</summary>
    public void OnLanguageChanged()
    {
        LanguageTick++;
        CustomHotkeys?.OnLanguageChanged();
        foreach (var f in SkinFields) f.RefreshLabel();
        foreach (var r in KeymapRows) r.RefreshComputed();
        OnPropertyChanged(nameof(SelectedLanguage));
    }
}
