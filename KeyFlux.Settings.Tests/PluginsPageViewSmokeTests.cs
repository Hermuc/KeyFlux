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
/// 插件页 (左侧导航「插件」, Claude 风格统一列表) 的运行时冒烟 + View 注册守护。
/// 编译期查不出「View 未在 MainWindow 的 ContentControl.DataTemplates 注册」
/// (症状: 点击「插件」后内容区空白), 故本类:
///   ① 实例化真实 <see cref="MainWindow"/> (不 Show), 断言 PluginsPageViewModel 能解析出 PluginsPageView;
///   ② 把 <see cref="PluginsPageView"/> 挂进窗口渲染, 证明 XAML 运行时可用、文案绑定生效;
///   ③ 统一插件卡结构守护 (信息区 Button + 开关 + 开关下方状态文字同卡)。
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

    /// <summary>
    /// ① View 真能渲染: 统一列表 (首卡内置「快速切换」) + 入口按钮 + 运行时说明全部出现。
    /// 2026-09 Claude 风格重构后不再分区, 2419/2420 (内置/已导入分区标题) 已删, 不应出现。
    /// </summary>
    [AvaloniaFact]
    public void PluginsPage_Renders_Unified_List_And_Entries()
    {
        var original = I18n.Language;
        I18n.Language = I18n.Zh;
        var (page, _) = CreateVm(collectEnabled: true);
        page.Refresh();
        var view = new PluginsPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 760, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            var texts = window.GetVisualDescendants().OfType<TextBlock>()
                .Select(t => t.Text).Where(t => !string.IsNullOrEmpty(t)).ToList();

            Assert.Contains(I18n.T("2418"), texts); // 插件 (标题)
            Assert.Contains(I18n.T("2408"), texts); // 快速切换 (内置首卡)
            Assert.Contains(I18n.T("2422"), texts); // 快速切换描述
            Assert.Contains(I18n.T("2425"), texts); // 运行时说明 (诚实边界)
            Assert.Contains(I18n.T("2427"), texts); // 导入插件 (入口)
            Assert.Contains(I18n.T("2428"), texts); // 插件市场 (入口)
            Assert.Contains(I18n.T("2423"), texts); // 已启用 (开关下方状态文字)

            // 已删除的分区标题不应再出现 (键已从 i18n.json 移除, T() 回显键名本身也不会出现)
            Assert.DoesNotContain("内置插件", texts);
            Assert.DoesNotContain("已导入插件", texts);

            // 统一列表: 至少一张插件卡 (内置 QuickSwitch)
            Assert.NotEmpty(page.Plugins);
            Assert.Contains(page.Plugins, p => p.IsBuiltin && p.Manifest.Id == "quick_switch");

            Assert.Equal("插件", I18n.T("2418")); // sanity: 当前语言确为中文
        }
        finally
        {
            window.Close();
            I18n.Language = original;
        }
    }

    /// <summary>
    /// ② 内置卡开关由 config.options.quickSwitch.collectEnabled 驱动, 切换即写通配置,
    /// 状态文字 (2423/2424) 随开关翻转。
    /// </summary>
    [AvaloniaFact]
    public void BuiltIn_Card_Toggle_Reflects_And_Writes_Back_CollectEnabled()
    {
        var original = I18n.Language;
        I18n.Language = I18n.Zh;
        try
        {
            var (on, _) = CreateVm(collectEnabled: true);
            on.Refresh(); // 外部同步 (BuildNav 路径)
            var onCard = on.Plugins.Single(p => p.IsBuiltin);
            Assert.True(onCard.Enabled);
            Assert.Equal(I18n.T("2423"), onCard.StatusText); // 已启用

            var (off, offMain) = CreateVm(collectEnabled: false);
            off.Refresh();
            var offCard = off.Plugins.Single(p => p.IsBuiltin);
            Assert.False(offCard.Enabled);
            Assert.Equal(I18n.T("2424"), offCard.StatusText); // 已停用

            // 写通: 切换卡开关 -> 内存 Config 立即同步 + 状态文字翻转
            // (保存由 SaveAsync 异步链路承载; 测试后端缺失时直接返回, 不影响断言)
            offCard.Enabled = true;
            Assert.True(offMain.Config!.Options.QuickSwitch.CollectEnabled);
            Assert.Equal(I18n.T("2423"), offCard.StatusText);
            offCard.Enabled = false;
            Assert.False(offMain.Config!.Options.QuickSwitch.CollectEnabled);
            Assert.Equal(I18n.T("2424"), offCard.StatusText);

            // 内置卡不可删除
            Assert.False(offCard.CanDelete);
            Assert.True(offCard.CanConfigure);
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
    /// ④ 统一插件卡组件守护: 内置卡信息区为 Button (可点开配置) 且与 ToggleSwitch 同卡;
    /// 状态文字 (2423) 出现在同一卡的开关列 (开关正下方)。
    /// </summary>
    [AvaloniaFact]
    public void Unified_Card_Has_Button_Body_Toggle_And_Status_Text()
    {
        var original = I18n.Language;
        I18n.Language = I18n.Zh;
        var (page, _) = CreateVm(collectEnabled: true);
        page.Refresh();
        var view = new PluginsPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 760, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            var textBlocks = window.GetVisualDescendants().OfType<TextBlock>().ToList();

            // 内置卡信息区: 2408 (快速切换) 文案应位于 Button 内 (点击开配置对话框)
            var quickSwitchText = textBlocks.FirstOrDefault(t => t.Text == I18n.T("2408"));
            Assert.NotNull(quickSwitchText);
            Assert.True(GetVisualAncestorTypes(quickSwitchText!).Contains(typeof(Button)),
                "内置卡信息区 (2408 快速切换) 应为 Button 包裹, 使其可点击打开配置对话框");

            // 统一卡: 2408 所在卡 (pluginCard Border) 内应同时含 ToggleSwitch 与状态文字
            var cardBorder = GetVisualAncestors(quickSwitchText!)
                .OfType<Border>()
                .FirstOrDefault(b => b.Classes.Contains("pluginCard"));
            Assert.NotNull(cardBorder);
            var cardDescendants = cardBorder.GetVisualDescendants().ToList();
            Assert.NotEmpty(cardDescendants.OfType<Avalonia.Controls.ToggleSwitch>());
            var cardTexts = cardDescendants.OfType<TextBlock>().Select(t => t.Text).ToList();
            Assert.Contains(I18n.T("2423"), cardTexts); // 状态文字在卡内 (开关下方)

            // 入口按钮: 2427 (导入插件) / 2428 (插件市场) 应位于 Button 内
            foreach (var key in new[] { "2427", "2428" })
            {
                var entry = textBlocks.FirstOrDefault(t => t.Text == I18n.T(key));
                Assert.NotNull(entry);
                Assert.True(GetVisualAncestorTypes(entry!).Contains(typeof(Button)),
                    $"入口按钮 ({key}) 应为 Button 包裹, 使其可点击");
            }
        }
        finally
        {
            window.Close();
            I18n.Language = original;
        }
    }

    /// <summary>收集 Visual 的全部祖先 (向上遍历直到根)。</summary>
    private static IEnumerable<Avalonia.Visual> GetVisualAncestors(Avalonia.Visual visual)
    {
        var current = visual.GetVisualParent();
        while (current is not null)
        {
            yield return current;
            current = current.GetVisualParent();
        }
    }

    /// <summary>收集 Visual 的全部祖先类型 (向上遍历直到根)。</summary>
    private static HashSet<Type> GetVisualAncestorTypes(Avalonia.Visual visual)
        => GetVisualAncestors(visual).Select(v => v.GetType()).ToHashSet();
}
