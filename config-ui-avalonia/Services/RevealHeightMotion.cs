using System;
using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Styling;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 「揭示层高度」动效机制 (2026-09-24 从 <see cref="SectionUnroll"/> 拆出)。
///
/// <para><b>职责边界 (模块化, 只做机制不做策略)。</b> 本类只回答"怎么把一个裁剪容器的高度从 A
/// 连续搬到 B, 并保证搬完后接得住": 测量自然高 → 构造关键帧动画 → 驱动 <c>MaxHeight</c> → 有界等待
/// 动画收尾。**不含**手风琴语义、开合时机、减少动效闸门、可见性生命周期 —— 那些留在
/// <see cref="SectionUnroll"/>。拆出的三条理由:
/// <list type="number">
///   <item><b>脆弱的平台知识集中一处。</b> Avalonia 11.3 的三个坑只应在一个文件里解释清楚:
///     <br/>· 代码构造的 <c>Animation</c> 走 <c>GetAnimatorType</c> 兜底, **只覆盖基元类型**
///       (<c>Double</c> 可用; <c>RenderTransform</c> 直接抛 "No animator registered");
///     <br/>· 动画结束/被取消时属性会**回落局部值** —— 局部值写错就是收尾闪一下;
///     <br/>· 完成回调走 <c>TaskScheduler.FromCurrentSynchronizationContext</c>, 无同步上下文的
///       宿主推不动 ⇒ 必须自己兜一个时长上界。</item>
///   <item><b>可复用。</b> 任何"内容按高度逐步露出"的场景 (脚本预览、长列表分段展开) 可直接调
///     <see cref="AnimateAsync"/>, 无需理解状态机。</item>
///   <item><b>变更隔离。</b> 改观感 (时长/缓动取值) 与改交互 (何时开合) 不再互相牵动。</item>
/// </list></para>
///
/// <para><b>安全面 (本类被审过的四项)。</b>
/// · <i>输入校验</i>: 测量宽度必须有限且为正 (非有限值会让 <c>Measure</c> 抛, 异常会顺着绑定系统
///   外溢); 目标高必须落在 <see cref="CanDrive"/> 的合理区间 —— 病态超大内容直接拒绝动画,
///   避免为一张巨卡做 400ms 的逐帧全页布局;
/// · <i>越权面</i>: 只写宿主控件自身的布局/渲染属性, 不读外部状态、不写别人的属性 ⇒ 无权限模型
///   可谈, 也无跨控件越权面; 唯一外部输入是布局系统给的宽度, 已校验;
/// · <i>错误处理</i>: 动画起不来 (无动画器/无同步上下文) 不抛给调用方, 直接落终值, 由状态机收尾;
/// · <i>数据保护</i>: 本类**禁止引入任何 IO/网络/配置/剪贴板访问** —— 动效层碰数据是这类模块
///   最常见的越界, 一旦新增即为缺陷。当前实现仅内存内数值运算。</para>
/// </summary>
internal static class RevealHeightMotion
{
    /// <summary>
    /// 一摊一卷共用的缓动: SineEaseInOut (正弦进出, 起步与收尾都无突兀加速度)。
    /// ⚠ 与 XAML 侧四个 <c>Animation</c> 的 <c>Easing="SineEaseInOut"</c> **必须一致**
    /// (MotionSmokeTests 交叉断言: C# 曲线类型 ≡ 4 条 XAML 动画的缓动类型), 否则一摊一卷两套手感。
    /// </summary>
    internal static readonly Easing Ease = Easing.Parse("SineEaseInOut");

    /// <summary>
    /// 动画"到位保持点": 90% 处即达终值, 保持到 100%。理由 —— <c>MaxHeight</c> 的终值在动画结束后
    /// 会被落回局部值 (= 布局自然高), 若动画最后一帧还差一点 (慢机器/时钟滞后可差十几像素),
    /// 这一落就是一次**高度跳变** (下方卡片整体弹一下 + 揭示边缘多/少一条内容)。提前 10% 到位保持
    /// 到位并保持, 收尾就退化成"把同一个值写两遍", 残差恒为 0; 而正弦曲线在 90% 处已完成 97.8%,
    /// 观感上察觉不到这段收势 (200ms 令牌下仅约 20ms)。
    /// </summary>
    private const double HoldCue = 0.9;

    /// <summary>
    /// 收尾判据余量 (60ms): 调用方撤动画类必须等满 <c>duration + 本余量</c> —— 这里的动画
    /// (代码驱动 <c>MaxHeight</c>) 与 XAML 类动画走**两个时钟**, 类动画从"样式生效"那一刻起算,
    /// 比 <c>Classes.Add</c> 至少晚一帧; 早撤类会把还在半途的类动画 (卷曲带收尾淡出) 一把掐断,
    /// 表现为到位瞬间闪一下。
    /// </summary>
    private static readonly TimeSpan SettlePad = TimeSpan.FromMilliseconds(60);

