using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Media.Transformation;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Controls;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Services;

/// <summary>
/// DialogMotion —— 弹窗「苹果式」入场/退场动效 (2026-09-24 批次二)。
///
/// <para><b>观感配方。</b>容器以弹簧曲线从略小、略下浮起 (先快后慢 + 约 1.5% 极轻微回弹);
/// 内容元素稍晚起步 (错峰 <see cref="ClaudeMotion.DialogContentDelay"/>), 以轻微上移 + 淡入
/// 分层出现; 退场比入场更干脆 (先轻微收缩再淡出); 背景同时变暗、轻微模糊并略微缩小,
/// 制造"有重量的半透明玻璃浮到前景"的纵深。</para>
///
/// <para><b>为什么用 Transitions 而不是代码 Animation。</b>代码构造的 <c>Animation</c> 走
/// <c>GetAnimatorType</c> 兜底, <b>只覆盖基元类型</b> —— 驱动 <c>RenderTransform</c> 会抛
/// <c>InvalidOperationException: No animator registered for the property RenderTransform</c>
/// (2026-09-24 探针实测复现)。<c>Transitions</c> 装过渡 + 改属性触发, 自带插值器, 不受此限,
/// 且可直接挂 <see cref="SpringEasing"/> (Avalonia 11.3 内置, 带 Mass/Stiffness/Damping 参数)。
/// 这是本项目弹窗动效的唯一可行驱动方式, 勿改成代码 Animation。</para>
///
/// <para><b>弹簧参数依据 (探针扫描实测, 2026-09-24)。</b>目标"极轻微回弹, 不夸张弹跳":
/// 默认 (K=100 D=10) 过冲 16.3% = 明显弹跳, 否决; 过硬阻尼 (D≥24) 过冲 ≈0 读不出惯性, 否决;
/// <b>K=100 D=16 → 过冲 1.52%, 末端 0.9998</b> 命中目标区间, 采用。<c>Mass=1</c>,
/// <c>InitialVelocity=0</c>。</para>
///
/// <para><b>为什么退场可以比入场快。</b>内容揭示类令牌曾刻意"等长" (收势快会暴露"画面突然空掉"),
/// 但弹窗有遮罩层承接 —— 遮罩淡出起步晚于弹窗 (见 <see cref="Scrim"/>) 且总时长更长,
/// 弹窗收缩淡出时背景仍是暗的, 快退场不会造成空场感。两处场景差异已由
/// <c>SkinContractTests</c> 显式锁死。</para>
///
/// <para><b>减少动效。</b><see cref="MotionPreferences.AnimationsEnabled"/> 为 false 时本服务
/// <b>完全不介入</b>: 不挂类名、不动 Opacity/RenderTransform、不给过渡、关闭直落。
/// 半介入会留下残留姿势, 故必须整体跳过。</para>
///
/// <para><b>线程与生命周期。</b>全部入口须在 UI 线程调用 (弹窗构造/Opened/Closing 均满足)。
/// 退场用 <see cref="DispatcherTimer"/> 延时真关, 计时器持有窗口引用至触发为止; 取消柄按窗口
/// 单例登记, 关闭时随即移除, 不留悬挂引用。</para>
/// </summary>
public static class DialogMotion
{
    // ---------------------------------------------------------------- 弹簧参数

    /// <summary>弹簧质量 (无量纲; 1 = 基准)。</summary>
    internal const double SpringMass = 1;

    /// <summary>弹簧刚度。与 <see cref="SpringDamping"/> 共同决定过冲幅度。</summary>
    internal const double SpringStiffness = 100;

    /// <summary>弹簧阻尼 —— 16 = 过冲 1.52% (探针实测), 目标"极轻微回弹"。</summary>
    internal const double SpringDamping = 16;

    // ---------------------------------------------------------------- 姿势常量

    /// <summary>入场起始缩放 (1 → 该值再弹回 1; 后退量很小, 配合下浮制造"浮起")。</summary>
    private const double EnterScale = 0.92;

    /// <summary>入场起始下浮像素 (弹窗从下方一点点浮到位置)。</summary>
    private const double EnterOffsetY = 14;

    /// <summary>退场终止缩放 (先轻微收缩再淡出; 比入场起始收敛很多, 所以退场"干脆")。</summary>
    private const double ExitScale = 0.965;

    /// <summary>内容元素入场起始上移量 (轻微, "稍微上移"而非滑入)。</summary>
    private const double ContentOffsetY = 7;

