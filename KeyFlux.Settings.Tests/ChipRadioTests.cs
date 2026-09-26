using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 芯片单选 (ChipRadio) 功能守护 (2026-09-13 行为选择器重构):
/// 自绘模板生效 (Fluent 原模板被替换) + 选中态走 Terracotta 软底/描边 (不再有 Fluent 冷蓝)。
/// 断言读 PART_Root 生效值; 为免 120ms 画刷过渡插值干扰 (坑 39), 事前清空 Transitions。
/// </summary>
[Collection("I18nSerial")]
public sealed class ChipRadioTests
{
    [AvaloniaFact]
    public void ChipRadio_Custom_Template_Applies_And_Checked_State_Is_Warm()
    {
        var app = Application.Current!;
        Assert.True(app.TryFindResource("ChipRadio", out var themeObj), "ChipRadio 皮肤键缺失");
        var rb = new RadioButton
        {
            Theme = (ControlTheme)themeObj!,
            GroupName = "t",
            Content = "测试项",
        };
        var window = new Window { Width = 300, Height = 200, Content = rb };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            // 自绘模板生效: 视觉树存在 PART_Root Border (Fluent 原模板无此部件名)
            var root = rb.GetVisualDescendants().OfType<Border>()
                .FirstOrDefault(b => b.Name == "PART_Root");
            Assert.NotNull(root);

            // 未选中基态: White 底 + Warm 边
            rb.Transitions = null; // 清过渡: 样式生效值即时落定, 免插值断言
            Assert.Equal(Color.Parse("#ffffff"), ((ISolidColorBrush)root!.Background!).Color);
            Assert.Equal(Color.Parse("#e8e6dc"), ((ISolidColorBrush)root.BorderBrush!).Color);

            // 选中态: Terracotta 软底 + Terracotta 描边 (画刷真值经应用资源读取比对)
            rb.IsChecked = true;
            Dispatcher.UIThread.RunJobs();
            Assert.Equal(((ISolidColorBrush)app.FindResource("ClaudeTerracottaSoftBrush")!).Color,
                ((ISolidColorBrush)root.Background!).Color);
            Assert.Equal(((ISolidColorBrush)app.FindResource("ClaudeTerracottaBrush")!).Color,
                ((ISolidColorBrush)root.BorderBrush!).Color);
        }
        finally
        {
            window.Close();
        }
    }
}
