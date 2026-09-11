using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 插件页视图 (左侧导航「插件」): 内置插件「快速切换」现状 + 第三方插件市场未开放说明。
/// 点击内置插件卡片弹 QuickSwitchDialogWindow 编辑配置 (副本编辑, 保存才落盘), 关闭后刷新启用状态。
/// </summary>
public partial class PluginsPageView : UserControl
{
    public PluginsPageView() => InitializeComponent();

    private async void OnQuickSwitchCardClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is PluginsPageViewModel vm
            && TopLevel.GetTopLevel(this) is Window owner)
        {
            var dialog = new QuickSwitchDialogWindow
            {
                DataContext = new QuickSwitchDialogViewModel(vm.Main),
            };
            await dialog.ShowDialog(owner);
            vm.Refresh();
        }
    }
}
