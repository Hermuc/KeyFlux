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
/// · 内容揭示类 Roll/Unroll — 上限 600ms, 类内 Roll &lt; Unroll (先卷后摊的收势不做主角)。
/// ⚠ 跨类**不再互相单调** (2026-09-24 用户两轮提速的既定形态): 揭示类原本是"低频大动作比交互慢"
///   的示范, 但用户先要求"稍微加快一点点"(420→380), 再直接指定 200ms ⇒ Roll=160 落在 Micro 与
///   Standard 之间, Unroll=200 与 Standard 同值。以用户裁定为准, 契约因此只保证类内有序 + 上限
///   (上限的意义 = 防慢到可感知卡顿, 而不是规定它必须比交互慢)。
///   揭示类始终只配 SineEaseInOut (起止都柔), 提速靠缩短时长而非换更陡的曲线。
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
    /// 分区体卷起 (收拢) 时长 160ms, SineEaseInOut —— 卷轴往回收, 快于摊开 (让位不做主角)。
    /// 2026-09-24 随摊开一同收紧 (原 300ms), 维持 摊开:卷起 = 5:4 的既有比例: 用户只点名了展开,
    /// 但"收起比展开更慢"是错的交互读感 (收势拖沓且多占 160ms 布局), 故按比例跟调 ——
    /// 只想让收起回到 300ms 的话, 改这一个数即可。
    /// 与 <see cref="Unroll"/> 共用同一缓动: 一摊一卷的加减速手感必须对称, 否则"节奏连贯"破功。
    /// </summary>
    public static readonly TimeSpan Roll = TimeSpan.FromMilliseconds(160);

    /// <summary>
    /// 分区体摊开 (展开) 时长 200ms, SineEaseInOut —— 2026-09-24 用户直接指定 (初版 420 → 380 → 200)。
    /// 实现: 揭示层 <c>MaxHeight</c> 由状态机从 0 动画到内容自然高 (真实布局增长, 下方卡片被
    /// 顺次推开), 同时前缘卷曲光影带与内容微沉降同步走完; 见 Services/SectionUnroll.cs。
    /// </summary>
    public static readonly TimeSpan Unroll = TimeSpan.FromMilliseconds(200);
}
