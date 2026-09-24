using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Layout;
using Avalonia.Media.Transformation;
using Avalonia.Styling;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 设置面板动效守护 (2026-09-13 动效批次; 2026-09-24 分区体改为「卷轴摊开」):
/// ① 设置页根类 .motion 与系统动效偏好一致 —— 关闭动画时入场级联/分区摊开整体跳过的唯一开关
///    (页内动画选择器均以 .motion 开头);
/// ② 7 个分区体全部由 <see cref="SectionUnroll"/> 状态机接管 (IsOpen 附加属性驱动
///    MaxHeight / IsVisible / .unroll / .rollup), 且揭示层结构完整 (揭示层 + 内容 + 卷曲带);
/// ③ 摊开/卷起的 XAML 动画时长与皮肤令牌对账 (两向各 2 条: 卷曲带 + 内容落平), 且内容落平的
///    RenderTransform 能连续插值 (离散跳变会让"铺展"退化成硬跳);
/// ④ 主窗页面切换 = TransitioningContentControl + 皮肤令牌时长的 CrossFade。
/// 注: 动画本身不在此断言 —— headless xunit 不驱动 Avalonia 全局时钟 (实测连旧有的卡片入场级联
/// 都停在 Opacity=1), 只锁结构与接线; 观感由实机验证。同理状态机的收尾 (动画完成回调) 在
/// headless 里不会发生 ⇒ 只断言"当帧已进入摊开/卷起态"这一侧。
/// </summary>
[Collection("I18nSerial")]
public sealed class MotionSmokeTests
{
    private static SettingsPageViewModel NewPage(out MainViewModel main)
    {
        main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        // 补 keymap/1 = 「自定义热键」分区的数据源: 缺它时该卡整卡隐藏 (IsVisible=NotNull(CustomHotkeys)),
        // 揭示层宽度为 0 ⇒ 量不到自然高, 摊开会走"量不到就直落"的兜底。本组用例要覆盖全部 7 个分区。
        main.Config.Keymaps.Add(new Keymap { Id = 1, Name = "k1", Hotkey = "!x", ParentId = 0 });
        var vm = new SettingsPageViewModel(main);
        vm.RefreshKeymapSection();
        return vm;
    }

