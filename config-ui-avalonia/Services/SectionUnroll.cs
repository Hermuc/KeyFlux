using System;
using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Styling;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Services;

/// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
public sealed class SectionUnrollMarker
{
}

/// <summary>
/// 设置页「选项」页手风琴分区体的**卷轴摊开**状态机 (2026-09-24 二版, 取代一版"淡入+位移")。
///
/// <para><b>观感目标 (用户原话)。</b>「像书卷缓缓展开一样, 展开过程中带有自然的卷曲与铺展感,
/// 过渡顺滑、节奏连贯, 整体呈现出卷轴逐步摊开的视觉体验」。⇒ 三个可分解的技术动作:
/// ① <b>逐步摊开</b> = 揭示层高度从 0 连续长到内容自然高 —— 真·布局增长 (不是缩放/淡入),
///    所以内容是"被纸张边缘逐行露出来", 下方卡片被同时推开;
/// ② <b>铺展感</b> = 内容自身还有极小幅度的"落平" (translateY -5px + scaleY 0.985 → 归位);
/// ③ <b>卷曲</b> = 前缘一条 16px 渐变光影带 + 1px 纸边线, 恒定贴在被揭示的边缘上
///    (靠 Grid + VerticalAlignment=Bottom 免费跟随, 不需要第二条动画去追)。</para>
///
/// <para><b>为什么揭示要用代码驱动, 而卷曲/落平用 XAML。</b>
/// 揭示的终点是**运行时才知道的**内容自然高 (每张卡不同、且语言/数据变化后也会变), 写不进
/// XAML 关键帧 ⇒ 只能在代码里按当前测得的高度构造 Animation 再 RunAsync。反向约束: 代码构造的
/// Animation 走的是 <c>Animation.GetAnimator</c> 兜底表, **只有基元类型属性有兜底**
/// (实测: Double 的 MaxHeight/Opacity 可以, ITransform 的 RenderTransform 会抛
/// "No animator registered for the property RenderTransform") ⇒ 位移/缩放这类
/// RenderTransform 关键帧必须留在 XAML (XAML 编译器会替 setter 注册动画器)。二者分工由此确定。</para>
///
/// <para><b>状态 (两互斥类 + IsVisible + MaxHeight, 由本类独占, 勿在 XAML 再绑)。</b>
/// <list type="bullet">
///   <item><c>IsOpen=true</c>: 显示 → 量自然高 → <c>MaxHeight</c> 0→H 摊开 (420ms) + 挂
///         <c>.unroll</c>; 动画结束把 <c>MaxHeight</c> 落回 ∞ (内容可自由再长高, 如新增行)。</item>
///   <item><c>IsOpen=false</c>: <c>MaxHeight</c> 当前值→0 卷起 (300ms) + 挂 <c>.rollup</c>;
///         卷完才 <c>IsVisible=false</c> (IsVisible 不可动画, 提前隐藏会把整段卷起吃掉)。</item>
/// </list></para>
///
/// <para><b>首次赋值不播动画。</b> 装载期初值 (判定 = <c>OldValue is null</c>, 见
/// <see cref="IsOpenProperty"/> 的可空性论证) 只落 <c>IsVisible</c> + <c>MaxHeight</c>,
/// 不挂类 ⇒ 页面挂载瞬间不额外抢戏 (那一刻由卡片入场级联负责)。</para>
///
/// <para><b>打断与反悔。</b> 每次状态变更先 Cancel 上一轮动画 (取消会让动画订阅立即释放、
/// 属性回落到局部值), 再以**当前值**为新动画的起点 ⇒ 连点不会跳变。取消后的续行什么都不做
/// (靠 <c>cts.IsCancellationRequested</c> 早退), 避免"取消了还把面板藏掉"。</para>
///
/// <para><b>收尾不闪 (2026-09-24 用户报障修复, 三条硬约束)。</b>
/// ① 局部 <c>MaxHeight</c> 必须写成**终值**且在动画生效之后写 —— 动画释放时会回落到局部值,
///    写成起点就会在收尾瞬间先塌回起点再弹回 (见 <see cref="AnimateAsync"/>);
/// ② 揭示动画 90% 处即达终值并保持 (见 <see cref="HoldCue"/>), 使"落回 ∞ = 布局自然高"
///    这一步的残差恒为 0;
/// ③ 撤类必须等满 <c>令牌时长 + SettlePad</c> —— 揭示了 MaxHeight 的代码动画与 XAML 类动画
///    (卷曲带/内容落平) 走两个时钟, 类动画起步更晚, 早撤类会把它们的收尾一把掐断。</para>
///
/// <para><b>减少动效。</b> <c>KEYFLUX_NO_MOTION=1</c> (MotionPreferences) 时直接落终态,
/// 不留 300~420ms 的空白占位。</para>
///
/// <para><b>为什么这里允许布局动画</b> (页面其它动效仍守"零布局参与"): 摊开在物理上就是占据
/// 空间, 缩放/裁剪做不出"纸张摊开"的读感。安全边界已验: 在揭示层增长全程, 页面外层 Viewbox
/// 的测量高不变 (右列 Grid 固定 Height=800, 内容在内部 ScrollViewer 里) ⇒ 不会反过来触发
/// 整页等比缩放。代价是展开期间每帧一次布局, 单卡 420ms, 可接受。</para>
/// </summary>
public static class SectionUnroll
{
    /// <summary>摊开中类名 (前缘卷曲带 / 内容落平的 XAML 动画锚点, 时长 = Unroll)。</summary>
    public const string UnrollClass = "unroll";

