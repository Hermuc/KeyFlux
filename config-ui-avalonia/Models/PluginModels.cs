using System.Text.Json.Serialization;

namespace KeyFlux.Settings.Models;

/// <summary>
/// 插件包 manifest DTO —— wire 格式 = 文件格式 = 后端 internal/plugins (specVersion 1)。
/// 用户插件位于 data/plugins/&lt;id&gt;/plugin.json, 经插件页导入 (zip) 与删除。
/// </summary>
public sealed class PluginManifest
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

    [JsonPropertyName("author")]
    public string? Author { get; set; }

    [JsonPropertyName("entry")]
    public PluginEntry Entry { get; set; } = new();

    [JsonPropertyName("permissions")]
    public List<string>? Permissions { get; set; }
}

/// <summary>插件入口声明: script = 包内 AHK 脚本 + 入口函数 (运行时加载为阶段 2)。</summary>
public sealed class PluginEntry
{
    [JsonPropertyName("kind")]
    public string Kind { get; set; } = "script";

    [JsonPropertyName("file")]
    public string? File { get; set; }

    [JsonPropertyName("func")]
    public string? Func { get; set; }
}

/// <summary>GET /api/plugins 响应: 用户插件列表 (ID 字典序) + 逐包加载告警。</summary>
public sealed class PluginListResponse
{
    [JsonPropertyName("plugins")]
    public List<PluginManifest> Plugins { get; set; } = [];

    [JsonPropertyName("errors")]
    public List<string>? Errors { get; set; }
}

/// <summary>
/// 插件市场目录条目 (marketplace.json)。由市场窗口拉取, url 指向插件包 zip;
/// 安装 = 客户端下载 zip 字节 -> POST /api/plugins/import (复用本地导入链路)。
/// </summary>
public sealed class MarketPluginEntry
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("nameEn")]
    public string? NameEn { get; set; }

    [JsonPropertyName("version")]
    public string? Version { get; set; }

    [JsonPropertyName("description")]
    public string? Description { get; set; }

    [JsonPropertyName("author")]
    public string? Author { get; set; }

    /// <summary>插件包 zip 下载地址 (https)。</summary>
    [JsonPropertyName("url")]
    public string Url { get; set; } = "";
}

/// <summary>插件市场目录 (marketplace.json 顶层结构)。</summary>
public sealed class MarketCatalog
{
    [JsonPropertyName("name")]
    public string? Name { get; set; }

    [JsonPropertyName("plugins")]
    public List<MarketPluginEntry> Plugins { get; set; } = [];
}
