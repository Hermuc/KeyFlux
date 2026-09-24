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
}
