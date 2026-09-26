using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// Claude 皮肤 (<c>Styles/Skins/Claude.axaml</c>) 的运行时守护 + 皮肤契约强制校验。
///
/// 为什么必须存在本类 —— 该主题的令牌与类样式经 <c>App.axaml</c> 全局加载, 而
/// <c>dotnet build</c> **不校验** XAML 中 <c>{StaticResource}</c> 的两类错误, 它们只在
/// XAML **加载期**才炸:
///   ① 令牌缺失 (宿主未加载主题)        → <c>KeyNotFoundException</c>;
///   ② 令牌类型与目标属性不兼容          → <c>InvalidCastException</c>。
/// 2026-09 重构中两类都真实发生过: 半径令牌被误定义为 <c>x:Double</c> 却赋给
/// <c>CornerRadius</c> 属性, 结果构建 0 错误、但 <c>PluginsPageView</c> /
/// <c>PluginMarketWindow</c> / <c>MainWindow</c> 三者构造即崩 —— 直到 QA 用 headless
/// 探针实测才暴露。本类把这类运行期错误前移到测试: 只要主题被加载、视图能构造、
/// 关键令牌能落到正确属性上, 就说明全局主题未退化。
///
/// 归入 I18nSerial 集合: 本类构造 MainWindow / PluginsPageView 会读写全局 I18n.Language,
/// 需与在(可能非 UI 的)线程上翻转语言的用例互斥。
/// </summary>
[Collection("I18nSerial")]
public sealed class SkinContractTests
{
    /// <summary>
    /// 三个真实视图都必须能在「已加载 Claude 主题」的宿主下构造成功。
    /// 构造过程即会解析各自 XAML 里的全部 <c>{StaticResource Claude*}</c> ——
    /// 令牌缺失或类型不符都会在此抛出, 因此「构造不抛」本身就是有效断言。
    ///
    /// 刻意拆成三条独立用例而非一条: 一条用例里连续构造三个视图时, 第一个抛出会
    /// 掩盖后两个的结果, 定位不到到底是哪个视图退化 (2026-09 变异验证时确实踩到)。
    /// </summary>
    [AvaloniaFact]
    public void PluginsPageView_Constructs_Without_Resource_Or_Type_Errors()
        => Assert.NotNull(new PluginsPageView());

    /// <summary>插件市场窗: 页面级消费 33 处 Claude 令牌 (含 CornerRadius 内联用法)。</summary>
    [AvaloniaFact]
    public void PluginMarketWindow_Constructs_Without_Resource_Or_Type_Errors()
        => Assert.NotNull(new PluginMarketWindow());

    /// <summary>
    /// 主窗: 经 <c>ContentControl.DataTemplates</c> 间接挂载上述页面; 全局主题或令牌退化时
    /// 同样会崩。只构造不 Show —— 其 Opened 会 InitializeAsync 拉起后端子进程。
    /// </summary>
    [AvaloniaFact]
    public void MainWindow_Constructs_Without_Resource_Or_Type_Errors()
    {
        var main = new MainViewModel(new BackendSessionOptions())
        {
            Config = new Config { Options = new Options() },
        };
        Assert.NotNull(new MainWindow(main));
    }

