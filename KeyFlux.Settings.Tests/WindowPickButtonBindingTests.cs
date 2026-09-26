using System.ComponentModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Data;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Markup.Xaml.Styling;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Themes.Fluent;
using KeyFlux.Settings.Controls;
using KeyFlux.Settings.Services;

[assembly: AvaloniaTestApplication(typeof(KeyFlux.Settings.Tests.WindowPickBindingBootstrapper))]

namespace KeyFlux.Settings.Tests;

/// <summary>
/// Headless 测试宿主 Application。刻意不走被测 <c>App</c> —— 其
/// OnFrameworkInitializationCompleted 会创建 MainViewModel/MainWindow 并拉起
/// settings.exe 后端子进程; 测试只需要控件 XAML 依赖的 StaticResource
/// (Tr / NotEmpty / IntStr, 对应被测 App.axaml 的注册) 与 FluentTheme。
/// </summary>
public sealed class WindowPickBindingTestApp : Application
{
    public override void Initialize()
    {
        Styles.Add(new FluentTheme());
        // Claude 暖色共享主题 (令牌 + 复用类样式), 对应被测 App.axaml 的 StyleInclude 注册。
        // 必须加载: 插件页/市场窗/主窗的页面级 {StaticResource Claude*} 全部依赖其 Styles.Resources,
        // 缺它则构造 XAML 时立刻抛 KeyNotFoundException, 测试根本走不到真正的断言。
        // 注意 StyleInclude 的 Source 必须显式赋值 —— 构造函数参数是 baseUri, 不是加载目标。
        Styles.Add(new StyleInclude(new Uri("avares://KeyFlux.Settings/"))
        {
            Source = new Uri("avares://KeyFlux.Settings/Styles/Skins/Claude.axaml"),
        });
        Resources["Tr"] = new I18nConverter();
        Resources["NotEmpty"] = new StringNotEmptyConverter();
        Resources["IntStr"] = new IntToStringConverter();
        // 补全被测 App.axaml 的注册 (MainWindow 的导航图标绑定与部分视图的 IsVisible 依赖);
        // 插件页注册测试需实例化真实 MainWindow 以读取其 ContentControl.DataTemplates。
        Resources["NotNull"] = new NotNullConverter();
        Resources["MdiFont"] = new FontFamily(
            "avares://KeyFlux.Settings/Assets/Fonts/materialdesignicons.ttf#Material Design Icons");
        // 镜像 App.axaml 的皮肤侧 Fluent 模板覆盖 (夹具不加载 App.axaml, 故需手动同步;
        // 与上方 Tr/NotEmpty/... 同一做法)。缺它们则 ToggleSwitch 开启态退回系统强调色,
        // SkinContractTests 的覆盖键校验也会红。
        Resources["ToggleSwitchFillOn"] = new SolidColorBrush(Color.Parse("#c96442"));
        Resources["ToggleSwitchStrokeOn"] = new SolidColorBrush(Color.Parse("#c96442"));
        Resources["ToggleSwitchKnobFillOn"] = new SolidColorBrush(Color.Parse("#faf9f5"));
        // 开启态的悬停/按下变体 —— Fluent 内置值是硬编码蓝, 必须一并覆盖
        Resources["ToggleSwitchFillOnPointerOver"] = new SolidColorBrush(Color.Parse("#e1957a"));
        Resources["ToggleSwitchStrokeOnPointerOver"] = new SolidColorBrush(Color.Parse("#e1957a"));
        Resources["ToggleSwitchKnobFillOnPointerOver"] = new SolidColorBrush(Color.Parse("#faf9f5"));
        Resources["ToggleSwitchFillOnPressed"] = new SolidColorBrush(Color.Parse("#c96442"));
        Resources["ToggleSwitchStrokeOnPressed"] = new SolidColorBrush(Color.Parse("#c96442"));
        Resources["ToggleSwitchKnobFillOnPressed"] = new SolidColorBrush(Color.Parse("#faf9f5"));
        Resources["ToggleSwitchFillOffPointerOver"] = new SolidColorBrush(Colors.Transparent);
        Resources["ToggleSwitchStrokeOffPointerOver"] = new SolidColorBrush(Color.Parse("#87867f"));
        Resources["ToggleSwitchFillOffPressed"] = new SolidColorBrush(Colors.Transparent);
        Resources["ToggleSwitchStrokeOffPressed"] = new SolidColorBrush(Color.Parse("#5e5d59"));
        // 镜像 App.axaml 的系统强调色暖色化 (2026-09-13, 机制见 App.axaml 同名注释): 类型必须是 Color
        // —— Fluent 的 RadioButton/CheckBox/Slider/AutoComplete 选中态引用 SystemAccentColor 系,
        // 不覆盖则测试中这些控件退回 OS 蓝 #0078d7, SkinContractTests 的暖色契约也会红。
        Resources["SystemAccentColor"] = Color.Parse("#c96442");
        Resources["SystemAccentColorDark1"] = Color.Parse("#a8522f");
        Resources["SystemAccentColorDark2"] = Color.Parse("#8f4426");
        Resources["SystemAccentColorDark3"] = Color.Parse("#75351c");
        Resources["SystemAccentColorLight1"] = Color.Parse("#d97757");
        Resources["SystemAccentColorLight2"] = Color.Parse("#e1957a");
        Resources["SystemAccentColorLight3"] = Color.Parse("#f0b49e");
        // 镜像 App.axaml 的 ComboBox 弹层暖化与统一圆角 (2026-09-13, 机制见 App.axaml 同名注释)
        Resources["ComboBoxDropdownBackground"] = new SolidColorBrush(Color.Parse("#faf9f5"));
        Resources["ComboBoxDropdownBorderBrush"] = new SolidColorBrush(Color.Parse("#e8e6dc"));
        Resources["OverlayCornerRadius"] = new CornerRadius(8);
    }
}

