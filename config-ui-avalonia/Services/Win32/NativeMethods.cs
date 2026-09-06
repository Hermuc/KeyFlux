using System.Runtime.InteropServices;
using System.Text;

namespace KeyFlux.Settings.Services.Win32;

// ============================================================================
// Win32 P/Invoke 基础设施层 (L1): 集中「窗口拾取准星」功能所需的全部原生 API。
//
// 风格约定 (对齐 Program.cs / BackendSession.cs 既有先例):
//   - 传统 [DllImport] (非 [LibraryImport]);
//   - kernel32 一律 SetLastError=true (需 Marshal.GetLastWin32Error 判定降级);
//     user32 不设 SetLastError;
//   - 字符串 API 用 CharSet.Unicode;
//   - 句柄统一用 IntPtr; 文本缓冲用 StringBuilder;
//   - 不用 ExactSpelling; 不显式设 CallingConvention。
// 按用途分组注释, 便于维护与检索。
// ============================================================================

internal static class NativeMethods
{
    // ------------------------------------------------------------------ 常量: 进程 / 错误
    public const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
    public const int ERROR_ACCESS_DENIED = 5;
    public const int ERROR_INSUFFICIENT_BUFFER = 122; // QueryFullProcessImageNameW 缓冲不足, 翻倍重试

    // ------------------------------------------------------------------ 常量: 窗口祖先 / DWM
    public const uint GA_ROOT = 2;
    public const uint DWMWA_EXTENDED_FRAME_BOUNDS = 9;

    // ------------------------------------------------------------------ 常量: 钩子
    public const int WH_MOUSE_LL = 14;
    public const int WH_KEYBOARD_LL = 13;

    // ------------------------------------------------------------------ 常量: 虚拟键 / 光标
    public const uint VK_ESCAPE = 0x1B;
    public const int IDC_CROSS = 32515;
    public const int IDC_ARROW = 32512;

    // ------------------------------------------------------------------ 常量: 系统级光标替换 (High#2)
    // SetCursor 在钩子线程只改本线程光标、系统不显示; 必须用 SetSystemCursor 做系统级替换。
    // 关键陷阱: SetSystemCursor 会销毁传入句柄 -> 必须传 CopyImage 副本, 绝不可传 LoadCursor 的共享系统句柄。
    public const uint IMAGE_CURSOR = 2;
    public const uint LR_COPYRETURNORG = 0x00000008;
    public const uint OCR_NORMAL = 32512;          // 标准箭头 (被替换为准星)
    public const uint SPI_SETCURSORS = 0x0057;     // 重载用户光标方案 (Cleanup 恢复)
    public const uint SPIF_UPDATEINIFILE = 0x01;
    public const uint SPIF_SENDCHANGE = 0x02;

    // ------------------------------------------------------------------ 常量: 窗口样式 (高亮框)
    public const uint WS_POPUP = 0x80000000;
    public const uint WS_EX_TOPMOST = 0x00000008;
    public const uint WS_EX_TRANSPARENT = 0x00000020;
    public const uint WS_EX_TOOLWINDOW = 0x00000080;
    public const uint WS_EX_LAYERED = 0x00080000;
    public const uint WS_EX_NOACTIVATE = 0x08000000;

    // ------------------------------------------------------------------ 常量: SetWindowPos
    public const uint SWP_NOSIZE = 0x0001;
    public const uint SWP_NOMOVE = 0x0002;
    public const uint SWP_NOZORDER = 0x0004;
    public const uint SWP_NOACTIVATE = 0x0010;
    public const uint SWP_SHOWWINDOW = 0x0040;
    public const uint SWP_HIDEWINDOW = 0x0080;
    public const uint SWP_NOOWNERZORDER = 0x0200;

    public static readonly IntPtr HWND_TOPMOST = new(-1);

    // ------------------------------------------------------------------ 常量: ShowWindow
    public const int SW_HIDE = 0;
    public const int SW_SHOWNOACTIVATE = 4;
    public const int SW_SHOW = 5;

    // ------------------------------------------------------------------ 常量: 消息
    public const uint WM_DESTROY = 0x0002;
    public const uint WM_PAINT = 0x000F;
    public const uint WM_QUIT = 0x0012;
    public const uint WM_ERASEBKGND = 0x0014;
    public const uint WM_TIMER = 0x0113;
    public const uint WM_KEYDOWN = 0x0100;
    public const uint WM_KEYUP = 0x0101;           // Medium#4: 吞配对 UP (Esc)
    public const uint WM_SYSKEYDOWN = 0x0104;
    public const uint WM_SYSKEYUP = 0x0105;        // Medium#4: Alt+Esc 的系统键 UP
    public const uint WM_MOUSEMOVE = 0x0200;
    public const uint WM_LBUTTONDOWN = 0x0201;
    public const uint WM_LBUTTONUP = 0x0202;       // Medium#4: 吞配对 UP (左键)
    public const uint WM_LBUTTONDBLCLK = 0x0203;   // Medium#4: 双击纳入 down 分支
    public const uint WM_RBUTTONDOWN = 0x0204;
    public const uint WM_RBUTTONUP = 0x0205;       // Medium#4: 吞配对 UP (右键, 防孤立 UP 弹残留菜单)
    public const uint WM_RBUTTONDBLCLK = 0x0206;   // Medium#4: 双击纳入 down 分支
    // 自定义线程消息 (PostThreadMessage 唤醒专用消息泵)
    public const uint WM_APP_COMMIT = 0x8001;
    public const uint WM_APP_CANCEL = 0x8002;

