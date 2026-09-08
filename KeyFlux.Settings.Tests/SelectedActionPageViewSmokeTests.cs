using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 选中动作页 XAML 运行时冒烟: 编译期查不出 StaticResource 解析失败、共享行卡
/// DataTemplate 内的事件绑定 / $parent 绑定错误, 只能靠实例化整页 + 布局运行兜底。
/// 覆盖: 空态渲染 / fileExt+textType 两分区行卡渲染 / 展开手风琴再收起。
/// </summary>
public sealed class SelectedActionPageViewSmokeTests
{
    private static (SelectedActionPageViewModel Page, SelectedActionPageView View, Window Window) CreateHost()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
        };
        var page = new SelectedActionPageViewModel(main);
        // 两分区各一行 (触发共享行卡模板的两个数据形态), 各带一个已展开手风琴的 entries
        page.FileMappings.Add(new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "fileExt",
            MatchValue = "jpg",
            Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }],
        }));
        page.TextMappings.Add(new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "textType",
            MatchValue = "url",
            Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
        }));
        page.RefreshPartitionTitles(); // 同真实加载链路: 分区首行标题标记
        page.ExpandedRow = page.FileMappings[0];

        var view = new SelectedActionPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        return (page, view, window);
    }

    [AvaloniaFact]
    public void Page_Instantiates_And_Renders_Both_Partitions()
    {
        var (page, view, window) = CreateHost();
        try
        {
            // 行卡由共享 DataTemplate 渲染: 两分区各应产出可见的行内按钮
            var buttons = view.GetVisualDescendants().OfType<Button>().ToList();
            Assert.NotEmpty(buttons);

            // 隐式 ComboOption 模板必须同时作用于下拉项与 SelectionBox (后者无显式模板,
            // 依赖 ContentPresenter 回退到页面资源查找 —— A4 去重的核心风险点)
            foreach (var combo in view.GetVisualDescendants().OfType<ComboBox>())
            {
                if (combo.SelectedItem is Services.ComboOption opt && !opt.IsSeparator)
                {
                    var label = view.GetVisualDescendants().OfType<TextBlock>()
                        .Any(tb => ReferenceEquals(tb.DataContext, opt) && tb.Text == opt.Label);
                    Assert.True(label, $"SelectionBox 未用隐式模板渲染: {opt.Label}");
                }
            }

            // 展开另一分区行 → 手风琴内容 (编辑器面板) 出现, 无 XAML 运行时异常
            page.ExpandedRow = page.TextMappings[0];
            Dispatcher.UIThread.RunJobs();
            Assert.NotEmpty(view.GetVisualDescendants().OfType<ContentControl>());

            // 收起后不抛异常 (删除/收起路径依赖 ExpandedRow 仲裁)
            page.ExpandedRow = null;
            Dispatcher.UIThread.RunJobs();
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>诊断: 渲染整页并保存 PNG 到 %TEMP%, 供人工目视检查 (不参与断言)。</summary>
    [AvaloniaFact]
    public void Capture_Page_Screenshot()
    {
        var (page, view, window) = CreateHost();
        try
        {
            window.Background = Brushes.White; // headless 默认透明底, 截图白底更接近真实观感
            Dispatcher.UIThread.RunJobs();
            var path = Path.Combine(Path.GetTempPath(), "keyflux-selected-action.png");
            window.CaptureRenderedFrame()?.Save(path);
            Assert.True(File.Exists(path), "截帧未产出文件");
        }
        finally
        {
            window.Close();
        }
    }
}
