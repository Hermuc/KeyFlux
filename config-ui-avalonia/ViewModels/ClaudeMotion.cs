using System;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// Claude 皮肤动效时长令牌 (C# 强类型真源) —— 与色值真源 <see cref="ClaudePalette"/> 同一先例:
/// XAML 一律经 <c>{x:Static vm:ClaudeMotion.*}</c> 引用, 勿写散落字面量。
/// 换肤 = 连同本类与皮肤 XAML 整组替换 (皮肤文件头「动效时长令牌」段有契约说明)。
///
/// 基线取生产率工具档 (design-motion-principles): 交互反馈类 (高频) 全部 ≤300ms;
/// 频率闸门 = 按压/悬停 (高频) 最短, 页面级过渡 (低频) 最长。
///
/// 两类令牌 (SkinContractTests 分别锁**上限**与**类内**递增):
/// · 交互反馈类 Press/Micro/Standard/Enter — 上限 300ms (生产率基线);
/// · 内容揭示类 Roll/Unroll — 上限 600ms, 类内 Roll ≤ Unroll (2026-09-24 起二者等长, 见下)。
/// ⚠ 跨类**不再互相单调** (2026-09-24 用户多次提速的既定形态): 揭示类原本是"低频大动作比交互慢"
///   的示范, 但用户连续收窄 (420 → 380 → 200 → 100ms) ⇒ 最终 Roll=Unroll=100ms (均在 Micro 与
///   Standard 之间)。以用户裁定为准, 契约因此只保证类内有序 + 上限
///   (上限的意义 = 防慢到可感知卡顿, 而不是规定它必须比交互慢)。
///   揭示类始终只配 SineEaseInOut (起止都柔), 提速靠缩短时长而非换更陡的曲线。
/// </summary>
public static class ClaudeMotion
{
    /// <summary>按压下沉等即时反馈 (80ms, QuadraticEaseOut)。</summary>
    public static readonly TimeSpan Press = TimeSpan.FromMilliseconds(80);

    /// <summary>悬停/选中等画刷变色微过渡 (120ms)。</summary>
    public static readonly TimeSpan Micro = TimeSpan.FromMilliseconds(120);

    /// <summary>面板级标准过渡: 页面切换 CrossFade (200ms; 分区展开另用揭示类 <see cref="Unroll"/>)。</summary>
    public static readonly TimeSpan Standard = TimeSpan.FromMilliseconds(200);

    /// <summary>入场级联单卡时长 (220ms; 级联错峰 30ms/卡在消费方定义)。</summary>
    public static readonly TimeSpan Enter = TimeSpan.FromMilliseconds(220);

    /// <summary>
    /// 分区体卷起 (收拢) 时长 100ms, SineEaseInOut —— **与 <see cref="Unroll"/> 等长**。
    /// 演进: 300ms (初版) → 160ms (随摊开收紧) → 80ms (按 5:4 跟调) → 100ms (与摊开齐平)。
    /// 修「折叠比展开卡」时改为等长: 帧数才是流畅度的决定因素 (软件光栅下每帧成本相同, 时长更短
    /// = 帧更少 = 更"跳"), 刻意让收势更快的收益远小于它带来的卡顿感知; 二者等长后一摊一卷手感一致。
    /// ⚠ 本值同时是换卡串行闸的等待时长 (见 Services/SectionUnroll.s_rollupGate): 改这里会一并改变
    /// "点开另一张卡后新内容出现的延迟" —— 二者共用一个数是有意的, 闸的语义就是"等这张卡卷完"。
    /// 与 <see cref="Unroll"/> 共用同一缓动: 一摊一卷的加减速手感必须对称, 否则"节奏连贯"破功。
    /// </summary>
    public static readonly TimeSpan Roll = TimeSpan.FromMilliseconds(100);

    /// <summary>
    /// 分区体摊开 (展开) 时长 100ms, SineEaseInOut —— 2026-09-24 用户指定 (演进 420 → 380 → 200 → 100)。
    /// ⚠ 100ms 已贴近"逐帧重光栅"的可行下限: 重卡 (命令框皮肤, 511 视觉元素) 单帧成本 20~30ms
    /// (Debug; Release 约 3~5× 快) ⇒ 100ms 只够 6~15 帧; 若实机读起来像"啪一下弹开"而非摊开,
    /// 优先考虑放宽本值 (而非再压), 根治手段则是换 GPU 渲染 (见 Program.cs KEYFLUX_RENDER_GPU)。
    /// 实现: 揭示层 <c>MaxHeight</c> 由状态机从 0 动画到内容自然高 (真实布局增长, 下方卡片被
    /// 顺次推开), 同时前缘卷曲光影带与内容微沉降同步走完; 见 Services/SectionUnroll.cs。
    /// </summary>
    public static readonly TimeSpan Unroll = TimeSpan.FromMilliseconds(100);

