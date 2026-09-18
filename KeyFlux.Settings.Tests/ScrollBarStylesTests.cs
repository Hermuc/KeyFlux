using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Transformation;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 滚动条细条几何契约 (2026-09-18 用户要求) —— 四条一起锁定:
///   ① 收起态细条**居中**于滚动条带 (左右余量相等), 不再贴滚动区域右缘;
///   ② 鼠标悬浮展开时**向两侧对称变粗** (原为只向左单侧延伸);
///   ③ 展开动画的**每一帧**都保持中心不变 (不只是终态居中);
///   ④ 改动后滚动功能仍正常 (滚动条驱动 + 行滚动均可)。
///
/// 实现落点在 <c>config-ui-avalonia/App.axaml</c> 的 VerticalSmallScrollThumbScaleTransform /
/// HorizontalSmallScrollThumbScaleTransform (成因: Fluent 模板内联 RenderTransformOrigin
/// 以右缘/下缘为缩放原点, 且该内联值优先级高于应用级样式, 改不动 ⇒ 只能覆盖变换资源)。
/// 本文件同时守卫「字面量耦合」—— -4.375px 由 ScrollBarSize=10 与 Fluent 的 0.125 推出,
/// 三者任一改动都会让 ①②③ 里的居中断言变红。
/// </summary>
public sealed class ScrollBarStylesTests
{
    /// <summary>与 App.axaml 的 ScrollBarSize 保持一致 (由 App_Axaml_Declares_Centered_Thumb_Transform 文本校验防漂移)。</summary>
    private const double ScrollBarSize = 10.0;

    private const string VerticalTransform = "scaleX(0.125) translateX(-4.375px)";

    private const string HorizontalTransform = "scaleY(0.125) translateY(-4.375px)";

    private static readonly string[] ResourceKeys =
    {
        "ScrollBarSize",
        "VerticalSmallScrollThumbScaleTransform",
        "HorizontalSmallScrollThumbScaleTransform",
    };

    // ---------------------------------------------------------------- 文本契约 (App.axaml)

    /// <summary>
    /// App.axaml 必须真的声明这三个键与对应字面量。
    /// 必要性: 测试夹具不加载 App.axaml (它只装配 FluentTheme + 皮肤), 故下方几何用例是
    /// 「镜像」资源; 没有本条校验, 只改 App.axaml 而忘记同步常量时, 几何用例仍会假绿。
    /// </summary>
    [Fact]
    public void App_Axaml_Declares_Centered_Thumb_Transform()
    {
        var xaml = File.ReadAllText(AppAxamlPath());

        Assert.Contains($"<x:Double x:Key=\"ScrollBarSize\">{ScrollBarSize}</x:Double>", xaml);
        Assert.Contains(
            $"<TransformOperations x:Key=\"VerticalSmallScrollThumbScaleTransform\">{VerticalTransform}</TransformOperations>",
            xaml);
        Assert.Contains(
            $"<TransformOperations x:Key=\"HorizontalSmallScrollThumbScaleTransform\">{HorizontalTransform}</TransformOperations>",
            xaml);

        // 原值是贴边版 (以右缘/下缘为缩放原点), 回归锁: 元素行不得复活旧字面量
        // (只断言元素行 —— App.axaml 的注释里为说明成因引用了旧值, 全文断言会误伤)
        Assert.DoesNotContain(
            "<TransformOperations x:Key=\"VerticalSmallScrollThumbScaleTransform\">scaleX(0.125) translateX(-2px)</TransformOperations>",
            xaml);
        Assert.DoesNotContain(
            "<TransformOperations x:Key=\"HorizontalSmallScrollThumbScaleTransform\">scaleY(0.125) translateY(-2px)</TransformOperations>",
            xaml);
    }

    // ---------------------------------------------------------------- 几何契约

