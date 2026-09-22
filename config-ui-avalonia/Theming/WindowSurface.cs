using System;
using System.IO;
using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Media;
using KeyFlux.Settings.Models;

namespace KeyFlux.Settings.Theming;

/// <summary>
/// 亚克力(毛玻璃)窗口底色的**唯一应用点**。
/// <para>
/// 用法: 设置变更时调 <see cref="Apply"/>; 所有窗口以
/// <c>{DynamicResource ClaudeWindowSurfaceBrush}</c> 取底色, 无需逐窗改代码,
/// 换肤也只需保持同名令牌。
/// </para>
/// <para>
/// <b>为什么透明度 0 必须给实色</b>: 透明度为 0 表示"不要透明",
/// 此时若底色仍带 alpha, 窗口会把画面叠在背后的未知像素上, 表现为发灰/花屏/文字糊。
/// 故 0 -> 完全不透明 (alpha=255) 的实色 Parchment。
/// </para>
/// <para>
/// <b>为什么 100 仍夹一个最小不透明度</b>: 全透明时文字直接压在模糊背景上可读性差,
/// 且一旦平台不支持透明 (TransparencyLevelHint 回落 None) 会更糟。
/// 故夹 <see cref="MinOpacity"/>。
/// </para>
/// <para>
/// <b>ContentSurface 随 Apply 联动</b>: 8 个对话框 Window.Background 固定消费
/// ClaudeContentSurfaceBrush (皮肤静态默认 85% 半透明); solid 模式
/// (透明度=0 / 未启用 / 段缺失) 时 Apply 同步把它覆写为实色 Parchment ——
/// 否则毛玻璃关闭时 DWMSBT_NONE 无模糊, 对话框会以 85% 半透明直接叠在锐利桌面上。
/// </para>
/// </summary>
public static class WindowSurface
{
    /// <summary>窗口底色令牌键 (与 Styles/Skins/*.axaml 中的定义同名)。</summary>
    public const string SurfaceResourceKey = "ClaudeWindowSurfaceBrush";

    /// <summary>
    /// 内容层底色令牌键 (对话框 Window.Background 消费)。
    /// <see cref="Apply"/> 与 <see cref="SurfaceResourceKey"/> 同模式联动覆写:
    /// solid 模式 -> 实色 Parchment, 非 solid -> 皮肤静态默认 (#D9f5f4ed, ≈85%)。
    /// </summary>
    public const string ContentSurfaceResourceKey = "ClaudeContentSurfaceBrush";

    /// <summary>
    /// 即便透明度拉到 100 也保留的最小不透明度 (35%)。
    /// 🔴 2026-09-21 演进 (三改): 0.15 -> 0.65 -> 0.80 -> 0.35 + 内容层分层。
    /// 前两次直接提窗口底浓度是治标 —— 文字发虚的真因是内容 Transparent 直压在
    /// 半透明窗口层上, 亚克力噪点/壁纸纹理顶到笔画底下。现已引入近实色内容层
    /// ClaudeContentSurfaceBrush (见 Claude.axaml), 文字可读性由内容层保证,
    /// 窗口层浓度还给「背景透明度」滑块: 0.35 只防极端值下边隙花屏。
    /// </summary>
    public const double MinOpacity = 0.35;

    /// <summary>
    /// 由配置算出底色不透明度 0..1。
    /// <list type="bullet">
    /// <item>段缺失或未启用 -> 1.0 (实色)</item>
    /// <item>透明度 0      -> 1.0 (实色, 即需求要求的"给好背景颜色")</item>
    /// <item>透明度 100    -> <see cref="MinOpacity"/></item>
    /// </list>
    /// </summary>
    public static double OpacityFor(AcrylicOption? option)
    {
        if (option is null || !option.Enabled) return 1.0;

        var t = Math.Clamp(option.Transparency, 0, 100) / 100.0;
        return Math.Max(1.0 - t, MinOpacity);
    }

    /// <summary>按配置生成底色画刷 (Parchment + 算出的 alpha)。</summary>
    public static ISolidColorBrush CreateBrush(AcrylicOption? option)
    {
        var p = Color.Parse(ClaudePalette.Parchment);
        var alpha = (byte)Math.Round(OpacityFor(option) * 255);
        return new SolidColorBrush(Color.FromArgb(alpha, p.R, p.G, p.B));
    }

