using System.Linq;
using Avalonia;
using Avalonia.Animation;
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
/// 选中动作页组件框外观守护 (2026-09-16, 聚合卡重构后适配):
///
/// 本页配方 = BoxShadow **SelectedActionCardShadow** (静止) / **SelectedActionCardShadowHover** (悬停光圈)
/// (2026-09-22 起由皮肤 Halo 家族改为本页专用四面等距配方 —— 原 offsetY 6 导致顶部/左右几乎无影)
/// + BorderThickness **1** + ClaudeBorderCreamBrush。
/// 阴影取 Settings / 插件页同款; **描边有意比其它三处(2px)更细** —— 用户看过实际效果后裁定
/// 本页"线条太粗/太重"。
///
/// 重构后页面由两张聚合卡 (TypeCardVm: 文本特征 / 文件后缀) + 主快捷键卡 + 模拟测试条 组成,
/// 卡内类型 toggle 的 ItemsControl 必须关裁剪 (toggle 的光圈/描边画在自身边界外), 卡体在 ScrollViewer 内须留阴影余量。
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
        var page = new SelectedActionPageViewModel(main); // 构造即按模型建两张卡 + 默认查看已配置类型

        var view = new SelectedActionPageView { DataContext = page };
        // 窗口需足够高: 聚合卡重构后两张卡内联渲染 toggle + 编辑器, 页面内容约 1150 高,
        // 用 1000 会把最底部的「模拟测试」卡拉到窗口外 (y≈1001), 悬停测试的点落窗外 → 必挂。
        var win = new Window { Width = 1200, Height = 1400, Content = view };
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
                .Where(b => b.Classes.Contains("actionCard")
                            && !b.Classes.Contains("rowEditor")
                            && !b.Classes.Contains("row-card")).ToList();
            Assert.True(cards.Count >= 3,
                $"应有 >=3 个组件框 (2 张聚合卡 + 主快捷键卡), 实得 {cards.Count}");

            var shadow = (BoxShadows)view.FindResource("SelectedActionCardShadow")!;
            Assert.True(Application.Current!.TryGetResource("ClaudeBorderCreamBrush", out var creamObj));
            var cream = ((SolidColorBrush)creamObj!).Color;

            foreach (var card in cards)
            {
                Assert.Equal(1, card.BorderThickness.Left);
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);
                Assert.Equal(shadow.ToString(), card.BoxShadow.ToString());
                // 悬停光圈必须是**淡入**的: 卡片自带 BoxShadow 过渡 (皮肤令牌 120ms)
                var t = Assert.Single(card.Transitions!.OfType<BoxShadowsTransition>());
                Assert.Equal("BoxShadow", t.Property!.Name);
                Assert.Equal(ClaudeMotion.Micro, t.Duration);
            }

            // 卡内嵌套子卡片豁免统一配方 (2026-09-18 rowEditor / 2026-09-22 row-card):
            // 它们嵌在聚合卡内部, 双层投影与外层卡阴影叠加显脏 ⇒ 静止档必须零阴影,
            // 只留 1px 奶油描边分层。
            var innerCards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("rowEditor") || b.Classes.Contains("row-card")).ToList();
            Assert.NotEmpty(innerCards);
            var zero = BoxShadows.Parse("0 0 0 0 Transparent");
            foreach (var e in innerCards)
            {
                Assert.Equal(zero.ToString(), e.BoxShadow.ToString());
            }
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ③ 承载类型 toggle 的 ItemsControl **必须关掉裁剪**。
    /// toggle 的悬停/点亮光圈与描边画在自身边界之外, 若 ItemsControl 裁边界则被切掉。
    /// (原「已配置小圆点微标」已于 2026-09-22 按用户要求移除, 该断言继续守护裁剪不变式。)
    /// </summary>
    [AvaloniaFact]
    public void Toggle_Lists_Must_Not_Clip_Card_Shadow()
    {
        var (_, view, win) = CreateHost();
        try
        {
            var lists = view.GetVisualDescendants().OfType<ItemsControl>()
                .Where(ic => ic.DataContext is TypeCardVm)
                .ToList();
            Assert.Equal(2, lists.Count); // 文本卡 + 文件卡
            foreach (var list in lists)
            {
                Assert.False(list.ClipToBounds, "类型 toggle 的 ItemsControl 必须 ClipToBounds=False, 否则圆点徽标被裁");
            }
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ④ 聚合卡必须在 ScrollViewer 视口内**留出画阴影的四边余量**（2026-09-22 扩展为四面）。
    /// 阴影本页配方 SelectedActionCardShadow: blur 4/14 + offsetY 0/2 ⇒ 四边外扩约 5~9px,
    /// 取 9px 作为统一余量要求; 内容左对齐贴视口左沿或卡片顶到视口上下沿都会把投影裁掉。
    /// </summary>
    [AvaloniaFact]
    public void Cards_Must_Leave_Room_For_Shadow_Inside_Scroller()
    {
        var (_, view, win) = CreateHost();
        try
        {
            var scroller = view.GetVisualDescendants().OfType<ScrollViewer>().First();
            var cards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("type-card")).ToList();
            Assert.Equal(2, cards.Count);

            var svOrigin = scroller.TranslatePoint(default, win)!.Value;
            const double need = 9; // SelectedActionCardShadow 最大外扩 (blur 14 / 2 + offsetY 2 = 9)

            foreach (var card in cards)
            {
                var p = card.TranslatePoint(default, win)!.Value;
                var cardRight = p.X + card.Bounds.Width;
                var cardBottom = p.Y + card.Bounds.Height;
                var viewportRight = svOrigin.X + scroller.Bounds.Width;

                Assert.True(p.X - svOrigin.X >= need,
                    $"聚合卡左余量仅 {p.X - svOrigin.X:F1}px (需 >= {need}px) ⇒ 左侧阴影被裁");
                Assert.True(viewportRight - cardRight >= need,
                    $"聚合卡右余量仅 {viewportRight - cardRight:F1}px (需 >= {need}px) ⇒ 右侧阴影被裁");
                Assert.True(p.Y - svOrigin.Y >= need - 2,
                    $"聚合卡上余量仅 {p.Y - svOrigin.Y:F1}px ⇒ 顶部阴影被裁");
                Assert.True(cardBottom <= svOrigin.Y + scroller.Bounds.Height + need,
                    "聚合卡底部超视口 ⇒ 底部阴影被裁");
            }
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ⑤ 四个组件框**悬停时描边色不变 (无灰线) 且点亮陶土色光圈**。
    /// </summary>
    [AvaloniaFact]
    public void All_Action_Cards_Hover_Adds_Halo_Without_Gray_Border()
    {
        var (_, view, win) = CreateHost();
        try
        {
            Assert.True(Application.Current!.TryGetResource("ClaudeBorderCreamBrush", out var creamObj));
            var cream = ((SolidColorBrush)creamObj!).Color;
            var rest = (BoxShadows)view.FindResource("SelectedActionCardShadow")!;
            var hover = (BoxShadows)view.FindResource("SelectedActionCardShadowHover")!;

            var cards = view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("actionCard") && !b.Classes.Contains("rowEditor") && !b.Classes.Contains("row-card")).ToList();
            Assert.True(cards.Count >= 3, $"应有 >=3 个组件框, 实得 {cards.Count}");

            foreach (var c in cards)
            {
                c.Transitions = new Transitions();
            }

            var checkedNames = new List<string>();
            foreach (var card in cards)
            {
                win.MouseMove(new Point(1, 1));
                SettleShadow(card, rest, $"静止档 [{checkedNames.Count}]");

                Assert.False(card.IsPointerOver,
                    $"{string.Join("+", card.Classes)} 指针停在 (1,1) 时不应命中 (Bounds={card.Bounds})");
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);

                var p = card.TranslatePoint(
                    new Point(card.Bounds.Width / 2, card.Bounds.Height / 2), win)!.Value;
                win.MouseMove(p);
                SettleShadow(card, hover, $"悬停档 [{checkedNames.Count}]");

                Assert.True(card.IsPointerOver,
                    $"{string.Join("+", card.Classes)} 悬停应命中 (Bounds={card.Bounds})");
                Assert.Equal(cream, ((ISolidColorBrush)card.BorderBrush!).Color);
                checkedNames.Add(string.Join("+", card.Classes));
            }
            Assert.Equal(cards.Count, checkedNames.Count);

            // 行内编辑器卡: 悬停也必须保持零阴影 (2026-09-18 去影裁定, 光圈不适用于嵌套面板)
            var editor = view.GetVisualDescendants().OfType<Border>()
                .First(b => b.Classes.Contains("rowEditor"));
            editor.Transitions = new Transitions();
            win.MouseMove(new Point(1, 1));
            Dispatcher.UIThread.RunJobs();
            var ep = editor.TranslatePoint(
                new Point(editor.Bounds.Width / 2, editor.Bounds.Height / 2), win)!.Value;
            win.MouseMove(ep);
            Dispatcher.UIThread.RunJobs();
            Assert.True(editor.IsPointerOver, "编辑器卡悬停应命中 (否则本断言是空跑)");
            Assert.Equal(BoxShadows.Parse("0 0 0 0 Transparent").ToString(), editor.BoxShadow.ToString());
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// 把卡片的 BoxShadow 推到期望的**终点态**再断言。headless 下过渡动画由平台时钟驱动,
    /// 而 `Dispatcher.UIThread.RunJobs()` **不推进**它 ⇒ 带过渡直接读只能拿到插值中间值。
    /// </summary>
    private static void SettleShadow(Border card, BoxShadows expected, string what)
    {
        for (var i = 0; i < 40; i++)
        {
            Dispatcher.UIThread.RunJobs();
            if (card.BoxShadow.ToString() == expected.ToString())
            {
                return;
            }

            AvaloniaHeadlessPlatform.ForceRenderTimerTick();
        }

        Assert.True(false,
            $"{what} {string.Join("+", card.Classes)} 40 帧后仍未到终点态: 期望 {expected}; " +
            $"实得 {card.BoxShadow}; IsPointerOver={card.IsPointerOver}; " +
            $"Transitions={card.Transitions.Count}; Bounds={card.Bounds}");
    }

    /// <summary>② 选中态 (.matched) 已随「模拟测试」卡片一并移除, 命中高亮不再存在。</summary>
}
