using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.VisualTree;

namespace KeyFlux.Settings.Views;

/// <summary>键位图页视图 (复刻 views/Keymap.vue)。</summary>
public partial class KeymapPageView : UserControl
{
    public KeymapPageView() => InitializeComponent();

    /// <summary>
    /// 键盘行面板尺寸变化 -> 编辑面板宽度同步: 遍历 KeyboardGrid 下各行面板
    /// (横向且 Left 对齐, Bounds.Width 即该行实际自然宽度), 取最大值赋给编辑面板外框,
    /// 使外框与键盘网格最宽一行严格等宽 (内部白卡片由外框 Padding=2,0 左右各内缩 2);
    /// 不同布局/模式下最宽行不同, 故不写死数值。
    /// 不再向右补偿占位 (历史遗留的 616-W 右 Margin 会把左列 StackPanel 撑宽,
    /// 经 Viewbox 等比缩放后在键盘网格右缘与备注列之间放大成百余物理像素空白,
    /// 实测 UIA: 改前模式页间距 153px >> Command 页 41px); 左列占位 = 最宽行实际宽度,
    /// 备注列间距仅由使用处左 Margin 32 + Viewbox 固有平台余量构成, 与 Command 页一致。
    /// 行面板位于 ItemsPanelTemplate 内 (模板级 NameScope, 页面 code-behind 无法以 x:Name 引用),
    /// 故在 XAML 中直接挂事件; XAML 中 Width=620 仅作首帧初始值。
    /// </summary>
    private void OnKeyboardRowSizeChanged(object? sender, SizeChangedEventArgs e)
    {
        var max = 0.0;
        foreach (var panel in KeyboardGrid.GetVisualDescendants().OfType<StackPanel>())
        {
            if (panel.Orientation == Orientation.Horizontal)
                max = Math.Max(max, panel.Bounds.Width);
        }
        if (max <= 0) return;

        EditorFrame.Width = max;
    }
}
