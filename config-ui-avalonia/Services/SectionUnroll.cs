using System;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Services;

/// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
public sealed class SectionUnrollMarker
{
}

/// <summary>
/// 设置页「选项」页手风琴分区体的**卷轴摊开**生命周期状态机 (2026-09-24; 高度动效机制见
/// <see cref="RevealHeightMotion"/>, 本类只管"何时开合、谁负责隐藏、失败怎么收场")。
///
/// <para><b>观感目标 (用户原话)。</b>「像书卷缓缓展开一样, 展开过程中带有自然的卷曲与铺展感,
/// 过渡顺滑、节奏连贯, 整体呈现出卷轴逐步摊开的视觉体验」。⇒ 三个可分解的技术动作:
/// ① <b>逐步摊开</b> = 揭示层高度从 0 连续长到内容自然高 (真·布局增长, 内容被纸张边缘逐行露出,
///    下方卡片被同时推开); ② <b>铺展感</b> = 内容自身还有极小幅度的"落平" (XAML 侧
///    <c>translateY(-3px)</c> → 归位); ③ <b>卷曲</b> = 前缘 16px 渐变光影带 + 1px 纸边线,
///    靠 Grid + VerticalAlignment=Bottom 免费跟随被揭示的边缘。</para>
///
/// <para><b>状态 (两个互斥类 + IsVisible + MaxHeight, 由本类独占, 勿在 XAML 再绑)。</b>
/// <list type="bullet">
///   <item><c>IsOpen=true</c>: 显示 → 量自然高 → <c>MaxHeight</c> 0→H 摊开 + 挂 <c>.unroll</c>;
///         动画收尾后把 <c>MaxHeight</c> 落回 ∞ (内容可自由再长高, 如新增行)。</item>
///   <item><c>IsOpen=false</c>: <c>MaxHeight</c> 当前值→0 卷起 + 挂 <c>.rollup</c>;
///         卷完才 <c>IsVisible=false</c> (IsVisible 不可动画, 提前隐藏会把整段卷起吃掉)。</item>
/// </list></para>
///
/// <para><b>首次赋值不播动画。</b> 装载期初值 (判定 = <c>OldValue is null</c>, 见
/// <see cref="IsOpenProperty"/> 的可空性论证) 只落终态, 不挂类。</para>
///
/// <para><b>打断与反悔。</b> 每次状态变更先 Cancel 上一轮动画 (取消会让动画订阅立即释放、属性回落
/// 到局部值), 再以**当前值**为新动画的起点 ⇒ 连点不跳变。被取消的那一轮什么都不做
/// (靠 token 早退), 避免"取消了还把面板藏掉"。</para>
///
/// <para><b>换卡串行 (2026-09-24 用户裁定, 为流畅度)。</b> 换卡 = 同帧关旧卡 + 开新卡, 两卡并发
/// 会让每帧重光栅成本翻倍 (软件渲染下重卡单帧已 20~30ms ⇒ 掉帧) ⇒ 摊开前先等最近一轮卷起跑完
/// (见 <see cref="s_rollupGate"/>)。代价: 新内容晚约 80ms 出现 (闸 = <see cref="ClaudeMotion.Roll"/>)。若将来渲染管线换成 GPU 有余量,
/// 应去掉这道闸 (串行届时纯属拖慢)。</para>
///
/// <para><b>安全面 (本类被审过的四项)。</b>
/// · <i>输入校验</i>: 唯一外部输入是绑定给的 <c>bool?</c> 与布局给的尺寸 —— 尺寸校验在
///   <see cref="RevealHeightMotion"/>; 附加属性为 <c>bool?</c> 且对 <c>null</c> 直接忽略;
/// · <i>越权面</i>: 本类无权限模型可言 (纯本地视觉), 但把**可被误调的公开入口**收敛到一处:
///   <see cref="SetIsOpen"/> 是唯一外部写入口, 且只会作用在宿主控件自身的
///   <c>MaxHeight</c>/<c>IsVisible</c>/<c>Classes</c> 上 —— 误加到别的控件上最多是"该控件被隐藏",
///   不会读写它的任何业务状态。**刻意不做**宿主类型/类名白名单: 那会让"新布局忘挂类"从
///   "有动画但没卷曲带"退化成"整块内容打不开", 失败模式更重 (由测试锁结构替代, 见 MotionSmokeTests);
/// · <i>错误处理</i>: 状态变更全程包在 <see cref="Apply"/> 的兜底里 —— 动效层任何意外都直落终态,
///   绝不把面板留在"半开/类残留", 也绝不让异常顺着绑定系统外溢 (见 <see cref="Terminal"/>);
/// · <i>数据保护</i>: 不读不写配置/文件/剪贴板/网络, 运行时状态只有每控件一个取消柄, 且以
///   <see cref="ConditionalWeakTable{TKey,TValue}"/> 弱持有 (页面重建不留悬挂引用、不泄漏)。
///   **本类新增任何 IO 都属越界**。</para>
///
/// <para><b>为什么这里允许布局动画</b> (页面其它动效仍守"零布局参与"): 摊开在物理上就是占据空间,
/// 缩放/裁剪做不出"纸张摊开"的读感。安全边界已验: 揭示层增长全程页面外层 Viewbox 的测量高不变
/// (右列 Grid 固定 Height=800, 内容在内部 ScrollViewer 里) ⇒ 不会反过来触发整页等比缩放。</para>
/// </summary>
public static class SectionUnroll
{
    /// <summary>摊开中类名 (前缘卷曲带 / 内容落平的 XAML 动画锚点, 时长 = Unroll)。</summary>
    public const string UnrollClass = "unroll";