    private static (SettingsPageView View, Window Window, List<Border> Reveals) Mount(SettingsPageViewModel vm)
    {
        var view = new SettingsPageView { DataContext = vm };
        var window = new Window { Width = 1500, Height = 950, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        var reveals = view.GetVisualDescendants().OfType<Border>()
            .Where(b => b.Classes.Contains("reveal")).ToList();
        return (view, window, reveals);
    }

    [AvaloniaFact]
    public void SettingsPage_Motion_Gate_And_Section_Reveals_Are_Wired()
    {
        // 闸门默认开; KEYFLUX_NO_MOTION=1 时 .motion 不挂 → 页内动画选择器全部失配
        Assert.True(MotionPreferences.AnimationsEnabled, "默认应启用动效 (未设 KEYFLUX_NO_MOTION)");
        Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", "1");
        try
        {
            Assert.False(MotionPreferences.AnimationsEnabled);
            Assert.DoesNotContain("motion", new SettingsPageView().Classes);
        }
        finally
        {
            Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", null);
        }

        var vm = NewPage(out _);
        var (view, window, reveals) = Mount(vm);
        Assert.Contains("motion", view.Classes);
        try
        {
            // 7 张卡各一个揭示层 (语言/自定义热键/鼠标参数/键盘布局/触发延时/命令框皮肤/路径变量;
            // 「程序分组」卡是 Click 事件而非分区体)。条数变动 = 新增/删卡时忘了同步本断言。
            Assert.Equal(7, reveals.Count);

            // 接线对账: 每层都必须收到过初值 (附加属性非 null ⇒ 绑定真的接上了);
            // 漏接的新卡会停在 null, 界面表现为永远不展开 —— 此处必红。
            Assert.All(reveals, r => Assert.NotNull(SectionUnroll.GetIsOpen(r)));

            // 每层结构完整: 一个内容 StackPanel.sectionBody + 一条前缘卷曲带 Border.curl
            foreach (var r in reveals)
            {
                Assert.NotNull(r.GetVisualDescendants().OfType<StackPanel>()
                    .FirstOrDefault(p => p.Classes.Contains("sectionBody")));
                Assert.NotNull(r.GetVisualDescendants().OfType<Border>()
                    .FirstOrDefault(b => b.Classes.Contains("curl")));
            }

            // 装载期不播动画: 收起态当场落 0 高 + 隐藏; 默认展开的「设置触发延时」落 ∞ 高且可见,
            // 都不带 .unroll/.rollup (挂载瞬间的露出交给卡片入场级联)
            Assert.DoesNotContain(reveals, r => r.Classes.Contains(SectionUnroll.UnrollClass)
                                                || r.Classes.Contains(SectionUnroll.RollUpClass));
            var delayReveal = reveals.Single(r => r.IsVisible);
            Assert.True(double.IsPositiveInfinity(delayReveal.MaxHeight));
            Assert.All(reveals.Where(r => !ReferenceEquals(r, delayReveal)),
                r => Assert.False(r.IsVisible, "收起态分区体装载期没有隐藏"));

            // 手风琴换卡: 旧卡当帧进卷起态 (.rollup), 新卡的摊开被串行闸推迟到卷起跑完
            vm.ToggleSectionCommand.Execute("mouse"); // 手风琴: 关「设置触发延时」+ 开「修改鼠标参数」
            Dispatcher.UIThread.RunJobs();
            Assert.True(vm.ShowMouseOption);
            Assert.False(vm.ShowKeymapDelay);
            // 旧卡当帧即进入卷起态, 且**当帧不得隐藏** (退场动画要跑完才隐藏); 新卡的摊开被串行闸
            // 推迟到卷起跑完 (见 s_rollupGate) ⇒ 这两条断言必须在等闸之前
            Assert.Contains(SectionUnroll.RollUpClass, delayReveal.Classes);
            Assert.True(delayReveal.IsVisible,
                "卷起动画尚未跑完就 IsVisible=false: 退场被硬切 (SectionUnroll 应等动画结束)");
            // 串行闸的回归锁: 换卡当帧只允许旧卡卷起, 新卡的摊开必须等闸 (去掉闸这条必红)
            Assert.DoesNotContain(reveals, r => r.Classes.Contains(SectionUnroll.UnrollClass));

            var unrolling = WaitForUnroll(reveals);
            Assert.NotSame(delayReveal, unrolling);
            Assert.True(unrolling.IsVisible, "摊开态分区体不可见: 状态机与 IsVisible 脱钩");

            // 7 个分区逐一展开: 每次都必须是**另一个**揭示层亮起 —— 端到端证明 7 张卡全部接线
            string[] keys = ["language", "customhotkeys", "mouse", "layout", "delay", "skin", "pathvars"];
            var seen = new HashSet<Control>();
            foreach (var key in keys)
            {
                vm.ToggleSectionCommand.Execute(key);
                Dispatcher.UIThread.RunJobs();
                var lit = WaitForUnroll(reveals);
                Assert.True(lit.IsVisible, $"「{key}」摊开的分区体不可见");
                Assert.True(seen.Add(lit), $"「{key}」与其它分区共用同一揭示层: 手风琴串台");
            }
            Assert.Equal(7, seen.Count);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>
    /// 减少动效逃生口: KEYFLUX_NO_MOTION=1 时不挂类 (没有动画可播), 且必须**同步**落终态 ——
    /// 否则会留 160~200ms 的空白占位 (比动画本身更难看)。
    /// </summary>
    [AvaloniaFact]
    public void SettingsPage_Reduced_Motion_Applies_Final_State_Immediately()
    {
        Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", "1");
        try
        {
            var vm = NewPage(out _);
            var (_, window, reveals) = Mount(vm);
            try
            {
                var delayReveal = reveals.Single(r => r.IsVisible);

                vm.ToggleSectionCommand.Execute("mouse");
                Dispatcher.UIThread.RunJobs();

                Assert.False(delayReveal.IsVisible, "无动效档必须立即隐藏 (不得留空白占位)");
                Assert.Equal(0, delayReveal.MaxHeight);
                Assert.DoesNotContain(reveals, r => r.Classes.Contains(SectionUnroll.UnrollClass));
                Assert.DoesNotContain(reveals, r => r.Classes.Contains(SectionUnroll.RollUpClass));

                var opened = reveals.Single(r => r.IsVisible);
                Assert.True(double.IsPositiveInfinity(opened.MaxHeight), "无动效档展开后必须放开高度上限");
            }
            finally
            {
                window.Close();
            }
        }
        finally
        {
            Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", null);
        }
    }

    /// <summary>
    /// 摊开/卷起的**逐帧实测** (headless 里代码驱动的动画确实会随真实时间推进, 实测 MaxHeight
    /// 能读到插值中的中间值): 高度必须"逐步"长到内容自然高再放开上限, 卷起时逐步回落到 0,
    /// 且**动画跑完才** IsVisible=false —— 后者正是"收起硬切"这个老问题的回归锁。
    /// </summary>
    [AvaloniaFact]
    public void SettingsPage_Unroll_Grows_Height_Then_Settles()
    {
        var vm = NewPage(out _);
        var (_, window, reveals) = Mount(vm);
        try
        {
            var toggleAt = Stopwatch.StartNew();
            vm.ToggleSectionCommand.Execute("mouse");
            Dispatcher.UIThread.RunJobs();
            var target = WaitForUnroll(reveals);

            var grown = SampleUntilClassGone(target, SectionUnroll.UnrollClass);
            toggleAt.Stop();
            // 撤类必须等满「令牌时长 + 余量」: 揭示了 MaxHeight 的代码动画与 XAML 类动画
            // (卷曲带/内容落平) 走两个时钟, 类动画起步更晚; 早撤类 = 把它们掐在半途,
            // 表现就是"完全展开到位那一瞬闪一下"(2026-09-24 用户报障的根因)。
            Assert.True(toggleAt.Elapsed >= ClaudeMotion.Unroll + TimeSpan.FromMilliseconds(50),
                $"撤类过早 ({toggleAt.ElapsedMilliseconds}ms < {ClaudeMotion.Unroll.TotalMilliseconds + 50}ms): 类动画会被掐断");
            Assert.DoesNotContain(SectionUnroll.UnrollClass, target.Classes);
            Assert.True(double.IsPositiveInfinity(target.MaxHeight), "摊开结束后必须放开 MaxHeight 上限");
            var natural = target.Bounds.Height;
            Assert.True(natural > 20, $"摊开后应有真实内容高, 实际 {natural:0.#}");

            // 过程实测 (mouse 卡, 自然高 540): MaxH 14→40→120→244→379→485→537 后落 ∞。
            // 断言取宽裕边界: 必须从贴 0 起步、收到接近终高, 且中途有多个严格介于两端的采样
            // ⇒ 是"逐步摊开"而非一步跳到位; 单向不回退。
            Assert.True(grown.Count >= 5, $"摊开中间态采样过少 ({grown.Count}): 动画可能没在推进");
            Assert.True(grown[0] < natural * 0.3, $"摊开起步过高 ({grown[0]:0.#}): 不是从 0 长起");
            Assert.True(grown[^1] > natural * 0.8, $"摊开收尾未接近终高 ({grown[^1]:0.#} < {natural:0.#})");
            AssertMonotonic(grown, increasing: true);

            // 卷起: 高度逐步回落; 动画未结束前必须仍可见, 结束后才隐藏且归 0
            vm.ToggleSectionCommand.Execute("layout");
            Dispatcher.UIThread.RunJobs();
            Assert.Contains(SectionUnroll.RollUpClass, target.Classes);
            Assert.True(target.IsVisible, "卷起动画未跑完就 IsVisible=false: 退场被硬切");

            var shrunk = SampleUntilClassGone(target, SectionUnroll.RollUpClass);
            Assert.DoesNotContain(SectionUnroll.RollUpClass, target.Classes);
            Assert.Equal(0, target.MaxHeight);
            Assert.False(target.IsVisible);
            Assert.True(shrunk.Count >= 4, $"卷起中间态采样过少 ({shrunk.Count})");
            Assert.True(shrunk[0] > natural * 0.8, $"卷起起步未接近满高 ({shrunk[0]:0.#})");
            Assert.True(shrunk[^1] < natural * 0.3, $"卷起收尾未接近 0 ({shrunk[^1]:0.#})");
            AssertMonotonic(shrunk, increasing: false);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>
    /// 等"某一张卡真的进入摊开态"。换卡时摊开被串行闸推迟 (先等旧卡卷起跑完), 故不能当帧断言;
    /// 同时它也顺带守住"闸不会永远不放行" (超时即红)。
    /// </summary>
    private static Border WaitForUnroll(List<Border> reveals)
    {
        var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        while (DateTime.UtcNow < deadline)
        {
            var hit = reveals.Where(r => r.Classes.Contains(SectionUnroll.UnrollClass)).ToList();
            if (hit.Count == 1) return hit[0];
            AvaloniaHeadlessPlatform.ForceRenderTimerTick();
            Dispatcher.UIThread.RunJobs();
            System.Threading.Thread.Sleep(5);
        }

        Assert.Fail("等待摊开态超时: 串行闸可能没放行 (或无卡进入摊开)");
        return null!;
    }

    /// <summary>
    /// 轮询 MaxHeight 直到指定类被状态机撤下 (动画收尾的判据), 返回过程采样。
    /// 必须显式推进 headless 渲染时钟: 否则动画被粗粒度地一次跳到底 (实测: 不 tick 时
    /// 只读到 7.68 → 517.47 两个值), 中间态断言形同虚设; 带 tick 后可读到完整曲线。
    /// </summary>
    private static List<double> SampleUntilClassGone(Control host, string cls)
    {
        var samples = new List<double>();
        var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        while (DateTime.UtcNow < deadline && host.Classes.Contains(cls))
        {
            if (double.IsFinite(host.MaxHeight)) samples.Add(host.MaxHeight);
            AvaloniaHeadlessPlatform.ForceRenderTimerTick();
            Dispatcher.UIThread.RunJobs();
            System.Threading.Thread.Sleep(10);
        }
        return samples;
    }

    private static void AssertMonotonic(List<double> samples, bool increasing)
    {
        Assert.True(samples.Count >= 3, $"采样过少 ({samples.Count}): 动画可能没在推进");
        for (var i = 1; i < samples.Count; i++)
        {
            var delta = samples[i] - samples[i - 1];
            Assert.True(increasing ? delta >= -0.01 : delta <= 0.01,
                $"高度未单向变化: {samples[i - 1]:0.##} → {samples[i]:0.##}");
        }
    }

    /// <summary>
    /// 摊开/卷起配方对账: 两向各 2 条 XAML 动画 (卷曲带 + 内容落平), 时长必须来自皮肤令牌
    /// (无散落字面量); 内容落平的三处 RenderTransform 逐位一致且能连续插值 —— 换成自造写法
    /// 破坏逐项插值即红。另一摊一卷必须共用同一缓动 (节奏对称为"连贯"的前提)。
    /// </summary>
    [AvaloniaFact]
    public void SettingsPage_Unroll_Recipe_Matches_Skin_Tokens()
    {
        var view = new SettingsPageView();
        var animations = view.Styles.OfType<Style>().SelectMany(s => s.Animations).OfType<Animation>().ToList();

        var unrollAnims = animations.Where(a => a.Duration == ClaudeMotion.Unroll).ToList();
        var rollAnims = animations.Where(a => a.Duration == ClaudeMotion.Roll).ToList();
        Assert.Equal(2, unrollAnims.Count); // 卷曲带 + 内容落平
        Assert.Equal(2, rollAnims.Count);

        foreach (var a in unrollAnims.Concat(rollAnims)) Assert.IsType<SineEaseInOut>(a.Easing);

        // C# 侧曲线 (RevealHeightMotion.Ease, 驱动 MaxHeight) 必须与 XAML 侧 4 条动画同型同参:
        // 两处曲线一旦漂移, 一摊一卷就是两套手感 (跨文件契约, 只锁类型不锁实例)。
        Assert.IsType<SineEaseInOut>(RevealHeightMotion.Ease);
        foreach (var a in unrollAnims.Concat(rollAnims))
        {
            Assert.Equal(RevealHeightMotion.Ease.Ease(0.37), a.Easing!.Ease(0.37), 9);
        }

        // 内容落平: 摊开 = 抬起 → 归位; 卷起 = 归位 → 抬起 (同姿态反向走)
        var bodyUnroll = unrollAnims.Single(a => a.Children.Any(HasRenderTransform));
        var poses = bodyUnroll.Children.Select(RenderTransformOf).ToList();
        AssertPose(poses[0], "translateY(-3px)");
        AssertPose(poses[^1], "translateY(0px)");
        var bodyRoll = rollAnims.Single(a => a.Children.Any(HasRenderTransform));
        AssertPose(RenderTransformOf(bodyRoll.Children.First()), "translateY(0px)");
        AssertPose(RenderTransformOf(bodyRoll.Children.Last()), "translateY(-3px)");

        // 落平**必须是纯位移, 不含缩放**: 缩放会改变内容实际高度, 而揭示层按内容自然高裁切,
        // 动画末段就会出现"内容已露完、裁切边还在长"的空档, 归位那一刻补上 = 一次可见跳动
        // (2026-09-24 用户报「到位瞬间闪一下」的成因之一)。M22 全帧必须恒等于 1。
        foreach (var p in poses.Concat([RenderTransformOf(bodyRoll.Children.First()), RenderTransformOf(bodyRoll.Children.Last())]))
        {
            Assert.Equal(1d, p.Value.M22, 6);
        }

        // 逐项插值连续: 中点必须落在两端之间
        var half = TransformOperations.Interpolate(poses[0], poses[^1], 0.5).Value;
        Assert.Equal(1d, half.M22, 6);
        Assert.InRange(half.M32, -2.999, -0.001);

        // 缓动对称性: SineEaseInOut 起止都柔, 一摊一卷共用它才有连贯手感
        var e = RevealHeightMotion.Ease;
        Assert.Equal(1d - e.Ease(0.25), e.Ease(0.75), 4);

        // 揭示动画的"到位保持": 0% → 90% (达终值) → 100% (保持终值)。末两帧等值 ⇒ 撤类把
        // MaxHeight 落回 ∞ 时残差恒为 0, 不会出现高度跳变 (2026-09-24 修闪烁的硬约束之二)。
        var reveal = RevealHeightMotion.BuildAnimation(0, 540, ClaudeMotion.Unroll);
        Assert.Equal(ClaudeMotion.Unroll, reveal.Duration);
        Assert.Equal([0d, 0.9, 1d], reveal.Children.Select(k => k.Cue.CueValue).ToList());
        var heights = reveal.Children
            .Select(k => (double)k.Setters.OfType<Setter>()
                .Single(s => s.Property == Layoutable.MaxHeightProperty).Value!).ToList();
        Assert.Equal([0d, 540d, 540d], heights);
    }

    /// <summary>
    /// 跨文件契约: 状态机里的类名常量必须与 XAML 选择器里的锚点类逐字对应。
    /// 动机 (模块化): 类名是 C# 与 XAML 之间**唯一**的接线方式, 却以裸字符串存在两处 ——
    /// 改常量而忘了改选择器, 动画会静默失效 (类挂上了但没人监听), 界面上只剩"没有卷曲带"这种
    /// 症状不明的缺陷。此处把两侧对账钉死。
    /// </summary>
    [AvaloniaFact]
    public void SectionUnroll_Class_Names_Match_Xaml_Selectors()
    {
        var view = new SettingsPageView();
        var selectorTexts = view.Styles.OfType<Style>()
            .Select(s => s.Selector?.ToString() ?? string.Empty)
            .Where(t => t.Length > 0)
            .ToList();

        foreach (var cls in new[] { SectionUnroll.UnrollClass, SectionUnroll.RollUpClass, "reveal", "curl", "sectionBody" })
        {
            Assert.True(selectorTexts.Any(t => t.Contains(cls, StringComparison.Ordinal)),
                $"XAML 样式里找不到锚点类「{cls}」: 类名常量与选择器脱钩。已见选择器: {string.Join(" | ", selectorTexts)}");
        }
    }

    /// <summary>
    /// 输入校验 (安全面): 高度驱动只接受有限且落在合理区间的值 —— 非有限值会让 <c>Measure</c> 抛
    /// (异常在属性变更回调里抛出会顺绑定系统外溢), 病态超大值会让 400ms 高度动画变成逐帧全页布局。
    /// 另外确认"量不到"这一负路径返回 0 而不是抛。
    /// </summary>
    [AvaloniaFact]
    public void RevealHeightMotion_Validation_Rejects_Unusable_Heights()
    {
        Assert.False(RevealHeightMotion.CanDrive(0));
        Assert.False(RevealHeightMotion.CanDrive(1));
        Assert.False(RevealHeightMotion.CanDrive(-5));
        Assert.False(RevealHeightMotion.CanDrive(double.NaN));
        Assert.False(RevealHeightMotion.CanDrive(double.PositiveInfinity));
        Assert.False(RevealHeightMotion.CanDrive(20001)); // 病态超大: 拒绝动画、直落终态
        Assert.True(RevealHeightMotion.CanDrive(540));
        Assert.True(RevealHeightMotion.CanDrive(20000));

        // 未挂树/无父级 ⇒ 量不到自然高 ⇒ 返回 0 (调用方据此走直落终态, 而不是抛)
        Assert.Equal(0, RevealHeightMotion.MeasureNaturalHeight(new Border()));
    }

    private static bool HasRenderTransform(KeyFrame kf)
        => kf.Setters.OfType<Setter>().Any(s => s.Property == Visual.RenderTransformProperty);

    private static void AssertPose(TransformOperations actual, string spec)
    {
        var want = TransformOperations.Parse(spec).Value;
        Assert.Equal(want.M11, actual.Value.M11, 4);
        Assert.Equal(want.M22, actual.Value.M22, 4);
        Assert.Equal(want.M31, actual.Value.M31, 4);
        Assert.Equal(want.M32, actual.Value.M32, 4);
    }

    private static TransformOperations RenderTransformOf(KeyFrame kf)
        => (TransformOperations)kf.Setters.OfType<Setter>()
            .Single(s => s.Property == Visual.RenderTransformProperty).Value!;

    [AvaloniaFact]
    public void MainWindow_Page_Switch_Uses_CrossFade_With_Skin_Token()
    {
        var main = new MainViewModel(new BackendSessionOptions())
        {
            Config = new Config { Options = new Options() },
        };
        var window = new MainWindow(main);
        // 只构造不 Show (Show 会走 Opened -> InitializeAsync 拉起后端子进程, 见 SkinContractTests);
        // TransitioningContentControl 是 XAML 直接子元素, 构造期已挂在 Content 视觉子树上,
        // 从 Content 根遍历即可, 无需窗口模板应用
        var contentRoot = (Visual)window.Content!;
        var host = contentRoot.GetVisualDescendants().OfType<TransitioningContentControl>().First();
        var fade = Assert.IsType<CrossFade>(host.PageTransition);
        // 时长必须来自皮肤令牌 ClaudeMotion.Standard (换肤整组调动效的契约), 而非散落字面量
        Assert.Equal(ClaudeMotion.Standard, fade.Duration);
    }
}
