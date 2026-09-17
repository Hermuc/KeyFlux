using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 设置页组件框三态守护 —— **以插件页 Border.pluginCard 为基准** (2026-09-15 用户要求
/// 两页组件框描边线条视觉一致, 插件页为准):
/// ① 静止态: **2px** 奶油边框 + ClaudeShadowCard 双层下坠影
///    (2026-09-15 与插件页统一机制时曾为 1px; 2026-09-16 用户反馈两页皆偏细 ⇒ 同步加粗到 2px);
/// ② 悬停态: **边框色不变、无灰描边**, 仅阴影换 ClaudeShadowCardDeep (去环版强影)
///    —— 2026-09-17 用户要求清除悬停灰描边, 三页卡片统一由 Ring 版改回去环版;
/// ③ 卡内控件获焦 → :focus-within 边框色仍不变, 阴影换 ClaudeShadowFocusRing (Coral 2px 环)
///    —— 与插件页 :focus-within 同款。
/// 断言读 BorderBrush/BorderThickness/BoxShadow 的生效值 (样式优先级已折算)。
/// 另含跨页一致性测试: Settings 卡与插件页卡的描边配方逐项相同。
/// </summary>
[Collection("I18nSerial")]
public sealed class SettingsCardEffectTests
{
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
            //    **边框色不变** (悬停只改 BoxShadow, 不动 BorderBrush)
            //    阴影换 ClaudeShadowCardDeep —— 去环版 (2026-09-17 用户要求清除灰描边;
            //    原用 ClaudeShadowCardHover 会带 1px #d1cfc5 灰环, 悬停即出灰线)
            var pt = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), window)!.Value;
            window.MouseMove(pt);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsPointerOver, "悬停应命中卡片");
            var hoverBrush = (ISolidColorBrush)card.BorderBrush!;
            Assert.Equal(creamObj.Color, hoverBrush.Color);
            var hoverShadow = (BoxShadows)view.FindResource("ClaudeShadowCardDeep")!;
            Assert.Equal(hoverShadow.ToString(), card.BoxShadow.ToString());

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
    /// 跨页悬停守护 (2026-09-17 用户要求清除组件框悬停灰描边):
    /// 插件页 pluginCard 与设置页 settingsCard / leftPanel 悬停时都必须
    /// **描边色不变 + 投影换 ClaudeShadowCardDeep** —— 任一页漂回 Ring 版 (带 #d1cfc5 灰环) 即红。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Deepens_Shadow_Without_Gray_Ring_On_Both_Pages()
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
            var deep = (BoxShadows)sview.FindResource("ClaudeShadowCardDeep")!;

            var pluginCard = Assert.Single(
                pview.GetVisualDescendants().OfType<Border>(),
                b => b.Classes.Contains("pluginCard"));
            AssertHoverHasNoGrayRing(pwin, pluginCard, deep);

            // 只悬停**可见且已布局**的组件框: 折叠分区内的卡 Bounds=0/IsVisible=False, 本就无法悬停
            // (其静止配方仍由 Settings_Cards_Match_Plugins_Card_Border_Recipe 用全集守护)
            var settingsCards = sview.GetVisualDescendants().OfType<Border>()
                .Where(b => (b.Classes.Contains("settingsCard") || b.Classes.Contains("leftPanel"))
                            && b.IsEffectivelyVisible && b.Bounds.Width > 0 && b.Bounds.Height > 0)
                .ToList();
            Assert.True(settingsCards.Count > 0, "未找到可见的 Settings 页组件框");
            foreach (var c in settingsCards)
            {
                AssertHoverHasNoGrayRing(swin, c, deep);
            }
        }
        finally
        {
            pwin.Close();
            swin.Close();
        }
    }

    /// <summary>悬停某卡片: 命中后断言描边色不变 (无灰线) 且 BoxShadow == deep 档; 随后移出复位。</summary>
    private static void AssertHoverHasNoGrayRing(Window win, Border card, BoxShadows deep)
    {
        var restBrush = ((ISolidColorBrush)card.BorderBrush!).Color;
        var p = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), win)!.Value;
        win.MouseMove(p);
        Dispatcher.UIThread.RunJobs();

        Assert.True(card.IsPointerOver,
            $"{string.Join("+", card.Classes)} 悬停应命中 (Bounds={card.Bounds}, IsVisible={card.IsVisible}, EffVisible={card.IsEffectivelyVisible}, pt={p})");
        Assert.Equal(restBrush, ((ISolidColorBrush)card.BorderBrush!).Color);
        Assert.Equal(deep.ToString(), card.BoxShadow.ToString());

        win.MouseMove(new Point(1, 1));
        Dispatcher.UIThread.RunJobs();
    }
}