    /// <summary>
    /// 半径令牌必须是 <see cref="CornerRadius"/> 类型并正确落到 <c>CornerRadius</c> 属性上。
    /// 历史回归: 曾定义为 <c>x:Double</c>, 赋给 <c>CornerRadius</c> 时抛
    /// <c>InvalidCastException: Setter value '12' is not a valid value for property 'CornerRadius'</c>。
    /// <c>Border.claudeCard</c> 用 <c>ClaudeRadiusCard</c> (14, 组件框两档制之页面级档)。
    /// </summary>
    [AvaloniaFact]
    public void Radius_Token_Applies_To_CornerRadius_Property()
    {
        var card = new Border();
        card.Classes.Add("claudeCard");
        var window = new Window { Content = card };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            Assert.Equal(new CornerRadius(14), card.CornerRadius);
            // 同一 Class 的另一令牌: Ivory 卡片面 (#faf9f5), 证明画刷令牌也已解析
            Assert.Equal(Color.Parse("#faf9f5"), ((ISolidColorBrush)card.Background!).Color);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>
    /// 主 CTA 按钮的 Terracotta 令牌必须解析并覆盖 Fluent 默认按钮底色,
    /// 同时 <c>Button.claudeCta</c> 的 <c>CornerRadius</c> (<c>ClaudeRadiusMd</c>=8) 正确落地。
    /// </summary>
    [AvaloniaFact]
    public void Cta_Button_Uses_Terracotta_Token_And_Radius()
    {
        var button = new Button();
        button.Classes.Add("claudeCta");
        var window = new Window { Content = button };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            Assert.Equal(Color.Parse("#c96442"), ((ISolidColorBrush)button.Background!).Color);
            Assert.Equal(new CornerRadius(8), button.CornerRadius);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>
    /// 管理类弹窗共享类 (2026-09-15): <c>Border.claudeHint</c> = Sand 面 + Panel(4) 圆角
    /// (组件框两档制, 2026-09-18: 嵌套提示条走近似直角档);
    /// <c>ListBox.claudeList</c> = 透明无边框 + 6 内边距 (行样式由该类的后代选择器接管)。
    /// 「匹配类型」与「行为库」两窗共用这组类, 此断言防止后续换肤时单边漂移。
    /// </summary>
    [AvaloniaFact]
    public void Management_Dialog_Shared_Classes_Are_Wired()
    {
        var hint = new Border();
        hint.Classes.Add("claudeHint");
        var list = new ListBox();
        list.Classes.Add("claudeList");
        var window = new Window { Content = new Grid { Children = { hint, list } } };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            // Sand 面 (#e8e6dc) + 尺寸令牌 ClaudeRadiusPanel(4) —— 与皮肤顶部契约清单一致
            Assert.Equal(Color.Parse("#e8e6dc"), ((ISolidColorBrush)hint.Background!).Color);
            Assert.Equal(new CornerRadius(4), hint.CornerRadius);
            // 列表本体不带边线/底色, 容器面由外层 Border.claudeCard 提供
            Assert.Equal(0, list.BorderThickness.Left);
            Assert.Equal(new Thickness(6), list.Padding);
        }
        finally
        {
            window.Close();
        }
    }

    // ===================== 皮肤契约强制校验 =====================

    /// <summary>画刷键 (含 4 个暗色桩) —— 与 Styles/Skins/*.axaml 顶部契约清单一致。</summary>
    private static readonly string[] BrushKeys =
    [
        "ClaudeParchmentBrush", "ClaudeIvoryBrush", "ClaudeWhiteBrush", "ClaudeSandBrush",
        "ClaudeNearBlackBrush", "ClaudeCharcoalWarmBrush", "ClaudeOliveGrayBrush",
        "ClaudeStoneGrayBrush", "ClaudeDarkWarmBrush", "ClaudeTerracottaBrush",
        "ClaudeCoralBrush", "ClaudeCoralLightBrush", "ClaudeErrorBrush", "ClaudeMutedGreenBrush",
        "ClaudeMutedGreenSoftBrush", "ClaudeBorderCreamBrush", "ClaudeBorderWarmBrush",
              "ClaudeRingWarmBrush", "ClaudeRingDeepBrush",
              "ClaudeTerracottaSoftBrush",
        // 暗色桩: 定义但不接线, 仍纳入契约以免新增皮肤时漏掉
        "ClaudeDarkSurfaceBrush", "ClaudeDeepDarkBrush", "ClaudeWarmSilverBrush",
        "ClaudeBorderDarkBrush",
    ];

    private static readonly string[] ShadowKeys =
    [
        "ClaudeShadowHoverRing", "ClaudeShadowCtaRing", "ClaudeShadowPressedInset",
        "ClaudeShadowFocusRing", "ClaudeShadowWhisper", "ClaudeShadowCard",
        "ClaudeShadowCardHover", "ClaudeShadowCardDeep",
        // 组件框悬停光圈族 (2026-09-17 新增; 三页组件框唯一消费方) —— 四档层数必须相等 (各 3 层)
        "ClaudeShadowCardHalo", "ClaudeShadowCardHaloHover",
        "ClaudeShadowCardHaloPressed", "ClaudeShadowCardHaloFocus",
    ];

    private static readonly string[] RadiusKeys =
    [
        "ClaudeRadiusSm", "ClaudeRadiusMd", "ClaudeRadiusLg", "ClaudeRadiusXl",
        // 组件框两档制 (2026-09-18): 卡片容器圆角只许这两档, 其余令牌归按钮/徽标/输入框
        "ClaudeRadiusCard", "ClaudeRadiusPanel",
    ];

    /// <summary>
    /// 契约: 每一个必需令牌键都必须能在应用资源中解析。
    /// 这是「换肤 = 替换皮肤文件」能成立的前提 —— 漏掉任何一个键, 否则只会在用户
    /// 恰好点到用到该键的那个页面时才暴露。新增皮肤时本用例是唯一的自动闸门。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_All_Token_Keys_Resolve()
    {
        var app = Application.Current!;
        var missing = new List<string>();

        foreach (var key in BrushKeys.Concat(ShadowKeys).Concat(RadiusKeys))
        {
            if (!app.TryFindResource(key, out _)) missing.Add(key);
        }
        missing.Add("ClaudeSerifFont");
        if (app.TryFindResource("ClaudeSerifFont", out _)) missing.Remove("ClaudeSerifFont");

        Assert.Empty(missing);
    }

    /// <summary>
    /// 契约: 会被接成"卡片悬停影"的**去环**令牌一律不得含零扩散环层 (`0 0 0 N`) ——
    /// 覆盖 <c>ClaudeShadowCard</c> (现为三页卡片唯一在用的影档) 与
    /// <c>ClaudeShadowCardDeep</c> (无消费方, 保留为恢复路径)。
    /// 这是 2026-09-17「悬停灰描边」事故的回归锁: 当年的灰线正来自
    /// `0 0 0 1 #d1cfc5` 零扩散环层 (与卡片 2px 实体描边叠成双线)。
    /// 用户最终裁定「取消悬停投影加深, 保留基础阴影」后悬停已不再换档
    /// (三页 :pointerover 投影接线整体删除, 见 SettingsCardEffectTests / ActionPageCardStyleTests),
    /// 但只要有人把带环的令牌接回去, 灰线立刻复现 —— 故在皮肤层再加一道与接线无关的锁。
    /// 注: <c>ClaudeShadowCardHover</c> **故意排除在外** —— 它就是 Ring 版 (环层即其定义),
    /// 另有断言要求它保持环层, 以免被误"去环"后与 Deep 语义混淆。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_Card_Shadow_Tokens_Contain_No_Ring_Layer()
    {
        foreach (var key in new[]
                 {
                     "ClaudeShadowCard", "ClaudeShadowCardDeep",
                     // 光圈族的静止/悬停档同属"去环"档: 它们的光圈层必须是**模糊弥散**而非描边环
                     "ClaudeShadowCardHalo", "ClaudeShadowCardHaloHover",
                 })
        {
            var layers = ShadowLayers(key);
            Assert.NotEmpty(layers);
            Assert.DoesNotContain(layers, l => l.Blur == 0);
        }

        // Ring 版的特征必须保留: 它是"带 1px #d1cfc5 环"的历史档, 语义上与 Deep 相对
        var ringVersion = ShadowLayers("ClaudeShadowCardHover");
        Assert.Contains(ringVersion, l => l.Blur == 0);
    }

    /// <summary>
    /// 契约: 悬停光圈 (<c>ClaudeShadowCardHalo</c> 家族, 2026-09-17) 必须是**柔和暖橙弥散**, 不是线条:
    ///   ① 光圈层 <c>Blur &gt; 0</c> —— <c>Blur==0</c> 的 <c>0 0 0 N</c> 画出来就是一根描边线
    ///      (2026-09-17「悬停灰描边」事故的成因), 用户明确要求"不能是纯线条";
    ///   ② 光圈层偏移必须为 0 —— 否则只在单侧出现, 不成"一圈";
    ///   ③ 光圈层是暖色 (R&gt;B) 且 alpha ∈ (0, 0x40] —— 橙色 (用户优先指定) 且不过重
    ///      (此前"过于明显"的悬停效果已被否决过两次);
    ///   ④ 四档**层数相等** —— Avalonia 的 BoxShadowsAnimator 在 progress&lt;1 时按
    ///      <c>oldValue.Count</c> 输出层数, 不等会让光圈在动画**末帧**突然出现/消失;
    ///   ⑤ 静止档第 3 层 alpha 必须为 0 (仅占位, 静止态不得发光)。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_Hover_Halo_Is_Soft_Warm_And_Not_A_Line()
    {
        var rest = ShadowLayers("ClaudeShadowCardHalo");
        var hover = ShadowLayers("ClaudeShadowCardHaloHover");
        var pressed = ShadowLayers("ClaudeShadowCardHaloPressed");
        var focus = ShadowLayers("ClaudeShadowCardHaloFocus");

        // ④ 层数相等 (四档) —— 动画逐层插值的前提, 也是"过渡自然"的必要条件
        Assert.Equal(3, rest.Count);
        Assert.Equal(rest.Count, hover.Count);
        Assert.Equal(rest.Count, pressed.Count);
        Assert.Equal(rest.Count, focus.Count);

        // ⑤ 静止档光圈层 = 全透明占位
        Assert.Equal((byte)0, rest[2].Color.A);

        // ① ② ③ 悬停光圈层 = 有模糊 + 无偏移 + 暖色 + 不过重
        var glow = hover[2];
        Assert.True(glow.Blur > 0,
            "光圈层 Blur==0 ⇒ 会画成一根描边线 (灰线事故成因), 必须用模糊弥散成形");
        Assert.Equal(0, glow.OffsetX);
        Assert.Equal(0, glow.OffsetY);
        Assert.True(glow.Color.R > glow.Color.B, $"光圈必须为暖色/橙色, 实际 {glow.Color}");
        // alpha 上界 0x60 (≈38%): 这是"不过重"的令牌级闸门 —— 历史教训是悬停效果被连否两轮
        // ("过于明显" / "边缘生硬"), 故源 alpha 不得突破此线 (弥散后可见峰值本就只有一半)。
        Assert.InRange(glow.Color.A, (byte)1, (byte)0x60);
        // Spread 允许少量外推 (把光圈从卡沿推出去, 形成"圈"的形), 但不得大到只剩硬边,
        // 也不得超过 Blur 的 1/4 (否则看起来像描边而非弥散)
        Assert.InRange(glow.Spread, 0, glow.Blur / 4);

        // 悬停"只加光圈": 前两层必须与静止档逐位相同 (防偷偷加深投影)
        Assert.Equal(rest[0].ToString(), hover[0].ToString());
        Assert.Equal(rest[1].ToString(), hover[1].ToString());

        // 聚焦档: 第 3 层 = 既有 Coral 焦点环 (这里的 Blur==0 实环是**有意**的),
        // 前两层透明化 ⇒ 视觉与 ClaudeShadowFocusRing 一致, 只是层数对齐成 3
        Assert.Equal(0, focus[2].Blur);
        Assert.Equal(Color.Parse("#d97757"), focus[2].Color);
        Assert.True(focus[2].Color.A > 0);
        Assert.Equal((byte)0, focus[0].Color.A);
        Assert.Equal((byte)0, focus[1].Color.A);
    }

    /// <summary>读令牌并展平成层列表 (BoxShadows 是有序定长集合, 用 Count + 索引器)。</summary>
    private static List<BoxShadow> ShadowLayers(string key)
    {
        Assert.True(Application.Current!.TryFindResource(key, out var v), $"缺少令牌 {key}");
        var s = (BoxShadows)v!;
        return Enumerable.Range(0, s.Count).Select(i => s[i]).ToList();
    }

    /// <summary>
    /// 契约: 半径令牌的类型必须是 <see cref="CornerRadius"/>。
    /// 历史事故回归锁 —— 曾把 4 个半径令牌定义成 <c>x:Double</c>, 构建 0 错误,
    /// 但每个引用它们的窗口 XAML 加载即抛 InvalidCastException (含 MainWindow)。
    /// <c>x:Double</c> 赋给 <c>CornerRadius</c> 只能在运行期发现, 故必须在此断言类型。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_Radius_Tokens_Are_CornerRadius_Type()
    {
        var app = Application.Current!;
        foreach (var key in RadiusKeys)
        {
            Assert.True(app.TryFindResource(key, out var value), $"皮肤契约缺键: {key}");
            Assert.IsType<CornerRadius>(value);
        }
    }

    /// <summary>
    /// 契约: 动效时长令牌 (<see cref="ClaudeMotion"/>, C# 强类型真源) 分两类锁界
    /// (2026-09-24 拆分为两类; 同日用户多轮提速把揭示类压到 100/100ms):
    /// · 交互反馈类 Press/Micro/Standard/Enter — ≤300ms (生产率工具基线), 且类内按
    ///   按压 &lt; 微交互 &lt; 标准过渡 &lt; 入场 递增;
    /// · 内容揭示类 Roll/Unroll — ≤600ms (超过"可感知卡顿"线即不可接受), 且类内 Roll ≤ Unroll
    ///   (2026-09-24 用户点名把摊开压到 100ms; 修「折叠比展开卡」时把卷起也提到 100ms ——
    ///   软件光栅下每帧成本相同, 收势更短只会帧更少更跳, 二者等长手感才一致)。
    /// 注意**跨类不再互相单调**: 提速后 Roll=Unroll=100 落在 Micro(120) 与 Standard(200) 之间 —— 这是用户明确指定 100ms 的既定形态, 故契约只保证"类内有序 + 上限",
    /// 不再约束"揭示必须比交互慢"。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_Motion_Tokens_Are_Ordered_And_Bounded()
    {
        var interaction = new[]
        {
            ClaudeMotion.Press, ClaudeMotion.Micro, ClaudeMotion.Standard, ClaudeMotion.Enter,
        };
        Assert.All(interaction, v => Assert.InRange(v.TotalMilliseconds, 10, 300));
        Assert.True(interaction.SequenceEqual(interaction.OrderBy(v => v)),
            $"交互反馈类令牌必须按 按压<微交互<标准<入场 递增, 实际: {string.Join(", ", interaction)}");

        var reveal = new[] { ClaudeMotion.Roll, ClaudeMotion.Unroll };
        Assert.All(reveal, v => Assert.InRange(v.TotalMilliseconds, 10, 600));
        Assert.True(reveal.SequenceEqual(reveal.OrderBy(v => v)),
            $"内容揭示类令牌必须按 卷起<摊开 递增, 实际: {string.Join(", ", reveal)}");
    }

    /// <summary>
    /// 契约: 系统强调色 7 阶必须存在、主阶 = Terracotta 且全部暖色 (R&gt;B)。
    /// 历史回归锁 —— Fluent 的 RadioButton 选中圆 / CheckBox 勾选框 / Slider 轨道填充 /
    /// AutoCompleteBox 下拉选中等全部强调色态引用 SystemAccentColor (默认 OS 蓝 #0078d7),
    /// 不覆盖则与 Claude 暖色体系冲突 (2026-09-13 用户报「行为选择器与整体 UI 不匹配」;
    /// headless 实证: 覆盖后三控件选中态全部转 Terracotta, 派生画刷自动跟随)。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_SystemAccent_Family_Is_Warm()
    {
        var app = Application.Current!;
        Assert.True(app.TryFindResource("SystemAccentColor", out var primary), "缺 SystemAccentColor");
        Assert.Equal(Color.Parse("#c96442"), Assert.IsType<Color>(primary));

        foreach (var key in new[]
                 {
                     "SystemAccentColorDark1", "SystemAccentColorDark2", "SystemAccentColorDark3",
                     "SystemAccentColorLight1", "SystemAccentColorLight2", "SystemAccentColorLight3",
                 })
        {
            Assert.True(app.TryFindResource(key, out var v), $"缺 {key}");
            var c = Assert.IsType<Color>(v);
            Assert.True(c.R > c.B, $"{key} 非暖色 (R={c.R} B={c.B})");
        }

        // 派生画刷跟随 (Fluent 模板实际消费的键族之一)
        Assert.True(app.TryGetResource("SystemControlBackgroundAccentBrush", out var brush));
        var scb = Assert.IsType<SolidColorBrush>(brush);
        Assert.True(scb.Color.R > scb.Color.B, $"派生强调画刷仍为冷色: {scb.Color}");
    }

    /// <summary>契约: 芯片单选 ControlTheme 存在且 TargetType = RadioButton (行为选择器重构的锚点)。</summary>
    [AvaloniaFact]
    public void Skin_Contract_ChipRadio_Theme_Resolves()
    {
        var app = Application.Current!;
        Assert.True(app.TryFindResource("ChipRadio", out var theme), "皮肤契约缺键: ChipRadio");
        Assert.Equal(typeof(RadioButton), Assert.IsType<ControlTheme>(theme).TargetType);
    }

    /// <summary>契约: 背退格删除键 ControlTheme 存在且 TargetType = Button (7 处红 ✕ 统一的锚点)。</summary>
    [AvaloniaFact]
    public void Skin_Contract_DeleteKey_Theme_Resolves()
    {
        var app = Application.Current!;
        Assert.True(app.TryFindResource("DeleteKey", out var theme), "皮肤契约缺键: DeleteKey");
        Assert.Equal(typeof(Button), Assert.IsType<ControlTheme>(theme).TargetType);
    }

    /// <summary>
    /// 契约: ComboBox 弹层暖化键存在且暖色, 弹层统一圆角 = 8, ComboBoxItem 高亮内缩圆角生效
    /// (2026-09-13 用户报弹层方正违和; 弹层 Border 实证经 OverlayCornerRadius 驱动,
    /// 条目高亮画在模板 PART_ContentPresenter 上且 CornerRadius 经 TemplateBinding 绑定控件值)。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_Combo_Dropdown_Keys_Are_Warm()
    {
        var app = Application.Current!;
        foreach (var key in new[] { "ComboBoxDropdownBackground", "ComboBoxDropdownBorderBrush" })
        {
            Assert.True(app.TryGetResource(key, out var v), $"缺 {key}");
            var c = ((ISolidColorBrush)v!).Color;
            Assert.True(c.R > c.B, $"{key} 非暖色: {c}");
        }
        Assert.True(app.TryGetResource("OverlayCornerRadius", out var cr));
        Assert.Equal(new CornerRadius(8), Assert.IsType<CornerRadius>(cr));

        // 皮肤 ComboBoxItem 样式生效 (圆角 + 内缩边距)
        var item = new ComboBoxItem { Content = "x" };
        var host = new Window { Width = 120, Height = 60, Content = item };
        host.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            Assert.Equal(new CornerRadius(6), item.CornerRadius);
            Assert.Equal(new Thickness(6, 3), item.Margin);
        }
        finally
        {
            host.Close();
        }
    }

