using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 窗口条件组对话框 (复刻 WindowGroupDialog.vue): 模态打开, 保存才整体替换
/// options.windowGroups; 对话框打开期间跟随全局语言切换。
/// </summary>
public partial class WindowGroupDialogWindow : Window
{
    public WindowGroupDialogWindow()
    {
        InitializeComponent();
        Services.Win32.DialogChrome.Apply(this);
        // 标题栏小图标透明化 (与主窗口同一助手, 行为一致)
        TitleBarIconSuppressor.Attach(this);
        I18n.Changed += OnLanguageChanged;
        Closed += (_, _) => I18n.Changed -= OnLanguageChanged;
    }

    private void OnLanguageChanged()
    {
        if (DataContext is WindowGroupDialogViewModel vm)
        {
            vm.LanguageTick++;
            vm.RefreshTip();
        }
    }

    /// <summary>取消: 直接关闭 (副本未写回)。</summary>
    private void OnCancelClick(object? sender, RoutedEventArgs e) => Close();

    /// <summary>保存: 执行 VM 保存后关闭窗口。</summary>
    private void OnSaveClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is WindowGroupDialogViewModel vm)
        {
            vm.SaveCommand.Execute(null);
        }
        Close();
    }
}
