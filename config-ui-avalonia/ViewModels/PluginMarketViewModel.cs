using System.Collections.ObjectModel;
using System.Net.Http;
using System.Text.Json;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 插件市场窗口 VM: 拉取市场目录 (JSON) 并支持一键安装。
///   - 目录与下载走独立 HttpClient (外部网络; .NET 默认遵循系统代理);
///   - 安装 = 客户端下载插件包 zip -> POST /api/plugins/import (与本地导入同链路,
///     后端不做出网请求);
///   - 已安装判定 = 本地用户插件目录 (打开窗口时快照 + 安装后即时更新)。
/// 市场目录格式 (marketplace.json):
///   { "name": "...", "plugins": [ { "id", "name", "nameEn", "version",
///     "description", "author", "url" (zip 下载地址) } ] }
/// </summary>
public sealed partial class PluginMarketViewModel : ObservableObject
{
    /// <summary>市场目录地址 (发布侧: 仓库 plugins/marketplace.json)。</summary>
    private const string CatalogUrl =
        "https://raw.githubusercontent.com/Hermuc/KeyFlux/main/plugins/marketplace.json";

    /// <summary>
    /// 内置插件 ID 集 (与 Go internal/plugins.BuiltinPluginIDs、插件页 VM 对应):
    /// 市场目录里的内置条目 (如 QuickSwitch 展示条目) 恒为「已安装」, 不提供安装按钮。
    /// </summary>
    private static readonly HashSet<string> BuiltinPluginIds = ["quick_switch"];

    private readonly MainViewModel _main;
    private readonly HttpClient _http = new() { Timeout = TimeSpan.FromSeconds(15) };

    public PluginMarketViewModel(MainViewModel main) => _main = main;

    /// <summary>语言切换递增, 驱动 ConverterParameter 式文案绑定重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>目录拉取中。</summary>
    [ObservableProperty]
    private bool _isLoading = true;

    /// <summary>目录拉取失败原因; null = 成功。</summary>
    [ObservableProperty]
    private string? _loadError;

    /// <summary>市场条目 (目录序)。</summary>
    public ObservableCollection<MarketEntryVm> Entries { get; } = [];

    /// <summary>安装动作回显 (成功提示; 失败走消息对话框)。</summary>
    [ObservableProperty]
    private string? _statusText;

    public bool ShowEmpty => !IsLoading && LoadError is null && Entries.Count == 0;
    public bool ShowError => !IsLoading && LoadError is not null;

    /// <summary>拉取市场目录并重建条目 (构造后/重试调用)。</summary>
    public async Task LoadAsync()
    {
        IsLoading = true;
        LoadError = null;
        try
        {
            using var resp = await _http.GetAsync(CatalogUrl);
            if (!resp.IsSuccessStatusCode)
            {
                throw new HttpRequestException($"HTTP {(int)resp.StatusCode}");
            }
            await using var stream = await resp.Content.ReadAsStreamAsync();
            var catalog = await JsonSerializer.DeserializeAsync<MarketCatalog>(stream)
                          ?? throw new JsonException("目录为空");

            var installed = await LoadInstalledIdsAsync();
            Entries.Clear();
            foreach (var entry in catalog.Plugins)
            {
                // 内置条目 (如 QuickSwitch 展示条目) 恒为已安装 —— 它随软件分发, 无独立 zip
                Entries.Add(new MarketEntryVm(entry)
                {
                    IsInstalled = installed.Contains(entry.Id) || BuiltinPluginIds.Contains(entry.Id),
                });
            }
        }
        catch (Exception ex)
        {
            LoadError = ex.Message;
        }
        finally
        {
            IsLoading = false;
            OnPropertyChanged(nameof(ShowEmpty));
            OnPropertyChanged(nameof(ShowError));
        }
    }

    /// <summary>重试拉取目录。</summary>
    [RelayCommand]
    private Task RetryAsync() => LoadAsync();

    private async Task<HashSet<string>> LoadInstalledIdsAsync()
    {
        if (_main.Session.Api is not { } api) return [];
        var resp = await api.GetPluginsAsync();
        return resp.Success && resp.Value is not null
            ? resp.Value.Plugins.Select(p => p.Id).ToHashSet()
            : [];
    }

    /// <summary>安装市场条目: 下载 zip -> 走本地导入 API。内置条目双保险拒绝 (无独立 zip)。</summary>
    [RelayCommand]
    private async Task InstallAsync(MarketEntryVm? entry)
    {
        if (entry is null || _main.Session.Api is not { } api) return;
        if (BuiltinPluginIds.Contains(entry.Entry.Id)) return;
        entry.IsInstalling = true;
        try
        {
            var zip = await _http.GetByteArrayAsync(entry.Entry.Url);
            var resp = await api.ImportPluginAsync(zip, $"{entry.Entry.Id}.zip");
            if (!resp.Success || resp.Value is null)
            {
                _main.ShowMessage(I18n.T("2432"), resp.ErrorMessage ?? $"HTTP {resp.StatusCode}");
                return;
            }
            entry.IsInstalled = true;
            StatusText = string.Format(I18n.T("2431"), resp.Value.Name);
        }
        catch (Exception ex)
        {
            _main.ShowMessage(I18n.T("2432"), ex.Message);
        }
        finally
        {
            entry.IsInstalling = false;
        }
    }

    public void OnLanguageChanged() => ++LanguageTick;
}

/// <summary>市场条目卡 VM: 包装目录条目 + 安装状态。</summary>
public sealed partial class MarketEntryVm : ObservableObject
{
    public MarketEntryVm(MarketPluginEntry entry) => Entry = entry;

    public MarketPluginEntry Entry { get; }

    [ObservableProperty]
    private bool _isInstalled;

    [ObservableProperty]
    private bool _isInstalling;

    public string DisplayName =>
        I18n.Language == I18n.En && !string.IsNullOrEmpty(Entry.NameEn)
            ? Entry.NameEn!
            : Entry.Name;

    public string VersionText => string.IsNullOrEmpty(Entry.Version) ? "" : $"v{Entry.Version}";

    public string Description => Entry.Description ?? "";

    public string Author => Entry.Author ?? "";

    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(DisplayName));
        OnPropertyChanged(nameof(VersionText));
    }
}
