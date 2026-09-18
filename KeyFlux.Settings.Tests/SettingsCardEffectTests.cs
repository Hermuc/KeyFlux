using System.Linq;
using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;
using Xunit.Abstractions;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 设置页组件框三态守护 —— **以插件页 Border.pluginCard 为基准** (2026-09-15 用户要求
/// 两页组件框描边线条视觉一致, 插件页为准):
/// ① 静止态: **2px** 奶油边框 + ClaudeShadowCardHalo (原 ClaudeShadowCard 两层投影 +
///    第 3 层 alpha=0 的光圈占位, 视觉与旧档逐位相同);
/// ② 悬停态: **边框色不变 (无灰线)、投影前两层不变, 只点亮第 3 层的陶土色弥散光圈** ——
///    2026-09-17 用户先裁定「取消悬停投影加深, 保留基础阴影」, 当日又要求「悬停时组件框周围
///    显示一圈**光圈**, 不能是纯线条」⇒ 落地为 ClaudeShadowCardHaloHover (Blur=16 弥散,
///    非零扩散描边), 描边与投影均不动;
/// ③ 卡内控件获焦 → ComponentFocusRing 挂 .ring, 边框色仍不变, 阴影换 ClaudeShadowCardHaloFocus
///    (视觉 == ClaudeShadowFocusRing 的 Coral 2px 实环, 只是层数对齐成 3 以便平滑过渡)。
/// 断言读 BorderBrush/BorderThickness/BoxShadow 的生效值 (样式优先级已折算)。
/// 另含跨页一致性测试: Settings 卡与插件页卡的描边配方逐项相同。
///
/// ⚠ headless 读"终点态"前会先摘掉卡片的 BoxShadow 过渡 (见 DetachShadowTransition) ——
/// 动画时钟不由 RunJobs 推进 (MotionSmokeTests 亦记录「动画本身不在 headless 断言」),
/// 带过渡读到的会是插值中间值, 结果随真实耗时抖动。过渡接线由独立断言守护。
/// </summary>
[Collection("I18nSerial")]
public sealed class SettingsCardEffectTests
{
    private readonly ITestOutputHelper _output;

    public SettingsCardEffectTests(ITestOutputHelper output) => _output = output;
    [AvaloniaFact]
    public void SettingsCard_States_Switch_Border_And_Effect()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var view = new SettingsPageView { DataContext = new SettingsPageViewModel(main) };
        var window = new Window { Width = 1500, Height = 950, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            var cards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("settingsCard")).ToList();
            Assert.True(cards.Count > 0,
                $"未找到 settingsCard; 全部 Border 类: " +
                string.Join(" | ", view.GetVisualDescendants().OfType<Border>()
                    .Select(b => string.Join("+", b.Classes)).Distinct()));
            var card = cards[0];
            DetachShadowTransition(card);

            var creamObj = Application.Current!.TryGetResource("ClaudeBorderCreamBrush", out var o1)
                ? (SolidColorBrush)o1! : null;
            var restBrush = Assert.IsType<SolidColorBrush>(card.BorderBrush);
            var restShadow = (BoxShadows)view.FindResource("ClaudeShadowCardHalo")!;
            var hoverShadow = (BoxShadows)view.FindResource("ClaudeShadowCardHaloHover")!;

            // ① 静止态: 2px 奶油边框 + ClaudeShadowCardHalo (两层投影 + alpha=0 光圈占位)
            //    (2026-09-16 用户反馈「两页描边都有点细」⇒ 1px -> 2px, 与插件页同步)
            Assert.Equal(creamObj!.Color, restBrush.Color);
            Assert.Equal(2, card.BorderThickness.Left);
            Assert.Equal(restShadow.ToString(), card.BoxShadow.ToString());

            // ② 悬停: headless 鼠标移到卡片中心 → :pointerover
            //    **边框色不变 (无灰线) + 投影前两层不变 + 第 3 层光圈点亮** —— 2026-09-17 用户裁定
            //    「取消悬停投影加深, 保留基础阴影」后, 当日又要求「悬停时周围显示一圈光圈,
            //    不能是纯线条」⇒ 悬停档 = 静止两层原样 + 陶土色弥散层 (Blur=16, 非描边环)。
            var pt = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), window)!.Value;
            window.MouseMove(pt);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsPointerOver, "悬停应命中卡片");
            var hoverBrush = (ISolidColorBrush)card.BorderBrush!;
            Assert.Equal(creamObj.Color, hoverBrush.Color);
            Assert.Equal(hoverShadow.ToString(), card.BoxShadow.ToString());
            // 悬停档必须与静止档**前两层逐位相同** (只多出光圈), 否则等于又偷偷加深了投影
            Assert.Equal(restShadow[0].ToString(), hoverShadow[0].ToString());
            Assert.Equal(restShadow[1].ToString(), hoverShadow[1].ToString());

            // ③ 单击卡内开关 (ToggleSwitch 获焦) → .ring 焦点环
            //    边框色仍不变; 阴影换 ClaudeShadowCardHaloFocus (视觉 == Coral 2px 焦点环)
            var toggle = card.GetVisualDescendants().OfType<ToggleSwitch>().First();
            var tp = Avalonia.VisualExtensions.TranslatePoint(
                toggle, new Point(toggle.Bounds.Width / 2, toggle.Bounds.Height / 2), window)!.Value;
            window.MouseDown(tp, MouseButton.Left, RawInputModifiers.None);
            window.MouseUp(tp, MouseButton.Left, RawInputModifiers.None);
            Dispatcher.UIThread.RunJobs();
            var focusBrush = (ISolidColorBrush)card.BorderBrush!;
            Assert.Equal(creamObj.Color, focusBrush.Color);
            var ring = (BoxShadows)view.FindResource("ClaudeShadowCardHaloFocus")!;
            Assert.Equal(ring.ToString(), card.BoxShadow.ToString());
            Assert.True(toggle.IsFocused || card.IsFocused);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>
    /// 跨页一致性: Settings 页组件框 (settingsCard / leftPanel) 与插件页组件框 (pluginCard)
    /// 的**描边配方逐项相同** —— 边框色 / 粗细 / 圆角 / 静止阴影 全等 (2026-09-15 用户要求)。
    /// </summary>
    [AvaloniaFact]
    public void Settings_Cards_Match_Plugins_Card_Border_Recipe()
    {
        // ---- 插件页 (基准) ----
        var pmain = new MainViewModel(new BackendSessionOptions());
        pmain.Config = new Config
        {
            Options = new Options { QuickSwitch = new QuickSwitchOption() },
        };
        var pview = new PluginsPageView { DataContext = new PluginsPageViewModel(pmain) };
        var pwin = new Window { Width = 1200, Height = 760, Content = pview };
        pwin.Show();
        Dispatcher.UIThread.RunJobs();

        // ---- Settings 页 ----
        var smain = new MainViewModel(new BackendSessionOptions());
        smain.Config = ConfigReadDefaults.Apply(new Config());
        smain.Config.Keymaps.Add(new Keymap { Id = 5, Name = "J 模式", Hotkey = "*j", ParentId = 0 });
        var sview = new SettingsPageView { DataContext = new SettingsPageViewModel(smain) };
        var swin = new Window { Width = 1500, Height = 950, Content = sview };
        swin.Show();
        Dispatcher.UIThread.RunJobs();

        try
        {
            var pluginCard = Assert.Single(
                pview.GetVisualDescendants().OfType<Border>(),
                b => b.Classes.Contains("pluginCard"));
            var settingsCards = sview.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("settingsCard") || b.Classes.Contains("leftPanel")).ToList();
            Assert.True(settingsCards.Count > 0, "未找到 Settings 页组件框");

            foreach (var card in settingsCards)
            {
                Assert.Equal(pluginCard.BorderThickness, card.BorderThickness);
                Assert.Equal(((ISolidColorBrush)pluginCard.BorderBrush!).Color,
                             ((ISolidColorBrush)card.BorderBrush!).Color);
                Assert.Equal(pluginCard.BoxShadow.ToString(), card.BoxShadow.ToString());
                Assert.Equal(pluginCard.CornerRadius, card.CornerRadius);
            }
        }
        finally
        {
            pwin.Close();
            swin.Close();
        }
    }

    /// <summary>
    /// 跨页悬停守护 (2026-09-17 用户裁定「取消悬停投影加深」后新增「悬停光圈」):
    /// 插件页 pluginCard 与设置页 settingsCard / leftPanel 悬停时都必须
    /// **描边色不变 (无灰线) + 阴影换 ClaudeShadowCardHaloHover (只点亮第 3 层光圈)** ——
    /// 任一页漂回带 #d1cfc5 零扩散灰环的 ClaudeShadowCardHover, 或又给 :pointerover
    /// 加回"加深投影"档, 本项即红。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Shows_Halo_Without_Gray_Ring_On_Both_Pages()
    {
        var pmain = new MainViewModel(new BackendSessionOptions());
        pmain.Config = new Config
        {
            Options = new Options { QuickSwitch = new QuickSwitchOption() },
        };
        var pview = new PluginsPageView { DataContext = new PluginsPageViewModel(pmain) };
        var pwin = new Window { Width = 1200, Height = 760, Content = pview };
        pwin.Show();
        Dispatcher.UIThread.RunJobs();

        var smain = new MainViewModel(new BackendSessionOptions());
        smain.Config = ConfigReadDefaults.Apply(new Config());
        var sview = new SettingsPageView { DataContext = new SettingsPageViewModel(smain) };
        var swin = new Window { Width = 1500, Height = 950, Content = sview };
        swin.Show();
        Dispatcher.UIThread.RunJobs();

        try
        {
            var rest = (BoxShadows)sview.FindResource("ClaudeShadowCardHalo")!;
            var hover = (BoxShadows)sview.FindResource("ClaudeShadowCardHaloHover")!;

            var pluginCard = Assert.Single(
                pview.GetVisualDescendants().OfType<Border>(),
                b => b.Classes.Contains("pluginCard"));
            AssertHoverShowsHalo(pwin, pluginCard, rest, hover);

            // 只悬停**可见且已布局**的组件框: 折叠分区内的卡 Bounds=0/IsVisible=False, 本就无法悬停
            // (其静止配方仍由 Settings_Cards_Match_Plugins_Card_Border_Recipe 用全集守护)
            var settingsCards = sview.GetVisualDescendants().OfType<Border>()
                .Where(b => (b.Classes.Contains("settingsCard") || b.Classes.Contains("leftPanel"))
                            && b.IsEffectivelyVisible && b.Bounds.Width > 0 && b.Bounds.Height > 0)
                .ToList();
            Assert.True(settingsCards.Count > 0, "未找到可见的 Settings 页组件框");
            foreach (var c in settingsCards)
            {
                AssertHoverShowsHalo(swin, c, rest, hover);
            }
        }
        finally
        {
            pwin.Close();
            swin.Close();
        }
    }

    /// <summary>
    /// 过渡接线守护: 三页组件框必须自带 <see cref="BoxShadowsTransition"/> (皮肤令牌 120ms) ——
    /// 用户要求光圈"过渡自然", 没有这条过渡就会硬切。
    /// headless 不推动画时钟, 故此处只锁**接线**(时长必须来自 ClaudeMotion 令牌, 非散落字面量),
    /// 与 MotionSmokeTests 的约定一致。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Halo_Transition_Is_Wired_On_All_Three_Pages()
    {
        var pmain = new MainViewModel(new BackendSessionOptions());
        pmain.Config = new Config { Options = new Options { QuickSwitch = new QuickSwitchOption() } };
        var pview = new PluginsPageView { DataContext = new PluginsPageViewModel(pmain) };
        var pwin = new Window { Width = 1200, Height = 760, Content = pview };
        pwin.Show();

        var smain = new MainViewModel(new BackendSessionOptions());
        smain.Config = ConfigReadDefaults.Apply(new Config());
        var sview = new SettingsPageView { DataContext = new SettingsPageViewModel(smain) };
        var swin = new Window { Width = 1500, Height = 950, Content = sview };
        swin.Show();
        Dispatcher.UIThread.RunJobs();

        try
        {
            var cards = pview.GetVisualDescendants().OfType<Border>()
                    .Where(b => b.Classes.Contains("pluginCard"))
                .Concat(sview.GetVisualDescendants().OfType<Border>()
                    .Where(b => b.Classes.Contains("settingsCard") || b.Classes.Contains("leftPanel")))
                .ToList();
            Assert.True(cards.Count >= 3, $"应至少含插件页 1 张 + 设置页 2 类, 实得 {cards.Count}");

            foreach (var card in cards)
            {
                var t = Assert.Single(card.Transitions!.OfType<BoxShadowsTransition>());
                Assert.Equal("BoxShadow", t.Property!.Name);
                Assert.Equal(ClaudeMotion.Micro, t.Duration);
            }
        }
        finally
        {
            pwin.Close();
            swin.Close();
        }
    }

    /// <summary>
    /// 像素级证据: 悬停光圈必须**真的渲染出来**且**是暖色弥散**。对插件页最靠下的可见卡片做
    /// Skia 截帧, 比对卡下沿外侧 2..7 行:
    ///   (a) 平均亮度**下降** (陶土 #c96442 比米色底暗 ⇒ 光圈点亮必然压暗);
    ///   (b) 暖度 (R−B) **上升** ⇒ 证明落下来的是橙色, 而非黑色投影或灰色描边;
    ///   (c) 变化幅度落在 (0.004, 0.12) 区间 —— 下界防"看不见", 上界防重蹈"过于明显"覆辙。
    /// 取最靠下的卡是为了其下方无同层兄弟遮挡 (卡底边距 10px, 采样带 2..7 行落在空隙内)。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Halo_Lights_Warm_Pixels_Below_Card()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            Options = new Options { QuickSwitch = new QuickSwitchOption() },
        };
        var view = new PluginsPageView { DataContext = new PluginsPageViewModel(main) };
        var window = new Window { Width = 1200, Height = 900, Content = view, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        try
        {
            var card = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("pluginCard")
                            && b.IsEffectivelyVisible && b.Bounds.Width > 0 && b.Bounds.Height > 0)
                .OrderByDescending(b => b.Bounds.Bottom)
                .First();
            DetachShadowTransition(card);

            var bottomCenter = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height), window)!.Value;
            var x = (int)bottomCenter.X;
            var yTop = (int)bottomCenter.Y + 2; // 避开卡片自身的抗锯齿边
            var yBottom = (int)bottomCenter.Y + 7;
            Assert.True(yBottom < window.Height,
                $"采样带需落在窗口内: 卡下沿 y={bottomCenter.Y}, 窗口高={window.Height}");

            using var restFrame = window.CaptureRenderedFrame()!;
            var restLum = MeanLuminance(restFrame, x, yTop, yBottom);
            var restWarm = MeanWarmth(restFrame, x, yTop, yBottom);
            // 通道序自证: CaptureRenderedFrame 的 framebuffer 在部分 Avalonia/Skia 组合下是 RGBA 而非
            // BGRA —— 按错序读会把奶油色读成偏蓝色 (2026-09-16 曾因此误判"边框变蓝")。
            // 这里直接打印格式与原始 4 字节, 免得后人重踩。
            using (var probe = restFrame.Lock())
            {
                var one = new byte[4];
                Marshal.Copy(probe.Address + yTop * probe.RowBytes + x * 4, one, 0, 4);
                _output.WriteLine($"帧格式={probe.Format}, 静止像素原始字节={one[0]},{one[1]},{one[2]},{one[3]}");
            }

            var center = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), window)!.Value;
            window.MouseMove(center);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsPointerOver, "悬停应命中卡片");

            using var hoverFrame = window.CaptureRenderedFrame()!;
            var hoverLum = MeanLuminance(hoverFrame, x, yTop, yBottom);
            var hoverWarm = MeanWarmth(hoverFrame, x, yTop, yBottom);

            _output.WriteLine(
                $"卡下采样带 (x={x}, y={yTop}..{yBottom}, 卡底 y={bottomCenter.Y}): " +
                $"亮度 静止={restLum:F4} → 悬停={hoverLum:F4} (Δ={restLum - hoverLum:F4}); " +
                $"暖度(R-B) 静止={restWarm:F2} → 悬停={hoverWarm:F2} (Δ={hoverWarm - restWarm:F2})");

            var darken = restLum - hoverLum;
            Assert.True(darken > 0.004,
                $"悬停光圈未点亮: 卡下亮度几乎未变 (Δ={darken:F4}) —— 检查 ClaudeShadowCardHaloHover 第 3 层 alpha");
            Assert.True(darken < 0.12,
                $"悬停光圈过重 (Δ={darken:F4}) —— 用户此前已否决「过于明显」的悬停效果, 请下调光圈 alpha");
            Assert.True(hoverWarm - restWarm > 1.0,
                $"光圈不是暖色: 暖度(R-B) 仅变化 {hoverWarm - restWarm:F2} —— 光圈层必须是陶土色而非黑/灰");
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>回读单列若干行的平均相对亮度 (0=黑, 1=白); 4 字节/像素, 通道序按 framebuffer 实际格式。</summary>
    private static double MeanLuminance(WriteableBitmap frame, int x, int yTop, int yBottom)
    {
        using var fb = frame.Lock();
        var bgra = fb.Format == PixelFormat.Bgra8888;
        var buf = new byte[4];
        double sum = 0;
        for (var y = yTop; y <= yBottom; y++)
        {
            Marshal.Copy(fb.Address + y * fb.RowBytes + x * 4, buf, 0, 4);
            var (r, g, b) = bgra ? (buf[2], buf[1], buf[0]) : (buf[0], buf[1], buf[2]);
            sum += (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        }
        return sum / (yBottom - yTop + 1);
    }

    /// <summary>回读单列若干行的平均「暖度」= R − B (0..255 刻度)。暖色光圈抬高该值, 灰/黑影不会。</summary>
    private static double MeanWarmth(WriteableBitmap frame, int x, int yTop, int yBottom)
    {
        using var fb = frame.Lock();
        var bgra = fb.Format == PixelFormat.Bgra8888;
        var buf = new byte[4];
        double sum = 0;
        for (var y = yTop; y <= yBottom; y++)
        {
            Marshal.Copy(fb.Address + y * fb.RowBytes + x * 4, buf, 0, 4);
            var (r, b) = bgra ? (buf[2], buf[0]) : (buf[0], buf[2]);
            sum += r - b;
        }
        return sum / (yBottom - yTop + 1);
    }

    /// <summary>
    /// 摘掉卡片上的 BoxShadow 过渡, 使属性读回直接是**终点态**。
    /// 原因: headless 下动画时钟不由 <c>RunJobs</c> 推进 (MotionSmokeTests 已记录
    /// 「动画本身不在此断言 (headless 时钟推进不确定)」), 带过渡时读到的会是插值中间值,
    /// 断言结果随真实耗时抖动。局部值优先级高于样式 Setter, 故此处能压掉样式里的过渡。
    /// 过渡**接线**本身由 Hover_Halo_Transition_Is_Wired_On_All_Three_Pages 守护。
    /// </summary>
    private static void DetachShadowTransition(Border card) => card.Transitions = new Transitions();

    /// <summary>悬停某卡片: 命中后断言描边色不变 (无灰线) 且 BoxShadow == 悬停光圈档; 随后移出复位。</summary>
    private static void AssertHoverShowsHalo(Window win, Border card, BoxShadows rest, BoxShadows hover)
    {
        DetachShadowTransition(card);
        var restBrush = ((ISolidColorBrush)card.BorderBrush!).Color;
        var p = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), win)!.Value;
        win.MouseMove(p);
        Dispatcher.UIThread.RunJobs();

        Assert.True(card.IsPointerOver,
            $"{string.Join("+", card.Classes)} 悬停应命中 (Bounds={card.Bounds}, IsVisible={card.IsVisible}, EffVisible={card.IsEffectivelyVisible}, pt={p})");
        Assert.Equal(restBrush, ((ISolidColorBrush)card.BorderBrush!).Color);
        Assert.Equal(hover.ToString(), card.BoxShadow.ToString());
        // 只许"多一层光圈": 前两层必须与静止档逐位相同 (防偷偷加深投影)
        for (var i = 0; i < 2; i++)
        {
            Assert.Equal(rest[i].ToString(), hover[i].ToString());
        }

        win.MouseMove(new Point(1, 1));
        Dispatcher.UIThread.RunJobs();
    }
}
