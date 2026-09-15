using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 匹配类型弹窗外壳: 窗口属性与自绘标题栏同 MainWindow, 内容为 <see cref="MatchTypesPageView"/>。
/// 无边框 + 亚克力窗口 (ExtendClientAreaChromeHints=NoChrome) 需自带拖动区与最小化/最大化/关闭三键, 见 axaml。
/// 调用方在关闭后读 <c>MatchTypesPageViewModel.LastCreatedTypeId</c> 决定是否自动选中新类型。
/// </summary>
public partial class MatchTypesDialogWindow : Window
{
    public MatchTypesDialogWindow()
    {
        InitializeComponent();
        // 无边框模式系统不绘制标题, 窗口标题仅用于任务栏与无障碍朗读
        Title = I18n.T("2519");
    }

    /// <summary>标题栏拖动区: 左键拖拽移动窗口, 双击在最大化/还原之间切换。</summary>
    private void OnTitleBarPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (!e.GetCurrentPoint(this).Properties.IsLeftButtonPressed) return;
        if (e.ClickCount == 2)
        {
            ToggleMaximize();
            return;
        }
        // 最大化状态下拖拽交给系统还原语义, 避免出现"拖不动的最大化窗口"
        if (WindowState == WindowState.Normal)
        {
            BeginMoveDrag(e);
        }
    }

    private void OnMinimizeClick(object? sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void OnMaximizeClick(object? sender, RoutedEventArgs e) => ToggleMaximize();

    private void OnCaptionCloseClick(object? sender, RoutedEventArgs e) => Close();

    private void ToggleMaximize()
        => WindowState = WindowState == WindowState.Maximized ? WindowState.Normal : WindowState.Maximized;
}
