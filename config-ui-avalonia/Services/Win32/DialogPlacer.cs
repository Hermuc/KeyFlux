using System;
using Avalonia;
using Avalonia.Controls;

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
//   4) 时序约定 (2026-09-23 白/黑帧修复定稿): 内容就绪后才 ShowDialog (插件设置弹窗的
//      LoadAsync 已移到调用方) —— 勿再引入开窗后异步长高的形态 (白帧/黑帧/离屏门均已被否)。
//      2026-09-25 追记: 「内容就绪后开窗」只消掉了 SizeToContent 的大跳, 开窗瞬间仍有
//      首帧前的白帧 + 微小尺寸变动引发的重定位 —— 由 DialogReveal (分层窗口首帧门) 统一
//      收尾; 本类只负责「在哪」, 不再掺和「何时可见」。
//
// 新增插件弹窗的使用方式 (无需重复配置):
//   构造函数里 AttachAutoCenter(this); 内容异步加载完成后可再 CenterToOwner(this) 兜底。
// ============================================================================

internal static class DialogPlacer
{
    /// <summary>高度超出宿主时贴顶保留的最小顶边距 (DIP)。集中参数, 便于全局统一调整。</summary>
    public const double MinTopGap = 8;

    /// <summary>
    /// 相对宿主窗口居中一次 (SizeToContent 异步长高后调用; 也可用于打开时的兜底)。
    /// 无宿主 (Show 而非 ShowDialog) 时退化为屏幕工作区居中。
    /// </summary>
    public static void CenterToOwner(Window dialog)
    {
        // 2026-09-25 DPI 修复: 旧实现把 DIP 差值 (Bounds) 直接加进物理坐标 (Position/
        // WorkingArea 均为物理像素), 125% DPI 下偏移量被少乘 1.25 —— 弹窗系统性左偏/上偏
        // (用户截图实测: 宿主宽 ~1094 DIP、弹窗 520 DIP 时左偏 ~72 物理像素, 与两帧位置差
        // ~90px 吻合)。改为: 客户区物理原点经 PointToScreen 取得, 尺寸按各自 RenderScaling
        // 换算, 全程物理像素运算。
        var dw = dialog.Bounds.Width * dialog.RenderScaling;
        var dh = dialog.Bounds.Height * dialog.RenderScaling;
        PixelPoint target;
        if (dialog.Owner is Window owner)
        {
            var origin = owner.PointToScreen(new Point(0, 0)); // 客户区左上角 (物理像素)
            var ow = owner.Bounds.Width * owner.RenderScaling;
            var oh = owner.Bounds.Height * owner.RenderScaling;
            var x = origin.X + (ow - dw) / 2;
            var y = Math.Max(origin.Y + MinTopGap * dialog.RenderScaling, origin.Y + (oh - dh) / 2);
            target = new PixelPoint((int)Math.Round(x), (int)Math.Round(y));
        }
        else
        {
            var screen = dialog.Screens.ScreenFromWindow(dialog) ?? dialog.Screens.Primary;
            var wa = screen.WorkingArea; // 物理像素
            target = new PixelPoint(
                (int)Math.Round(wa.X + (wa.Width - dw) / 2),
                (int)Math.Round(wa.Y + (wa.Height - dh) / 2));
        }
        dialog.Position = target;
    }

    /// <summary>
    /// 挂接自动重居中: SizeChanged (异步内容加载/语言切换导致的尺寸变化) → 重居中;
    /// 窗口 Closed 自动解绑。重复调用安全 (解绑逻辑保证只挂一份)。
    /// 与 <see cref="DialogReveal"/> 配合: 开窗首帧前的重定位发生在不可见期, 用户无感。
    /// </summary>
    public static void AttachAutoCenter(Window dialog)
    {
        void OnSizeChanged(object? sender, SizeChangedEventArgs e) => CenterToOwner(dialog);
        dialog.SizeChanged += OnSizeChanged;
        dialog.Closed += (_, _) => dialog.SizeChanged -= OnSizeChanged;
    }
}
