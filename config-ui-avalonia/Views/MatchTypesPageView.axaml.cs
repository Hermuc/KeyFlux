using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Media;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 匹配类型页视图 (左侧导航「匹配类型」)。
/// 视图职责: 打开/关闭编辑面板、行内编辑与删除转发、删除确认对话框 (需 Window owner)。
/// 业务逻辑全部在 <see cref="MatchTypesPageViewModel"/>。
/// </summary>
public partial class MatchTypesPageView : UserControl
{
    public MatchTypesPageView()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    /// <summary>每次导航进入时重拉行为目录 (专属行为数依赖它; DataTemplate 会重建视图)。</summary>
    private async void OnLoaded(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not MatchTypesPageViewModel vm) return;
        vm.ConfirmAsync = ShowConfirmAsync;
        try { await vm.ReloadAsync(); }
        catch { /* 后端未就绪: 保持列表现状, 由页面状态提示兜底 */ }
    }

    /// <summary>「新建匹配类型」(2522)。</summary>
    private async void OnCreate(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MatchTypesPageViewModel vm) await vm.OpenCreateCommand.ExecuteAsync(null);
    }

    /// <summary>底部「编辑」(2546): 对当前选中行打开面板, 标识锁定 (仅自定义类型可点)。</summary>
    private void OnEditSelected(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MatchTypesPageViewModel { SelectedRow: { } row } vm)
        {
            vm.OpenEditCommand.Execute(row);
        }
    }

    /// <summary>底部「删除」(2536): 确认后落配置并级联删除同名专属行为包 (仅自定义类型可点)。</summary>
    private async void OnDeleteSelected(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MatchTypesPageViewModel vm)
        {
            await vm.AskRemoveAsync(vm.SelectedRow);
        }
    }

    /// <summary>
    /// 列表行双击 = 主操作 (与底部按钮同义, 提升便捷性):
    /// 自定义类型 → 编辑; 未配置行为的类型 → 配置行为; 内置类型无操作。
    /// </summary>
    private async void OnRowDoubleTapped(object? sender, TappedEventArgs e)
    {
        if (DataContext is not MatchTypesPageViewModel vm) return;
        if (vm.CanEditSelected)
        {
            vm.OpenEditCommand.Execute(vm.SelectedRow);
            return;
        }
        if (vm.CanSetAction) await OpenBehaviorLibraryAsync();
    }

    /// <summary>「常用类型」胶囊: 一键填好名称 / 匹配条件 / 条件或扩展名 / 默认动作。</summary>
    private void OnApplyPreset(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MatchTypesPageViewModel { Editor: { } editor } &&
            sender is Button { DataContext: MatchTypePreset preset })
        {
            editor.ApplyPresetCommand.Execute(preset);
        }
    }

    /// <summary>底部「配置行为」(2569): 打开行为库窗口为该类型建专属行为 (仅"尚未配置行为"的自定义类型可点)。</summary>
    private async void OnSetAction(object? sender, RoutedEventArgs e) => await OpenBehaviorLibraryAsync();

    /// <summary>
    /// 打开行为库窗口 (底部「配置行为」与列表行双击共用); 关窗后重拉行为目录 ——
    /// 列表的「已配置行为 N / 未配置行为」随之刷新。
    /// </summary>
    private async Task OpenBehaviorLibraryAsync()
    {
        if (TopLevel.GetTopLevel(this) is not Window owner || DataContext is not MatchTypesPageViewModel vm)
        {
            return;
        }
        var win = new BehaviorLibraryWindow { DataContext = new BehaviorLibraryViewModel(vm.Main) };
        await win.ShowDialog(owner);
        await vm.ReloadAsync();
    }

    /// <summary>「添加规则」(2527)。</summary>
    private void OnAddRule(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MatchTypesPageViewModel { Editor: { } editor })
        {
            editor.AddRuleCommand.Execute(null);
        }
    }

    /// <summary>规则行 ✕ (至少保留一条)。</summary>
    private void OnRemoveRule(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MatchTypesPageViewModel { Editor: { } editor } && sender is Button { DataContext: MatchRuleRowVm row })
        {
            editor.RemoveRuleCommand.Execute(row);
        }
    }

    /// <summary>两按钮确认对话框 (与选中动作页同款: OK 为危险色, 取消为白色)。</summary>
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
            Content = I18n.T("970"),
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
