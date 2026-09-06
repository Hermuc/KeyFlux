using System.Reflection;
using System.Text.Json;
using System.Text.RegularExpressions;
using MyKeymap.Settings.Services;

namespace MyKeymap.Settings.Tests;

/// <summary>
/// Resources/i18n.json (I18n 文案表外置) 的守卫测试。
/// 外置把「漏键」从编译期错误降级成运行时数据错误, 这些断言是唯一的网:
/// 新增文案却忘了写进 JSON、改 JSON 时误伤 null / "" 语义、文案表根本没被复制到
/// 输出目录 (加载器静默降级成空表), 都会在这里变红。
/// 刻意不依赖 Avalonia 宿主 —— 这正是选松散 JSON 而非 avares:// 的原因。
/// </summary>
/// <remarks>
/// 本类会改写全局静态 <see cref="I18n.Language"/>, 每个用例都在 finally 里还原;
/// xunit 同类内串行, 且其余测试类对 I18n 的引用数为 0, 故不存在并行串扰。
/// </remarks>
public sealed class I18nResourceTests
{
    /// <summary>外置前 C# 字典的键数: 307 个数字键 + 301err / 301hint 两个非数字键;
    /// 2026-09 移除 default 匹配类型相关 4 键后为 303 数字键 + 2 非数字键;
    /// 2026-09 行为库新增 20 个数字键 (1083-1100 与 1103/1104) + 4 个非数字键 (1101_applied/1102_deleted/1103_only/1104_any)。
    /// 2026-09 移除方案卡片「编辑」按钮词条 966 (点卡片即编辑, 按钮冗余) -> 328;
    /// 2026-09 方案 D 重构: 删 52 键 (多方案列表/方案名/导入导出/两级导航 959-1028 段 +
    /// 旧静态行为词表 1037-1058), 增 10 键 (1105-1114 选中动作单屏页) -> 286;
    /// 2026-09 复核恢复 1025 (热键冲突提示, 保留控件 HotkeyCapture 仍引用, 重构误删) -> 287;
    /// 2026-09 死代码清理: 删行为库迭代遗留孤儿 6 键 (994/1007/1009/1010/1084/1100,
    /// cs/axaml/配置/Go 全域零引用, 见 89708f4) -> 282。</summary>
    private const int ExpectedKeyCount = 282;

    private const string LabelPrefix = "label:";

    /// <summary>9 个英文专属键名 (原 I18n.cs 里 zh 写作 null 的那批)。</summary>
    private static readonly string[] EnglishOnlyKeys = ["62", "63", "64", "65", "66", "67", "68", "69", "70"];

    // ---------------------------------------------------------------- 基础设施

