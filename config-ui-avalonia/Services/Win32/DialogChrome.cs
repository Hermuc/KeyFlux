using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace KeyFlux.Settings.Services.Win32;

// ============================================================================
// DialogChrome —— Win11 原生标题栏着色。
// 设计决策 (与 §窗口 chrome 先例一致): 弹窗保留系统标题栏 (不做无边框自绘),
// 但默认的冷白标题栏与 Claude 暖色 UI 割裂 —— 经 DWM 标题栏着色接口
// (DWMWA_CAPTION_COLOR / TEXT_COLOR / BORDER_COLOR, Windows 11 22000+) 把标题栏
// 染成当前主题表面色, 原生关闭/最大化按钮与拖动行为原样保留。
// 非 Win11 系统 DwmSetWindowAttribute 返回非零 HRESULT, 静默忽略无副作用。
//
// 颜色取当前 ClaudeWindowSurfaceBrush 的 RGB (跟随用户亚克力设置的基础色调;
// DWM 标题栏不支持半透明, 故忽略 alpha —— 透明度 0 时与内容完全同色, 高透明度
// 时标题栏为同色系实心, 可接受)。文字用 NearBlack。回退 Parchment。
// ============================================================================

internal static class DialogChrome
{
    private const string SurfaceBrushKey = "ClaudeWindowSurfaceBrush";

    /// <summary>
    /// 构造函数中调用。句柄在构造期尚不存在, 实际着色延迟到 Opened 事件 (Show 之后)。
    /// 幂等: 每窗口实例至多生效一次。
    /// </summary>
    public static void Apply(Window window)
    {
        var caption = Color.Parse("#f5f4ed"); // Parchment 回退
        if (Application.Current?.TryGetResource(SurfaceBrushKey, out var value) == true
            && value is ISolidColorBrush brush)
        {
            caption = brush.Color;
        }
        var text = Color.Parse("#141413"); // NearBlack

        window.Opened += (_, _) =>
        {
            var hwnd = window.TryGetPlatformHandle()?.Handle ?? IntPtr.Zero;
            if (hwnd == IntPtr.Zero)
                return;
            SetColor(hwnd, NativeMethods.DWMWA_CAPTION_COLOR, caption);
            SetColor(hwnd, NativeMethods.DWMWA_TEXT_COLOR, text);
            SetColor(hwnd, NativeMethods.DWMWA_BORDER_COLOR, caption);
        };
    }

    /// <summary>COLORREF = 0x00BBGGRR; HRESULT 非零静默忽略。</summary>
    private static void SetColor(IntPtr hwnd, uint attribute, Color c)
    {
        var colorref = (uint)(c.R | (c.G << 8) | (c.B << 16));
        NativeMethods.DwmSetWindowAttribute(hwnd, attribute, ref colorref, sizeof(uint));
    }
}
