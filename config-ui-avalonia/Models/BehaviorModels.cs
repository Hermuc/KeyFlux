using System.Text.Json.Serialization;

namespace KeyFlux.Settings.Models;

/// <summary>
/// 行为包 manifest DTO —— wire 格式 = 文件格式 = 后端 CONTRACTS §3.9 (specVersion 1)。
/// 内置包随软件分发 (只读), 用户包位于 data/behaviors, 经行为库窗口增删。
/// </summary>
public sealed class BehaviorPack
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("nameEn")]
    public string? NameEn { get; set; }

    [JsonPropertyName("version")]
    public string? Version { get; set; }

    [JsonPropertyName("specVersion")]
    public int SpecVersion { get; set; }

    [JsonPropertyName("description")]
    public string? Description { get; set; }

    [JsonPropertyName("appliesTo")]
    public List<BehaviorAppliesTo> AppliesTo { get; set; } = [];

    [JsonPropertyName("entry")]
    public BehaviorEntry Entry { get; set; } = new();

    [JsonPropertyName("permissions")]
    public List<string>? Permissions { get; set; }

    /// <summary>来源标记 (builtin/user), 后端加载期附加, 不属于包文件本身。</summary>
    [JsonPropertyName("source")]
    public string? Source { get; set; }
}

/// <summary>生效前提条目: fileExt 用显式后缀集 ("*"=任意文件), textType 用特征枚举值。</summary>
public sealed class BehaviorAppliesTo
{
    [JsonPropertyName("type")]
    public string Type { get; set; } = "fileExt";

    [JsonPropertyName("exts")]
    public List<string>? Exts { get; set; }

    [JsonPropertyName("value")]
    public string? Value { get; set; }

    /// <summary>该前提桶的默认推荐行为 (切换匹配类型/条件时自动落到此行为)。</summary>
    [JsonPropertyName("default")]
    public bool Default { get; set; }
}

public sealed class BehaviorEntry
{
    /// <summary>builtin = 基础动作组合 (一期); script = 自定义脚本 (二期)。</summary>
    [JsonPropertyName("kind")]
    public string Kind { get; set; } = "builtin";

    [JsonPropertyName("action")]
    public string? Action { get; set; }

    [JsonPropertyName("params")]
    public BehaviorEntryParams? Params { get; set; }

    [JsonPropertyName("file")]
    public string? File { get; set; }

    [JsonPropertyName("func")]
    public string? Func { get; set; }
}

public sealed class BehaviorEntryParams
{
    [JsonPropertyName("actionValue")]
    public string? ActionValue { get; set; }

    [JsonPropertyName("workingDir")]
    public string? WorkingDir { get; set; }
}

/// <summary>GET /api/behaviors 响应: 内置 + 用户两组 (各自 ID 字典序) + 加载告警。</summary>
public sealed class BehaviorCatalogResponse
{
    [JsonPropertyName("builtin")]
    public List<BehaviorPack> Builtin { get; set; } = [];

    [JsonPropertyName("user")]
    public List<BehaviorPack> User { get; set; } = [];

    [JsonPropertyName("errors")]
    public List<string>? Errors { get; set; }
}
