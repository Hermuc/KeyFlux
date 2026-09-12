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
/// TopFlareWings —— 顶缘吸附翼: 卡片顶部穿越视口上缘期间, 在视口顶部两角
/// 绘制向外扩散的圆角翼 (水滴吸附天花板的摊开轮廓), 卡片完全移出视野后隐藏。
///
/// 挂在滚动容器上 (FlareOverlay = 翼形画布, 置于滚动容器之上的固定覆盖层,
/// 不随内容滚动 —— 效果钉在视口顶部); 事件驱动 (ScrollChanged), 仅遍历直接子卡片
/// (~10), 值不变不写。
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
        if (overlay is null || sv.Content is not Panel panel)
        {
            return;
        }

        // 任一卡片跨越视口上缘 (top < 0 < bottom) → 显示吸附翼, 否则隐藏
        var straddling = false;
        foreach (var child in panel.Children)
        {
            if (child is not Border card)
            {
                continue;
            }
            if (card.TranslatePoint(new Point(0, 0), sv) is not { } top)
            {
                continue;
            }
            var bottom = top.Y + card.Bounds.Height;
            if (top.Y < 0 && bottom > 0)
            {
                straddling = true;
                break;
            }
        }
        if (overlay.IsVisible != straddling)
        {
            overlay.IsVisible = straddling;
        }
    }
}
