using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 插件页 QuickSwitch 配置对话框 (逻辑自 Settings 页「快速切换」分区迁入)。
/// 副本编辑: 关闭前不影响真源; 保存成功置 Saved 并 Close (由 PluginsPageView 刷新状态)。
/// </summary>
public partial class QuickSwitchDialogWindow : Window
{
    private bool _saving;

    /// <summary>保存成功后置 true。</summary>
    public bool Saved { get; private set; }

    public QuickSwitchDialogWindow()
    {
        InitializeComponent();
        TitleBarIconSuppressor.Attach(this);
        Title = I18n.T("2408");
        I18n.Changed += OnLanguageChanged;
        Closed += (_, _) => I18n.Changed -= OnLanguageChanged;
    }

    private void OnLanguageChanged()
    {
        if (DataContext is QuickSwitchDialogViewModel vm) vm.LanguageTick++;
        Title = I18n.T("2408");
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e) => Close();

    private async void OnSaveClick(object? sender, RoutedEventArgs e)
    {
        if (_saving) return;
        _saving = true;
        try
        {
            if (DataContext is QuickSwitchDialogViewModel vm && await vm.SaveAsync())
            {
                Saved = true;
                Close();
            }
            // 保存失败时 SaveAsync 内部已弹出原因, 窗口保持打开供修正
        }
        finally
        {
            _saving = false;
        }
    }
}
