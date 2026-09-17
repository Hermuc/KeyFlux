using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless;
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
/// 选中动作页组件框外观守护 (2026-09-16)。
///
/// 本页配方 = BoxShadow **ClaudeShadowCard** + BorderThickness **1** + ClaudeBorderCreamBrush。
/// 阴影取 Settings / 插件页同款; **描边有意比其它三处(2px)更细** —— 用户看过实际效果后裁定
/// 本页"线条太粗/太重": 本页卡片密集堆叠(行卡间距仅 8px, 且行卡内嵌一张子卡),
/// 同一 2px 在此处框线密度过高而发重。**勿把本页"修正"成 2px**, 那不是笔误。
///
/// 本页原先把 BorderBrush/BorderThickness **内联**写在每个 Border 上且无阴影,
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
        // 窗口 1000 高: 页面内容约 900 高, 用 820 会把最底部的「模拟测试」卡切掉,
        // 导致悬停测试的点落在窗口外 (2026-09-16 踩过)
        var win = new Window { Width = 1200, Height = 1000, Content = view };
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
                Assert.Equal(1, card.BorderThickness.Left);
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);
                Assert.Equal(shadow.ToString(), card.BoxShadow.ToString());
            }
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ③ 承载映射行卡的两个 ItemsControl **必须关掉裁剪**。
    /// ItemsControl 默认裁剪到自身边界, 而行卡 BoxShadow 画在卡片边界之外 ⇒
    /// 不关掉就只剩卡片之间那一段可见, 列表外沿(首卡上/末卡下/各卡左右)的阴影全被切掉,
    /// 即用户报的「阴影被裁切」。插件页同款坑、同款修法 (PluginsPageView.axaml 有原始注释)。
    /// </summary>
    [AvaloniaFact]
    public void Mapping_Lists_Must_Not_Clip_Card_Shadow()
    {
        var (page, view, win) = CreateHost();
        try
        {
            var lists = view.GetVisualDescendants().OfType<ItemsControl>()
                .Where(ic => ReferenceEquals(ic.ItemsSource, page.TextMappings)
                          || ReferenceEquals(ic.ItemsSource, page.FileMappings))
                .ToList();
            Assert.Equal(2, lists.Count);
            foreach (var list in lists)
            {
                Assert.False(list.ClipToBounds, "映射行卡的 ItemsControl 必须 ClipToBounds=False, 否则阴影被裁");
            }
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ④ 行卡必须在 ScrollViewer 视口内**留出画阴影的水平余量**。
    /// 实测过的不对称: 内容左对齐贴着视口左沿时, 卡片左沿 - 视口左沿 = 0px ⇒ 左侧阴影被
    /// ScrollViewer 视口裁掉, 而右侧有 566px 余量正常渲染 ⇒ 表现为"右有左无"。
    /// 阴影 ClaudeShadowCard 第二层 blur=18, 水平外扩约 9px ⇒ 左余量须 >= 9px。
    /// 修法是**移动裁剪边界而非移动内容**: 页面 Grid 左内边距 20->8, Header 与 PageRoot 各 +12 左 Margin,
    /// 三者等量抵消故视觉布局不变, 但给左侧腾出 12px。**勿把这三处改回等值**, 否则阴影又被裁。
    /// </summary>
    [AvaloniaFact]
    public void Row_Cards_Must_Leave_Room_For_Shadow_Inside_Scroller()
    {
        var (page, view, win) = CreateHost();
        try
        {
            var scroller = view.GetVisualDescendants().OfType<ScrollViewer>().First();
            var card = view.GetVisualDescendants().OfType<Border>()
                .First(b => ReferenceEquals(b.DataContext, page.FileMappings[0])
                         || ReferenceEquals(b.DataContext, page.TextMappings[0]));
            _ = page;

            var svLeft = scroller.TranslatePoint(default, win)!.Value.X;
            var cardLeft = card.TranslatePoint(default, win)!.Value.X;

            Assert.True(cardLeft - svLeft >= 9,
                $"行卡左沿距视口左沿仅 {cardLeft - svLeft:F1}px ⇒ 左侧阴影会被视口裁掉 (需要 >=9px)");
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ⑤ 五个组件框**悬停时不得出现灰描边** (2026-09-17 用户要求清除组件框悬停灰线),
    /// 且改为**加深投影** (ClaudeShadowCardDeep) 保留"抬起"感。
    /// 原实现悬停把 BorderBrush 转 ClaudeRingWarmBrush(#d1cfc5 灰) —— 本项反向断言:
    /// 悬停后 (a) 描边色仍 == 静止色(奶油), (b) BoxShadow 换 Deep 档。
    /// </summary>
    [AvaloniaFact]
    public void All_Action_Cards_Hover_Deepens_Shadow_Without_Gray_Border()
    {
        var (_, view, win) = CreateHost();
        try
        {
            Assert.True(Application.Current!.TryGetResource("ClaudeBorderCreamBrush", out var creamObj));
            var cream = ((SolidColorBrush)creamObj!).Color;
            var rest = (BoxShadows)view.FindResource("ClaudeShadowCard")!;
            var deep = (BoxShadows)view.FindResource("ClaudeShadowCardDeep")!;

            var cards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("actionCard")).ToList();
            Assert.True(cards.Count >= 5, $"应有 >=5 个组件框, 实得 {cards.Count}");

            var checkedNames = new List<string>();
            foreach (var card in cards)
            {
                // 静止态基线
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);
                Assert.Equal(rest.ToString(), card.BoxShadow.ToString());

                var p = card.TranslatePoint(
                    new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), win)!.Value;
                win.MouseMove(p);
                Dispatcher.UIThread.RunJobs();

                Assert.True(card.IsPointerOver,
                    $"{string.Join("+", card.Classes)} 悬停应命中 (Bounds={card.Bounds})");
                // 悬停: 无灰线 (描边色不变), 只加深投影
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);
                Assert.Equal(deep.ToString(), card.BoxShadow.ToString());
                checkedNames.Add(string.Join("+", card.Classes));

                win.MouseMove(new Point(1, 1));
                Dispatcher.UIThread.RunJobs();
            }
            Assert.Equal(5, checkedNames.Count);
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

            // 悬停已选中的行卡时仍须保持陶土边 —— 2026-09-17 后悬停只改 BoxShadow、
            // 不动描边色, 与 .matched 的 BorderBrush 无交集 ⇒ 天然不冲突;
            // 此项防将来又把悬停改回"换描边色"而压掉陶土边。
            var hp = card.TranslatePoint(new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), win)!.Value;
            win.MouseMove(hp);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsPointerOver);
            Assert.Equal(((ISolidColorBrush)terra!).Color, ((ISolidColorBrush)card.BorderBrush!).Color);
        }
        finally
        {
            win.Close();
        }
    }
}
