using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Animation;
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
/// 动作编辑面板悬停光圈守护 (2026-09-25): 用户要求与插件页/选中动作页/选项页组件框
/// 同款陶土色悬停光圈。⚠ 面板位于 Viewbox 分数缩放页内, BoxShadow (含负 Margin 环)
/// 在真实软件渲染管线 + Viewbox 组合下渲染损坏 (用户实测「四角暗斑/组件框变方」),
/// 故面板弃用 BoxShadow, 改为**嵌套 Padding 填充环**: far/near 双环为卡的祖先
/// Border, 静止 Background=Transparent, 悬停经 :pointerover 换陶土色阶梯 (35%/19%/9%),
/// BrushTransition (120ms = ClaudeMotion.Micro) 淡入。环是卡的祖先 ⇒ 不能用 Opacity
/// 淡入 (祖先 Opacity 会连卡片一起藏掉), 故必须走画刷换色。
/// 断言: 静止三环透明; 悬停三环分别命中三档陶土色; 过渡接线为 BrushTransition(Micro)。
/// </summary>
public sealed class EditorCardHaloTests
{
    private readonly ITestOutputHelper _output;
    public EditorCardHaloTests(ITestOutputHelper output) => _output = output;

    [AvaloniaFact]
    public void EditorPanel_Hover_Halo_Rings_Swap_Brushes()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var panel = new ActionEditorPanel { Width = 588 };
        var window = new Window { Width = 800, Height = 700, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        var rings = panel.GetVisualDescendants().OfType<Border>()
            .Where(b => b.Classes.Contains("haloRing")).ToList();
        var far = Assert.Single(rings, b => b.Classes.Contains("far"));
        var near = Assert.Single(rings, b => b.Classes.Contains("near"));

        // ⓪ 过渡接线 (摘除前断言): 每环必须有 BrushTransition (120ms = ClaudeMotion.Micro)
        foreach (var ring in rings)
        {
            var t = Assert.Single(ring.Transitions!.OfType<BrushTransition>());
            Assert.Equal("Background", t.Property!.Name);
            Assert.Equal(ViewModels.ClaudeMotion.Micro, t.Duration);
        }

        // headless 不推动画时钟: 摘掉过渡使 :pointerover 换色直达终点态 (同 SettingsCardEffectTests)
        foreach (var ring in rings) ring.Transitions = null;

        var terracotta = new byte[] { 0xc9, 0x64, 0x42 }; // #c96442 RGB
        var rest = Colors.Transparent; // 静止态 = 透明 (样式字面量 "Transparent")
        var hoverNear = Color.FromArgb(0x4d, terracotta[0], terracotta[1], terracotta[2]);
        var hoverFar = Color.FromArgb(0x26, terracotta[0], terracotta[1], terracotta[2]);

        // ① 静止态: 双环全透明
        Assert.Equal(rest, ((ISolidColorBrush)far.Background!).Color);
        Assert.Equal(rest, ((ISolidColorBrush)near.Background!).Color);

        // ② 悬停: 指针落在卡片内 → 三环分别换三档陶土色 (外淡内浓阶梯)
        var card = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("editorCard"));
        var pt = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.True(far.IsPointerOver, "悬停应命中面板 (far 环为卡片祖先)");
        Assert.Equal(hoverFar, ((ISolidColorBrush)far.Background!).Color);
        Assert.Equal(hoverNear, ((ISolidColorBrush)near.Background!).Color);

