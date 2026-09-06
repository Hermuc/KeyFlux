using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>缩写命令页视图 (复刻 views/Abbr.vue): 命令框回车执行指令。</summary>
public partial class AbbrPageView : UserControl
{
    public AbbrPageView() => InitializeComponent();

    /// <summary>
    /// chips 面板尺寸变化 -> 编辑框宽度同步: chips 行为自然宽度换行,
    /// WrapPanel Left 对齐后 ActualWidth 即实际最宽行宽, 编辑框与其严格一致且不拉伸格子。
    /// WrapPanel 位于 ItemsPanelTemplate 内 (模板级 NameScope, 页面 code-behind 无法以 x:Name 引用),
    /// 故在 XAML 中直接挂事件, sender 即面板实例; Width=570 仅作首帧初始值。
    /// </summary>
    private void OnChipsPanelSizeChanged(object? sender, SizeChangedEventArgs e)
        => EditorPanel.Width = e.NewSize.Width;

    /// <summary>命令框回车 (复刻 @keydown.enter=runCmd)。</summary>
    private void OnCmdKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter && DataContext is AbbrPageViewModel vm)
        {
            vm.RunCmdCommand.Execute(null);
            e.Handled = true;
        }
    }
}
