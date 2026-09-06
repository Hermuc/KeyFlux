using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// Settings 选项页视图 (逐项复刻 Settings.vue)。
/// 名称/触发键输入框失焦时复刻 checkKeymapData: 重复热键删行 + 规范化键名。
/// </summary>
public partial class SettingsPageView : UserControl
{
    public SettingsPageView() => InitializeComponent();

    /// <summary>名称/触发键失焦 -> 复刻 Vue 的 checkKeymapData (blur 事件)。</summary>
    private void OnRowFieldLostFocus(object? sender, RoutedEventArgs e)
    {
        if (sender is Control { DataContext: KeymapRowViewModel row }
            && DataContext is SettingsPageViewModel vm)
        {
            vm.CommitKeymapEdit(row);
        }
    }

    /// <summary>打开窗口条件组对话框 (模态); 保存后重建键位图系页面刷新分组下拉。</summary>
    private async void OnEditWindowGroups(object? sender, RoutedEventArgs e)
    {
        if (DataContext is SettingsPageViewModel vm
            && TopLevel.GetTopLevel(this) is Window owner)
        {
            var dialog = new WindowGroupDialogWindow
            {
                DataContext = new WindowGroupDialogViewModel(vm.Main),
            };
            await dialog.ShowDialog(owner);
            if (dialog.DataContext is WindowGroupDialogViewModel dlg && dlg.Saved)
            {
                vm.Main.RecreateKeymapPages();
            }
        }
    }
}
