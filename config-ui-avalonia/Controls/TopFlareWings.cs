using System;
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
/// 挂在滚动容器上 (FlareOverlay = 翼形画布, 置于滚动容器之上的固定覆盖层,
/// 不随内容滚动 —— 效果钉在视口顶部); 事件驱动 (ScrollChanged), 值不变不写。
/// </summary>
public static class TopFlareWings
{
    public static readonly AttachedProperty<bool> IsEnabledProperty =
        AvaloniaProperty.RegisterAttached<TopFlareWingsMarker, ScrollViewer, bool>("IsEnabled");

    public static readonly AttachedProperty<Control?> FlareOverlayProperty =
        AvaloniaProperty.RegisterAttached<TopFlareWingsMarker, ScrollViewer, Control?>("FlareOverlay");

    public static bool GetIsEnabled(ScrollViewer sv) => sv.GetValue(IsEnabledProperty);
    public static void SetIsEnabled(ScrollViewer sv, bool value) => sv.SetValue(IsEnabledProperty, value);

    public static Control? GetFlareOverlay(ScrollViewer sv) => sv.GetValue(FlareOverlayProperty);
    public static void SetFlareOverlay(ScrollViewer sv, Control? overlay) => sv.SetValue(FlareOverlayProperty, overlay);

    static TopFlareWings()
    {
        IsEnabledProperty.Changed.AddClassHandler<ScrollViewer>((sv, e) =>
        {
            if (e.NewValue is true)
            {
                sv.ScrollChanged += OnScrollChanged;
                Update(sv);
            }
            else
            {
                sv.ScrollChanged -= OnScrollChanged;
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
    }
}
