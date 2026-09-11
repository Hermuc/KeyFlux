using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 插件页 (左侧导航「插件」) VM。统一插件列表 (不区分内置/导入):
///   - 列表首卡恒为内置「快速切换 (QuickSwitch)」(IsBuiltin=true), 后接用户插件目录
///     (GET /api/plugins, data/plugins);
///   - 每卡启停开关: 内置卡直通 config.options.quickSwitch.collectEnabled,
///     用户卡直通注册表 config.options.plugins.disabled (缺省启用); 状态文字 (已启用/已停用)
///     由卡 VM 的 StatusText 提供 (开关下方显示);
///   - 内置卡信息区可点击 (弹 QuickSwitchDialogWindow 编辑完整配置); 用户卡可删除;
///   - 入口: 「导入插件」(本地 zip 经 POST /api/plugins/import 安装) 与「插件市场」
///     (PluginMarketWindow 拉取目录, 一键下载安装)。
/// 引擎侧插件运行时为阶段 2 —— 导入的插件暂不参与脚本生成, 仅入册管理 (卡片带状态标注)。
/// </summary>
public sealed partial class PluginsPageViewModel : ObservableObject
{
    /// <summary>内置插件 ID 集 (与 Go internal/plugins.BuiltinPluginIDs 对应)。</summary>
    internal static readonly HashSet<string> BuiltinPluginIds = ["quick_switch"];

    private readonly MainViewModel _main;

    /// <summary>外部同步 (Refresh/重建卡片) 期间抑制开关直通保存, 防回写循环。</summary>
    private bool _syncingFromConfig;

    /// <summary>主 VM (打开 QuickSwitch 配置对话框等场景使用)。</summary>
    public MainViewModel Main => _main;

    public PluginsPageViewModel(MainViewModel main)
    {
        _main = main;
        // 构造即插入内置卡: 无后端场景 (单测/headless) 下统一列表也不空,
        // 状态由 Refresh 同步 (BuildNav 在 InitializeAsync 后必然调用)。
        Plugins.Add(CreateQuickSwitchCard());
    }

    /// <summary>语言切换递增, 驱动页面静态文案 (XAML 的 Tr 绑定) 重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    // --------------------------------------------------------------- 统一插件列表

    /// <summary>统一插件卡列表: 首卡恒为内置 QuickSwitch, 后接用户插件 (后端 ID 字典序)。</summary>
    public ObservableCollection<PluginCardVm> Plugins { get; } = [];

    /// <summary>插件目录加载中 (首次/刷新)。</summary>
    [ObservableProperty]
    private bool _isLoadingPlugins = true;

    /// <summary>目录加载告警 (后端逐包错误隔离汇总); null = 无告警。</summary>
    [ObservableProperty]
    private string? _pluginsLoadError;

    /// <summary>一次性操作回显 (导入/删除成功提示), 语言切换或下次操作时覆盖。</summary>
    [ObservableProperty]
    private string? _statusText;

    /// <summary>是否存在用户 (非内置) 插件 —— 驱动空态引导行。</summary>
    public bool HasUserPlugins => Plugins.Any(p => !p.IsBuiltin);

    /// <summary>空态引导可见 (加载完成、无告警、无用户插件)。</summary>
    public bool ShowEmptyState => !IsLoadingPlugins && PluginsLoadError is null && !HasUserPlugins;

    /// <summary>目录加载失败可见。</summary>
    public bool ShowLoadError => !IsLoadingPlugins && PluginsLoadError is not null;

