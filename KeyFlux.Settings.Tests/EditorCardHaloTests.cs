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
    /// 悬停让位 (2026-09-25 用户要求): 指针悬到子选项框 (Global 下拉) 时, 外圈光圈熄灭、
    /// 下拉框自己的 comboHalo 橙色线条描边点亮 (仅边框外一线, 非整框变色);
    /// 指针移回卡面空白区时反向恢复。
    /// </summary>
    [AvaloniaFact]
    public void EditorPanel_Hover_Combo_Suppresses_Outer_Halo_And_Lights_Combo_Ring()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var panel = new ActionEditorPanel { Width = 588 };
        var window = new Window { Width = 800, Height = 700, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        var far = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("haloRing") && b.Classes.Contains("far"));
        var wrapper = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("comboHalo"));
        // 两个下拉框 (窗口分组/动作类型) 均适用; 取第一个 (Global) 断言
        var combo = panel.GetVisualDescendants().OfType<ComboBox>().First();

        // headless 不推动画时钟: 摘掉画刷过渡直达终点态
        foreach (var b in panel.GetVisualDescendants().OfType<Border>()
                     .Where(b => b.Classes.Contains("haloRing") || b.Classes.Contains("comboHalo")))
            b.Transitions = null;

        var card = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Classes.Contains("editorCard"));
        var hoverFar = Color.FromArgb(0x26, 0xc9, 0x64, 0x42); // far 悬停档 (两环版 30%)

        // ① 悬停下拉框: 外圈让位 (透明), comboHalo 橙色描边点亮
        var pt = Avalonia.VisualExtensions.TranslatePoint(
            combo, new Point(combo.Bounds.Width / 2, combo.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.True(combo.IsPointerOver, "悬停应命中下拉框");
        // Fluent 模板 :pointerover 边框经面板级资源覆盖为奶油色 (消除默认深灰"黑圈")
        var templateBorder = panel.GetVisualDescendants().OfType<Border>()
            .First(b => b.Name == "Background");
        Assert.Equal(Color.FromArgb(0xff, 0xf0, 0xee, 0xe6), ((ISolidColorBrush)templateBorder.BorderBrush!).Color);
        _output.WriteLine($"DEBUG: far.Classes=[{string.Join(",", far.Classes)}], far.IsPointerOver={far.IsPointerOver}, " +
                          $"combo.IsPointerOver={combo.IsPointerOver}, panel.IsPointerOver={panel.IsPointerOver}");
        Assert.Equal(Colors.Transparent, ((ISolidColorBrush)far.Background!).Color);
        // comboHalo = 边框外一圈橙色线条 (BorderBrush 变陶土, 非整框填充)
        Assert.Equal(Color.FromArgb(0xff, 0xc9, 0x64, 0x42), ((ISolidColorBrush)wrapper.BorderBrush!).Color);
        Assert.Equal(Colors.Transparent, ((ISolidColorBrush)wrapper.Background!).Color);

        // ② 移回卡面空白区 (底部): 外圈恢复 (两环版 far 悬停档 = 30%), 下拉框环熄灭
        var pt2 = Avalonia.VisualExtensions.TranslatePoint(
            card, new Point(card.Bounds.Width / 2, card.Bounds.Height - 30), window)!.Value;
        window.MouseMove(pt2);
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(hoverFar, ((ISolidColorBrush)far.Background!).Color);
        Assert.Equal(Colors.Transparent, ((ISolidColorBrush)wrapper.BorderBrush!).Color);
    }

    /// <summary>
    /// 未配置档不亮描边 (2026-09-25 用户要求): 动作类型下拉处于「未配置」档
    /// (未绑定动作/类型0) 时, 悬停不出现橙色线条; 配置后 (类型1) 悬停恢复正常描边。
    /// </summary>
    [AvaloniaFact]
    public void EditorPanel_Hover_Unconfigured_Type_Combo_Shows_No_Ring()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var core = new KeymapEditorCore(main, new Models.Keymap { Id = 10, Hotkey = "F", Enable = true });
        var panel = new ActionEditorPanel { Width = 580, DataContext = core.Editor };
        var window = new Window { Width = 800, Height = 700, Content = panel, Background = Brushes.White };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        foreach (var b in panel.GetVisualDescendants().OfType<Border>()
                     .Where(b => b.Classes.Contains("haloRing") || b.Classes.Contains("comboHalo")))
            b.Transitions = null;

        var typeCombo = panel.GetVisualDescendants().OfType<ComboBox>().Last(); // 第二个 = 动作类型
        var typeWrapper = panel.GetVisualDescendants().OfType<Border>()
            .Last(b => b.Classes.Contains("comboHalo"));

        // ① 未配置档 (未绑定动作 → IsTypeUnconfigured=true): 悬停不亮描边
        Assert.True(core.Editor.IsTypeUnconfigured, "未绑定动作应为未配置档");
        var pt = Avalonia.VisualExtensions.TranslatePoint(
            typeCombo, new Point(typeCombo.Bounds.Width / 2, typeCombo.Bounds.Height / 2), window)!.Value;
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.True(typeWrapper.IsPointerOver, "悬停应命中类型下拉框");
        Assert.Equal(Colors.Transparent, ((ISolidColorBrush)typeWrapper.BorderBrush!).Color);

        // ② 配置后 (类型1): 悬停恢复橙色描边
        core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = 1 });
        Dispatcher.UIThread.RunJobs();
        Assert.False(core.Editor.IsTypeUnconfigured, "绑定类型1后应非未配置档");
        window.MouseMove(pt);
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(Color.FromArgb(0xff, 0xc9, 0x64, 0x42), ((ISolidColorBrush)typeWrapper.BorderBrush!).Color);
    }
}
