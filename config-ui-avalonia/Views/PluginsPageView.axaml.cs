using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Input;
using Avalonia.Platform.Storage;
using Avalonia.VisualTree;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 插件页视图 (左侧导航「插件」) —— Claude 暖色风格 (DESIGN.md)。
/// 统一插件列表 (内置「快速切换」+ 用户插件不分区): 每卡 = 信息区 (内置卡可点开配置对话框)
/// + 开关 (状态文字在开关正下方) + 删除 (仅用户卡); 顶部入口 = 插件市场 / 本地 zip 导入。
/// 文件选择与模态窗口属视图职责 (需 StorageProvider / ShowDialog owner), 逻辑在 VM。
/// </summary>
public partial class PluginsPageView : UserControl
{
    public PluginsPageView()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    /// <summary>每次导航进入时刷新插件目录 (DataTemplate 每次导航重建视图)。</summary>
    private async void OnLoaded(object? sender, RoutedEventArgs e)
    {
        if (DataContext is PluginsPageViewModel vm)
        {
            try { await vm.ReloadAsync(); }
            catch { /* 目录加载失败已由 VM 状态呈现 */ }
        }
    }

    /// <summary>
    /// 卡内任意区域点击 = 选中该卡 (用户报: 只有点文字部分才有橙环): 非交互区域 (内衬/空隙/纯文字)
    /// 按下时把焦点转移到信息区按钮, 由 Border.pluginCard:focus-within 点亮橙色焦点环。
    /// 交互子项 (开关/删除按钮) 自己处理焦点与语义, 不抢 —— Avalonia 无 ButtonBase,
    /// ToggleSwitch/ToggleButton/普通按钮全是 Button 派生, is Button 一网打尽。
    /// </summary>
    private void OnCardPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is not Border card) return;
        for (var v = (Avalonia.Visual?)e.Source; v is not null && !ReferenceEquals(v, card); v = v.GetVisualParent())
        {
            if (v is Button) return;
        }
        card.GetVisualDescendants().OfType<Button>()
            .FirstOrDefault(b => b.Classes.Contains("cardBody"))?.Focus();
    }

    /// <summary>卡信息区点击: 内置卡 (CanConfigure) 弹 QuickSwitchDialogWindow; 用户卡暂无配置。</summary>
    private async void OnCardConfigureClick(object? sender, RoutedEventArgs e)
    {
        if (sender is Button { CommandParameter: PluginCardVm card } && card.CanConfigure
            && DataContext is PluginsPageViewModel vm
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

    /// <summary>导入本地插件包: 选 zip -> 读字节 -> POST /api/plugins/import。</summary>
    private async void OnImportClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not PluginsPageViewModel vm
            || TopLevel.GetTopLevel(this) is not Window owner)
        {
            return;
        }
        var picker = owner.StorageProvider;
        var options = new FilePickerOpenOptions
        {
            Title = I18n.T("2427"),
            AllowMultiple = false,
            FileTypeFilter =
            [
                new FilePickerFileType(I18n.T("2437")) { Patterns = ["*.zip"] },
            ],
        };
        var files = await picker.OpenFilePickerAsync(options);
        if (files.Count == 0) return;
        try
        {
            await using var stream = await files[0].OpenReadAsync();
            using var ms = new MemoryStream();
            await stream.CopyToAsync(ms);
            await vm.ImportZipAsync(ms.ToArray(), files[0].Name);
        }
        catch (Exception ex)
        {
            vm.Main.ShowMessage(I18n.T("2432"), ex.Message);
        }
    }

    /// <summary>打开插件市场窗口; 关闭后刷新已导入列表 (市场内可能已安装新插件)。</summary>
    private async void OnMarketClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not PluginsPageViewModel vm
            || TopLevel.GetTopLevel(this) is not Window owner)
        {
            return;
        }
        var dialog = new PluginMarketWindow
        {
            DataContext = new PluginMarketViewModel(vm.Main),
        };
        await dialog.ShowDialog(owner);
        await vm.ReloadAsync();
    }
}
