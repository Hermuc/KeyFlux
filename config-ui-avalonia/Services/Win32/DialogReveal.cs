using System;
using System.Runtime.CompilerServices;
using Avalonia.Controls;
using Avalonia.Threading;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Services.Win32;

// ============================================================================
// DialogReveal —— 弹窗首帧门 (2026-09-25 修复「白窗 → 变色/移位 → 内容」三段式弹窗)。
//
// 现象 (用户报障, 插件页点卡片): 弹窗打开瞬间先闪一块纯白空窗 (图1), 随后变色并移位
// (图2), 最后才出现应有内容 —— 多个视觉阶段全部发生在窗口可见之后。
//
// 成因 (软件渲染管线, Program.cs 默认 Software + RedirectionSurface):
//   1) PlatformImpl.Show 之后到首个合成帧画进表面之前, DWM 重定向表面未被画过 ——
//      窗口类 hbrBackground=0, 首帧呈现前没人上色, 呈现的初始表面即纯白;
//   2) 首帧前后布局定型/取整触发 Window.SizeChanged → DialogPlacer 重居中 ⇒ 窗口跳位;
//   3) 内容随场景推进再补一帧。三者叠加 = 用户看到的三段式。
//
// 为什么不能用 Window.Opacity 门: 11.3 起 ITopLevelImpl 已无 Opacity 通道, Visual.Opacity
// 是合成器层属性; 软件路径呈现时 alpha 被丢弃 —— 根节点 Opacity=0 会把整个表面画成
// 不透明黑 (历史上出现过的「黑帧」), 比白帧更糟。
// 为什么不用 IRenderer.SceneInvalidated / ITopLevelImpl.Compositor: 11.3 的公开程序集
// 里这些成员的访问器被声明为 internal ([PrivateApi] 稳定面), 外部不可订阅。
//
// 方案: 绕开 Avalonia, 直接用 Win32 分层窗口 (NativeMethods 既有原语, 高亮框窗口同款):
//   · 构造期挂 WS_EX_LAYERED + alpha=0 —— Window 构造时 hwnd 已由
//     PlatformManager.CreateWindow() 建好, 先于 ShowDialog; Show 后的一切中间态
//     (白帧/移位/上色) 全部发生在不可见期;
//   · 显现时机 (全程公共 API): Opened (首个渲染 pass 已由 StartRendering 排队) →
//     RequestAnimationFrame 回调 (在渲染 pass 开始时触发) → Post(Reveal, Background) ——
//     Background 低于 Render, Reveal 在整个渲染 pass 结束 (合成 batch 已下发渲染线程)
//     之后执行; 此时窗口仍 alpha=0, 重居中无感, 淡入即「最终形态淡入」;
//   · 淡入用 S 曲线 (先缓后急再缓): 起步 alpha 贴 0, 即便渲染线程仍在画首帧的几十毫秒
//     内, 合成出去的也只是近全透的表面, 不可能再出现可感知的白帧;
//   · 信号迟迟不来 (极端负载/渲染器异常) 时超时兜底显现, 弹窗绝不失踪;
//   · 动效总闸 (MotionPreferences, KEYFLUX_NO_MOTION=1) 关闭时跳过淡入直接显现。
// 幂等与降级: 每窗口仅 Attach 一次; 句柄不可得或挂分层样式失败时整条门自动跳过
// (完全回落旧行为); 揭示路径任何异常都兜底把 alpha 拉满, 不留隐形弹窗。
// 层级关系: 本门只管「何时可见」; 「在哪」仍归 DialogPlacer —— Reveal 时显式调一次
// CenterToOwner, 与 AttachAutoCenter 的 SizeChanged 安全网共用同一套 (已修 DPI 的) 数学。
// ============================================================================

internal static class DialogReveal
{
    /// <summary>淡入时长 = 微过渡档 (与 ClaudeMotion.Micro 同源 120ms, 服务层引用令牌先例见 SectionUnroll)。</summary>
    private static readonly TimeSpan FadeDuration = ClaudeMotion.Micro;

    /// <summary>首帧信号迟到兜底 (超时直接显现; 覆盖渲染器异常/极端负载)。</summary>
    private static readonly TimeSpan RevealTimeout = TimeSpan.FromMilliseconds(400);

    /// <summary>淡入定时器帧间隔 (≈100fps, 120ms 共 12 档, 步进足够平滑)。</summary>
    private static readonly TimeSpan FadeTick = TimeSpan.FromMilliseconds(10);

