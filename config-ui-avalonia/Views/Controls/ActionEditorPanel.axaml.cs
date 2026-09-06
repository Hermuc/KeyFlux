using Avalonia.Controls;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views.Controls;

/// <summary>
/// 动作编辑面板 (复刻 actions/Action.vue): 窗口分组 + 动作类型下拉 + 按类型分发编辑器。
/// </summary>
public partial class ActionEditorPanel : UserControl
{
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
}
