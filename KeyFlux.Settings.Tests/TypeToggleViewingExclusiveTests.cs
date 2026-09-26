using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Interactivity;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 聚合卡类型 toggle 的语义锁定 (2026-09 重构, 原 TextTypeToggleExclusiveTests 反转为「查看态」):
/// 卡内全部类型 toggle 始终可见; 点亮 = 正在查看该类型 (卡内唯一点亮),
/// 点击 toggle **只切查看对象、不改变任何配置** —— 与旧两分区行卡「点亮=改写 MatchValue」语义相反。
/// 小圆点 = 已配置 (存在对应 mapping); 未配置类型点击进入「待配置」详情态 (不落盘)。
/// Toggle 数量/顺序与 <see cref="ActionSchemeCatalog.TextTypes"/> 一致, 由注册表派生断言保证。
/// </summary>
[Collection("I18nSerial")]
public sealed class TypeToggleViewingExclusiveTests
{
    private static (SelectedActionPageViewModel Page, Config Config) CreatePage()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        var config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
            SelectedAction = new SelectedAction
            {
                Mappings =
                [
                    new SelectedMapping
                    {
                        MatchType = "textType",
                        MatchValue = "url",
                        Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
                    },
                ],
            },
        };
        main.Config = config;
        return (new SelectedActionPageViewModel(main), config);
    }

    /// <summary>卡内 toggle 数量由注册表派生 (不再硬编码): 新增内置文本特征而漏加 XAML Toggle 时, 这里立即变红。</summary>
    [Fact]
    public void TextCard_Shows_All_TextType_Toggles()
    {
        var (page, _) = CreatePage();
        Assert.Equal(ActionSchemeCatalog.TextTypes.Length, page.TextCard.Toggles.Count);
        // 顺序与注册表一致 (plain 恒居末位)
        Assert.Equal(ActionSchemeCatalog.TextTypes.Select(t => t.Value),
            page.TextCard.Toggles.Select(t => t.Id));
    }

    /// <summary>点击 toggle 切换查看对象, **不**改写任何 mapping 的 MatchValue (语义反转核心)。</summary>
    [Fact]
    public void Clicking_Toggle_Switches_View_Not_Data()
    {
        var (page, config) = CreatePage();
        var card = page.TextCard;
        Assert.Equal("url", card.SelectedToggleId); // 初始: url 已配置 -> 默认查看 url
        var urlMapping = config.SelectedAction.Mappings.Single(m => m.MatchValue == "url");
        Assert.True(card.IsTypeConfigured("url"));

        card.SelectType("path"); // 点 path toggle 切查看
        Assert.Equal("path", card.SelectedToggleId); // 点亮切到 path
        Assert.Equal("url", urlMapping.MatchValue);  // 数据丝毫不动
        Assert.True(card.IsTypeConfigured("url"));   // url 仍配置
        Assert.False(card.IsTypeConfigured("path")); // path 仍无配置
    }

    /// <summary>同一时刻卡内恰有一个点亮 (互斥查看), 与旧「至多点亮两个」缺陷相反。</summary>
    [Fact]
    public void Exactly_One_Toggle_Lit_At_A_Time()
    {
        var (page, _) = CreatePage();
        var card = page.TextCard;
        foreach (var id in new[] { "path", "magnet", "bilibili", "plain", "url" })
        {
            card.SelectType(id);
            var lit = card.Toggles.Where(t => t.IsLit).ToList();
            Assert.Single(lit);
            Assert.Equal(id, lit[0].Id);
        }
    }

    /// <summary>未配置类型点击进入「待配置」态 (transient 详情, 无点), 不落盘。</summary>
    [Fact]
    public void Unconfigured_Type_Shows_Pending_Detail_And_No_Dot()
    {
        var (page, config) = CreatePage();
        var card = page.TextCard;
        card.SelectType("path");
        Assert.True(card.IsPending);
        Assert.False(card.IsTypeConfigured("path"));
        Assert.NotNull(card.Detail);
        Assert.True(card.Detail!.IsTransient);
        Assert.Equal(0, config.SelectedAction.Mappings.Count(m => m.MatchValue == "path"));
    }

    /// <summary>已配置类型: 有点 + 真实 (非 transient) 编辑器详情。</summary>
    [Fact]
    public void Configured_Type_Shows_Dot_And_Real_Editor_Detail()
    {
        var (page, _) = CreatePage();
        var card = page.TextCard;
        Assert.Equal("url", card.SelectedToggleId);
        Assert.False(card.IsPending);
        Assert.NotNull(card.Detail);
        Assert.False(card.Detail!.IsTransient);
        Assert.Single(card.Detail.Mapping.Entries);
    }

    /// <summary>XAML 接线: 视图内点 toggle 按钮 -> 切查看 (OneWay IsLit + Click 链路), 且数据不变。</summary>
    [AvaloniaFact]
    public void View_Clicking_Toggle_Switches_View_And_Keeps_Data()
    {
        var (page, config) = CreatePage();
        var view = new SelectedActionPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        try
        {
            // 仅统计文本卡内的 toggle (视图含两张卡, 须按 DataContext 实例收窄到 TextCard)
            var textCardBorder = view.GetVisualDescendants().OfType<Border>()
                .First(b => ReferenceEquals(b.DataContext, page.TextCard));
            var toggles = textCardBorder.GetVisualDescendants().OfType<ToggleButton>()
                .Where(b => b.Classes.Contains("type-toggle")).ToList();
            Assert.Equal(ActionSchemeCatalog.TextTypes.Length, toggles.Count);

            var urlMapping = config.SelectedAction.Mappings.Single(m => m.MatchValue == "url");
            var pathBtn = toggles.First(t => (t.Content?.ToString() ?? "").StartsWith("路径"));
            // 模拟真实点击: 触发 Click 路由事件 -> OnTypeToggleClicked -> SelectCommand (而非直接置 IsChecked, 后者不触达 handler)
            pathBtn.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            Dispatcher.UIThread.RunJobs();

            Assert.Equal("path", page.TextCard.SelectedToggleId);
            Assert.Equal("url", urlMapping.MatchValue); // 数据未变
            var lit = toggles.Where(t => t.IsChecked == true).Select(t => t.Content?.ToString()).ToList();
            Assert.Single(lit); // 仍恰一个亮
        }
        finally
        {
            window.Close();
        }
    }
}
