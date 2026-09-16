using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 选中动作页组件框外观守护 (2026-09-16 用户要求与 Settings / 插件页同款)。
///
/// 统一配方 (与 Border.settingsCard / Border.pluginCard / Border.leftPanel 逐项一致):
///   BoxShadow = ClaudeShadowCard; BorderThickness = 2; BorderBrush = ClaudeBorderCreamBrush。
/// 本页原先把这三项**内联**写在每个 Border 上且无阴影、1px 描边, 故与其它页明显不一致;
/// 现统一由 Border.actionCard 样式承载 (内联值优先级高于样式 Setter, 必须移除内联项)。
///
/// 另一条守护: **默认态与选中态 (.matched) 的描边粗细与阴影必须相同**, 仅颜色区分 ——
/// 状态样式只覆盖 BorderBrush, 不得改粗细/阴影, 否则选中行会与相邻行"厚薄不一"。
/// </summary>
[Collection("I18nSerial")]
public sealed class ActionPageCardStyleTests
{
    private static (SelectedActionPageViewModel Page, SelectedActionPageView View, Window Win) CreateHost()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
        };
        var page = new SelectedActionPageViewModel(main);
        page.FileMappings.Add(new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "fileExt", MatchValue = "jpg",
            Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }],
        }));
        page.TextMappings.Add(new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "textType", MatchValue = "url",
            Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
        }));
        page.RefreshPartitionTitles();
        page.ExpandedRow = page.FileMappings[0]; // 展开一行 ⇒ 嵌套条目卡也实例化

        var view = new SelectedActionPageView { DataContext = page };
        var win = new Window { Width = 1200, Height = 820, Content = view };
        win.Show();
        Dispatcher.UIThread.RunJobs();
        return (page, view, win);
    }

    /// <summary>① 页内每个组件框都必须带上统一配方 (粗细 / 描边色 / 阴影 全等)。</summary>
    [AvaloniaFact]
    public void Action_Cards_Use_Unified_Card_Recipe()
    {
        var (_, view, win) = CreateHost();
        try
        {
            var cards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("actionCard")).ToList();
            Assert.True(cards.Count >= 5,
                $"应有 >=5 个组件框 (2 张行卡 + 主快捷键卡 + 模拟测试条 + 嵌套条目卡), 实得 {cards.Count}");

            var shadow = (BoxShadows)view.FindResource("ClaudeShadowCard")!;
            Assert.True(Application.Current!.TryGetResource("ClaudeBorderCreamBrush", out var creamObj));
            var cream = ((SolidColorBrush)creamObj!).Color;

            foreach (var card in cards)
            {
                Assert.Equal(2, card.BorderThickness.Left);
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);
                Assert.Equal(shadow.ToString(), card.BoxShadow.ToString());
            }
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>② 选中态 (.matched) 只换颜色, 粗细与阴影必须与默认态完全一致。</summary>
    [AvaloniaFact]
    public void Matched_State_Keeps_Thickness_And_Shadow()
    {
        var (page, view, win) = CreateHost();
        try
        {
            var target = page.FileMappings[0];
            var card = view.GetVisualDescendants().OfType<Border>()
                .First(b => b.Classes.Contains("row-card") && ReferenceEquals(b.DataContext, target));

            var thicknessBefore = card.BorderThickness;
            var shadowBefore = card.BoxShadow.ToString();
            Assert.DoesNotContain("matched", card.Classes);

            target.IsMatched = true; // 真实驱动 Classes.matched 绑定
            Dispatcher.UIThread.RunJobs();

            Assert.Contains("matched", card.Classes);
            Assert.Equal(thicknessBefore, card.BorderThickness);
            Assert.Equal(shadowBefore, card.BoxShadow.ToString());
            Application.Current!.TryGetResource("ClaudeTerracottaBrush", out var terra);
            Assert.Equal(((ISolidColorBrush)terra!).Color, ((ISolidColorBrush)card.BorderBrush!).Color);
        }
        finally
        {
            win.Close();
        }
    }
}
