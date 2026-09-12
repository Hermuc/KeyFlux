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
/// 顶缘吸附翼对齐守护 (用户三报翼不贴合/整列移位的回归门):
/// ① 翼画布 Width=0 零测量足迹 —— Viewbox 以 ∞ 测量子内容, 画布任何正宽度都会撑大
///    * 列自然宽, 翼显隐翻转时缩放比跳动 (用户报: 滚轮滑动时右侧一整列移动);
/// ② 滚动后两翼 Path 竖直边按卡片实际渲染边界定位 —— 左翼竖直边 (局部 x=18) 压卡左缘,
///    右翼竖直边 (局部 x=0) 压卡右缘;
/// ③ 画布顶与右列 ScrollViewer 视口顶重合 (翼钉在视口上缘)。
/// </summary>
[Collection("I18nSerial")]
public sealed class TopFlareWingsAlignmentTests
{
    [AvaloniaFact]
    public void Wing_Canvas_Is_Concentric_With_Cards_And_Pinned_To_Viewport_Top()
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
        // 卡片列 = Width=368 锁宽的 StackPanel (卡 Border 靠拉伸铺满, 自身无显式宽)
        var panel = host.GetVisualDescendants().OfType<StackPanel>()
            .First(p => p.Width == 368);
        var card = panel.GetVisualDescendants().OfType<Border>().First();
        var sv = host.GetVisualDescendants().OfType<ScrollViewer>()
            .First(s => TopFlareWings.GetIsEnabled(s));

        // headless 默认全部折叠段收起, 内容高 ≤ 视口 800 → 无滚动量 (Offset 被钳到 0);
        // 强制面板超高制造滚动量 (只影响纵向, 水平几何不变)
        panel.Height = 2000;
        Dispatcher.UIThread.RunJobs();
        // 未滚动时行为把画布藏起 (IsVisible=false 不参与布局, Bounds=0); 触发滚动驱动定位
        sv.Offset = new Vector(0, 50);
        Dispatcher.UIThread.RunJobs();
        Assert.True(canvas.IsVisible,
            $"翼未显示: offset={sv.Offset} extent={sv.Extent} viewport={sv.Viewport}");
        Assert.Equal(2, canvas.Children.Count);

        var cardT = card.TranslatePoint(new Point(0, 0), window);
        // 卡右缘同样走 TranslatePoint (card.Bounds 是未缩放布局单位, 手工加会混掉 Viewbox 缩放)
        var cardR = card.TranslatePoint(new Point(card.Bounds.Width, 0), window)!.Value;
        var canvasT = canvas.TranslatePoint(new Point(0, 0), window);
        var svT = sv.TranslatePoint(new Point(0, 0), window);

        var leftEdge = canvas.Children[0].TranslatePoint(new Point(18, 0), window)!.Value;
        var rightEdge = canvas.Children[1].TranslatePoint(new Point(0, 0), window)!.Value;

        var diag = $"cardT={cardT} cardW={card.Bounds.Width} leftEdge={leftEdge} rightEdge={rightEdge} canvasT={canvasT} "
                 + $"getL={Canvas.GetLeft(canvas.Children[0])} getR={Canvas.GetLeft(canvas.Children[1])} "
                 + $"rB={canvas.Children[1].Bounds} cB={canvas.Bounds} pB={panel.Bounds}";
        // ② 两翼竖直边压进卡缘 1.5 设计像素 (同色重叠消抗锯齿缝; 完美相切会留暗线)
        var scale = canvas.Bounds.Height / 18;
        var overlap = 1.5 * scale;
        Assert.True(Math.Abs((leftEdge.X - cardT.Value.X) - overlap) < 0.5, $"left wing {diag}");
        Assert.True(Math.Abs((cardR.X - rightEdge.X) - overlap) < 0.5, $"right wing {diag}");
        // ③ 画布顶 == 视口顶 (翼钉在视口上缘, 不随视口在格内垂直居中而漂移)
        Assert.True(Math.Abs(svT!.Value.Y - canvasT!.Value.Y) < 0.5, $"top {diag}");
    }
}
