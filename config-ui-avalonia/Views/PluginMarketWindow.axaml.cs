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
    /// <summary>目录拉取任务 (OnDataContextChanged 启动; Opened 显形时作 gate, 网络差最多隐身 1.5s)。</summary>
    private Task? _loadTask;

    public PluginMarketWindow()
    {
        InitializeComponent();
        ComponentFocusRing.Attach(this); // 焦点环最内层转移 (替代 :focus-within)
        Services.Win32.DialogChrome.Apply(this);
        Closed += (_, _) => UnsubscribeLanguage();
        Services.Win32.DialogPlacer.OpenOffscreen(this); // 屏幕外开门: 挡住打开瞬间的白帧与目录长高后的黑帧 (09-23)
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
            _loadTask = vm.LoadAsync();
        }
    }

    /// <summary>Opened 显形: 至多等目录拉取 1.5s (网络差时带着加载态显形, 不无限隐身)。</summary>
    protected override async void OnOpened(EventArgs e)
    {
        base.OnOpened(e);
        await Services.Win32.DialogPlacer.RevealWhenRendered(this, _loadTask);
    }

    private void OnCloseClick(object? sender, RoutedEventArgs e) => Close();
}
