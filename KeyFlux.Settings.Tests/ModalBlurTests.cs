using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Threading;
using KeyFlux.Settings.Controls;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// ModalBlur 附加行为守护 (弹窗背景模糊替代颜色遮罩):
/// ① IsActive 开关 → Effect 增删, Radius 传递;
/// ② IsHost 宿主注册 + SetActive 叠窗计数 (归零才撤销模糊)。
/// </summary>
[Collection("I18nSerial")]
public sealed class ModalBlurTests
{
    [AvaloniaFact]
    public void IsActive_Toggles_BlurEffect_And_Radius_Propagates()
    {
        var border = new Border();
        var window = new Window { Content = border };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            ModalBlur.SetRadius(border, 20);
            ModalBlur.SetIsActive(border, true);
            var blur = Assert.IsType<BlurEffect>(border.Effect);
            Assert.Equal(20, blur.Radius);

            ModalBlur.SetIsActive(border, false);
            Assert.Null(border.Effect);
        }
        finally
        {
            window.Close();
        }
    }

    [AvaloniaFact]
    public void Host_Registration_And_SetActive_Counting()
    {
        var host = new Border();
        var mainWin = new Window { Width = 600, Height = 400, Content = host };
        ModalBlur.SetIsHost(host, true);
        mainWin.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            ModalBlur.SetActive(mainWin, true);   // DialogChrome 的调用形态 (owner=主窗)
            Assert.IsType<BlurEffect>(host.Effect);

            ModalBlur.SetActive(mainWin, true);   // 叠窗 +1
            ModalBlur.SetActive(mainWin, false);  // 关一层, 还剩一层 → 仍模糊
            Assert.IsType<BlurEffect>(host.Effect);

            ModalBlur.SetActive(mainWin, false);  // 全关 → 撤销
            Assert.Null(host.Effect);
        }
        finally
        {
            mainWin.Close();
        }
    }
}