/// <summary>Avalonia.Headless.XUnit 启动入口 (程序集级 AvaloniaTestApplication 引用)。</summary>
public static class WindowPickBindingBootstrapper
{
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<WindowPickBindingTestApp>()
            .UseHeadless(new AvaloniaHeadlessPlatformOptions { UseHeadlessDrawing = false })
            .UseSkia(); // Skia 渲染: 支撑 CaptureRenderedFrame 截帧诊断
}

/// <summary>测试 VM: 复刻宿主绑定源的 INPC 形态 (ActionEditorViewModel.WinTitle / WindowGroupRowVm.Value)。</summary>
public sealed class PickHostTestVm : INotifyPropertyChanged
{
    private string _winTitle = "";

    public string WinTitle
    {
        get => _winTitle;
        set
        {
            _winTitle = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(WinTitle)));
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;
}

/// <summary>
/// 「拾取窗口准星单击不回填」bug 的绑定链路回归锁定 (构造函数修复后时序)。
///
/// 回归锁定: 构造函数【修复后】不设 DataContext, WriteBack 必须沿 TwoWay 推送回宿主 VM;
/// 修复前构造函数末尾设 DataContext = this 会钉死绑定源 (历史 H2 裁决依据):
/// 本地值优先级高于继承, 宿主绑定初始化时源已是控件自身, 路径解析失败且永不重试,
/// SetCurrentValue 只改本地值、无法沿一条断裂的绑定推送回 VM。
///
/// 复刻宿主绑定 (Views/Controls/ActionEditorPanel.axaml:39 与 Views/WindowGroupDialogWindow.axaml:64):
///   ① <c>new WindowPickButton()</c> —— 构造函数不设 DataContext (修复后, 见控件构造函数注释);
///   ② 宿主 XAML 对 Text 设 <c>{Binding WinTitle}</c> (相对 DataContext, StyledProperty 默认 TwoWay);
///   ③ 挂入窗口 (窗口 DataContext = vm) 并 Show;
///   ④ 以 Success 语义调 <see cref="WindowPickButton.WriteBack"/>。
/// </summary>
/// <remarks>
/// 本类宿主的控件订阅 <see cref="I18n.Changed"/> 且其回调 SetAndRaise DirectProperty
/// (要求 UI 线程), 故纳入 I18nSerial 集合: 与会在(可能非 UI 的)线程上翻转
/// I18n.Language 的用例互斥, 否则抛 "Call from invalid thread"。
/// </remarks>
[Collection("I18nSerial")]
public sealed class WindowPickButtonBindingTests
{
    /// <summary>裁决主实验: Success 写回必须沿 TwoWay 绑定更新宿主 VM 源。</summary>
    [AvaloniaFact]
    public void WriteBack_Success_Pushes_Value_To_Hosted_Binding_Source()
    {
        var vm = new PickHostTestVm();
        var button = new WindowPickButton(); // 回归锁定: 构造函数【修复后】不设 DataContext (历史 H2 裁决依据, 见类注释)
        button.Bind(WindowPickButton.TextProperty,
            new Binding(nameof(PickHostTestVm.WinTitle)) { Mode = BindingMode.TwoWay });

        var window = new Window { Content = button, DataContext = vm };
        window.Show();
        try
        {
            button.WriteBack("记事本 ahk_exe notepad.exe");

            Assert.Equal("记事本 ahk_exe notepad.exe", vm.WinTitle);
        }
        finally
        {
            // 关闭窗口 -> 触发控件 DetachedFromVisualTree -> 退订 I18n.Changed。
            // 不关闭会留下订阅泄漏: 后续任何【非 UI 线程】的 I18n.Language 变更
            // (如 I18nResourceTests 的 [Fact] 用例) 都会命中该订阅, 在 UI 线程外
            // SetAndRaise DirectProperty 抛 "Call from invalid thread"。
            window.Close();
        }
    }

