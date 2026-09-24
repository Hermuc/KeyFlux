using System;
using System.Globalization;
using System.Runtime.CompilerServices;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Media.Transformation;
using Avalonia.Threading;
using Avalonia.VisualTree;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 弹窗「生长」动效 (2026-09-24 用户从四方案中选定 **B 生长**; 统一覆盖设置面板所有弹窗)。
///
/// <para><b>运动语言。</b> 开: 内容自**触发方向**长出 —— 起始 <c>translateY(∓8px) scaleY(0.95)</c>
/// (顶/底边为缩放原点) 淡入到归位; 关: 沿同一方向缩回并淡出, **动画跑完才真正 Close**。
/// 触发方向在构造期由「owner 当前焦点元素」相对 owner 中心的位置推断 (点开弹窗的那个按钮通常
/// 刚拿到焦点): 焦点在上半 ⇒ 弹窗在其下方 ⇒ 自上向下长出; 在下半则反之。取不到锚点时退化为顶边中点。</para>
///
/// <para><b>为什么只动「窗口内容根」, 而不动窗口本身、也不额外包一层。</b>
/// ① 弹窗保留系统标题栏 (DialogChrome 既定决策) ⇒ 标题栏不参与内容变换, 窗口级整体缩放做不到;
///    真正做得到的是"框先出现、内容长出", 故只动内容。
/// ② **刻意不包层**: 包一层 Border 会改变可视树形状, 实测直接打破既有结构断言
///    (MatchTypesLayoutTests ×3 与 PluginMarketWindow 构造测试都按 <c>Window.Content</c> 取根) ——
///    零结构改动在这里比"独占属性"更重要。
/// ③ 代价与约束 (由守卫测试锁): 内容根**不得自带** <c>Transitions</c>/<c>RenderTransform</c>,
///    否则本动效会与其争用同一组属性; 现 8 个弹窗的内容根均未占用 (PluginMarket 的 4 处
///    样式只作用于窗内卡片, 不涉及根)。挂载后给内容根打 <see cref="MotionClass"/> 类名做标记。
/// ④ 起始姿势在构造函数里落位 (Show 之前) ⇒ 首帧就是"未长出"状态, 不会先闪一下全尺寸。</para>
///
/// <para><b>为什么用 <c>Transitions</c> 而不是 XAML 关键帧动画。</b>
/// 10 处弹窗只需一套机制: 代码装过渡 → 改属性触发, 时长/幅度按档位参数化, 且**零 XAML 改动**。
/// 关键约束 (本工程实测过): 代码构造的 <c>Animation</c> 走动画器兜底表, <c>RenderTransform</c>
/// 没有兜底会直接抛; 而 <c>TransformOperationsTransition</c> 自带插值器, 不受此限 (皮肤里的
/// 按压微动效用的是同款机制)。过渡的安装时刻完全由本类掌控 (构造时装好、Opened 才改属性)
/// ⇒ 不存在"先改属性后装过渡"的竞态。</para>
///
/// <para><b>关闭路径唯一化。</b> Esc / 系统 ✕ / 取消 / 确认 / 关闭按钮全部经 <c>Closing</c>:
/// 首次拦截 (<c>e.Cancel</c>) → 播退场 → 计时结束后再 <c>Close()</c>; 第二次 Closing 由
/// <see cref="State.Closing"/> 放行 ⇒ 无递归。退场期间 <c>IsHitTestVisible=false</c> 防误点。
/// 因为"真关"被推迟到退场之后, <c>Closed</c> 里既有的一切 (DialogChrome 撤背景模糊、owner 计数)
/// 时序天然正确 —— 不会出现"背景先变清晰、弹窗还在淡出"。</para>
///
/// <para><b>减少动效。</b> <c>KEYFLUX_NO_MOTION=1</c> (MotionPreferences) 时不拦 Closing、不装过渡
/// ⇒ 即刻开合、零延时。</para>
/// </summary>
public static class DialogMotion
{
    /// <summary>档位: 按弹窗体量与出现频率分档 (由各弹窗构造期显式指定, 不做类型嗅探)。</summary>
    public enum Profile
    {
        /// <summary>标准档: 表单小窗 (动作编辑 / 插件设置 / 页内浮层)。</summary>
        Standard,

