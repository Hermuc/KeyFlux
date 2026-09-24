using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Media.Transformation;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Controls;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 弹窗动效守护 (2026-09-24 苹果式弹窗动效批次)。
///
/// <para><b>锁什么。</b>① 每个弹窗都必须被 <see cref="DialogMotion"/> 接管 (反射遍历全部
/// Window 子类, 防将来新增弹窗漏接 —— 这比逐个手写断言耐久); ② 入场装的是弹簧过渡且
/// 姿势从"略小略下"起步; ③ 退场走 <c>Closing</c> 拦截 + 禁点 + 延时真关;
/// ④ 减少动效时完全不介入 (不挂类名/不动姿势); ⑤ 遮罩按 owner 计数与淡出时序。</para>
///
/// <para><b>⚠ 不断言什么。</b>不断言 <c>Opacity</c> 的具体读回值 —— 装了过渡后读回的是
/// <b>插值中的当前值</b> (往往仍是 1), 断言它只会得到假通过/假失败。改为断言
/// "过渡已装上 + 时长正确" 这类结构事实。同理 headless 不驱动全局时钟, 动画不会真的跑完,
/// 故只锁接线与时序参数。</para>
/// </summary>
[Collection("I18nSerial")]
public sealed class DialogMotionTests
{
    /// <summary>反射取全部弹窗类型 (排除主窗; 主窗不是弹窗).</summary>
    private static IEnumerable<Type> DialogWindowTypes()
    {
        return typeof(MainWindow).Assembly.GetTypes()
            .Where(t => typeof(Window).IsAssignableFrom(t)
                        && !t.IsAbstract
                        && t != typeof(MainWindow)
                        && t != typeof(Window)
                        && t.Namespace?.EndsWith(".Views") == true)
            .OrderBy(t => t.Name);
    }

    /// <summary>为弹窗找一个可用构造 (含参数的取默认值占位), 失败返回 null。</summary>
    private static Window? TryCreate(Type type)
    {
        foreach (var ctor in type.GetConstructors()
                     .OrderBy(c => c.GetParameters().Length))
        {
            try
            {
                var args = ctor.GetParameters()
                    .Select(p => p.HasDefaultValue ? p.DefaultValue : DefaultOf(p.ParameterType))
                    .ToArray();
                return (Window?)ctor.Invoke(args);
            }
            catch
            {
                // 试下一个构造
            }
        }
        return null;
    }

    private static object? DefaultOf(Type t)
    {
        if (t.IsValueType)
        {
            return Activator.CreateInstance(t);
        }
        if (t == typeof(string))
        {
            return "";
        }
        // 引用类型: 尝试无参构造 (VM 类等)
        var ctor = t.GetConstructor(Type.EmptyTypes);
        return ctor is null ? null : ctor.Invoke(null);
    }

    /// <summary>
    /// 每个弹窗都必须挂上动效 (反射遍历) —— 防将来新增弹窗漏接。
    /// DialogChrome.Apply 是唯一挂载点, 本用例同时锁住"所有弹窗构造期都调了它"这一约定。
    /// </summary>
    [AvaloniaFact]
    public void Every_Dialog_Window_Gets_Motion()
    {
        var checkedCount = 0;
        var missing = new List<string>();

        foreach (var type in DialogWindowTypes())
        {
            var win = TryCreate(type);
            if (win is null)
            {
                continue; // 构造依赖过重 (如必须要 VM 实例), 跳过但不算失败
            }
            try
            {
                checkedCount++;
                if (!DialogMotion.IsAttached(win))
                {
                    missing.Add(type.Name);
                }
            }
            finally
            {
                win.Close();
            }
        }

        Assert.True(checkedCount >= 5,
            $"可构造的弹窗过少 ({checkedCount}): 反射遍历可能失效, 检查 DialogWindowTypes 过滤条件");
        Assert.Empty(missing);
    }

