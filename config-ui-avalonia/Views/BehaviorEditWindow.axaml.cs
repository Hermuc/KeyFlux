using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 行为新建/编辑表单窗口 (模态): 保存经后端校验, 成功后置 <see cref="Saved"/> 由
/// 行为库窗口据此刷新目录并标记待生效。
/// </summary>
public partial class BehaviorEditWindow : Window
{
    /// <summary>保存成功标记 (行为库窗口据此刷新 + 标记 IsDirty)。</summary>
    public bool Saved { get; private set; }

    /// <summary>DataContext 构造后才赋值, 此处不做构造期强转 (同 BehaviorLibraryWindow)。</summary>
    public BehaviorEditWindow()
    {
        InitializeComponent();
        Services.Win32.DialogChrome.Apply(this);
        Closed += (_, _) => (DataContext as BehaviorEditViewModel)?.UnsubscribeLanguage();
    }

    private async void OnSaveClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not BehaviorEditViewModel vm) return;
        try
        {
            var error = await vm.SaveAsync();
            if (error is not null)
            {
                vm.StatusText = error;
                return;
            }
            Saved = true;
            Close();
        }
        catch (Exception ex)
        {
            vm.StatusText = ex.Message;
        }
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e) => Close();
}