        /// <summary>大窗档: 内容重 (行为库 / 匹配类型 / 插件市场 / 概览 / 窗口组) ⇒ 幅度收小。</summary>
        Large,

        /// <summary>高频档: 短命弹窗 (快捷切换 / 保存失败提示) ⇒ 时长短、幅度小, 不拖手感。</summary>
        Quick,
    }

    /// <summary>挂载标记类名 (守卫测试据此识别"已接弹窗动效"; 不改变可视树形状)。</summary>
    public const string MotionClass = "dlgMotion";

    /// <summary>归位姿势 (过渡的终点: 位移 0、纵向缩放 1)。</summary>
    private static readonly TransformOperations RestPose = TransformOperations.Parse("translateY(0px) scaleY(1)");

    /// <summary>退场收尾余量: 过渡时钟起步比属性赋值晚一帧, 恰好卡时长会切掉末帧。</summary>
    private static readonly TimeSpan FinishPad = TimeSpan.FromMilliseconds(50);

    /// <summary>进场缓动 (起止都柔, 与全局动效语言一致)。</summary>
    private static readonly Easing EnterEase = Easing.Parse("CubicEaseOut");

    /// <summary>退场缓动 (加速抽走, 让位不做主角)。</summary>
    private static readonly Easing ExitEase = Easing.Parse("CubicEaseIn");

    private sealed class State
    {
        public Control? Body;
        public Profile Profile;
        public bool AnchorAtBottom;

        /// <summary>已在下场 (Closing 被拦过一次): 第二次 Closing 必须放行, 否则关不掉。</summary>
        public bool Closing;

        /// <summary>页内浮层的延时隐藏句柄 (浮层没有窗口 Closing 可拦, 靠计时收尾)。</summary>
        public IDisposable? Hide;
    }

    private static readonly ConditionalWeakTable<Window, State> States = new();

    /// <summary>取该内容根所属档位的退场时长 (守卫测试对账用; 非动效路径不读它)。</summary>
    internal static TimeSpan ExitDurationOf(Control body)
    {
        foreach (var entry in States)
        {
            if (ReferenceEquals(entry.Value.Body, body)) return Spec(entry.Value.Profile).Exit;
        }
        return Spec(Profile.Standard).Exit;
    }

    /// <summary>某档位的姿势与时长 (位移 px / 纵向缩放 / 进场 / 退场)。</summary>
    private static (double OffsetY, double ScaleY, TimeSpan Enter, TimeSpan Exit) Spec(Profile profile) => profile switch
    {
        Profile.Quick => (6, 0.97, TimeSpan.FromMilliseconds(130), TimeSpan.FromMilliseconds(85)),
        Profile.Large => (6, 0.97, TimeSpan.FromMilliseconds(170), TimeSpan.FromMilliseconds(120)),
        _ => (8, 0.95, TimeSpan.FromMilliseconds(190), TimeSpan.FromMilliseconds(135)),
    };

    /// <summary>
    /// 挂载动效 (由弹窗构造函数经 <c>DialogChrome.Apply</c> 调用)。幂等: 同窗口重复调用只更新档位。
    /// </summary>
    public static void Attach(Window window, Profile profile)
    {
        // 减少动效档: 完全不介入 —— 不包层、不挂钩、不落起始姿势, 弹窗保持原样即刻开合
        if (!MotionPreferences.AnimationsEnabled) return;

        var state = States.GetValue(window, _ => new State());
        state.Profile = profile;
        state.AnchorAtBottom = ResolveAnchorAtBottom(window);

        state.Body = window.Content as Control;
        if (state.Body is null) return; // 无内容根 (理论上不会): 静默跳过, 不影响开关
        state.Body.Classes.Add(MotionClass);

        // ② 起始姿势先落位 (窗口尚未显示 ⇒ 不会闪); 过渡等 Opened 再装
        state.Body.RenderTransformOrigin = new RelativePoint(0.5, state.AnchorAtBottom ? 1 : 0, RelativeUnit.Relative);
        state.Body.RenderTransform = Pose(state);
        state.Body.Opacity = 0;

        window.Opened += (_, _) => PlayEnter(state);
        window.Closing += (_, e) => OnClosing(window, state, e);
    }

