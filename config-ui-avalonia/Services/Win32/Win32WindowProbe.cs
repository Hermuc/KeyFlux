using System.Runtime.InteropServices;
using System.Text;
using Avalonia;
using static KeyFlux.Settings.Services.Win32.NativeMethods;

namespace KeyFlux.Settings.Services.Win32;

// ============================================================================
// 窗口探测层 (L3): 屏幕物理点 -> 顶层窗口 -> 进程映像 -> WindowDescriptor。
//
// 探测序列 (对齐方案 §六.1): WindowFromPoint -> GetAncestor(GA_ROOT) ->
//   GetWindowText + GetClassName + GetWindowThreadProcessId ->
//   OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION) -> QueryFullProcessImageNameW ->
//   finally CloseHandle -> Path.GetFileName。
// 关键边界:
//   - UWP 修正 (复刻 Functions.ahk L107-126): ApplicationFrameHost.exe 时回溯子 hwnd 真实进程;
//   - 自身过滤: pid==Environment.ProcessId -> FailedNoWindow (防止拾取设置窗口/高亮框);
//   - 提权降级: OpenProcess ACCESS_DENIED -> FailedAccessDenied (绝不静默给错值);
//   - PID 复用安全缓存: Dictionary<(pid, createTime), fullPath>, 会话级、结束整体丢弃;
//   - 禁用 Process.MainModule.FileName (跨位数抛异常/句柄不透明);
//   - 全程物理像素同一坐标系闭环, 不做 DPI 换算。
// ============================================================================

/// <summary>窗口探测抽象 (接口化 -> 可 mock / 可整体替换)。</summary>
public interface IWindowProbe
{
    /// <summary>探测给定屏幕物理像素点下的顶层窗口。</summary>
    WindowProbeResult ProbeAt(PixelPoint screenPointPhysical);
}

/// <summary>
/// 探测结果: 成功时 <see cref="Window"/> 非空且 <see cref="Status"/>=Success;
/// OpenProcess ACCESS_DENIED -> FailedAccessDenied; 无窗口/自身进程 -> FailedNoWindow。
/// </summary>
public sealed record WindowProbeResult(WindowDescriptor? Window, WindowPickStatus Status);

/// <summary><see cref="IWindowProbe"/> 的 Win32 实现。</summary>
public sealed class Win32WindowProbe : IWindowProbe
{
    /// <summary>ApplicationFrameHost.exe 进程名 (UWP 商店应用外壳进程)。</summary>
    private const string ApplicationFrameHostExe = "ApplicationFrameHost.exe";

    /// <summary>
    /// (pid, createTime) -> 进程完整路径 会话级缓存。createTime 取自 GetProcessTimes,
    /// 规避 PID 复用; 探测器随拾取会话创建/丢弃, 零常驻内存。
    /// </summary>
    private readonly Dictionary<(int Pid, long CreateTime), string> _cache = [];

    public WindowProbeResult ProbeAt(PixelPoint screenPointPhysical)
    {
        IntPtr hwnd = WindowFromPoint(new POINT(screenPointPhysical.X, screenPointPhysical.Y));
        if (hwnd == IntPtr.Zero)
        {
            return new WindowProbeResult(null, WindowPickStatus.FailedNoWindow);
        }

        // 统一到顶层窗口 (命中点可能落在子控件上)
        hwnd = GetAncestor(hwnd, GA_ROOT);
        if (hwnd == IntPtr.Zero)
        {
            return new WindowProbeResult(null, WindowPickStatus.FailedNoWindow);
        }

        // Low#12: 自身进程过滤提到 GetWindowText/GetClassName 之前 ——
        // 避免悬停自身窗口时每 tick 对自己 UI 线程发 WM_GETTEXT (跨线程阻塞风险)
        GetWindowThreadProcessId(hwnd, out var pidU);
        if (pidU == 0)
        {
            return new WindowProbeResult(null, WindowPickStatus.FailedNoWindow);
        }
        if (pidU == (uint)Environment.ProcessId)
        {
            // 防止拾取设置窗口自己或高亮框 (根本手段, 不依赖 WS_EX_TRANSPARENT 语义)
            return new WindowProbeResult(null, WindowPickStatus.FailedNoWindow);
        }

        var title = GetWindowTitle(hwnd);
        var className = GetWindowClassName(hwnd);

        var (fullPath, status) = ResolveProcessImage(pidU);
        if (status != WindowPickStatus.Success)
        {
            return new WindowProbeResult(null, status);
        }

        var pid = unchecked((int)pidU);
        var exeName = Path.GetFileName(fullPath);

        // UWP 修正: 商店应用真实进程挂在 ApplicationFrameHost 的子 hwnd 上
        if (string.Equals(exeName, ApplicationFrameHostExe, StringComparison.OrdinalIgnoreCase))
        {
            var corrected = TryResolveUwpChild(hwnd, pidU);
            if (corrected is not null)
            {
                // Low#7: 找到子窗口但 OpenProcess 被拒 -> FailedAccessDenied (勿静默回落 AFH), 交控件走 I18n 提示
                if (corrected.Value.Status == WindowPickStatus.FailedAccessDenied)
                {
                    return new WindowProbeResult(null, WindowPickStatus.FailedAccessDenied);
                }
                pid = corrected.Value.Pid;
                fullPath = corrected.Value.FullPath;
                exeName = Path.GetFileName(fullPath);
            }
        }

        var descriptor = new WindowDescriptor(hwnd, title, className, pid, exeName, fullPath, GetBounds(hwnd));
        return new WindowProbeResult(descriptor, WindowPickStatus.Success);
    }

