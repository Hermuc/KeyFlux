using System;
using System.Diagnostics;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using KeyFlux.Settings.Services.Win32;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 插件弹窗「屏幕外开门」守护 (2026-09-23 用户报障: 打开瞬间整窗白帧 + SizeToContent
/// 长高后底部黑帧; v1 的 Opacity 门在本应用软件渲染+重定向表面管线下诱发持久黑块,
/// 三弹窗全中毒, 已废弃)。核心 = 构造期把窗口开到屏幕外 (OpenOffscreen), 内容就绪后
/// RevealWhenRendered 移入屏幕 (表面内容跨移动保留, 移入即完整最终帧)。
/// </summary>
[Collection("I18nSerial")]
public sealed class PluginDialogRevealTests
{
    private static async Task<bool> WaitForAsync(Func<bool> done, int timeoutMs)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < timeoutMs)
        {
            if (done()) return true;
            await Task.Delay(25);
        }
        return done();
    }

    private static void AssertOffscreen(Window dlg)
    {
        Assert.Equal(WindowStartupLocation.Manual, dlg.WindowStartupLocation);
        Assert.True(dlg.Position.X <= DialogPlacer.OffscreenCoord,
            $"窗口应在屏幕外: Position={dlg.Position}");
    }

    [AvaloniaFact]
    public async Task QuickSwitchDialog_Opens_Offscreen_Then_Reveals()
    {
        var dlg = new QuickSwitchDialogWindow();
        AssertOffscreen(dlg);
        dlg.Show();
        Assert.True(await WaitForAsync(
            () => dlg.Position.X > DialogPlacer.OffscreenCoord, 5000), "Opened 后未移入屏幕 (离屏卡死)");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task PluginSettingsDialog_Reveals_Even_Without_DataContext()
    {
        // 无 DataContext (LoadAsync 跳过) —— 移入屏幕必须无条件发生, 防离屏卡死回归
        var dlg = new PluginSettingsDialogWindow();
        AssertOffscreen(dlg);
        dlg.Show();
        Assert.True(await WaitForAsync(
            () => dlg.Position.X > DialogPlacer.OffscreenCoord, 5000), "OnOpened 后未移入屏幕");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task PluginMarketWindow_Opens_Offscreen_Then_Reveals()
    {
        var dlg = new PluginMarketWindow();
        AssertOffscreen(dlg);
        dlg.Show(); // 无 DataContext ⇒ _loadTask=null ⇒ 走 null-gate 快路径
        Assert.True(await WaitForAsync(
            () => dlg.Position.X > DialogPlacer.OffscreenCoord, 5000), "Opened 后未移入屏幕");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task RevealWhenRendered_Cap_Bounds_Offscreen_Time()
    {
        var dlg = new QuickSwitchDialogWindow();
        AssertOffscreen(dlg);
        dlg.Show();
        // 永不完成的 gate: cap 到点必须移入屏幕 (网络差时带加载态显形的契约)
        var sw = Stopwatch.StartNew();
        await DialogPlacer.RevealWhenRendered(dlg, Task.Delay(Timeout.Infinite), capMs: 80);
        Assert.True(sw.ElapsedMilliseconds < 3000, $"cap 未生效: {sw.ElapsedMilliseconds}ms");
        Assert.True(dlg.Position.X > DialogPlacer.OffscreenCoord, "cap 到点仍未移入屏幕");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task RevealWhenRendered_Waits_For_Gate_When_Fast()
    {
        // 用普通 Window 隔离验证 gate 机制 (三个 placer 弹窗会自行在 Opened 显形,
        // 外部 gate 拦不住它们 —— gate 语义只服务带自持加载任务的窗口, 如市场窗)
        var dlg = new Window();
        DialogPlacer.OpenOffscreen(dlg);
        AssertOffscreen(dlg);
        dlg.Show();
        // 200ms 后完成的 gate: 完成前必须保持离屏 (gate 快于 cap 时 honoring)
        var gate = Task.Delay(200);
        var reveal = DialogPlacer.RevealWhenRendered(dlg, gate, capMs: 5000, graceMs: 0);
        await Task.Delay(100);
        Assert.True(dlg.Position.X <= DialogPlacer.OffscreenCoord, "gate 未完成就移入屏幕了");
        await reveal;
        Assert.True(dlg.Position.X > DialogPlacer.OffscreenCoord, "reveal 后未移入屏幕");
        dlg.Close();
    }
}
