using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Threading;

namespace KeyFlux.Settings.Services.Win32;

// ============================================================================
// DialogPlacer —— 插件弹窗统一定位 (2026-09-23)。
//
// 背景: 插件相关弹窗均为 SizeToContent="Height" + WindowStartupLocation="CenterOwner"。
// Avalonia 在打开瞬间按「内容尚未加载的小尺寸」居中; 表单/列表异步加载后窗口向下长高、
// 锚点不动 ⇒ 弹窗严重偏下 (Everything 搜索设置弹窗实测: 顶部 267/716, 底部贴窗口沿)。
//
// 统一策略 (参数集中管理, 各弹窗零配置):
//   1) 相对宿主窗口 (Owner) 水平垂直居中; 无宿主退化为屏幕工作区居中;
//   2) 高度超出宿主时贴顶 (仅留 MinTopGap 边距), 不再往下顶;
//   3) AttachAutoCenter 订阅 SizeChanged —— 异步内容加载/语言切换导致的尺寸变化
//      都会重新居中; 用户拖动窗口不改尺寸, 不会被拉回; 窗口 Closed 自动解绑。
//
// 新增插件弹窗的使用方式 (无需重复配置):
//   构造函数里 AttachAutoCenter(this); 内容异步加载完成后可再 CenterToOwner(this) 兜底。
// ============================================================================

internal static class DialogPlacer
{
    /// <summary>高度超出宿主时贴顶保留的最小顶边距 (DIP)。集中参数, 便于全局统一调整。</summary>
    public const double MinTopGap = 8;

    /// <summary>屏幕外开门的锚点坐标 (int 安全范围内远离一切显示器)。</summary>
    public const int OffscreenCoord = -20000;

    /// <summary>
    /// 相对宿主窗口居中一次 (SizeToContent 异步长高后调用; 也可用于打开时的兜底)。
    /// 无宿主 (Show 而非 ShowDialog) 时退化为屏幕工作区居中。
    /// </summary>
    public static void CenterToOwner(Window dialog)
    {
        var w = dialog.Bounds.Width;
        var h = dialog.Bounds.Height;
        PixelPoint target;
        if (dialog.Owner is Window owner)
        {
            var op = owner.Position;
            var ow = owner.Bounds.Width;
            var oh = owner.Bounds.Height;
            var x = op.X + (ow - w) / 2;
            var y = Math.Max(op.Y + MinTopGap, op.Y + (oh - h) / 2);
            target = new PixelPoint((int)Math.Round(x), (int)Math.Round(y));
        }
        else
        {
            var screen = dialog.Screens.ScreenFromWindow(dialog) ?? dialog.Screens.Primary;
            var wa = screen.WorkingArea;
            target = new PixelPoint(
                (int)Math.Round(wa.X + (wa.Width - w) / 2),
                (int)Math.Round(wa.Y + (wa.Height - h) / 2));
        }
        dialog.Position = target;
    }

    /// <summary>
    /// 挂接自动重居中: SizeChanged (异步内容加载/语言切换导致的尺寸变化) → 重居中;
    /// 窗口 Closed 自动解绑。重复调用安全 (解绑逻辑保证只挂一份)。
    /// </summary>
    public static void AttachAutoCenter(Window dialog)
    {
        void OnSizeChanged(object? sender, SizeChangedEventArgs e) => CenterToOwner(dialog);
        dialog.SizeChanged += OnSizeChanged;
        dialog.Closed += (_, _) => dialog.SizeChanged -= OnSizeChanged;
    }

    /// <summary>
    /// 屏幕外开门 (2026-09-23 插件弹窗白/黑帧修复 v2): 构造期调用, 把窗口开到屏幕外 ——
    /// 打开瞬间的「首帧未呈现」(整窗白) 与 SizeToContent 异步长高后的「未绘区」(黑)
    /// 都发生在屏幕外 (不可见); 内容就绪后调 <see cref="RevealWhenRendered"/> 移入屏幕
    /// (窗口表面内容跨移动保留, 移入即完整最终帧)。
    /// 三处 placer 弹窗 (插件设置/市场/QuickSwitch) 共用。
    /// ⚠️ 勿改回 Opacity 门: 本应用为软件渲染 + 重定向表面管线, Window.Opacity<1 会
    /// 打入 WS_EX_LAYERED 分层路径, 与 DWM 重定向内容冲突 ⇒ 未初始化分层缓冲渲染成
    /// 持久黑块且三弹窗全中毒 (2026-09-23 v1 实测, 见 [[keyflux-deprecated-approaches]] 27)。
    /// </summary>
    public static void OpenOffscreen(Window dialog)
    {
        dialog.WindowStartupLocation = WindowStartupLocation.Manual;
        dialog.Position = new PixelPoint(OffscreenCoord, OffscreenCoord);
    }

    /// <summary>
    /// 移入屏幕 (配 <see cref="OpenOffscreen"/>): 可选等异步内容加载任务 (capMs 上限,
    /// 网络差时带着加载态移入, 不让窗口无限离屏), 两轮 Render 优先级排空 + 30ms 缓冲
    /// (≈2 帧) 让最终尺寸的表面完成呈现, 再移入屏幕并挂自动重居中。
    /// </summary>
    /// <param name="dialog">目标窗口。</param>
    /// <param name="gate">可选的异步内容加载任务 (如市场目录拉取); 传 null 只等渲染排空。</param>
    /// <param name="capMs">gate 的最长离屏等待 (默认 1500ms)。</param>
    /// <param name="graceMs">渲染排空后的额外缓冲 (默认 30ms ≈ 2 帧)。</param>
    public static async Task RevealWhenRendered(Window dialog, Task? gate = null, int capMs = 1500, int graceMs = 30)
    {
        if (gate is not null)
        {
            await Task.WhenAny(gate, Task.Delay(capMs));
        }
        await Dispatcher.UIThread.InvokeAsync(() => { }, DispatcherPriority.Render);
        await Dispatcher.UIThread.InvokeAsync(() => { }, DispatcherPriority.Render);
        await Task.Delay(graceMs);
        CenterToOwner(dialog);
        AttachAutoCenter(dialog);
    }
}
