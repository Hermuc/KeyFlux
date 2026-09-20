using System;
using System.Linq;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 设置面板动效守护 (2026-09-13 动效批次):
/// ① 设置页根类 .motion 与系统动效偏好一致 —— 用户关闭「动画效果」时入场级联/分区展开
///    整体跳过的唯一开关 (XAML 动画选择器均以 .motion 开头);
/// ② 8 个分区体挂 sectionBody 类 (open 动画的锚点), 手风琴展开时 open 类与 IsVisible 同步;
/// ③ 主窗页面切换宿主 = TransitioningContentControl + 皮肤令牌时长的 CrossFade。
/// 注: 动画本身不在此断言 (headless 时钟推进不确定), 只锁结构与接线; 观感由实机验证。
/// </summary>
[Collection("I18nSerial")]
public sealed class MotionSmokeTests
{
    [AvaloniaFact]
    public void SettingsPage_Motion_Gate_And_Section_Bodies_Are_Wired()
    {
        // 闸门默认开; KEYFLUX_NO_MOTION=1 时 .motion 不挂 → 页内动画选择器全部失配
        Assert.True(MotionPreferences.AnimationsEnabled, "默认应启用动效 (未设 KEYFLUX_NO_MOTION)");
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var vm = new SettingsPageViewModel(main);
        var view = new SettingsPageView { DataContext = vm };
        Assert.Contains("motion", view.Classes);

        Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", "1");
        try
        {
            Assert.False(MotionPreferences.AnimationsEnabled);
            Assert.DoesNotContain("motion", new SettingsPageView().Classes);
        }
        finally
        {
            Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", null);
        }

        var window = new Window { Width = 1500, Height = 950, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            var bodies = view.GetVisualDescendants().OfType<StackPanel>()
                .Where(p => p.Classes.Contains("sectionBody")).ToList();
            // 9 = 语言/程序分组(无模板)/自定义热键/鼠标参数/键盘布局/触发延时/命令框皮肤
            //     /窗口毛玻璃/命令框字体/路径变量 —— 「程序分组」卡是 Click 事件而非 sectionBody,
            //     由 2026-09-20 新增「命令框字体」卡从 8 增至 9 (漏改会让本用例变红, 正是其价值)。
            Assert.Equal(9, bodies.Count);

            // 手风琴展开: open 类与 IsVisible 必须绑定同一 Show* 源, 展开动画才有触发时机
            vm.ToggleSectionCommand.Execute("mouse");
            Dispatcher.UIThread.RunJobs();
            Assert.True(vm.ShowMouseOption);
            var opened = bodies.FirstOrDefault(b => b.Classes.Contains("open"));
            Assert.NotNull(opened);
            Assert.True(opened!.IsVisible, "open 类已挂但分区体不可见: Classes.open 与 IsVisible 绑定源不同步");
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public void MainWindow_Page_Switch_Uses_CrossFade_With_Skin_Token()
    {
        var main = new MainViewModel(new BackendSessionOptions())
        {
            Config = new Config { Options = new Options() },
        };
        var window = new MainWindow(main);
        // 只构造不 Show (Show 会走 Opened -> InitializeAsync 拉起后端子进程, 见 SkinContractTests);
        // TransitioningContentControl 是 XAML 直接子元素, 构造期已挂在 Content 视觉子树上,
        // 从 Content 根遍历即可, 无需窗口模板应用
        var contentRoot = (Visual)window.Content!;
        var host = contentRoot.GetVisualDescendants().OfType<TransitioningContentControl>().First();
        var fade = Assert.IsType<CrossFade>(host.PageTransition);
        // 时长必须来自皮肤令牌 ClaudeMotion.Standard (换肤整组调动效的契约), 而非散落字面量
        Assert.Equal(ClaudeMotion.Standard, fade.Duration);
    }
}
