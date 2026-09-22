using KeyFlux.Settings.Models;
using MatchType = KeyFlux.Settings.Models.MatchType;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 文件后缀 (fileExt) 相关的单元测试 (聚合卡重构后):
///   - 文件卡类型 toggle 由 Config.FileGroups + Config.MatchTypes(kind=fileExt) 动态生成;
///   - 分组 toggle 的「已配置」判定按后缀集等价 (SameExts) 而非字面匹配 —— 旧「分组写回 / 行内分组 Toggle /
///     前提快照重绑」机制已随聚合卡重构移除 (见 SelectedActionCardInvariantTests / TypeCardVm);
///   - ActionSchemeCatalog.NormalizeExts / SameExts 纯函数守卫 (新代码 IsCovered 仍依赖)。
/// 纯 ViewModel 测试 (无 Avalonia 宿主)。
/// </summary>
[Collection("I18nSerial")]
public sealed class FileGroupActionFilterTests
{
    private static readonly string[] ImageExts = ["jpg", "jpeg", "png", "gif", "bmp", "webp", "svg", "ico"];
    private static readonly string[] CodeExts = ["c", "cpp", "h", "py", "go", "rs", "json"];

    private static void EnsureBehaviorCatalog()
        => BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);

    private static (SelectedActionPageViewModel Page, Config Config) CreatePage(Config config)
    {
        EnsureBehaviorCatalog();
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = config;
        return (new SelectedActionPageViewModel(main), config);
    }

    // ------------------------------------------------------------- 文件卡 toggle 生成

    /// <summary>文件卡 toggle = 每个 FileGroup 一个 (group:&lt;name&gt;) + 每个 kind=fileExt 自定义类型一个 (type:&lt;id&gt;)。</summary>
    [Fact]
    public void FileCard_Generates_Toggle_Per_FileGroup_And_Custom_FileExt_Type()
    {
        var config = new Config
        {
            FileGroups =
            [
                new FileGroup { Name = "image", Label = "图片", Exts = [.. ImageExts] },
                new FileGroup { Name = "code", Label = "代码", Exts = [.. CodeExts] },
            ],
            MatchTypes = [new MatchType { Id = "x1", Kind = "fileExt", Label = "自定义后缀" }],
        };
        var (page, _) = CreatePage(config);

        var ids = page.FileCard.Toggles.Select(t => t.Id).ToList();
        Assert.Contains("group:image", ids);
        Assert.Contains("group:code", ids);
        Assert.Contains("type:x1", ids);
        // 顺序: 分组在前, 自定义类型在后
        Assert.True(ids.IndexOf("group:code") < ids.IndexOf("type:x1"));
    }

    /// <summary>文本卡 toggle = 内置 TextTypes (plain 恒末) + kind=text 自定义类型。</summary>
    [Fact]
    public void TextCard_Toggles_Follow_Registry_And_Custom_TextTypes()
    {
        var config = new Config
        {
            MatchTypes = [new MatchType { Id = "t1", Kind = "text", Label = "自定义文本" }],
        };
        var (page, _) = CreatePage(config);

        var ids = page.TextCard.Toggles.Select(t => t.Id).ToList();
        Assert.Equal(ActionSchemeCatalog.TextTypes.Select(t => t.Value), ids.Take(ActionSchemeCatalog.TextTypes.Length));
        Assert.Equal("plain", ids[^2]); // plain 恒居内置末位
        Assert.Contains("type:t1", ids);
    }

    /// <summary>分组 toggle 的「已配置」按后缀集等价判定: MatchValue ≡ 分组全集 -> 覆盖; 子集 -> 不覆盖。</summary>
    [Fact]
    public void Group_Toggle_Configured_By_ExtSet_Equivalence_Not_Literal()
    {
        var config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = [.. ImageExts] }],
            SelectedAction = new SelectedAction
            {
                Mappings =
                [
                    new SelectedMapping
                    {
                        MatchType = "fileExt", MatchValue = string.Join(", ", ImageExts),
                        Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }],
                    },
                ],
            },
        };
        var (page, _) = CreatePage(config);

        // MatchValue ≡ 分组全集 -> 分组 toggle 视为已配置 (小圆点)
        Assert.True(page.FileCard.IsTypeConfigured("group:image"));

        // 改写为子集 -> 不再被该分组 toggle 覆盖 (保留为孤儿 toggle, 不被隐藏)
        config.SelectedAction.Mappings[0].MatchValue = "jpg, png";
        page.RebuildCards();
        Assert.False(page.FileCard.IsTypeConfigured("group:image"));
        Assert.Contains("orphan:0", page.FileCard.Toggles.Select(t => t.Id));
    }

    /// <summary>fileExt 映射行编辑器: 行为下拉 = 通配覆盖集 6 项 (文本专属行为不出现)。</summary>
    [Fact]
    public void FileExt_MappingRow_Shows_Generic_Covering_Set()
    {
        var config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = [.. ImageExts] }],
        };
        var (page, _) = CreatePage(config);
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "fileExt",
            MatchValue = string.Join(", ", ImageExts),
            Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }],
        });
        row.OpenEditor();
        Assert.Equal(["copy", "open", "open_folder", "open_path", "run", "script"],
            row.Editors[0].BehaviorOptions.Select(o => o.Value));
    }

    // ------------------------------------------------------------- NormalizeExts

    /// <summary>混合分隔符/前导点/空 token/重复 (忽略大小写保留首个书写形式)。</summary>
    [Fact]
    public void NormalizeExts_Handles_Separators_Points_Empty_And_Dupes()
    {
        Assert.Equal(new[] { "JPG", "png", "gif", "webp" },
            ActionSchemeCatalog.NormalizeExts("JPG, .png,, gif; webp"));
        Assert.Equal(new[] { "jpg" }, ActionSchemeCatalog.NormalizeExts("jpg、JPG"));   // 去重保留首个
        Assert.Equal(new[] { "a", "b" }, ActionSchemeCatalog.NormalizeExts("，a;；b、")); // 全角分隔符
        Assert.Empty(ActionSchemeCatalog.NormalizeExts(""));
        Assert.Empty(ActionSchemeCatalog.NormalizeExts(null));
        Assert.Empty(ActionSchemeCatalog.NormalizeExts("  , ., ; "));
    }

    /// <summary>F4: NormalizeExts 两端去点 (含尾点+尾随空格); SameExts 双侧归一化。</summary>
    [Fact]
    public void NormalizeExts_Trims_Both_Ends_And_SameExts_Normalizes_Both_Sides()
    {
        Assert.Equal(new[] { "jpg", "png" }, ActionSchemeCatalog.NormalizeExts(".jpg, png."));
        Assert.Equal(new[] { "jpg" }, ActionSchemeCatalog.NormalizeExts("jpg. ")); // 尾点死条目
        // 双侧归一化: 带点书写与归一化值视为一致
        Assert.True(ActionSchemeCatalog.SameExts(new[] { "jpg", "png" }, new[] { ".jpg", "png." }));
        Assert.True(ActionSchemeCatalog.SameExts(new[] { ".jpg." }, new[] { "jpg" }));
        Assert.False(ActionSchemeCatalog.SameExts(new[] { "jpg" }, new[] { "png" }));
    }
}
