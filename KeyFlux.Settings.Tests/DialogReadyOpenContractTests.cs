using System;
using System.Text.Json;
using System.Threading.Tasks;
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
/// <para><b>走过三条路, 前两条已废弃 (本文件锁死它们不再被引入)。</b><br/>
/// ① <b>手动 Measure 定尺寸 (在数据就绪前)</b> —— 在 <c>Show</c> 前 <c>Measure</c> 内容根取
///    <c>DesiredSize</c> 再设死 <c>Height</c>。实测致命: 表单区由
///    <c>IsVisible="{Binding ShowForm}"</c> 驱动, <c>Rows</c> 还空时表单不展开 ⇒ 量出的高度
///    严重偏小, 设死后把表单<b>裁掉</b> (用户报障"显示不完整")。<br/>
/// ② <b>透明期落定</b> —— <c>Opacity=0</c> → <c>ShowDialog</c> → 等几跳 → <c>Opacity=1</c>。
///    真机实测更糟: 全透明期间合成器不绘制内容, 恢复不透明时布局未排空 ⇒ <b>620 高的持续黑块</b>。<br/>
/// ③ <b>先量后显 (现行)</b> —— <c>await LoadAsync()</c> 填好 <c>Rows</c> → <c>Measure</c> 取
///    含表单的真实高度 → 预设 <c>Height</c> (保留 <c>SizeToContent</c>) → <c>ShowDialog</c>。
///    与 ① 的唯一差别是<b>量的时机在数据就绪之后</b>, 这正是 ① 失败的真因。</para>
///
/// <para><b>⚠ 本文件为什么不断言"量的高度够不够" (第三次踩坑的诚实记录)。</b>
/// 独立结构探针 (<c>%TEMP%/kfdeep</c>, 2026-09-24) 实测: <b>headless 文本栈下
/// <c>ItemsControl</c> 根本不存在于视觉树</b> —— 弹窗 Show 前后
/// <c>Icon 探针: IC=无 TB=4~6 TextBox=0</c>, 内容根 <c>DesiredSize</c> 恒为 <c>194x92</c>;
/// 甚至连一棵手工搭建的"裸树" (同结构 Border→StackPanel→ScrollViewer→ItemsControl)
/// 也只量出 <c>520x58</c>、<c>ItemsControl</c> 同样不存在。
/// ⇒ headless 下既无法构造表单树, 也无法验证高度。硬写断言只会得到假绿或假红。
/// <b>最终判据在真机</b> (轨迹探针: 窗口应只出现一条 <c>0x0 → WxH</c> 的 SizeChanged)。</para>
///
/// <para><b>本文件因此只锁三条可测且根因相邻的契约。</b><br/>
/// ① <c>SizeToContent="Height"</c> 必须保留、<c>Height</c> 不得被构造期设死;
/// ② 打开流程结束后窗口级不透明必须恢复;
/// ③ <c>PrepareForShowBeforeShowAsync</c> 必须存在 (作为"首帧即终帧"锚定的唯一入口)。</para>
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
    /// 「首帧即终帧」契约: 显示前必须把窗口锚定到 <c>MaxHeight</c> 并摘掉
    /// <c>SizeToContent</c> —— 这样首帧尺寸与最终尺寸一致, 不存在"新暴露区域" (黑块)。
    /// </summary>
    [AvaloniaFact]
    public async Task Prepare_Anchors_Size_Before_Show()
    {
        var win = Build();
        try
        {
            var ok = await win.PrepareForShowBeforeShowAsync();

            Assert.True(ok, "有 MaxHeight 的弹窗必须锚定尺寸");
            Assert.Equal(620d, win.Height);
            Assert.Equal(SizeToContent.Manual, win.SizeToContent);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 锚定不依赖数据是否就绪 —— 现行方案不量内容高, 所以后端失败/空声明时同样锚定
    /// (否则会退回"以 MaxHeight 上屏再塌缩"的黑块路径)。
    /// </summary>
    [AvaloniaFact]
    public async Task Prepare_Anchors_Even_When_Data_Not_Ready()
    {
        var win = Build();
        try
        {
            win.SetLoadStepForProbe(() => Task.CompletedTask); // Rows 保持为空

            var ok = await win.PrepareForShowBeforeShowAsync();

            var vm = (PluginSettingsDialogViewModel)win.DataContext!;
            Assert.Empty(vm.Rows);
            Assert.True(ok, "无数据时也必须锚定 (黑块与数据无关)");
            Assert.Equal(620d, win.Height);
            Assert.Equal(SizeToContent.Manual, win.SizeToContent);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 入场结束后必须把 <c>SizeToContent</c> 还给 <c>Height</c>, 让 620 的锚定高度收回到
    /// 内容真实高度 —— 否则短表单底部会长期留一大块空白。
    /// </summary>
    [AvaloniaFact]
    public async Task SizeToContent_Returns_After_Enter_Motion()
    {
        var win = Build();
        try
        {
            await win.PrepareForShowBeforeShowAsync();
            Assert.Equal(SizeToContent.Manual, win.SizeToContent);

            win.Show();
            // 入场 360ms + 收尾 60ms; headless 下 DispatcherTimer 不受 ForceRenderTimerTick
            // 驱动, 故用真实等待。
            Thread.Sleep(600);
            Dispatcher.UIThread.RunJobs();

            Assert.Equal(SizeToContent.Height, win.SizeToContent);
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
