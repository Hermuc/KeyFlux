using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views.Controls;
using Xunit;
using Xunit.Abstractions;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 动作编辑面板悬停反馈守护 (2026-09-25/26 多轮迭代后的最终形态):
///   · 悬停反馈 = 输入框自己的边框变陶土色 2px (ComboBoxBorderBrushPointerOver /
///     TextControlBorderBrushPointerOver 面板级资源覆盖), 移出回落奶油色;
///   · 外圈光圈 (haloRing far/near 双层填充环): 指针在卡面空白区/环带上点亮, 悬停
///     可交互子控件时熄灭让位 (childHover 类, code-behind 指针命中测试切换);
///   · 未选键的禁用态类型下拉: 不可命中 → 悬停无反馈 (边框保持奶油色)。
/// Viewbox 分数缩放页内弃用 BoxShadow/负 Margin 画法 (渲染损坏, 见 git 历史)。
/// </summary>
public sealed class EditorCardHaloTests
{
    private readonly ITestOutputHelper _output;
    public EditorCardHaloTests(ITestOutputHelper output) => _output = output;

    private static readonly Color Cream = Color.FromArgb(0xff, 0xf0, 0xee, 0xe6);
    private static readonly Color Terracotta = Color.FromArgb(0xff, 0xc9, 0x64, 0x42);
    private static readonly Color BandHover = Color.FromArgb(0x26, 0xc9, 0x64, 0x42);

    /// <summary>摘掉画刷过渡 (headless 时钟不推进, 直达终点态; 同 SettingsCardEffectTests)。</summary>
    private static void DetachTransitions(ActionEditorPanel panel)
    {
        foreach (var b in panel.GetVisualDescendants().OfType<Border>())
            b.Transitions = null;
    }

    [AvaloniaFact]
    public void EditorPanel_Hover_Combo_Border_Turns_Terracotta_And_Outer_Halo_Yields()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var core = new KeymapEditorCore(main, new Models.Keymap { Id = 10, Hotkey = "F", Enable = true });
        core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = 1, Target = "shortcuts-WeChat.link" });
        var panel = new ActionEditorPanel { Width = 580, DataContext = core.Editor };
        var window = new Window { Width = 800, Height = 900, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        DetachTransitions(panel);

        var far = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("haloRing") && b.Classes.Contains("far"));
        var combo = panel.GetVisualDescendants().OfType<ComboBox>().First();
        // 悬停变色发生在模板边框上 (Fluent ComboBox 模板的 Border#Background)
        var templateBorder = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Name == "Background");
        var card = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("editorCard"));

        // ① 悬停下拉框: 自己的边框变陶土 (2px); 外圈光圈熄灭让位
        var pt = Avalonia.VisualExtensions.TranslatePoint(
            combo, new Point(combo.Bounds.Width / 2, combo.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.True(combo.IsPointerOver, "悬停应命中下拉框");
        Assert.Equal(Terracotta, ((ISolidColorBrush)templateBorder.BorderBrush!).Color);
        Assert.Equal(Colors.Transparent, ((ISolidColorBrush)far.Background!).Color);

        // ② 指针移回卡面空白区: 边框回落奶油色, 外圈光圈恢复
        var pt2 = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height - 30), window)!.Value;
        window.MouseMove(pt2);
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(Cream, ((ISolidColorBrush)templateBorder.BorderBrush!).Color);
        Assert.Equal(BandHover, ((ISolidColorBrush)far.Background!).Color);
    }

    [AvaloniaFact]
    public void EditorPanel_Disabled_Type_Combo_Shows_No_Ring_And_Enabled_Shows()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var core = new KeymapEditorCore(main, new Models.Keymap { Id = 10, Hotkey = "F", Enable = true });
        var panel = new ActionEditorPanel { Width = 580, DataContext = core.Editor };
        var window = new Window { Width = 800, Height = 700, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        DetachTransitions(panel);

        var typeCombo = panel.GetVisualDescendants().OfType<ComboBox>().Last(); // 第二个 = 动作类型
        var typeTemplateBorder = panel.GetVisualDescendants().OfType<Border>()
            .Last(b => b.Name == "Background");

        // ① 未选键: 类型下拉禁用 (深灰不可点) → 边框保持奶油色 (无悬停反馈)
        Assert.True(core.Editor.IsTypeDisabled, "未绑定动作时类型下拉应为禁用态");
        Assert.Equal(Cream, ((ISolidColorBrush)typeCombo.BorderBrush!).Color);

        // ② 绑定动作 (类型1) → 启用: 悬停边框变陶土
        core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = 1 });
        Dispatcher.UIThread.RunJobs();
        Assert.False(core.Editor.IsTypeDisabled, "绑定类型1后应非禁用态");
        var pt = Avalonia.VisualExtensions.TranslatePoint(
            typeCombo, new Point(typeCombo.Bounds.Width / 2, typeCombo.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.True(typeCombo.IsPointerOver, "悬停应命中类型下拉框");
        Assert.Equal(Terracotta, ((ISolidColorBrush)typeTemplateBorder.BorderBrush!).Color);

        // ③ 已选键但类型仍为「未配置」(类型0, 白/可点): 悬停照常变陶土
        core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = 0 });
        Dispatcher.UIThread.RunJobs();
        Assert.False(core.Editor.IsTypeDisabled, "已选键后应非禁用态 (即使类型为未配置)");
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(Terracotta, ((ISolidColorBrush)typeTemplateBorder.BorderBrush!).Color);
    }
}
