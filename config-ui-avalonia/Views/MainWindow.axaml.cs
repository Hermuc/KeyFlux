using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 主窗口：标题 "Setting"（AHK 侧以 "Setting ahk_exe KeyFlux.Settings.exe" 匹配窗口）。
/// 标题栏小图标透明化经 <see cref="TitleBarIconSuppressor"/> 统一接入 (与两个对话框窗口共用)。
/// 生命周期: Opened -> 自激活一次 + InitializeAsync (连接后端/加载配置);
/// Closing -> 同步关停后端会话 (整树 Kill); 另有 App.Exit 与 Program.Main finally 两层兜底。
/// </summary>
public partial class MainWindow : Window
{
    private readonly MainViewModel _viewModel;

    public MainWindow(MainViewModel viewModel)
    {
        InitializeComponent();
        _viewModel = viewModel;
        DataContext = viewModel;

        // 模态提示对话框需要 Owner (保存 400 弹后端 message 等)
        if (viewModel.Messages is DialogMessageService dialogs)
        {
            dialogs.Owner = this;
        }

        // 标题栏小图标透明化: 助手内部订阅 Opened(应用)/ScalingChanged(DPI 变化重放)/Closed(回收句柄)
        TitleBarIconSuppressor.Attach(this);

        // 唤起时自激活一次; 进程无前台授权时可能被系统拒绝, 由 AHK 侧 Z 序兜底补足
        // (刻意不设 Topmost: 非常驻置顶, 仍可手动切后台)
        Opened += (_, _) =>
        {
            if (WindowState == WindowState.Minimized)
            {
                WindowState = WindowState.Normal;
            }
            Activate();
            _ = viewModel.InitializeAsync();
        };
        Closing += (_, _) => viewModel.Session.Shutdown();
    }

    // ===================== 自绘标题栏 (无边框窗口, 见 MainWindow.axaml 顶部注释) =====================
    // 窗口用 ExtendClientAreaChromeHints=NoChrome 去掉了系统标题栏与三键,
    // 拖动/最大化/最小化/关闭全部由下面四个处理器承担 —— 少了它们会导致窗口无法移动或关闭。
    // Alt+F4 仍是系统级兜底 (不依赖本处代码)。

    /// <summary>标题栏拖动区: 左键拖拽移动窗口, 双击在最大化/还原之间切换。</summary>
    private void OnTitleBarPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (!e.GetCurrentPoint(this).Properties.IsLeftButtonPressed) return;
        if (e.ClickCount == 2)
        {
            ToggleMaximize();
            return;
        }
        // 最大化状态下拖拽交给系统还原语义, 避免出现"拖不动的最大化窗口"
        if (WindowState == WindowState.Normal)
        {
            BeginMoveDrag(e);
        }
    }

    private void OnMinimizeClick(object? sender, RoutedEventArgs e)
        => WindowState = WindowState.Minimized;

    private void OnMaximizeClick(object? sender, RoutedEventArgs e)
        => ToggleMaximize();

    private void OnCloseClick(object? sender, RoutedEventArgs e)
        => Close();

    private void ToggleMaximize()
        => WindowState = WindowState == WindowState.Maximized
            ? WindowState.Normal
            : WindowState.Maximized;
}
