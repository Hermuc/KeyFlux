using System.Text.Json;
using KeyFlux.Settings.Models;
using MatchType = KeyFlux.Settings.Models.MatchType;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 聚合卡不变量测试 (2026-09 重构新增):
///   - 同类型最多一条 mapping (卡级 toggle 与 FindMappingForType 去重, AddMapping 拒绝重复);
///   - toggle 顺序 canonical (文本特征 plain 恒末; 文件分组在前、自定义类型在后);
///   - 禁止空 entries (单行为删除被守卫; transient 详情只在首个行为加入后才落盘);
///   - 模型投影幂等 (RunTestAsync 的序列化快照往返稳定, Config 即唯一真源)。
/// </summary>
[Collection("I18nSerial")]
public sealed class SelectedActionCardInvariantTests
{
    private static void EnsureBehaviorCatalog()
        => BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);

    private static (SelectedActionPageViewModel Page, Config Config) CreatePage(Config config)
    {
        EnsureBehaviorCatalog();
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = config;
        return (new SelectedActionPageViewModel(main), config);
    }

    // ------------------------------------------------------------- 同类型最多一条 mapping

    /// <summary>同一 (matchType, matchValue) 经 AddMapping 去重: 重复添加不增加条数。</summary>
    [Fact]
    public void Same_Type_And_Value_Deduplicated_To_Single_Mapping()
    {
        var config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
        };
        var (page, cfg) = CreatePage(config);

        page.OpenAddPanelCommand.Execute(null);
        var p1 = page.AddPanel!;
        p1.MatchValue = "jpg";
        p1.BehaviorPicks.First(x => x.Pack.Id == "open").IsChecked = true;
        page.AddMapping(p1);
        Assert.Single(cfg.SelectedAction.Mappings);

        page.OpenAddPanelCommand.Execute(null);
        var p2 = page.AddPanel!;
        p2.MatchValue = "jpg"; // 完全相同的匹配条件
        p2.BehaviorPicks.First(x => x.Pack.Id == "open").IsChecked = true;
        page.AddMapping(p2);

        Assert.Single(cfg.SelectedAction.Mappings); // 去重, 不重复添加
    }

    /// <summary>卡级 toggle 对每个类型 id 唯一; FindMappingForType 至多命中一条。</summary>
    [Fact]
    public void Card_Never_Produces_Duplicated_Type_Toggle()
    {
        var config = new Config
        {
            FileGroups =
            [
                new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] },
                new FileGroup { Name = "code", Label = "代码", Exts = ["c", "cpp"] },
            ],
            MatchTypes = [new MatchType { Id = "x1", Kind = "fileExt", Label = "自定义" }],
            SelectedAction = new SelectedAction
            {
                Mappings =
                [
                    new SelectedMapping { MatchType = "fileExt", MatchValue = "jpg, png", Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }] },
                    new SelectedMapping { MatchType = "fileExt", MatchValue = "jpg, png", Entries = [new SelectedEntry { Behavior = "run", Options = new RuleOptions() }] },
                ],
            },
        };
        var (page, cfg) = CreatePage(config);

        // 两条相同 (fileExt, "jpg, png") mapping -> 文件卡只有一个 group:image toggle (去重)
        var ids = page.FileCard.Toggles.Select(t => t.Id).ToList();
        Assert.Single(ids.Where(id => id == "group:image"));
        // FindMappingForType 至多命中一条 (取首条)
        Assert.NotNull(page.FileCard.FindMappingForType("group:image"));
        Assert.Equal(2, cfg.SelectedAction.Mappings.Count(m => m.MatchValue == "jpg, png"));
    }

    // ------------------------------------------------------------- 顺序 canonical

    /// <summary>文本特征 toggle 顺序 = 注册表顺序且 plain 恒居末位。</summary>
    [Fact]
    public void TextToggles_Follow_Canonical_Registry_Order_Plain_Last()
    {
        var (page, _) = CreatePage(new Config());
        var ids = page.TextCard.Toggles.Select(t => t.Id).ToList();
        Assert.Equal(ActionSchemeCatalog.TextTypes.Select(t => t.Value), ids);
        Assert.Equal("plain", ids[^1]);
    }

    /// <summary>文件卡 toggle 顺序: 分组在前 (按 Config.FileGroups 序), 自定义 fileExt 类型在后。</summary>
    [Fact]
    public void FileToggles_Follow_Canonical_Group_Then_Custom_Order()
    {
        var config = new Config
        {
            FileGroups =
            [
                new FileGroup { Name = "image", Label = "图片", Exts = ["jpg"] },
                new FileGroup { Name = "code", Label = "代码", Exts = ["c"] },
            ],
            MatchTypes =
            [
                new MatchType { Id = "a", Kind = "fileExt", Label = "A" },
                new MatchType { Id = "b", Kind = "fileExt", Label = "B" },
            ],
        };
        var (page, _) = CreatePage(config);
        var ids = page.FileCard.Toggles.Select(t => t.Id).ToList();
        Assert.Equal(["group:image", "group:code", "type:a", "type:b"], ids);
    }

    // ------------------------------------------------------------- 禁止空 entries

    /// <summary>单行为映射: 删除最后一个行为被守卫 (CanRemove=false), 不产生空 entries。</summary>
    [Fact]
    public void Single_Entry_Deletion_Guarded_No_Empty_Entries()
    {
        var (page, cfg) = CreatePage(new Config());
        cfg.SelectedAction.Mappings.Add(new SelectedMapping
        {
            MatchType = "textType", MatchValue = "url",
            Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
        });
        page.RebuildCards();
        page.TextCard.SelectType("url");
        var row = page.TextCard.Detail!;
        Assert.False(row.Editors[0].CanRemove);

        row.RemoveEntry(row.Editors[0]); // 守卫: 至少保留一个行为
        Assert.Single(row.Mapping.Entries);
        Assert.Single(cfg.SelectedAction.Mappings.Single(m => m.MatchValue == "url").Entries);
    }

    /// <summary>未配置类型: 进入 transient 详情 (不落盘); 首个行为加入后才提交真实 mapping (无空 entries)。</summary>
    [Fact]
    public void Transient_Detail_Commits_Only_After_First_Entry()
    {
        var (page, cfg) = CreatePage(new Config());
        page.TextCard.SelectType("path"); // 未配置
        var row = page.TextCard.Detail!;
        Assert.True(row.IsTransient);
        Assert.True(page.TextCard.IsPending);
        Assert.Empty(cfg.SelectedAction.Mappings.Where(m => m.MatchValue == "path"));

        row.AddEntryCommand.Execute(null); // 加入首个行为 -> 提交真实 mapping
        Assert.False(row.IsTransient);
        var created = cfg.SelectedAction.Mappings.Single(m => m.MatchValue == "path");
        Assert.Single(created.Entries); // 落盘即有 >=1 entry, 从不产生空 entries
    }

    // ------------------------------------------------------------- 投影幂等

    /// <summary>模型投影幂等: RunTestAsync 的序列化快照往返稳定 (序列化->反序列化->再序列化 一致)。</summary>
    [Fact]
    public void Model_Projection_RoundTrip_Is_Idempotent()
    {
        var config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
            SelectedAction = new SelectedAction
            {
                Hotkey = ">^p",
                Enable = true,
                Mappings =
                [
                    new SelectedMapping
                    {
                        MatchType = "textType", MatchValue = "url",
                        Entries =
                        [
                            new SelectedEntry { Behavior = "open_url", ActionValue = "", WorkingDir = "", Options = new RuleOptions() },
                        ],
                    },
                ],
            },
        };
        var (_, cfg) = CreatePage(config);

        var sa = cfg.SelectedAction;
        var first = JsonSerializer.Serialize(sa, SettingsJson.Options);
        var round = JsonSerializer.Deserialize<SelectedAction>(first, SettingsJson.Options)!;
        var second = JsonSerializer.Serialize(round, SettingsJson.Options);
        Assert.Equal(first, second);
    }
}