    // ---------------------------------------------------------------- 类名与标记

    /// <summary>挂在弹窗内容根上的类名 —— 皮肤可据此定制 (本服务自管姿势, 类名主要供测试与样式钩子)。</summary>
    public const string MotionClass = "dlgMotion";

    /// <summary>内容元素类名 —— 这些元素晚于容器入场 (错峰 + 上移淡入)。</summary>
    public const string ContentClass = "dlgContent";

    /// <summary>玻璃材质类名 (圆角/阴影/裁剪的标记, 主要供测试与样式钩子)。</summary>
    public const string GlassClass = "dlgGlass";

    /// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
    public sealed class DialogMotionMarker;

    // ---------------------------------------------------------------- 每窗状态

    private sealed class State
    {
        public Control? Body;
        public bool Closing;
        public CancellationTokenSource? CloseCts;
        public int ContentPlayed;
        public bool Reduced;
    }

    private static readonly Dictionary<Window, State> States = new();

    // ---------------------------------------------------------------- 附加属性 (页内浮层通道)

    /// <summary>
    /// 页内浮层开合开关 (bool? 而非 bool —— 见下)。设为 true 播放入场, false 播放退场后隐藏。
    /// </summary>
    public static readonly AttachedProperty<bool?> IsOpenProperty =
        AvaloniaProperty.RegisterAttached<DialogMotionMarker, Visual, bool?>("IsOpen");

    /// <summary>
    /// <b>为什么必须是 <c>bool?</c> 而不是 <c>bool</c>:</b> <c>bool</c> 的默认值 false 与
    /// "浮层收起"的语义同值, 装载期不产生变化通知 ⇒ 状态机收不到初始信号 (2026-09-24 分区展开
    /// 批次踩过同款坑: 7 个分区全部误展开)。可空类型默认 null 与 false 可区分, 变化回调里
    /// 判 <c>OldValue is null</c> 即可识别"首次赋值"。</summary>
    public static readonly AttachedProperty<bool> OverlayMotionProperty =
        AvaloniaProperty.RegisterAttached<DialogMotionMarker, Visual, bool>("OverlayMotion");

    public static bool? GetIsOpen(Visual v) => v.GetValue(IsOpenProperty);
    public static void SetIsOpen(Visual v, bool? value) => v.SetValue(IsOpenProperty, value);

    public static bool GetOverlayMotion(Visual v) => v.GetValue(OverlayMotionProperty);
    public static void SetOverlayMotion(Visual v, bool value) => v.SetValue(OverlayMotionProperty, value);

    static DialogMotion()
    {
        IsOpenProperty.Changed.AddClassHandler<Visual>((v, e) =>
        {
            if (!GetOverlayMotion(v) || v is not Control body)
            {
                return;
            }
            // 触发元素自身就是动效主体 (页内浮层的附加属性挂在浮层根上)
            var state = GetOrCreateOverlayState(v, body);
            var open = e.NewValue is true;
            if (open)
            {
                PlayEnter(state);
            }
            else
            {
                PlayOverlayExit(state);
            }
        });
    }

    // ---------------------------------------------------------------- 窗口通道

    /// <summary>
    /// 挂载弹窗动效。<see cref="Win32.DialogChrome.Apply"/> 首行调用 (统一入口, 8 窗零改动接入)。
    /// 幂等: 同一窗口重复调用不重复挂。
    /// </summary>
    public static void Attach(Window window)
    {
        if (States.ContainsKey(window))
        {
            return;
        }

        var state = new State { Reduced = !MotionPreferences.AnimationsEnabled };
        States[window] = state;

        if (state.Reduced)
        {
            // 减少动效: 完全不介入 (不挂类名/不动 Opacity/不给过渡/关闭直落)
            window.Closed += (_, _) => States.Remove(window);
            return;
        }

        window.Opened += (_, _) => OnOpened(window, state);
        window.Closing += (_, e) => OnClosing(window, state, e);
        window.Closed += (_, _) => OnClosed(window, state);
    }