    /// <summary>卷起中类名 (同上, 时长 = Roll; 两个类分开只为让两向时长各自独立)。</summary>
    public const string RollUpClass = "rollup";

    /// <summary>分区体是否展开 (替换原先裸绑 <c>IsVisible</c> + <c>Classes.open</c> 的写法)。</summary>
    /// <remarks>
    /// <b>为什么是 <c>bool?</c> 而非 <c>bool</c>:</b> 状态机必须能区分「装载期初值」与「用户交互」。
    /// 用 <c>bool</c> 时属性默认值 false 与「收起」同值 ⇒ 收起态的卡片在装载期**不产生变化通知**
    /// (false→false), 状态机收不到任何信号, <c>IsVisible</c> 会停在默认 true (7 个分区全部展开)。
    /// 可空类型的默认值 null 与两个真实状态都不同 ⇒ 装载期每个分区体都必然产生一次通知,
    /// <c>OldValue is null</c> 即"初始化", 判定零歧义, 也不依赖"XAML 赋值发生在挂树前"这一时序假设。
    /// </remarks>
    public static readonly AttachedProperty<bool?> IsOpenProperty =
        AvaloniaProperty.RegisterAttached<SectionUnrollMarker, Control, bool?>("IsOpen");

    /// <summary>每个分区体一份取消柄 (弱表持有: 页面重建不留悬挂引用)。</summary>
    private static readonly ConditionalWeakTable<Control, State> States = new();

    /// <summary>
    /// 换卡**串行闸** (2026-09-24 用户裁定: 在"保持软件渲染"的前提下用串行换平滑)。
    ///
    /// <para>手风琴换卡会在同一帧里"关旧卡 + 开新卡", 两卡并发动画 ⇒ 每帧重光栅成本叠加。
    /// 实测软件光栅路径: 轻卡每帧 1.4ms, 重卡 (命令框皮肤卡) 单帧 20~30ms (Debug), 两卡并发
    /// 直接双倍 ⇒ 掉帧。故摊开前先等最近一轮卷起跑完, 峰值回到单卡水平。</para>
    ///
    /// <para><b>为什么闸只等"卷起令牌时长"而不是"卷起动画完成回调"</b>: 后者要连收尾余量一起等
    /// (再多约 60ms), 那段空档里没有任何动画在播, 纯属白等; 前者只让新卡晚 <see cref="ClaudeMotion.Roll"/>
    /// (80ms) 出现, 且其尾部与卷起的收尾余量重叠 —— 那时旧卡高度已≈0, 重光栅成本可忽略。
    /// 传 <c>cts.Token</c>: 卷起被取消 (用户连点) 时闸立即放行, 不留下无谓等待。</para>
    ///
    /// <para>未在飞时是已完成的 <c>Task.CompletedTask</c> ⇒ <c>await</c> 同步返回,
    /// **单卡展开仍是零延迟**; 新内容晚约 80ms 出现是本次取舍的既定代价。</para>
    /// </summary>
    private static Task s_rollupGate = Task.CompletedTask;

