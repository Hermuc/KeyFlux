using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings;

/// <summary>
/// 应用程序类：加载 App.axaml 并在框架初始化完成后创建主窗口。
/// 启动参数 (复刻任务规格):
///   --port &lt;N&gt; 直连已运行后端 (开发调试);
///   缺省时由 BackendSession 拉起同目录 settings.exe --headless。
/// </summary>
public partial class App : Application
{
    private static MainViewModel? _mainViewModel;

    /// <summary>
    /// 最末层兜底关停 (Program.Main finally 调用): 无论何种退出路径,
    /// 确保 settings.exe 子进程被整树终止。BackendSession.Shutdown 幂等。
    /// </summary>
    public static void EnsureBackendShutdown() => _mainViewModel?.Session.Shutdown();

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var options = BackendSessionOptions.Parse(desktop.Args ?? []);
            var viewModel = new MainViewModel(options);
            _mainViewModel = viewModel;
            desktop.MainWindow = new MainWindow(viewModel);

            // Dispatcher 生命周期退出兜底 (窗口 Closing 已先行关停; Shutdown 幂等)
            desktop.Exit += (_, _) => viewModel.Session.Shutdown();
        }

        base.OnFrameworkInitializationCompleted();
    }
}
