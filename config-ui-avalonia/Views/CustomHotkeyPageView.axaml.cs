using Avalonia.Controls;
using Avalonia.Input;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 全局自定义热键页视图 (复刻 views/CustomHotkey.vue):
/// 整行点选/聚焦行即选中; 热键编辑框失焦提交 (复刻 @change)。
/// </summary>
public partial class CustomHotkeyPageView : UserControl
{
    public CustomHotkeyPageView() => InitializeComponent();

    /// <summary>整行点选 (复刻 tr @click=checkRow)。</summary>
    private void OnRowPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is Control { DataContext: CustomHotkeyRowVm row }
            && DataContext is CustomHotkeyPageViewModel vm)
        {
            vm.SelectRow(row);
        }
    }

    /// <summary>编辑框聚焦也视为选中该行 (覆盖 TextBox 拦截指针事件的场景)。</summary>
    private void OnRowFocused(object? sender, GotFocusEventArgs e)
    {
        if (sender is Control { DataContext: CustomHotkeyRowVm row }
            && DataContext is CustomHotkeyPageViewModel vm)
        {
            vm.SelectRow(row);
        }
    }

    /// <summary>热键失焦提交 (复刻 v-text-field @change=changeCustomHotkey)。</summary>
    private void OnHotkeyLostFocus(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (sender is Control { DataContext: CustomHotkeyRowVm row }
            && DataContext is CustomHotkeyPageViewModel vm)
        {
            vm.CommitRow(row);
        }
    }
}
