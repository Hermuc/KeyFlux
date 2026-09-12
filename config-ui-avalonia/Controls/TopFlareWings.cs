using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.VisualTree;

namespace KeyFlux.Settings.Controls;

/// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
public sealed class TopFlareWingsMarker
{
}

/// <summary>
/// TopFlareWings —— 顶缘吸附翼: 右列一旦滚动 (Offset &gt; 0), 视口顶部左右两角
/// 持续显示向外扩散的圆角翼 (水滴吸附天花板的摊开轮廓), 滚回顶部后隐藏。
/// 「持续显示」是刻意设计: 卡片间存在间隙, 若仅在卡片跨越上缘时显示,
/// 翼会在卡片间隙处消失再出现 (用户报: 翼存在时间太短)。
///
/// 翼与卡片边缘的贴合 (用户两报「左上翼与组件框不贴合」的根因): 不做任何静态对齐 ——
/// Viewbox 排布下 * 列实际宽度随窗口变化 (∞ 测量塌缩到内容自然宽, 排布时是剩余宽),
/// 内容在视口内的对齐也随 presenter 浮动, 静态同心必然在某些窗口尺寸下脱开。
/// 改为滚动时按第一张卡片的实际边界 TranslatePoint 动态定位 (所有卡同宽, 量一张即可)。
///
/// 挂在滚动容器上 (FlareOverlay = 翼形画布, 置于滚动容器之上的固定覆盖层,
/// 不随内容滚动 —— 效果钉在视口顶部); 事件驱动 (ScrollChanged/SizeChanged), 值不变不写。
/// </summary>
public static class TopFlareWings
{
    private const double WingWidth = 18;

    public static readonly AttachedProperty<bool> IsEnabledProperty =
        AvaloniaProperty.RegisterAttached<TopFlareWingsMarker, ScrollViewer, bool>("IsEnabled");

    public static readonly AttachedProperty<Control?> FlareOverlayProperty =
        AvaloniaProperty.RegisterAttached<TopFlareWingsMarker, ScrollViewer, Control?>("FlareOverlay");

    public static bool GetIsEnabled(ScrollViewer sv) => sv.GetValue(IsEnabledProperty);
    public static void SetIsEnabled(ScrollViewer sv, bool value) => sv.SetValue(IsEnabledProperty, value);

    public static Control? GetFlareOverlay(ScrollViewer sv) => sv.GetValue(FlareOverlayProperty);
    public static void SetFlareOverlay(ScrollViewer sv, Control? value) => sv.SetValue(FlareOverlayProperty, value);

    static TopFlareWings()
    {
        IsEnabledProperty.Changed.AddClassHandler<ScrollViewer>((sv, e) =>
        {
            if (e.NewValue is true)
            {
                sv.ScrollChanged += OnScrollChanged;
                sv.SizeChanged += OnSizeChanged;
                Update(sv);
            }
            else
            {
                sv.ScrollChanged -= OnScrollChanged;
                sv.SizeChanged -= OnSizeChanged;
            }
        });
    }

    private static void OnScrollChanged(object? sender, ScrollChangedEventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            Update(sv);
        }
    }

    private static void OnSizeChanged(object? sender, SizeChangedEventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            Update(sv);
        }
    }

    private static void Update(ScrollViewer sv)
    {
        var overlay = sv.GetValue(FlareOverlayProperty);
        if (overlay is null)
        {
            return;
        }
        // 翼显隐 = 右列是否处于滚动状态 ( Offset > 1px 视为滚动 );
        // 持续显示直到滚回顶部 —— 卡片间隙处不断裂 (用户规格: 翼持续到下一张卡)
        var scrolled = sv.Offset.Y > 1;
        if (overlay.IsVisible != scrolled)
        {
            overlay.IsVisible = scrolled;
        }
        if (scrolled)
        {
            PositionWings(sv, overlay);
        }
    }

    /// <summary>
    /// 两翼 Path 竖直边精确压在卡片左右缘: 左翼竖直边在自身局部 x=18 (翼体向左铺开),
    /// 右翼竖直边在自身局部 x=0 (翼体向右铺开) —— 几何与定位解耦, Path 只管形状。
    /// </summary>
    private static void PositionWings(ScrollViewer sv, Control overlay)
    {
        if (overlay is not Canvas canvas || canvas.Children.Count < 2)
        {
            return;
        }
        if (sv.Content is not Visual content)
        {
            return;
        }
        var card = content.GetVisualDescendants().OfType<Border>().FirstOrDefault();
        if (card is null)
        {
            return;
        }
        var origin = card.TranslatePoint(new Point(0, 0), overlay);
        if (origin is not Point p)
        {
            return;
        }
        var left = canvas.Children[0];
        var right = canvas.Children[1];
        var lx = p.X - WingWidth;
        var rx = p.X + card.Bounds.Width;
        if (Canvas.GetLeft(left) != lx)
        {
            Canvas.SetLeft(left, lx);
        }
        if (Canvas.GetLeft(right) != rx)
        {
            Canvas.SetLeft(right, rx);
        }
    }
}
