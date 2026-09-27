using System;
using System.Linq;
using System.Reflection;
using System.Threading.Tasks;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.VisualTree;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views.Controls;

/// <summary>
/// 下拉框展开/折叠动效 (2026-09-26 用户要求「与选项页设置列表组件框同款」):
/// 打开时 PopupBorder 高度 0→自然高 卷轴摊开 (100ms SineEaseInOut), 关闭时反向卷起。
/// 高度揭示机制复用 <see cref="RevealHeightMotion"/> —— 与选项页揭示动效同一引擎、同一
/// 时长令牌 (<see cref="ClaudeMotion.Unroll"/>/<see cref="ClaudeMotion.Roll"/>)、同一缓动。
///
/// <para><b>与选项页同款的取舍。</b> 保留: 高度揭示 (真·尺寸收缩) + 100ms + SineEaseInOut +
/// 内层内容 3px 落平 (类驱动 XAML 动画, 样式见 Claude.axaml comboPopup 段)。
/// 省略: 16px 卷曲光影带 —— 它是选项页模板里的专用元素, ComboBox 的 Popup 模板不可注入。</para>
///
/// <para><b>折叠为什么要反射内部 Closing 事件。</b> Avalonia 11.3 的 Popup.Closing 是 internal
/// —— 不订阅它就没有「popup 已决定关闭、尚未隐藏」的插入点 (DropDownClosed 触发时内容已随
/// PopupRoot 卸载, 只能看着它瞬间消失)。反射只碰这一个事件名 (Avalonia 锁 11.3.20);
/// 升级后若事件缺失, 钩子静默跳过 = 折叠退化为瞬时关闭, 失败模式可接受。</para>
///
/// <para><b>关闭协议。</b> 选中/失焦/按 Esc 触发关闭 → CloseCore 先发 Closing: 拦下
/// (Cancel=true) → 高度卷起 → 完成后 <see cref="Popup.Close"/> 二次触发 Closing, 以
/// IgnoreClose 标记放行。折叠中途再点开: DropDownOpened 先撤折叠动画 (取消即放行),
/// 不留半卷状态。</para>
///
/// <para><b>安全面。</b> 只写 popup 自身模板部件 (PopupBorder) 的 MaxHeight/类名, 不碰
/// 业务状态; 无 IO; 反射仅订阅一个内部事件; 每 combo 一份状态 (ConditionalWeakTable,
/// 与 SectionUnroll 同款弱持有, 页面重建不留悬挂引用)。</para>
/// </summary>
public static class ComboDropdownMotion
{
    private const string UnrollClass = "comboPopupUnroll";
    private const string RollUpClass = "comboPopupRollup";
    private const string ClosingEventName = "Closing";

    private sealed class State
    {
        public bool Wired;
        public bool IgnoreClose; // 我们主动 Popup.Close 的第二轮 Closing: 放行
        public CancellationTokenSource? Cts;
        public Border? PopupBorder;
    }

    private static readonly System.Runtime.CompilerServices.ConditionalWeakTable<ComboBox, State> States = new();

    /// <summary>为下拉框接上展开/折叠动效 (幂等; 页面 OnLoaded 逐个调用)。</summary>
    public static void Attach(ComboBox combo)
    {
        var state = States.GetValue(combo, _ => new State());
        if (state.Wired) return;
        state.Wired = true;

        combo.DropDownOpened += (_, _) => OnDropDownOpened(state);
        // Popup 在模板内: 挂树后才可枚举; 顺带反射订阅 internal Closing (见类注释)
        combo.AttachedToVisualTree += (_, _) =>
        {
            var popup = combo.GetVisualDescendants().OfType<Popup>().FirstOrDefault();
            if (popup is null) return;

            state.PopupBorder = popup.Child as Border;
            var closing = typeof(Popup).GetEvent(ClosingEventName,
                BindingFlags.Instance | BindingFlags.NonPublic);
            closing?.AddEventHandler(popup, new EventHandler<System.ComponentModel.CancelEventArgs>(
                (_, e) => OnPopupClosing(popup, state, e)));
        };
    }

    // ------------------------------------------------------------- 展开

