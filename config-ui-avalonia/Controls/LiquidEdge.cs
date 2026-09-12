using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Media.Transformation;
using Avalonia.Styling;
using Avalonia.Utilities;
using Avalonia.VisualTree;

namespace KeyFlux.Settings.Controls;

// 静态类不能作为泛型类型参数 (CS0718), 附加属性注册用 marker 类型占位
public sealed class LiquidEdgeMarker
{
}

/// <summary>作用边缘词表 (可组合)。</summary>
[Flags]
public enum LiquidEdges
{
    None = 0,
    Top = 1,
    Bottom = 2,
    Vertical = Top | Bottom,
    Left = 4,
    Right = 8,
    Horizontal = Left | Right,
    All = Vertical | Horizontal,
}

/// <summary>
/// LiquidEdge —— 滚动边缘「液态表面张力吸附」附加行为。
///
/// 挂在滚动容器内的卡片 (Border) 上, 卡片接近/接触/穿透宿主滚动视口边缘时呈现液态形变:
///   接近   : 向边缘平移 + 近侧圆角渐平 (被吸附的预兆);
///   接触   : 以被边缘咬住的一侧为原点纵向拉长, 近侧圆角归零形成弯月面,
///            头部随滚动逃逸而身体滞后 —— 粘滞拉丝;
///   断裂   : 穿透超过 SnapPenetration 后形状弹性回弹归位, 之后随滚动自然滑出;
///   回卷   : 反向滚动重新进入接触段时形变按滚动距离连续恢复 (完全可逆, 非一次性动画)。
///
/// 实现要点 (性能红线: 本软件为软件渲染, 禁用 shader/实时模糊/阴影动画):
///   - 事件驱动: 仅订阅宿主 ScrollChanged 与卡片 SizeChanged, 无定时器无渲染循环;
///   - 每次滚动事件只处理「吸引区内的卡片」, 属性带变更防抖 (值不变不写, 不触发无效重绘);
///   - 形变 = RenderTransform (TransformOperations, 固定操作序列保证相邻两帧可插值)
///     + CornerRadius 渐变, 二者走 Transitions 平滑插值 —— 连续滚动的"液态粘滞感"
///     由插值自然产生; 断裂回弹用一次 ElasticEaseOut 动画覆盖 (动画优先级高于过渡)。
///   - 虚拟化安全: 随容器 AttachedToVisualTree 挂载 / DetachedFromVisualTree 清理。
/// </summary>
public static class LiquidEdge
{
    // ------------------------------------------------------------ 可调参数 (默认值, 克制取向)

    /// <summary>吸引区半径: 卡片距边缘小于该值开始形变。</summary>
    public const double AttractDistance = 96;

    /// <summary>接近段向边缘平移的上限。</summary>
    public const double MaxPull = 8;

    /// <summary>接触段拉长量的像素上限 (防止长卡片过度重叠相邻内容)。</summary>
    public const double MaxStretchExtra = 26;

    /// <summary>拉长速率: 每穿透 1px 拉长多少 (产生粘滞滞后感)。</summary>
    public const double Resistance = 1.7;

    /// <summary>穿透超过该值视为「断裂」, 形状弹性回弹归位。</summary>
    public const double SnapPenetration = 52;

    /// <summary>近侧圆角压平比例 (接触时近侧圆角 = 基础值 × (1-该值))。</summary>
    public const double CornerFlatten = 0.9;

    /// <summary>断裂回弹时长 (弹性缓动)。</summary>
    public const int ReboundMs = 300;

    // ------------------------------------------------------------ 附加属性

    public static readonly AttachedProperty<bool> IsEnabledProperty =
        AvaloniaProperty.RegisterAttached<LiquidEdgeMarker, Border, bool>("IsEnabled");

    /// <summary>作用边缘 (默认上下; 水平滚动容器可设 Left|Right/All)。</summary>
    public static readonly AttachedProperty<LiquidEdges> EdgesProperty =
        AvaloniaProperty.RegisterAttached<LiquidEdgeMarker, Border, LiquidEdges>("Edges", LiquidEdges.Vertical);

