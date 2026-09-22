using KeyFlux.Settings.Theming;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 行为库窗口 (CONTRACTS §3.9): 浏览/新建/编辑/删除行为包; 变更标记 IsDirty, 「立即生效」显式重启引擎。
/// 新建与编辑**在本窗右侧详情区就地展开表单** (不再弹 <c>BehaviorEditWindow</c>, 避免三层弹窗);
/// 表单的保存/取消直接绑定 VM 命令 (<c>SaveEditorCommand</c> / <c>CancelEditorCommand</c>), 此处只转发列表操作。
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
        WindowSurface.Attach(this, WindowSurface.ContentSurfaceResourceKey); // 真·毛玻璃: 窗口打开时挂 accent 亚克力磨砂 (迟开窗口经此补挂; ContentSurface 供 R3-1 退化恢复)
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

    /// <summary>「新建行为」: 就地展开空白表单 (不弹窗)。</summary>
    private void OnNewClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is BehaviorLibraryViewModel vm) vm.OpenCreate();
    }

    /// <summary>「编辑行为」: 就地展开填充表单 (仅自定义包)。</summary>
    private void OnEditClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not BehaviorLibraryViewModel vm) return;
        if (vm.SelectedRow is { IsUser: true } row) vm.OpenEdit(row.Pack);
    }

    /// <summary>列表行双击 = 编辑行为 (仅自定义包; 内置包只读 ⇒ 无操作)。</summary>
    private void OnRowDoubleTapped(object? sender, TappedEventArgs e)
    {
        if (DataContext is not BehaviorLibraryViewModel vm) return;
        if (vm.SelectedRow is { IsUser: true } row) vm.OpenEdit(row.Pack);
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
}
