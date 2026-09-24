using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Media.Transformation;
using Avalonia.Styling;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Controls;

/// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
public sealed class ScrimMarker;

/// <summary>
/// Scrim —— 弹窗遮罩 + 背景纵深 (2026-09-24 苹果式弹窗动效批次)。
///
/// <para><b>它在做什么。</b>弹窗打开时, 给 owner 主窗的"背景容器"施加一次性缩小
/// (1 → <see cref="ClaudeMotion.ScrimBackdropScale"/>) 并铺一层遮罩 (柔和淡入变暗);
/// 关闭时遮罩<b>稍晚</b>淡出, 避免"画面突然空掉"。</para>
///
/// <para><b>背景为什么不逐帧重新模糊。</b>模糊由既有的 <see cref="Controls.ModalBlur"/>
/// 负责 (挂 <c>BlurEffect</c>)。关键机制 (真 Skia 探针实测, 2026-09-24):
/// 给已挂 <c>BlurEffect</c> 的元素叠加逐帧 <c>RenderTransform</c> 缩放, 每帧成本
/// <b>7.09ms vs 静止模糊 6.03ms = 仅 1.18x</b> —— 因为模糊结果被缓存成图层, 缩放作用于
/// <b>该图层</b>, 而不是"每帧重新模糊变化后的内容"。故"背景略微缩小"是廉价操作,
/// 不需要换 GPU 渲染。探针同时测出无模糊基线 7.10ms (与缩放后持平), 说明缩放本身几乎免费。</para>
///
/// <para><b>为什么遮罩在 MainWindow 里而不是运行时注入。</b>弹窗是独立 Window, 它无法
/// 直接绘制到 owner 的画布上; 而运行时把 Border 塞进 owner 的视觉树需要改结构 (根 Border
/// 只能有一个 Child)。故遮罩层在 MainWindow.axaml 显式声明为 <c>#dlgScrim</c>,
/// 本服务只负责开关与时序 —— 结构显式, 也让"改了什么"在 XAML 里一眼可见。</para>
///
/// <para><b>叠窗与生命周期。</b>遮罩按 owner 计数 (叠窗时只有最外层归零才撤);
/// 全部状态以 <see cref="ConditionalWeakTable{TKey,TValue}"/> 弱持有, 窗口销毁即回收,
/// 不阻止 GC。所有入口须在 UI 线程调用。</para>
/// </summary>
public static class Scrim
{
    /// <summary>遮罩不透明度 (变暗幅度) —— 0.42 是"看得到压暗但背景仍可辨认"的区间。</summary>
    private const double ScrimOpacity = 0.42;

    /// <summary>遮罩底色 (暖黑, 与 Claude 暖色体系一致而非纯黑)。</summary>
    private static readonly Color ScrimColor = Color.Parse("#1a1815");

    private sealed class State
    {
        public Border? ScrimLayer;
        public Control? Backdrop;
        public int Count;
        public bool Reduced;
        public IDisposable? PendingExit;
    }

    private static readonly ConditionalWeakTable<TopLevel, State> States = new();

    /// <summary>
    /// 标在"背景容器"上 (MainWindow.axaml 的 #appRoot): 弹窗开启时它缩小, 遮罩盖在它上面。
    /// </summary>
    public static readonly AttachedProperty<bool> IsBackdropProperty =
        AvaloniaProperty.RegisterAttached<ScrimMarker, Visual, bool>("IsBackdrop");

    public static bool GetIsBackdrop(Visual v) => v.GetValue(IsBackdropProperty);
    public static void SetIsBackdrop(Visual v, bool value) => v.SetValue(IsBackdropProperty, value);

    static Scrim()
    {
        IsBackdropProperty.Changed.AddClassHandler<Visual>((v, e) =>
        {
            if (e.NewValue is true && v.GetVisualRoot() is TopLevel tl)
            {
                var s = States.GetOrCreateValue(tl);
                s.Backdrop = v as Control;
                s.ScrimLayer = FindScrimLayer(v);
            }
        });
    }

    /// <summary>
    /// 从背景容器向上找到同层的遮罩 Border (#dlgScrim)。二者是兄弟 (同一 Grid 的两个子),
    /// 故"向上找父级 Grid, 再取其中名为 dlgScrim 的子"即可, 不需要额外传参。
    /// </summary>
    private static Border? FindScrimLayer(Visual backdrop)
    {
        foreach (var ancestor in backdrop.GetVisualAncestors())
        {
            if (ancestor is Panel panel)
            {
                foreach (var child in panel.Children)
                {
                    if (child is Border { Name: "dlgScrim" } border)
                    {
                        return border;
                    }
                }
            }
        }
        return null;
    }

    /// <summary>弹窗打开: 背景缩小 + 遮罩柔和淡入。</summary>
    public static void PlayEnter(TopLevel owner)
    {
        if (!States.TryGetValue(owner, out var s))
        {
            return;
        }
        s.Count++;
        if (s.Count != 1)
        {
            return; // 叠窗: 已处于开启态, 只计数
        }
        // 取消尚未触发的退场 (快速开关时遮罩不该先撤再入, 否则闪一下)
        s.PendingExit?.Dispose();
        s.PendingExit = null;

        PlayBackdrop(s, shrink: true, ClaudeMotion.ScrimBackdrop, new CubicEaseOut());
        PlayScrim(s, visible: true, ClaudeMotion.ScrimFadeIn, new SineEaseOut());
    }