    public static bool GetIsEnabled(Border border) => border.GetValue(IsEnabledProperty);
    public static void SetIsEnabled(Border border, bool value) => border.SetValue(IsEnabledProperty, value);

    public static LiquidEdges GetEdges(Border border) => border.GetValue(EdgesProperty);
    public static void SetEdges(Border border, LiquidEdges value) => border.SetValue(EdgesProperty, value);

    static LiquidEdge()
    {
        IsEnabledProperty.Changed.AddClassHandler<Border>((border, e) =>
        {
            if (e.NewValue is true)
            {
                Attach(border);
            }
            else
            {
                Detach(border);
            }
        });
    }

    // ------------------------------------------------------------ 每卡片状态

    private sealed class State
    {
        public Border Border = null!;
        public ScrollViewer? Host;
        public LiquidEdges Edges = LiquidEdges.Vertical;
        public CornerRadius BaseRadius;        // 从样式捕获的基础圆角 (形变基线)
        public bool BaseCaptured;
        public string LastOps = "";            // 变更防抖
        public CornerRadius LastRadius;
        public bool RadiusInitialized;
        public bool NearTop = true;            // 当前形变的被咬侧在卡片上方?
        public bool TopContact;                // 顶缘接触段进行中 (0 < 穿透 < Snap)
        public bool BottomContact;
        public bool Rebounding;                // 回弹动画进行中 (连续映射让位)
        public CancellationTokenSource? ReboundCts;
    }

    private static readonly Dictionary<Border, State> States = new();

    // ------------------------------------------------------------ 挂载 / 清理

    private static void Attach(Border border)
    {
        if (States.ContainsKey(border))
        {
            return;
        }
        var st = new State { Border = border, Edges = border.GetValue(EdgesProperty) };
        States[border] = st;
        border.AttachedToVisualTree += OnAttachedToTree;
        border.DetachedFromVisualTree += OnDetachedFromTree;
        if (border.IsAttachedToVisualTree())
        {
            HookHost(st);
        }
    }

    private static void Detach(Border border)
    {
        if (!States.TryGetValue(border, out var st))
        {
            return;
        }
        st.ReboundCts?.Cancel();
        border.AttachedToVisualTree -= OnAttachedToTree;
        border.DetachedFromVisualTree -= OnDetachedFromTree;
        UnhookHost(st);
        States.Remove(border);
    }

    private static void OnAttachedToTree(object? sender, VisualTreeAttachmentEventArgs e)
    {
        if (sender is Border b && States.TryGetValue(b, out var st))
        {
            HookHost(st);
        }
    }

    private static void OnDetachedFromTree(object? sender, VisualTreeAttachmentEventArgs e)
    {
        if (sender is Border b && States.TryGetValue(b, out var st))
        {
            UnhookHost(st);
        }
    }

    /// <summary>沿可视树向上找到包裹的 ScrollViewer 作为宿主 (找不到则行为静默休眠)。</summary>
    private static void HookHost(State st)
    {
        if (st.Host is not null)
        {
            return;
        }
        foreach (var ancestor in st.Border.GetVisualAncestors())
        {
            if (ancestor is ScrollViewer sv)
            {
                st.Host = sv;
                break;
            }
        }
        if (st.Host is null)
        {
            return;
        }
        st.Host.ScrollChanged += OnHostChanged;
        st.Border.SizeChanged += OnBorderSizeChanged;
        Update(st);
    }

    private static void UnhookHost(State st)
    {
        if (st.Host is not null)
        {
            st.Host.ScrollChanged -= OnHostChanged;
        }
        st.Border.SizeChanged -= OnBorderSizeChanged;
        st.Host = null;
    }

    private static void OnHostChanged(object? sender, ScrollChangedEventArgs e)
    {
        if (sender is ScrollViewer sv)
        {
            UpdateByHost(sv);
        }
    }

    private static void OnBorderSizeChanged(object? sender, SizeChangedEventArgs e)
    {
        if (sender is Border b && States.TryGetValue(b, out var st))
        {
            Update(st);
        }
    }