    /// <summary>对照实验: 无论绑定是否断裂, WriteBack 至少要更新控件本地 Text (区分"没写回"与"绑定断")。</summary>
    [AvaloniaFact]
    public void WriteBack_Always_Updates_Control_Local_Text()
    {
        var vm = new PickHostTestVm();
        var button = new WindowPickButton();
        button.Bind(WindowPickButton.TextProperty,
            new Binding(nameof(PickHostTestVm.WinTitle)) { Mode = BindingMode.TwoWay });
        var window = new Window { Content = button, DataContext = vm };
        window.Show();
        try
        {
            button.WriteBack("记事本 ahk_exe notepad.exe");

            Assert.Equal("记事本 ahk_exe notepad.exe", button.Text);
        }
        finally
        {
            window.Close(); // 关窗退订 I18n.Changed, 防跨用例订阅泄漏 (见首个用例注释)
        }
    }

    /// <summary>AppendMode 追加语义: 首次写回整值替换, 已有内容时合并为新行。</summary>
    [AvaloniaFact]
    public void WriteBack_AppendMode_Merges_As_New_Line()
    {
        var vm = new PickHostTestVm { WinTitle = "已有内容" };
        var button = new WindowPickButton { AppendMode = true };
        button.Bind(WindowPickButton.TextProperty,
            new Binding(nameof(PickHostTestVm.WinTitle)) { Mode = BindingMode.TwoWay });
        var window = new Window { Content = button, DataContext = vm };
        window.Show();
        try
        {
            button.WriteBack("第二行");

            Assert.Equal("已有内容\n第二行", vm.WinTitle);
        }
        finally
        {
            window.Close(); // 关窗退订 I18n.Changed, 防跨用例订阅泄漏 (见首个用例注释)
        }
    }

    /// <summary>
    /// M3 兼容性纯测试: FirstNoWindow 默认 false —— 既有构造点与 switch 零改动;
    /// 首次无窗口反馈路径显式传 FirstNoWindow=true。
    /// </summary>
    [Fact]
    public void WindowPickResult_FirstNoWindow_Defaults_To_False_And_Flags_On_First()
    {
        var legacy = new WindowPickResult(WindowPickStatus.Success, null, WindowMatchKind.TitleAndExe, "");
        Assert.False(legacy.FirstNoWindow);

        var firstNoWindow = new WindowPickResult(
            WindowPickStatus.FailedNoWindow, null, WindowMatchKind.TitleAndExe, "", FirstNoWindow: true);
        Assert.True(firstNoWindow.FirstNoWindow);
        Assert.Equal(WindowPickStatus.FailedNoWindow, firstNoWindow.Status);
    }
}