    private sealed class State
    {
        /// <summary>当前动画的取消柄。<b>释放权归使用它的动画任务</b> (见 UnrollAsync/RollUpAsync
        /// 的 finally), 本类只负责 Cancel —— 单一所有者避免 ObjectDisposedException 与泄漏。</summary>
        public CancellationTokenSource? Cts;
    }

    public static bool? GetIsOpen(Control host) => host.GetValue(IsOpenProperty);

    public static void SetIsOpen(Control host, bool? value) => host.SetValue(IsOpenProperty, value);

    static SectionUnroll()
    {
        IsOpenProperty.Changed.AddClassHandler<Control>(
            (host, e) => Apply(host, e.OldValue is null, e.GetNewValue<bool?>()));
    }

    /// <summary>
    /// 状态变更入口 (兼作错误边界): 任何意外都收敛为"直落终态"。属性变更回调里抛异常会沿着绑定
    /// 系统外溢到调用方, 属不可接受的失败模式; 且失败时必须保证"点开就能看到内容", 宁可没有动画。
    /// </summary>
    private static void Apply(Control host, bool isFirstValue, bool? open)
    {
        if (open is not { } wantsOpen) return; // 未赋值 (理论上不会): 忽略而非猜

        try
        {
            var state = States.GetValue(host, _ => new State());

            if (isFirstValue)
            {
                // 装载期初值: 只落终态, 不挂类 ⇒ 页面挂载瞬间无动画、不与卡片入场级联抢戏
                Terminal(host, wantsOpen);
                return;
            }

            TryCancel(state);

            if (!MotionPreferences.AnimationsEnabled)
            {
                // 无动效档: 直落终态 (绝不挂类 —— 否则 XAML 动画会被跳过但类还留在控件上)
                Terminal(host, wantsOpen);
                return;
            }

            var cts = new CancellationTokenSource();
            state.Cts = cts;
            _ = wantsOpen ? UnrollAsync(host, state, cts) : RollUpAsync(host, state, cts);
        }
        catch (Exception)
        {
            TryCancel(States.GetValue(host, _ => new State()));
            Terminal(host, wantsOpen);
        }
    }

    // ------------------------------------------------------------- 摊开

    private static async Task UnrollAsync(Control host, State state, CancellationTokenSource cts)
    {
        try
        {
            // 串行闸: 换卡时先让上一张卡的"卷起"跑完, 避免两卡并发动画让每帧重光栅成本翻倍
            // (软件渲染下重卡单帧已 20~30ms, 见 s_rollupGate 注释)。无卷起在飞时立即通过。
            await SkipCancellationAsync(s_rollupGate).ConfigureAwait(true);
            if (cts.IsCancellationRequested) return;

            host.IsVisible = true;
            // 顺序不能反: 必须**先记下当前高度** (打断卷起时是插值到一半的值), 再放开上限去量自然高
            // —— 放开之后 host.MaxHeight 就成了 ∞, 再取值只会得到 0, 反悔时会从 0 重新长 (可见跳变)
            var current = host.MaxHeight;
            host.MaxHeight = double.PositiveInfinity;
            var natural = RevealHeightMotion.MeasureNaturalHeight(host);
            if (!RevealHeightMotion.CanDrive(natural))
            {
                Finish(host, state, cts, UnrollClass); // 量不到 / 病态超大: 直落终态
                return;
            }

            // 起点 = 记下的当前高度 (打断场景), 归一化到 [0, natural];
            // **不写局部值**: 由 RevealHeightMotion 在动画生效后写成终值 (写起点会导致收尾闪一下)
            var from = double.IsFinite(current) ? Math.Clamp(current, 0, natural) : 0;
            SwapClass(host, UnrollClass, RollUpClass);

            await RevealHeightMotion.AnimateAsync(host, from, natural, ClaudeMotion.Unroll, cts.Token)
                .ConfigureAwait(true);
            Finish(host, state, cts, UnrollClass);
        }
        catch (Exception)
        {
            Finish(host, state, cts, UnrollClass); // 兜底: 不把面板留在半开态
        }
        finally
        {
            cts.Dispose(); // 取消柄的唯一所有者 = 使用它的动画任务
        }
    }

