using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Avalonia;
using Avalonia.Media;
using Avalonia.Media.Fonts;
using Avalonia.Win32;
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
    /// FontFallbacks: 让未拆段的零星 emoji 字符可回退到 Segoe UI Emoji 取到字形
    /// (文本栈仅单色; 彩色渲染由 MarkdownRenderer 的 Skia 位图方案负责)。
    /// </summary>
    /// <summary>
    /// 渲染管线开关 (2026-09-24, 动效流畅度优化引入)。<b>默认仍是软件渲染</b> —— 见下方 2026-09-12
    /// 的瞬峰治理理由与"勿删此配置"契约。
    ///
    /// <para><b>为什么现在需要这个开关。</b> 2026-09-12 那条决策的前提是"设置面板为静态内容, 软件渲染
    /// 视觉无差"。⚠ 该前提自 2026-09-24 起不再成立: 选项页新增了「卷轴摊开」布局动画 (揭示层
    /// <c>MaxHeight</c> 0→内容自然高), 每帧都要重画整张卡的内容。实测 (真 Skia 软件光栅, 逐帧探针):
    /// 轻卡 (约 50 个视觉元素) 每帧中位 1.4ms; 重卡 (命令框皮肤卡, 511 个元素) 每帧 <b>20~30ms</b>
    /// (Debug 构建; Release 约快 3~5×) ⇒ 60Hz 的 16.6ms 预算被单帧吃穿 ⇒ 掉帧即"卡顿"。
    /// 消融实验 (摘投影 / 摘裁剪 / 摘卷曲带) 都不改变量级 ⇒ 成本在"整卡内容逐帧重光栅"本身,
    /// 是渲染管线属性, 不是动画代码可优化的 ⇒ 唯一有效杠杆是换 GPU 渲染。</para>
    ///
    /// <para><b>用法与边界。</b> 置环境变量 <c>KEYFLUX_RENDER_GPU=1</c> ⇒ GPU 光栅 (ANGLE/D3D) +
    /// WinUI 合成, 各留软件回落; 不置则完全维持原行为。**在完成"瞬峰/内存回归实测"之前不得改默认值**
    /// (契约要求, 该实测需要真机启动设置面板采样内存, 故留开关先供试用/测量)。</para>
    /// </summary>
    internal static bool UseGpuRendering =>
        Environment.GetEnvironmentVariable("KEYFLUX_RENDER_GPU") == "1";

    public static AppBuilder BuildAvaloniaApp()
    {
        var builder = AppBuilder.Configure<App>().UsePlatformDetect();

        if (!UseGpuRendering)
        {
            builder = builder.With(new Win32PlatformOptions
            {
                // 2026-09-12 启动瞬峰治理 (dotnet-counters 剖析: GC 托管堆仅 ~15MB、分配速率
                // ~1.2MB/s, 私有提交却冲 400-650MB —— 大头是 GPU 渲染管线在进程内的
                // ANGLE/D3D11/WinUIComposition 交换链与驱动分配)。设置面板为静态内容, 软件渲染
                // 视觉无差, 换取瞬峰压平与更低常驻内存; RedirectionSurface = 渲染进缓冲后经
                // GDI 重定向呈现, 全程不建 D3D 设备。
                // ⚠ 渲染契约: 换回 GPU 渲染需回归瞬峰/内存实测; 勿删此配置。
                RenderingMode = new[] { Win32RenderingMode.Software },
                CompositionMode = new[] { Win32CompositionMode.RedirectionSurface },
            });
        }
        else
        {
            // GPU 档: 显式写出"GPU 优先 + 软件兜底", 不用平台默认值以免将来默认值变化时行为漂移
            builder = builder.With(new Win32PlatformOptions
            {
                RenderingMode = new[] { Win32RenderingMode.AngleEgl, Win32RenderingMode.Software },
                CompositionMode = new[]
                {
                    Win32CompositionMode.WinUIComposition,
                    Win32CompositionMode.RedirectionSurface,
                },
            });
        }

        return builder
            .With(new FontManagerOptions
            {
                FontFallbacks = new[]
                {
                    new FontFallback { FontFamily = new FontFamily("Segoe UI Emoji") }
                }
            })
            .LogToTrace();
    }

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
