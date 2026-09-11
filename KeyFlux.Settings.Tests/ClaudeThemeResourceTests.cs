using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Threading;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// Claude 暖色共享主题 (<c>Styles/ClaudeTheme.axaml</c>) 的运行时守护。
///
/// 为什么必须存在本类 —— 该主题的令牌与类样式经 <c>App.axaml</c> 全局加载, 而
/// <c>dotnet build</c> **不校验** XAML 中 <c>{StaticResource}</c> 的两类错误, 它们只在
/// XAML **加载期**才炸:
///   ① 令牌缺失 (宿主未加载主题)        → <c>KeyNotFoundException</c>;
///   ② 令牌类型与目标属性不兼容          → <c>InvalidCastException</c>。
/// 2026-09 重构中两类都真实发生过: 半径令牌被误定义为 <c>x:Double</c> 却赋给
/// <c>CornerRadius</c> 属性, 结果构建 0 错误、但 <c>PluginsPageView</c> /
/// <c>PluginMarketWindow</c> / <c>MainWindow</c> 三者构造即崩 —— 直到 QA 用 headless
/// 探针实测才暴露。本类把这类运行期错误前移到测试: 只要主题被加载、视图能构造、
/// 关键令牌能落到正确属性上, 就说明全局主题未退化。
///
/// 归入 I18nSerial 集合: 本类构造 MainWindow / PluginsPageView 会读写全局 I18n.Language,
/// 需与在(可能非 UI 的)线程上翻转语言的用例互斥。
/// </summary>
[Collection("I18nSerial")]
public sealed class ClaudeThemeResourceTests
{
    /// <summary>
    /// 三个真实视图都必须能在「已加载 Claude 主题」的宿主下构造成功。
    /// 构造过程即会解析各自 XAML 里的全部 <c>{StaticResource Claude*}</c> ——
    /// 令牌缺失或类型不符都会在此抛出, 因此「构造不抛」本身就是有效断言。
    ///
    /// 刻意拆成三条独立用例而非一条: 一条用例里连续构造三个视图时, 第一个抛出会
    /// 掩盖后两个的结果, 定位不到到底是哪个视图退化 (2026-09 变异验证时确实踩到)。
    /// </summary>
    [AvaloniaFact]
    public void PluginsPageView_Constructs_Without_Resource_Or_Type_Errors()
        => Assert.NotNull(new PluginsPageView());

    /// <summary>插件市场窗: 页面级消费 33 处 Claude 令牌 (含 CornerRadius 内联用法)。</summary>
    [AvaloniaFact]
    public void PluginMarketWindow_Constructs_Without_Resource_Or_Type_Errors()
        => Assert.NotNull(new PluginMarketWindow());

    /// <summary>
    /// 主窗: 经 <c>ContentControl.DataTemplates</c> 间接挂载上述页面; 全局主题或令牌退化时
    /// 同样会崩。只构造不 Show —— 其 Opened 会 InitializeAsync 拉起后端子进程。
    /// </summary>
    [AvaloniaFact]
    public void MainWindow_Constructs_Without_Resource_Or_Type_Errors()
    {
        var main = new MainViewModel(new BackendSessionOptions())
        {
            Config = new Config { Options = new Options() },
        };
        Assert.NotNull(new MainWindow(main));
    }

    /// <summary>
    /// 半径令牌必须是 <see cref="CornerRadius"/> 类型并正确落到 <c>CornerRadius</c> 属性上。
    /// 历史回归: 曾定义为 <c>x:Double</c>, 赋给 <c>CornerRadius</c> 时抛
    /// <c>InvalidCastException: Setter value '12' is not a valid value for property 'CornerRadius'</c>。
    /// <c>Border.claudeCard</c> 用 <c>ClaudeRadiusLg</c> (12)。
    /// </summary>
    [AvaloniaFact]
    public void Radius_Token_Applies_To_CornerRadius_Property()
    {
        var card = new Border();
        card.Classes.Add("claudeCard");
        var window = new Window { Content = card };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            Assert.Equal(new CornerRadius(12), card.CornerRadius);
            // 同一 Class 的另一令牌: Ivory 卡片面 (#faf9f5), 证明画刷令牌也已解析
            Assert.Equal(Color.Parse("#faf9f5"), ((ISolidColorBrush)card.Background!).Color);
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>
    /// 主 CTA 按钮的 Terracotta 令牌必须解析并覆盖 Fluent 默认按钮底色,
    /// 同时 <c>Button.claudeCta</c> 的 <c>CornerRadius</c> (<c>ClaudeRadiusMd</c>=8) 正确落地。
    /// </summary>
    [AvaloniaFact]
    public void Cta_Button_Uses_Terracotta_Token_And_Radius()
    {
        var button = new Button();
        button.Classes.Add("claudeCta");
        var window = new Window { Content = button };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            Assert.Equal(Color.Parse("#c96442"), ((ISolidColorBrush)button.Background!).Color);
            Assert.Equal(new CornerRadius(8), button.CornerRadius);
        }
        finally
        {
            window.Close();
        }
    }
}