    /// <summary>
    /// 把当前配置写进应用资源, 使所有以 DynamicResource 取底色的窗口立即跟随。
    /// 写入 Application.Resources (而非皮肤文件的 Styles.Resources) —— 后者是皮肤真源,
    /// 不该被运行期改写; 此处同名键会覆盖它, 皮肤文件本身保持纯净。
    /// </summary>
    public static void Apply(AcrylicOption? option)
    {
        _lastOption = option;
        var resources = Application.Current!.Resources;
        resources[SurfaceResourceKey] = CreateBrush(option);

        // 内容层联动 (2026-09-22 R1): solid 模式下对话框不能再坐在 85% 半透明上
        bool solid = option is null || !option.Enabled || option.Transparency <= 0;
        resources[ContentSurfaceResourceKey] = solid
            ? CreateBrush(null)   // 实色 Parchment (alpha=255)
            : new SolidColorBrush(Color.Parse("#D9f5f4ed")); // 与皮肤静态定义同值

        // 已打开的窗口立即跟随 (设置页拖滑块时不用重开窗口)
        if (Application.Current.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            foreach (var window in desktop.Windows)
                ApplyBackdrop(window, option);
        }
    }

    /// <summary>最近一次应用的毛玻璃配置 (迟开窗口经 <see cref="Attach"/> 取用)。</summary>
    private static AcrylicOption? _lastOption;

    /// <summary>
    /// 窗口打开时应用当前毛玻璃设置 (各窗口 ctor 调一次) —— 覆盖「在主窗口之后才打开」
    /// 的对话框: <see cref="Apply"/> 只遍历已打开窗口, 迟开者须经本入口补挂。
    /// </summary>
    public static void Attach(Window window)
    {
        window.Opened += (_, _) => ApplyBackdrop(window, _lastOption);
    }

    // ---- 真·毛玻璃: Win11 22H2+ DWM 系统背景材质 ----

    private const int DwmwaSystembackdropType = 38;   // DWMWA_SYSTEMBACKDROP_TYPE
    private const int DwmsbtNone = 1;                 // DWMSBT_NONE: 无材质
    private const int DwmsbtTransientWindow = 3;      // DWMSBT_TRANSIENTWINDOW: 亚克力(毛玻璃)

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    /// <summary>
    /// 给窗口挂/摘 DWM 系统毛玻璃材质。
    /// <para>
    /// 🔴 为什么不走 TransparencyLevelHint 的 Blur/AcrylicBlur (2026-09-21):
    /// Win11 24H2 上 SetWindowCompositionAttribute 的 ACCENT_ENABLE_BLURBEHIND /
    /// ACCENT_ENABLE_ACRYLICBLURBEHIND 已被微软**静默失效** (实测: 窗口只透不糊,
    /// 壁纸锐利可见) —— Avalonia 的这两个透明级别走的正是该旧 API, 提示列表
    /// 「Blur,AcrylicBlur,Transparent」逐级尝试后实际停在 Transparent。
    /// 系统官方替代 = DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE):
    /// DWMSBT_TRANSIENTWINDOW (亚克力) / DWMSBT_NONE (无)。Win10 及更早调用失败
    /// 返回非 0 且无副作用, 行为退化回「仅半透明」= 原状, 不会崩。
    /// </para>
    /// <para>
    /// 材质分层: 系统毛玻璃在最底, 上面叠本类的半透明 Parchment 底色刷 ——
    /// 透明度滑块照常控制暖色调浓度, 模糊由系统提供。
    /// </para>
    /// </summary>
    public static void ApplyBackdrop(Window window, AcrylicOption? option)
    {
        try
        {
            var hwnd = window.TryGetPlatformHandle()?.Handle ?? IntPtr.Zero;
            if (hwnd == IntPtr.Zero)
                return;
            int backdrop = option is { Enabled: true, Transparency: > 0 }
                ? DwmsbtTransientWindow
                : DwmsbtNone;
            int hr = DwmSetWindowAttribute(hwnd, DwmwaSystembackdropType, ref backdrop, sizeof(int));

            // 诊断日志 (验证期暂留, 确认毛玻璃生效后移除): 记录挂载结果与提示的透明级别
            try
            {
                File.AppendAllText(
                    Path.Combine(Path.GetTempPath(), "kf_acrylic.log"),
                    $"{DateTime.Now:HH:mm:ss} hwnd=0x{hwnd.ToInt64():X} title={window.Title} backdrop={backdrop} hr={hr} hint={string.Join(",", window.TransparencyLevelHint)}\n");
            }
            catch
            {
                // 日志失败不影响功能
            }
        }
        catch
        {
            // 非 Windows 平台 / 句柄不可得: 无背景材质, 行为同现状
        }
    }
}