    private static readonly ConditionalWeakTable<Window, object> Attached = new();

    /// <summary>接入首帧门。窗口构造函数中调用一次 (须在任何 Show/ShowDialog 之前)。</summary>
    public static void Attach(Window dialog)
    {
        if (!Attached.TryAdd(dialog, new object())) return;

        var hwnd = dialog.TryGetPlatformHandle()?.Handle ?? IntPtr.Zero;
        if (hwnd == IntPtr.Zero) return;

        // 挂分层样式并置全透明。样式挂不上 (非预期环境) 则整条门跳过, 完全回落旧行为 ——
        // 此时也不该留下 alpha=0 的分层属性 (虽然没挂样式它本身不生效, 防御性跳过)。
        if (!TryAddLayeredStyle(hwnd)) return;
        SetLayeredAlpha(hwnd, 0);

        // 门状态存于闭包 (与 TitleBarIconSuppressor 同款, 随窗口事件生命周期存续)
        var revealed = false;
        DispatcherTimer? fadeTimer = null;
        DispatcherTimer? timeoutTimer = null;

        void Reveal()
        {
            if (revealed) return;
            revealed = true; // 先落闸: 幂等 (首帧信号/超时兜底只生效一次)
            timeoutTimer?.Stop();
            try
            {
                // 首帧渲染 pass 已结束但窗口仍不可见: 这里做最后一跳 (几何已定型, 与
                // AttachAutoCenter 同一套数学, 等值时为 no-op), 之后以最终位置淡入
                DialogPlacer.CenterToOwner(dialog);

                if (!MotionPreferences.AnimationsEnabled)
                {
                    SetLayeredAlpha(hwnd, 255);
                    return;
                }

                var elapsed = TimeSpan.Zero;
                fadeTimer = new DispatcherTimer { Interval = FadeTick };
                fadeTimer.Tick += (_, _) =>
                {
                    elapsed += FadeTick;
                    var t = Math.Min(1.0, elapsed / FadeDuration);
                    var eased = t * t * (3 - 2 * t); // smoothstep: 两端缓中段顺
                    SetLayeredAlpha(hwnd, (byte)Math.Round(byte.MaxValue * eased));
                    if (t >= 1) fadeTimer!.Stop();
                };
                fadeTimer.Start();
            }
            catch
            {
                // 兜底: 揭示路径出任何意外都不允许留下隐形弹窗
                SetLayeredAlpha(hwnd, 255);
            }
        }

        timeoutTimer = new DispatcherTimer { Interval = RevealTimeout };
        timeoutTimer.Tick += (_, _) => Reveal();

        // Opened 在 ShowCore 内同步触发, 此刻首个渲染 pass 已排队未执行: 挂 rAF 回调,
        // 它会在该 pass 开始时被唤起, 再以 Background 优先级把 Reveal 排到 pass 结束后
        dialog.Opened += (_, _) =>
        {
            dialog.RequestAnimationFrame(_ =>
                Dispatcher.UIThread.Post(Reveal, DispatcherPriority.Background));
        };

        timeoutTimer.Start();

        dialog.Closed += (_, _) =>
        {
            timeoutTimer.Stop();
            fadeTimer?.Stop();
        };
    }

    /// <summary>
    /// 给窗口补挂 WS_EX_LAYERED (幂等)。返回 false = 不可用, 门整体跳过。
    /// SetWindowLong 的返回值是「旧值」(0 是合法旧值, 不能当失败判据), 故以回读验证为准。
    /// </summary>
    private static bool TryAddLayeredStyle(IntPtr hwnd)
    {
        try
        {
            var style = unchecked((uint)NativeMethods.GetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE).ToInt32());
            if ((style & NativeMethods.WS_EX_LAYERED) == 0)
            {
                NativeMethods.SetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE,
                    new IntPtr(unchecked((int)(style | NativeMethods.WS_EX_LAYERED))));
                style = unchecked((uint)NativeMethods.GetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE).ToInt32());
            }
            return (style & NativeMethods.WS_EX_LAYERED) != 0;
        }
        catch
        {
            return false;
        }
    }

    private static void SetLayeredAlpha(IntPtr hwnd, byte alpha)
    {
        NativeMethods.SetLayeredWindowAttributes(hwnd, 0, alpha, NativeMethods.LWA_ALPHA);
    }
}
