using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 行为库窗口 (CONTRACTS §3.9): 浏览/新建/编辑/删除行为包; 新建与编辑打开
/// <see cref="BehaviorEditWindow"/> 模态表单; 变更标记 IsDirty, 「立即生效」显式重启引擎。
/// </summary>
public partial class BehaviorLibraryWindow : Window
{
    /// <summary>
    /// DataContext 由调用方以对象初始化器在构造之后赋值, 故此处不做构造期强转
    /// (先例: WindowGroupDialogWindow —— 事件处理器里按需取 VM, 避免 NRE 闪退)。
    /// </summary>
    public BehaviorLibraryWindow()
    {
        InitializeComponent();
        Services.Win32.DialogChrome.Apply(this);
        Closed += (_, _) => (DataContext as BehaviorLibraryViewModel)?.UnsubscribeLanguage();
    }

    /// <summary>DataContext 在构造后由对象初始化器赋值, 列表加载挂在此处保证时序正确。</summary>
    protected override void OnDataContextChanged(EventArgs e)
    {
        base.OnDataContextChanged(e);
        ReloadSilently();
    }

    private async void ReloadSilently(string? selectId = null)
    {
        try
        {
            if (DataContext is BehaviorLibraryViewModel vm) await vm.ReloadAsync(selectId);
        }
        catch (Exception ex)
        {
            if (DataContext is BehaviorLibraryViewModel vm) vm.StatusText = ex.Message;
        }
    }

    private async void OnNewClick(object? sender, RoutedEventArgs e)
        => await OpenEditAsync(null);

    private async void OnEditClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not BehaviorLibraryViewModel vm) return;
        if (vm.SelectedRow is { } row) await OpenEditAsync(row.Pack);
    }

    private async Task OpenEditAsync(BehaviorPack? existing)
    {
        if (DataContext is not BehaviorLibraryViewModel vm) return;
        var dialog = new BehaviorEditWindow { DataContext = new BehaviorEditViewModel(vm.Main, existing) };
        await dialog.ShowDialog(this);
        if (dialog.Saved)
        {
            vm.IsDirty = true;
            ReloadSilently(dialog.DataContext is BehaviorEditViewModel form ? form.SavedId : null);
        }
        else
        {
            ReloadSilently();
        }
    }

    private async void OnDeleteClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not BehaviorLibraryViewModel vm) return;
        try
        {
            var error = await vm.DeleteSelectedAsync();
            if (error is not null) vm.StatusText = error;
        }
        catch (Exception ex)
        {
            vm.StatusText = ex.Message;
        }
    }

    private async void OnApplyClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not BehaviorLibraryViewModel vm) return;
        try
        {
            var error = await vm.ApplyAsync();
            if (error is not null) vm.StatusText = error;
        }
        catch (Exception ex)
        {
            vm.StatusText = ex.Message;
        }
    }

    private void OnCloseClick(object? sender, RoutedEventArgs e) => Close();
}
