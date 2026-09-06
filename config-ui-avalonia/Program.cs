using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Avalonia;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings;

/// <summary>
/// 程序入口：单实例守卫 + 初始化并启动 Avalonia 桌面应用。
/// 单实例约定 (与 AHK 侧 KeyFluxOpenSettings 三分支语义配合):
///   命名 Mutex "KeyFlux.Settings.SingleInstance"; 第二实例不重复开窗口,
///   激活已有窗口 (标题 "Setting", 限本进程) 后立即退出。
/// </summary>
internal static class Program
{
    /// <summary>进程创建时间 (Win32 GetProcessTimes; 托管 Stopwatch 在 Main 入口才初始化, 会漏掉运行时启动耗时)。</summary>
    public static readonly DateTime ProcessStartTimeUtc = GetProcessCreationTimeUtc();

    private const string SingleInstanceMutexName = "KeyFlux.Settings.SingleInstance";
    private const string MainWindowTitle = "Setting";

    /// <summary>
    /// 主入口点，启动经典桌面生命周期。
    /// </summary>
    [STAThread]
    public static void Main(string[] args)
    {
        // 单实例: 命名 Mutex 全局协调。createdNew=false 表示已有实例在运行。
        using var mutex = new Mutex(true, SingleInstanceMutexName, out var createdNew);
        if (!createdNew)
        {
            ActivateExistingWindow();
            return; // 第二实例直接退出, 不启动 Avalonia
        }

        // M-2: 系统光标启动自愈 + 崩溃兜底。窗口拾取准星用 SetSystemCursor 改的是系统全局光标表,
        // 不随进程退出回滚; 若上次会话硬崩溃 (FailFast/StackOverflow/外部 TerminateProcess/断电) 绕过 finally,
        // 桌面箭头会滞留准星。此处: ① 无条件幂等重载用户既有光标方案 (自愈上次残留, 不覆盖自定义);
        // ② 注册 AppDomain.UnhandledException + ProcessExit 兜底 (覆盖托管崩溃/正常退出路径)。
        WindowPickerService.InstallStartupCursorGuard();

        try
        {
            BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
        }
        finally
        {
            // 最末层兜底: 无论何种退出路径, 确保后端子进程整树终止, 绝不留孤儿。
            // (正常路径已在 MainWindow.Closing / App.Exit 执行, Shutdown 幂等)
            App.EnsureBackendShutdown();
        }
    }

    /// <summary>
    /// 构建 Avalonia 应用：平台自动检测 + Fluent 主题（见 App.axaml）。
    /// </summary>
    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .LogToTrace();

    // ----------------------------------------------------------- 窗口激活 (P/Invoke)

    /// <summary>
    /// 枚举顶层窗口, 找到本进程内标题为 "Setting" 的已有实例窗口
    /// (叠加进程 ID 过滤: 新标题较通用, 防误激活他进程同名窗口):
    /// 最小化则还原 (SW_RESTORE), 然后置为前台。找到第一个即停。
    /// </summary>
    private static void ActivateExistingWindow()
    {
        var currentPid = GetCurrentProcessId();
        EnumWindows((hWnd, _) =>
        {
            GetWindowThreadProcessId(hWnd, out var windowPid);
            if (windowPid != currentPid) return true; // 非本进程窗口, 跳过

            var title = GetWindowTitle(hWnd);
            if (title != MainWindowTitle) return true; // 继续枚举

            if (IsIconic(hWnd)) ShowWindow(hWnd, SW_RESTORE); // 最小化 -> 还原
            SetForegroundWindow(hWnd);
            return false; // 已找到, 停止枚举
        }, IntPtr.Zero);
    }

    private static string GetWindowTitle(IntPtr hWnd)
    {
        var sb = new StringBuilder(512);
        return GetWindowText(hWnd, sb, sb.Capacity) > 0 ? sb.ToString() : "";
    }

    private const int SW_RESTORE = 9;

    private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    private static extern bool IsIconic(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentProcessId();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    // ----------------------------------------------------------- 进程创建时间 (启动计时)

    private static DateTime GetProcessCreationTimeUtc()
    {
        try
        {
            var h = System.Diagnostics.Process.GetCurrentProcess().Handle;
            if (GetProcessTimes(h, out var creation, out _, out _, out _))
            {
                return DateTime.FromFileTimeUtc(creation);
            }
        }
        catch { /* 失败时退化为当前时刻 (耗时略偏小) */ }
        return DateTime.UtcNow;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetProcessTimes(IntPtr hProcess,
        out long lpCreationTime, out long lpExitTime, out long lpKernelTime, out long lpUserTime);
}