    /// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
    public sealed class DialogMotionMarker
    {
    }

    /// <summary>
    /// 页内浮层是否展开 (替换原先裸绑 <c>IsVisible</c> 的写法)。
    /// <para><b>为什么是 <c>bool?</c>:</b> 与分区体动效同一理由 —— <c>bool</c> 的默认值 false 与
    /// "收起"同值 ⇒ 收起态的浮层在装载期不产生变化通知, 状态机收不到任何信号, 只能永远显示。
    /// 可空默认 null 与两个真实状态都不同 ⇒ 初值必定产生一次通知, <c>OldValue is null</c> 即"初始化"。</para>
    /// </summary>
    public static readonly AttachedProperty<bool?> IsOpenProperty =
        AvaloniaProperty.RegisterAttached<DialogMotionMarker, Control, bool?>("IsOpen");

    public static bool? GetIsOpen(Control host) => host.GetValue(IsOpenProperty);

    public static void SetIsOpen(Control host, bool? value) => host.SetValue(IsOpenProperty, value);

    private static readonly ConditionalWeakTable<Control, State> Overlays = new();

    static DialogMotion()
    {
        IsOpenProperty.Changed.AddClassHandler<Control>(
            (host, e) => OnOverlayChanged(host, e.OldValue is null, e.GetNewValue<bool?>()));
    }

    /// <summary>
    /// 页内浮层状态机 (与窗口动效同一套姿势/令牌, 只是"谁负责隐藏"不同):
    /// 浮层没有窗口 <c>Closing</c> 可拦, 故退场跑完由计时器置 <c>IsVisible=false</c>。
    /// 首次赋值 (装载期初值) 只落终态, 不播动画 —— 与卡片动效同一规则, 避免开页时抢戏。
    /// </summary>
    private static void OnOverlayChanged(Control host, bool isFirstValue, bool? open)
    {
        if (open is not { } wantsOpen) return; // 未赋值: 忽略而非猜

        var state = Overlays.GetValue(host, _ => new State());
        var (_, _, enter, exit) = Spec(state.Profile);

        if (isFirstValue)
        {
            host.RenderTransformOrigin = new RelativePoint(0.5, 0, RelativeUnit.Relative);
            host.RenderTransform = wantsOpen ? RestPose : Pose(state);
            host.Opacity = wantsOpen ? 1 : 0;
            host.IsVisible = wantsOpen;
            return;
        }

        state.Hide?.Dispose();
        state.Hide = null;

        if (!MotionPreferences.AnimationsEnabled)
        {
            // 无动效档: 直落终态 (不装过渡, 零延时)
            host.IsVisible = wantsOpen;
            host.Opacity = wantsOpen ? 1 : 0;
            host.RenderTransform = null;
            return;
        }

        if (wantsOpen)
        {
            host.IsVisible = true; // 先显示才看得到过渡; 起点姿势已在隐藏态就位
            host.Transitions = PoseTransitions(enter, EnterEase);
            host.RenderTransform = RestPose;
            host.Opacity = 1;
            return;
        }

        host.IsHitTestVisible = false; // 退场期间不吃点击 (浮层是整页透明层, 必须挡掉)
        host.Transitions = PoseTransitions(exit, ExitEase);
        host.RenderTransform = Pose(state);
        host.Opacity = 0;
        state.Hide = DispatcherTimer.RunOnce(
            () =>
            {
                host.IsVisible = false;
                host.IsHitTestVisible = true;
                host.Transitions = null; // 释放过渡, 起点姿势保留 (下次进场从它出发)
                state.Hide = null;
            },
            exit + FinishPad,
            DispatcherPriority.Normal);
    }

