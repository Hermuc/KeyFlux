using System;
using System.Linq;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 弹窗首帧/尾帧观感契约 (2026-09-24 用户报障"打开时文字闪一下" + "内容先消失、窗口后关闭")。
///
/// <para><b>Bug ① 闪一下 —— 起始姿势必须早于首帧渲染。</b>实测: <c>Window.ShowCore</c> 先
/// <c>IsVisible = true</c>, 之后才由 <c>IsVisibleChanged</c> 触发 <c>Opened</c> ⇒
/// <b>Opened 触发时窗口已经以"不透明、原尺寸"渲染过至少一帧</b> (探针读回: Opened 进入时
/// <c>body.Opacity == 1</c>)。若在 Opened 里才设 <c>Opacity = 0</c>, 那一帧完整内容已被看见。
/// 故起始姿势必须在 <c>Show()</c> <b>之前</b>落好 —— 由 <see cref="DialogMotion.Attach"/>
/// 在构造期完成 (此时 Content 已由 XAML 就位、VisualRoot 为 null, 设的只是本地值)。</para>
///
/// <para><b>Bug ② 内容先消失 —— 退场过渡缺 Opacity 项。</b>早期退场调
/// <c>PoseTransitions(DialogExit, easing)</c> 漏传 <c>includeOpacity</c> (默认 false) ⇒
/// 过渡只含 <c>RenderTransform</c>, 而 <c>Opacity = 0</c> <b>瞬时生效</b>: 内容"啪"地消失,
/// 缩放却还要跑 200ms, 再加 40ms pad 才关窗 ⇒ 用户看到一段空窗口停留 ("滞后一下")。
/// 本文件锁死"退场过渡必须同时含 Transform 与 Opacity 两项, 且时长一致"。</para>
/// </summary>
[Collection("I18nSerial")]
public sealed class DialogFirstLastFrameContractTests
{
    /// <summary>Bug ① 回归: 构造完成 (Show 之前) 起始姿势必须已就位。</summary>
    [AvaloniaFact]
    public void Enter_Pose_Is_Set_Before_Window_Is_Shown()
    {
        var win = new MatchTypesDialogWindow();
        try
        {
            var body = (Control)win.Content!;

            // Show 之前: 透明 + 有缩放姿势 + 已被接管
            Assert.Equal(0d, body.Opacity);
            Assert.NotNull(body.RenderTransform);
            Assert.Contains(DialogMotion.MotionClass, body.Classes);

            // 窗口真正打开时姿势仍是起点 ⇒ 首帧渲染的就是"透明的小窗", 不可能闪
            var opacityAtOpened = double.NaN;
            win.Opened += (_, _) => opacityAtOpened = body.Opacity;
            win.Show();

            Assert.Equal(0d, opacityAtOpened);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>Bug ② 回归: 退场过渡必须同时含 Transform 与 Opacity, 且时长一致。</summary>
    [AvaloniaFact]
    public void Exit_Transition_Fades_Opacity_Alongside_Transform()
    {
        var win = new Window { Width = 300, Height = 200, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Pump();

        try
        {
            win.Close();
            Pump();

            var body = (Control)win.Content!;
            var transform = body.Transitions?.OfType<TransformOperationsTransition>().SingleOrDefault();
            var opacity = body.Transitions?.OfType<DoubleTransition>().SingleOrDefault();

            Assert.NotNull(transform);
            Assert.NotNull(opacity); // ← 漏传 includeOpacity 时这里立刻红
            Assert.Equal(ClaudeMotion.DialogExit, transform!.Duration);
            Assert.Equal(transform.Duration, opacity!.Duration); // 两段必须同步走完, 否则又会出现"内容先没"
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>入场过渡也必须含 Opacity (否则入场同样是"瞬现")。</summary>
    [AvaloniaFact]
    public void Enter_Transition_Also_Fades_Opacity()
    {
        var win = new Window { Width = 300, Height = 200, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Pump();

        try
        {
            var body = (Control)win.Content!;
            Assert.NotNull(body.Transitions?.OfType<TransformOperationsTransition>().SingleOrDefault());
            Assert.NotNull(body.Transitions?.OfType<DoubleTransition>().SingleOrDefault());
        }
        finally
        {
            win.Close();
        }
    }

    private static void Pump()
    {
        Dispatcher.UIThread.RunJobs();
        AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        Dispatcher.UIThread.RunJobs();
        Thread.Sleep(20);
        Dispatcher.UIThread.RunJobs();
    }
}
