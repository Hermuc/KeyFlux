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
/// 两类令牌 (SkinContractTests 分别锁上下界):
/// · 交互反馈类 Press/Micro/Standard/Enter — 上限 300ms (生产率基线)。
/// · 内容揭示类 Roll/Unroll — 上限 600ms。**放宽的由来 (2026-09-24 用户裁定)**:
///   分区体展开要"像书卷/卷轴缓缓摊开", 240ms 级别的淡入+位移读不出"缓缓铺展"的体量感,
///   故揭示类整体慢于交互类; 摊开时长经两轮调校定为 380ms (初版 420ms, 后按"稍微加快一点点"收紧),
///   仍远低于 600ms 的"可感知卡顿"线, 且只用 SineEaseInOut (起止都柔) 保证"节奏连贯"。
/// 递增序 (在两类内部各自递增): Press &lt; Micro &lt; Standard &lt; Enter &lt; Roll &lt; Unroll。
/// </summary>
public static class ClaudeMotion
{
    /// <summary>按压下沉等即时反馈 (80ms, QuadraticEaseOut)。</summary>
    public static readonly TimeSpan Press = TimeSpan.FromMilliseconds(80);

    /// <summary>悬停/选中等画刷变色微过渡 (120ms)。</summary>
    public static readonly TimeSpan Micro = TimeSpan.FromMilliseconds(120);

    /// <summary>面板级标准过渡: 页面切换 CrossFade / 分区展开 (200ms)。</summary>
    public static readonly TimeSpan Standard = TimeSpan.FromMilliseconds(200);

    /// <summary>入场级联单卡时长 (220ms; 级联错峰 30ms/卡在消费方定义)。</summary>
    public static readonly TimeSpan Enter = TimeSpan.FromMilliseconds(220);

    /// <summary>
    /// 分区体卷起 (收拢) 时长 300ms, SineEaseInOut —— 卷轴往回收, 略快于摊开 (让位不做主角)。
    /// 与 <see cref="Unroll"/> 共用同一缓动: 一摊一卷的加减速手感必须对称, 否则"节奏连贯"破功。
    /// </summary>
    public static readonly TimeSpan Roll = TimeSpan.FromMilliseconds(300);

    /// <summary>
    /// 分区体摊开 (展开) 时长 380ms, SineEaseInOut —— 卷轴徐徐铺展的体量感来源。
    /// 实现: 揭示层 <c>MaxHeight</c> 由状态机从 0 动画到内容自然高 (真实布局增长, 下方卡片被
    /// 顺次推开), 同时前缘卷曲光影带与内容微沉降同步走完; 见 Services/SectionUnroll.cs。
    /// </summary>
    public static readonly TimeSpan Unroll = TimeSpan.FromMilliseconds(380);
}
