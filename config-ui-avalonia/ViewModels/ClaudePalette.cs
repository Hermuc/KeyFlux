using Avalonia.Media;

namespace KeyFlux.Settings;

/// <summary>
/// Claude / Anthropic 暖色令牌的 C# 真源, 与 <c>Styles/ClaudeTheme.axaml</c> 同值同源。
/// <para>
/// 资源字典 (XAML) 不会被 C# 自动查找, 故 8 处 C# 色值引用点 (计划书 §三) 改为引用本类,
/// 与 XAML 资源键保持单一事实来源, 避免两处各写一份 hex 导致日后漂移。
/// 本文件为提交 1 仅建好此类; 实际替换 8 处引用点在提交 5。
/// </para>
/// </summary>
public static class ClaudePalette
{
    // ── 画刷 / 颜色令牌 (浅色 18 项, 与 ClaudeTheme.axaml 一一对应) ──
    public const string Parchment      = "#f5f4ed";
    public const string Ivory          = "#faf9f5";
    public const string White          = "#ffffff";
    public const string Sand           = "#e8e6dc";
    public const string NearBlack      = "#141413";
    public const string CharcoalWarm   = "#4d4c48";
    public const string OliveGray      = "#5e5d59";
    public const string StoneGray      = "#87867f";
    public const string DarkWarm       = "#3d3d3a";
    public const string Terracotta     = "#c96442";
    public const string Coral          = "#d97757";
    public const string Error          = "#b53333";
    public const string MutedGreen     = "#5e7d5a";
    public const string MutedGreenSoft = "#e7ebe3";
    public const string BorderCream    = "#f0eee6";
    public const string BorderWarm     = "#e8e6dc";
    public const string RingWarm       = "#d1cfc5";
    public const string RingDeep       = "#c2c0b6";

    // ── 暗色桩 (4 项, 定义但不接线) ──
    public const string DarkSurface = "#30302e";
    public const string DeepDark    = "#141413";
    public const string WarmSilver = "#b0aea5";
    public const string BorderDark  = "#30302e";

    // ── IBrush 静态实例 (与 XAML 资源键同值) ──
    public static readonly IBrush ParchmentBrush       = new SolidColorBrush(Color.Parse(Parchment));
    public static readonly IBrush IvoryBrush           = new SolidColorBrush(Color.Parse(Ivory));
    public static readonly IBrush WhiteBrush           = new SolidColorBrush(Color.Parse(White));
    public static readonly IBrush SandBrush            = new SolidColorBrush(Color.Parse(Sand));
    public static readonly IBrush NearBlackBrush       = new SolidColorBrush(Color.Parse(NearBlack));
    public static readonly IBrush CharcoalWarmBrush    = new SolidColorBrush(Color.Parse(CharcoalWarm));
    public static readonly IBrush OliveGrayBrush       = new SolidColorBrush(Color.Parse(OliveGray));
    public static readonly IBrush StoneGrayBrush       = new SolidColorBrush(Color.Parse(StoneGray));
    public static readonly IBrush DarkWarmBrush        = new SolidColorBrush(Color.Parse(DarkWarm));
    public static readonly IBrush TerracottaBrush      = new SolidColorBrush(Color.Parse(Terracotta));
    public static readonly IBrush CoralBrush           = new SolidColorBrush(Color.Parse(Coral));
    public static readonly IBrush ErrorBrush           = new SolidColorBrush(Color.Parse(Error));
    public static readonly IBrush MutedGreenBrush      = new SolidColorBrush(Color.Parse(MutedGreen));
    public static readonly IBrush MutedGreenSoftBrush  = new SolidColorBrush(Color.Parse(MutedGreenSoft));
    public static readonly IBrush BorderCreamBrush     = new SolidColorBrush(Color.Parse(BorderCream));
    public static readonly IBrush BorderWarmBrush      = new SolidColorBrush(Color.Parse(BorderWarm));
    public static readonly IBrush RingWarmBrush        = new SolidColorBrush(Color.Parse(RingWarm));
    public static readonly IBrush RingDeepBrush        = new SolidColorBrush(Color.Parse(RingDeep));

    // ── 暗色桩 IBrush ──
    public static readonly IBrush DarkSurfaceBrush = new SolidColorBrush(Color.Parse(DarkSurface));
    public static readonly IBrush DeepDarkBrush    = new SolidColorBrush(Color.Parse(DeepDark));
    public static readonly IBrush WarmSilverBrush  = new SolidColorBrush(Color.Parse(WarmSilver));
    public static readonly IBrush BorderDarkBrush  = new SolidColorBrush(Color.Parse(BorderDark));

    // ── 焦点环色 (Coral, 计划书 §四 裁定, 非 Focus Blue) ──
    public static readonly Color FocusRingColor = Color.Parse(Coral);
}