        // ④ 移出复位: 双环回到透明
        window.MouseMove(new Point(1, 1));
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(rest, ((ISolidColorBrush)far.Background!).Color);
        Assert.Equal(rest, ((ISolidColorBrush)near.Background!).Color);
    }

    /// <summary>
    /// 悬停让位 (2026-09-25 用户要求): 指针悬到子选项框 (Global 下拉) 时, 外圈光圈熄灭;
    /// 2026-09-26 起 (用户裁定「边框颜色直接变为橙色, 在框边缘以内」) 下拉框自身边框
    /// (模板 Border#Background) 直接变陶土色, 不再有外悬线圈。指针移回卡面空白区恢复。
    /// </summary>
    [AvaloniaFact]
    public void EditorPanel_Hover_Combo_Suppresses_Outer_Halo_And_Tints_Combo_Border()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var panel = new ActionEditorPanel { Width = 588 };
        var window = new Window { Width = 800, Height = 700, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        var far = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("haloRing") && b.Classes.Contains("far"));
        var combo = panel.GetVisualDescendants().OfType<ComboBox>().First();

        foreach (var b in panel.GetVisualDescendants().OfType<Border>()
                     .Where(b => b.Classes.Contains("haloRing")))
            b.Transitions = null;

        var card = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("editorCard"));
        var templateBorder = combo.GetVisualDescendants().OfType<Border>()
            .First(b => b.Name == "Background");
        templateBorder.Transitions = null;

        var pt = Avalonia.VisualExtensions.TranslatePoint(
            combo, new Point(combo.Bounds.Width / 2, combo.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.True(combo.IsPointerOver, "悬停应命中下拉框");
        Assert.Equal(Colors.Transparent, ((ISolidColorBrush)far.Background!).Color);
        Assert.Equal(Color.FromArgb(0xff, 0xc9, 0x64, 0x42), ((ISolidColorBrush)templateBorder.BorderBrush!).Color);

        var pt2 = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height - 30), window)!.Value;
        window.MouseMove(pt2);
        Dispatcher.UIThread.RunJobs();
        // 指针回到卡面空白区: 外圈恢复 far 悬停档 (30% 陶土), 下拉框边框回奶油
        Assert.Equal(Color.FromArgb(0x26, 0xc9, 0x64, 0x42), ((ISolidColorBrush)far.Background!).Color);
        Assert.Equal(Color.FromArgb(0xff, 0xf0, 0xee, 0xe6), ((ISolidColorBrush)templateBorder.BorderBrush!).Color);
    }

    /// <summary>
    /// 禁用态不亮描边 (2026-09-25 用户要求, 同日二次细化): 未选键时类型下拉深灰禁用,
    /// 悬停不出现橙色描边; 已选键后 (无论类型是「未配置」还是已配置) 下拉框白色可点,
    /// 悬停照常变陶土 —— 与其他子选项框一致。
    /// </summary>
    [AvaloniaFact]
    public void EditorPanel_Hover_Disabled_Type_Combo_Shows_No_Tint()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var core = new KeymapEditorCore(main, new Models.Keymap { Id = 10, Hotkey = "F", Enable = true });
        var panel = new ActionEditorPanel { Width = 580, DataContext = core.Editor };
        var window = new Window { Width = 800, Height = 700, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        var typeCombo = panel.GetVisualDescendants().OfType<ComboBox>().Last();
        var templateBorder = typeCombo.GetVisualDescendants().OfType<Border>()
            .First(b => b.Name == "Background");
        templateBorder.Transitions = null;

        Assert.True(core.Editor.IsTypeDisabled, "未绑定动作时类型下拉应为禁用态");
        var pt = Avalonia.VisualExtensions.TranslatePoint(
            typeCombo, new Point(typeCombo.Bounds.Width / 2, typeCombo.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.NotEqual(Color.FromArgb(0xff, 0xc9, 0x64, 0x42), ((ISolidColorBrush)templateBorder.BorderBrush!).Color);

        core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = 1 });
        Dispatcher.UIThread.RunJobs();
        Assert.False(core.Editor.IsTypeDisabled, "绑定类型1后应非禁用态");
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(Color.FromArgb(0xff, 0xc9, 0x64, 0x42), ((ISolidColorBrush)templateBorder.BorderBrush!).Color);

        core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = 0 });
        Dispatcher.UIThread.RunJobs();
        Assert.False(core.Editor.IsTypeDisabled, "已选键后应非禁用态 (即使类型为未配置)");
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(Color.FromArgb(0xff, 0xc9, 0x64, 0x42), ((ISolidColorBrush)templateBorder.BorderBrush!).Color);
    }

}
