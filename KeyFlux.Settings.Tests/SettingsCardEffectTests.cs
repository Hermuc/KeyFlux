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
/// ① 静止态: **1px** 奶油边框 + ClaudeShadowCard 双层下坠影 (原为 2px, 与插件页差一倍);
/// ② 悬停态: **边框色不变**, 仅阴影换 ClaudeShadowCardHover (内含 1px #d1cfc5 环 + 强影)
///    —— 原实现把边框换成 #c9c7bd 并用去环版 Deep, 与插件页的环机制不同;
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

            // ① 静止态: 1px 奶油边框 + ClaudeShadowCard 双层下坠影
            Assert.Equal(creamObj!.Color, restBrush.Color);
            Assert.Equal(1, card.BorderThickness.Left);
            Assert.Equal(restShadow.ToString(), card.BoxShadow.ToString());

            // ② 悬停: headless 鼠标移到卡片中心 → :pointerover
            //    **边框色不变** (与插件页一致: 状态变化走 BoxShadow 环, 不动 BorderBrush)
            //    阴影换 ClaudeShadowCardHover (内含 1px #d1cfc5 环 + 强影)
            var pt = Avalonia.VisualExtensions.TranslatePoint(
                card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), window)!.Value;
            window.MouseMove(pt);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsPointerOver, "悬停应命中卡片");
            var hoverBrush = (ISolidColorBrush)card.BorderBrush!;
            Assert.Equal(creamObj.Color, hoverBrush.Color);
            var hoverShadow = (BoxShadows)view.FindResource("ClaudeShadowCardHover")!;
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
}
