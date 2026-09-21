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
/// 字重下拉条目: 值 = 落 config 的档位标识, 标签 = i18n 键 (语言变化时重算)。
/// 该档位**真实生效** (2026-09-21 起): 生成端据此从源字体同目录挑一个预烘焙变体
/// (<c>tools/font_weight_prebake.py</c> 产出) 再复制成 <c>bin/font/font.ttf</c>;
/// exe 恒定请求 BOLD(700) 且元数据已精确匹配 ⇒ 无合成加粗, 笔画粗细 100% 由落地字形决定。
/// 见 CONTRACTS §3.11.1「字重档位机制」。
/// </summary>
public sealed class FontWeightOption : ObservableObject
{
    private readonly string _labelKey;

    public FontWeightOption(string value, string labelKey)
    {
        Value = value;
        _labelKey = labelKey;
    }

    /// <summary>落配置的值 ("thin"/"light"/"regular"/"semibold"/"bold")。</summary>
    public string Value { get; }

    /// <summary>下拉显示名 (跟随语言刷新)。</summary>
    public string Title => I18n.T(_labelKey);

    /// <summary>语言变化后通知标题重算。</summary>
    public void RefreshLabel() => OnPropertyChanged(nameof(Title));
}

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

    /// <summary>复刻 deleteBtnTip; 可删除时给正向动作提示 (原空串会弹空白 tooltip 框, 用户报)。</summary>
    public string DeleteTip =>
        Model.Enable ? I18n.T("950") : HasSubKeymap ? I18n.T("951") : I18n.T("1121");

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
        LoadCommandFont();
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
    [ObservableProperty] private bool _showAcrylic;

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
            "acrylic" => ShowAcrylic,
            _ => false,
        };
        ShowMouseOption = ShowLanguageOption = ShowKeyboardLayout = false;
        ShowKeymapDelay = ShowSkin = ShowPathVariables = ShowCustomHotkeys = false;
        ShowAcrylic = false;
        if (wasOpen) return;
        switch (which)
        {
            case "mouse": ShowMouseOption = true; break;
            case "language": ShowLanguageOption = true; break;
            case "layout": ShowKeyboardLayout = true; break;
            case "delay": ShowKeymapDelay = true; break;
            // 「命令框皮肤」卡同时承载皮肤 18 键与命令框字体小节 (2026-09-21 合并):
            // 两者生效条件一致 (都需重启命令框进程), 故共用一个分区开关。
            case "skin": ShowSkin = true; break;
            case "pathvars": ShowPathVariables = true; break;
            case "customhotkeys": ShowCustomHotkeys = true; break;
            case "acrylic": ShowAcrylic = true; break;
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

    // ------------------------------------------------------------- 命令框字体

    /// <summary>
    /// 字重下拉候选 (值固定, 标签走 i18n)。
    /// 档位**真实生效**: 生成端按所选档位挑一个预烘焙变体复制成 <c>bin/font/font.ttf</c>。
    /// exe 硬编码请求 <c>DWRITE_FONT_WEIGHT_BOLD(700)</c> 且不可改, 但元数据已与请求精确匹配
    /// (无 BOLDSIM 合成加粗) ⇒ 笔画粗细 100% 由落地的**字形轮廓**决定 ⇒ 换档即真实换粗细。
    /// 出厂默认档 = <see cref="ConfigReadDefaults.DefaultCommandFontWeight"/> (半粗)。
    /// </summary>
    public IReadOnlyList<FontWeightOption> FontWeights { get; } =
    [
        // 顺序即下拉顺序 (由细到粗)。落配置的 Value 必须与 Go 侧 script.FontWeightVariants
        // 的 key 完全一致, 否则生成端认不出档位 -> 静默回落源字体。
        // 🔴 键号必须用**真实空洞**。历史上这里误用过 "741"(实为「命令框皮肤」卡片标题)
        // 和 "2517"/"2518"(实为 SelectedActionPage 的留桩提示条文案与「创建专属行为」按钮,
        // 且因 JSON 后定义覆盖前定义, 直接把那两处文案改成了「极细」/「细」)。
        // thin/light 现用空键 2526/2549 —— 查证方法见 I18nResourceTests 注释。
        new FontWeightOption("thin", "2526"),
        new FontWeightOption("light", "2549"),
        new FontWeightOption("regular", "2509"),
        new FontWeightOption("semibold", "2511"),
        // 🔴 key 必须用 2516「粗体 / Bold」。历史上这里误用过 "741" —— 那个键的真实
        // 内容是「命令框皮肤 / Command Window」(卡片标题), 于是下拉显示成了
        // 隔壁卡片的标题 (用户报障截图)。741 属别的文案域, 不可复用。
        new FontWeightOption("bold", "2516"),
    ];

    /// <summary>
    /// 用户选定的字体文件绝对路径 (空 = 未自定义, 沿用现有 <c>bin/font/font.ttf</c>)。
    /// 写入 config 后由**生成端**复制到 <c>bin/font/font.ttf</c>; 命令框进程只在启动时
    /// 读一次该文件 ⇒ 需重启命令框才生效 (契约 §3.11.1 硬约束 7)。
    /// </summary>
    [ObservableProperty] private string _commandFontPath = "";

    /// <summary>显示用: 未选择时给占位文案 (i18n 2506), 否则显示完整路径。</summary>
    public string CommandFontDisplay =>
        string.IsNullOrWhiteSpace(CommandFontPath) ? I18n.T("2506") : CommandFontPath;

    /// <summary>选定字体的即时校验提示 (空 = 无提示)。</summary>
    [ObservableProperty] private string _commandFontNotice = "";

    /// <summary>提示是否为**问题态** (决定 UI 用警示色还是中性色)。</summary>
    [ObservableProperty] private bool _commandFontNoticeIsError;

    /// <summary>是否有提示可显示 (供 AXAML 控制提示条可见性)。</summary>
    public bool HasCommandFontNotice => !string.IsNullOrEmpty(CommandFontNotice);

    /// <summary>当前选中的字重条目 (对象形式供 ComboBox 双向绑定)。</summary>
    public FontWeightOption? SelectedFontWeight
    {
        get => FontWeights.FirstOrDefault(w => w.Value == Options.CommandFont?.Weight)
               ?? FontWeights[0];
        set
        {
            if (value is null) return;
            var opt = CurrentCommandFont();
            if (opt.Weight == value.Value) return;
            opt.Weight = value.Value;
            OnPropertyChanged();
        }
    }

    /// <summary>从配置载入字体段 (构造期调用一次), 非法字重值回退默认档位。</summary>
    private void LoadCommandFont()
    {
        var f = Options.CommandFont;
        CommandFontPath = f?.SourcePath ?? "";
        if (f is not null)
        {
            // 就地规范化: 配置被手改坏 / 旧版本写入未知档位时不留脏值 (与读取默认值同口径)
            f.Weight = ConfigReadDefaults.NormalizeFontWeight(f.Weight);
        }
        // 🔴 启动时**不**复检已存路径。理由: 配置里的路径很可能指向一个已经不存在或
        // 超限的文件 (正是用户上次踩的坑), 生成端会静默回落、命令框显示的是既有字体。
        // 若在载入时就弹错误提示, 用户每次打开设置都会看到一条无法消除的告警 —— 噪声。
        // 改为**只在用户主动选择时**给出结论 (见 OnCommandFontPathChanged), 那才是
        // 需要解释"为什么没变化"的时刻。
        CommandFontNotice = "";
        CommandFontNoticeIsError = false;
    }

    /// <summary>把 UI 上的字体段写回配置 (缺段时按需新建)。</summary>
    private CommandFontOption CurrentCommandFont()
    {
        var f = Options.CommandFont;
        if (f is null)
        {
            f = new CommandFontOption();
            Options.CommandFont = f;
        }
        f.SourcePath = CommandFontPath;
        f.Weight = ConfigReadDefaults.NormalizeFontWeight(f.Weight);
        return f;
    }

    /// <summary>路径变更后同步配置并刷新显示文本与校验提示。</summary>
    partial void OnCommandFontPathChanged(string value)
    {
        CurrentCommandFont().SourcePath = value;
        OnPropertyChanged(nameof(CommandFontDisplay));
        RefreshCommandFontNotice();
    }

    /// <summary>重新评估当前所选字体, 更新提示条。空路径 = 未自定义, 清除提示。</summary>
    private void RefreshCommandFontNotice()
    {
        if (string.IsNullOrWhiteSpace(CommandFontPath))
        {
            // 未自定义 -> 清除提示 (既不报错也不提示"已选")
            CommandFontNotice = "";
            CommandFontNoticeIsError = false;
            return;
        }

        var r = CommandFontValidator.Validate(CommandFontPath);
        CommandFontNotice = r.Message;
        // 只有真正不可用 (超限/格式不支持/读不了) 才算错误态; "集合只用首 face" 是提醒,
        // 用中性色 —— 否则用户会以为自己选错了。
        CommandFontNoticeIsError = !r.Usable;
    }

    /// <summary>提示文本变化后同步可见性绑定。</summary>
    partial void OnCommandFontNoticeChanged(string value)
    {
        OnPropertyChanged(nameof(HasCommandFontNotice));
    }

    /// <summary>
    /// 「恢复默认」: 清空自定义路径 (沿用现有 <c>bin/font/font.ttf</c>) 并把字重归位默认档
    /// (<see cref="ConfigReadDefaults.DefaultCommandFontWeight"/> = 半粗)。
    /// 注意"沿用现有字体"是指**不替换文件** —— 生成端因 sourcePath 为空而不动部署树的
    /// <c>font.ttf</c>, 但字重仍会按默认档去源字体同目录找变体, 找不到就保持原样。
    /// </summary>
    [RelayCommand]
    private void ResetCommandFont()
    {
        CommandFontPath = "";   // 触发 OnCommandFontPathChanged -> 清空提示
        var opt = CurrentCommandFont();
        opt.SourcePath = "";
        opt.Weight = ConfigReadDefaults.DefaultCommandFontWeight;
        CommandFontNotice = "";
        CommandFontNoticeIsError = false;
        OnPropertyChanged(nameof(SelectedFontWeight));
    }

    /// <summary>
    /// 弹窗回调: 由视图层的系统文件选择器在用户选定字体文件后调用 (文件选择属视图职责,
    /// 需 StorageProvider; 逻辑仍在 VM)。取消选择时传 null, 保持原值不变。
    /// </summary>
    public void SetCommandFontPath(string? path)
    {
        if (path is null) return;
        CommandFontPath = path;
    }

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
    /// <remarks>
    /// **无实际变化时不得重建 (2026-09-15 修用户报「切换输入框整窗闪白」)**:
    /// 从一个输入框点到另一个输入框时, 前一个框失焦即走本方法, 而此时**什么都没变**。
    /// 原实现无条件 `RefreshKeymapSection()` —— 它会 `KeymapRows.Clear()` + 逐行重建 +
    /// `OnNavInvalidated()` (⇒ 导航重建), 每次切焦点都把整张表拆了重搭, 视觉上就是闪一下。
    /// 故: ① 有变化才继续; ② 即使需要重建, 导航侧也已由 MainViewModel.OnCurrentNavItemChanged
    /// 的 null 防护兜住内容区不被清空。
    /// </remarks>
    public void CommitKeymapEdit(KeymapRowViewModel row)
    {
        var km = row.Model;
        var f = Config.Keymaps.FirstOrDefault(k => k.Hotkey == km.Hotkey && k.ParentId == km.ParentId) ?? km;
        if (f.Id != km.Id && !string.IsNullOrEmpty(km.Hotkey))
        {
            // 触发键重复 ⇒ 本行被删除: 行集合真的少了一项, 必须重建
            RemoveKeymapById(km.Id);
            RefreshKeymapSection();
            return;
        }

        // 仅切换焦点 (未改动任何内容) ⇒ 直接返回, 不触碰 KeymapRows / 导航
        var normalized = ConfigActions.NormalizeKeyName(km.Hotkey);
        if (normalized == km.Hotkey) return;

        // 触发键文本被规范化 (bs -> Backspace 等): 需要重建以刷新显示与导航徽标
        km.Hotkey = normalized;
        RefreshKeymapSection();
    }

    // ------------------------------------------------------------- 语言刷新

    /// <summary>全局语言变化时由 MainViewModel 调用。</summary>
    public void OnLanguageChanged()
    {
        LanguageTick++;
        CustomHotkeys?.OnLanguageChanged();
        foreach (var f in SkinFields) f.RefreshLabel();
        foreach (var w in FontWeights) w.RefreshLabel();
        foreach (var r in KeymapRows) r.RefreshComputed();
        OnPropertyChanged(nameof(SelectedLanguage));
        OnPropertyChanged(nameof(SelectedFontWeight));
        OnPropertyChanged(nameof(CommandFontDisplay));
    }
}