    /// <summary>从后端拉取用户插件目录并重建卡片 (导航进入/导入/删除/市场安装后调用)。</summary>
    public async Task ReloadAsync()
    {
        if (_main.Session.Api is not { } api) return;
        IsLoadingPlugins = true;
        PluginsLoadError = null;
        var resp = await api.GetPluginsAsync();
        if (!resp.Success || resp.Value is null)
        {
            PluginsLoadError = resp.ErrorMessage ?? $"HTTP {resp.StatusCode}";
            IsLoadingPlugins = false;
            OnPropertyChanged(nameof(ShowEmptyState));
            OnPropertyChanged(nameof(ShowLoadError));
            return;
        }

        _syncingFromConfig = true;
        Plugins.Clear();
        Plugins.Add(CreateQuickSwitchCard());
        var disabled = _main.Config?.Options.Plugins?.Disabled;
        foreach (var m in resp.Value.Plugins)
        {
            Plugins.Add(new PluginCardVm(this, m)
            {
                Enabled = disabled?.Contains(m.Id) != true,
            });
        }
        _syncingFromConfig = false;

        if (resp.Value.Errors is { Count: > 0 })
        {
            PluginsLoadError = string.Join("\n", resp.Value.Errors);
        }
        IsLoadingPlugins = false;
        OnPropertyChanged(nameof(HasUserPlugins));
        OnPropertyChanged(nameof(ShowEmptyState));
        OnPropertyChanged(nameof(ShowLoadError));
    }

    /// <summary>构造内置「快速切换」卡 (合成 manifest; 开关直通 collectEnabled)。</summary>
    private PluginCardVm CreateQuickSwitchCard() => new(
        this,
        new PluginManifest
        {
            Id = "quick_switch",
            Name = I18n.T("2408"),
            NameEn = "Quick Switch",
            Description = I18n.T("2422"),
        },
        isBuiltin: true)
    {
        Enabled = _main.Config?.Options.QuickSwitch.CollectEnabled ?? false,
    };

    /// <summary>卡片开关联动 (PluginCardVm.Enabled 变更入口): 按卡来源分流写配置并保存。</summary>
    internal void OnCardEnabledChanged(PluginCardVm card, bool value)
    {
        if (_syncingFromConfig) return;
        if (card.IsBuiltin)
        {
            // 内置 QuickSwitch: 直通 collectEnabled (PUT /config 会重生成脚本并重启引擎)
            var qs = _main.Config?.Options.QuickSwitch;
            if (qs is null || qs.CollectEnabled == value) return;
            qs.CollectEnabled = value;
            _ = _main.SaveAsync(force: true); // 失败弹窗由 SaveAsync 统一处理 (同 Startup 开关先例)
            return;
        }

        var registry = _main.Config?.Options.Plugins;
        if (registry is null || registry.Disabled.Contains(card.Manifest.Id) == !value) return;
        ApplyEnabled(registry, card.Manifest.Id, value);
        _ = _main.SaveAsync(force: true); // 启停语义 = 保存链路 (引擎重启由 PUT /config 既有语义承载)
    }

    private static void ApplyEnabled(PluginsOption registry, string id, bool enabled)
    {
        if (enabled)
        {
            registry.Disabled.Remove(id);
        }
        else if (!registry.Disabled.Contains(id))
        {
            registry.Disabled.Add(id);
        }
    }

    /// <summary>保存/导航重建后同步全部卡开关状态 (由 MainViewModel.BuildNav 调用)。</summary>
    public void Refresh()
    {
        _syncingFromConfig = true;
        var collectEnabled = _main.Config?.Options.QuickSwitch.CollectEnabled ?? false;
        var disabled = _main.Config?.Options.Plugins?.Disabled;
        foreach (var card in Plugins)
        {
            card.Enabled = card.IsBuiltin
                ? collectEnabled
                : disabled?.Contains(card.Manifest.Id) != true;
        }
        _syncingFromConfig = false;
        OnPropertyChanged(nameof(HasUserPlugins));
    }

    // --------------------------------------------------------------- 导入 / 删除

    /// <summary>
    /// 导入本地插件包 zip (视图层 StorageProvider 选文件后调用)。
    /// 成功后刷新目录并回显; 失败弹后端 message (校验/zip 解析/冲突均 400)。
    /// </summary>
    public async Task ImportZipAsync(byte[] zipBytes, string fileName)
    {
        if (_main.Session.Api is not { } api) return;
        var resp = await api.ImportPluginAsync(zipBytes, fileName);
        if (!resp.Success || resp.Value is null)
        {
            _main.ShowMessage(I18n.T("2432"), resp.ErrorMessage ?? $"HTTP {resp.StatusCode}");
            return;
        }
        StatusText = string.Format(I18n.T("2431"), resp.Value.Name);
        await ReloadAsync();
    }