    /// <summary>卷起中类名 (同上, 时长 = Roll; 两个类分开只为让两向时长各自独立)。</summary>
    public const string RollUpClass = "rollup";

    /// <summary>
    /// 一摊一卷共用同一缓动: SineEaseInOut —— 正弦进出, 起步与收尾都无突兀加速度,
    /// 是"过渡顺滑、节奏连贯"最直接的曲线选择 (实测 f(0.25)=0.1464 / f(0.75)=0.8536, 对称)。
    /// 用 Parse 而非直接 new: 避免对具体缓动类型的编译期依赖 (与 XAML 字符串走同一解析).
    /// </summary>
    internal static readonly Easing Ease = Easing.Parse("SineEaseInOut");

    /// <summary>
    /// 收尾判据余量 (2026-09-24 修「到位瞬间闪一下」): 揭示动画 (代码驱动 MaxHeight) 的结束
    /// **不等于** XAML 类动画 (卷曲带 / 内容落平) 的结束 —— 后者的时钟从"样式生效"那一刻起算,
    /// 比 <c>Classes.Add</c> 至少晚一帧。先前撤类由揭示动画的完成信号触发, 于是类动画可能被
    /// 掐在 96% 处: 卷曲带透明度从 ~0.2 直接跳 0、内容变换从 99% 跳 100%, 恰好落在"完全展开
    /// 到位"那一瞬 ⇒ 肉眼就是一次闪烁。现在一律**等满 duration + 本余量**再撤类 (见
    /// <see cref="AnimateAsync"/>), 余量 60ms 远大于一帧, 类动画必然已自行收尾。
    /// </summary>
    private static readonly TimeSpan SettlePad = TimeSpan.FromMilliseconds(60);

    /// <summary>
    /// 揭示动画的"到位保持点": 90% 处即达终值, 保持到 100%。理由: 撤类时要把 MaxHeight 落回 ∞
    /// (= 布局自然高), 若动画最后一帧还差一点 (慢机器/时钟滞后可差十几像素), 这一落就是一次
    /// **高度跳变** (下方卡片整体弹一下 + 揭示边缘多/少一条内容)。提前 10% (≈42ms) 到位并保持,
    /// 撤类就退化成"把同一个值写两遍", 残差恒为 0; 而正弦曲线在 90% 处已完成 97.8%,
    /// 观感上察觉不到这 42ms 的收势。
    /// </summary>
    private const double HoldCue = 0.9;

    /// <summary>分区体是否展开 (替换原先裸绑 <c>IsVisible</c> + <c>Classes.open</c> 的写法)。</summary>
    public static readonly AttachedProperty<bool?> IsOpenProperty =
        AvaloniaProperty.RegisterAttached<SectionUnrollMarker, Control, bool?>("IsOpen");

    private static readonly ConditionalWeakTable<Control, State> States = new();

    private sealed class State
    {
        /// <summary>当前动画的取消柄 (新状态接管时先 Cancel, 让属性立刻回落再取新起点)。</summary>
        public CancellationTokenSource? Cts;
    }

    public static bool? GetIsOpen(Control host) => host.GetValue(IsOpenProperty);