    private static void UpdateByHost(ScrollViewer sv)
    {
        foreach (var pair in States)
        {
            if (ReferenceEquals(pair.Value.Host, sv))
            {
                Update(pair.Value);
            }
        }
    }

    // ------------------------------------------------------------ 形变核心

    private const string IdentityOps = "translate(0px, 0px) scale(1, 1)";

    private static void Update(State st)
    {
        if (st.Rebounding || st.Host is null)
        {
            return; // 回弹动画拥有变换, 连续映射让位
        }
        if (!st.BaseCaptured)
        {
            st.BaseRadius = st.Border.CornerRadius; // 样式值 (已应用), 作为形变基线
            st.LastRadius = st.BaseRadius;
            st.BaseCaptured = true;
        }

        var viewportH = st.Host.Bounds.Height;
        if (!st.Border.IsVisible || viewportH <= 0 || st.Border.Bounds.Height >= viewportH)
        {
            // 卡片比视口还高 (展开的长段) —— 不做形变, 保证内容可读
            ResetToBase(st);
            return;
        }

        if (st.Border.TranslatePoint(new Point(0, 0), st.Host) is not { } cardTop)
        {
            return;
        }
        var h = st.Border.Bounds.Height;

        // 计算上下两缘的严重度, 取主导者 (h < viewport 时卡片不会同时被两缘咬住)
        double sevTop = 0, sevBottom = 0;
        if ((st.Edges & LiquidEdges.Top) != 0)
        {
            var p = -cardTop.Y;                                     // 顶缘穿透 (卡顶越过视口上缘为正)
            sevTop = p > 0 ? p : Math.Max(0, AttractDistance - p);
        }
        if ((st.Edges & LiquidEdges.Bottom) != 0)
        {
            var dBottom = viewportH - (cardTop.Y + h);
            var p = -dBottom;                                       // 底缘穿透 (卡底越过视口下缘为正)
            sevBottom = p > 0 ? p : Math.Max(0, AttractDistance - dBottom);
        }

        if (sevTop >= sevBottom && sevTop > 0)
        {
            ApplyEdge(st, isTopEdge: true, sevTop, h, viewportH);
        }
        else if (sevBottom > 0)
        {
            ApplyEdge(st, isTopEdge: false, sevBottom, h, viewportH);
        }
        else
        {
            ResetToBase(st);
        }
    }

    /// <summary>
    /// 单缘形变映射: severity ≤ AttractDistance = 接近段 (平移+近角渐平);
    /// severity &gt; AttractDistance = 接触段 (原点钉在被咬侧, 纵向拉长 + 横向轻微收细 = 拉丝);
    /// 穿透 ≥ SnapPenetration = 断裂 (弹性回弹归位, 之后自然滑出, 回卷时重新进入接触段)。
    /// </summary>
    private static void ApplyEdge(State st, bool isTopEdge, double severity, double h, double viewportH)
    {
        // 被咬侧: 顶缘交互 = 卡片上方两角; 底缘交互 = 卡片下方两角
        st.NearTop = isTopEdge;

        if (severity <= AttractDistance)
        {
            // ---------- 接近段: 距离越近平移越多 + 近侧圆角渐平 (可逆, 过渡插值平滑) ----------
            var d = AttractDistance - severity;                      // 到边缘的剩余距离
            var t = 1 - Math.Clamp(d / AttractDistance, 0, 1);
            var pull = -t * MaxPull * (isTopEdge ? 1 : -1);          // 上缘向上吸 / 下缘向下吸
            Apply(st, $"translate(0px, {pull:0.##}px) scale(1, 1)", isTopEdge, 1 - 0.5 * t, 1);
            st.TopContact = false;
            st.BottomContact = false;
            return;
        }

        // ---------- 接触段 / 断裂 ----------
        var p = severity - AttractDistance;                          // 穿透量 (卡缘越过视口缘)
        if (p >= SnapPenetration)
        {
            // 断裂: 弹性回弹归位, 之后自然滑出; 回卷 (穿透重新 < Snap) 时恢复接触段
            var engaged = isTopEdge ? st.TopContact : st.BottomContact;
            if (engaged && !st.Rebounding)
            {
                st.Border.CornerRadius = st.BaseRadius;
                PlayRebound(st);
            }
            ResetToBase(st);
            if (isTopEdge)
            {
                st.TopContact = false;
            }
            else
            {
                st.BottomContact = false;
            }
            return;
        }

        // 拉长量按像素封顶 (长卡片不过度覆盖相邻内容), 横向轻微收细 = 颈部
        var extra = Math.Min(Resistance * p, MaxStretchExtra);
        var k = h > 0 ? extra / h : 0;
        var sy = 1 + k;
        var sx = 1 - 0.22 * k;
        Apply(st, $"translate(0px, 0px) scale({sx:0.####}, {sy:0.####})", isTopEdge, 1 - CornerFlatten, 1);
        if (isTopEdge)
        {
            st.TopContact = true;
            st.BottomContact = false;
        }
        else
        {
            st.BottomContact = true;
            st.TopContact = false;
        }
    }

