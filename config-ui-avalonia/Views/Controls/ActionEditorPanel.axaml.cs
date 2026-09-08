using System.ComponentModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views.Controls;

/// <summary>
/// 动作编辑面板 (复刻 actions/Action.vue): 窗口分组 + 动作类型下拉 + 按类型分发编辑器。
/// </summary>
public partial class ActionEditorPanel : UserControl
{
    private ActionEditorViewModel? _hooked;

    public ActionEditorPanel() => InitializeComponent();

    /// <summary>类型 8 示例下拉选中 -> 填入 ahkCode (复刻 v-combobox items)。</summary>
    private void OnAhkExampleSelected(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string example }
            && DataContext is AhkCodeEditorVm vm)
        {
            vm.AhkCode = example;
        }
    }

    protected override void OnDataContextChanged(EventArgs e)
    {
        base.OnDataContextChanged(e);
        if (_hooked is not null) _hooked.PropertyChanged -= OnVmPropertyChanged;
        _hooked = DataContext as ActionEditorViewModel;
        if (_hooked is not null) _hooked.PropertyChanged += OnVmPropertyChanged;
    }

    /// <summary>
    /// 编辑器重建 (点选键格/切换类型/切语言) 后, 把已选中的单选项滚入可视区:
    /// 文字编辑相关 (类型 7) 选项分两行, 第二行 (编辑键/特殊键) 在面板固定高度内的
    /// 滚动区下方, 不滚动定位会被误认为"没定位到该键功能" (2026-09-08)。
    /// 注: 不用 BringIntoView (在 Viewbox 缩放页内实测不触发滚动), 直接算相对位置设 Offset;
    /// 外层 Background Post 保证编辑器子树完成布局后再取坐标。
    /// </summary>
    private void OnVmPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(ActionEditorViewModel.Editor)) return;
        Dispatcher.UIThread.Post(ScrollCheckedIntoView, DispatcherPriority.Background);
    }

    private void ScrollCheckedIntoView()
    {
        var rb = this.GetVisualDescendants().OfType<RadioButton>()
            .FirstOrDefault(b => b.IsChecked == true);
        if (rb is null || rb.Bounds.Height <= 0) return; // 子树未就绪, 放弃 (下次切换会重试)
        var sv = EditorScroll;
        var center = rb.TranslatePoint(new Point(rb.Bounds.Width / 2, rb.Bounds.Height / 2), sv);
        if (center is not Point p) return;
        var target = p.Y + sv.Offset.Y - sv.Viewport.Height / 2;
        target = Math.Clamp(target, 0, Math.Max(0, sv.Extent.Height - sv.Viewport.Height));
        sv.Offset = new Avalonia.Vector(sv.Offset.X, target);
    }
}
