using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 选中动作单屏页视图交互: 确认框注入、行为目录拉取、行头点击展开手风琴等。
/// </summary>
public partial class SelectedActionPageView : UserControl
{
    public SelectedActionPageView()
    {
        InitializeComponent();
        DataContextChanged += (_, _) => InjectConfirmDialog();
        // 视图随导航重建而 VM 是单例, 确认框必须始终指向挂在树上的当前视图
        AttachedToVisualTree += (_, _) =>
        {
            InjectConfirmDialog();
            if (DataContext is SelectedActionPageViewModel vm)
            {
                _ = vm.EnsureBehaviorCatalogAsync();
            }
        };
    }

    private void InjectConfirmDialog()
    {
        // 仅当本视图当前挂在窗口上时才注入 (脱离树的旧视图 GetTopLevel 恒为 null)
        if (TopLevel.GetTopLevel(this) is Window && DataContext is SelectedActionPageViewModel vm)
        {
            vm.ConfirmAsync = ShowConfirmAsync;
        }
    }

    /// <summary>「管理行为…」: 打开行为库窗口, 关闭后重拉行为目录 (行为包可能增删)。</summary>
    private async void OnManageBehaviors(object? sender, RoutedEventArgs e)
    {
        if (TopLevel.GetTopLevel(this) is not Window owner ||
            DataContext is not SelectedActionPageViewModel vm)
        {
            return;
        }
        var win = new BehaviorLibraryWindow { DataContext = new BehaviorLibraryViewModel(vm.Main) };
        await win.ShowDialog(owner);
        await vm.ReloadBehaviorCatalogAsync();
    }

    /// <summary>行头点击展开/收起手风琴; 点在输入控件/按钮/开关上时交给控件自身。</summary>
    private void OnRowHeaderTapped(object? sender, TappedEventArgs e)
    {
        if (e.Source is Visual source
            && source.GetSelfAndVisualAncestors().Any(a => a is Button or ToggleSwitch or ComboBox or TextBox or CheckBox or ToggleButton))
        {
            return;
        }
        if (sender is Border { DataContext: MappingRowVm row })
        {
            row.ToggleExpandCommand.Execute(null);
        }
    }

    /// <summary>
    /// 类型下拉打开后给分隔项容器打 sep-item 类 (样式表据此把分隔项压成一条线)。
    /// </summary>
    private void OnTypeDropdownOpened(object? sender, EventArgs e)
    {
        if (sender is not ComboBox box) return;
        Dispatcher.UIThread.Post(() =>
        {
            foreach (var item in box.GetVisualDescendants().OfType<ComboBoxItem>())
            {
                var isSep = item.DataContext is Services.ComboOption { IsSeparator: true };
                if (isSep && !item.Classes.Contains("sep-item")) item.Classes.Add("sep-item");
                if (!isSep) item.Classes.Remove("sep-item");
            }
        }, DispatcherPriority.Loaded);
    }

    /// <summary>文本特征 Toggle 联动: 数据写入已在 VM 完成, 此处仅刷新同组其余三项视觉态。</summary>
    private void OnTextTypeToggleChanged(object? sender, RoutedEventArgs e)
    {
        if (sender is ToggleButton { DataContext: MappingRowVm row })
        {
            row.NotifyTogglesChanged();
        }
    }

    /// <summary>两按钮确认对话框 (复刻旧实现): 返回是否确认。</summary>
    private async Task<bool> ShowConfirmAsync(string title, string message)
    {
        if (TopLevel.GetTopLevel(this) is not Window owner) return false;

        var confirmed = false;
        var dialog = new Window
        {
            Title = title,
            Width = 400,
            SizeToContent = SizeToContent.Height,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            CanResize = false,
            ShowInTaskbar = false,
        };

        var ok = new Button
        {
            Content = "OK",
            Background = new SolidColorBrush(Color.Parse(ClaudePalette.Error)),
            Foreground = Brushes.White,
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(20, 7),
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        ok.Click += (_, _) => { confirmed = true; dialog.Close(); };

        var cancel = new Button
        {
            Content = Services.I18n.T("970"),
            Background = Brushes.White,
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(20, 7),
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        cancel.Click += (_, _) => dialog.Close();

        dialog.Content = new DockPanel
        {
            Margin = new Thickness(22, 18),
            Children =
            {
                new TextBlock
                {
                    Text = message,
                    TextWrapping = TextWrapping.Wrap,
                    FontSize = 13,
                    Margin = new Thickness(0, 0, 0, 18),
                    [DockPanel.DockProperty] = Dock.Top,
                },
                new StackPanel
                {
                    Orientation = Orientation.Horizontal,
                    HorizontalAlignment = HorizontalAlignment.Right,
                    Spacing = 10,
                    Children = { cancel, ok },
                },
            },
        };

        await dialog.ShowDialog(owner);
        return confirmed;
    }
}