    /// <summary>
    /// ① 收起态细条水平居中: 左余量与右余量相等, 且细条确实比滚动条带窄。
    /// 这条同时是「不再贴右缘」的直接断言 —— 旧值下右余量为 2px、左余量为 6.75px。
    /// </summary>
    [AvaloniaFact]
    public void Collapsed_Thumb_Is_Centered_In_Scroll_Bar_Band()
    {
        var snap = SnapshotResources();
        try
        {
            ApplyScrollBarResources();
            var (win, _, bar, thumb, track) = BuildScrollHost();
            try
            {
                Assert.False(bar.IsExpanded, "未悬停时滚动条不应处于展开态");

                double band = track.Bounds.Width;
                var (left, right) = RenderedX(thumb);
                double leftMargin = left;
                double rightMargin = band - right;

                Assert.True(right - left < band - 0.5,
                    $"收起态应为细条 (应窄于滚动条带 {band:F2}), 实测宽 {right - left:F2}");
                Assert.True(Math.Abs(leftMargin - rightMargin) <= 0.5,
                    $"细条未居中: 左余量={leftMargin:F3} 右余量={rightMargin:F3} (带宽={band:F2})");
                Assert.True(rightMargin >= 1.0,
                    $"细条仍贴右缘: 右余量={rightMargin:F3} (应 >= 1px)");
            }
            finally { win.Close(); }
        }
        finally { RestoreResources(snap); }
    }

    /// <summary>
    /// ② + ③ 悬停展开: 逐帧采样, 宽度必须变大, 且**每一帧**的中心都等于滚动条带中心。
    /// 只断言终态是不够的 —— 真实过渡是动画, 用户看到的是过程; 中心恒定等价于
    /// 「两侧等量外扩」, 这正是"向两侧扩展"的形式化表述。
    /// </summary>
    [AvaloniaFact]
    public void Hover_Expands_Thumb_Symmetrically_To_Both_Sides()
    {
        var snap = SnapshotResources();
        try
        {
            ApplyScrollBarResources();
            var (win, _, bar, thumb, track) = BuildScrollHost();
            try
            {
                double band = track.Bounds.Width;
                double bandCenter = band / 2;

                var samples = new List<(double Width, double Center)>();
                var (l0, r0) = RenderedX(thumb);
                samples.Add((r0 - l0, (l0 + r0) / 2));

                var target = bar.TranslatePoint(
                    new Point(bar.Bounds.Width / 2, bar.Bounds.Height / 2), win)!.Value;
                win.MouseMove(target);
                Dispatcher.UIThread.RunJobs();

                for (int i = 0; i < 30; i++)
                {
                    AvaloniaHeadlessPlatform.ForceRenderTimerTick(2);
                    Dispatcher.UIThread.RunJobs();
                    var (l, r) = RenderedX(thumb);
                    samples.Add((r - l, (l + r) / 2));
                }

                Assert.True(bar.IsExpanded, "指针悬停后滚动条应进入展开态");
                Assert.True(bar.IsPointerOver, "指针应确实落在滚动条上");

                double widest = samples.Max(s => s.Width);
                Assert.True(widest > samples[0].Width + 1.0,
                    $"悬停后细条应变粗: 起始宽={samples[0].Width:F3} 最大宽={widest:F3}");

                foreach (var (width, center) in samples)
                {
                    Assert.True(Math.Abs(center - bandCenter) <= 0.5,
                        $"展开过程中细条中心偏移: 中心={center:F3} 带中心={bandCenter:F3} 宽={width:F3} " +
                        "(两侧不等量外扩即为此断言失败)");
                }
            }
            finally { win.Close(); }
        }
        finally { RestoreResources(snap); }
    }

    /// <summary>
    /// ④ 改动不得影响滚动本身: 滚动条仍能驱动滚动 (设 Value = 拖拽等价操作),
    /// 行滚动 (滚轮/键盘等价路径) 也仍生效。
    /// </summary>
    [AvaloniaFact]
    public void Scroll_Still_Works_After_Thumb_Restyle()
    {
        var snap = SnapshotResources();
        try
        {
            ApplyScrollBarResources();
            var (win, sv, bar, _, _) = BuildScrollHost();
            try
            {
                Assert.True(sv.Extent.Height > sv.Viewport.Height,
                    $"夹具应可滚动: Extent={sv.Extent.Height:F1} Viewport={sv.Viewport.Height:F1}");

                // 拖动滚动条等价: 直接设滚动条值, 视口偏移必须跟随
                bar.Value = 240;
                Dispatcher.UIThread.RunJobs();
                Assert.True(Math.Abs(sv.Offset.Y - 240) < 1.0,
                    $"滚动条未驱动滚动: Offset.Y={sv.Offset.Y:F1} (设 Value=240)");

                // 行滚动: Offset 必须前进
                double before = sv.Offset.Y;
                sv.LineDown();
                Dispatcher.UIThread.RunJobs();
                Assert.True(sv.Offset.Y > before,
                    $"行滚动失效: {before:F1} -> {sv.Offset.Y:F1}");
            }
            finally { win.Close(); }
        }
        finally { RestoreResources(snap); }
    }