    // ------------------------------------------------------------------ 常量: PeekMessage (High#3 预建消息队列)
    public const uint PM_NOREMOVE = 0x00000000;
    public const uint PM_REMOVE = 0x00000001;   // H-A: DrainPairedUp 轮询取消息 (取出即移除), 与定时器解耦的有界排空

    // ------------------------------------------------------------------ 常量: 区域合成 / 分层窗
    public const int RGN_DIFF = 4;
    public const uint LWA_COLORKEY = 0x00000001;
    public const uint LWA_ALPHA = 0x00000002;

    // ================================================================== 结构体

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT
    {
        public int X;
        public int Y;

        public POINT(int x, int y) { X = x; Y = y; }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MSG
    {
        public IntPtr hwnd;
        public uint message;
        public IntPtr wParam;
        public IntPtr lParam;
        public uint time;
        public POINT pt;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MSLLHOOKSTRUCT
    {
        public POINT pt;
        public uint mouseData;
        public uint flags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct KBDLLHOOKSTRUCT
    {
        public uint vkCode;
        public uint scanCode;
        public uint flags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct WNDCLASS
    {
        public uint style;
        public IntPtr lpfnWndProc;
        public int cbClsExtra;
        public int cbWndExtra;
        public IntPtr hInstance;
        public IntPtr hIcon;
        public IntPtr hCursor;
        public IntPtr hbrBackground;
        public string? lpszMenuName;
        public string? lpszClassName;
    }

    // ================================================================== 委托
    /// <summary>低级钩子回调 (WH_MOUSE_LL / WH_KEYBOARD_LL 共用签名)。</summary>
    public delegate IntPtr LowLevelProc(int nCode, IntPtr wParam, IntPtr lParam);

    /// <summary>窗口过程回调。</summary>
    public delegate IntPtr WndProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    /// <summary>定时器回调 (hwnd=NULL 定时器, 由 DispatchMessage 在消息泵线程调用)。</summary>
    public delegate void TimerProc(IntPtr hWnd, uint uMsg, IntPtr nIdEvent, uint dwTime);

    /// <summary>EnumChildWindows 回调 (返回 false 停止枚举)。</summary>
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    // ================================================================== 组1: 命中测试 / 窗口信息
    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out POINT lpPoint);

    [DllImport("user32.dll")]
    public static extern IntPtr WindowFromPoint(POINT point);

    [DllImport("user32.dll")]
    public static extern IntPtr GetAncestor(IntPtr hWnd, uint gaFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);

    // ================================================================== 组2: 进程信息
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr OpenProcess(uint dwDesiredAccess, bool bInheritHandle, uint dwProcessId);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern bool QueryFullProcessImageNameW(IntPtr hProcess, uint dwFlags, StringBuilder lpExeName, ref uint lpdwSize);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GetProcessTimes(IntPtr hProcess,
        out long lpCreationTime, out long lpExitTime, out long lpKernelTime, out long lpUserTime);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr hObject);

    // ================================================================== 组3: DWM 边界
    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr hWnd, uint dwAttribute, out RECT pvAttribute, uint cbAttribute);

    // ================================================================== 组4: 高亮框窗口 (类注册 / 创建 / 区域 / 分层)
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern ushort RegisterClass(ref WNDCLASS lpWndClass);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr CreateWindowExW(uint dwExStyle, string lpClassName, string? lpWindowName, uint dwStyle,
        int X, int Y, int nWidth, int nHeight, IntPtr hWndParent, IntPtr hMenu, IntPtr hInstance, IntPtr lpParam);

    [DllImport("user32.dll")]
    public static extern IntPtr DefWindowProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool DestroyWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);

    [DllImport("user32.dll")]
    public static extern int SetWindowRgn(IntPtr hWnd, IntPtr hRgn, bool bRedraw);

    [DllImport("user32.dll")]
    public static extern bool SetLayeredWindowAttributes(IntPtr hwnd, uint crKey, byte bAlpha, uint dwFlags);

