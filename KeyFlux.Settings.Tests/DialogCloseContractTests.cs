using System;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using KeyFlux.Settings.Services;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 弹窗关闭契约 (2026-09-24 用户报障"关闭按钮要按两次"回归守卫)。
///
/// <para><b>锁什么。</b>① 一次 <c>Close()</c> + 退场延时到期 = 窗口真的关掉 (不许要第二次);
/// ② 退场中的重复 <c>Close()</c> 被拦下且不重复编排; ③ 放行标志与"退场中"标志互不串台
/// (这是根因: 早期用 <c>Closing=false</c> 表示放行, 恰好使 <c>if (Closing) return</c>
/// 守卫失效 ⇒ 放行触发的重入又被完整拦截一次)。</para>
///
/// <para><b>⚠ 为什么用 <see cref="DialogMotion.ForceExitDelayElapsed"/> 而不是真等。</b>
/// 退场由 <c>DispatcherTimer</c> 驱动, headless 宿主不推进真实时钟 (探针实测:
/// <c>ForceRenderTimerTick</c> 不驱动 dispatcher 计时器, 延时回调永不执行,
/// <c>Closing</c> 全程只触发 1 次)。真等只能得到假失败, 故走测试缝调用回调体的等价逻辑。</para>
/// </summary>
[Collection("I18nSerial")]
public sealed class DialogCloseContractTests
{
    [AvaloniaFact]
    public void Single_Close_Plus_Exit_Really_Closes()
    {
        var win = new Window { Width = 200, Height = 150, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Pump();

        var closingCount = 0;
        win.Closing += (_, _) => closingCount++;

        win.Close(); // 第一次 (也是唯一一次) 用户点关闭
        Pump();

        // 退场中: 窗口还在, 且被拦
        Assert.True(win.IsVisible, "退场期间窗口不该立刻消失 (那会让退场动画没机会播)");
        Assert.True(DialogMotion.IsClosing(win));

        // 退场延时到期 → 真关
        DialogMotion.ForceExitDelayElapsed(win);
        Pump();

        Assert.False(win.IsVisible, "退场延时到期后窗口必须真的关闭 (不许要用户再点一次)");
        Assert.False(DialogMotion.IsClosing(win));
        Assert.Equal(2, closingCount); // ① 用户点的那次 (被拦) + ② 放行的真关
    }

    /// <summary>放行那次不得再被拦截 (根因回归)。</summary>
    [AvaloniaFact]
    public void Forced_Close_Is_Not_Intercepted_Again()
    {
        var win = new Window { Width = 200, Height = 150, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Pump();

        win.Close();
        Pump();
        Assert.True(DialogMotion.IsClosing(win));
        Assert.False(DialogMotion.IsForced(win));

        DialogMotion.ForceExitDelayElapsed(win);
        Pump();

        // 关键: 放行后 Closing 不得重新变 true (早期实现会因重入拦截而停在 true)
        Assert.False(DialogMotion.IsClosing(win), "放行的真关不得被重新拦截 (这正是'要按两次'的根因)");
        Assert.False(win.IsVisible);
    }

    /// <summary>退场中重复 Close(): 拦下但不重复编排 (计时器不叠加)。</summary>
    [AvaloniaFact]
    public void Repeated_Close_During_Exit_Is_Idempotent()
    {
        var win = new Window { Width = 200, Height = 150, Content = new Border() };
        DialogMotion.Attach(win);
        win.Show();
        Pump();

        win.Close();
        Pump();
        var body = (Control)win.Content!;
        var first = body.Transitions;

        win.Close(); // 退场途中再点一次
        Pump();

        Assert.True(win.IsVisible, "重复关闭不该让窗口提前消失");
        Assert.True(DialogMotion.IsClosing(win));
        // 重复 Close 不重装过渡 (引用不变即未重复编排)
        Assert.Same(first, body.Transitions);

        DialogMotion.ForceExitDelayElapsed(win);
        Pump();
        Assert.False(win.IsVisible);
    }

    /// <summary>减少动效: 关闭直落 (不拦截, 一次就关)。</summary>
    [AvaloniaFact]
    public void Reduced_Motion_Closes_Directly()
    {
        var win = new Window { Width = 200, Height = 150, Content = new Border() };
        using var _ = new EnvVarScope("KEYFLUX_NO_MOTION", "1");
        DialogMotion.Attach(win);
        win.Show();
        Pump();

        win.Close();
        Pump();

        Assert.False(win.IsVisible, "减少动效时必须直落关闭 (不介入)");
    }

    private static void Pump()
    {
        Dispatcher.UIThread.RunJobs();
        AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        Dispatcher.UIThread.RunJobs();
    }

    private sealed class EnvVarScope : IDisposable
    {
        private readonly string _key;
        private readonly string? _old;

        public EnvVarScope(string key, string value)
        {
            _key = key;
            _old = Environment.GetEnvironmentVariable(key);
            Environment.SetEnvironmentVariable(key, value);
        }

        public void Dispose() => Environment.SetEnvironmentVariable(_key, _old);
    }
}
