using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.VisualTree;

namespace KeyFlux.Settings.Controls;

/// <summary>附加属性注册用 marker (静态类不能作泛型类型参数)。</summary>
public sealed class ModalBlurMarker
{
}

/// <summary>
/// ModalBlur —— 弹窗背景模糊: 弹窗/面板打开时, 对其背后的内容区施加高斯式模糊
/// (替代颜色遮罩; 侧栏不在宿主范围内, 天然保持清晰)。
///
/// 三种用法:
/// · IsHost="True" (主窗内容区): 附加到可视树时以弱引用注册到所属 TopLevel 名下;
///   弹窗服务 (DialogChrome) 在弹窗 Opened/Closed 时调 SetActive(owner, …) 统一开关,
///   新增弹窗自动覆盖, 主窗零 code-behind。
/// · IsActive (页内浮层): 直接绑定浮层开关 (如 选中动作页 IsAddPanelOpen)。
/// · Radius: 模糊半径 (默认 14)。
///
/// 性能 (软件渲染决策): 遮罩拦截输入 → 背景静止 → 模糊只在打开瞬间计算一次
/// (毫秒级), 无逐帧成本; 内存为打开期间数 MB 临时表面, 关闭即释放; 磁盘零新增。
/// 渲染走 Avalonia.Media.BlurEffect (Skia), 无 Win32 依赖, 可移植。
/// </summary>
public static class ModalBlur
{
    /// <summary>模糊半径缺省值。</summary>
    private const double DefaultRadius = 14;

    /// <summary>宿主登记: TopLevel (通常为主窗) → 宿主元素 + 弹窗开启计数 (叠窗时计数>0 才模糊)。</summary>
    private sealed class HostState
    {
        public Visual? Host;
        public int Count;
    }

    private static readonly Dictionary<TopLevel, HostState> Hosts = new();

    public static readonly AttachedProperty<bool> IsHostProperty =
        AvaloniaProperty.RegisterAttached<ModalBlurMarker, Visual, bool>("IsHost");

    public static readonly AttachedProperty<bool> IsActiveProperty =
        AvaloniaProperty.RegisterAttached<ModalBlurMarker, Visual, bool>("IsActive");

    public static readonly AttachedProperty<double> RadiusProperty =
        AvaloniaProperty.RegisterAttached<ModalBlurMarker, Visual, double>("Radius", DefaultRadius);

    public static bool GetIsHost(Visual v) => v.GetValue(IsHostProperty);
    public static void SetIsHost(Visual v, bool value) => v.SetValue(IsHostProperty, value);

    public static bool GetIsActive(Visual v) => v.GetValue(IsActiveProperty);
    public static void SetIsActive(Visual v, bool value) => v.SetValue(IsActiveProperty, value);

    public static double GetRadius(Visual v) => v.GetValue(RadiusProperty);
    public static void SetRadius(Visual v, double value) => v.SetValue(RadiusProperty, value);

    static ModalBlur()
    {
        IsHostProperty.Changed.AddClassHandler<Visual>((v, e) =>
        {
            if (e.NewValue is true)
            {
                // XAML 赋值发生在挂树前: 挂树事件补登记; 已在树内 (代码后置赋值) 则立即登记
                v.AttachedToVisualTree += OnHostAttached;
                v.DetachedFromVisualTree += OnHostDetached;
                if (v.GetVisualRoot() is TopLevel tl)
                {
                    Register(tl, v);
                }
            }
            else
            {
                v.AttachedToVisualTree -= OnHostAttached;
                v.DetachedFromVisualTree -= OnHostDetached;
                if (v.GetVisualRoot() is TopLevel tl)
                {
                    Unregister(tl);
                }
            }
        });

        IsActiveProperty.Changed.AddClassHandler<Visual>((v, e) => Apply(v, e.NewValue is true));

        RadiusProperty.Changed.AddClassHandler<Visual>((v, _) =>
        {
            // 半径变更即时反映到当前生效的模糊上
            Apply(v, v.GetValue(IsActiveProperty));
        });
    }

    /// <summary>弹窗服务入口: owner 窗口背后的宿主模糊 +1/-1 (叠窗计数, 归零才撤销)。</summary>
    public static void SetActive(TopLevel owner, bool active)
    {
        if (!Hosts.TryGetValue(owner, out var state))
        {
            return;
        }
        state.Count = Math.Max(0, state.Count + (active ? 1 : -1));
        if (state.Host is not null)
        {
            Apply(state.Host, state.Count > 0);
        }
    }

    private static void OnHostAttached(object? sender, Avalonia.VisualTreeAttachmentEventArgs e)
    {
        if (sender is Visual v && v.GetVisualRoot() is TopLevel tl)
        {
            Register(tl, v);
        }
    }

    private static void OnHostDetached(object? sender, Avalonia.VisualTreeAttachmentEventArgs e)
    {
        if (sender is Visual v && v.GetVisualRoot() is TopLevel tl)
        {
            Unregister(tl);
        }
    }

    private static void Register(TopLevel tl, Visual host)
    {
        Hosts[tl] = new HostState { Host = host };
    }

    private static void Unregister(TopLevel tl)
    {
        Hosts.Remove(tl);
    }

    private static void Apply(Visual v, bool active)
    {
        v.Effect = active ? new BlurEffect { Radius = v.GetValue(RadiusProperty) } : null;
    }
}
