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

    /// <summary>
    /// 「先量后显」预留余量 —— 入场姿势的下浮量与玻璃阴影都在布局尺寸之外, 预留高度必须
    /// 覆盖二者, 否则弹窗底边会切到按钮。取值与 <see cref="EnterOffsetY"/> 同量级并略大。
    /// 详见 <see cref="ReserveHeightBeforeShow"/> 的三道几何推理。
    /// </summary>
    internal const double HeightReserveMargin = 20;

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

        /// <summary>
        /// 放行标志 —— 退场跑完后由延时回调置 true, 表示"这次 Closing 是最后一次, 直接放行"。
        ///
        /// <para><b>⚠ 为什么必须与 <see cref="Closing"/> 分开 (2026-09-24 用户报障"关闭按钮要按两次")。</b>
        /// 早期实现用 <c>state.Closing = false</c> 表示放行, 但 <see cref="OnClosing"/> 的守卫是
        /// <c>if (state.Closing) return;</c> —— <b>false 恰好不满足该守卫</b>, 于是放行触发的
        /// 第二次 <c>Close()</c> 重入 <c>OnClosing</c> 时又走了完整拦截流程 (再播一次退场 +
        /// 注册新计时器), 窗口依然不关; 直到用户第二次点关闭按钮, 此时 <c>Closing</c> 为 true
        /// 才被守卫直接放行。用户感知即"要按两次"。语义上"正在退场"与"该放行了"本就是两件事,
        /// 必须用两个标志表达。</para>
        /// </summary>
        public bool Forced;

        /// <summary>页内浮层的宿主 (挂附加属性的透明入口层; 仅作钩子与主体查找起点)。</summary>
        public Control? OverlayHost;

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
            if (!GetOverlayMotion(v) || v is not Control host)
            {
                return;
            }
            // 宿主只是钩子 (附加属性挂在稳定声明的透明入口上); 动效主体在 PlayEnter 时解析
            var state = GetOrCreateOverlayState(v, host);
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
    ///
    /// <para><b>⚠ 为什么起始姿势在这里落、而不是在 <c>Opened</c> 里落 (2026-09-24 用户报障
    /// "打开弹窗时文字会闪一下")。</b>实测 (headless 探针 + 反编译双重确认):
    /// <c>Window.ShowCore</c> 先把 <c>IsVisible = true</c>, 之后才由 <c>IsVisibleChanged</c> 触发
    /// <c>Opened</c> —— 即 <b>Opened 触发时窗口已经以"不透明、原尺寸"渲染过至少一帧</b>
    /// (探针读回: <c>IsVisible -> True</c> 先于 <c>Opened</c>, 且 Opened 进入时
    /// <c>body.Opacity == 1</c>)。若在 Opened 里才设 <c>Opacity = 0</c>, 那一帧的完整内容已被
    /// 用户看见 ⇒ 观感即"闪一下"。探针同时确认: <b>Show 之前设 Opacity=0, Opened 时读回仍是 0</b>
    /// ⇒ 起始姿势必须在 Show 之前落好。</para>
    ///
    /// <para>时机可行性: <see cref="Win32.DialogChrome.Apply"/> 在弹窗构造期、紧跟
    /// <c>InitializeComponent()</c> 之后调用, 此刻 <c>window.Content</c> 已由 XAML 就位
    /// (探针实测: Content 非空、IsInitialized=true、VisualRoot=null)。VisualRoot 为 null 意味着
    /// 此时设的只是本地值、不参与渲染 —— 零副作用。</para>
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

        // 构造期预落起始姿势: 必须在窗口首次可见【之前】就位, 否则首帧以不透明渲染 ⇒ 闪一下。
        // 此时 Content 已就位但未挂视觉树, 设的是纯本地值。
        PoseBeforeShow(window, state);

        window.Opened += (_, _) => OnOpened(window, state);
        window.Closing += (_, e) => OnClosing(window, state, e);
        window.Closed += (_, _) => OnClosed(window, state);
    }

    /// <summary>
    /// 构造期预落起始姿势 (略小 + 略下 + 透明)。若此刻 <c>Content</c> 还没就位 (少数弹窗在
    /// 构造后才设 Content), 则留到 <see cref="OnOpened"/> 兜底 —— 那种情况仍会闪一下,
    /// 但属于"内容本就晚于窗口出现", 不是本动效引入的问题。
    /// </summary>
    private static void PoseBeforeShow(Window window, State state)
    {
        if (window.Content is not Control content)
        {
            return;
        }
        var body = EnsureGlassShell(content);
        state.Body = body;
        body.Classes.Add(MotionClass);
        body.Opacity = 0;
        body.RenderTransform = Pose(EnterScale, EnterOffsetY);
    }

    /// <summary>
    /// <b>在首次显示之前把窗口尺寸"锚定"住</b> —— 消除「首帧按 <c>MaxHeight</c> 上屏、
    /// 随后塌缩到真实高度」造成的黑块 (2026-09-24 用户报障, 最终方案)。
    ///
    /// <para><b>⚠ 踩过的四条路 (全部真机实测, 勿重蹈)。</b><br/>
    /// ① <b>在数据就绪前手动 Measure</b> —— 量到"标题 + 空态"的残缺值, 设死后把表单裁掉
    ///    (用户报障"显示不完整")。<br/>
    /// ② <b>数据就绪后手动 Measure</b> —— 仍量不准: 真机探针实测 <b>窗口首次 Show 之前,
    ///    <c>ScrollViewer</c> 的 <c>IsVisible</c> 虽已推送为 True, 但其内容 <c>ItemsControl</c>
    ///    从未进入视觉树</b> (<c>IC=无</c>、<c>TextBox=0</c>), 无论 <c>Rows</c> 有几行, 量出的
    ///    内容高恒为 <c>~107~153</c>。这是 Avalonia 窗口管线固有的 (项容器要等首个渲染周期),
    ///    不是绑定问题 —— 显式 <c>ApplyTemplate</c>/<c>Arrange</c> 都无法绕过。<br/>
    /// ③ <b>窗口级 Opacity=0 等几跳再恢复</b> —— 全透明期间合成器不绘制内容, 恢复时布局未排空
    ///    ⇒ <b>620 高的持续黑块</b>, 比原先"一闪"更严重。<br/>
    /// ④ <b>预设 Height = 量出的高度</b> —— 因 ② 量不准, 等于把窗口钉死在残缺高度。</para>
    ///
    /// <para><b>✅ 现行做法: 不动高度, 只把 <c>SizeToContent</c> 临时摘掉。</b>
    /// 黑块的成因是"<b>首帧尺寸与后续尺寸不同</b>"——<c>SizeToContent</c> 让窗口以
    /// <c>MaxHeight</c> 建立表面, 布局落定后再 <c>SetWindowPos</c> 缩小。既然量不准真实高度,
    /// 那就<b>让第一帧就已经是最终尺寸</b>: 令 <c>Height = MaxHeight</c> 且
    /// <c>SizeToContent = Manual</c>, 窗口首帧就是 620 —— 布局落定前后尺寸完全一致,
    /// <b>不存在"新暴露区域"</b>, 黑块随之消失。<c>DialogPlacer</c> 以 620 居中, 不会偏下。</para>
    ///
    /// <para><b>入场结束后的收尾 (为什么保留 SizeToContent)。</b>620 对短表单偏大 (底部留白)。
    /// 入场动画跑完后由 <see cref="ReleaseReservedHeight"/> 把 <c>SizeToContent</c> 还给
    /// <c>Height</c>, 窗口一次性收到真实高度。此时<b>内容已绘制完成</b> (只是窗口变大变小,
    /// 不涉及未绘制区域), 且用户正处于"弹窗已就位"的认知中, 收缩几乎察觉不到。</para>
    ///
    /// <para><b>与"透明期落定"的区别。</b>那里是"先给旧尺寸再换新尺寸 + 用透明掩盖";
    /// 这里是"首帧直接给最终尺寸, 全程不透明"。<b>不透明是关键</b> —— 内容从第一帧起就被
    /// 正常绘制, 不存在"透明期不绘制、恢复时来不及"的窗口。</para>
    /// </summary>
    /// <returns>是否成功锚定尺寸 (供测试与日志判断)。</returns>
    public static bool ReserveHeightBeforeShow(Window window)
    {
        if (States.TryGetValue(window, out var state) && state.Reduced)
        {
            return false; // 减少动效: 无位移姿势, 无需锚定
        }
        if (double.IsNaN(window.MaxHeight) || window.MaxHeight <= 0)
        {
            return false; // 无 MaxHeight 约束的窗口交给 SizeToContent 自行工作
        }

        // 首帧尺寸 = 最终尺寸 = MaxHeight。布局落定后 SizeToContent 不会再改它 ⇒ 无跳变。
        window.Height = window.MaxHeight;
        window.SizeToContent = SizeToContent.Manual;
        return true;
    }

    private static void OnOpened(Window window, State state)
    {
        var content = window.Content as Control;
        if (content is null)
        {
            return;
        }

        // 兜底: 构造期 Content 未就位时在这里补落姿势 (正常路径已在 PoseBeforeShow 落好)。
        //
        // ⚠ 正常路径下【不要】在这里重复设 Opacity/RenderTransform (2026-09-24 用户报障
        // "弹窗先显示了文字, 然后闪一下再出现")。构造期 PoseBeforeShow 已经把姿势落成
        // Opacity=0, 窗口首帧就是透明的; 若这里再赋一遍同样的值, 虽然值相同不会产生过渡,
        // 但会给将来任何"让 Opacity 在 Opened 之前被别处改回 1"的改动留下伪装 ——
        // 一旦真发生, 用户看到的就是"先可见 → 被压回 0 → 再淡入"的三段闪烁。
        // 故只在姿势确实没落上时才补。
        var body = state.Body ?? EnsureGlassShell(content);
        state.Body = body;
        if (!body.Classes.Contains(MotionClass))
        {
            body.Classes.Add(MotionClass);
            body.Opacity = 0;
            body.RenderTransform = Pose(EnterScale, EnterOffsetY);
        }

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

        // 入场结束 (弹簧收束) 后收掉预留高度, 消除内容与窗口底边之间的空档。
        // 用一次性 DispatcherTimer 而非叠加在上一跳: 上一跳只隔一帧, 弹簧还没跑完。
        var settle = new DispatcherTimer { Interval = ClaudeMotion.DialogEnter + TimeSpan.FromMilliseconds(60) };
        settle.Tick += (s, _) =>
        {
            settle.Stop();
            if (!state.Closing)
            {
                ReleaseReservedHeight(window);
            }
        };
        settle.Start();
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
        // 放行通道: 退场已跑完, 这次是真关 —— 必须放在 `Closing` 守卫之前,
        // 否则又会被完整拦截一遍 (见 State.Forced 注释里的"要按两次"根因)
        if (state.Forced)
        {
            state.CloseCts?.Cancel();
            state.CloseCts = null;
            return;
        }

        if (state.Closing)
        {
            // 退场中的重复 Close(): 拦下即可, 但不重复编排 (幂等)
            e.Cancel = true;
            return;
        }
        state.Closing = true;

        // 退场期间禁点, 防重复触发 (用户连点关闭按钮)
        window.IsHitTestVisible = false;

        var body = state.Body;
        if (body is not null)
        {
            // ⚠ includeOpacity: true 不可省 (2026-09-24 用户报障"内容先消失, 窗口再关闭")。
            // 早期传默认 false ⇒ 过渡只含 RenderTransform, `Opacity = 0` 于是**瞬时生效**:
            // 内容"啪"地消失, 而缩放还在跑 200ms, 再加 40ms pad 才关窗 ⇒ 用户看到一段
            // 空窗口停留 (即"滞后一下")。透明度必须与缩放同一段过渡一起走完。
            body.Transitions = PoseTransitions(ClaudeMotion.DialogExit, new CubicEaseIn(), includeOpacity: true);
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
            if (cts.IsCancellationRequested || state.Forced)
            {
                return;
            }
            if (window.IsVisible)
            {
                state.Forced = true; // 放行下次 Closing (与 Closing 分开, 见 State.Forced 注释)
                window.Close();
            }
        }, ClaudeMotion.DialogExit + TimeSpan.FromMilliseconds(40));
    }

    /// <summary>
    /// 入场动效跑完后把窗口收回到内容的真实高度。
    ///
    /// <para><b>⚠ 必须把 <c>Height</c> 清回 <c>NaN</c>, 只改 <c>SizeToContent</c> 无效。</b>
    /// 锚定时设了 <c>Height = MaxHeight</c> (本地值优先级最高), <c>SizeToContent</c> 会被它压住 ——
    /// 实测只切 <c>SizeToContent</c> 窗口仍停在 620。清掉本地值后 <c>SizeToContent</c> 才接管。</para>
    ///
    /// <para><b>为什么可以安全收缩。</b>此时入场动画已结束、内容<b>已完整绘制</b>, 窗口从 620
    /// 收到真实高度只是"变大变小", 不产生未绘制区域 (黑块的成因)。推迟到动画结束也是为了
    /// 让动画全程尺寸稳定 —— 动画中途改高会让入场姿势与新尺寸不同步。</para>
    ///
    /// <para><b>阴影余量。</b>根 Border 的 <c>BoxShadow</c> 渲染在布局尺寸之外, 窗口贴合内容
    /// 高度时下沿会吃掉一圈阴影。<see cref="HeightReserveMargin"/> 作为补偿一并计入。</para>
    /// </summary>
    private static void ReleaseReservedHeight(Window window)
    {
        if (window.SizeToContent != SizeToContent.Manual)
        {
            return;
        }
        window.Height = double.NaN; // 清本地值, 把尺寸决定权交还 SizeToContent
        window.SizeToContent = SizeToContent.Height;
    }

    private static void OnClosed(Window window, State state)
    {
        state.CloseCts?.Cancel();
        state.CloseCts = null;
        state.Closing = false;
        state.Forced = false;
        // 遮罩退场已在 OnClosing 编排 (此处窗口已不可见, 再触发会重复计数)
        States.Remove(window);
    }

    // ---------------------------------------------------------------- 页内浮层

    // 页内浮层状态: 弱持有 —— 浮层元素随页面销毁, 强引用字典会永久持有 (泄漏)
    private static readonly ConditionalWeakTable<Visual, State> OverlayStates = new();

    private static State GetOrCreateOverlayState(Visual v, Control host)
    {
        var s = OverlayStates.GetOrCreateValue(v);
        s.Reduced = !MotionPreferences.AnimationsEnabled;
        s.OverlayHost = host;
        return s;
    }

    /// <summary>
    /// 页内浮层「动效主体类名」—— 标在浮层里<b>真正可见</b>的那层容器上。
    /// </summary>
    public const string PanelClass = "dlgPanel";

    /// <summary>
    /// 把动效主体从"挂附加属性的宿主"下沉到<b>真正可见的浮层容器</b>。
    ///
    /// <para><b>⚠ 为什么必须下沉 (2026-09-24 用户报障"单击卡片直接弹出、没有动画")。</b>
    /// 页内浮层在 XAML 里是三明治结构: 最外层是<b>透明的输入拦截 Border</b> (Background=Transparent,
    /// 占满整页、负责点外部关闭), 附加属性 <c>IsOpen</c>/<c>OverlayMotion</c> 挂在它身上;
    /// 真正可见的弹层是内层 <c>ContentControl</c> 的数据模板根 (带背色/圆角/阴影的 Border)。
    /// 早期实现直接拿宿主当动效主体, 于是弹簧缩放/淡入全作用在一个<b>完全透明的容器</b>上 ——
    /// 动效确实在跑 (探针实测: 宿主 Opacity/Transform/Transitions 均已被改写), 但用户
    /// <b>视觉上什么都看不到</b>, 感知就是"直接弹出、没有动画"。</para>
    ///
    /// <para>解法定为"服务自己向下找主体"而不是"把附加属性挂到内层" —— 内层模板由
    /// 数据对象 (<c>AddMappingVm</c>) 驱动、随类型切换重建, 附加属性挂不稳; 而宿主是 XAML 里
    /// 静态声明的, 天然稳定。故保留宿主的钩子职责, 主体改为向下查找: 优先找标了
    /// <see cref="PanelClass"/> 的后代, 找不到则回退到"第一个可见且有渲染内容的子级"
    /// (兜底: 至少不是透明容器), 再不行才退回宿主自身。</para>
    /// </summary>
    private static Control ResolveOverlayBody(Control host)
    {
        foreach (var c in host.GetVisualDescendants().OfType<Control>())
        {
            if (c.Classes.Contains(PanelClass))
            {
                return c;
            }
        }
        // 兜底: 第一个"自身有视觉呈现"的后代 (排除透明/无子级的中间层)
        foreach (var c in host.GetVisualDescendants().OfType<Control>())
        {
            if (c is Border { Child: not null } b && b.Background is not null)
            {
                return c;
            }
        }
        return host;
    }

    private static void PlayEnter(State state)
    {
        if (state.Reduced || state.OverlayHost is null)
        {
            return;
        }
        var host = state.OverlayHost;

        // ⚠ 主体解析必须推迟到 dispatcher 回调里 (不能在此刻同步解析): IsOpen 回调触发时
        // <c>AddPanel</c> 刚被赋值, ContentControl 的数据模板还没展开 —— 此刻
        // GetVisualDescendants() 拿不到内层 Border (探针实测: 同步期后代为 0, 布局后才有)。
        // 故整段「解析主体 → 落起始姿势 → 推终值」都在提交后才做。
        Dispatcher.UIThread.Post(() =>
        {
            if (state.Reduced)
            {
                return;
            }
            var body = ResolveOverlayBody(host);
            state.Body = body;

            // 起始姿势 (先落起点, 再在下一跳推终值 —— 见下方注释)
            body.Classes.Add(MotionClass);
            body.Opacity = 0;
            body.RenderTransform = Pose(EnterScale, EnterOffsetY);

            // 起始姿势必须"先被渲染一帧"再推终值, 否则过渡没有起点可言 —— 页内浮层尤其敏感:
            // 宿主 IsVisible 由绑定在同批属性变更里翻 true, 若终值也落在同一批, 起点姿势被合并掉,
            // 观感就是"直接弹出、没有动画" (2026-09-24 用户报障)。
            Dispatcher.UIThread.Post(() =>
            {
                if (state.Reduced || !ReferenceEquals(state.Body, body))
                {
                    return;
                }
                body.Transitions = PoseTransitions(ClaudeMotion.DialogEnter, Spring());
                body.Opacity = 1;
                body.RenderTransform = Pose(1, 0);
            }, DispatcherPriority.Background);
        }, DispatcherPriority.Render);
    }

    private static void PlayOverlayExit(State state)
    {
        if (state.Reduced)
        {
            return;
        }
        var body = state.Body;
        if (body is null)
        {
            return;
        }
        body.Transitions = PoseTransitions(ClaudeMotion.DialogExit, new CubicEaseIn(), includeOpacity: true);
        body.Opacity = 0;
        body.RenderTransform = Pose(ExitScale, 0);
        // 内容根由绑定控制 IsVisible, 退场后由绑定侧收起; 这里只负责过渡姿势
        // (includeOpacity 同样不可省, 理由见 OnClosing 的同名注释)
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

    /// <summary>
    /// 测试缝: 模拟"退场延时到期" —— 把该窗口推进到放行状态并真正 <c>Close()</c>。
    ///
    /// <para>存在的理由: 退场由 <see cref="DispatcherTimer"/> 驱动, 而 headless 宿主不推进
    /// 真实时钟 (<c>ForceRenderTimerTick</c> 只走渲染计时器, 不驱动 dispatcher 计时器) ——
    /// 探针实测 <c>Closing</c> 全程只触发 1 次, 延时回调永不执行。要让"关一次就真关"这条
    /// 契约可测, 必须有办法绕过计时器直接走到回调体的等价位置。</para>
    /// </summary>
    internal static void ForceExitDelayElapsed(Window window)
    {
        if (!States.TryGetValue(window, out var s))
        {
            return;
        }
        if (s.CloseCts?.IsCancellationRequested != false || s.Forced)
        {
            return;
        }
        if (window.IsVisible)
        {
            s.Forced = true;
            window.Close();
        }
    }

    /// <summary>测试缝: 读窗口是否已被放行 (退场延时已到期)。</summary>
    internal static bool IsForced(Window window) =>
        States.TryGetValue(window, out var s) && s.Forced;

    /// <summary>测试缝: 读某窗口是否被本服务接管。</summary>
    internal static bool IsAttached(Window window) => States.ContainsKey(window);
}
