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
/// 弹窗尺寸稳定性契约 (2026-09-24 用户报障"打开弹窗一瞬间出现大片未绘制黑块")。
///
/// <para><b>根因。</b><see cref="PluginSettingsDialogWindow"/> 是
/// <c>SizeToContent="Height"</c> + <c>MaxHeight="620"</c>, 设置项要等一次后端往返才知道有几行。
/// 早期实现是"先 <c>ShowDialog</c> 再在 <c>Opened</c> 里 <c>LoadAsync</c>" ⇒
/// 窗口先以 <b>620</b> 高上屏 (SizeToContent 按 MaxHeight 取值), 布局落定后塌缩到
/// 真实高度 (Everything 搜索 = <b>108</b>)。窗口尺寸一变, 新暴露区域在首帧绘制完成前是
/// 未绘制的窗口表面 (观感即黑块); <c>DialogPlacer</c> 再按新尺寸挪位, 叠加成"跳一下"。</para>
///
/// <para><b>修法。</b>显示前两步走完: ① 预加载 (<c>LoadAsync</c> 不碰视觉树);
/// ② 预测量 (<c>Measure</c>/<c>Arrange</c> 内容根取真实 <c>DesiredSize</c>, 据此设
/// <c>Height</c> 并清 <c>SizeToContent</c>)。窗口首次可见即终态尺寸, 全程不触发尺寸变更。</para>
///
/// <para>本文件锁死"首次可见尺寸 == 最终尺寸"这一根因级不变量, 而不是只断言"能看到窗口"
/// —— 后者在尺寸跳变的情况下同样会通过。</para>
/// </summary>
[Collection("I18nSerial")]
public sealed class DialogSizeStabilityTests
{
    private static string RepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException("找不到仓库根, BaseDirectory=" + AppContext.BaseDirectory);
    }

    private static PluginManifest EverythingManifest()
    {
        var p = Path.Combine(RepoRoot(), "plugins", "examples", "everything_search", "plugin.json");
        return JsonSerializer.Deserialize<PluginManifest>(
            File.ReadAllText(p), new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
    }

    private static PluginSettingsDialogWindow BuildWithRowsFilled()
    {
        var win = new PluginSettingsDialogWindow
        {
            DataContext = new PluginSettingsDialogViewModel(
                new MainViewModel(new BackendSessionOptions()), EverythingManifest()),
        };
        if (win.DataContext is PluginSettingsDialogViewModel vm)
        {
            foreach (var s in EverythingManifest().Settings!)
            {
                vm.Rows.Add(new PluginSettingRowVm(s, s.Default ?? ""));
            }
            vm.IsLoading = false;
        }
        return win;
    }

    /// <summary>
    /// 根因回归: 走"显示前定尺寸"后, <c>Show</c> 那一刻的窗口尺寸必须已等于最终尺寸,
    /// 且之后不再发生任何尺寸变更。
    ///
    /// <para>反例守卫是本测试的重点: 断言 <b>SizeChanged 事件不得出现"尺寸变化"</b>
    /// (首次建立窗口表面如实测为 <c>0,0 -&gt; W,H</c> 那一次不算变化, 其余任何条目都是跳变)。
    /// 修法回退时, 首帧会是 620 而最终 108, SizeChanged 会多出 620-&gt;108 一条 ⇒ 立刻红。</para>
    /// </summary>
    [AvaloniaFact]
    public void Show_FirstFrame_Is_Already_Final_Size()
    {
        var win = BuildWithRowsFilled();
        var changes = new List<Size>();

        try
        {
            win.SettleHeightBeforeShowForProbe();
            var sizeAfterSettle = new Size(win.Width, win.Height);

            win.SizeChanged += (_, e) => changes.Add(e.NewSize);

            var sizeAtShow = default(Size);
            win.Opened += (_, _) => sizeAtShow = win.Bounds.Size;
            win.Show();
            Pump();

            var sizeAtRest = win.Bounds.Size;

            Assert.Equal(520d, sizeAtShow.Width);
            Assert.Equal(sizeAtRest.Height, sizeAtShow.Height);
            Assert.Equal(sizeAfterSettle.Height, sizeAtRest.Height);

            // 首次建立窗口表面 = 0,0 -> W,H 一条; 再有任何一条都是尺寸跳变
            Assert.True(
                changes.Count <= 1,
                $"窗口尺寸在显示后发生了 {changes.Count} 次变化 (应 ≤1): "
                    + string.Join(", ", changes.ConvertAll(s => $"{s.Width}x{s.Height}")));
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>定尺寸必须清掉 <c>SizeToContent</c> —— 否则内容再变又会走"改尺寸 ⇒ 新区域未绘制"的老路。</summary>
    [AvaloniaFact]
    public void Settle_Height_Clears_SizeToContent()
    {
        var win = BuildWithRowsFilled();
        try
        {
            Assert.Equal(SizeToContent.Height, win.SizeToContent);

            win.SettleHeightBeforeShowForProbe();

            Assert.Equal(SizeToContent.Manual, win.SizeToContent);
            Assert.False(double.IsNaN(win.Height), "Height 必须已固化为具体值");
            Assert.True(win.Height > 0, $"Height 应为正数, 实际 {win.Height}");
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary><c>MaxHeight</c> 仍是封顶: 内容超高时窗口不得突破 620 (超出部分由 ScrollViewer 滚动)。</summary>
    [AvaloniaFact]
    public void Settle_Height_Respects_MaxHeight()
    {
        var win = BuildWithRowsFilled();
        try
        {
            win.SettleHeightBeforeShowForProbe();
            Assert.True(win.Height <= 620, $"窗口高度不得突破 MaxHeight=620, 实际 {win.Height}");
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
