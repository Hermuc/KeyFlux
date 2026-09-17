using System.Linq;
using System.Runtime.InteropServices;
using Avalonia;
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
/// ① 静止态: **2px** 奶油边框 + ClaudeShadowCard 双层下坠影
///    (2026-09-15 与插件页统一机制时曾为 1px; 2026-09-16 用户反馈两页皆偏细 ⇒ 同步加粗到 2px);
/// ② 悬停态: **边框色不变、阴影不变、无灰描边** —— 2026-09-17 用户最终裁定
///    「取消悬停投影加深, 保留基础阴影」, 三页卡片的 :pointerover 接线已整体删除
///    (历史: 先由 Ring 版 ClaudeShadowCardHover 出灰线, 改为去环版 Deep, 再经历
///     加深→过重两轮调整, 最终取消);
/// ③ 卡内控件获焦 → :focus-within 边框色仍不变, 阴影换 ClaudeShadowFocusRing (Coral 2px 环)
///    —— 与插件页 :focus-within 同款。
/// 断言读 BorderBrush/BorderThickness/BoxShadow 的生效值 (样式优先级已折算)。
/// 另含跨页一致性测试: Settings 卡与插件页卡的描边配方逐项相同。
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

            var creamObj = Application.Current!.TryGetResource("ClaudeBorderCreamBrush", out var o1)
                ? (SolidColorBrush)o1! : null;
            var restBrush = Assert.IsType<SolidColorBrush>(card.BorderBrush);
            var restShadow = (BoxShadows)view.FindResource("ClaudeShadowCard")!;

            // ① 静止态: 2px 奶油边框 + ClaudeShadowCard 双层下坠影
            //    (2026-09-16 用户反馈「两页描边都有点细」⇒ 1px -> 2px, 与插件页同步)
            Assert.Equal(creamObj!.Color, restBrush.Color);
            Assert.Equal(2, card.BorderThickness.Left);
            Assert.Equal(restShadow.ToString(), card.BoxShadow.ToString());

            // ② 悬停: headless 鼠标移到卡片中心 → :pointerover
            //    **边框色不变、阴影也不变** —— 2026-09-17 用户最终裁定「取消悬停投影加深,
            //    保留基础阴影」, 三页的 :pointerover 投影接线已删除, 悬停时回落静止配方。
            //    (历史: 曾用 Ring 版 ClaudeShadowCardHover → 出灰线; 改去环版 Deep → 加深不足;
            //     再加到双层强影 → 又过重; 故最终取消, 只保留静止档。)
            var pt = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), window)!.Value;
            window.MouseMove(pt);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsPointerOver, "悬停应命中卡片");
            var hoverBrush = (ISolidColorBrush)card.BorderBrush!;
            Assert.Equal(creamObj.Color, hoverBrush.Color);
            Assert.Equal(restShadow.ToString(), card.BoxShadow.ToString());

            // ③ 单击卡内开关 (ToggleSwitch 获焦) → :focus-within
            //    边框色仍不变; 阴影换 ClaudeShadowFocusRing (Coral 2px 环, 与插件页同款)
            var toggle = card.GetVisualDescendants().OfType<ToggleSwitch>().First();
            var tp = Avalonia.VisualExtensions.TranslatePoint(
                toggle, new Point(toggle.Bounds.Width / 2, toggle.Bounds.Height / 2), window)!.Value;
            window.MouseDown(tp, MouseButton.Left, RawInputModifiers.None);
            window.MouseUp(tp, MouseButton.Left, RawInputModifiers.None);
            Dispatcher.UIThread.RunJobs();
            var focusBrush = (ISolidColorBrush)card.BorderBrush!;
            Assert.Equal(creamObj.Color, focusBrush.Color);
            var ring = (BoxShadows)view.FindResource("ClaudeShadowFocusRing")!;
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
    /// 跨页悬停守护 (2026-09-17 用户裁定「取消悬停投影加深, 保留基础阴影」):
    /// 插件页 pluginCard 与设置页 settingsCard / leftPanel 悬停时都必须
    /// **描边色不变 + 阴影保持静止档 ClaudeShadowCard** —— 任一页漂回 Ring 版
    /// (带 #d1cfc5 灰环) 或又给 :pointerover 加回加深档即红。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Keeps_Base_Shadow_Without_Gray_Ring_On_Both_Pages()
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
            var rest = (BoxShadows)sview.FindResource("ClaudeShadowCard")!;

            var pluginCard = Assert.Single(
                pview.GetVisualDescendants().OfType<Border>(),
                b => b.Classes.Contains("pluginCard"));
            AssertHoverKeepsBaseShadow(pwin, pluginCard, rest);

            // 只悬停**可见且已布局**的组件框: 折叠分区内的卡 Bounds=0/IsVisible=False, 本就无法悬停
            // (其静止配方仍由 Settings_Cards_Match_Plugins_Card_Border_Recipe 用全集守护)
            var settingsCards = sview.GetVisualDescendants().OfType<Border>()
                .Where(b => (b.Classes.Contains("settingsCard") || b.Classes.Contains("leftPanel"))
                            && b.IsEffectivelyVisible && b.Bounds.Width > 0 && b.Bounds.Height > 0)
                .ToList();
            Assert.True(settingsCards.Count > 0, "未找到可见的 Settings 页组件框");
            foreach (var c in settingsCards)
            {
                AssertHoverKeepsBaseShadow(swin, c, rest);
            }
        }
        finally
        {
            pwin.Close();
            swin.Close();
        }
    }

    /// <summary>
    /// 像素级证据 (2026-09-17 用户裁定「取消悬停投影加深, 保留基础阴影」): 不满足于
    /// "BoxShadow 取值相等"的字符串断言, 而是**实测渲染结果** —— 对插件页 **最靠下**的可见卡片
    /// 做 Skia 截帧, 逐字节比对悬停帧与静止帧在卡片下沿外侧 2..7 行的像素, 要求**完全一致**。
    /// (历史: 本用例曾反向断言"悬停必须明显变暗, Δ亮度 ≥0.015", 见证过 加深不足→过重 两轮整改;
    ///  用户最终取消加深效果后, 断言随之反转为"零变化"。)
    /// 取最靠下的卡是为了其下方无同层兄弟遮挡 (卡底边距 10px, 采样带 2..7 行落在空隙内)。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Keeps_Base_Shadow_Rendered_Pixels_Unchanged()
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

            var bottomCenter = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height), window)!.Value;
            var x = (int)bottomCenter.X;
            var yTop = (int)bottomCenter.Y + 2; // 避开卡片自身的抗锯齿边
            var yBottom = (int)bottomCenter.Y + 7;
            Assert.True(yBottom < window.Height,
                $"采样带需落在窗口内: 卡下沿 y={bottomCenter.Y}, 窗口高={window.Height}");

            using var restFrame = window.CaptureRenderedFrame()!;
            var restLum = MeanLuminance(restFrame, x, yTop, yBottom);
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

            // 悬停帧与静止帧在采样带上必须**逐字节一致** —— 用户裁定「取消悬停投影加深, 保留基础阴影」后,
            // 悬停不得改变卡下任何像素。这比字符串断言强: 字符串只能证明 BoxShadow 取值相等,
            // 这里量的是**渲染结果**(守"接线删了、某处又悄悄加回投影/环"的回归)。
            var restBand = SampleBand(restFrame, x, yTop, yBottom);
            var hoverBand = SampleBand(hoverFrame, x, yTop, yBottom);
            _output.WriteLine(
                $"卡下采样带 (x={x}, y={yTop}..{yBottom}, 卡底 y={bottomCenter.Y}): " +
                $"静止亮度={restLum:F4} 悬停亮度={hoverLum:F4} " +
                $"像素一致={restBand.SequenceEqual(hoverBand)}");
            Assert.Equal(restBand, hoverBand);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>回读单列若干行的字节 (每像素 4 字节, 行内按 framebuffer 实际通道序)。</summary>
    private static byte[] SampleBand(WriteableBitmap frame, int x, int yTop, int yBottom)
    {
        using var fb = frame.Lock();
        var buf = new byte[(yBottom - yTop + 1) * 4];
        for (var y = yTop; y <= yBottom; y++)
        {
            Marshal.Copy(fb.Address + y * fb.RowBytes + x * 4, buf, (y - yTop) * 4, 4);
        }
        return buf;
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

    /// <summary>悬停某卡片: 命中后断言描边色不变 (无灰线) 且 BoxShadow 仍是静止档 (无加深); 随后移出复位。</summary>
    private static void AssertHoverKeepsBaseShadow(Window win, Border card, BoxShadows rest)
    {
        var restBrush = ((ISolidColorBrush)card.BorderBrush!).Color;
        var p = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), win)!.Value;
        win.MouseMove(p);
        Dispatcher.UIThread.RunJobs();

        Assert.True(card.IsPointerOver,
            $"{string.Join("+", card.Classes)} 悬停应命中 (Bounds={card.Bounds}, IsVisible={card.IsVisible}, EffVisible={card.IsEffectivelyVisible}, pt={p})");
        Assert.Equal(restBrush, ((ISolidColorBrush)card.BorderBrush!).Color);
        Assert.Equal(rest.ToString(), card.BoxShadow.ToString());

        win.MouseMove(new Point(1, 1));
        Dispatcher.UIThread.RunJobs();
    }
}
