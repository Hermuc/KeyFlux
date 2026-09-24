using System;
using System.Collections.Generic;
using System.Text.Json;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 「显示前把尺寸落定」契约 (2026-09-24 用户报障"打开弹窗瞬间大片黑块" + "显示不完整")。
///
/// <para><b>背景。</b><see cref="PluginSettingsDialogWindow"/> 是
/// <c>SizeToContent="Height"</c> + <c>MaxHeight="620"</c>, 设置项要等一次后端往返才知道几行。
/// 窗口以某个尺寸上屏后, 内容变化会让 <c>SizeToContent</c> 调 <c>SetWindowPos</c> 改尺寸,
/// 新暴露区域在首帧绘制完成前是未绘制的窗口表面 (观感即黑块)。</para>
///
/// <para><b>走过两条路, 第一条已废弃 (本文件锁死它不再被引入)。</b><br/>
/// ① <b>手动 Measure 定尺寸</b> —— 在 <c>Show</c> 前 <c>Measure/Arrange</c> 内容根取
///    <c>DesiredSize</c> 再设死 <c>Height</c>。实测致命: 表单区由
///    <c>IsVisible="{Binding ShowForm}"</c> 驱动, <c>Show</c> 前绑定尚未生效
///    (<c>ScrollViewer.IsVisible=False</c>、<c>ItemsControl</c> 不存在) ⇒ 量出的高度严重偏小,
///    设死后把表单<b>裁掉</b> (用户报障"显示不完整": 取消/保存浮在输入框上且被底边切掉)。<br/>
/// ② <b>透明期落定 (现行)</b> —— <c>Opacity=0</c> → <c>ShowDialog</c> (尺寸在此期间落定)
///    → 等布局排空 → <c>CenterToOwner</c> → <c>Opacity=1</c>。</para>
///
/// <para><b>本文件锁什么。</b>headless 下表单 <c>IsVisible</c> 链路不张开, 无法断言"最终高度
/// 是否容纳表单" (那是真机判据)。因此这里锁定<b>可测且是根因相邻项</b>的两条:
/// ① <c>SettleHeightBeforeShow</c> 这类"设死 Height + 清 SizeToContent"的写法不得复活;
/// ② 打开流程必须恢复窗口级不透明 (不能因引入透明期而把弹窗永久留成透明)。</para>
/// </summary>
[Collection("I18nSerial")]
public sealed class DialogReadyOpenContractTests
{
    private static string RepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException("找不到仓库根, BaseDirectory=" + AppContext.BaseDirectory);
    }

    private static PluginManifest EverythingManifest() =>
        JsonSerializer.Deserialize<PluginManifest>(
            File.ReadAllText(Path.Combine(RepoRoot(), "plugins", "examples", "everything_search", "plugin.json")),
            new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;

    private static PluginSettingsDialogWindow Build()
    {
        var win = new PluginSettingsDialogWindow
        {
            DataContext = new PluginSettingsDialogViewModel(
                new MainViewModel(new BackendSessionOptions()), EverythingManifest()),
        };
        if (win.DataContext is PluginSettingsDialogViewModel vm)
        {
            vm.IsLoading = false;
        }
        return win;
    }

    /// <summary>
    /// 反例守卫: 窗口必须保留 <c>SizeToContent="Height"</c>。
    ///
    /// <para>若有人再次引入"设死 Height + 清 SizeToContent"的写法, 这里立刻红 —— 那条路
    /// 已由用户报障证明会裁掉表单 ("显示不完整")。</para>
    /// </summary>
    [AvaloniaFact]
    public void Window_Keeps_SizeToContent_Height()
    {
        var win = Build();
        try
        {
            Assert.Equal(SizeToContent.Height, win.SizeToContent);
            Assert.True(double.IsNaN(win.Height), "Height 不应被预先设死 (会裁掉异步展开的表单)");
            Assert.Equal(620d, win.MaxHeight);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 打开流程结束后窗口必须是不透明的 —— 透明期只用于尺寸落定, 不得残留。
    ///
    /// <para>这条防的是"引入窗口级透明来消黑块"时忘了恢复 (弹窗永久透明 / 内容看不见)。</para>
    /// </summary>
    [AvaloniaFact]
    public void Open_Flow_Restores_Window_Opacity()
    {
        var win = Build();
        try
        {
            win.Show();
            Pump();
            Assert.Equal(1d, win.Opacity);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 内容根在构造期就落好入场姿势 (Opacity=0 + 有变换) —— 这是"首帧不以不透明渲染"的前提,
    /// 直接对应"打开时文字不该先出现再闪"。
    /// </summary>
    [AvaloniaFact]
    public void Content_Pose_Is_Posed_Before_Show()
    {
        var win = Build();
        try
        {
            var body = (Control)win.Content!;
            Assert.Equal(0d, body.Opacity);
            Assert.NotNull(body.RenderTransform);
            Assert.Contains(DialogMotion.MotionClass, body.Classes);

            // Opened 时仍是起点 ⇒ 首帧渲染的就是透明内容
            var atOpened = double.NaN;
            win.Opened += (_, _) => atOpened = body.Opacity;
            win.Show();
            Assert.Equal(0d, atOpened);
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
