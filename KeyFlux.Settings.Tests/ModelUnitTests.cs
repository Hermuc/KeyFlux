using System.Text.Json;
using KeyFlux.Settings.Models;
// 避免与 System.Action 歧义
using Action = KeyFlux.Settings.Models.Action;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 纯单元层契约守护 (不需要真实服务):
/// 1. C# 模型的 JSON 属性名与 Go json tag 逐字对齐 (防拼写回归);
/// 2. Go omitempty 字段缺失时反序列化容忍 (可空/默认空集合);
/// 3. 只读路径默认值注入 (对照 config-ui/src/store/config.ts)。
/// </summary>
public sealed class ModelSerializationTests
{
    private static string Serialize(object value) => JsonSerializer.Serialize(value, SettingsJson.Options);

    [Fact]
    public void Config_TopLevel_JsonNames_MatchGoTags()
    {
        var json = Serialize(new Config());
        using var doc = JsonDocument.Parse(json);
        var keys = doc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(
            new HashSet<string> { "keymaps", "options", "selectedAction", "fileGroups", "overviewDocMd" },
            keys);
    }

    [Fact]
    public void Keymap_And_Action_JsonNames_MatchGoTags()
    {
        var km = new Keymap
        {
            Id = 1,
            Name = "主键盘",
            Enable = true,
            Hotkey = "CapsLock",
            ParentId = 0,
            Delay = 0,
            DisableAt = "notepad.exe",
            Hotkeys =
            {
                ["a"] =
                [
                    new Action
                    {
                        WindowGroupId = 1,
                        TypeId = 5,
                        Comment = "备注",
                        Hotkey = "a",
                        KeysToSend = "^c",
                        RemapToKey = "b",
                        ValueId = 2,
                        WinTitle = "ahk_exe x.exe",
                        Target = "cmd.exe",
                        Args = "/c dir",
                        WorkingDir = "C:\\",
                        RunAsAdmin = true,
                        RunInBackground = true,
                        DetectHiddenWindow = true,
                        AhkCode = "MsgBox 1",
                    },
                ],
            },
        };
        var json = Serialize(km);
        using var doc = JsonDocument.Parse(json);
        var kmKeys = doc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(
            new HashSet<string> { "id", "name", "enable", "hotkey", "parentID", "delay", "disableAt", "hotkeys" },
            kmKeys);

        var actionKeys = doc.RootElement.GetProperty("hotkeys").GetProperty("a")[0]
            .EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(
            new HashSet<string>
            {
                "windowGroupID", "actionTypeID", "comment", "hotkey", "keysToSend", "remapToKey",
                "actionValueID", "winTitle", "target", "args", "workingDir", "runAsAdmin",
                "runInBackground", "detectHiddenWindow", "ahkCode",
            },
            actionKeys);
    }

    [Fact]
    public void Options_And_SubStructs_JsonNames_MatchGoTags()
    {
        var json = Serialize(new Options());
        using var doc = JsonDocument.Parse(json);
        var keys = doc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(
            new HashSet<string>
            {
                "hideMatrix", "keyfluxVersion", "windowGroups", "mouse", "scroll",
                "commandInputSkin", "pathVariables", "startup", "language", "keyMapping", "keyboardLayout",
                "quickSwitch",
            },
            keys);

        var qsKeys = doc.RootElement.GetProperty("quickSwitch").EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(
            new HashSet<string>
            {
                "collectEnabled", "autoShow", "autoJumpOpen", "autoJumpSave",
                "pollIntervalMs", "maxHistory", "overlayRows", "overlayRowsCompact", "excludedPrefixes",
            },
            qsKeys);

        var mouseKeys = doc.RootElement.GetProperty("mouse").EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(
            new HashSet<string>
            {
                "keepMouseMode", "showTip", "tipSymbol", "delay1", "delay2",
                "fastSingle", "fastRepeat", "slowSingle", "slowRepeat",
            },
            mouseKeys);

        var scrollKeys = doc.RootElement.GetProperty("scroll").EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(new HashSet<string> { "delay1", "delay2", "onceLineCount" }, scrollKeys);

        var skinKeys = doc.RootElement.GetProperty("commandInputSkin").EnumerateObject().Select(p => p.Name).ToHashSet();
        Assert.Equal(18, skinKeys.Count);
        Assert.Contains("backgroundColor", skinKeys);
        Assert.Contains("hideAnimationDuration", skinKeys);
        Assert.Contains("windowShadowSize", skinKeys);
    }

