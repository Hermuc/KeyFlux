using Avalonia;

namespace KeyFlux.Settings.Services;

// ============================================================================
// 窗口匹配领域模型 (L2): 匹配类型枚举 + 窗口描述符 + 纯函数格式化器。
//
// 设计: 零 Win32 依赖、零 UI 依赖 —— WindowMatchFormatter 全部为静态纯函数,
// 可被单元测试直接调用 (沿用 HotkeyLogic / HotkeyLogicTests 范式)。
// 探测 (Win32WindowProbe) 负责把屏幕点解析为 WindowDescriptor, 本层只负责
// 把描述符按用户选择的匹配类型格式化为 AHK WinTitle 可消费的标识符串。
// ============================================================================

/// <summary>
/// 窗口匹配类型: 决定 <see cref="WindowMatchFormatter.Format"/> 生成何种 AHK 标识符。
/// 默认 <see cref="TitleAndExe"/> (组合: 窗口名 + 进程名, 比单用进程名更精确,
/// 与项目自带 WindowSpy 推荐一致)。其余类型作为未来可选增强预留。
/// </summary>
public enum WindowMatchKind
{
    /// <summary>仅进程名: "ahk_exe name.exe"。</summary>
    Exe,

    /// <summary>进程完整路径: "ahk_exe C:\path\name.exe"。</summary>
    FullPath,

    /// <summary>窗口类名: "ahk_class ClassName"。</summary>
    Class,

    /// <summary>窗口标题 (原样): "{Title}"。</summary>
    Title,

    /// <summary>进程 ID: "ahk_pid {Pid}"。</summary>
    Pid,

    /// <summary>窗口句柄 (十进制): "ahk_id {Hwnd}"。</summary>
    HwndId,

    /// <summary>组合 (默认): "{Title} ahk_exe {ExeName}"; 标题为空时退化为 "ahk_exe {ExeName}"。</summary>
    TitleAndExe,
}

/// <summary>
/// 窗口探测结果描述符 (物理像素坐标系, 全程不做 DPI 换算)。
/// 由 <see cref="Win32.Win32WindowProbe"/> 填充, 供 <see cref="WindowMatchFormatter"/> 格式化。
/// </summary>
/// <param name="Hwnd">顶层窗口句柄 (已 GetAncestor(GA_ROOT) 归一)。</param>
/// <param name="Title">窗口标题 (GetWindowText)。</param>
/// <param name="ClassName">窗口类名 (GetClassName)。</param>
/// <param name="Pid">进程 ID (UWP 修正后为真实进程)。</param>
/// <param name="ExeName">进程名 (Path.GetFileName)。</param>
/// <param name="FullPath">进程完整路径。</param>
/// <param name="Bounds">窗口边界 (物理像素, 优先 DWM 扩展边框, 回落 GetWindowRect)。</param>
public sealed record WindowDescriptor(
    IntPtr Hwnd,
    string Title,
    string ClassName,
    int Pid,
    string ExeName,
    string FullPath,
    PixelRect Bounds);

/// <summary>
/// 匹配格式化纯函数: <see cref="WindowDescriptor"/> + <see cref="WindowMatchKind"/> -> AHK 标识符串。
/// 无任何 Win32 / UI 依赖, 可直接单测。
/// </summary>
public static class WindowMatchFormatter
{
    /// <summary>
    /// 按匹配类型格式化窗口描述符。
    /// TitleAndExe (默认): "{Title.Trim()} ahk_exe {ExeName}"; 标题为 null/空白时退化 "ahk_exe {ExeName}"。
    /// </summary>
    public static string Format(WindowDescriptor d, WindowMatchKind kind) => kind switch
    {
        WindowMatchKind.Exe => $"ahk_exe {d.ExeName}",
        WindowMatchKind.FullPath => $"ahk_exe {d.FullPath}",
        WindowMatchKind.Class => $"ahk_class {d.ClassName}",
        WindowMatchKind.Title => d.Title,
        WindowMatchKind.Pid => $"ahk_pid {d.Pid}",
        WindowMatchKind.HwndId => $"ahk_id {d.Hwnd}",
        WindowMatchKind.TitleAndExe => FormatTitleAndExe(d),
        _ => FormatTitleAndExe(d),
    };

    private static string FormatTitleAndExe(WindowDescriptor d)
    {
        var title = d.Title?.Trim();
        return string.IsNullOrWhiteSpace(title)
            ? $"ahk_exe {d.ExeName}"
            : $"{title} ahk_exe {d.ExeName}";
    }
}
