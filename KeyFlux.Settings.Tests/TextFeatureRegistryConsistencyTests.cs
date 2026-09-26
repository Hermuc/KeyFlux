using System.Text.Json;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 内置文本特征注册表的**三端一致性**守护 (界面侧)。
///
/// 背景: 文本特征的命中语义由 Go (config-server/internal/behaviors/textfeatures.go 注册表) 与
/// AHK (SelectedAction.ahk 的 TextFeatureSpecs) 双端实现, 界面侧 <see cref="ActionSchemeCatalog.TextTypes"/>
/// 是第三份镜像。三份之间的顺序与取值必须恒等 —— 否则映射行 Toggle、添加映射下拉、保存校验与
/// 运行时命中会各说各话 (2026-09-17 之前全靠注释约定, 每加一个特征都要人肉对三处)。
///
/// 契约真源 = <c>config-server/internal/script/testdata/text_types.json</c> 的 <c>types</c>:
/// 同一份 JSON 也是 Go 测试 (texttype_vector_test.go) 与 AHK 运行时对账
/// (tools/texttype_conformance.py, make check-texttypes) 的输入。本测试把 C# 镜像钉进同一契约。
/// </summary>
public sealed class TextFeatureRegistryConsistencyTests
{
    /// <summary>仓库根: 从测试输出目录逐级向上找含 config-ui-avalonia 的目录 (同 I18nResourceTests)。</summary>
    private static string RepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException(
            "找不到仓库根 (含 config-ui-avalonia 的目录), BaseDirectory=" + AppContext.BaseDirectory);
    }

    private static string VectorPath()
        => Path.Combine(RepoRoot(), "config-server", "internal", "script", "testdata", "text_types.json");

    private static string[] VectorTypes()
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(VectorPath()));
        return doc.RootElement.GetProperty("types")
            .EnumerateArray().Select(e => e.GetString()!).ToArray();
    }

    private static string[] MirrorTypes()
        => ActionSchemeCatalog.TextTypes.Select(t => t.Value).ToArray();

    [Fact]
    public void Types_Order_Matches_Shared_Vector()
    {
        var expected = VectorTypes();
        Assert.Equal(expected, MirrorTypes());
    }

    [Fact]
    public void Fallback_Is_Unique_And_Last()
    {
        // 兜底特征 (plain = "其余具名特征全不命中" 的派生兜底) 必须唯一且居末:
        // 它排末位才能保证"新增特征追加后兜底仍在最后"的界面顺序稳定。
        var mirror = MirrorTypes();
        Assert.Equal(ActionSchemeCatalog.FallbackTextType, mirror[^1]);
        Assert.Equal(1, mirror.Count(t => t == ActionSchemeCatalog.FallbackTextType));

        // 与共享向量一致
        Assert.Equal(VectorTypes()[^1], mirror[^1]);
    }

    [Fact]
    public void Vector_Must_Exist_With_Cases()
    {
        // 向量是三端契约的载体: 缺文件 / 空用例都意味着契约被破坏 (而非环境问题)。
        Assert.True(File.Exists(VectorPath()), "共享向量缺失: " + VectorPath());
        using var doc = JsonDocument.Parse(File.ReadAllText(VectorPath()));
        var count = doc.RootElement.GetProperty("cases").GetArrayLength();
        Assert.True(count >= 40, $"向量用例 {count} < 40 (边界覆盖不足)");
        Assert.Equal(1, doc.RootElement.GetProperty("version").GetInt32());
    }
}