    private static void OnDropDownOpened(State state)
    {
        var border = state.PopupBorder;
        if (border is null) return;

        // 撤掉在飞的折叠 (折叠中途再点开): 取消即放行, 不留半卷状态
        state.Cts?.Cancel();
        state.Cts = null;
        state.IgnoreClose = false;
        border.Classes.Remove(RollUpClass);

        if (!MotionPreferences.AnimationsEnabled) return; // 无动效档: popup 原样全高展开

        // 先把高度归零 (同步, 抢在首帧渲染前), 再量自然高摊开; 内层内容同步淡入
        var inner = border.Child as Control;
        border.MaxHeight = 0;
        var natural = RevealHeightMotion.MeasureNaturalHeight(border);
        if (!RevealHeightMotion.CanDrive(natural))
        {
            border.MaxHeight = double.PositiveInfinity;
            return;
        }

        var cts = new CancellationTokenSource();
        state.Cts = cts;
        if (inner is not null) PlayFade(inner, 0, 1, ClaudeMotion.Unroll, cts.Token);
        _ = RunAsync(border, 0, natural, ClaudeMotion.Unroll, cts, state,
            done: () => border.MaxHeight = double.PositiveInfinity); // 展开完放开上限, 列表可自由滚动
    }

    // ------------------------------------------------------------- 折叠

    private static void OnPopupClosing(Popup popup, State state, System.ComponentModel.CancelEventArgs e)
    {
        if (state.IgnoreClose) return; // 我们折叠完主动 Close 的第二轮: 放行

        var border = state.PopupBorder;
        if (border is null || !MotionPreferences.AnimationsEnabled) return; // 无动效档: 直接关

        var from = border.Bounds.Height;
        if (!RevealHeightMotion.CanDrive(from)) return; // 量不到: 直接关

        e.Cancel = true; // 拦下关闭, 折叠完再 Close
        state.Cts?.Cancel();
        var cts = new CancellationTokenSource();
        state.Cts = cts;
        var inner = border.Child as Control;
        if (inner is not null) PlayFade(inner, inner.Opacity, 0, ClaudeMotion.Roll, cts.Token);
        _ = RunAsync(border, from, 0, ClaudeMotion.Roll, cts, state,
            done: () =>
            {
                border.MaxHeight = double.PositiveInfinity;
                state.IgnoreClose = true; // 第二轮 Closing 放行
                popup.Close();
            });
    }

    // ------------------------------------------------------------- 驱动与收尾

    private static async Task RunAsync(Border border, double from, double target,
        TimeSpan duration, CancellationTokenSource cts, State state, Action done)
    {
        try
        {
            await RevealHeightMotion.AnimateAsync(border, from, target, duration, cts.Token)
                .ConfigureAwait(true);
            if (cts.IsCancellationRequested) return; // 被新状态接管: 收尾让位
            done();
        }
        catch (OperationCanceledException)
        {
            // 被新状态接管
        }
        finally
        {
            cts.Dispose();
            if (ReferenceEquals(state.Cts, cts)) state.Cts = null;
        }
    }

    // ------------------------------------------------------------- 内容淡入淡出

    /// <summary>内层内容淡入/淡出 (Opacity 为双精度基元属性, 代码构造动画器可用;
    /// RenderTransform 的代码动画无动画器会抛, 故落平位移省略 —— 高度揭示为主视觉)。</summary>
    private static void PlayFade(Control inner, double from, double to, TimeSpan duration, CancellationToken token)
    {
        try
        {
            var anim = new Animation { Duration = duration, Easing = RevealHeightMotion.Ease };
            anim.Children.Add(FadeKey(0, from));
            anim.Children.Add(FadeKey(1, to));
            _ = anim.RunAsync(inner, token);
        }
        catch
        {
            // 动画起不来: 跳过 (不遮挡功能性内容)
        }
    }

    private static KeyFrame FadeKey(double cue, double opacity)
    {
        var keyFrame = new KeyFrame { Cue = new Cue(cue) };
        keyFrame.Setters.Add(new Setter(Avalonia.Visual.OpacityProperty, opacity));
        return keyFrame;
    }
}
