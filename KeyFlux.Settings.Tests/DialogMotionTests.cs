using System;
using System.Linq;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Threading;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 弹窗动效 (方案 B「生长」) 守护:
/// ① **每个弹窗窗口**都被挂上动效 —— 反射枚举程序集里全部 <see cref="Window"/> 子类逐一实例化
///    (排除主窗), 断言内容根带挂载标记且起始姿势已落位 ⇒ 将来新增弹窗忘了接统一入口即红;
/// ② 关闭被延后: 退场跑完才真关, 且退场期间禁点;
/// ③ 减少动效档**完全不介入** (不包层、不拦关闭、即刻关);
/// ④ 页内浮层的进/退场与延时隐藏。
/// 注: headless 不驱动过渡时钟 ⇒ 只断言"结构与当帧状态", 观感由实机验证 (与卡片动效同一纪律)。
/// </summary>
[Collection("I18nSerial")]
public sealed class DialogMotionTests
{
    private static void SetReducedMotion(bool on) =>
        Environment.SetEnvironmentVariable("KEYFLUX_NO_MOTION", on ? "1" : null);

    [AvaloniaFact]
    public void Every_Dialog_Window_Gets_Motion_Body()
    {
        var dialogTypes = typeof(App).Assembly.GetTypes()
            .Where(t => !t.IsAbstract && t.IsSubclassOf(typeof(Window)) && t != typeof(MainWindow))
            .OrderBy(t => t.Name)
            .ToList();

        Assert.NotEmpty(dialogTypes);
        foreach (var type in dialogTypes)
        {
            // 弹窗构造签名不一 (OverviewEditWindow 需 (MainViewModel, string)) ⇒ 取最短构造, string 给空串、
            // 其余给 null; 若将来有弹窗在构造里解引用参数, 这里会明确报出是哪个类型
            var ctor = type.GetConstructors().OrderBy(c => c.GetParameters().Length).First();
            var args = ctor.GetParameters()
                .Select(p => p.ParameterType == typeof(string) ? (object?)string.Empty : null)
                .ToArray();
            var window = (Window)ctor.Invoke(args);
            var body = window.Content as Control;

            Assert.True(body?.Classes.Contains(DialogMotion.MotionClass) == true,
                $"{type.Name} 未接弹窗动效: 内容根缺 {DialogMotion.MotionClass} 标记 (漏调 DialogChrome.Apply?)");
            // 起始姿势已落位: 透明 + 非归位的纵向压缩 ⇒ 首帧就是"未长出", 不会先闪一下全尺寸
            Assert.Equal(0, body!.Opacity);
            var m = (body.RenderTransform as ITransform)?.Value ?? Matrix.Identity;
            Assert.NotEqual(1d, m.M22);
        }
    }

    [AvaloniaFact]
    public void Closing_Is_Deferred_Until_Exit_Finishes()
    {
        var window = new QuickSwitchDialogWindow();
        window.Show();
        Dispatcher.UIThread.RunJobs();
        Assert.True(window.IsVisible);

        window.Close();
        Dispatcher.UIThread.RunJobs();

        Assert.True(window.IsVisible, "Closing 未被延后 —— 退场动画会被整段吃掉");
        var body = (Control)window.Content!;
        Assert.False(body.IsHitTestVisible);    // 退场期间不吃点击
        // 退场过渡已装上 (时长 = 该档退场时长)。**不可断言 Opacity==0** —— 过渡模式下属性值是插值中的
        // 当前值 (headless 不推进时钟 ⇒ 读回仍是 1), 这正是"过渡"与"关键帧动画"的差别
        Assert.NotNull(body.Transitions);
        var exit = DialogMotion.ExitDurationOf(body);
        var transform = Assert.Single(body.Transitions!.OfType<TransformOperationsTransition>());
        Assert.Equal(exit, transform.Duration);

        window.Close(); // 收尾: 第二次 Closing 放行 (防测试留窗)
        Dispatcher.UIThread.RunJobs();
    }

    [AvaloniaFact]
    public void Reduced_Motion_Closes_Immediately_And_Leaves_Dialogs_Untouched()
    {
        SetReducedMotion(true);
        try
        {
            Assert.False(MotionPreferences.AnimationsEnabled);

            var window = new QuickSwitchDialogWindow();
            Assert.DoesNotContain(DialogMotion.MotionClass, (window.Content as Control)!.Classes); // 完全不介入

            window.Show();
            Dispatcher.UIThread.RunJobs();
            window.Close();
            Dispatcher.UIThread.RunJobs();

            Assert.False(window.IsVisible, "无动效档必须即刻关闭 (不得有任何延时)");
        }
        finally
        {
            SetReducedMotion(false);
        }
    }

    [AvaloniaFact]
    public void Overlay_Plays_Enter_And_Defers_Exit()
    {
        var host = new Border();

        DialogMotion.SetIsOpen(host, false); // 装载期初值: 直落终态, 不播动画
        Assert.False(host.IsVisible);
        Assert.Equal(0, host.Opacity);

        DialogMotion.SetIsOpen(host, true); // 进场: 可见 + 归位 + 装了过渡
        Assert.True(host.IsVisible);
        Assert.Equal(1, host.Opacity);
        Assert.Equal(1d, ((host.RenderTransform as ITransform)?.Value ?? Matrix.Identity).M22);
        Assert.NotNull(host.Transitions);
        Assert.Single(host.Transitions!.OfType<TransformOperationsTransition>());

        DialogMotion.SetIsOpen(host, false); // 退场: 当帧**不得**隐藏, 且禁点
        Assert.True(host.IsVisible, "浮层退场未延后 —— 退场动画会被吃掉");
        Assert.Equal(0, host.Opacity);
        Assert.False(host.IsHitTestVisible);
    }

    [AvaloniaFact]
    public void Overlay_Reduced_Motion_Hides_Immediately()
    {
        SetReducedMotion(true);
        try
        {
            var host = new Border();
            DialogMotion.SetIsOpen(host, false);
            DialogMotion.SetIsOpen(host, true);
            Assert.True(host.IsVisible);

            DialogMotion.SetIsOpen(host, false);
            Assert.False(host.IsVisible, "无动效档浮层必须即刻隐藏");
        }
        finally
        {
            SetReducedMotion(false);
        }
    }
}
