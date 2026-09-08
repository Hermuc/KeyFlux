using System.ComponentModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Data;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
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
        Resources["Tr"] = new I18nConverter();
        Resources["NotEmpty"] = new StringNotEmptyConverter();
        Resources["IntStr"] = new IntToStringConverter();
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

        button.WriteBack("记事本 ahk_exe notepad.exe");

        Assert.Equal("记事本 ahk_exe notepad.exe", vm.WinTitle);
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

        button.WriteBack("记事本 ahk_exe notepad.exe");

        Assert.Equal("记事本 ahk_exe notepad.exe", button.Text);
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

        button.WriteBack("第二行");

        Assert.Equal("已有内容\n第二行", vm.WinTitle);
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
