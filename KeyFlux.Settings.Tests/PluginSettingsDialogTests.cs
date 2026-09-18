using System.Text.Json;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 「声明式插件设置」契约测试 (C# 侧):
///   - manifest.settings 与 GET/PUT /api/plugins/:id/settings 的 wire 格式能按 DTO 还原
///     (字段名漂移 = 界面拿到空标签/空类型, 编译期查不出);
///   - 仓库里**真实**的 plugin.json 能被解析 (与 I18nResourceTests 读源 i18n.json 同款思路:
///     直接拿真源当夹具, 而不是手抄一份可能与真源脱节的副本);
///   - 行 VM 的类型派生 / 长度上限 / 上下限提示 / 即时校验;
///   - 无后端时 LoadAsync / SaveAsync 安全降级 (不抛异常)。
/// 归入 I18nSerial: Label/Hint 的取值依赖全局 I18n.Language。
/// </summary>
[Collection("I18nSerial")]
public sealed class PluginSettingsDialogTests
{
    // ---------------------------------------------------------------- 真源夹具

    private static string RepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException("找不到仓库根, BaseDirectory=" + AppContext.BaseDirectory);
    }

    private static string EverythingPluginJson() =>
        Path.Combine(RepoRoot(), "plugins", "examples", "everything_search", "plugin.json");

    private static readonly JsonSerializerOptions Json = new() { PropertyNameCaseInsensitive = true };

    /// <summary>① 仓库里真实存在的 everything_search/plugin.json 能按 C# DTO 完整还原。</summary>
    [Fact]
    public void Real_Plugin_Manifest_Deserializes()
    {
        var path = EverythingPluginJson();
        Assert.True(File.Exists(path), $"缺少插件 manifest: {path}");
        var m = JsonSerializer.Deserialize<PluginManifest>(File.ReadAllText(path), Json)!;

        Assert.Equal("everything_search", m.Id);
        Assert.Equal(1, m.SpecVersion);
        Assert.Equal("main.ahk", m.Entry.File);
        Assert.Equal("EverythingSearchMain", m.Entry.Func);

        // 声明了 settings 就必须申请 settings 权限 (Go ValidateManifest 的同一不变量;
        // C# 侧断言一遍是为了让「手改 manifest 忘了权限」在更早的闸门红掉)
        Assert.NotNull(m.Settings);
        Assert.Contains("settings", m.Permissions ?? []);

        var byKey = m.Settings!.ToDictionary(s => s.Key);
        Assert.Equal(
            new[] { "triggerKey", "everythingPath", "esPath", "limit" },
            m.Settings.Select(s => s.Key).ToArray());

        // 触发键: char + 默认空格 (空格是单字符但 string.IsNullOrEmpty 判不出来, 必须显式断言)
        Assert.Equal(PluginSettingTypes.Char, byKey["triggerKey"].Type);
        Assert.Equal(" ", byKey["triggerKey"].Default);

        // 路径项: file + 过滤名 (浏览按钮据此过滤)
        Assert.Equal(PluginSettingTypes.File, byKey["everythingPath"].Type);
        Assert.Equal("everything.exe", byKey["everythingPath"].Filter);
        Assert.Equal("es.exe", byKey["esPath"].Filter);

        // 条数上限: number + 闭区间 (界面据此显示区间徽标, 后端据此校验)
        Assert.Equal(PluginSettingTypes.Number, byKey["limit"].Type);
        Assert.Equal("20", byKey["limit"].Default);
        Assert.Equal(1, byKey["limit"].Min);
        Assert.Equal(100, byKey["limit"].Max);

        // 每个键都要有中英标签与提示 —— 界面渲染直接吃这四个字段
        foreach (var s in m.Settings)
        {
            Assert.False(string.IsNullOrWhiteSpace(s.Label), $"{s.Key} 缺 label");
            Assert.False(string.IsNullOrWhiteSpace(s.LabelEn), $"{s.Key} 缺 labelEn");
            Assert.False(string.IsNullOrWhiteSpace(s.Hint), $"{s.Key} 缺 hint");
            Assert.False(string.IsNullOrWhiteSpace(s.HintEn), $"{s.Key} 缺 hintEn");
        }
    }

    /// <summary>② 设置端点响应体 (Go pluginSettingsDTO) 能按 DTO 还原, 含声明与值两张表。</summary>
    [Fact]
    public void Settings_Response_Deserializes()
    {
        // 与 Go server/plugins.go pluginSettingsDTO 的字段名逐一对齐 (json tag)
        const string wire = """
        {
          "id": "everything_search",
          "settings": [
            {"key": "triggerKey", "type": "char", "label": "前置触发键", "labelEn": "Trigger key",
             "default": " ", "hint": "h", "hintEn": "he"},
            {"key": "limit", "type": "number", "label": "结果条数上限", "labelEn": "Max results",
             "default": "20", "min": 1, "max": 100}
          ],
          "values": {"triggerKey": ";", "limit": "50", "everythingPath": ""}
        }
        """;
        var r = JsonSerializer.Deserialize<PluginSettingsResponse>(wire, Json)!;

        Assert.Equal("everything_search", r.Id);
        Assert.Equal(2, r.Settings.Count);
        Assert.Equal(1d, r.Settings[1].Min!.Value);
        Assert.Equal(100d, r.Settings[1].Max!.Value);
        Assert.Equal(";", r.Values["triggerKey"]);
        Assert.Equal("50", r.Values["limit"]);

        // PUT 请求体同样要对得上 (Go saveSettingsRequest 只认 values 包裹层)
        var req = JsonSerializer.Serialize(new PluginSettingsRequest { Values = { ["a"] = "1" } }, Json);
        using var doc = JsonDocument.Parse(req);
        Assert.Equal("1", doc.RootElement.GetProperty("values").GetProperty("a").GetString());
    }

    // ---------------------------------------------------------------- 行 VM

    private static PluginSetting Setting(string key, string type, double? min = null, double? max = null, int maxLength = 0)
        => new() { Key = key, Type = type, Label = "标签", LabelEn = "Label", Min = min, Max = max, MaxLength = maxLength };

    /// <summary>③ 类型派生: 四个标志互斥且完备, 未知类型回落 text。</summary>
    [Fact]
    public void Row_Type_Flags_Are_Exclusive_And_Total()
    {
        var cases = new (string Type, bool IsChar, bool IsFile, bool IsNumber, bool IsText)[]
        {
            (PluginSettingTypes.Char, true, false, false, false),
            (PluginSettingTypes.File, false, true, false, false),
            (PluginSettingTypes.Number, false, false, true, false),
            (PluginSettingTypes.Text, false, false, false, true),
            ("something_unknown", false, false, false, true), // 未知类型按 text (不渲染空白行)
        };
        foreach (var c in cases)
        {
            var row = new PluginSettingRowVm(Setting("k", c.Type), "");
            Assert.Equal(c.IsChar, row.IsChar);
            Assert.Equal(c.IsFile, row.IsFile);
            Assert.Equal(c.IsNumber, row.IsNumber);
            Assert.Equal(c.IsText, row.IsText);
        }
    }

    /// <summary>④ 长度上限: char 恒 1, text 用声明的 maxLength, file/number 用后端上限兜底。</summary>
    [Fact]
    public void Row_MaxLength_Matches_Backend_ValueLimit()
    {
        Assert.Equal(1, new PluginSettingRowVm(Setting("k", PluginSettingTypes.Char), "").MaxLength);
        Assert.Equal(32, new PluginSettingRowVm(Setting("k", PluginSettingTypes.Text, maxLength: 32), "").MaxLength);
        Assert.Equal(1024, new PluginSettingRowVm(Setting("k", PluginSettingTypes.Text), "").MaxLength);
        Assert.Equal(1024, new PluginSettingRowVm(Setting("k", PluginSettingTypes.File), "").MaxLength);
    }

    /// <summary>⑤ 区间徽标: 单边/无边界的呈现。</summary>
    [Fact]
    public void Row_RangeHint_And_HasRange()
    {
        var both = new PluginSettingRowVm(Setting("k", PluginSettingTypes.Number, 1, 100), "");
        Assert.True(both.HasRange);
        Assert.Contains("1", both.RangeHint);
        Assert.Contains("100", both.RangeHint);

        var none = new PluginSettingRowVm(Setting("k", PluginSettingTypes.Number), "");
        Assert.False(none.HasRange);
        Assert.Equal("", none.RangeHint);

        // 非 number 类型即便误带 min/max 也不显示区间 (manifest 校验会拒绝这种声明, 这里只是不炸)
        var wrong = new PluginSettingRowVm(Setting("k", PluginSettingTypes.Text, 1, 2), "");
        Assert.False(wrong.HasRange);
    }

    /// <summary>⑥ 空格回显: 单字符框里看不出「值是空格」, 需要可读回显。</summary>
    [Fact]
    public void Row_ShowSpaceToken()
    {
        var row = new PluginSettingRowVm(Setting("k", PluginSettingTypes.Char), " ");
        Assert.True(row.ShowSpaceToken);
        row.Value = "";           // 清空 = 回落默认值 (空格), 仍应回显
        Assert.True(row.ShowSpaceToken);
        row.Value = ";";
        Assert.False(row.ShowSpaceToken);
        // 非 char 类型不出现该回显 (值的空白与否对文本/路径项没有特殊含义)
        Assert.False(new PluginSettingRowVm(Setting("k", PluginSettingTypes.Text), " ").ShowSpaceToken);
    }

    /// <summary>⑦ 即时校验与后端 ValidateSettingValue 同口径 (空串恒合法 = 清除覆盖)。</summary>
    [Fact]
    public void Row_Validate_Matches_Backend_Rules()
    {
        var charRow = new PluginSettingRowVm(Setting("k", PluginSettingTypes.Char), " ");
        Assert.Null(charRow.Validate());
        charRow.Value = "\t";
        Assert.NotNull(charRow.Validate());      // 控制字符做不了触发键
        charRow.Value = "";                       // 空 = 清除覆盖
        Assert.Null(charRow.Validate());

        var num = new PluginSettingRowVm(Setting("k", PluginSettingTypes.Number, 1, 100), "20");
        Assert.Null(num.Validate());
        foreach (var bad in new[] { "abc", "20.5", "0", "101" })
        {
            num.Value = bad;
            Assert.NotNull(num.Validate());
        }
        num.Value = "100";                        // 边界含在内
        Assert.Null(num.Validate());

        var file = new PluginSettingRowVm(Setting("k", PluginSettingTypes.File), @"D:\Everything\everything.exe");
        Assert.Null(file.Validate());
        file.Value = new string('a', 2000);        // 超后端上限
        Assert.NotNull(file.Validate());
    }

    /// <summary>⑧ 标签/提示随语言切换 (英文界面优先 labelEn/hintEn, 缺失回退中文)。</summary>
    [Fact]
    public void Row_Label_And_Hint_Follow_Language()
    {
        var original = I18n.Language;
        try
        {
            var s = new PluginSetting { Key = "k", Type = PluginSettingTypes.Text, Label = "中文标签", LabelEn = "En label", Hint = "中文提示", HintEn = "En hint" };
            var row = new PluginSettingRowVm(s, "");

            I18n.Language = I18n.Zh;
            Assert.Equal("中文标签", row.Label);
            Assert.Equal("中文提示", row.Hint);

            I18n.Language = I18n.En;
            Assert.Equal("En label", row.Label);
            Assert.Equal("En hint", row.Hint);

            // 英文缺失时回退中文 (同 nameEn 口径)
            var noEn = new PluginSettingRowVm(new PluginSetting { Key = "k", Label = "只有中文" }, "");
            Assert.Equal("只有中文", noEn.Label);
        }
        finally
        {
            I18n.Language = original;
        }
    }

    // ---------------------------------------------------------------- 无后端降级

    /// <summary>⑨ 未连接后端时 LoadAsync 不抛异常, 落到 LoadError 且表单不呈现。</summary>
    [Fact]
    public async Task LoadAsync_Without_Backend_Degrades_Safely()
    {
        var (vm, main) = CreateVm();
        Assert.Null(main.Session.Api); // 前提: 未连接

        await vm.LoadAsync();

        Assert.False(vm.IsLoading);
        Assert.NotNull(vm.LoadError);
        Assert.False(vm.ShowForm);
        Assert.False(vm.ShowEmptyState); // 加载失败 ≠ 没有配置项
        Assert.Empty(vm.Rows);
    }

    /// <summary>⑩ 未连接后端时 SaveAsync 安全返回 false (不置 Saved)。</summary>
    [Fact]
    public async Task SaveAsync_Without_Backend_Returns_False()
    {
        var (vm, _) = CreateVm();
        vm.Rows.Add(new PluginSettingRowVm(Setting("k", PluginSettingTypes.Text), "v"));

        Assert.False(await vm.SaveAsync());
        Assert.False(vm.Saved);
    }

    private static (PluginSettingsDialogViewModel Vm, MainViewModel Main) CreateVm()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        var manifest = new PluginManifest
        {
            Id = "everything_search",
            Name = "Everything 搜索",
            NameEn = "Everything Search",
            Settings = [Setting("limit", PluginSettingTypes.Number, 1, 100)],
        };
        return (new PluginSettingsDialogViewModel(main, manifest), main);
    }

    /// <summary>⑪ 卡片「可配置」判定: 内置卡恒可配置, 用户卡看是否声明了设置。</summary>
    [Fact]
    public void Card_CanConfigure_Requires_Settings_For_User_Plugins()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        var page = new PluginsPageViewModel(main);

        var withSettings = new PluginCardVm(page, new PluginManifest
        {
            Id = "everything_search",
            Name = "Everything 搜索",
            Settings = [Setting("limit", PluginSettingTypes.Number)],
        });
        Assert.True(withSettings.CanConfigure);
        Assert.True(withSettings.CanDelete);   // 用户卡仍可删除

        var without = new PluginCardVm(page, new PluginManifest { Id = "bare", Name = "裸插件" });
        Assert.False(without.CanConfigure);

        Assert.True(new PluginCardVm(page, new PluginManifest { Id = "quick_switch", Name = "快速切换" }, isBuiltin: true).CanConfigure);
    }
}
