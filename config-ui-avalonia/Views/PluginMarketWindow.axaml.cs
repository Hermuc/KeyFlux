using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 插件市场窗口: 拉取 marketplace.json 目录 + 一键安装 (下载 zip -> 本地导入 API)。
/// 目录不可达时如实呈现错误与重试; 关闭后由插件页刷新已导入列表。
/// </summary>
public partial class PluginMarketWindow : Window
{
    public PluginMarketWindow()
    {
        InitializeComponent();
        Closed += (_, _) => UnsubscribeLanguage();
        I18n.Changed += OnLanguageChanged;
    }

    private void OnLanguageChanged()
    {
        if (DataContext is PluginMarketViewModel vm)
        {
            vm.OnLanguageChanged();
            foreach (var entry in vm.Entries) entry.RefreshLanguage();
        }
    }

    private void UnsubscribeLanguage() => I18n.Changed -= OnLanguageChanged;

    /// <summary>DataContext 在构造后由对象初始化器赋值, 目录拉取挂在此处保证时序。</summary>
    protected override void OnDataContextChanged(EventArgs e)
    {
        base.OnDataContextChanged(e);
        if (DataContext is PluginMarketViewModel vm)
        {
            _ = vm.LoadAsync();
        }
    }

    private void OnCloseClick(object? sender, RoutedEventArgs e) => Close();
}