    /// <summary>
    /// 可动画高度上界 (px)。超过即拒绝动画、直落终态: 高度动画每帧都会触发一次全页布局, 病态超大
    /// 内容 (异常数据/超大插件面板) 上做 400ms 逐帧布局会把 UI 拖住 —— 宁可不播动画, 不能不响应。
    /// 取 20000 的依据: 右列视口高 800, 现有最大卡片 (命令框皮肤 18 字段 + 字体小节) 实测约 800,
    /// 留 25 倍余量, 正常内容不可能触及。
    /// </summary>
    private const double MaxAnimatedHeight = 20000;

    /// <summary>目标高是否在可动画区间 (输入校验: 也是"量不到就直落"的判据)。</summary>
    internal static bool CanDrive(double height) => height > 1 && height <= MaxAnimatedHeight;

    /// <summary>
    /// 量内容自然高。<b>宽度校验是必须的</b>: <c>Layoutable.Measure</c> 对 NaN 抛
    /// <see cref="ArgumentException"/>, 而这里处在属性变更回调链上 —— 抛出去会顺着绑定系统外溢,
    /// 属不可接受的失败模式, 故非有限/非正值一律按"量不到"返回 0 (调用方据此走直落终态)。
    /// 宽度从**父级**兜底同理: 揭示层自身此刻可能还是 <c>IsVisible=false</c> (Bounds 为 0)。
    /// </summary>
    internal static double MeasureNaturalHeight(Control host)
    {
        var width = host.Bounds.Width;
        if (!IsUsableWidth(width)) width = (host.Parent as Control)?.Bounds.Width ?? 0;
        if (!IsUsableWidth(width)) return 0;

        host.Measure(new Size(width, double.PositiveInfinity));
        var height = host.DesiredSize.Height;
        return double.IsFinite(height) && height > 0 ? height : 0;
    }

    private static bool IsUsableWidth(double width) => double.IsFinite(width) && width > 0;

    /// <summary>
    /// 把 <paramref name="host"/> 的高度从 <paramref name="from"/> 动到 <paramref name="target"/>,
    /// 并保证"调用方拿到控制权时动画已收尾"。
    ///
    /// <para><b>局部值规则 (收尾不闪的关键)。</b> 局部 <c>MaxHeight</c> 必须写成**终值**, 且必须写在
    /// 动画生效**之后**: 动画值走 Animation 优先级压住局部值, 但它一旦结束/被取消, 属性会回落到
    /// 局部值 —— 局部值若还是起点 (如展开前的 0), 收尾瞬间就会先塌回起点再被收尾逻辑写回终值,
    /// 两帧之间只要夹了一次渲染, 用户看到的就是"完全展开到位时闪一下"。</para>
    ///
    /// <para><b>等待策略。</b> 等"动画任务完成"或"名义时长 + <see cref="SettlePad"/>", 二者取后者为
    /// 下界: 前者在无同步上下文的宿主可能永不落地 (实测), 后者保证类动画也已跑完。取消时不抛 ——
    /// 由调用方按 token 判定让位。</para>
    /// </summary>
    internal static async Task AnimateAsync(Control host, double from, double target, TimeSpan duration, CancellationToken token)
    {
        Task run;
        try
        {
            run = BuildAnimation(from, target, duration).RunAsync(host, token);
            host.MaxHeight = target; // 动画退场后的落点 = 终值 (关键: 不是 from)
        }
        catch (Exception)
        {
            // 动画起不来 (宿主无动画器/无同步上下文): 直接落终值, 交给调用方收尾
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
            // 被新状态接管: 收尾交给调用方按 token 早退
        }
    }

    /// <summary>
    /// 关键帧配方 (internal 供守卫测试对账): 0% = <paramref name="from"/> → <see cref="HoldCue"/>
    /// 处即达 <paramref name="target"/> → 100% 保持 (末两帧等值 ⇒ 收尾无残差)。
    /// </summary>
    internal static Animation BuildAnimation(double from, double target, TimeSpan duration)
    {
        var anim = new Animation { Duration = duration, Easing = Ease };
        anim.Children.Add(KeyFrameAt(0, from));
        anim.Children.Add(KeyFrameAt(HoldCue, target));
        anim.Children.Add(KeyFrameAt(1, target));
        return anim;
    }

    private static KeyFrame KeyFrameAt(double cue, double maxHeight)
    {
        var keyFrame = new KeyFrame { Cue = new Cue(cue) };
        keyFrame.Setters.Add(new Setter(Layoutable.MaxHeightProperty, maxHeight));
        return keyFrame;
    }
}
