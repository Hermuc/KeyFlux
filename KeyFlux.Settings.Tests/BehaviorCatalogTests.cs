using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 内置行为包测试夹具: 与 bin/behaviors/ 11 个 manifest 的 appliesTo/entry 语义一致
/// (行为库/覆盖过滤测试共用; 若内置包前提调整, 此处与 golden 守卫需同步)。
/// 条目顺序 = 目录序 (ID 字典序), 决定 BehaviorCatalog 默认推导的稳定序。
/// </summary>
public static class BehaviorFixtures
{
    public static List<BehaviorPack> Builtin() =>
    [
        Pack("copy", [Text("plain"), File(["*"])], "copy"),
        Pack("magnet_download", [Text("magnet", def: true)], "magnet_download"),
        Pack("open", [File(["*"])], "open"),
        Pack("open_folder", [Text("path"), File(["*"])], "open_folder"),
        Pack("open_path", [Text("path", def: true), File(["*"], def: true)], "open_path"),
        Pack("open_registry", [Text("plain")], "open_registry"),
        Pack("open_url", [Text("url", def: true)], "open_url"),
        Pack("run", [Text("plain"), File(["*"])], "run", template: "%selected%"),
        Pack("script", [Text("plain"), File(["*"])], "script"),
        Pack("search", [Text("url"), Text("plain", def: true)], "search",
             template: "https://www.google.com/search?q=%selected%"),
        Pack("send_keys", [Text("plain")], "send_keys"),
    ];

    /// <summary>用户行为包样例 (PS 编辑图片)。</summary>
    public static BehaviorPack PsEdit() => Pack("ps_edit", [File(["jpg", "png"])], "run",
        source: "user", template: "Photoshop.exe \"%selected%\"");

    private static BehaviorAppliesTo File(string[] exts, bool def = false)
        => new() { Type = "fileExt", Exts = [.. exts], Default = def };

    private static BehaviorAppliesTo Text(string value, bool def = false)
        => new() { Type = "textType", Value = value, Default = def };

    private static BehaviorPack Pack(string id, BehaviorAppliesTo[] appliesTo, string action,
        string source = "builtin", string? template = null)
        => new()
        {
            Id = id,
            Name = id,
            NameEn = id,
            SpecVersion = 1,
            AppliesTo = [.. appliesTo],
            Entry = new BehaviorEntry
            {
                Kind = "builtin",
                Action = action,
                Params = template is null ? null : new BehaviorEntryParams { ActionValue = template },
            },
            Source = source,
        };
}

