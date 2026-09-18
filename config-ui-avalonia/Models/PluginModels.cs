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

    /// <summary>
    /// 声明式设置项 (可选)。非空即代表「该插件可通过点击卡片配置」——
    /// 设置界面据此渲染编辑器, 后端据此校验值 (见 plugin_settings.go)。
    /// </summary>
    [JsonPropertyName("settings")]
    public List<PluginSetting>? Settings { get; set; }
}

/// <summary>
/// 单个插件设置项的声明 (manifest.settings[], 与 Go internal/plugins.Setting 同构)。
/// 只描述「有哪些设置、长什么样」; 真实值在 GET/PUT /api/plugins/:id/settings 里。
/// </summary>
public sealed class PluginSetting
{
    /// <summary>存储键 (也是 values 字典的键): ^[A-Za-z][A-Za-z0-9_]{0,31}$。</summary>
    [JsonPropertyName("key")]
    public string Key { get; set; } = "";

    /// <summary>编辑器类型: char / text / number / file (未知值按 text 处理)。</summary>
    [JsonPropertyName("type")]
    public string Type { get; set; } = PluginSettingTypes.Text;

    /// <summary>中文标签。</summary>
    [JsonPropertyName("label")]
    public string Label { get; set; } = "";

    /// <summary>英文标签 (英文界面优先)。</summary>
    [JsonPropertyName("labelEn")]
    public string? LabelEn { get; set; }

    /// <summary>默认值 (未存过时界面显示的初值)。</summary>
    [JsonPropertyName("default")]
    public string? Default { get; set; }

    /// <summary>仅 file: 文件选择器的类型过滤名 (如 "everything.exe")。</summary>
    [JsonPropertyName("filter")]
    public string? Filter { get; set; }

    [JsonPropertyName("hint")]
    public string? Hint { get; set; }

    [JsonPropertyName("hintEn")]
    public string? HintEn { get; set; }

    /// <summary>仅 number: 闭区间下限 (null = 不限)。</summary>
    [JsonPropertyName("min")]
    public double? Min { get; set; }

    /// <summary>仅 number: 闭区间上限 (null = 不限)。</summary>
    [JsonPropertyName("max")]
    public double? Max { get; set; }

    /// <summary>仅 text: 值长度上限 (0 = 用后端默认上限)。</summary>
    [JsonPropertyName("maxLength")]
    public int MaxLength { get; set; }
}

/// <summary>设置项类型常量 (与 Go internal/plugins 的 SettingType* 词表一致)。</summary>
public static class PluginSettingTypes
{
    public const string Char = "char";
    public const string Text = "text";
    public const string Number = "number";
    public const string File = "file";
}

/// <summary>PUT /api/plugins/:id/settings 请求体。</summary>
public sealed class PluginSettingsRequest
{
    [JsonPropertyName("values")]
    public Dictionary<string, string> Values { get; set; } = [];
}

/// <summary>
/// GET/PUT /api/plugins/:id/settings 响应体: 声明 (渲染契约) + 值 (数据) 一并返回,
/// 端点自洽 —— 打开对话框不必先另外拉一次 manifest。
/// </summary>
public sealed class PluginSettingsResponse
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("settings")]
    public List<PluginSetting> Settings { get; set; } = [];

    /// <summary>已存值合并 manifest 默认值后的完整值表 (键 = PluginSetting.Key)。</summary>
    [JsonPropertyName("values")]
    public Dictionary<string, string> Values { get; set; } = [];
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