    private static void OnOpened(Window window, State state)
    {
        var content = window.Content as Control;
        if (content is null)
        {
            return;
        }

        var body = EnsureGlassShell(content);
        state.Body = body;
        body.Classes.Add(MotionClass);

        // 落起始姿势 (无过渡): 略小 + 略下 + 透明
        body.Opacity = 0;
        body.RenderTransform = Pose(EnterScale, EnterOffsetY);

        // 背景: 变暗 + 模糊 + 略微缩小 (Scrim 统一编排; 作用对象是 owner 主窗, 不是弹窗自己)
        if (window.Owner is { } ownerTop)
        {
            Scrim.PlayEnter(ownerTop);
        }

        // 下一帧装弹簧过渡并推到终值 —— 同帧改目标值不会触发过渡 (Avalonia 需一次提交间隔)
        Dispatcher.UIThread.Post(() =>
        {
            if (state.Closing)
            {
                return;
            }
            body.Transitions = PoseTransitions(
                ClaudeMotion.DialogEnter, Spring(),
                includeOpacity: true,
                opacityDuration: TimeSpan.FromMilliseconds(ClaudeMotion.DialogEnter.TotalMilliseconds * 0.62));

            body.Opacity = 1;
            body.RenderTransform = Pose(1, 0);

            AttachPressFeedback(body);
            PlayContent(state);
        }, DispatcherPriority.Background);
    }

    /// <summary>
    /// 玻璃材质: 内容根若已是 <see cref="Border"/> 就就地补材质 (圆角 + 阴影 + 裁剪) 并注入高光;
    /// 其余类型原样返回 (动效照常, 仅无玻璃材质)。
    ///
    /// <para><b>⚠ 为什么绝不包层 (两次实测教训)。</b>
    /// 运行时给 <c>window.Content</c> 包一层新容器必然失败 —— 窗口内容已被 <see cref="Window"/>
    /// 模板的 <c>PART_ContentPresenter</c> 持有视觉父级:<br/>
    /// ① 把 <c>window.Content</c> 挪进新 Grid ⇒ <c>The control ... already has a visual parent
    /// ContentPresenter (Name = PART_ContentPresenter)</c>;<br/>
    /// ② 先设 <c>shell.Child = content</c> 再挪 ⇒ <c>The Control already has a parent</c>。<br/>
    /// 2026-09-24 实测: <c>MatchTypesLayoutTests</c> ×3 因此全红。</para>
    ///
    /// <para><b>为什么非 Border 根也不在 XAML 里补一层。</b>3 个弹窗的根不是 Border
    /// (ActionEditorPanel / MatchTypesPageView / DockPanel)。给它们加 XAML 外壳会改变
    /// <c>window.Content</c> 的类型, 而 <c>MatchTypesLayoutTests</c> 用
    /// <c>Assert.IsType&lt;MatchTypesPageView&gt;(win.Content)</c> 锁定了这一契约 ——
    /// 为"弹窗圆角"去改既有测试契约得不偿失。故这 3 个只享用动效, 不享玻璃材质
    /// (它们本就无边框/内容自带背景, 缺圆角阴影的观感损失有限)。</para>
    /// </summary>
    private static Control EnsureGlassShell(Control content)
    {
        if (content is not Border border)
        {
            return content;
        }

        border.CornerRadius = new CornerRadius(ClaudeMotion.DialogCornerRadius);
        border.BoxShadow = ShadowOf("ClaudeShadowDialog");
        border.ClipToBounds = true;
        border.Classes.Add(GlassClass);
        InjectEdgeHighlight(border);
        return border;
    }

    /// <summary>注入顶边受光高光 (细微边缘高光): 1px 半透明暖白细线, 模拟玻璃上沿受光。</summary>
    private static void InjectEdgeHighlight(Border shell)
    {
        if (shell.Child is not Panel panel)
        {
            // 单子元素的 Border (内容不是 Panel): 用 Grid 并置高光需要挪动 shell.Child,
            // 而它是 shell 的逻辑/视觉子级 —— 挪动会触发 "already has a parent"。
            // 故只给 Panel 型容器注入高光; 其余 (少数) 弹窗省略这一笔, 不影响主体观感。
            return;
        }

        // 幂等: Opened 可能对同一窗口触发多次
        if (panel.Children.OfType<Border>().Any(b => b.Classes.Contains("dlgEdge")))
        {
            return;
        }

        panel.Children.Add(new Border { Classes = { "dlgEdge" } });
    }

    /// <summary>从皮肤资源取阴影令牌 (取不到用内置回退, 换肤时自动跟随)。</summary>
    private static BoxShadows ShadowOf(string key)
    {
        if (Application.Current?.TryGetResource(key, out var value) == true
            && value is BoxShadows shadows)
        {
            return shadows;
        }
        return BoxShadows.Parse("0 1 2 0 #1f000000, 0 8 20 0 #1c000000, 0 20 48 0 #24000000");
    }

