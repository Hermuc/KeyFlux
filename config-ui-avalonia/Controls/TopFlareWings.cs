using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using AvPath = Avalonia.Controls.Shapes.Path;

namespace KeyFlux.Settings.Controls;

/// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
public sealed class TopFlareWingsMarker
{
}

/// <summary>
/// TopFlareWings —— 顶缘吸附翼: 右列滚动期间 (Offset &gt; 1), 视口顶部左右两角显示
/// 随卡片位置连续形变的翼 (水滴压上天花板的材质摊开轮廓), 滚回顶部后隐藏。
///
/// 形变 = 位置驱动的动画 (Web 界做法是 CSS scroll-driven animation + corner-shape,
/// 软件渲染无此能力, 用 StreamGeometry 随滚动逐帧重建等效实现):
/// 翼的内边界逐点贴合卡片实际轮廓 ——
/// · 卡片圆角尚在视口内: 翼填满圆角与上缘之间的缺口 (固定直边翼在此留缝 = 用户报"割裂");
/// · 圆角越过上缘: 翼退化为贴卡缘的固定圆角翼 (用户验收过的形态);
/// · 卡片顶还没到上缘: 翼呈悬挂薄片向下够向卡片 (卡片间距 12 &lt; 包覆深度 18, 恒有重叠)。
/// 每次滚动重算几何, 连续可逆, 无关键帧无定时器; 每帧仅重建两片小 Path。
/// 「持续显示」是刻意设计: 卡片间隙处翼不断裂 (用户规格: 翼持续到滚回顶部)。
/// </summary>
public static class TopFlareWings
{
    private const double WingWidth = 18;    // 沿天花板向外铺开宽度
    private const double HangDepth = 18;    // 沿卡缘向下包覆深度
    private const double EdgeOverlap = 1.5; // 内边界压进卡内深度 (消抗锯齿缝, 同色重叠不可见)
    private const int BoundarySamples = 8;  // 内边界采样数 (圆角弧段在此分辨率下平滑)

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
                sv.LayoutUpdated += OnLayoutUpdated;
                Update(sv);
            }
            else
            {
                sv.ScrollChanged -= OnScrollChanged;
                sv.SizeChanged -= OnSizeChanged;
                sv.LayoutUpdated -= OnLayoutUpdated;
            }
        });
    }

    private static void OnScrollChanged(object? sender, EventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            ScheduleUpdate(sv);
        }
    }

    private static void OnSizeChanged(object? sender, SizeChangedEventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            ScheduleUpdate(sv);
        }
    }

    private static void OnLayoutUpdated(object? sender, EventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            ScheduleUpdate(sv);
        }
    }

    /// <summary>
    /// 重建必须等布局完全落定: ScrollChanged / LayoutUpdated (画布自身排布完成即触发)
    /// 时卡片可能还在本次布局半路, 直接量会拿到滞后一帧的卡位 (滚动停止时翼形停在中间态)。
    /// 投递到 Background 优先级 (布局队列之后) 执行, Tag 去重防止一帧多排。
    /// </summary>
    private static void ScheduleUpdate(ScrollViewer sv)
    {
        if (sv.Tag is not null)
        {
            return;
        }
        sv.Tag = new object();
        Dispatcher.UIThread.Post(() =>
        {
            sv.Tag = null;
            Update(sv);
        }, DispatcherPriority.Background);
    }

    private static void Update(ScrollViewer sv)
    {
        var overlay = sv.GetValue(FlareOverlayProperty);
        if (overlay is null)
        {
            return;
        }
        // 翼显隐 = 右列是否处于滚动状态 ( Offset > 1px 视为滚动 )
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

    /// <summary>翼与卡缘的贴合不做任何静态对齐 (Viewbox 排布下列宽/内容对齐随窗口浮动,
    /// 静态同心必有残余误差) —— 滚动时按卡片实际边界动态重建几何, 任意窗口尺寸下自校正。</summary>
    private static void PositionWings(ScrollViewer sv, Control overlay)
    {
        if (overlay is not Canvas canvas || canvas.Children.Count < 2)
        {
            return;
        }
        if (sv.Content is not Panel panel)
        {
            return;
        }
        var cards = panel.Children.OfType<Border>().ToList();
        if (cards.Count == 0)
        {
            return;
        }

        // 驱动卡 = 顶缘最接近上缘的一张 (top 最大且未远离翼区; 它的圆角是翼要包覆的对象)
        Border driver = cards[0];
        var driverTop = double.NegativeInfinity;
        foreach (var c in cards)
        {
            if (c.TranslatePoint(new Point(0, 0), overlay) is not Point pt)
            {
                continue;
            }
            if (pt.Y <= HangDepth + 24 && pt.Y > driverTop)
            {
                driverTop = pt.Y;
                driver = c;
            }
        }
        if (driver.TranslatePoint(new Point(0, 0), overlay) is not Point pos)
        {
            return;
        }

        // 位置未变守卫: LayoutUpdated 每个布局 pass 都触发, 几何只在卡位/卡宽真正变化时重建
        if (canvas.Tag is not double[] last || last.Length < 3)
        {
            last = new[] { double.NaN, double.NaN, double.NaN };
            canvas.Tag = last;
        }
        if (Math.Abs(pos.X - last[0]) < 0.01 && Math.Abs(pos.Y - last[1]) < 0.01
            && Math.Abs(driver.Bounds.Width - last[2]) < 0.01)
        {
            return;
        }
        last[0] = pos.X;
        last[1] = pos.Y;
        last[2] = driver.Bounds.Width;

        var left = canvas.Children[0] as AvPath;
        var right = canvas.Children[1] as AvPath;
        if (left is null || right is null)
        {
            return;
        }
        var cardRight = pos.X + driver.Bounds.Width;
        BuildWing(left, isLeft: true, innerBase: pos.X, outerX: pos.X - WingWidth,
            radius: driver.CornerRadius.TopLeft, cardTopY: pos.Y, hangDepth: HangDepth);
        BuildWing(right, isLeft: false, innerBase: cardRight, outerX: cardRight + WingWidth,
            radius: driver.CornerRadius.TopRight, cardTopY: pos.Y, hangDepth: HangDepth);
    }

    /// <summary>
    /// 重建一片翼: 内边界 = 卡片实际轮廓 (圆角弧/直边, 压入 EdgeOverlap) 从上缘到包覆深度逐点采样;
    /// 外边界 = 从包覆深度终点到天花板外尖的凹形收窄曲线。绝对画布坐标, Path 无需再定位。
    /// </summary>
    private static void BuildWing(AvPath path, bool isLeft, double innerBase, double outerX,
        double radius, double cardTopY, double hangDepth)
    {
        // side: 内边界压入卡内的方向 (左翼向右压 +1, 右翼向左压 -1)
        var side = isLeft ? 1.0 : -1.0;

        double InnerBoundary(double y)
        {
            if (y < cardTopY)
            {
                return innerBase; // 卡片顶还没到: 悬挂薄片, 内边贴卡缘线 (无卡可压, 不加重叠)
            }
            var dy = cardTopY + radius - y;
            if (dy > 0 && radius > 0)
            {
                // 圆角弧段: 卡片材料边界向内收 (缺口), 翼内边界贴合并压入 EdgeOverlap
                var inset = radius - Math.Sqrt(Math.Max(0, radius * radius - dy * dy));
                return innerBase + side * (inset + EdgeOverlap);
            }
            return innerBase + side * EdgeOverlap; // 直边段
        }

        var geometry = new StreamGeometry();
        using (var ctx = geometry.Open())
        {
            var tip = new Point(outerX, 0);
            ctx.BeginFigure(tip, true);
            Point last = default;
            for (var i = 0; i <= BoundarySamples; i++)
            {
                var y = hangDepth * i / BoundarySamples;
                last = new Point(InnerBoundary(y), y);
                ctx.LineTo(last);
            }
            // 凹形收窄曲线回到外尖 (控制点靠近上缘角部 ≈ 用户验收过的四分之一圆弧轮廓)
            var control = new Point(outerX + side * WingWidth * 0.22, hangDepth * 0.17);
            ctx.QuadraticBezierTo(control, tip);
            ctx.EndFigure(true);
        }
        path.Data = geometry;
    }
}
