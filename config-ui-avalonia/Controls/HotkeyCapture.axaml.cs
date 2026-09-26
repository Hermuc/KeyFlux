using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Controls;

/// <summary>
/// 热键捕获控件 (复刻 components/action/HotkeyCapture.vue):
///   - <see cref="Hotkey"/> 双向绑定 AHK 格式热键 (^+q / &lt;^q);
///   - <see cref="UsedHotkeys"/> 为已占用热键集合 (归一化后), 冲突时展示保守报错;
///   - 点击进入捕获态, 左右修饰键 (Key.LeftCtrl/RightCtrl 等物理键) 暂存等待主键,
///     组合生成后提交; Esc 单独按下取消; 焦点丢失取消 (复刻 focusout);
///   - 纯逻辑在 <see cref="HotkeyLogic"/> / <see cref="HotkeyCaptureCore"/> (可单测)。
/// </summary>
public partial class HotkeyCapture : UserControl
{
    /// <summary>AHK 格式热键 (如 ^+q / &lt;^q), 双向绑定。</summary>
    public static readonly StyledProperty<string> HotkeyProperty =
        AvaloniaProperty.Register<HotkeyCapture, string>(nameof(Hotkey), "", defaultBindingMode: Avalonia.Data.BindingMode.TwoWay);

    /// <summary>已占用的热键集合 (归一化后, 复刻 usedHotkeys prop)。</summary>
    public static readonly StyledProperty<HashSet<string>?> UsedHotkeysProperty =
        AvaloniaProperty.Register<HotkeyCapture, HashSet<string>?>(nameof(UsedHotkeys));

    // ---- 呈现属性 (DirectProperty: 标准 INPC 通知链路, 支撑编译绑定) ----
    public static readonly DirectProperty<HotkeyCapture, string> DisplayTextProperty =
        AvaloniaProperty.RegisterDirect<HotkeyCapture, string>(nameof(DisplayText), o => o.DisplayText);
    public static readonly DirectProperty<HotkeyCapture, string> IconTextProperty =
        AvaloniaProperty.RegisterDirect<HotkeyCapture, string>(nameof(IconText), o => o.IconText);
    public static readonly DirectProperty<HotkeyCapture, IBrush> TextBrushProperty =
        AvaloniaProperty.RegisterDirect<HotkeyCapture, IBrush>(nameof(TextBrush), o => o.TextBrush);
    public static readonly DirectProperty<HotkeyCapture, bool> HasConflictProperty =
        AvaloniaProperty.RegisterDirect<HotkeyCapture, bool>(nameof(HasConflict), o => o.HasConflict);
    public static readonly DirectProperty<HotkeyCapture, bool> ShowClearButtonProperty =
        AvaloniaProperty.RegisterDirect<HotkeyCapture, bool>(nameof(ShowClearButton), o => o.ShowClearButton);
    /// <summary>语言切换递增, 供控件内 ConverterParameter 文案重算。</summary>
    public static readonly DirectProperty<HotkeyCapture, int> LanguageTickProperty =
        AvaloniaProperty.RegisterDirect<HotkeyCapture, int>(nameof(LanguageTick), o => o.LanguageTick);

    private readonly HotkeyCaptureCore _core = new();

    public HotkeyCapture()
    {
        InitializeComponent();
        DataContext = this; // 控件自包含: 绑定源为控件自身属性

        _core.StateChanged += RefreshPresentation;
        _core.HotkeyCommitted += ahk => SetCurrentValue(HotkeyProperty, ahk);

        HotkeyProperty.Changed.AddClassHandler<HotkeyCapture>((c, _) => c.RefreshPresentation());
        UsedHotkeysProperty.Changed.AddClassHandler<HotkeyCapture>((c, _) => c.RefreshPresentation());

        I18n.Changed += OnLanguageChanged;
        DetachedFromVisualTree += (_, _) => I18n.Changed -= OnLanguageChanged;
        // 初值兜底: 宿主绑定可能在构造期(此 handler 注册前)已写入 Hotkey —— 那次变更
        // 不会触发类处理器, 首帧 DisplayText 停留在空串 (bug2: 迁移来的 >^p 显示为空)。
        AttachedToVisualTree += (_, _) => RefreshPresentation();
    }

    private void OnLanguageChanged()
    {
        // I18n.Changed 是全局静态事件, 触发线程不受本控件控制。
        // 当前生产路径 (MainWindow.Opened -> InitializeAsync, 且无 ConfigureAwait(false))
        // 的 await 续体会回到 UI 线程, 故生产侧暂不可达; 但【测试并行串扰】会让 [Fact] 用例
        // 在线程池线程改 I18n.Language, 从而命中本控件的订阅。本回调会 SetAndRaise 多个
        // DirectProperty 并经 RefreshPresentation 触碰 BoxBorder, 全部要求 UI 线程,
        // 否则抛 "Call from invalid thread"。
        // 故此处为【防御性加固 (同时消除测试并行串扰)】: 非 UI 线程一律把整个回调封送到
        // UI 线程 (与 WindowPickButton 同形态同处理), UI 线程路径内联、行为不变。
        if (!Dispatcher.UIThread.CheckAccess())
        {
            Dispatcher.UIThread.Post(OnLanguageChanged);
            return;
        }
        SetAndRaise(LanguageTickProperty, ref _languageTick, _languageTick + 1);
        RefreshPresentation();
    }