    // ---------------------------------------------------------------- 夹具与工具

    /// <summary>
    /// 复刻插件页/选中动作页的滚动区形态: 全网宽 ScrollViewer + MaxWidth 左对齐内容 + 高内容。
    /// 内容是否左对齐/多宽与本契约无关, 但保留它可让「细条居中于条带」与「内容右缘」形成对照,
    /// 避免后来者把它误读成"居中于内容"。
    /// </summary>
    private static (Window Win, ScrollViewer Sv, ScrollBar Bar, Thumb Thumb, Track Track) BuildScrollHost()
    {
        var win = new Window { Width = 1200, Height = 800, Background = Brushes.White };

        var inner = new StackPanel
        {
            Margin = new Thickness(36, 32, 36, 40),
            Spacing = 16,
            MaxWidth = 820,
            HorizontalAlignment = HorizontalAlignment.Left,
        };
        for (int i = 0; i < 40; i++)
        {
            inner.Children.Add(new Border
            {
                Height = 44,
                Margin = new Thickness(0, 0, 0, 8),
                Background = Brushes.LightGray,
            });
        }

        var sv = new ScrollViewer { Background = Brushes.Transparent, Content = inner };
        win.Content = sv;
        win.Show();
        Dispatcher.UIThread.RunJobs();

        var bar = sv.GetVisualDescendants().OfType<ScrollBar>()
            .First(b => b.Orientation == Orientation.Vertical);
        // 镜像 Styles/ScrollBarStyles.axaml: 把 Fluent 默认的 ShowDelay=0.5s / HideDelay=2s 归零。
        // 夹具不加载该样式文件, 不镜像则"悬停要等 500ms 真实时间才展开", 逐帧断言会误判为未展开。
        bar.ShowDelay = TimeSpan.Zero;
        bar.HideDelay = TimeSpan.Zero;
        var thumb = bar.GetVisualDescendants().OfType<Thumb>().First();
        var track = bar.GetVisualDescendants().OfType<Track>().First();
        return (win, sv, bar, thumb, track);
    }

    /// <summary>
    /// 解算 Thumb 的渲染 X 范围 (相对其父级 Track): 布局 Bounds + RenderTransform + 缩放原点。
    /// 必须自己算了看 Bounds —— Fluent 的"细条/展开"是 RenderTransform 缩放, Bounds 恒为 ScrollBarSize,
    /// 直接读 Bounds 会把两种状态读成一样 (2026-09-18 实测踩过)。
    /// </summary>
    private static (double Left, double Right) RenderedX(Thumb thumb)
    {
        double w = thumb.Bounds.Width;
        Matrix m = (thumb.RenderTransform as TransformOperations)?.Value ?? Matrix.Identity;
        var origin = thumb.RenderTransformOrigin;
        double ox = origin.Unit == RelativeUnit.Absolute ? origin.Point.X : w * origin.Point.X;
        double MapX(double x) => m.M11 * (x - ox) + m.M31 + ox;
        return (MapX(0), MapX(w));
    }

    private static void ApplyScrollBarResources()
    {
        var res = Application.Current!.Resources;
        res["ScrollBarSize"] = ScrollBarSize;
        res["VerticalSmallScrollThumbScaleTransform"] = TransformOperations.Parse(VerticalTransform);
        res["HorizontalSmallScrollThumbScaleTransform"] = TransformOperations.Parse(HorizontalTransform);
    }

    /// <summary>
    /// 夹具不加载 App.axaml, 故在此镜像上述三个资源; 用快照/还原避免污染同进程的其它用例
    /// (滚动条宽度会改变全部页面的量测结果)。
    /// </summary>
    private static Dictionary<string, object?> SnapshotResources()
    {
        var app = Application.Current!;
        var snap = new Dictionary<string, object?>();
        foreach (var key in ResourceKeys)
        {
            snap[key] = app.TryFindResource(key, out var value) ? value : null;
        }
        return snap;
    }

    private static void RestoreResources(Dictionary<string, object?> snap)
    {
        var res = Application.Current!.Resources;
        foreach (var (key, value) in snap)
        {
            if (value is null) res.Remove(key);
            else res[key] = value;
        }
    }

    private static string AppAxamlPath() =>
        Path.Combine(RepoRoot(), "config-ui-avalonia", "App.axaml");

    private static string RepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException(
            "找不到仓库根 (含 config-ui-avalonia 的目录), BaseDirectory=" + AppContext.BaseDirectory);
    }
}
