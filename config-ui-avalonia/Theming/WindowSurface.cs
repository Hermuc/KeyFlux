using Avalonia;
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
/// </summary>
public static class WindowSurface
{
    /// <summary>窗口底色令牌键 (与 Styles/Skins/*.axaml 中的定义同名)。</summary>
    public const string SurfaceResourceKey = "ClaudeWindowSurfaceBrush";

    /// <summary>即便透明度拉到 100 也保留的最小不透明度 (15%)。</summary>
    public const double MinOpacity = 0.15;

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
        Application.Current!.Resources[SurfaceResourceKey] = CreateBrush(option);
    }
}