    /// <summary>
    /// 删除用户插件 (后端删 data/plugins/&lt;id&gt; 目录); 若该插件在启停注册表中,
    /// 顺带清理孤儿项并保存 (行为包先例: config 变更统一走 UI 保存链路)。
    /// 内置卡由 CanDelete 拦截, 不会进入此方法。
    /// </summary>
    [RelayCommand]
    private async Task DeletePlugin(PluginCardVm? card)
    {
        if (card is null || card.IsBuiltin) return;
        if (_main.Session.Api is not { } api) return;
        var resp = await api.DeletePluginAsync(card.Manifest.Id);
        if (!resp.Success)
        {
            _main.ShowMessage(I18n.T("2436"), resp.ErrorMessage ?? $"HTTP {resp.StatusCode}");
            return;
        }
        var registry = _main.Config?.Options.Plugins;
        if (registry is not null && registry.Disabled.Remove(card.Manifest.Id))
        {
            _ = await _main.SaveAsync(force: true);
        }
        await ReloadAsync();
    }

    /// <summary>语言切换: 刷新静态文案与卡片显示名 (由 MainViewModel 分发)。</summary>
    public void OnLanguageChanged()
    {
        LanguageTick++;
        foreach (var card in Plugins) card.RefreshLanguage();
        if (StatusText is not null) OnPropertyChanged(nameof(StatusText)); // 已有回显保持, 仅触发重绘
    }
}

/// <summary>
/// 统一插件卡 VM: 包装 manifest + 启停开关 + 来源标记。
/// 内置卡 (IsBuiltin): 开关直通 collectEnabled, 信息区可点开配置对话框, 不可删除;
/// 用户卡: 开关直通注册表 (options.plugins.disabled), 可删除。
/// Enabled 变更经 owner.OnCardEnabledChanged 写配置; 外部同步期间由 _syncingFromConfig 抑制回写。
/// </summary>
public sealed partial class PluginCardVm : ObservableObject
{
    private readonly PluginsPageViewModel _owner;

    public PluginCardVm(PluginsPageViewModel owner, PluginManifest manifest, bool isBuiltin = false)
    {
        _owner = owner;
        Manifest = manifest;
        IsBuiltin = isBuiltin;
    }

    public PluginManifest Manifest { get; }

    /// <summary>内置插件标记 (首卡 QuickSwitch; 市场目录内置条目同款判定)。</summary>
    public bool IsBuiltin { get; }

    /// <summary>仅用户插件可删除。</summary>
    public bool CanDelete => !IsBuiltin;

    /// <summary>仅内置插件有配置对话框 (QuickSwitch; 用户插件配置为后续阶段)。</summary>
    public bool CanConfigure => IsBuiltin;

    /// <summary>启停开关 (内置卡 = collectEnabled; 用户卡 = 注册表 disabled 的反义)。</summary>
    [ObservableProperty]
    private bool _enabled;

    partial void OnEnabledChanged(bool value)
    {
        OnPropertyChanged(nameof(StatusText));
        _owner.OnCardEnabledChanged(this, value);
    }

    /// <summary>开关下方的状态文字 (2423 已启用 / 2424 已停用)。</summary>
    public string StatusText => Enabled ? I18n.T("2423") : I18n.T("2424");

    /// <summary>显示名: 英文界面优先 nameEn (同行为包 LabelFor 口径)。</summary>
    public string DisplayName =>
        I18n.Language == I18n.En && !string.IsNullOrEmpty(Manifest.NameEn)
            ? Manifest.NameEn!
            : Manifest.Name;

    /// <summary>版本徽标 (无版本信息时为空)。</summary>
    public string VersionText => string.IsNullOrEmpty(Manifest.Version) ? "" : $"v{Manifest.Version}";

    public string Description => Manifest.Description ?? "";

    public string Author => Manifest.Author ?? "";

    /// <summary>语言切换后刷新派生显示名与状态文字 (由页 VM 分发)。</summary>
    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(DisplayName));
        OnPropertyChanged(nameof(VersionText));
        OnPropertyChanged(nameof(StatusText));
    }
}