    // ------------------------------------------------------------------ 进程映像解析

    /// <summary>
    /// OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION) -> QueryFullProcessImageNameW -> finally CloseHandle。
    /// 返回 (完整路径, 状态); 句柄为零时按 GetLastWin32Error 区分 ACCESS_DENIED 与其它失败。
    /// </summary>
    private (string FullPath, WindowPickStatus Status) ResolveProcessImage(uint pid)
    {
        IntPtr hProc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if (hProc == IntPtr.Zero)
        {
            // 提权降级: 绝不静默回落错误值 —— ACCESS_DENIED 明确上报 FailedAccessDenied
            var err = Marshal.GetLastWin32Error();
            return ("", err == ERROR_ACCESS_DENIED
                ? WindowPickStatus.FailedAccessDenied
                : WindowPickStatus.FailedNoWindow);
        }

        try
        {
            // Low#11: 检查 GetProcessTimes 返回值 —— 失败则不以 (pid,0) 作缓存键 (假键会撞 PID 复用), 直接查询不缓存
            var haveTimes = GetProcessTimes(hProc, out var createTime, out _, out _, out _);
            if (haveTimes)
            {
                var key = (unchecked((int)pid), createTime);
                if (_cache.TryGetValue(key, out var cached))
                {
                    return (cached, WindowPickStatus.Success);
                }

                var cachedPath = QueryFullImageName(hProc);
                if (string.IsNullOrEmpty(cachedPath))
                {
                    return ("", WindowPickStatus.FailedNoWindow);
                }

                _cache[key] = cachedPath;
                return (cachedPath, WindowPickStatus.Success);
            }

            var full = QueryFullImageName(hProc);
            if (string.IsNullOrEmpty(full))
            {
                return ("", WindowPickStatus.FailedNoWindow);
            }
            return (full, WindowPickStatus.Success);
        }
        finally
        {
            CloseHandle(hProc);
        }
    }

    /// <summary>
    /// UWP 修正 (复刻 Functions.ahk L107-126): EnumChildWindows 找首个 PID != AFH 的子 hwnd,
    /// 重取其真实进程映像。找不到则返回 null (调用方保留 AFH 结果)。
    /// </summary>
    private (int Pid, string FullPath, WindowPickStatus Status)? TryResolveUwpChild(IntPtr afhHwnd, uint afhPid)
    {
        (int Pid, string FullPath, WindowPickStatus Status)? found = null;

        // 局部函数捕获为委托, 同步调用期间保持存活
        EnumWindowsProc cb = (child, _) =>
        {
            GetWindowThreadProcessId(child, out var childPid);
            if (childPid != 0 && childPid != afhPid)
            {
                var (path, st) = ResolveProcessImage(childPid);
                if (st == WindowPickStatus.Success)
                {
                    found = (unchecked((int)childPid), path, WindowPickStatus.Success);
                    return false; // 停止枚举
                }
                // Low#7: 找到非 AFH 子窗口但 OpenProcess 被拒 -> 上报 FailedAccessDenied, 勿静默回落 AFH
                if (st == WindowPickStatus.FailedAccessDenied)
                {
                    found = (0, "", WindowPickStatus.FailedAccessDenied);
                    return false; // 停止枚举
                }
            }
            return true; // 继续枚举 (FailedNoWindow: 该子窗口无法解析, 试下一个)
        };

        EnumChildWindows(afhHwnd, cb, IntPtr.Zero);
        GC.KeepAlive(cb); // Low#6: 防止委托在原生回调期间被 GC 回收
        return found;
    }

    // ------------------------------------------------------------------ 便捷封装

    private static string QueryFullImageName(IntPtr hProc)
    {
        // Low#10: ERROR_INSUFFICIENT_BUFFER(122) 时缓冲翻倍重试 (上限 32768), 兜底超长路径
        var capacity = 1024;
        while (true)
        {
            var sb = new StringBuilder(capacity);
            var size = (uint)capacity;
            if (QueryFullProcessImageNameW(hProc, 0, sb, ref size))
            {
                return sb.ToString(0, (int)size);
            }

            var err = Marshal.GetLastWin32Error();
            if (err == ERROR_INSUFFICIENT_BUFFER && capacity < 32768)
            {
                capacity = Math.Min(capacity * 2, 32768);
                continue;
            }
            return "";
        }
    }

    private static string GetWindowTitle(IntPtr hwnd)
    {
        var sb = new StringBuilder(GetWindowTextLength(hwnd) + 1);
        return GetWindowText(hwnd, sb, sb.Capacity) > 0 ? sb.ToString() : "";
    }

    private static string GetWindowClassName(IntPtr hwnd)
    {
        var sb = new StringBuilder(256);
        return GetClassName(hwnd, sb, sb.Capacity) > 0 ? sb.ToString() : "";
    }

    /// <summary>窗口边界 (物理像素): 优先 DwmGetWindowAttribute(EXTENDED_FRAME_BOUNDS), 失败回落 GetWindowRect。</summary>
    private static PixelRect GetBounds(IntPtr hwnd)
    {
        var size = (uint)Marshal.SizeOf<RECT>();
        if (DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, out var r, size) == 0)
        {
            return ToPixelRect(r);
        }
        return GetWindowRect(hwnd, out r) ? ToPixelRect(r) : default;
    }

    private static PixelRect ToPixelRect(RECT r)
        => new(r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top);
}