    private static void Apply(State st, string ops, bool nearTop, double nearFactor, double farFactor)
    {
        if (st.LastOps == ops && st.LastRadius == ComposeRadius(st, nearFactor, farFactor))
        {
            return; // 变更防抖: 与上次完全一致则不写, 避免无效重绘
        }
        st.LastOps = ops;
        st.NearTop = nearTop;
        st.Border.RenderTransformOrigin = nearTop
            ? new RelativePoint(0.5, 0.0, RelativeUnit.Relative)
            : new RelativePoint(0.5, 1.0, RelativeUnit.Relative);
        st.Border.RenderTransform = TransformOperations.Parse(ops);
        st.LastRadius = ComposeRadius(st, nearFactor, farFactor);
        st.Border.CornerRadius = st.LastRadius;
    }

    private static CornerRadius ComposeRadius(State st, double nearFactor, double farFactor)
    {
        var r = st.BaseRadius;
        if (st.NearTop)
        {
            return new CornerRadius(r.TopLeft * nearFactor, r.TopRight * nearFactor, r.BottomRight * farFactor, r.BottomLeft * farFactor);
        }
        return new CornerRadius(r.TopLeft * farFactor, r.TopRight * farFactor, r.BottomRight * nearFactor, r.BottomLeft * nearFactor);
    }

    private static void ResetToBase(State st)
    {
        if (st.LastOps == IdentityOps && st.LastRadius == st.BaseRadius)
        {
            return; // 已归位
        }
        st.LastOps = IdentityOps;
        st.LastRadius = st.BaseRadius;
        st.Border.RenderTransformOrigin = new RelativePoint(0.5, 0.5, RelativeUnit.Relative);
        st.Border.RenderTransform = TransformOperations.Parse(IdentityOps);
        st.Border.CornerRadius = st.BaseRadius;
    }

    private static void PlayRebound(State st)
    {
        st.ReboundCts?.Cancel();
        st.ReboundCts = new CancellationTokenSource();
        var token = st.ReboundCts.Token;
        var animation = new Animation
        {
            Duration = TimeSpan.FromMilliseconds(ReboundMs),
            Easing = new ElasticEaseOut(),
            Children =
            {
                new KeyFrame
                {
                    Cue = new Cue(0.0),
                    Setters = { new Setter { Property = Visual.RenderTransformProperty, Value = TransformOperations.Parse(st.LastOps) } },
                },
                new KeyFrame
                {
                    Cue = new Cue(1.0),
                    Setters = { new Setter { Property = Visual.RenderTransformProperty, Value = TransformOperations.Parse(IdentityOps) } },
                },
            },
        };
        _ = RunReboundAsync(st, animation, token);
    }

    private static async Task RunReboundAsync(State st, Animation animation, CancellationToken token)
    {
        try
        {
            await animation.RunAsync(st.Border, token);
        }
        catch (OperationCanceledException)
        {
        }
        finally
        {
            if (!token.IsCancellationRequested)
            {
                st.Rebounding = false;
            }
        }
    }
}
