using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Avalonia.Controls;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 标题栏小图标透明化助手 (MainWindow / WindowGroupDialogWindow / OverviewEditWindow 共用)。
/// 用法: 窗口构造函数中调用 <c>TitleBarIconSuppressor.Attach(this)</c> 一次即可。
///
/// 原理: 窗口 Opened 后把标题栏小图标 (WM_SETICON/ICON_SMALL) 替换为全透明 HICON——
/// 不能靠删 XAML Icon 属性实现 (Avalonia 会用 csproj ApplicationIcon 打包的默认图标兜底,
/// 橙标照样出现); 任务栏/Alt-Tab/悬停预览用 ICON_BIG, 不受影响仍显橙色图标。
/// ScalingChanged 重放对抗 WM_DPICHANGED -> Avalonia WindowImpl.RefreshIcon() 重设真实小图标
/// (Avalonia 11.3 AppWndProc 中 RefreshIcon 先于 ScalingChanged 触发, 时序确定);
/// Closed 时销毁句柄并把槽位清空, 防泄漏/悬垂。
///
/// 视觉: 标题栏图标槽留空白占位、标题文字不左移 (Win32 非客户区布局决定, 属预期而非 bug)。
/// 幂等: per-window 句柄存于订阅闭包, ScalingChanged 重入先销毁旧句柄再重建;
/// ConditionalWeakTable 防 Attach 重入双订阅 (弱表, 不阻止窗口被 GC)。
/// </summary>
public static class TitleBarIconSuppressor
{
    private static readonly ConditionalWeakTable<Window, object> Attached = new();

    /// <summary>接入标题栏小图标透明化。每窗口仅生效一次 (重复调用为 no-op)。</summary>
    public static void Attach(Window window)
    {
        if (!Attached.TryAdd(window, new object())) return;

        // per-window 透明图标句柄: 存于下列本地函数共享的闭包, 随窗口事件生命周期存续
        var blankIcon = IntPtr.Zero;

        void Apply()
        {
            var hwnd = window.TryGetPlatformHandle()?.Handle ?? IntPtr.Zero;
            if (hwnd == IntPtr.Zero) return;

            // 幂等: 重建前先摘下并销毁旧透明图标, 防句柄泄漏/悬垂
            if (blankIcon != IntPtr.Zero)
            {
                SendMessage(hwnd, WM_SETICON, ICON_SMALL, IntPtr.Zero);
                DestroyIcon(blankIcon);
                blankIcon = IntPtr.Zero;
            }

            blankIcon = CreateFullyTransparentIcon();
            if (blankIcon == IntPtr.Zero) return;

            SendMessage(hwnd, WM_SETICON, ICON_SMALL, blankIcon);
            // 强制非客户区重算+重绘, 立即刷新标题栏显示
            SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }

        void Restore()
        {
            if (blankIcon == IntPtr.Zero) return;
            var hwnd = window.TryGetPlatformHandle()?.Handle ?? IntPtr.Zero;
            if (hwnd != IntPtr.Zero) SendMessage(hwnd, WM_SETICON, ICON_SMALL, IntPtr.Zero); // 先摘下防悬垂
            DestroyIcon(blankIcon);
            blankIcon = IntPtr.Zero;
        }

        // ScalingChanged 用 lambda 订阅: 11.3 事件参数非公开泛型类型, 靠推断避开硬编码签名
        window.Opened += (_, _) => Apply();
        window.ScalingChanged += (_, _) => Apply();
        window.Closed += (_, _) => Restore();
    }

    // ------------------------------------------------- Win32 (自 MainWindow.axaml.cs 迁入)

    private const uint WM_SETICON = 0x0080;
    private const IntPtr ICON_SMALL = 0; // wParam: 标题栏小图标槽; ICON_BIG=1 (任务栏/Alt-Tab) 不动
    private const int SM_CXSMICON = 49;
    private const int SM_CYSMICON = 50;
    private const uint BI_BITFIELDS = 3;
    private const uint DIB_RGB_COLORS = 0;
    private const uint SWP_NOSIZE = 0x0001, SWP_NOMOVE = 0x0002, SWP_NOZORDER = 0x0004,
        SWP_NOACTIVATE = 0x0010, SWP_FRAMECHANGED = 0x0020;