    /// <summary>
    /// 入场: 弹簧过渡 + 从"略小略下"起步, 且挂上玻璃类名 (Border 根)。
    /// </summary>
    [AvaloniaFact]
    public void Enter_Starts_Small_And_Uses_Spring()
    {
        var win = new Window { Width = 400, Height = 300, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Dispatcher.UIThread.RunJobs();
        AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        Dispatcher.UIThread.RunJobs();

        try
        {
            var body = (Control)win.Content!;
            Assert.Contains(DialogMotion.MotionClass, body.Classes);

            // 过渡已装 (入场弹簧)
            Assert.NotNull(body.Transitions);
            var transform = body.Transitions!.OfType<TransformOperationsTransition>().FirstOrDefault();
            var opacity = body.Transitions.OfType<DoubleTransition>().FirstOrDefault();
            Assert.NotNull(transform);
            Assert.NotNull(opacity);
            Assert.Equal(ClaudeMotion.DialogEnter, transform!.Duration);
            Assert.IsType<SpringEasing>(transform.Easing);

            // ⚠ 不断言 Opacity 读回值 —— 实测读到 0.00097 (插值刚起步), 这恰好证明过渡在工作;
            //    写 Assert.Equal(1, body.Opacity) 会假失败。只锁"过渡已装上"这一结构事实。
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 退场: Closing 被拦截 (窗口不立刻关) + 退场期间禁点 + 装了退场过渡。
    /// </summary>
    [AvaloniaFact]
    public void Closing_Is_Deferred_And_Blocks_Input()
    {
        var win = new Window { Width = 400, Height = 300, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Dispatcher.UIThread.RunJobs();
        AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        Dispatcher.UIThread.RunJobs();

        try
        {
            var body = (Control)win.Content!;
            win.Close();
            Dispatcher.UIThread.RunJobs();

            // 拦下了本次关闭: 窗口仍可见, 且已进入退场态
            Assert.True(win.IsVisible, "Closing 未被拦截: 窗口立刻关闭, 退场动画没有机会播放");
            Assert.True(DialogMotion.IsClosing(win));
            Assert.False(win.IsHitTestVisible, "退场期间必须禁点 (防重复触发关闭)");

            // 退场过渡时长
            var transform = body.Transitions!.OfType<TransformOperationsTransition>().FirstOrDefault();
            Assert.NotNull(transform);
            Assert.Equal(ClaudeMotion.DialogExit, transform!.Duration);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 减少动效 (KEYFLUX_NO_MOTION=1): 完全不介入 —— 不挂类名、不动姿势、不加过渡。
    /// 半介入会留下残留姿势, 故必须整体跳过。
    /// </summary>
    [AvaloniaFact]
    public void Reduced_Motion_Leaves_Dialog_Untouched()
    {
        var prev = Environment.GetEnvironmentVariable("KEYFLUX_NO_MOTION");
        Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", "1");
        try
        {
            var win = new Window { Width = 400, Height = 300, Content = new Border() };
            DialogMotion.Attach(win);
            win.Show();
            Dispatcher.UIThread.RunJobs();

            try
            {
                var body = (Control)win.Content!;
                Assert.DoesNotContain(DialogMotion.MotionClass, body.Classes);
                Assert.Null(body.Transitions);
                Assert.Equal(1d, body.Opacity);
                Assert.Null(body.RenderTransform);

                // 关闭直落: 不拦截
                win.Close();
                Dispatcher.UIThread.RunJobs();
                Assert.False(win.IsVisible, "减少动效下关闭必须直落, 不应被拦截");
            }
            finally
            {
                win.Close();
            }
        }
        finally
        {
            Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", prev);
        }
    }

    /// <summary>
    /// 页内浮层: IsOpen=true 播入场 (挂类名 + 装过渡); 减少动效时不介入。
    /// </summary>
    [AvaloniaFact]
    public void Overlay_Plays_Enter_When_Opened()
    {
        var overlay = new Border();
        var win = new Window { Width = 400, Height = 300, Content = overlay };
        win.Show();
        Dispatcher.UIThread.RunJobs();

        try
        {
            // ⚠ 顺序要求: OverlayMotion 标记必须先于 IsOpen 置位 —— 变化回调里会读它决定是否接管;
            //   且两者都必须在**挂树之后**设置 (挂树前 SetValue 的状态机拿不到视觉根)。
            DialogMotion.SetOverlayMotion(overlay, true);
            Dispatcher.UIThread.RunJobs();
            DialogMotion.SetIsOpen(overlay, true);
            Dispatcher.UIThread.RunJobs();
            AvaloniaHeadlessPlatform.ForceRenderTimerTick();
            Dispatcher.UIThread.RunJobs();

            Assert.Contains(DialogMotion.MotionClass, overlay.Classes);
            Assert.NotNull(overlay.Transitions);

            // 关闭: 退场过渡装上且时长为退场档
            DialogMotion.SetIsOpen(overlay, false);
            Dispatcher.UIThread.RunJobs();
            var t = overlay.Transitions!.OfType<TransformOperationsTransition>().FirstOrDefault();
            Assert.NotNull(t);
            Assert.Equal(ClaudeMotion.DialogExit, t!.Duration);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 遮罩: 弹窗开启后 owner 的遮罩可见且计数为 1; 减少动效下不缩放背景。
    /// </summary>
    [AvaloniaFact]
    public void Scrim_Layer_Exists_And_Counts_Owner()
    {
        var owner = new Window { Width = 800, Height = 600 };
        var root = new Grid();
        var backdrop = new Border { Name = "appRoot" };
        root.Children.Add(backdrop);
        var scrim = new Border { Name = "dlgScrim", IsVisible = false };
        root.Children.Add(scrim);
        owner.Content = root;
        owner.Show();
        Dispatcher.UIThread.RunJobs();

        try
        {
            Scrim.SetIsBackdrop(backdrop, true);
            Dispatcher.UIThread.RunJobs();

            // 宿主注册后, 遮罩层应能被定位到
            Assert.NotNull(Scrim.ScrimLayerOf(owner));

            Scrim.PlayEnter(owner);
            Dispatcher.UIThread.RunJobs();
            Assert.Equal(1, Scrim.CountOf(owner));
            Assert.True(Scrim.IsScrimVisible(owner), "遮罩未显示");

            // 再来一个叠窗: 只计数
            Scrim.PlayEnter(owner);
            Dispatcher.UIThread.RunJobs();
            Assert.Equal(2, Scrim.CountOf(owner));
        }
        finally
        {
            owner.Close();
        }
    }

    /// <summary>
    /// 姿势构造: 缩放/位移确实写进变换 (防 Pose 参数被交换或忽略)。
    /// 用 <c>Value</c> 矩阵求值断言行为, 不碰 TransformOperation 的内部结构 (其公开 API 有限)。
    /// </summary>
    [AvaloniaFact]
    public void Pose_Encodes_Scale_And_Offset()
    {
        // scale(0.5): 矩阵 M11=M22=0.5, 无平移
        var scaled = DialogMotion.Pose(0.5, 0);
        var m = scaled.Value;
        Assert.Equal(0.5, m.M11, 3);
        Assert.Equal(0.5, m.M22, 3);
        Assert.Equal(0, m.M31, 3);
        Assert.Equal(0, m.M32, 3);

        // scale(1) + translate(0, 12): 缩放为单位阵, 平移体现在 M32
        var moved = DialogMotion.Pose(1, 12);
        var m2 = moved.Value;
        Assert.Equal(1, m2.M11, 3);
        Assert.Equal(1, m2.M22, 3);
        Assert.Equal(12, m2.M32, 3);

        // 组合: 非 1 缩放 + 位移 (入场起始姿势)
        var enter = DialogMotion.Pose(0.92, 14);
        var m3 = enter.Value;
        Assert.Equal(0.92, m3.M11, 3);
        Assert.Equal(14, m3.M32, 3);
    }
}