    // ------------------------------------------------------------- 卷起

    private static async Task RollUpAsync(Control host, State state, CancellationTokenSource cts)
    {
        try
        {
            host.Classes.Remove(UnrollClass);
            if (!host.IsVisible)
            {
                Finish(host, state, cts, RollUpClass);
                return;
            }

            // 起点: 摊开动画进行中是插值中的 MaxHeight; 已完全摊开时 MaxHeight=∞ ⇒ 取实际布局高。
            // 同样不写局部值 (写起点会让动画释放时先弹回满高再消失)
            var from = double.IsFinite(host.MaxHeight) ? Math.Max(host.MaxHeight, 0) : host.Bounds.Height;
            if (!RevealHeightMotion.CanDrive(from))
            {
                Finish(host, state, cts, RollUpClass);
                return;
            }

            SwapClass(host, RollUpClass, UnrollClass);
            // 登记串行闸 (仅在本轮**确实要播动画**时才登记, 否则会平白拖住下一张卡的摊开)
            s_rollupGate = Task.Delay(ClaudeMotion.Roll, cts.Token);

            await RevealHeightMotion.AnimateAsync(host, from, 0, ClaudeMotion.Roll, cts.Token)
                .ConfigureAwait(true);
            Finish(host, state, cts, RollUpClass);
        }
        catch (Exception)
        {
            Finish(host, state, cts, RollUpClass);
        }
        finally
        {
            cts.Dispose();
        }
    }

    // ------------------------------------------------------------- 收尾 / 工具

    /// <summary>
    /// 动画收尾: 撤类 + 落终态。<b>被新状态接管时直接让位, 什么都不做</b> (否则会把刚展开的面板藏掉);
    /// 只有"这一轮动画真正跑完"才收尾。
    /// </summary>
    private static void Finish(Control host, State state, CancellationTokenSource cts, string played)
    {
        if (cts.IsCancellationRequested) return;
        if (!ReferenceEquals(state.Cts, cts)) return;
        state.Cts = null;
        host.Classes.Remove(played);
        Terminal(host, played == UnrollClass);
    }

    /// <summary>
    /// 直落终态 (无动画)。三个调用点共用: 装载期初值、减少动效档、失败兜底 —— 单一定义可审,
    /// 避免三处各写一套 <c>MaxHeight</c>/<c>IsVisible</c>/类名组合而逐渐漂移。
    /// </summary>
    private static void Terminal(Control host, bool open)
    {
        host.Classes.Remove(UnrollClass);
        host.Classes.Remove(RollUpClass);
        host.MaxHeight = open ? double.PositiveInfinity : 0;
        host.IsVisible = open;
    }

    /// <summary>类切换在同一帧内完成 (去旧挂新), 避免两向动画窗口重叠。</summary>
    private static void SwapClass(Control host, string add, string remove)
    {
        host.Classes.Remove(remove);
        host.Classes.Add(add);
    }

    /// <summary>
    /// 等闸放行; 闸被取消 (卷起被打断) 视为"无需再等", 直接放行。
    /// 闸本身是 <c>Task.Delay(令牌时长, token)</c>, 故等待有上界, 不会挂住。
    /// </summary>
    private static async Task SkipCancellationAsync(Task gate)
    {
        try
        {
            await gate.ConfigureAwait(true);
        }
        catch (OperationCanceledException)
        {
            // 卷起被取消: 没有动画在跑, 无需让位
        }
    }

    /// <summary>
    /// 取消上一轮动画。吞掉 <see cref="ObjectDisposedException"/>: 取消柄的释放权在动画任务侧,
    /// 正常情况下这里拿到的一定是未释放的实例, 但"已跑完的取消柄还没被清空"这一瞬不该让状态机崩。
    /// </summary>
    private static void TryCancel(State state)
    {
        try
        {
            state.Cts?.Cancel();
        }
        catch (ObjectDisposedException)
        {
            // 已释放: 那轮动画必然已结束, 无需再取消
        }

        state.Cts = null;
    }
}