    /// <summary>
    /// 确定性构造全透明 HICON: 32bpp BI_BITFIELDS XOR 位图全零 (ARGB 全透明,
    /// 掩码 R=0x00FF0000/G=0x0000FF00/B=0x000000FF, 第 4 字节为 A=0xFF000000) +
    /// 单色 AND 掩码全 0xFF, 经 CreateIconIndirect 合成; 两个临时位图随即释放,
    /// 返回的 HICON 由调用方负责 DestroyIcon。
    /// </summary>
    private static IntPtr CreateFullyTransparentIcon()
    {
        var cx = GetSystemMetrics(SM_CXSMICON);
        var cy = GetSystemMetrics(SM_CYSMICON);
        if (cx <= 0) cx = 16;
        if (cy <= 0) cy = 16;

        var bmi = new BITMAPINFO
        {
            bmiHeader = new BITMAPINFOHEADER
            {
                biSize = (uint)Marshal.SizeOf<BITMAPINFOHEADER>(),
                biWidth = cx,
                biHeight = -cy, // 负高 = top-down
                biPlanes = 1,
                biBitCount = 32,
                biCompression = BI_BITFIELDS,
            },
            redMask = 0x00FF0000,
            greenMask = 0x0000FF00,
            blueMask = 0x000000FF,
        };

        var hXor = CreateDIBSection(IntPtr.Zero, ref bmi, DIB_RGB_COLORS, out var pBits, IntPtr.Zero, 0);
        if (hXor == IntPtr.Zero || pBits == IntPtr.Zero)
        {
            if (hXor != IntPtr.Zero) DeleteObject(hXor);
            return IntPtr.Zero;
        }

        // CreateDIBSection 像素内存未初始化, 显式清零 -> ARGB 全透明
        Marshal.Copy(new byte[cx * cy * 4], 0, pBits, cx * cy * 4);

        // AND 掩码: 单色位图每行按 WORD 对齐, 全 0xFF (32bpp 下 alpha 优先, 仅占位)
        var andStride = ((cx + 15) / 16) * 2;
        var andBits = new byte[andStride * cy];
        Array.Fill(andBits, (byte)0xFF);
        var hAnd = CreateBitmap(cx, cy, 1, 1, andBits);
        if (hAnd == IntPtr.Zero)
        {
            DeleteObject(hXor);
            return IntPtr.Zero;
        }

        var ii = new ICONINFO { fIcon = 1, hbmMask = hAnd, hbmColor = hXor };
        var hIcon = CreateIconIndirect(ref ii);

        DeleteObject(hAnd);
        DeleteObject(hXor);
        return hIcon;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BITMAPINFOHEADER
    {
        public uint biSize;
        public int biWidth;
        public int biHeight;
        public ushort biPlanes;
        public ushort biBitCount;
        public uint biCompression;
        public uint biSizeImage;
        public int biXPelsPerMeter;
        public int biYPelsPerMeter;
        public uint biClrUsed;
        public uint biClrImportant;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BITMAPINFO
    {
        public BITMAPINFOHEADER bmiHeader;
        public uint redMask;
        public uint greenMask;
        public uint blueMask;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ICONINFO
    {
        public int fIcon; // BOOL: 1 = icon
        public int xHotspot;
        public int yHotspot;
        public IntPtr hbmMask;
        public IntPtr hbmColor;
    }

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    [DllImport("gdi32.dll", SetLastError = true)]
    private static extern IntPtr CreateDIBSection(IntPtr hdc, ref BITMAPINFO pbmi,
        uint iUsage, out IntPtr ppvBits, IntPtr hSection, uint dwOffset);

    [DllImport("gdi32.dll", SetLastError = true)]
    private static extern IntPtr CreateBitmap(int nWidth, int nHeight, uint nPlanes, uint nBitCount, byte[] lpBits);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr CreateIconIndirect(ref ICONINFO pliconinfo);

    [DllImport("gdi32.dll")]
    private static extern bool DeleteObject(IntPtr hObject);

    [DllImport("user32.dll")]
    private static extern bool DestroyIcon(IntPtr hIcon);

    [DllImport("user32.dll")]
    private static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter,
        int x, int y, int cx, int cy, uint uFlags);
}
