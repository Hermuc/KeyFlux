using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.VisualTree;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// Settings 选项页视图 (逐项复刻 Settings.vue)。
/// 名称/触发键输入框失焦时复刻 checkKeymapData: 重复热键删行 + 规范化键名。
/// </summary>
public partial class SettingsPageView : UserControl
{
    public SettingsPageView() => InitializeComponent();

    /// <summary>
    /// 组件框任意处点击 = 展开/收起 (用户报: 只有点文字部分才能展开)。
    /// 实现转交给卡内的分区标题按钮 (ToggleSectionCommand / 编辑程序分组的 Click),
    /// 逻辑单源; 交互控件 (TextBox/Slider/ToggleSwitch/ComboBox 等) 自行处理指针 ——
    /// 判定用 Focusable (可聚焦控件默认 true, 纯文本/Border 默认 false, 前向兼容)。
    /// </summary>
    private void OnCardPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is not Border card) return;
        for (var v = (Avalonia.Visual?)e.Source; v is not null && !ReferenceEquals(v, card); v = v.GetVisualParent())
        {
            if (v is InputElement { Focusable: true }) return;
        }
        var header = card.GetVisualDescendants().OfType<Button>()
            .FirstOrDefault(b => b.Classes.Contains("sectionHeader"));
        if (header is null) return;
        if (header.Command?.CanExecute(header.CommandParameter) == true)
        {
            header.Command.Execute(header.CommandParameter);
        }
        else
        {
            // 无 Command 的标题按钮 (编辑程序分组 = Click 事件): 派发路由 Click
            header.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
        }
    }

    /// <summary>名称/触发键失焦 -> 复刻 Vue 的 checkKeymapData (blur 事件)。</summary>
    private void OnRowFieldLostFocus(object? sender, RoutedEventArgs e)
    {
        if (sender is Control { DataContext: KeymapRowViewModel row }
            && DataContext is SettingsPageViewModel vm)
        {
            vm.CommitKeymapEdit(row);
        }
    }

    /// <summary>打开窗口条件组对话框 (模态); 保存后重建键位图系页面刷新分组下拉。</summary>
    private async void OnEditWindowGroups(object? sender, RoutedEventArgs e)
    {
        if (DataContext is SettingsPageViewModel vm
            && TopLevel.GetTopLevel(this) is Window owner)
        {
            var dialog = new WindowGroupDialogWindow
            {
                DataContext = new WindowGroupDialogViewModel(vm.Main),
            };
            await dialog.ShowDialog(owner);
            if (dialog.DataContext is WindowGroupDialogViewModel dlg && dlg.Saved)
            {
                vm.Main.RecreateKeymapPages();
            }
        }
    }

    // ----------------------------------------------------- 自定义热键分区 (原 Custom Hotkeys 页迁入)

    /// <summary>热键编辑框聚焦即选中该行 (编辑器随选中键定位, 复刻 OnRowFocused)。</summary>
    private void OnCustomHotkeyRowFocused(object? sender, GotFocusEventArgs e)
    {
        if (sender is Control { DataContext: CustomHotkeyRowVm row }
            && DataContext is SettingsPageViewModel { CustomHotkeys: { } ck })
        {
            ck.SelectRow(row);
        }
    }

    /// <summary>热键失焦提交 (复刻 @change=changeCustomHotkey: 改名后选中新键)。</summary>
    private void OnCustomHotkeyLostFocus(object? sender, RoutedEventArgs e)
    {
        if (sender is Control { DataContext: CustomHotkeyRowVm row }
            && DataContext is SettingsPageViewModel { CustomHotkeys: { } ck })
        {
            ck.CommitRow(row);
        }
    }

    /// <summary>
    /// 单击「功能」(原备注列): 先选中该行动作, 再弹出动作编辑面板窗口模态编辑;
    /// 编辑字段经 Core.NotifyDataChanged 即时刷新行表备注, 无需关闭后手动刷新。
    /// </summary>
    private async void OnCustomHotkeyCommentClick(object? sender, RoutedEventArgs e)
    {
        if (sender is Control { DataContext: CustomHotkeyRowVm row }
            && DataContext is SettingsPageViewModel { CustomHotkeys: { } ck }
            && TopLevel.GetTopLevel(this) is Window owner)
        {
            ck.SelectRow(row);
            var dialog = new ActionEditorWindow { DataContext = ck };
            await dialog.ShowDialog(owner);
        }
    }
}
