using System.Text.Json;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 命令框字体配置 (config.options.commandFont) 的契约与边界守护。
///
/// 该段由设置页「选项」→「命令框字体」卡写入, 由 **Go 生成端**读取并把 sourcePath
/// 指向的文件复制到 <c>bin/font/font.ttf</c> (命令框 exe 内烧录的字面量, 唯一字体来源,
/// 见 CONTRACTS §3.11.1)。故这里守住三件事:
///   ① C# 模型的 json 键名与 Go struct 的 tag 一致 (跨语言 wire 契约);
///   ② 读取默认值注入 —— 旧配置缺段时补齐, 不产生 null 崩溃;
///   ③ 字重取值非法时回退默认档位 (配置文件被手改坏 / 旧版本写入未知档位)。
/// </summary>
public sealed class CommandFontContractTests
{
    private static readonly JsonSerializerOptions Opts = SettingsJson.Options;

    [Fact]
    public void CommandFontOption_JsonNames_MatchGoTags()
    {
        var json = JsonSerializer.Serialize(new CommandFontOption(), Opts);
        using var doc = JsonDocument.Parse(json);
        Assert.Equal(
            new HashSet<string> { "sourcePath", "weight" },
            doc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet());
    }

    /// <summary>写盘省略空值语义: 空串字段仍输出 (非 omitempty), 与 Go 侧零值序列化一致。</summary>
    [Fact]
    public void CommandFontOption_EmptyFields_StillSerialize()
    {
        var json = JsonSerializer.Serialize(new CommandFontOption { SourcePath = "", Weight = "" }, Opts);
        using var doc = JsonDocument.Parse(json);
        Assert.Equal("", doc.RootElement.GetProperty("sourcePath").GetString());
        Assert.Equal("", doc.RootElement.GetProperty("weight").GetString());
    }

    /// <summary>缺段 (旧配置) 时读时补齐: 段非 null, 路径为空 (= 沿用现有字体), 字重为默认档。</summary>
    [Fact]
    public void Apply_MissingSection_FillsDefaults()
    {
        var config = new Config();
        config.Options.CommandFont = null;

        var result = ConfigReadDefaults.Apply(config);

        Assert.NotNull(result.Options.CommandFont);
        Assert.Equal("", result.Options.CommandFont.SourcePath);
        Assert.Equal(ConfigReadDefaults.DefaultCommandFontWeight, result.Options.CommandFont.Weight);
    }

    /// <summary>
    /// 已有段时**不覆盖用户数据**: 即使用户把路径清空、档位选了 bold, Apply 也必须原样保留
    /// (同 Acrylic 的「只在整段缺失时补」口径, 不逐字段覆盖)。
    /// </summary>
    [Fact]
    public void Apply_ExistingSection_PreservesUserValues()
    {
        var config = new Config();
        config.Options.CommandFont = new CommandFontOption { SourcePath = "", Weight = "bold" };

        var result = ConfigReadDefaults.Apply(config);

        Assert.NotNull(result.Options.CommandFont);
        Assert.Equal("", result.Options.CommandFont.SourcePath);
        Assert.Equal("bold", result.Options.CommandFont.Weight);
    }

    /// <summary>往返: 段内容经序列化/反序列化不丢字段 (PUT/GET wire 契约)。</summary>
    [Fact]
    public void CommandFont_RoundTrips()
    {
        var original = new CommandFontOption { SourcePath = @"D:\fonts\My Font.ttf", Weight = "semibold" };
        var json = JsonSerializer.Serialize(original, Opts);
        var back = JsonSerializer.Deserialize<CommandFontOption>(json, Opts);

        Assert.NotNull(back);
        Assert.Equal(original.SourcePath, back.SourcePath);
        Assert.Equal(original.Weight, back.Weight);
    }

    /// <summary>
    /// 出厂默认字重 = **半粗** (2026-09-21 由 regular 改定, 用户要求「把当前设置的字体设置为
    /// 默认字体, 粗细要确保一致」)。
    ///
    /// 🔴 这个值把三处行为绑在一起, 漏改一处即产生"默认值漂移": ① 配置缺段时的读取默认值;
    /// ② <see cref="ConfigReadDefaults.NormalizeFontWeight"/> 的回落; ③ 「恢复默认」按钮落点。
    /// 用户若从 UI 换档, 本断言会立刻红 —— 那是**提醒**去同步默认值, 而不是测试过时。
    /// </summary>
    [Fact]
    public void DefaultWeight_IsSemibold()
    {
        Assert.Equal("semibold", ConfigReadDefaults.DefaultCommandFontWeight);

        // ① 缺段补齐走同一常量
        var config = new Config();
        config.Options.CommandFont = null;
        Assert.Equal("semibold", ConfigReadDefaults.Apply(config).Options.CommandFont!.Weight);

        // ② 规范化回落走同一常量
        Assert.Equal("semibold", ConfigReadDefaults.NormalizeFontWeight(null));
        Assert.Equal("semibold", ConfigReadDefaults.NormalizeFontWeight("不存在的档位"));
    }

    /// <summary>字重规范化: 空/未知值一律回退默认档, 合法档位原样保留 (边界情况)。</summary>
    [Theory]
    [InlineData(null, "semibold")]
    [InlineData("", "semibold")]
    [InlineData("REGULAR", "semibold")] // 大小写敏感 (值域是小写标识)
    [InlineData("ultra-black", "semibold")]
    [InlineData("medium", "semibold")] // 旧档名已移除, 必须回落而不是留脏值
    [InlineData("thin", "thin")]
    [InlineData("light", "light")]
    [InlineData("regular", "regular")]
    [InlineData("semibold", "semibold")]
    [InlineData("bold", "bold")]
    public void NormalizeFontWeight_FallsBackToDefault_OnInvalid(string? input, string expected)
        => Assert.Equal(expected, ConfigReadDefaults.NormalizeFontWeight(input));

    /// <summary>档位白名单与 UI 下拉候选一一对应 (漏一档会让用户选了之后被规范化吃掉)。</summary>
    [Fact]
    public void CommandFontWeights_Whitelist_MatchesUiOptions()
    {
        Assert.Equal(
            new[] { "thin", "light", "regular", "semibold", "bold" },
            ConfigReadDefaults.CommandFontWeights);
        Assert.Contains(ConfigReadDefaults.DefaultCommandFontWeight, ConfigReadDefaults.CommandFontWeights);
    }

    /// <summary>平台存储路径转换: file:// URI -> Windows 本地路径 (Go 生成端按此读源文件)。</summary>
    [Fact]
    public void Uri_LocalPath_YieldsWindowsPath()
        => Assert.Equal(
            @"D:\fonts\My Font.ttf",
            new Uri(@"file:///D:/fonts/My%20Font.ttf").LocalPath);
}