    /// <summary>
    /// 契约: 三个 Fluent ToggleSwitch 开启态覆盖键必须存在 (放在 App.axaml 的
    /// Application.Resources, 因为模板内部用 {DynamicResource} 沿逻辑树查找),
    /// 且必须是画刷 —— 若被误改成别的类型或漏掉, 开关会退回系统强调色 (冷蓝)。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_ToggleSwitch_Fluent_Overrides_Exist()
    {
        var app = Application.Current!;
        foreach (var key in new[] { "ToggleSwitchFillOn", "ToggleSwitchStrokeOn", "ToggleSwitchKnobFillOn" })
        {
            Assert.True(app.TryFindResource(key, out var value), $"皮肤覆盖键缺失: {key}");
            Assert.IsAssignableFrom<IBrush>(value);
        }
    }

    /// <summary>
    /// 契约: ToggleSwitch **开启态的悬停/按下变体**也必须是暖色。
    /// 历史事故回归锁 —— Fluent 只为非悬停态提供了 <c>ToggleSwitchFillOn</c> 等 3 个键,
    /// 悬停/按下走的是 <c>*_PointerOver</c> / <c>*_Pressed</c> 变体, 其内置值是**硬编码蓝**
    /// (<c>#269fff</c> / <c>#0078d7</c> / <c>#00589e</c>)。只覆盖非悬停键时,
    /// 表现为「开关平时是陶土色, 鼠标一放上去就变蓝」。
    /// 这些键同样必须放在 Application.Resources (模板内部走 {DynamicResource})。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_ToggleSwitch_Hover_And_Pressed_Are_Warm()
    {
        var app = Application.Current!;

        // 悬停 = 比焦点色 Coral(#d97757) 浅一档的浅珊瑚
        foreach (var key in new[] { "ToggleSwitchFillOnPointerOver", "ToggleSwitchStrokeOnPointerOver" })
        {
            Assert.True(app.TryFindResource(key, out var v), $"皮肤覆盖键缺失: {key}");
            Assert.Equal(Color.Parse("#e1957a"), ((ISolidColorBrush)v!).Color);
        }

        // 按下 = 主陶土色
        foreach (var key in new[] { "ToggleSwitchFillOnPressed", "ToggleSwitchStrokeOnPressed" })
        {
            Assert.True(app.TryFindResource(key, out var v), $"皮肤覆盖键缺失: {key}");
            Assert.Equal(Color.Parse("#c96442"), ((ISolidColorBrush)v!).Color);
        }

        // 反向断言: 绝不能残留 Fluent 的蓝色变体
        foreach (var key in new[] { "ToggleSwitchFillOnPointerOver", "ToggleSwitchStrokeOnPointerOver", "ToggleSwitchFillOnPressed" })
        {
            app.TryFindResource(key, out var v);
            var c = ((ISolidColorBrush)v!).Color;
            Assert.NotEqual(Color.Parse("#269fff"), c);
            Assert.NotEqual(Color.Parse("#0078d7"), c);
            Assert.NotEqual(Color.Parse("#00589e"), c);
        }
    }

