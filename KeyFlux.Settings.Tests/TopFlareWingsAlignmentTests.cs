using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Controls;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 顶缘吸附翼守护 (用户多轮反馈的回归门):
/// ① 翼画布 Width=0 零测量足迹 —— Viewbox ∞ 测量下任何正宽度都会撑大 * 列自然宽,
///    翼显隐翻转时缩放比跳动 (用户报: 滚轮滑动时右侧一整列移动);
/// ② 翼几何随卡片位置形变 (位置驱动动画):
///    深跨越 (圆角已越过上缘) → 翼内边 = 卡缘 + 1.5 重叠;
///    圆角尚在视口内 → 翼内边填满圆角缺口 (固定直边翼在此留缝 = 用户报"割裂");
/// ③ 画布顶与右列 ScrollViewer 视口顶重合 (翼钉在视口上缘)。
/// 断言全部在翼画布坐标系 (设计像素) 内比对, Viewbox 缩放因子自然消去。
/// </summary>
[Collection("I18nSerial")]
public sealed class TopFlareWingsAlignmentTests
{
    private readonly Xunit.Abstractions.ITestOutputHelper _output;

    public TopFlareWingsAlignmentTests(Xunit.Abstractions.ITestOutputHelper output) => _output = output;

    [AvaloniaFact]
    public void Wing_Geometry_Morphs_With_Card_Position()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var view = new SettingsPageView { DataContext = new SettingsPageViewModel(main) };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();

        var canvas = view.FindControl<Canvas>("PART_TopFlareWings");
        Assert.NotNull(canvas);
        // ① 零测量足迹: 画布显式宽必须为 0 (子元素溢出渲染, 不参与 ∞ 测量撑大列宽)
        Assert.Equal(0, canvas!.Width);

        var host = canvas.GetVisualRoot() as Visual ?? throw new InvalidOperationException("no root");
        var panel = host.GetVisualDescendants().OfType<StackPanel>()
            .First(p => p.Width == 368);
        var card = panel.Children.OfType<Border>().First();
        var sv = host.GetVisualDescendants().OfType<ScrollViewer>()
            .First(s => TopFlareWings.GetIsEnabled(s));

        // headless 默认全部折叠段收起, 内容高 ≤ 视口 800 → 无滚动量; 强制面板超高
        panel.Height = 2000;
        Dispatcher.UIThread.RunJobs();

        var radius = card.CornerRadius.TopLeft;
        Assert.True(radius > 0, "卡片应有圆角 (翼形变围绕它设计)");

        // 断言读几何自身 Bounds —— path.Bounds 是 Shape 排布尺寸 (零宽画布把子项排布约束到近零宽),
        // 不反映几何真值; 渲染走几何本身 (默认无裁剪), 实机已验证完整成形
        static double GeoRight(Control path) => ((Avalonia.Controls.Shapes.Path)path).Data!.Bounds.Right;

        // 滚到卡片圆角已完全越过上缘 (T + r ≤ 0): 翼内边 = 卡缘 + EdgeOverlap(1.5)
        ScrollCardTopTo(sv, card, canvas, -radius - 5);
        var p = card.TranslatePoint(new Point(0, 0), canvas)!.Value;
        var leftPath = (Avalonia.Controls.Shapes.Path)canvas.Children[0];
        Assert.True(canvas.IsVisible, "滚动后翼应显示");
        Assert.Equal(2, canvas.Children.Count);
        Assert.Equal(p.X + 1.5, GeoRight(leftPath), 0.5);

        // 滚到圆角一半在视口内 (T = -r/2): 翼内边填圆角缺口 —— 右缘越过卡缘
        // inset(0) = r - sqrt(r² - (r/2)²) = r(1 - √3/2) ≈ 0.134r
        ScrollCardTopTo(sv, card, canvas, -radius / 2);
        p = card.TranslatePoint(new Point(0, 0), canvas)!.Value;
        var insetAtCeiling = radius - Math.Sqrt(radius * radius - (p.Y + radius) * (p.Y + radius));
        Assert.True(Math.Abs(GeoRight(leftPath) - (p.X + insetAtCeiling + 1.5)) < 0.6,
            $"corner-visible: p={p} geo={leftPath.Data!.Bounds} r={radius} offset={sv.Offset}");

        // ③ 画布顶 == 视口顶 (翼钉在视口上缘, 不随视口在格内垂直居中而漂移)
        var canvasT = canvas.TranslatePoint(new Point(0, 0), window);
        var svT = sv.TranslatePoint(new Point(0, 0), window);
        Assert.True(Math.Abs(svT!.Value.Y - canvasT!.Value.Y) < 0.5, $"top canvasT={canvasT} svT={svT}");
    }

    /// <summary>调整滚动偏移使卡片顶到达画布坐标系下的目标 Y ( ceiling = 0 )。
    /// 卡顶上移 (T 减小) = 内容上移 = offset 增大: offset += 当前T - 目标T。
    /// 两轮 RunJobs: 第一轮排空投递的翼重建 (Background), 第二轮排空 Data 变更
    /// 触发的布局 pass —— path.Bounds 要等重排才反映新几何。</summary>
    private void ScrollCardTopTo(ScrollViewer sv, Border card, Canvas canvas, double targetTopY)
    {
        var current = card.TranslatePoint(new Point(0, 0), canvas)!.Value.Y;
        var delta = current - targetTopY;
        sv.Offset = new Vector(sv.Offset.X, Math.Max(0, sv.Offset.Y + delta));
        Dispatcher.UIThread.RunJobs();
        var mid = card.TranslatePoint(new Point(0, 0), canvas)!.Value;
        Dispatcher.UIThread.RunJobs();
        var fin = card.TranslatePoint(new Point(0, 0), canvas)!.Value;
        _output.WriteLine($"scrollTo {targetTopY}: mid={mid} fin={fin} offset={sv.Offset}");
    }
}