    /// <summary>弹窗关闭: 遮罩稍晚淡出 (起步延迟 + 更长总时长 ⇒ 弹窗收缩时背景仍是暗的)。</summary>
    public static void PlayExit(TopLevel owner)
    {
        if (!States.TryGetValue(owner, out var s))
        {
            return;
        }
        s.Count = Math.Max(0, s.Count - 1);
        if (s.Count != 0)
        {
            return; // 还有叠窗: 遮罩保持
        }
        s.PendingExit?.Dispose();
        s.PendingExit = ScrimDelay.RunOnce(ClaudeMotion.ScrimExitDelay, () =>
        {
            PlayScrim(s, visible: false, ClaudeMotion.ScrimFadeOut, new SineEaseIn());
            PlayBackdrop(s, shrink: false, ClaudeMotion.ScrimFadeOut, new CubicEaseOut());
        });
    }

    private static void PlayBackdrop(State s, bool shrink, TimeSpan duration, Easing easing)
    {
        if (s.Backdrop is null)
        {
            return;
        }
        if (s.Reduced)
        {
            // 减少动效: 背景不缩放 (遮罩也直落), 保留可读的静态层叠
            s.Backdrop.RenderTransform = null;
            return;
        }

        s.Backdrop.RenderTransformOrigin = RelativePoint.Center;
        s.Backdrop.Transitions = new Transitions
        {
            new TransformOperationsTransition
            {
                Property = Visual.RenderTransformProperty,
                Duration = duration,
                Easing = easing,
            },
        };
        var scale = shrink ? ClaudeMotion.ScrimBackdropScale : 1.0;
        var b = new TransformOperations.Builder(1);
        b.AppendScale(scale, scale);
        s.Backdrop.RenderTransform = b.Build();
    }

    private static void PlayScrim(State s, bool visible, TimeSpan duration, Easing easing)
    {
        if (s.ScrimLayer is null)
        {
            return;
        }
        if (s.Reduced)
        {
            s.ScrimLayer.IsVisible = visible;
            s.ScrimLayer.Opacity = visible ? 1 : 0;
            s.ScrimLayer.IsHitTestVisible = visible;
            return;
        }

        if (visible)
        {
            s.ScrimLayer.Background = new SolidColorBrush(
                ScrimColor, ScrimOpacity);
            s.ScrimLayer.IsVisible = true;
            s.ScrimLayer.IsHitTestVisible = true;
            s.ScrimLayer.Opacity = 0;
            s.ScrimLayer.Transitions = new Transitions
            {
                new DoubleTransition
                {
                    Property = Visual.OpacityProperty,
                    Duration = duration,
                    Easing = easing,
                },
            };
            // 下一帧推终值 (同帧改目标不触发过渡)
            Dispatcher.UIThread.Post(() => s.ScrimLayer.Opacity = 1, DispatcherPriority.Background);
        }
        else
        {
            s.ScrimLayer.Transitions = new Transitions
            {
                new DoubleTransition
                {
                    Property = Visual.OpacityProperty,
                    Duration = duration,
                    Easing = easing,
                },
            };
            s.ScrimLayer.Opacity = 0;
            // 淡出结束才真正隐藏, 期间不吃输入 (弹窗已关, 不应再拦交互)
            s.ScrimLayer.IsHitTestVisible = false;
            var layer = s.ScrimLayer;
            ScrimDelay.RunOnce(duration, () =>
            {
                if (Math.Abs(layer.Opacity) < 0.01)
                {
                    layer.IsVisible = false;
                }
            });
        }
    }

    /// <summary>测试缝: 读某 owner 的遮罩是否可见。</summary>
    internal static bool IsScrimVisible(TopLevel owner) =>
        States.TryGetValue(owner, out var s) && s.ScrimLayer?.IsVisible == true;

    /// <summary>测试缝: 读遮罩层引用。</summary>
    internal static Border? ScrimLayerOf(TopLevel owner) =>
        States.TryGetValue(owner, out var s) ? s.ScrimLayer : null;

    /// <summary>测试缝: 读当前叠窗计数。</summary>
    internal static int CountOf(TopLevel owner) =>
        States.TryGetValue(owner, out var s) ? s.Count : 0;
}

/// <summary>可取消的延时 (用 DispatcherTimer 而非 Task.Delay, 避免异步回调跨线程序列化问题)。</summary>
internal static class ScrimDelay
{
    public static IDisposable RunOnce(TimeSpan delay, Action action)
    {
        var timer = new DispatcherTimer { Interval = delay };
        timer.Tick += (s, _) =>
        {
            timer.Stop();
            action();
        };
        timer.Start();
        return new Stopper(timer);
    }

    private sealed class Stopper(DispatcherTimer timer) : IDisposable
    {
        public void Dispose()
        {
            timer.Stop();
        }
    }
}
