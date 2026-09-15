using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 匹配类型弹窗的自适应布局守护 (2026-09-15 "表单过于拥挤" 改造):
/// 主从两列在**最小窗口尺寸**下不重叠, 右列 (表单/详情) 保持最小可用宽度;
/// 窗口放大时右列随之变宽 (弹性列宽而非固定 340px);
/// 表单的常驻操作栏与滚动区不重叠 (按钮始终可见, 内容不被截断)。
/// </summary>
[Collection("I18nSerial")]
public sealed class MatchTypesLayoutTests
{
    private static (MatchTypesDialogWindow Window, MatchTypesPageViewModel Vm) Create(double width, double height)
    {
        var main = new MainViewModel(new BackendSessionOptions())
        {
            Config = new Config { Options = new Options() },
        };
        var vm = new MatchTypesPageViewModel(main);
        var win = new MatchTypesDialogWindow { DataContext = vm, Width = width, Height = height };
        win.Show();
        Dispatcher.UIThread.RunJobs();
        return (win, vm);
    }

    [AvaloniaFact]
    public void Two_Panes_Do_Not_Overlap_At_Minimum_Window_Size()
    {
        // 与 MatchTypesDialogWindow 的 MinWidth/MinHeight 一致
        var (win, _) = Create(840, 560);
        try
        {
            var view = Assert.IsType<MatchTypesPageView>(win.Content);
            var list = view.FindControl<Border>("RowListCard");
            var detail = view.FindControl<Border>("DetailCard");
            Assert.NotNull(list);
            Assert.NotNull(detail);

            Assert.True(list!.Bounds.Width > 0 && detail!.Bounds.Width > 0, "两列都应完成布局");
            // 左列右边界不越过右列左边界 (无重叠)
            Assert.True(list.Bounds.Right <= detail.Bounds.Left + 0.5,
                $"左右两列重叠: 左列 Right={list.Bounds.Right}, 右列 Left={detail.Bounds.Left}");
            // 右列 (表单区) 最小可用宽度
            Assert.True(detail.Bounds.Width >= 360,
                $"右列过窄 ({detail.Bounds.Width}px), 表单会被挤压");
        }
        finally
        {
            win.Close();
        }
    }

    [AvaloniaFact]
    public void Right_Pane_Grows_With_The_Window()
    {
        var (small, _) = Create(840, 560);
        double smallWidth;
        try
        {
            var view = Assert.IsType<MatchTypesPageView>(small.Content);
            smallWidth = view.FindControl<Border>("DetailCard")!.Bounds.Width;
        }
        finally
        {
            small.Close();
        }

        var (large, _) = Create(1280, 820);
        try
        {
            var view = Assert.IsType<MatchTypesPageView>(large.Content);
            var largeWidth = view.FindControl<Border>("DetailCard")!.Bounds.Width;
            Assert.True(largeWidth > smallWidth + 100,
                $"窗口放大后右列未变宽 (small={smallWidth}, large={largeWidth}) —— 列宽应弹性分配");
            // 左列有上限 (不随窗口无限变宽), 保证主从比例稳定
            var leftWidth = view.FindControl<Border>("RowListCard")!.Bounds.Width;
            Assert.True(leftWidth <= 340.5, $"左列超出上限 ({leftWidth}px)");
        }
        finally
        {
            large.Close();
        }
    }

    [AvaloniaFact]
    public void Form_Action_Bar_Stays_Below_The_Scrolling_Area()
    {
        var (win, vm) = Create(840, 560);
        try
        {
            vm.OpenCreateCommand.Execute(null);
            Dispatcher.UIThread.RunJobs();

            var view = Assert.IsType<MatchTypesPageView>(win.Content);
            var scroll = view.FindControl<ScrollViewer>("FormScroll");
            var actions = view.FindControl<StackPanel>("FormActions");
            Assert.NotNull(scroll);
            Assert.NotNull(actions);

            // 常驻操作栏在滚动区下方, 互不重叠 ⇒ 保存按钮始终可见、表单内容不被按钮压住
            Assert.True(scroll!.Bounds.Bottom <= actions!.Bounds.Top + 0.5,
                $"操作栏与表单滚动区重叠: scroll.Bottom={scroll.Bounds.Bottom}, actions.Top={actions.Bounds.Top}");
            Assert.True(actions.Bounds.Height > 0, "操作栏应可见 (常驻)");
        }
        finally
        {
            win.Close();
        }
    }
}
