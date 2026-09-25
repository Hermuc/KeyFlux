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
/// 插件弹窗「隐身开门」守护 (2026-09-23 用户报障: 打开瞬间整窗白帧 + SizeToContent
/// 长高后底部黑帧)。核心 = 构造期 Opacity=0, 内容就绪后 RevealWhenRendered 显形。
/// 三处 placer 弹窗共用同一机制, 任一处退化为"打开即 Opacity=1"都会复现白/黑帧。
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

    [AvaloniaFact]
    public async Task QuickSwitchDialog_Gated_Then_Revealed()
    {
        var dlg = new QuickSwitchDialogWindow();
        // 构造期闸门: 打开前即隐身
        Assert.Equal(0, dlg.Opacity);
        dlg.Show();
        // Opened 后 RevealWhenRendered 排空渲染再显形
        Assert.True(await WaitForAsync(() => dlg.Opacity == 1, 5000), "Opened 后未显形 (隐身卡死)");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task PluginSettingsDialog_Reveals_Even_Without_DataContext()
    {
        // 无 DataContext (LoadAsync 跳过) —— 显形必须无条件发生, 防隐身卡死回归
        var dlg = new PluginSettingsDialogWindow();
        Assert.Equal(0, dlg.Opacity);
        dlg.Show();
        Assert.True(await WaitForAsync(() => dlg.Opacity == 1, 5000), "OnOpened 后未显形");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task PluginMarketWindow_Gated_Then_Revealed()
    {
        var dlg = new PluginMarketWindow();
        Assert.Equal(0, dlg.Opacity);
        dlg.Show(); // 无 DataContext ⇒ _loadTask=null ⇒ 走 null-gate 快路径
        Assert.True(await WaitForAsync(() => dlg.Opacity == 1, 5000), "Opened 后未显形");
        dlg.Close();
    }

    [AvaloniaFact]
    public async Task RevealWhenRendered_Cap_Bounds_Invisible_Time()
    {
        var w = new Window();
        DialogPlacer.HideUntilRevealed(w);
        w.Show();
        Assert.Equal(0, w.Opacity);
        // 永不完成的 gate: cap 到点必须显形 (网络差时带加载态显形的契约)
        var sw = Stopwatch.StartNew();
        await DialogPlacer.RevealWhenRendered(w, Task.Delay(Timeout.Infinite), capMs: 80);
        Assert.True(sw.ElapsedMilliseconds < 3000, $"cap 未生效: {sw.ElapsedMilliseconds}ms");
        Assert.Equal(1, w.Opacity);
        w.Close();
    }

    [AvaloniaFact]
    public async Task RevealWhenRendered_Waits_For_Gate_When_Fast()
    {
        var w = new Window();
        DialogPlacer.HideUntilRevealed(w);
        w.Show();
        // 200ms 后完成的 gate: 完成前必须保持隐身 (gate 快于 cap 时 honoring)
        var gate = Task.Delay(200);
        var reveal = DialogPlacer.RevealWhenRendered(w, gate, capMs: 5000, graceMs: 0);
        await Task.Delay(100);
        Assert.True(w.Opacity < 1, "gate 未完成就显形了");
        await reveal;
        Assert.Equal(1, w.Opacity);
        w.Close();
    }
}