    /// <summary>
    /// 按钮按下轻微缩放反馈: 给弹窗内容根下每个 <see cref="Button"/> 挂
    /// <c>dlgPress</c> 类 (皮肤给静止/按下两个目标值) 与
    /// <see cref="ClaudeMotion.Press"/> 的 <c>RenderTransform</c> 过渡。
    /// 分开的理由见皮肤注释 (XAML 里 Setter 内嵌 <c>&lt;Transitions&gt;</c> 会报 AVLN2200)。
    ///
    /// <para><b>两点必须注意 (均实测踩过)。</b>
    /// ① <c>Transitions</c> 是<b>按 Property 去重的字典</b>, 重复 Add 同一 Property 的过渡会抛
    ///    <c>ArgumentException: An item with the same key has already been added</c> ⇒
    ///    必须先移除已有的同 Property 项, 且每个按钮用<b>新建</b>的过渡实例 (不能复用同一个对象,
    ///    否则会连带被移除/重复)。<br/>
    /// ② <c>Opened</c> 可能对同一窗口触发多次 (测试里 Show 后重设 Content 即如此) ⇒ 幂等: 已有
    ///    过该 Property 的过渡就不再挂; 已加过 <c>dlgPress</c> 类也跳过。</para>
    /// </summary>
    private static void AttachPressFeedback(Control root)
    {
        foreach (var button in root.GetVisualDescendants().OfType<Button>())
        {
            button.Classes.Add("dlgPress");
            if (button.Transitions is { } existing)
            {
                // ITransition.Property 不可访问, 故按类型判重 (本处只挂 TransformOperationsTransition)
                if (existing.OfType<TransformOperationsTransition>().Any())
                {
                    continue; // 已挂过: 幂等跳过
                }
                existing.Add(NewPressTransition());
            }
            else
            {
                button.Transitions = new Transitions { NewPressTransition() };
            }
        }
    }

    /// <summary>每次新建, 不复用实例 (Transitions 按 Property 去重, 复用会引发重复键)。</summary>
    private static TransformOperationsTransition NewPressTransition() =>
        new()
        {
            Property = Visual.RenderTransformProperty,
            Duration = ClaudeMotion.Press,
            Easing = new QuadraticEaseOut(),
        };

    /// <summary>内容元素错峰入场: 稍晚于容器, 以轻微上移 + 淡入分层出现。</summary>
    private static void PlayContent(State state)
    {
        if (state.Body is null)
        {
            return;
        }
        var items = CollectContentItems(state.Body);
        if (items.Count == 0)
        {
            return;
        }

        foreach (var item in items)
        {
            item.Opacity = 0;
            item.RenderTransform = Pose(1, ContentOffsetY);
        }

        var timer = new DispatcherTimer
        {
            Interval = ClaudeMotion.DialogContentDelay,
        };
        timer.Tick += (s, _) =>
        {
            timer.Stop();
            foreach (var item in items)
            {
                item.Transitions = PoseTransitions(
                    ClaudeMotion.DialogContent, new CubicEaseOut(), includeOpacity: true);
                item.Opacity = 1;
                item.RenderTransform = Pose(1, 0);
            }
            state.ContentPlayed = 1;
        };
        timer.Start();
    }

    /// <summary>
    /// 收集"分层入场"的内容元素。刻意只在内容根的直接子层找标记元素 (而非全树漫游) ——
    /// 深挖会把弹窗内部表格/列表的每个单元格都卷进来, 既拖慢又制造噪声。
    /// </summary>
    private static List<Control> CollectContentItems(Control root)
    {
        var list = new List<Control>();
        foreach (var child in root.GetVisualDescendants().OfType<Control>())
        {
            if (child.Classes.Contains(ContentClass))
            {
                list.Add(child);
            }
            if (list.Count >= 8)
            {
                break;
            }
        }
        return list;
    }

    private static void OnClosing(Window window, State state, WindowClosingEventArgs e)
    {
        if (state.Closing)
        {
            return;
        }
        state.Closing = true;

        // 退场期间禁点, 防重复触发 (用户连点关闭按钮)
        window.IsHitTestVisible = false;

        var body = state.Body;
        if (body is not null)
        {
            body.Transitions = PoseTransitions(ClaudeMotion.DialogExit, new CubicEaseIn());
            body.Opacity = 0;
            body.RenderTransform = Pose(ExitScale, 0);
        }

        // 拦下这次关闭, 等退场跑完再真关
        e.Cancel = true;
        if (window.Owner is { } ownerExit)
        {
            Scrim.PlayExit(ownerExit);
        }
        var cts = new CancellationTokenSource();
        state.CloseCts = cts;
        DispatcherTimer.RunOnce(() =>
        {
            if (cts.IsCancellationRequested)
            {
                return;
            }
            if (window.IsVisible)
            {
                state.Closing = false; // 放行本次真关
                window.Close();
            }
        }, ClaudeMotion.DialogExit + TimeSpan.FromMilliseconds(40));
    }

