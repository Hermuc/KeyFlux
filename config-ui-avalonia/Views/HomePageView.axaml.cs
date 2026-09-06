using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>总览页视图 (复刻 Home.vue 的 config_doc 展示)。</summary>
public partial class HomePageView : UserControl
{
    public HomePageView() => InitializeComponent();

    /// <summary>「编辑总览」入口 (底部编辑区点击): 弹出 markdown 编辑窗口, 保存成功后刷新渲染。</summary>
    private async void OnEditOverviewClick(object? sender, RoutedEventArgs e)
        => await OpenOverviewEditorAsync();

    /// <summary>
    /// 打开总览编辑对话框 (独立方法, 与具体入口控件解耦:
    /// 原右上角按钮与现底部编辑区均复用此逻辑, 编辑区控件本身不含业务代码)。
    /// </summary>
    private async Task OpenOverviewEditorAsync()
    {
        if (DataContext is not HomePageViewModel vm) return;
        // 点击时本控件必已挂载到窗口, TopLevel 即宿主窗口
        var dialog = new OverviewEditWindow(vm.Main, vm.CurrentMd);
        await dialog.ShowDialog((Window)TopLevel.GetTopLevel(this)!);
        if (dialog.Saved)
        {
            await vm.ReloadAsync();
        }
    }
}