    /// <summary>仓库根: 从测试输出目录逐级向上找含 config-ui-avalonia 的目录。</summary>
    private static string RepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException(
            "找不到仓库根 (含 config-ui-avalonia 的目录), BaseDirectory=" + AppContext.BaseDirectory);
    }

    /// <summary>源码树里的文案表 (真源), 与输出目录里的副本分开校验。</summary>
    private static string SourceJsonPath() =>
        Path.Combine(RepoRoot(), "config-ui-avalonia", "Resources", "i18n.json");

    /// <summary>被扫描的 UI 源码目录。</summary>
    private static string UiSourceDir() => Path.Combine(RepoRoot(), "config-ui-avalonia");

    /// <summary>
    /// 文案表原始字典。公有 T() 会把 null 与 "" 一并回退, 无法区分,
    /// 而这两者的区别正是本任务要求 1:1 保真的数据, 故用反射直取私有 Map。
    /// </summary>
    private static Dictionary<string, (string? Zh, string? En)> RawMap()
    {
        var field = typeof(I18n).GetField("Map", BindingFlags.NonPublic | BindingFlags.Static);
        if (field?.GetValue(null) is not Dictionary<string, (string? Zh, string? En)> map)
        {
            throw new InvalidOperationException("取不到 I18n.Map 私有静态字段, 守卫测试需同步调整。");
        }
        return map;
    }

    /// <summary>排除 bin/obj 下的构建产物 (含 Avalonia 生成的中间 .axaml)。</summary>
    private static bool IsBuildOutput(string path)
    {
        var sep = Path.DirectorySeparatorChar;
        return path.Contains($"{sep}bin{sep}", StringComparison.OrdinalIgnoreCase)
            || path.Contains($"{sep}obj{sep}", StringComparison.OrdinalIgnoreCase);
    }

    private static string StripLabel(string key) =>
        key.StartsWith(LabelPrefix, StringComparison.Ordinal) ? key[LabelPrefix.Length..] : key;

    // ---------------------------------------------------------------- 守卫断言

    /// <summary>① 文案表真的被加载了, 且键数与 <see cref="ExpectedKeyCount"/> 一致、无重复。</summary>
    [Fact]
    public void Json_Is_Loaded_Completely_Without_Duplicate_Keys()
    {
        var path = SourceJsonPath();
        Assert.True(File.Exists(path), $"源码树缺少文案表: {path}");

        // KeyCount 由 I18n 加载器给出, 但加载器 Locate() 有「源码树向上回退」(回退②):
        // 在本仓布局下, 即便 csproj 的 Content 项失效、输出目录没有 Resources/i18n.json,
        // 回退②仍会从 BaseDirectory 上溯命中源码树里的 i18n.json, 使 KeyCount 照常等于 ExpectedKeyCount。
        // 因此 KeyCount 无法暴露「Content 项没复制到输出目录」这类故障 (只有生产 beta33\bin\ui
        // 下回退②触不到源码树时才会暴露), 必须由下方 Publish_Output_Contains_Loose_Resource
        // 的物理存在断言兜底 —— 那条不经加载器, 直接检查输出目录里的散部署物。
        Assert.Equal(ExpectedKeyCount, I18n.KeyCount);

        // JSON 解析器对重复键是「后者胜出」而非报错, 必须自己数一遍顶层属性名
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var duplicates = new List<string>();
        var reader = new Utf8JsonReader(File.ReadAllBytes(path));
        while (reader.Read())
        {
            if (reader.TokenType != JsonTokenType.PropertyName || reader.CurrentDepth != 1) continue;
            var name = reader.GetString()!;
            if (!seen.Add(name)) duplicates.Add(name);
        }

        Assert.True(duplicates.Count == 0, "i18n.json 存在重复键: " + string.Join(", ", duplicates));
        Assert.Equal(ExpectedKeyCount, seen.Count);
        Assert.Equal(seen.Count, I18n.KeyCount);
    }

    /// <summary>
    /// ①′ 松散部署物物理存在 (Kim H1) —— 兜底 ① 的「源码树回退」盲区。
    /// 不经 I18n 加载器, 直接断言输出目录里真有 Resources/i18n.json 散文件;
    /// 这样即便 Locate() 的回退②能从源码树读到文案表使 KeyCount 假绿,
    /// 只要 csproj 的 Content 项 CopyToPublishDirectory 失效, 本断言立刻失败。
    /// </summary>
    [Fact]
    public void Publish_Output_Contains_Loose_Resource()
    {
        Assert.True(File.Exists(Path.Combine(AppContext.BaseDirectory, "Resources", "i18n.json")),
            "publish 输出目录缺少松散部署物 Resources/i18n.json——Content 项的 CopyToPublishDirectory 可能失效");
    }

    /// <summary>② 键覆盖对账: 源码里出现的每一个文案键都必须在 JSON 中存在。</summary>
    [Fact]
    public void Every_Key_Referenced_In_Axaml_And_Cs_Exists_In_Json()
    {
        var uiDir = UiSourceDir();
        Assert.True(Directory.Exists(uiDir), uiDir);

        // AXAML: ConverterParameter=NNN / ConverterParameter="NNN"
        var paramRe = new Regex("ConverterParameter\\s*=\\s*\"?([^\"{}\\s,]+)\"?", RegexOptions.CultureInvariant);
        // C#: I18n.T("字面量")。I18n.T(变量) / I18n.T($"...") 是运行时键, 不在静态对账范围内
        var callRe = new Regex("I18n\\.T\\(\\s*\"([^\"]+)\"\\s*\\)", RegexOptions.CultureInvariant);

        var axamlFiles = Directory.GetFiles(uiDir, "*.axaml", SearchOption.AllDirectories)
            .Where(p => !IsBuildOutput(p)).ToArray();
        var csFiles = Directory.GetFiles(uiDir, "*.cs", SearchOption.AllDirectories)
            .Where(p => !IsBuildOutput(p)).ToArray();
        Assert.NotEmpty(axamlFiles);
        Assert.NotEmpty(csFiles);

        var referenced = new SortedSet<string>(StringComparer.Ordinal);
        foreach (var file in axamlFiles)
        {
            var text = File.ReadAllText(file);
            foreach (Match m in paramRe.Matches(text))
            {
                // 只认挂在 {StaticResource Tr} 上的 ConverterParameter (向前开 400 字符窗口),
                // 避开其它转换器可能出现的同名参数造成误报
                var from = Math.Max(0, m.Index - 400);
                var window = text.Substring(from, m.Index + m.Length - from);
                if (!window.Contains("StaticResource Tr}", StringComparison.Ordinal)) continue;
                referenced.Add(m.Groups[1].Value);
            }
        }
        foreach (var file in csFiles)
        {
            foreach (Match m in callRe.Matches(File.ReadAllText(file))) referenced.Add(m.Groups[1].Value);
        }

        // 元守卫: 扫描算法本身必须有效 (canary 分布在 axaml 与 cs 两侧, 且含非数字键)
        foreach (var canary in new[] { "1080", "301hint", "1081", "301err", "950", "1026" })
        {
            Assert.True(referenced.Contains(canary), $"扫描算法失效: 没扫到已知文案键 {canary}");
        }

        var map = RawMap();
        var missing = referenced.Select(StripLabel).Where(k => !map.ContainsKey(k)).ToArray();
        Assert.True(missing.Length == 0,
            $"源码引用了 i18n.json 里不存在的 {missing.Length} 个文案键: " + string.Join(", ", missing));
    }

    /// <summary>③ null 与空串的语义必须原样保留 (外置最容易踩的坑)。</summary>
    [Fact]
    public void Null_And_EmptyString_Semantics_Are_Preserved()
    {
        var map = RawMap();

        // 62-70: Esc / Backspace / Enter / Delete / Insert / Tab / Ctrl+Tab / Shift+Tab / Ctrl+Shift+Tab
        // 键名中英文同形, 原字典 zh 写作 null -> JSON 里必须是 null, 不得省略也不得写成空串
        foreach (var key in EnglishOnlyKeys)
        {
            Assert.True(map.TryGetValue(key, out var entry), $"i18n.json 缺少英文专属键 {key}");
            Assert.True(entry.Zh is null, $"键 {key} 的 zh 必须是 JSON null, 实际是 \"{entry.Zh}\"");
            Assert.False(string.IsNullOrEmpty(entry.En), $"键 {key} 的 en 不应为空");
        }

        // 504「开关」: 原字典 en 是空串 "" 而非 null。T() 靠 string.IsNullOrEmpty 回退,
        // 两者当前行为一致, 但数据必须区分「刻意留空」与「忘了写」
        Assert.True(map.TryGetValue("504", out var toggle), "i18n.json 缺少键 504");
        Assert.Equal("开关", toggle.Zh);
        Assert.True(toggle.En is not null, "键 504 的 en 必须是空串 \"\" 而不是 null");
        Assert.Equal("", toggle.En);

        // 全表口径: 有且仅有这 9 个 zh=null; 没有任何 en=null; 空串只出现在 504 的 en
        Assert.Equal(EnglishOnlyKeys, map.Where(kv => kv.Value.Zh is null).Select(kv => kv.Key).ToArray());
        Assert.Equal(0, map.Count(kv => kv.Value.En is null));
        Assert.Equal(new[] { "504" }, map.Where(kv => kv.Value.En == "").Select(kv => kv.Key).ToArray());
        Assert.Equal(0, map.Count(kv => kv.Value.Zh == ""));
    }

    /// <summary>④ 占位符与转义在 C# 字面量 -> JSON 的搬运中必须无损。</summary>
    [Fact]
    public void Placeholders_And_Escapes_Survive_The_Move_To_Json()
    {
        var map = RawMap();

        // 1109 (方案 D 删除映射确认) / 1102_deleted (行为库删除回显) 由调用方
        // string.Format 填充, 占位符必须原样保留 (969/1023 已随方案 D 删除)
        Assert.Contains("{0}", map["1109"].Zh!);
        Assert.Contains("{0}", map["1109"].En!);
        Assert.Contains("{0}", map["1102_deleted"].Zh!);
        Assert.Contains("{0}", map["1102_deleted"].En!);

        // 两条含转义最长的文案: \" 与 \\ (反斜杠路径) 必须还原成真实字符
        Assert.Contains("\"Exclude\"", map["612"].En!);
        Assert.Contains(@"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\", map["911"].Zh!);
        Assert.Contains("\"Microsoft Edge.lnk\"", map["911"].En!);
    }

    /// <summary>⑤ 语义往返: 中英切换、label: 前缀、缺失回退、未命中回显键名。</summary>
    [Fact]
    public void T_Resolves_Both_Languages_With_Fallback_And_Label_Prefix()
    {
        var original = I18n.Language;
        try
        {
            I18n.Language = I18n.Zh;
            Assert.Equal("名称", I18n.T("501"));
            Assert.Equal(I18n.T("501"), I18n.T("label:501"));
            Assert.Equal("Esc", I18n.T("62"));   // zh=null -> 回退英文
            Assert.Equal("开关", I18n.T("504"));
            Assert.Equal("", I18n.T(""));
            Assert.Equal("", I18n.T(null));
            Assert.Equal("不存在的键", I18n.T("不存在的键"));   // 未命中原样回显键名

            I18n.Language = I18n.En;
            Assert.Equal("Name", I18n.T("501"));
            Assert.Equal(I18n.T("501"), I18n.T("label:501"));
            Assert.Equal("开关", I18n.T("504"));  // en="" -> 回退中文
            Assert.Equal("Esc", I18n.T("62"));
            Assert.Equal("Cannot identify this window. Run the Settings UI as administrator and try again.",
                I18n.T("1081"));
        }
        finally
        {
            I18n.Language = original;
        }
    }

    /// <summary>⑥ 公有契约: Language 归一化 + 值未变不触发 Changed + ApplyConfigLanguage 只认 en。</summary>
    [Fact]
    public void Language_Normalizes_And_Raises_Changed_Only_On_Real_Change()
    {
        var original = I18n.Language;
        var raised = 0;
        void OnChanged() => raised++;

        I18n.Changed += OnChanged;
        try
        {
            I18n.Language = I18n.Zh;              // 归一化后与当前值相同 -> 不触发
            Assert.Equal(I18n.Zh, I18n.Language);
            Assert.Equal(0, raised);

            I18n.Language = "zh-Hans";            // 仅认 "zh" (忽略大小写), 其余一律落 en
            Assert.Equal(I18n.En, I18n.Language);
            Assert.Equal(1, raised);

            I18n.Language = "EN";                 // 未变 -> 不触发
            Assert.Equal(I18n.En, I18n.Language);
            Assert.Equal(1, raised);

            I18n.ApplyConfigLanguage("en");       // 未变 -> 不触发
            Assert.Equal(I18n.En, I18n.Language);
            Assert.Equal(1, raised);

            I18n.ApplyConfigLanguage(null);       // 只认 en, 其余落 zh
            Assert.Equal(I18n.Zh, I18n.Language);
            Assert.Equal(2, raised);

            I18n.ApplyConfigLanguage("fr");
            Assert.Equal(I18n.Zh, I18n.Language);
            Assert.Equal(2, raised);
        }
        finally
        {
            I18n.Changed -= OnChanged;
            I18n.Language = original;
        }
    }

    /// <summary>⑦ 编码契约: UTF-8 无 BOM (BOM 会让 Utf8JsonReader / Deserialize(Stream) 解析失败)。</summary>
    [Fact]
    public void Json_File_Is_Utf8_Without_Bom()
    {
        var bytes = File.ReadAllBytes(SourceJsonPath());
        Assert.True(bytes.Length > 3, "i18n.json 内容异常");
        Assert.False(bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF,
            "i18n.json 必须是 UTF-8 无 BOM; 写入请用 pwsh 7 的 [IO.File]::WriteAllText + UTF8Encoding($false), "
            + "PS 5.1 的 Set-Content -Encoding UTF8 会带 BOM");
        Assert.Equal((byte)'{', bytes[0]);
    }
}
