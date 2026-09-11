using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 插件页 (左侧导航「插件」) 的运行时冒烟 + View 注册守护。
/// 编译期查不出「View 未在 MainWindow 的 ContentControl.DataTemplates 注册」
/// (症状: 点击「插件」后内容区空白), 故本类:
///   ① 实例化真实 <see cref="MainWindow"/> (不 Show), 断言 PluginsPageViewModel 能解析出 PluginsPageView;
///   ② 把 <see cref="PluginsPageView"/> 挂进窗口渲染, 证明 XAML 运行时可用、文案绑定生效。
/// 归入 I18nSerial 集合: 本类与 I18nResourceTests 都读写全局 I18n.Language, 必须串行避免串扰。
/// </summary>
[Collection("I18nSerial")]
public sealed class PluginsPageViewSmokeTests
{
    private static (PluginsPageViewModel Page, MainViewModel Main) CreateVm(bool collectEnabled)
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            Options = new Options { QuickSwitch = new QuickSwitchOption { CollectEnabled = collectEnabled } },
        };
        return (new PluginsPageViewModel(main), main);
    }

    /// <summary>① View 真能渲染: 标题 + 内置插件行 + 第三方说明 + 启用状态全部出现。</summary>
    [AvaloniaFact]
    public void PluginsPage_Renders_BuiltIn_And_ThirdParty_Sections()
    {
        var original = I18n.Language;
        I18n.Language = I18n.Zh;
        var (page, _) = CreateVm(collectEnabled: true);
        var view = new PluginsPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 760, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            var texts = window.GetVisualDescendants().OfType<TextBlock>()
                .Select(t => t.Text).Where(t => !string.IsNullOrEmpty(t)).ToList();

            Assert.Contains(I18n.T("2418"), texts); // 插件 (标题)
            Assert.Contains(I18n.T("2419"), texts); // 内置插件
            Assert.Contains(I18n.T("2408"), texts); // 快速切换 (插件名)
            Assert.Contains(I18n.T("2420"), texts); // 第三方插件
            Assert.Contains(I18n.T("2421"), texts); // 插件市场尚未开放
            Assert.Contains(I18n.T("2423"), texts); // 已启用 (collectEnabled=true)
            Assert.DoesNotContain(I18n.T("2424"), texts); // 不应出现「已停用」

            Assert.Equal("插件", I18n.T("2418")); // sanity: 当前语言确为中文
        }
        finally
        {
            window.Close();
            I18n.Language = original;
        }
    }

    /// <summary>② 启用状态由 config.options.quickSwitch.collectEnabled 驱动 (2423 已启用 / 2424 已停用)。</summary>
    [AvaloniaFact]
    public void PluginsPage_Status_Reflects_CollectEnabled_Toggle()
    {
        var original = I18n.Language;
        I18n.Language = I18n.Zh;
        try
        {
            var (on, _) = CreateVm(collectEnabled: true);
            var (off, _) = CreateVm(collectEnabled: false);
            Assert.Equal(I18n.T("2423"), on.QuickSwitchStatusText);  // 已启用
            Assert.Equal(I18n.T("2424"), off.QuickSwitchStatusText); // 已停用
        }
        finally
        {
            I18n.Language = original;
        }
    }

    /// <summary>
    /// ③ View 注册守护: 复用真实 MainWindow 的 ContentControl.DataTemplates, 在一个普通窗口里
    /// 解析插件页 VM —— 若 MainWindow.axaml 漏了 PluginsPageViewModel -> PluginsPageView 的映射,
    /// 内容区将解析不出 PluginsPageView (真实症状即「点击插件页后一片空白」)。
    /// 只构造不 Show MainWindow —— 其 Opened 会 InitializeAsync 拉起后端子进程, 测试环境须回避。
    /// </summary>
    [AvaloniaFact]
    public void MainWindow_Registers_Plugins_Page_Template()
    {
        var (page, main) = CreateVm(collectEnabled: true);

        var mainWindow = new MainWindow(main); // 不 Show: 不触发 Opened/InitializeAsync
        var source = mainWindow.GetLogicalDescendants().OfType<ContentControl>()
            .FirstOrDefault(c => c.DataTemplates.Count > 0);
        Assert.NotNull(source);

        // 复用真实注册表做解析 (不硬编码 View 类型名, 直接以 Avalonia 模板匹配为准)
        var host = new ContentControl { Content = page };
        foreach (var template in source!.DataTemplates) host.DataTemplates.Add(template);
        var shell = new Window { Width = 1200, Height = 760, Content = host };
        shell.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            Assert.NotNull(shell.GetVisualDescendants().OfType<PluginsPageView>().FirstOrDefault());
        }
        finally
        {
            shell.Close();
        }
    }

    /// <summary>
    /// ④ 内置插件卡片为 Button (可点击): 包含 2408 文案的 TextBlock 祖先链中存在 Button。
    /// ⑤ 第三方插件区域仍为 Border (不可点击): 包含 2420/2421 文案的区域不存在 Button。
    /// </summary>
    [AvaloniaFact]
    public void BuiltIn_Card_Is_Button_And_ThirdParty_Is_Border()
    {
        var original = I18n.Language;
        I18n.Language = I18n.Zh;
        var (page, _) = CreateVm(collectEnabled: true);
        var view = new PluginsPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 760, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            var textBlocks = window.GetVisualDescendants().OfType<TextBlock>().ToList();

            // ④ 内置插件卡片: 找到 2408 (快速切换) 文案, 祖先链应包含 Button
            var quickSwitchText = textBlocks.FirstOrDefault(t => t.Text == I18n.T("2408"));
            Assert.NotNull(quickSwitchText);
            var hasButtonAncestor = GetVisualAncestorTypes(quickSwitchText!).Contains(typeof(Button));
            Assert.True(hasButtonAncestor,
                "内置插件卡片 (2408 快速切换) 应为 Button 包裹, 使其可点击打开配置对话框");

            // ⑤ 第三方插件区域: 找到 2420 (第三方插件) 文案, 祖先链不应包含 Button
            var thirdPartyText = textBlocks.FirstOrDefault(t => t.Text == I18n.T("2420"));
            Assert.NotNull(thirdPartyText);
            var thirdPartyHasButton = GetVisualAncestorTypes(thirdPartyText!).Contains(typeof(Button));
            Assert.False(thirdPartyHasButton,
                "第三方插件标题 (2420) 不应位于 Button 内, 该区域不可点击");

            // 同样验证 2421 (插件市场尚未开放)
            var marketText = textBlocks.FirstOrDefault(t => t.Text == I18n.T("2421"));
            Assert.NotNull(marketText);
            var marketHasButton = GetVisualAncestorTypes(marketText!).Contains(typeof(Button));
            Assert.False(marketHasButton,
                "第三方插件说明 (2421) 不应位于 Button 内, 该区域不可点击");
        }
        finally
        {
            window.Close();
            I18n.Language = original;
        }
    }

    /// <summary>收集 Visual 的全部祖先类型 (向上遍历直到根)。</summary>
    private static HashSet<Type> GetVisualAncestorTypes(Avalonia.Visual visual)
    {
        var types = new HashSet<Type>();
        Avalonia.Visual? current = visual;
        while (current is not null)
        {
            types.Add(current.GetType());
            current = current.GetVisualParent();
        }
        return types;
    }
}
