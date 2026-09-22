using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml.MarkupExtensions;
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
/// <b>磨砂通道 (2026-09-22 定案)</b>: <c>SetWindowCompositionAttribute(WCA_ACCENT_POLICY=19,
/// ACCENT_ENABLE_ACRYLICBLURBEHIND)</c> —— 24H2 探针实测返回 True 且渲染出强磨砂
/// (用户目视确认 Terminal 观感)。DWMSBT 路线在 Avalonia 窗口上返回成功但不渲染材质,
/// 已废弃。非 solid 时窗口层画刷置**全透明**, accent 的 GradientColor 暖纱
/// (Parchment, alpha 随滑块) 充当唯一窗口色调; solid 时 ACCENT_DISABLED + 实色画刷。
/// </para>
/// <para>
/// <b>ContentSurface 随 Apply 联动</b>: 8 个对话框 Window.Background 固定消费
/// ClaudeContentSurfaceBrush (皮肤静态默认 85% 半透明); solid 模式
/// (透明度=0 / 未启用 / 段缺失) 时 Apply 同步把它覆写为实色 Parchment ——
/// 否则毛玻璃关闭时无磨砂材质, 对话框会以 85% 半透明直接叠在锐利桌面上。
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
        bool solid = option is null || !option.Enabled || option.Transparency <= 0;

        // 窗口层 (2026-09-22 accent 通道): 非 solid 时置**全透明** —— accent 的
        // GradientColor 暖纱充当唯一的窗口色调, 画刷再叠 alpha 会双重变实。
        // solid 时维持原契约: 实色 Parchment。
        resources[SurfaceResourceKey] = solid
            ? CreateBrush(option)
            : new SolidColorBrush(Colors.Transparent);

        // 内容层联动 (2026-09-22 R1): solid 模式下对话框不能再坐在 85% 半透明上
        resources[ContentSurfaceResourceKey] = solid
            ? CreateBrush(null)   // 实色 Parchment (alpha=255)
            : new SolidColorBrush(Color.Parse("#D9f5f4ed")); // 与皮肤静态定义同值

        // 已打开的窗口立即跟随 (设置页拖滑块时不用重开窗口)
        if (Application.Current.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            foreach (var window in desktop.Windows)
                ApplyAccent(window, option);
        }
    }

    /// <summary>最近一次应用的毛玻璃配置 (迟开窗口经 <see cref="Attach"/> 取用)。</summary>
    private static AcrylicOption? _lastOption;

    /// <summary>
    /// 窗口打开时应用当前毛玻璃设置 (各窗口 ctor 调一次) —— 覆盖「在主窗口之后才打开」
    /// 的对话框: <see cref="Apply"/> 只遍历已打开窗口, 迟开者须经本入口补挂。
    /// <para>
    /// <paramref name="backgroundResourceKey"/>: 该窗口 Background 原本消费的资源键
    /// (主窗 = <see cref="SurfaceResourceKey"/>, 8 对话框 = <see cref="ContentSurfaceResourceKey"/>)。
    /// accent 失败退化会写**本地值**盖住动态绑定 (R3-1), 恢复时按此键还原。
    /// </para>
    /// </summary>
    public static void Attach(Window window, string backgroundResourceKey)
    {
        _backgroundKeys[window] = backgroundResourceKey;
        // Closed 时移除注册, 防窗口关闭后字典泄漏
        window.Closed += (_, _) =>
        {
            _backgroundKeys.Remove(window);
            _fallbackActive.Remove(window);
        };
        window.Opened += (_, _) => ApplyAccent(window, _lastOption);
    }

    /// <summary>Attach 注册的窗口 -> 其 Background 消费的资源键 (退化恢复用, R3-1)。</summary>
    private static readonly Dictionary<Window, string> _backgroundKeys = new();

    /// <summary>当前处于「本地画刷兜底」状态的窗口 (Background 被本地值接管, R3-1)。</summary>
    private static readonly HashSet<Window> _fallbackActive = new();

    // ---- 真·毛玻璃: SetWindowCompositionAttribute accent 通道 ----
    //
    // 🔴 2026-09-22 路线定案 (两轮探针实测):
    //  · DWMSBT (DwmSetWindowAttribute/DWMWA_SYSTEMBACKDROP_TYPE) 在 Avalonia 窗口上
    //    返回成功 (hr=0) 但**不渲染材质** —— 视觉是「锐利壁纸 + 轻纱」, 已废弃。
    //  · SetWindowCompositionAttribute(WCA_ACCENT_POLICY=19, ACCENT_ENABLE_ACRYLICBLURBEHIND=4)
    //    在本机 Win11 24H2 返回 True 且渲染出强磨砂 (用户截图确认 Terminal 观感)。
    //    早期「24H2 accent 失效」的论断是探针自身枚举 bug (WCA 写成 1, 正确 19) 误导。
    // ⚠ WCA_ACCENT_POLICY = 19 —— 写 1 (NCRENDERING_ENABLED) 必返回 err 87。
    private const int WcaAccentPolicy = 19;              // WCA_ACCENT_POLICY
    private const int AccentStateDisabled = 0;           // ACCENT_DISABLED
    private const int AccentStateAcrylicBlurBehind = 4;  // ACCENT_ENABLE_ACRYLICBLURBEHIND
    private const int AccentFlagsProbe = 2;              // 与探针一致 (值 2)

    /// <summary>
    /// 非 solid 时暖纱 alpha 下限 (0x2E ≈ 18%): T=100 时磨砂最强, 仍留一丝暖色调。
    /// </summary>
    public const byte MinAccentAlpha = 0x2E;

    [StructLayout(LayoutKind.Sequential)]
    private struct AccentPolicy
    {
        public int AccentState;
        public int AccentFlags;
        public uint GradientColor;
        public int AnimationId;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct WindowCompositionAttributeData
    {
        public int Attribute;
        public IntPtr Data;
        public int SizeOfData;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetWindowCompositionAttribute(IntPtr hwnd, ref WindowCompositionAttributeData data);

    /// <summary>
    /// 由配置算出 accent 暖纱 GradientColor (AABBGGRR, Parchment #f5f4ed)。
    /// alpha = clamp(255 - T*255/100, 0x2E, 255): T=100 -> 0x2E (18%, 磨砂最强),
    /// T 越小越实。solid 输入 (null/未启用/T=0) 按 T=0 处理 -> 0xFF 实色
    /// (此时 AccentState=DISABLED, 该值不被消费, 返回确定值便于测试)。
    /// </summary>
    internal static uint BuildAccentGradientColor(AcrylicOption? option)
    {
        var p = Color.Parse(ClaudePalette.Parchment);
        int t = option is { Enabled: true } ? Math.Clamp(option.Transparency, 0, 100) : 0;
        int alpha = Math.Clamp(255 - t * 255 / 100, MinAccentAlpha, 255);
        return ((uint)alpha << 24) | ((uint)p.B << 16) | ((uint)p.G << 8) | p.R;
    }

    /// <summary>
    /// 给窗口挂/摘 accent 亚克力磨砂。
    /// <para>
    /// 毛玻璃开且 T>0 -> ACCENT_ENABLE_ACRYLICBLURBEHIND + Parchment 暖纱
    /// (alpha 随「背景透明度」滑块); solid -> ACCENT_DISABLED。
    /// 失败 (返回 False, 如 Win10 部分版本/远程会话) 且为毛玻璃模式时, 把该窗口
    /// Background 直接覆写为半透明 Parchment 画刷 —— 退化为「仅半透明」的原画刷
    /// 路径, 不至于全透看穿锐利桌面。
    /// </para>
    /// </summary>
    public static void ApplyAccent(Window window, AcrylicOption? option)
    {
        bool frost = option is { Enabled: true, Transparency: > 0 };
        try
        {
            var hwnd = window.TryGetPlatformHandle()?.Handle ?? IntPtr.Zero;
            if (hwnd == IntPtr.Zero)
                return;

            var policy = new AccentPolicy
            {
                AccentState = frost ? AccentStateAcrylicBlurBehind : AccentStateDisabled,
                AccentFlags = AccentFlagsProbe,
                GradientColor = BuildAccentGradientColor(option),
                AnimationId = 0,
            };
            int size = Marshal.SizeOf<AccentPolicy>();
            IntPtr policyPtr = Marshal.AllocHGlobal(size);
            try
            {
                Marshal.StructureToPtr(policy, policyPtr, false);
                var data = new WindowCompositionAttributeData
                {
                    Attribute = WcaAccentPolicy,
                    Data = policyPtr,
                    SizeOfData = size,
                };
                bool ok = SetWindowCompositionAttribute(hwnd, ref data);
                int err = Marshal.GetLastWin32Error();

                if (!ok && frost && _backgroundKeys.ContainsKey(window))
                {
                    // 退化: accent 挂不上 -> 该窗口直接坐回半透明 Parchment 画刷
                    // (本地兜底; 仅限经 Attach 注册过资源键的窗口, 否则日后无法还原)
                    window.Background = CreateBrush(option);
                    _fallbackActive.Add(window);
                }
                else if (_fallbackActive.Remove(window))
                {
                    // 恢复 (accent ok / 切 solid): 撤销本地兜底, 还原 Background
                    // 动态资源绑定 —— 本地值会永久盖住资源, 不还原则 T=0 实色契约
                    // 和滑块联动双双失灵 (R3-1)
                    if (_backgroundKeys.TryGetValue(window, out var restoreKey))
                        window[!Window.BackgroundProperty] = new DynamicResourceExtension(restoreKey);
                }

                // 诊断日志 (验证期暂留, 确认毛玻璃生效后移除)
                try
                {
                    File.AppendAllText(
                        Path.Combine(Path.GetTempPath(), "kf_acrylic.log"),
                        $"{DateTime.Now:HH:mm:ss} hwnd=0x{hwnd.ToInt64():X} title={window.Title} accent={(ok ? "ok" : "FAIL")} err={err} state={policy.AccentState} alpha=0x{(policy.GradientColor >> 24) & 0xFF:X2} hint={string.Join(",", window.TransparencyLevelHint)}\n");
                }
                catch
                {
                    // 日志失败不影响功能
                }
            }
            finally
            {
                Marshal.FreeHGlobal(policyPtr);
            }
        }
        catch
        {
            // 非 Windows 平台 / 句柄不可得: 无背景材质, 行为同现状
        }
    }
}
