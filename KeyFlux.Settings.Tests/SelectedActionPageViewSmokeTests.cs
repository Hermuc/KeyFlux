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
/// 选中动作页 XAML 运行时冒烟 (聚合卡重构后): 编译期查不出 StaticResource 解析失败、
/// 共享编辑器 DataTemplate 内的事件绑定 / $parent 绑定错误, 只能靠实例化整页 + 布局运行兜底。
/// 覆盖: 空态渲染 / 两聚合卡 toggle 渲染 / 切换查看类型 (点亮 + 待配置态) / 弹窗模糊。
/// </summary>
[Collection("I18nSerial")]
public sealed class SelectedActionPageViewSmokeTests
{
    private static (SelectedActionPageViewModel Page, SelectedActionPageView View, Window Window) CreateHost()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
            SelectedAction = new SelectedAction
            {
                Mappings =
                [
                    new SelectedMapping
                    {
                        MatchType = "fileExt", MatchValue = "jpg, png",
                        Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }],
                    },
                    new SelectedMapping
                    {
                        MatchType = "textType", MatchValue = "url",
                        Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
                    },
                ],
            },
        };
        var page = new SelectedActionPageViewModel(main); // 按模型建两张卡

        var view = new SelectedActionPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        return (page, view, window);
    }

    /// <summary>
    /// 添加规则面板打开时背景模糊 (ModalBlur) 守护: 曾因遮罩 Background 写成
    /// "{StaticResource ...}00" 解析为不透明红 (用户报红色背景)。
    /// 打开 → 页头/列表 Effect 为 BlurEffect; 关闭 → 撤销。
    /// </summary>
    [AvaloniaFact]
    public void AddPanel_Open_Blurs_Page_Background()
    {
        var (page, view, window) = CreateHost();
        try
        {
            Assert.Null(BackgroundEffect(view));
            page.OpenAddPanelCommand.Execute(null);
            Dispatcher.UIThread.RunJobs();
            Assert.IsType<BlurEffect>(BackgroundEffect(view));

            page.CloseAddPanelCommand.Execute(null);
            Dispatcher.UIThread.RunJobs();
            Assert.Null(BackgroundEffect(view));
        }
        finally
        {
            window.Close();
        }
    }

    /// <summary>页内挂了 ModalBlur.IsActive 的元素 (页头 DockPanel / 列表 ScrollViewer) 的当前 Effect。</summary>
    private static Avalonia.Media.IEffect? BackgroundEffect(Avalonia.Visual root)
        => root.GetVisualDescendants()
            .OfType<Avalonia.Visual>()
            .Where(v => KeyFlux.Settings.Controls.ModalBlur.GetIsActive(v))
            .Select(v => v.Effect)
            .FirstOrDefault();

    [AvaloniaFact]
    public void Page_Instantiates_And_Renders_Both_Cards()
    {
        var (page, view, window) = CreateHost();
        try
        {
            // 两聚合卡 + 主快捷键卡 + 模拟测试条 均实例化
            var cards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("type-card")).ToList();
            Assert.Equal(2, cards.Count);

            // 类型 toggle 渲染 (文本卡 5 个内置特征 + 文件卡至少 1 个分组)
            var toggles = view.GetVisualDescendants().OfType<ToggleButton>()
                .Where(b => b.Classes.Contains("type-toggle")).ToList();
            Assert.True(toggles.Count >= 6, $"应渲染文本卡 5 toggle + 文件卡 >=1, 实得 {toggles.Count}");

            // 已配置类型编辑器内的 ComboBox (行为下拉) 经隐式 ComboOption 模板渲染 SelectionBox
            foreach (var combo in view.GetVisualDescendants().OfType<ComboBox>())
            {
                if (combo.SelectedItem is Services.ComboOption opt && !opt.IsSeparator)
                {
                    var label = view.GetVisualDescendants().OfType<TextBlock>()
                        .Any(tb => ReferenceEquals(tb.DataContext, opt) && tb.Text == opt.Label);
                    Assert.True(label, $"SelectionBox 未用隐式模板渲染: {opt.Label}");
                }
            }

            // 切换查看类型 (点亮 + 待配置态渲染), 无 XAML 运行时异常
            page.TextCard.SelectType("path"); // 未配置 -> 待配置态
            Dispatcher.UIThread.RunJobs();
            page.FileCard.SelectType("group:image"); // 已配置 -> 编辑器详情
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
        var (_, _, window) = CreateHost();
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