    [Fact]
    public void SelectedAction_Mapping_Entry_FileGroup_JsonNames_MatchGoTags()
    {
        var sa = new SelectedAction
        {
            Hotkey = ">^p",
            Enable = true,
            Mappings =
            [
                new SelectedMapping
                {
                    MatchType = "fileExt",
                    MatchValue = "jpg",
                    Entries =
                    [
                        new SelectedEntry
                        {
                            Behavior = "open",
                            ActionValue = "x.exe",
                            WorkingDir = "C:\\",
                            Options = new RuleOptions { CopyToClipboard = true, ClearSelection = true, Confirm = true },
                        },
                    ],
                },
            ],
        };
        var json = Serialize(sa);
        using var doc = JsonDocument.Parse(json);
        Assert.Equal(
            new HashSet<string> { "hotkey", "enable", "mappings" },
            doc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet());
        var mapping = doc.RootElement.GetProperty("mappings")[0];
        Assert.Equal(
            new HashSet<string> { "matchType", "matchValue", "entries" },
            mapping.EnumerateObject().Select(p => p.Name).ToHashSet());
        var entry = mapping.GetProperty("entries")[0];
        // 非空时 actionValue/workingDir 均输出
        Assert.Equal(
            new HashSet<string> { "behavior", "actionValue", "workingDir", "options" },
            entry.EnumerateObject().Select(p => p.Name).ToHashSet());
        Assert.Equal(
            new HashSet<string> { "copyToClipboard", "clearSelection", "confirm" },
            entry.GetProperty("options").EnumerateObject().Select(p => p.Name).ToHashSet());

        // 空 actionValue/workingDir omitempty 缺键 (对齐 Go dto_test 契约; options 恒输出)
        var emptyJson = Serialize(new SelectedEntry { Behavior = "open_url" });
        using var emptyDoc = JsonDocument.Parse(emptyJson);
        Assert.Equal(
            new HashSet<string> { "behavior", "options" },
            emptyDoc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet());

        var fgJson = Serialize(new FileGroup { Name = "image", Label = "图片", Exts = ["jpg"] });
        using var fgDoc = JsonDocument.Parse(fgJson);
        Assert.Equal(
            new HashSet<string> { "name", "label", "exts" },
            fgDoc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet());

        var wgJson = Serialize(new WindowGroup { Id = 1, Name = "g", Value = "v", ConditionType = 1 });
        using var wgDoc = JsonDocument.Parse(wgJson);
        Assert.Equal(
            new HashSet<string> { "id", "name", "value", "conditionType" },
            wgDoc.RootElement.EnumerateObject().Select(p => p.Name).ToHashSet());
    }

    [Fact]
    public void Deserialize_ToleratesMissingOmitemptyFields()
    {
        // 模拟旧版 config.json: 缺 selectedAction / fileGroups, Action 仅带必需字段
        const string minimal = """
        {
          "keymaps": [
            { "id": 1, "name": "主", "enable": true, "hotkey": "CapsLock", "parentID": 0, "delay": 0,
              "disableAt": "", "hotkeys": { "a": [ { "windowGroupID": 0, "actionTypeID": 5 } ] } }
          ],
          "options": {
            "hideMatrix": false, "keyfluxVersion": "", "windowGroups": [],
            "mouse": {}, "scroll": {}, "commandInputSkin": {}, "pathVariables": [],
            "startup": false, "language": "", "keyMapping": "", "keyboardLayout": ""
          }
        }
        """;
        var config = JsonSerializer.Deserialize<Config>(minimal, SettingsJson.Options);
        Assert.NotNull(config);
        // selectedAction 缺失时反序列化保持恒对象默认值 (C# 侧属性初始化器, 对齐 Go 侧 MigrateSelectedAction)
        Assert.NotNull(config!.SelectedAction);
        Assert.Empty(config.SelectedAction.Mappings);
        Assert.Empty(config.FileGroups);
        Assert.Single(config.Keymaps);
        var action = config.Keymaps[0].Hotkeys["a"][0];
        Assert.Equal(5, action.TypeId);
        Assert.Equal("", action.Comment);
        Assert.False(action.RunAsAdmin);
    }
}

public sealed class ConfigReadDefaultsTests
{
    private static Config EmptyConfig() => new();

    [Fact]
    public void Apply_FillsKeyboardLayout_WhenMissing()
    {
        var config = EmptyConfig();
        ConfigReadDefaults.Apply(config);
        Assert.Equal(ConfigReadDefaults.DefaultKeyboardLayout, config.Options.KeyboardLayout);

        // 已有值时不覆盖
        config.Options.KeyboardLayout = "custom layout";
        ConfigReadDefaults.Apply(config);
        Assert.Equal("custom layout", config.Options.KeyboardLayout);
    }

    [Fact]
    public void Apply_FillsLanguage_WithZhOrEn()
    {
        var config = EmptyConfig();
        ConfigReadDefaults.Apply(config);
        Assert.Contains(config.Options.Language, new[] { "zh", "en" });

        config.Options.Language = "zh";
        ConfigReadDefaults.Apply(config);
        Assert.Equal("zh", config.Options.Language);
    }

    [Fact]
    public void Apply_InsertsExcludeWindowGroup_WhenAbsent()
    {
        var config = EmptyConfig();
        config.Options.WindowGroups.Add(new WindowGroup { Id = 1, Name = "浏览器" });

        ConfigReadDefaults.Apply(config);

        // Exclude 哨兵 (id=-1) 插到头部; 另叠加读时补齐的「📂 文件对话框」组 (id=2, QuickSwitch)
        Assert.Equal(3, config.Options.WindowGroups.Count);
        Assert.Equal(ConfigReadDefaults.ExcludeGroupId, config.Options.WindowGroups[0].Id);
        Assert.Equal("🚫 Exclude", config.Options.WindowGroups[0].Name);

        // 已存在排除项时不重复插入
        ConfigReadDefaults.Apply(config);
        Assert.Equal(3, config.Options.WindowGroups.Count);
    }

    [Fact]
    public void Apply_SelectedAction_NeverNull()
    {
        var config = EmptyConfig();
        config.SelectedAction = null!;
        ConfigReadDefaults.Apply(config);
        Assert.NotNull(config.SelectedAction);
        Assert.Empty(config.SelectedAction.Mappings);
    }
}