    [DllImport("gdi32.dll")]
    public static extern IntPtr CreateRectRgn(int x1, int y1, int x2, int y2);

    [DllImport("gdi32.dll")]
    public static extern int CombineRgn(IntPtr hrgnDest, IntPtr hrgnSrc1, IntPtr hrgnSrc2, int iMode);

    [DllImport("gdi32.dll")]
    public static extern IntPtr CreateSolidBrush(uint color);

    [DllImport("gdi32.dll")]
    public static extern bool DeleteObject(IntPtr hObject);

    // ================================================================== 组5: 全局钩子 + 专用消息泵
    // Low#13: SetLastError=true -> 装钩失败时 Marshal.GetLastWin32Error() 记录错误码
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SetWindowsHookEx(int idHook, LowLevelProc lpfn, IntPtr hMod, uint dwThreadId);

    [DllImport("user32.dll")]
    public static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool UnhookWindowsHookEx(IntPtr hhk);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr GetModuleHandle(string? lpModuleName);

    [DllImport("kernel32.dll")]
    public static extern uint GetCurrentThreadId();

    [DllImport("user32.dll")]
    public static extern bool PostThreadMessage(uint idThread, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern int GetMessage(out MSG lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax);

    // High#3: 装钩前 PeekMessage(PM_NOREMOVE) 强制创建本线程消息队列, 消除 PostThreadMessage 丢信号竞态
    [DllImport("user32.dll")]
    public static extern bool PeekMessage(out MSG lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax, uint wRemoveMsg);

    [DllImport("user32.dll")]
    public static extern bool TranslateMessage(ref MSG lpMsg);

    [DllImport("user32.dll")]
    public static extern IntPtr DispatchMessage(ref MSG lpMsg);

    [DllImport("user32.dll")]
    public static extern IntPtr SetTimer(IntPtr hWnd, IntPtr nIDEvent, uint uElapse, TimerProc? lpTimerFunc);

    [DllImport("user32.dll")]
    public static extern bool KillTimer(IntPtr hWnd, IntPtr uIDEvent);

    // ================================================================== 组6: 光标
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr LoadCursor(IntPtr hInstance, IntPtr lpCursorName);

    [DllImport("user32.dll")]
    public static extern IntPtr SetCursor(IntPtr hCursor);

    [DllImport("user32.dll")]
    public static extern IntPtr GetCursor();

    // L-2: SetSystemCursor 失败时手动销毁 CopyImage 副本, 防 GDI 句柄泄漏
    [DllImport("user32.dll")]
    public static extern bool DestroyCursor(IntPtr hCursor);

    // High#2: 系统级光标替换 + 恢复
    /// <summary>替换系统光标 (会销毁传入句柄 -> 必须传 CopyImage 副本)。id 见 OCR_* 常量。</summary>
    [DllImport("user32.dll")]
    public static extern bool SetSystemCursor(IntPtr hcur, uint id);

    /// <summary>
    /// 复制光标/图像, 供 SetSystemCursor 安全销毁 (SetSystemCursor 会销毁传入句柄)。
    /// L-1 实测结论: type=IMAGE_CURSOR + flags=LR_COPYRETURNORG 返回真正的私有副本 (返回值 != 源句柄), 交系统销毁安全。
    /// 反例警示: 切勿改用 LR_COPYFROMRESOURCE —— 该标志意在把「共享的资源句柄」换成私有副本, 但对已是私有的句柄
    /// 可能原样返回同一句柄; 一旦把 LoadCursor 的共享系统句柄 (或与源相同的句柄) 交给会销毁它的 SetSystemCursor,
    /// 将销毁系统共享句柄导致整桌面光标损坏。调用方 ApplySystemCrossCursor 已加 (copy==Zero || copy==cross) 不变量守卫。
    /// </summary>
    [DllImport("user32.dll")]
    public static extern IntPtr CopyImage(IntPtr h, uint type, int cx, int cy, uint flags);

    /// <summary>SPI_SETCURSORS 重载用户光标方案 (Cleanup 恢复, 保留自定义方案, 优于强制 IDC_ARROW)。</summary>
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool SystemParametersInfo(uint uiAction, uint uiParam, IntPtr pvParam, uint fWinIni);

    // ================================================================== 便捷封装: LoadCursor(IDC_CROSS)
    /// <summary>加载标准准星光标 (MAKEINTRESOURCE(IDC_CROSS))。</summary>
    public static IntPtr LoadCrossCursor() => LoadCursor(IntPtr.Zero, new IntPtr(IDC_CROSS));

    /// <summary>加载标准箭头光标 (MAKEINTRESOURCE(IDC_ARROW))。</summary>
    public static IntPtr LoadArrowCursor() => LoadCursor(IntPtr.Zero, new IntPtr(IDC_ARROW));
}
