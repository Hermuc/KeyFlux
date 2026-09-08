using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 文本特征四选一 Toggle 的互斥性验证: 依次点击各 Toggle, 同一时刻必须只有一个点亮,
/// 且 MatchValue 与点亮项一致 (用户反馈"最多能点亮两个"的排查)。
/// </summary>
public sealed class TextTypeToggleExclusiveTests
{
    private static (SelectedActionPageViewModel Page, MappingRowVm Row, SelectedActionPageView View, Window Window) CreateHost()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config();
        var page = new SelectedActionPageViewModel(main);
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "textType",
            MatchValue = "url",
            Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
        });
        page.TextMappings.Add(row);

        var view = new SelectedActionPageView { DataContext = page };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        return (page, row, view, window);
    }

    /// <summary>行头里的四个类型 Toggle (textType 行特有)。</summary>
    private static List<ToggleButton> TypeToggles(SelectedActionPageView view)
        => view.GetVisualDescendants().OfType<ToggleButton>()
            .Where(b => b.Classes.Contains("type-toggle"))
            .ToList();

    private static string[] LitLabels(List<ToggleButton> toggles)
        => toggles.Where(t => t.IsChecked == true).Select(t => t.Content?.ToString() ?? "?").ToArray();

    [AvaloniaFact]
    public void Clicking_Toggles_Keeps_Exactly_One_Lit()
    {
        var (_, row, view, window) = CreateHost();
        try
        {
            var toggles = TypeToggles(view);
            Assert.Equal(4, toggles.Count);
            Assert.Equal(["链接"], LitLabels(toggles)); // 初始: url 行只有"链接"亮

            // 依次点击其余三个: 每次点击后必须恰有一个亮, 且是刚点的那个
            foreach (var expect in new[] { "路径", "磁力链接", "纯文本" })
            {
                var target = toggles.First(t => (t.Content?.ToString() ?? "").StartsWith(expect[..2]));
                target.IsChecked = true; // 模拟点击 (与用户点击走同一绑定/事件链路)
                Dispatcher.UIThread.RunJobs();
                var lit = LitLabels(toggles);
                Assert.True(lit.Length == 1,
                    $"点击「{expect}」后点亮了 {lit.Length} 个: [{string.Join(", ", lit)}]");
                Assert.Equal(expect, lit[0]);
            }

            // 数据面: MatchValue 始终单值
            Assert.Equal("plain", row.Mapping.MatchValue);

            // 点已亮的那个: 不应出现全灭 (数据保持, 视觉回弹)
            var plain = toggles.First(t => t.IsChecked == true);
            plain.IsChecked = false;
            Dispatcher.UIThread.RunJobs();
            Assert.Equal("plain", row.Mapping.MatchValue); // 数据未清
        }
        finally
        {
            window.Close();
        }
    }
}
