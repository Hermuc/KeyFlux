using Avalonia.Controls;
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
}