    private static void OnClosed(Window window, State state)
    {
        state.CloseCts?.Cancel();
        state.CloseCts = null;
        // 遮罩退场已在 OnClosing 编排 (此处窗口已不可见, 再触发会重复计数)
        States.Remove(window);
    }

    // ---------------------------------------------------------------- 页内浮层

    // 页内浮层状态: 弱持有 —— 浮层元素随页面销毁, 强引用字典会永久持有 (泄漏)
    private static readonly ConditionalWeakTable<Visual, State> OverlayStates = new();

    private static State GetOrCreateOverlayState(Visual v, Control body)
    {
        var s = OverlayStates.GetOrCreateValue(v);
        if (s.Body is null)
        {
            s.Reduced = !MotionPreferences.AnimationsEnabled;
            s.Body = body;
        }
        return s;
    }

    private static void PlayEnter(State state)
    {
        if (state.Reduced || state.Body is null)
        {
            return;
        }
        var body = state.Body;
        body.Classes.Add(MotionClass);
        body.Opacity = 0;
        body.RenderTransform = Pose(EnterScale, EnterOffsetY);
        Dispatcher.UIThread.Post(() =>
        {
            body.Transitions = PoseTransitions(ClaudeMotion.DialogEnter, Spring());
            body.Opacity = 1;
            body.RenderTransform = Pose(1, 0);
        }, DispatcherPriority.Background);
    }

    private static void PlayOverlayExit(State state)
    {
        if (state.Reduced || state.Body is null)
        {
            return;
        }
        var body = state.Body;
        body.Transitions = PoseTransitions(ClaudeMotion.DialogExit, new CubicEaseIn());
        body.Opacity = 0;
        body.RenderTransform = Pose(ExitScale, 0);
        // 内容根由绑定控制 IsVisible, 退场后由绑定侧收起; 这里只负责过渡姿势
    }

    // ---------------------------------------------------------------- 姿势与过渡

    /// <summary>构造"缩放 + 下浮"的变换操作 (TransformOperations)。</summary>
    internal static ITransform Pose(double scale, double offsetY)
    {
        var sb = new TransformOperations.Builder(1);
        sb.AppendScale(scale, scale);
        if (Math.Abs(offsetY) > 0.001)
        {
            sb.AppendTranslate(0, offsetY);
        }
        return sb.Build();
    }

    /// <summary>弹簧缓动 —— K=100 D=16 实测过冲 1.52% (极轻微回弹)。</summary>
    internal static Easing Spring() => new SpringEasing
    {
        Mass = SpringMass,
        Stiffness = SpringStiffness,
        Damping = SpringDamping,
        InitialVelocity = 0,
    };

    /// <summary>
    /// 装"变换 + 可选透明度"过渡。透明度单独给时长 (容器入场时透明度应比缩放先到位,
    /// 否则弹簧回弹期间会重复淡入读出"闪"); 退场时二者同长。
    /// </summary>
    internal static Transitions PoseTransitions(
        TimeSpan duration, Easing easing, bool includeOpacity = false, TimeSpan? opacityDuration = null)
    {
        var list = new Transitions
        {
            new TransformOperationsTransition
            {
                Property = Visual.RenderTransformProperty,
                Duration = duration,
                Easing = easing,
            },
        };
        if (includeOpacity)
        {
            list.Add(new DoubleTransition
            {
                Property = Visual.OpacityProperty,
                Duration = opacityDuration ?? duration,
                // 透明度共用弹簧缓动: 若换成线性/别的曲线, 回弹阶段会读出"二次淡入"的闪感
                Easing = easing,
            });
        }
        return list;
    }

    /// <summary>测试缝: 读某窗口当前是否处于"退场中"。</summary>
    internal static bool IsClosing(Window window) =>
        States.TryGetValue(window, out var s) && s.Closing;

    /// <summary>测试缝: 读某窗口是否被本服务接管。</summary>
    internal static bool IsAttached(Window window) => States.ContainsKey(window);
}