    public string Hotkey
    {
        get => GetValue(HotkeyProperty);
        set => SetValue(HotkeyProperty, value);
    }

    public HashSet<string>? UsedHotkeys
    {
        get => GetValue(UsedHotkeysProperty);
        set => SetValue(UsedHotkeysProperty, value);
    }

    // ------------------------------------------------------------- 呈现属性

    private string _displayText = "";
    private string _iconText = "⌨";
    private IBrush _textBrush = Brushes.Black;
    private bool _hasConflict;
    private bool _showClearButton;
    private int _languageTick;

    public string DisplayText { get => _displayText; private set => SetAndRaise(DisplayTextProperty, ref _displayText, value); }
    public string IconText { get => _iconText; private set => SetAndRaise(IconTextProperty, ref _iconText, value); }
    public IBrush TextBrush { get => _textBrush; private set => SetAndRaise(TextBrushProperty, ref _textBrush, value); }
    public bool HasConflict { get => _hasConflict; private set => SetAndRaise(HasConflictProperty, ref _hasConflict, value); }
    public bool ShowClearButton { get => _showClearButton; private set => SetAndRaise(ShowClearButtonProperty, ref _showClearButton, value); }
    public int LanguageTick => _languageTick;

    // ------------------------------------------------------------- 交互

    /// <summary>点击进入捕获态 (复刻 @click="startCapture"); 清除按钮自身区域除外。</summary>
    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        base.OnPointerPressed(e);
        if (e.Source is Visual source && source.GetSelfAndVisualAncestors().Contains(ClearButton))
        {
            return; // 交给清除按钮处理
        }
        Focus();
        _core.StartCapture();
        e.Handled = true;
    }

    /// <summary>捕获态按键 (复刻 onKeydown): 全部吞掉, 防止窗口级快捷键 (如 Ctrl+S) 误触发。</summary>
    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (!_core.Capturing)
        {
            base.OnKeyDown(e);
            return;
        }
        e.Handled = true;
        _core.HandleKeyDown(e.Key, e.KeyModifiers != KeyModifiers.None);
    }

    /// <summary>捕获态吞掉 KeyUp (暂存只在 Enter/Esc 变化, 松开不改变状态)。</summary>
    protected override void OnKeyUp(KeyEventArgs e)
    {
        if (_core.Capturing)
        {
            e.Handled = true;
            return;
        }
        base.OnKeyUp(e);
    }

    /// <summary>焦点离开取消捕获 (复刻 focusout)。</summary>
    protected override void OnLostFocus(RoutedEventArgs e)
    {
        _core.Cancel();
        base.OnLostFocus(e);
    }

    private void OnClearClick(object? sender, RoutedEventArgs e)
    {
        SetCurrentValue(HotkeyProperty, ""); // 复刻 ✕ 按钮: emit('update:modelValue', '')
        Focus();
    }

    // ------------------------------------------------------------- 呈现刷新

    private void RefreshPresentation()
    {
        var capturing = _core.Capturing;
        string text;
        IBrush brush;
        if (capturing)
        {
            text = _core.DisplayText(Hotkey, I18n.T("1026"));
            brush = new SolidColorBrush(Color.Parse(ClaudePalette.StoneGray));
        }
        else if (string.IsNullOrEmpty(Hotkey))
        {
            text = I18n.T("1027");
            brush = new SolidColorBrush(Color.Parse(ClaudePalette.StoneGray));
        }
        else
        {
            text = HotkeyLogic.AhkToDisplay(Hotkey);
            brush = new SolidColorBrush(Color.Parse(ClaudePalette.NearBlack));
        }
        DisplayText = text;
        TextBrush = brush;
        IconText = "⌨"; // 保留图标位, 捕获态以边框色区分
        ShowClearButton = !capturing && !string.IsNullOrEmpty(Hotkey);

        // 冲突检测 (复刻 conflict 计算): 归一化后与已占用集合比较, 保守策略
        HasConflict = !string.IsNullOrEmpty(Hotkey)
                      && UsedHotkeys is not null
                      && UsedHotkeys.Contains(HotkeyLogic.NormalizeHotkey(Hotkey));

        if (BoxBorder is not null)
        {
            BoxBorder.BorderBrush = HasConflict
                ? new SolidColorBrush(Color.Parse(ClaudePalette.Error))
                : new SolidColorBrush(capturing ? Color.Parse(ClaudePalette.Coral) : Color.Parse(ClaudePalette.RingWarm));
            BoxBorder.BorderThickness = new Avalonia.Thickness(capturing || HasConflict ? 1.5 : 1);
        }
    }
}