    // ------------------------------------------------------------- 进场 / 退场

    private static void PlayEnter(State state)
    {
        if (state.Body is not { } body) return;
        if (!MotionPreferences.AnimationsEnabled)
        {
            body.Opacity = 1; // 无动效档: 不起过渡, 直落终态 (起始姿势在构造期已落位)
            return;
        }

        var (_, _, enter, _) = Spec(state.Profile);
        body.Transitions = PoseTransitions(enter, EnterEase);
        body.RenderTransform = RestPose;
        body.Opacity = 1;
    }

    private static void OnClosing(Window window, State state, WindowClosingEventArgs e)
    {
        // 第二次 Closing (退场结束后的真关) / body 缺失 / 无动效档 ⇒ 放行
        if (state.Closing || state.Body is not { } body || !MotionPreferences.AnimationsEnabled) return;

        e.Cancel = true;               // 先拦下, 播完退场再真关
        state.Closing = true;
        body.IsHitTestVisible = false; // 退场期间不吃点击

        var (_, _, _, exit) = Spec(state.Profile);
        body.Transitions = PoseTransitions(exit, ExitEase);
        body.RenderTransform = Pose(state);
        body.Opacity = 0;

        // 收尾真关: 加 IsVisible 守卫 —— 退场期间若被外部(owner 关闭/应用退出)直接关掉,
        // 这里再关一次会对已关闭窗口操作, 反而制造异常
        DispatcherTimer.RunOnce(
            () => { if (window.IsVisible) window.Close(); },
            exit + FinishPad,
            DispatcherPriority.Normal);
    }

    private static Transitions PoseTransitions(TimeSpan duration, Easing easing) => new()
    {
        new TransformOperationsTransition { Property = Visual.RenderTransformProperty, Duration = duration, Easing = easing },
        new DoubleTransition { Property = Visual.OpacityProperty, Duration = duration, Easing = easing },
    };

    // ------------------------------------------------------------- 姿势 / 锚点

    /// <summary>
    /// 起始 (= 退场终点) 姿势: 沿锚点方向偏离 + 纵向压缩。
    /// 位移方向必须与缩放原点同侧 —— 顶边原点配负位移 (自上长出), 底边原点配正位移 (自下长出);
    /// 否则"压缩"与"位移"方向相反, 观感是往下坠而不是长出。
    /// </summary>
    private static TransformOperations Pose(State state)
    {
        var (offset, scale, _, _) = Spec(state.Profile);
        var dir = state.AnchorAtBottom ? offset : -offset;
        return TransformOperations.Parse(string.Create(CultureInfo.InvariantCulture,
            $"translateY({dir:0.0}px) scaleY({scale:0.00})"));
    }

    /// <summary>
    /// 锚点是否在下方 (决定"自上长出"还是"自下长出")。
    /// 判据: owner 当前焦点元素 (通常就是刚被点击、因而获得焦点的触发按钮) 中心相对 owner 中心的
    /// 上下关系 —— 弹窗 CenterOwner 居中于 owner, 故焦点在上半 ⇒ 弹窗在其下方 ⇒ 应自上向下长出。
    /// 取不到 (无 owner / 无焦点 / 无视觉根 / 无屏幕坐标) 时退化为"自上向下"。
    /// </summary>
    private static bool ResolveAnchorAtBottom(Window window)
    {
        try
        {
            if (window.Owner is not { } owner) return false;
            if (owner.FocusManager?.GetFocusedElement() is not Visual anchor) return false;
            if (owner.GetVisualRoot() is not Visual root) return false;

            var anchorCenter = anchor.PointToScreen(new Point(anchor.Bounds.Width / 2, anchor.Bounds.Height / 2));
            var ownerCenter = root.PointToScreen(new Point(root.Bounds.Width / 2, root.Bounds.Height / 2));
            return anchorCenter.Y > ownerCenter.Y;
        }
        catch (Exception)
        {
            return false; // 取不到坐标就按默认方向, 绝不影响开合
        }
    }
}
