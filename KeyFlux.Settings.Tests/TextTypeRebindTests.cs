using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 特征切换与行为联动回归锁定 (需求: 展开 = 所见即当前类型生效的行为):
/// 换特征后, 不适用新前提的 entry 自动换为该前提默认行为 (模板重置与手动换行为同语义);
/// 适用者保留。重绑直接作用于底层 Entries —— 收起态 Editors 为空也生效, 展开态重建编辑行。
/// fileExt 逐字符输入/分组填入不走重绑 (打字中途破坏性 + 分组"不兼容行为保持不动"既有约定)。
/// </summary>
[Collection("BehaviorCatalogSerial")]
public sealed class TextTypeRebindTests
{
    private static SelectedActionPageViewModel CreatePage()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config();
        return new SelectedActionPageViewModel(main);
    }

    private static MappingRowVm NewRow(SelectedActionPageViewModel page, string value, params string[] behaviors)
    {
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "textType",
            MatchValue = value,
            Entries = [.. behaviors.Select(b => new SelectedEntry { Behavior = b, Options = new RuleOptions() })],
        });
        page.TextMappings.Add(row);
        return row;
    }

    /// <summary>收起态换特征: 底层 Entries 直接重绑, 下次展开即新类型的默认行为。</summary>
    [Fact]
    public void Collapsed_Toggle_Switches_Dirty_Entries_To_Default_Of_New_Type()
    {
        var page = CreatePage();
        var row = NewRow(page, "url", "open_url");
        Assert.Equal("open_url", row.Mapping.Entries[0].Behavior);

        row.IsPlain = true; // open_url 不适用 plain → 换 plain 默认 search (模板随包默认)
        Assert.Equal("search", row.Mapping.Entries[0].Behavior);
        Assert.Equal("https://bing.com/search?q=%selected%", row.Mapping.Entries[0].ActionValue);

        row.IsPath = true; // search 不覆盖 path → open_path
        Assert.Equal("open_path", row.Mapping.Entries[0].Behavior);

        row.IsMagnet = true; // → magnet_download
        Assert.Equal("magnet_download", row.Mapping.Entries[0].Behavior);

        row.IsUrl = true; // → open_url (回到起点, 模板清空: open_url 无参)
        Assert.Equal("open_url", row.Mapping.Entries[0].Behavior);
        Assert.Equal("", row.Mapping.Entries[0].ActionValue);
    }

    /// <summary>展开态换特征: 编辑行随新行为重建, 下拉当前值立即可见。</summary>
    [Fact]
    public void Expanded_Toggle_Rebuilds_Editors_With_New_Default()
    {
        var page = CreatePage();
        var row = NewRow(page, "url", "open_url");
        page.ExpandedRow = row; // 展开构建编辑行
        Assert.Equal("open_url", row.Editors[0].Entry.Behavior);

        row.IsPlain = true;
        Assert.Equal("search", row.Editors[0].Entry.Behavior);
        Assert.Equal("search", row.Editors[0].BehaviorSelected?.Value); // 下拉当前值同步
        Assert.Contains("search", row.Editors[0].BuildBehaviorOptions().Select(o => o.Value)); // 脏值不再置顶
        Assert.Equal("search", row.Chips[0].Label); // chips 键位表随行为重算
    }

    /// <summary>适用新前提的行为保留不动 (跨类型通用行为不被误重置)。</summary>
    [Fact]
    public void Covering_Behavior_Preserved_On_Type_Switch()
    {
        var page = CreatePage();
        var row = NewRow(page, "url", "search"); // 构造脏值: search 已不覆盖 url (plain 下合法, 验证跨类型保留)
        row.IsPlain = true;
        Assert.Equal("search", row.Mapping.Entries[0].Behavior);
    }

    /// <summary>多 entry 全部重绑且数量稳定 (chips 序号自动顺延)。</summary>
    [Fact]
    public void Multi_Entry_All_Rebound_And_Entry_Count_Stable()
    {
        var page = CreatePage();
        var row = NewRow(page, "url", "open_url", "search");
        row.IsMagnet = true;
        Assert.All(row.Mapping.Entries, e => Assert.Equal("magnet_download", e.Behavior));
        Assert.Equal(2, row.Mapping.Entries.Count);
        Assert.Equal(2, row.Chips.Count); // 只换值不增删, 序号顺延
    }

    /// <summary>
    /// 前提快照记忆 (2026-09-10): 切走再切回, 之前配置 (含自改模板) 被还原, 而非重绑
    /// 默认模板 —— url(open_url) -> plain(重绑 search, 自改模板) -> url -> plain 还原自改模板。
    /// </summary>
    [Fact]
    public void Switch_Back_Restores_Previous_Premise_Snapshot()
    {
        var page = CreatePage();
        var row = NewRow(page, "url", "open_url");
        row.IsPlain = true; // 重绑 search + 包默认模板
        row.Mapping.Entries[0].ActionValue = "https://custom/?q=%selected%"; // 用户自改模板

        row.IsUrl = true; // 切回 url: 快照还原 (open_url, 无参)
        Assert.Equal("open_url", row.Mapping.Entries[0].Behavior);
        Assert.Equal("", row.Mapping.Entries[0].ActionValue);

        row.IsPlain = true; // 再切回 plain: 还原快照 (search, 自改模板), 非重绑默认模板
        Assert.Equal("search", row.Mapping.Entries[0].Behavior);
        Assert.Equal("https://custom/?q=%selected%", row.Mapping.Entries[0].ActionValue);
    }
}
