using System;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 页内浮层动效主体契约 (2026-09-24 用户报障"单击卡片直接弹出、没有动画"回归守卫)。
///
/// <para><b>根因回顾。</b>浮层在 XAML 里是三明治: 最外层<b>透明输入拦截 Border</b> 挂附加属性,
/// 内层数据模板根才是<b>真正可见</b>的弹层。早期实现把宿主当动效主体 ⇒ 弹簧缩放/淡入全作用在
/// 透明容器上, 动效"在跑"但用户完全看不到 (探针实测宿主 Opacity/Transform/Transitions 均被改写,
/// 内层毫无变化)。</para>
///
/// <para><b>锁什么。</b>① 动效主体必须是标了 <c>dlgPanel</c> 的那层可见容器, 而不是宿主;
/// ② 宿主自身不得被改姿势 (改了就是回到了老 bug); ③ 主体定位必须发生在模板展开之后
/// (同步解析会因后代为 0 而回退到宿主 —— 这是修复过程中的第二个坑)。</para>
/// </summary>
[Collection("I18nSerial")]
public sealed class OverlayMotionContractTests
{
    private static (SelectedActionPageViewModel Page, SelectedActionPageView View, Window Window) CreateHost()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            FileGroups = [],
            SelectedAction = new SelectedAction { Mappings = [] },
        };
        var page = new SelectedActionPageViewModel(main);
        var view = new SelectedActionPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        Pump();
        return (page, view, window);
    }

    /// <summary>动效主体 = 内层可见弹层, 不是透明宿主。</summary>
    [AvaloniaFact]
    public void Overlay_Body_Is_The_Visible_Panel_Not_The_Transparent_Host()
    {
        var (page, view, window) = CreateHost();

        var host = view.GetVisualDescendants()
            .OfType<Control>()
            .First(c => DialogMotion.GetOverlayMotion(c));

        page.OpenAddPanelCommand.Execute(null);
        Pump();

        var panel = host.GetVisualDescendants()
            .OfType<Control>()
            .FirstOrDefault(c => c.Classes.Contains(DialogMotion.PanelClass));

        Assert.NotNull(panel); // XAML 里必须给可见弹层标 dlgPanel

        // ① 主体拿到了动效
        Assert.Contains(DialogMotion.MotionClass, panel!.Classes);
        Assert.NotNull(panel.Transitions);
        Assert.NotNull(panel.RenderTransform);

        // ② 宿主 (透明层) 自身不得被动效改写 —— 改写即回到"用户看不到动画"的老 bug
        Assert.DoesNotContain(DialogMotion.MotionClass, host.Classes);
        Assert.Null(host.Transitions);
        Assert.Null(host.RenderTransform);

        window.Close();
    }

    /// <summary>重复开合: 每次入场都重新解析主体 (AddPanel 每次是新 VM ⇒ 新模板 ⇒ 新 Border)。</summary>
    [AvaloniaFact]
    public void Overlay_ReResolves_Body_On_Each_Open()
    {
        var (page, view, window) = CreateHost();
        var host = view.GetVisualDescendants()
            .OfType<Control>()
            .First(c => DialogMotion.GetOverlayMotion(c));

        page.OpenAddPanelCommand.Execute(null);
        Pump();
        var first = host.GetVisualDescendants().OfType<Control>()
            .First(c => c.Classes.Contains(DialogMotion.PanelClass));
        Assert.Contains(DialogMotion.MotionClass, first.Classes);

        page.CloseAddPanelCommand.Execute(null);
        Pump();
        page.OpenAddPanelCommand.Execute(null);
        Pump();

        var second = host.GetVisualDescendants().OfType<Control>()
            .First(c => c.Classes.Contains(DialogMotion.PanelClass));
        Assert.Contains(DialogMotion.MotionClass, second.Classes);
        Assert.NotNull(second.Transitions);

        window.Close();
    }

    private static void Pump()
    {
        Dispatcher.UIThread.RunJobs();
        AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        Dispatcher.UIThread.RunJobs();
        Thread.Sleep(20);
        Dispatcher.UIThread.RunJobs();
        AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        Dispatcher.UIThread.RunJobs();
    }
}
