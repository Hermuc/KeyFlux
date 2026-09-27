using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Services;
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
        ComponentFocusRing.Attach(this); // 焦点环最内层转移 (替代 :focus-within)
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
        LayoutUpdated += (_, _) => NeuterComboBoxHighlightBorder();
    }

    /// <summary>
    /// 输入框和谐灰边框 (2026-09-23) 的 ComboBox 收尾: Fluent 的 ControlTheme 给
    /// Border#HighlightBackground 静态设了**不透明主题色背景** (#ffc96442) + 40% 黑边 ——
    /// 页面级样式赢不过 ControlTheme (模板应用晚于页面样式), 但**本地值 (LocalValue)
    /// 优先级高于一切样式** ⇒ 用 LayoutUpdated 钩子在模板 (重)应用后直写。
    /// 设为同值无副作用 (Avalonia 相等即不触发变更), 无需脏检查。
    /// </summary>
    private void NeuterComboBoxHighlightBorder()
    {
        foreach (var border in this.GetVisualDescendants().OfType<Border>())
        {
            if ((border as StyledElement)?.Name != "HighlightBackground") continue;
            border.Background = Brushes.Transparent;
            border.BorderBrush = StoneBorderBrush;
        }
    }

    /// <summary>输入框和谐灰 (与页级 TextBox/ComboBox 样式同值; 集中一处便于统一调整)。</summary>
    private static readonly IBrush StoneBorderBrush =
        new SolidColorBrush(Color.Parse("#c8c3b4"));

    private void InjectConfirmDialog()
    {
        // 仅当本视图当前挂在窗口上时才注入 (脱离树的旧视图 GetTopLevel 恒为 null)
        if (TopLevel.GetTopLevel(this) is Window && DataContext is SelectedActionPageViewModel vm)
        {
            vm.ConfirmAsync = ShowConfirmAsync;
            vm.MatchTypesDialogAsync = ShowMatchTypesDialogAsync;
        }
    }

    /// <summary>
    /// 打开「匹配类型」弹窗 (模态), 返回本次新建的类型 id (取消/无新建为 null)。
    /// 关闭后重拉行为目录 —— 弹窗内可能新建/删除了专属行为包, 勾选列表的覆盖集随之变化。
    /// </summary>
    private async Task<string?> ShowMatchTypesDialogAsync()
    {
        if (TopLevel.GetTopLevel(this) is not Window owner || DataContext is not SelectedActionPageViewModel vm)
        {
            return null;
        }
        var dialogVm = new MatchTypesDialogViewModel(vm.Main);
        var win = new MatchTypesDialogWindow { DataContext = dialogVm };
        await win.ShowDialog(owner);
        await vm.ReloadBehaviorCatalogAsync();
        return dialogVm.LastCreatedTypeId;
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

    /// <summary>留桩提示条的「去创建专属行为」(2518): 打开行为库窗口 (阶段二将预填 appliesTo = 本类型; 此处先复用既有入口)。</summary>
    private async void OnCreateDedicatedBehavior(object? sender, RoutedEventArgs e)
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

    /// <summary>类型 toggle 点击: 切卡当前查看类型 (点亮=正在查看; 不改变配置)。</summary>
    private void OnTypeToggleClicked(object? sender, RoutedEventArgs e)
    {
        if (sender is ToggleButton { DataContext: TypeToggleVm toggle })
        {
            toggle.SelectCommand.Execute(null);
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