    public static void SetIsOpen(Control host, bool? value) => host.SetValue(IsOpenProperty, value);

    static SectionUnroll()
    {
        IsOpenProperty.Changed.AddClassHandler<Control>(
            (host, e) => Apply(host, e.OldValue is null, e.GetNewValue<bool?>()));
    }

    private static void Apply(Control host, bool isFirstValue, bool? open)
    {
        if (open is not { } wantsOpen) return; // 未赋值 (理论上不会)

        if (isFirstValue)
        {
            // 装载期初值: 只落终态, 不挂类 ⇒ 页面挂载瞬间无动画
            host.Classes.Remove(UnrollClass);
            host.Classes.Remove(RollUpClass);
            host.MaxHeight = wantsOpen ? double.PositiveInfinity : 0;
            host.IsVisible = wantsOpen;
            return;
        }

        var state = States.GetValue(host, _ => new State());
        state.Cts?.Cancel();
        state.Cts?.Dispose();
        state.Cts = null;

        if (!MotionPreferences.AnimationsEnabled)
        {
            // 无动效档: 直接落终态 (绝不挂类, 否则 300~420ms 的 XAML 动画会被跳过但类还在)
            host.Classes.Remove(UnrollClass);
            host.Classes.Remove(RollUpClass);
            host.MaxHeight = wantsOpen ? double.PositiveInfinity : 0;
            host.IsVisible = wantsOpen;
            return;
        }

        var cts = new CancellationTokenSource();
        state.Cts = cts;
        _ = wantsOpen ? UnrollAsync(host, state, cts) : RollUpAsync(host, state, cts);
    }

    // ------------------------------------------------------------- 摊开

    private static async Task UnrollAsync(Control host, State state, CancellationTokenSource cts)
    {
        host.IsVisible = true;
        // 顺序不能反: 必须**先记下当前高度** (打断卷起时是插值到一半的值), 再放开上限去量自然高 ——
        // 放开之后 host.MaxHeight 就成了 ∞, 再取值只会得到 0, 反悔时会从 0 重新长 (可见跳变)
        var current = host.MaxHeight;
        host.MaxHeight = double.PositiveInfinity;
        var natural = MeasureNaturalHeight(host);
        if (natural <= 1)
        {
            Finish(host, state, cts, UnrollClass); // 量不到 (未挂树/宽度为 0): 直落终态
            return;
        }

        // 起点 = 记下的当前高度 (打断场景); 归一化到 [0, natural]。
        // **不写局部值**: 局部值由 AnimateAsync 在动画生效后写成终值 —— 写成起点的话,
        // 动画一释放就会先回落到起点 = 收尾闪一下
        var from = double.IsFinite(current) ? Math.Clamp(current, 0, natural) : 0;
        SwapClass(host, UnrollClass, RollUpClass);

        await AnimateAsync(host, from, natural, ClaudeMotion.Unroll, cts.Token);
        Finish(host, state, cts, UnrollClass);
    }

    // ------------------------------------------------------------- 卷起

    private static async Task RollUpAsync(Control host, State state, CancellationTokenSource cts)
    {
        host.Classes.Remove(UnrollClass);
        if (!host.IsVisible)
        {
            Finish(host, state, cts, RollUpClass);
            return;
        }

        // 起点: 摊开动画进行中是插值中的 MaxHeight; 已完全摊开时 MaxHeight=∞ ⇒ 取实际布局高
        var from = double.IsFinite(host.MaxHeight) ? Math.Max(host.MaxHeight, 0) : host.Bounds.Height;
        if (from <= 1)
        {
            Finish(host, state, cts, RollUpClass);
            return;
        }

        // 起点: 摊开动画进行中是插值中的 MaxHeight; 已完全摊开时 MaxHeight=∞ ⇒ 取实际布局高。
        // 同样**不写局部值** (由 AnimateAsync 写成终值 0), 否则动画一释放会先弹回满高再消失
        SwapClass(host, RollUpClass, UnrollClass);

        await AnimateAsync(host, from, 0, ClaudeMotion.Roll, cts.Token);
        Finish(host, state, cts, RollUpClass);
    }

    // ------------------------------------------------------------- 收尾 / 工具