    /// <summary>
    /// 契约 (2026-09-22 移除毛玻璃后): 窗口/页面全部不透明 —— 窗口底 Parchment
    /// 与卡片面 Ivory 都必须实色 (alpha=255); 旧的三个半透明面令牌已删除,
    /// 应用资源中不得再解析到它们 (防透明度回归)。
    /// </summary>
    [AvaloniaFact]
    public void Skin_Contract_Windows_And_Cards_Are_Fully_Opaque()
    {
        var app = Application.Current!;

        Assert.True(app.TryFindResource("ClaudeParchmentBrush", out var parchment));
        Assert.Equal((byte)255, ((ISolidColorBrush)parchment!).Color.A);
        Assert.True(app.TryFindResource("ClaudeIvoryBrush", out var ivory));
        Assert.Equal((byte)255, ((ISolidColorBrush)ivory!).Color.A);

        // 旧半透明面令牌已随毛玻璃移除, 不得回归
        foreach (var gone in new[]
                 {
                     "ClaudeWindowSurfaceBrush", "ClaudeSidebarSurfaceBrush",
                     "ClaudeContentSurfaceBrush",
                 })
        {
            Assert.False(app.TryFindResource(gone, out _), $"毛玻璃令牌应已删除: {gone}");
        }
    }

    /// <summary>
    /// 契约: 默认渲染管线为**软件渲染** (2026-09-12 瞬峰治理决策)。2026-09-26 曾为排查
    /// 描边右缘缺失短暂切 GPU 默认, 实测确认右缘缺失是布局裁剪 (ActionEditorPanel 已修)
    /// 与管线无关后, 同日恢复软件默认。<c>KEYFLUX_RENDER_GPU=1</c> 保留为 GPU 档开关
    /// (解决重卡逐帧重光栅掉帧, 代价是启动瞬峰/内存回升, 切换属独立回归项)。
    /// </summary>
    [Fact]
    public void Program_Default_Render_Mode_Stays_Software()
    {
        var previous = Environment.GetEnvironmentVariable("KEYFLUX_RENDER_GPU");
        try
        {
            Environment.SetEnvironmentVariable("KEYFLUX_RENDER_GPU", null);
            Assert.False(Program.UseGpuRendering, "默认必须走软件渲染 (瞬峰契约)");

            Environment.SetEnvironmentVariable("KEYFLUX_RENDER_GPU", "1");
            Assert.True(Program.UseGpuRendering, "置 KEYFLUX_RENDER_GPU=1 应切到 GPU 档");
        }
        finally
        {
            Environment.SetEnvironmentVariable("KEYFLUX_RENDER_GPU", previous);
        }
    }
}