    // ========================================================================
    // 弹窗类令牌 (2026-09-24, 苹果式弹窗动效批次)
    //
    // 为什么单开一类而**不塞进交互反馈类**: 苹果式入场是"先快后慢 + 弹簧惯性"的
    // 长尾曲线 (弹簧余韵需要时间展开), 天然长于 ≤300ms 的交互反馈上限; 硬塞会破坏
    // 既定契约, 也丢失"弹窗是低频大动作、可以从容一点"的语义。
    // 契约 (SkinContractTests.Skin_Contract_Motion_Tokens_Are_Ordered_And_Bounded):
    //   类内递增 退场 < **分层内容** < 入场, 且均 ≤ 700ms 上限。
    //   分层内容 (240) 短于容器入场 (360): 内容是容器弹簧未收完时就跑完的**子动作**,
    //   起步晚 (错峰 90ms) 但结束早 ⇒ 才有"容器先浮起、内容跟着沉降到位"的层次。
    //
    // ⚠ 与揭示类的关系: 弹窗退场允许比入场**更短更快** (用户明确要求"消失比出现更干脆"),
    //   这与揭示类的"刻意等长"结论并不矛盾 —— 揭示类没有遮罩兜底, 收势快会暴露"画面突然空掉";
    //   弹窗有遮罩层承接 (遮罩稍晚淡出), 快退场是**安全的**。这是两处场景差异的显式记录。
    // ========================================================================

    /// <summary>
    /// 弹窗入场时长 360ms —— 弹簧曲线 (先快后慢 + 极轻微回弹)。
    /// 曲线不是标准 Easing: 由 <see cref="Services.DialogMotion"/> 以关键帧构造,
    /// 在 ~55% 处越过终值约 1.4%, 于 100% 收束到终值 (克制的惯性, 不夸张弹跳)。
    /// </summary>
    public static readonly TimeSpan DialogEnter = TimeSpan.FromMilliseconds(360);

    /// <summary>
    /// 弹窗退场时长 200ms —— 比入场更快 (用户要求"消失比出现更干脆迅速")。
    /// 形态: 先轻微收缩 (scale 1 → 0.965) 再淡出, 不是单纯 Opacity 归零。
    /// </summary>
    public static readonly TimeSpan DialogExit = TimeSpan.FromMilliseconds(200);

    /// <summary>
    /// 弹窗内元素的分层入场时长 240ms —— **晚于容器**起步 (错峰 90ms, 见
    /// <see cref="DialogContentDelay"/>), 以"轻微上移 + 淡入"呈现, 制造玻璃浮起后
    /// 内容跟着沉入的分层感。
    /// </summary>
    public static readonly TimeSpan DialogContent = TimeSpan.FromMilliseconds(240);

    /// <summary>
    /// 内容元素相对容器的起步延迟 90ms —— 容器先动, 内容稍晚跟上 (苹果式的分层节奏)。
    /// </summary>
    public static readonly TimeSpan DialogContentDelay = TimeSpan.FromMilliseconds(90);

    /// <summary>
    /// 遮罩淡入时长 300ms (柔和淡入, 略慢于弹窗本体起步 ⇒ 背景先"退后", 弹窗再浮起)。
    /// </summary>
    public static readonly TimeSpan ScrimFadeIn = TimeSpan.FromMilliseconds(300);

    /// <summary>
    /// 遮罩淡出时长 260ms —— **晚于弹窗退场起步** (见 <see cref="ScrimExitDelay"/>),
    /// 让弹窗先收缩淡出、遮罩后撤, 避免"画面突然空掉"。
    /// </summary>
    public static readonly TimeSpan ScrimFadeOut = TimeSpan.FromMilliseconds(260);

    /// <summary>
    /// 遮罩淡出的起步延迟 120ms —— 弹窗已开始收缩但尚未消失时, 遮罩才开始后撤。
    /// 总时长 (120 + 260 = 380ms) 略长于弹窗退场 (200ms) ⇒ 画面不会出现"空一瞬"。
    /// </summary>
    public static readonly TimeSpan ScrimExitDelay = TimeSpan.FromMilliseconds(120);

    /// <summary>
    /// 背景在弹窗期间的缩放目标 (1 → 0.985) —— "背景略微缩小"制造纵深。
    /// 实测依据 (真 Skia 逐帧探针, 2026-09-24): 给已挂 <c>BlurEffect</c> 的元素叠加逐帧
    /// <c>RenderTransform</c> 缩放, 每帧成本 7.09ms vs 静止模糊 6.03ms = **仅 1.18x**;
    /// 机制是"模糊结果缓存成图层, 缩放作用于该图层"而非每帧重新模糊变化后的内容
    /// (与 Win32 软件渲染/GPU 渲染的呈现方式无关, 是 Avalonia 渲染管线的共同行为)。
    /// ⚠ 该结论来自 headless 真 Skia, 实机 Software + RedirectionSurface 下仍需目视确认。
    /// </summary>
    public const double ScrimBackdropScale = 0.985;

    /// <summary>
    /// 背景缩放/遮罩淡入时长 340ms —— 与弹窗入场大致同步, 略短一点让背景先就位。
    /// </summary>
    public static readonly TimeSpan ScrimBackdrop = TimeSpan.FromMilliseconds(340);

    /// <summary>
    /// 按下时的缩放比例 (按钮即时反馈) —— 0.97 是"轻微"区间: 再小就读成弹跳, 再大则看不出。
    /// </summary>
    public const double PressScale = 0.97;

    /// <summary>
    /// 弹窗玻璃材质的圆角半径 16px (与皮肤 RadiusKeys 的卡片系圆角同一族, 弹窗取偏大档
    /// 以强调"浮起的独立面板"; 连续圆角观感由此半径 + 描边高光共同构成)。
    /// </summary>
    public const double DialogCornerRadius = 16;
}