    /// <summary>动画跑完 (或被取消) 后的收尾: 被取消 ⇒ 让位给新状态, 什么都不做。</summary>
    private static void Finish(Control host, State state, CancellationTokenSource cts, string played)
    {
        if (cts.IsCancellationRequested) return;
        if (!ReferenceEquals(state.Cts, cts)) return;
        state.Cts = null;
        cts.Dispose();

        host.Classes.Remove(played);
        if (played == UnrollClass)
        {
            host.MaxHeight = double.PositiveInfinity; // 摊平后放开上限: 后续内容长高不再被截
        }
        else
        {
            host.MaxHeight = 0;
            host.IsVisible = false;
        }
    }

    /// <summary>类切换在同一帧内完成 (去旧挂新), 避免两向动画窗口重叠。</summary>
    private static void SwapClass(Control host, string add, string remove)
    {
        host.Classes.Remove(remove);
        host.Classes.Add(add);
    }

    /// <summary>
    /// 量内容自然高。宽度从**父级**取: 揭示层自己此刻可能还是 <c>IsVisible=false</c>
    /// (Bounds 为 0), 首次展开时量不到就会白跑一趟动画。父级 (卡片内 StackPanel) 已布局, 宽度可靠。
    /// </summary>
    private static double MeasureNaturalHeight(Control host)
    {
        var width = host.Bounds.Width;
        if (width <= 0) width = (host.Parent as Control)?.Bounds.Width ?? 0;
        if (width <= 0) return 0;
        host.Measure(new Size(width, double.PositiveInfinity));
        return host.DesiredSize.Height;
    }

    /// <summary>
    /// 关键帧动画驱动 <c>MaxHeight</c>。代码侧只能用基元类型属性 (见类型头注释的动画器兜底说明),
    /// 这里恰好只需要 Double。
    /// <para>
    /// <b>局部值必须写成"终值", 且必须在动画生效之后写</b> (2026-09-24 修「到位瞬间闪一下」的根因):
    /// 动画值走 Animation 优先级压住局部值, 但它一旦结束/被取消, 属性会**回落到局部值**。若局部
    /// 值还是起点 (如展开前的 0), 收尾瞬间就会先塌成 0、再被收尾逻辑写成 ∞ —— 两帧之间只要夹了
    /// 一次渲染, 用户看到的就是"完全展开到位时闪一下"; 收起同理 (回落到满高再消失)。
    /// 写在动画之后: 此刻动画已生效, 写局部值不会抢画面, 只决定"动画退场后的落点"。
    /// </para>
    /// </summary>
    private static async Task AnimateAsync(Control host, double from, double target, TimeSpan duration, CancellationToken token)
    {
        Task run;
        try
        {
            run = BuildRevealAnimation(from, target, duration).RunAsync(host, token);
            host.MaxHeight = target; // 动画退场后的落点 = 终值 (关键: 不是 from)
        }
        catch (Exception)
        {
            // 动画起不来 (宿主无动画器/无同步上下文): 直接落终态, 交给调用方收尾
            host.MaxHeight = target;
            return;
        }

        var started = Stopwatch.StartNew();
        try
        {
            await Task.WhenAny(run, Task.Delay(duration + SettlePad, token)).ConfigureAwait(true);
            var remaining = duration + SettlePad - started.Elapsed;
            if (remaining > TimeSpan.Zero) await Task.Delay(remaining, token).ConfigureAwait(true);
        }
        catch (OperationCanceledException)
        {
            // 被新状态接管: 由调用方按 cts.IsCancellationRequested 早退, 这里不做收尾
        }
    }

    /// <summary>
    /// 揭示动画配方 (internal 供守卫测试对账): 起点 <paramref name="from"/> → <see cref="HoldCue"/>
    /// 处即达 <paramref name="to"/> → 100% 保持 <paramref name="to"/> (末两帧等值, 见 HoldCue 注释)。
    /// </summary>
    internal static Animation BuildRevealAnimation(double from, double to, TimeSpan duration)
    {
        var anim = new Animation { Duration = duration, Easing = Ease };
        anim.Children.Add(KeyFrameAt(0, from));
        anim.Children.Add(KeyFrameAt(HoldCue, to));
        anim.Children.Add(KeyFrameAt(1, to));
        return anim;
    }

    private static KeyFrame KeyFrameAt(double cue, double maxHeight)
    {
        var kf = new KeyFrame { Cue = new Cue(cue) };
        kf.Setters.Add(new Setter(Layoutable.MaxHeightProperty, maxHeight));
        return kf;
    }
}