public sealed class BehaviorCatalogTests
{
    /// <summary>文件语境覆盖集: 通配行为 6 项, 按目录序; 文本专用行为不覆盖文件前提。</summary>
    [Fact]
    public void Covering_FileExt_Yields_Generic_Builtin_Set()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var covering = BehaviorCatalog.Covering("fileExt", "jpg,png");
        Assert.Equal(new[] { "copy", "open", "open_folder", "open_path", "run", "script" },
            covering.Select(p => p.Id));
    }

    /// <summary>文本特征覆盖: url → open_url + search; plain → 6 项 (均为无 default 标记的覆盖包)。</summary>
    [Fact]
    public void Covering_TextType_Follows_Pack_Premises()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        Assert.Equal(new[] { "open_url", "search" },
            BehaviorCatalog.Covering("textType", "url").Select(p => p.Id));
        Assert.Equal(new[] { "copy", "open_registry", "run", "script", "search", "send_keys" },
            BehaviorCatalog.Covering("textType", "plain").Select(p => p.Id));
    }

    /// <summary>default 标记推导: url→open_url / path→open_path / magnet→magnet_download /
    /// plain→search / 任意文件→open_path; 用户包同样参与推导。</summary>
    [Fact]
    public void DefaultFor_Follows_Pack_Default_Flags()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), [BehaviorFixtures.PsEdit()]);
        Assert.Equal("open_url", BehaviorCatalog.DefaultFor("textType", "url"));
        Assert.Equal("open_path", BehaviorCatalog.DefaultFor("textType", "path"));
        Assert.Equal("magnet_download", BehaviorCatalog.DefaultFor("textType", "magnet"));
        Assert.Equal("search", BehaviorCatalog.DefaultFor("textType", "plain"));
        Assert.Equal("open_path", BehaviorCatalog.DefaultFor("fileExt", "jpg,webp"));
    }

    /// <summary>覆盖语义 (超集): 用户包 jpg/png 不覆盖含 gif 的规则; 条件值为空按通配处理。</summary>
    [Fact]
    public void Covering_Respects_Superset_Semantics()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), [BehaviorFixtures.PsEdit()]);
        Assert.Contains("ps_edit", BehaviorCatalog.Covering("fileExt", "jpg,png").Select(p => p.Id));
        Assert.DoesNotContain("ps_edit",
            BehaviorCatalog.Covering("fileExt", "jpg,png,gif").Select(p => p.Id));
        // 空条件值 → 按通配处理: 专属包 (ps_edit) 不覆盖, 仅通配内置行为可见
        Assert.DoesNotContain("ps_edit", BehaviorCatalog.Covering("fileExt", "").Select(p => p.Id));
    }

    /// <summary>显示名: 已知 ID 取包名, 未知 ID 回退原值 (脏值恒可见)。
    /// 不切换 I18n.Language —— 与 I18nResourceTests 存在并行串扰风险, 语言路径由其独占覆盖。</summary>
    [Fact]
    public void LabelFor_Falls_Back_To_Id_For_Unknown()
    {
        BehaviorCatalog.SeedForTests([new BehaviorPack
        {
            Id = "ps_edit", Name = "PS 编辑图片", NameEn = "PS Edit", SpecVersion = 1,
            AppliesTo = [new BehaviorAppliesTo { Type = "fileExt", Exts = ["jpg"] }],
            Entry = new BehaviorEntry { Kind = "builtin", Action = "run" },
            Source = "user",
        }], []);
        Assert.Contains(BehaviorCatalog.LabelFor("ps_edit"), new[] { "PS 编辑图片", "PS Edit" });
        Assert.Equal("ghost_id", BehaviorCatalog.LabelFor("ghost_id"));
    }

    /// <summary>无参语义: 内置语义集 + 未声明模板的包; 默认模板由包声明 (search/run)。</summary>
    [Fact]
    public void NoValue_And_Template_Follow_Pack_Entry()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), [BehaviorFixtures.PsEdit()]);
        Assert.True(BehaviorCatalog.IsNoValue("open_url"));
        Assert.True(BehaviorCatalog.IsNoValue("copy")); // 包未声明模板
        Assert.False(BehaviorCatalog.IsNoValue("search"));
        Assert.False(BehaviorCatalog.IsNoValue("ps_edit"));
        Assert.Equal("https://www.google.com/search?q=%selected%", BehaviorCatalog.DefaultTemplateFor("search"));
        Assert.Equal("%selected%", BehaviorCatalog.DefaultTemplateFor("run"));
        Assert.Equal("", BehaviorCatalog.DefaultTemplateFor("copy"));
    }

    /// <summary>基础动作展开: 内置 ID 直通, 用户包取 entry.action (ShowWorkingDir 依赖此判定)。</summary>
    [Fact]
    public void BaseActionOf_Resolves_User_Pack_Entry()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), [BehaviorFixtures.PsEdit()]);
        Assert.Equal("run", BehaviorCatalog.BaseActionOf("run"));
        Assert.Equal("run", BehaviorCatalog.BaseActionOf("ps_edit"));
        Assert.Equal("open_url", BehaviorCatalog.BaseActionOf("open_url"));
        Assert.Equal("ghost", BehaviorCatalog.BaseActionOf("ghost"));
    }
}
