using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

// ============================================================================
// 选中动作单屏页: 单一热键 + 匹配规则 + 行为菜单 (1..9 数字键选择)。
// 保存纪律: 启用/删除立即保存; 其余修改 (增行为/提交 transient/条件值) 经 MainViewModel.SaveAsync 咽喉。
// 数据真源 = Config.SelectedAction; 聚合卡 (TypeCardVm) 直接持有并就地修改底层 Mapping 对象。
// ============================================================================

/// <summary>行为胶囊 (只读投影; 序号 = entries 下标 + 1, 即菜单数字键位)。</summary>
public sealed record EntryChipVm(int Index, string Label, string ColorHex);

/// <summary>模拟结果菜单键位 (key 从 1 起; 颜色按行为基础动作推导)。</summary>
public sealed record MenuKeyVm(int Key, string Name, string ColorHex);

/// <summary>行为徽章配色: 链接蓝 / 路径绿 / 磁力·注册表紫 / 其余灰 (浅色主题可读; 行类型徽章另用后缀橙)。</summary>
public static class BehaviorBadgeColors
{
    public const string LinkDarkWarm = ClaudePalette.DarkWarm;
    public const string PathGreen = ClaudePalette.MutedGreen;
    public const string MagnetCoral = ClaudePalette.Coral;
    public const string PlainOlive = ClaudePalette.OliveGray;
    public const string ExtTerracotta = ClaudePalette.Terracotta;

    public static string ForBehavior(string id) => BehaviorCatalog.BaseActionOf(id) switch
    {
        "open_url" => LinkDarkWarm,
        "open_path" or "open_folder" => PathGreen,
        "magnet_download" => MagnetCoral,
        _ => PlainOlive,
    };
}

/// <summary>
/// 选中动作单屏页: 主快捷键捕获 + 启用开关 + 两张聚合卡 (文本特征 / 文件后缀) + 添加弹窗 + 模拟测试条。
/// 数据真源 = Config.SelectedAction。
/// </summary>
public sealed partial class SelectedActionPageViewModel : ObservableObject, ILanguageRefresh
{
    private readonly MainViewModel _main;

    public SelectedActionPageViewModel(MainViewModel main)
    {
        _main = main;
        UsedHotkeys = BuildUsedHotkeys();
        TextCard = new TypeCardVm(this, "textType");
        FileCard = new TypeCardVm(this, "fileExt");
        RebuildCards();
    }

    public MainViewModel Main => _main;
    public Config Config => _main.Config ?? throw new InvalidOperationException("Config 未加载");

    /// <summary>页内提示条 (重复映射等非阻断反馈; 空串隐藏)。</summary>
    [ObservableProperty]
    private string _statusText = "";
    private ISettingsApi? Api => _main.Session.Api;

    /// <summary>语言切换递增, 驱动页内 ConverterParameter 文案重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    private SelectedAction Sa => Config.SelectedAction;

    /// <summary>文件分组 (快捷填入数据源)。</summary>
    public IReadOnlyList<FileGroup> FileGroups => Config.FileGroups;

    /// <summary>确认对话框委托 (视图注入; 删除映射确认)。</summary>
    public Func<string, string, Task<bool>>? ConfirmAsync { get; set; }

    /// <summary>
    /// 匹配类型弹窗委托 (视图注入): 打开「匹配类型」窗口, 返回本次新建的类型 id (取消/无新建为 null)。
    /// </summary>
    public Func<Task<string?>>? MatchTypesDialogAsync { get; set; }

    // ------------------------------------------------------------- 两张聚合卡

    /// <summary>文本特征聚合卡 (matchType=textType)。</summary>
    public TypeCardVm TextCard { get; }

    /// <summary>文件后缀聚合卡 (matchType=fileExt)。</summary>
    public TypeCardVm FileCard { get; }

    /// <summary>配置变化后重建两卡的类型 toggle 集合 (加载 / 增删映射 / 新建类型后)。</summary>
    public void RebuildCards()
    {
        TextCard.RebuildToggles();
        FileCard.RebuildToggles();
        OnPropertyChanged(nameof(HasAnyMappings));
    }

    /// <summary>卡内 mapping 变化 (删除/提交 transient) 后刷新页级派生状态。</summary>
    internal void OnCardMappingChanged() => OnPropertyChanged(nameof(HasAnyMappings));

    public bool HasAnyMappings => Config.SelectedAction.Mappings.Count > 0;

    // ------------------------------------------------------------- 主快捷键 + 启用

    /// <summary>主快捷键 (AHK 格式; HotkeyCapture 捕获)。变更后展示未保存提示 (1077)。</summary>
    public string Hotkey
    {
        get => Sa.Hotkey;
        set
        {
            if (Sa.Hotkey == value) return;
            Sa.Hotkey = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(NoHotkeyWarning));
            OnPropertyChanged(nameof(HotkeyHintText));
            HotkeyPendingSave = true; // 未保存提示条 (与旧编辑器语义一致: 保存后才生效)
        }
    }

    /// <summary>空热键警示 (976)。</summary>
    public bool NoHotkeyWarning => string.IsNullOrEmpty(Sa.Hotkey);

    /// <summary>热键卡提示条文案: 未保存 (1077) 优先于空热键警示 (976); 空串隐藏。</summary>
    public string HotkeyHintText
        => HotkeyPendingSave ? I18n.T("1077") : NoHotkeyWarning ? I18n.T("976") : "";

    /// <summary>热键已改未保存提示 (1077; 保存成功后复位)。</summary>
    [ObservableProperty]
    private bool _hotkeyPendingSave;
    partial void OnHotkeyPendingSaveChanged(bool value) => OnPropertyChanged(nameof(HotkeyHintText));

    /// <summary>主配置保存成功后复位热键未保存提示。</summary>
    public void OnConfigSaved() => HotkeyPendingSave = false;

    /// <summary>已占用热键 (启用的 keymaps 全部热键; 单方案模型无其他方案冲突源)。</summary>
    [ObservableProperty]
    private HashSet<string> _usedHotkeys = [];

    private HashSet<string> BuildUsedHotkeys() => HotkeyLogic.CollectUsedHotkeys(Config.Keymaps);

    /// <summary>启用开关: 立即保存, 失败回滚 (避免显示与配置不一致)。</summary>
    public bool Enable
    {
        get => Sa.Enable;
        set
        {
            if (Sa.Enable == value) return;
            var original = Sa.Enable;
            Sa.Enable = value;
            OnPropertyChanged();
            _ = SaveEnableAsync(original, value);
        }
    }

    /// <summary>启用开关的立即保存: 失败则回滚 (用户已再次拨动则尊重最新意图)。</summary>
    private async Task SaveEnableAsync(bool original, bool attempted)
    {
        try
        {
            if (await SaveConfigAsync()) return;
        }
        catch
        {
            // SaveAsync 内部已消化 HTTP 错误分支; 此处兜底未预期异常后走回滚
        }
        if (Sa.Enable != attempted) return;
        Sa.Enable = original;
        OnPropertyChanged(nameof(Enable));
    }

    // ------------------------------------------------------------- 匹配类型管理 / 添加弹窗

    /// <summary>「匹配类型」: 打开弹窗管理自定义匹配类型 (文本特征 / 文件后缀), 关闭后刷新依赖方。</summary>
    [RelayCommand]
    private async Task ManageMatchTypesAsync()
    {
        if (MatchTypesDialogAsync is null) return;
        var created = await MatchTypesDialogAsync();
        RebuildCards(); // 类型可能增删 (分组 / 自定义类型)
        RefreshBehaviorOptions();
        AddPanel?.RefreshTypeOptions(created is null ? null : "type:" + created);
    }

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsAddPanelOpen))]
    private AddMappingVm? _addPanel;

    public bool IsAddPanelOpen => AddPanel is not null;

    [RelayCommand]
    private void OpenAddPanel() => AddPanel = new AddMappingVm(this);

    [RelayCommand]
    private void CloseAddPanel() => AddPanel = null;

    /// <summary>弹窗确认: 构造 SelectedMapping 插入对应分区 (组内行序 = 优先级, 新行排最后)。</summary>
    public void AddMapping(AddMappingVm panel)
    {
        var typeValue = panel.TypeSelected?.Value ?? "";
        if (string.IsNullOrEmpty(typeValue)) return; // 分隔项/未选中防御
        var isFileExt = typeValue.StartsWith("group:")
                        || (typeValue.StartsWith("type:")
                            && Config.MatchTypes.FirstOrDefault(t => "type:" + t.Id == typeValue)?.Kind == "fileExt");
        var mapping = new SelectedMapping
        {
            MatchType = isFileExt ? "fileExt" : "textType",
            MatchValue = isFileExt ? panel.MatchValue.Trim() : typeValue,
            Entries = panel.BehaviorPicks
                .Where(p => p.IsChecked)
                .Select(p => new SelectedEntry
                {
                    Behavior = p.Pack.Id,
                    ActionValue = BehaviorCatalog.IsNoValue(p.Pack.Id)
                        ? ""
                        : BehaviorCatalog.DefaultTemplateFor(p.Pack.Id),
                    WorkingDir = "",
                    Options = new RuleOptions(),
                })
                .ToList(),
        };
        if (mapping.Entries.Count == 0) return;

        // 同 (matchType, matchValue) 去重: 该类型已配置时不重复添加
        if (Config.SelectedAction.Mappings.Any(m =>
                m.MatchType == mapping.MatchType && m.MatchValue == mapping.MatchValue))
        {
            AddPanel = null;
            StatusText = I18n.T("1115"); // 该匹配条件已存在
            return;
        }

        AddPanel = null;
        Config.SelectedAction.Mappings.Add(mapping);
        RebuildCards();
        OnCardMappingChanged();
    }

    // ------------------------------------------------------------- 模拟测试条

    /// <summary>模拟选中内容 (复刻旧默认 "https://github.com/Hermuc/KeyFlux")。</summary>
    [ObservableProperty]
    private string _testContent = "https://github.com/Hermuc/KeyFlux";

    [ObservableProperty]
    private bool _testIsFile;

    [ObservableProperty]
    private bool _testing;

    [ObservableProperty]
    private string _testError = "";

    [ObservableProperty]
    private bool _hasResult;

    [ObservableProperty]
    private bool _resultMatched;

    /// <summary>命中类型徽章文本 (MatchTypeLabel)。</summary>
    [ObservableProperty]
    private string _matchedTypeText = "";

    /// <summary>命中条件值。</summary>
    [ObservableProperty]
    private string _matchedValueText = "";

    public ObservableCollection<MenuKeyVm> MenuKeys { get; } = [];

    /// <summary>执行预览。</summary>
    [ObservableProperty]
    private string _previewText = "";

    /// <summary>
    /// 模拟测试: 先投影回模型 (本版卡直接持有底层对象, Config 已是最新) 再深拷贝快照随请求发出;
    /// 命中时高亮对应卡详情并展示菜单键位预览; 400 展示后端 message。
    /// </summary>
    [RelayCommand]
    private async Task RunTestAsync()
    {
        if (string.IsNullOrWhiteSpace(TestContent))
        {
            TestError = I18n.T("991");
            return;
        }
        if (Api is null) return;

        Testing = true;
        TestError = "";
        HasResult = false;
        SetMatchedRow(null);

        var snapshot = JsonSerializer.Deserialize<SelectedAction>(
            JsonSerializer.Serialize(Sa, SettingsJson.Options), SettingsJson.Options);
        var resp = await Api.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = TestContent,
            IsFile = TestIsFile,
            SelectedAction = snapshot,
            MatchTypes = Config.MatchTypes, // 未保存的新类型也参与模拟测试 (§D.3.10)
        });

        Testing = false;
        if (!resp.Success)
        {
            TestError = I18n.T("992") + (resp.ErrorMessage ?? $"HTTP {resp.StatusCode}");
            return;
        }

        HasResult = true;
        var result = resp.Value;
        if (result is { Matched: true })
        {
            ResultMatched = true;
            MatchedTypeText = ActionSchemeCatalog.MatchTypeLabel(result.MatchType);
            MatchedValueText = result.MatchValue;
            MenuKeys.Clear();
            foreach (var item in result.Menu)
            {
                MenuKeys.Add(new MenuKeyVm(item.Key, item.Name, BehaviorBadgeColors.ForBehavior(item.Behavior)));
            }
            PreviewText = string.IsNullOrEmpty(result.Preview) ? I18n.T("997") : result.Preview;
            SetMatchedRow(FindRow(result.MatchType, result.MatchValue));
        }
        else
        {
            ResultMatched = false;
        }
    }

    /// <summary>行内 ▶ 测试: 预填底部模拟条 (按映射类型给示例内容) 并立即执行。</summary>
    public void RunTestFor(MappingRowVm row)
    {
        TestIsFile = !row.IsTextType;
        TestContent = row.IsTextType
            ? row.Mapping.MatchValue switch
            {
                "url" => "https://github.com/Hermuc/KeyFlux",
                "path" => "C:\\Windows\\explorer.exe",
                "magnet" => "magnet:?xt=urn:btih:example",
                "bilibili" => "BV1xx411c7mD",
                _ => "hello world",
            }
            : "C:\\example\\photo.jpg";
        _ = RunTestCommand.ExecuteAsync(null);
    }

    /// <summary>命中回显: 按后端返回的 matchType/matchValue 找到对应卡详情编辑器 (按值匹配, 忽略大小写)。</summary>
    private MappingRowVm? FindRow(string matchType, string matchValue)
    {
        foreach (var card in new[] { TextCard, FileCard })
        {
            if (card.Detail is { } d && d.MatchType == matchType
                && string.Equals(d.Mapping.MatchValue, matchValue, StringComparison.OrdinalIgnoreCase))
            {
                return d;
            }
        }
        return null;
    }

    private MappingRowVm? _matchedRow;

    private void SetMatchedRow(MappingRowVm? row)
    {
        if (ReferenceEquals(_matchedRow, row)) return;
        if (_matchedRow is not null) _matchedRow.IsMatched = false;
        _matchedRow = row;
        if (row is not null) row.IsMatched = true;
    }

    // ------------------------------------------------------------- 行为目录

    /// <summary>拉取行为目录快照 (视图挂载后触发; 已加载则跳过), 完成后刷新下拉与勾选列表。</summary>
    public async Task EnsureBehaviorCatalogAsync()
    {
        if (BehaviorCatalog.Loaded || Api is null) return;
        await BehaviorCatalog.LoadAsync(Api);
        RefreshBehaviorOptions();
    }

    /// <summary>行为库窗口关闭后强制重拉目录并刷新全部下拉/勾选 (行为包可能增删)。</summary>
    public async Task ReloadBehaviorCatalogAsync()
    {
        if (Api is null) return;
        await BehaviorCatalog.LoadAsync(Api);
        RefreshBehaviorOptions();
    }

    /// <summary>行为目录变化后刷新全部下拉/勾选列表 (行为库窗口关闭后也会调用)。</summary>
    public void RefreshBehaviorOptions()
    {
        foreach (var card in new[] { TextCard, FileCard })
        {
            if (card.Detail is { } d)
            {
                d.RefreshChips();
                foreach (var editor in d.Editors) editor.RefreshOptions();
            }
        }
        AddPanel?.RefreshPicks();
    }

    // ------------------------------------------------------------- 保存

    /// <summary>主配置保存 (启用开关/删除映射/提交 transient 语义为「立即保存」故跳过节流)。</summary>
    public Task<bool> SaveConfigAsync() => _main.SaveAsync(force: true);

    // ------------------------------------------------------------- 语言刷新

    public void OnLanguageChanged()
    {
        LanguageTick++;
        OnPropertyChanged(nameof(HotkeyHintText)); // 条件拼接文案, 语言切换需重算
        TextCard.RefreshLanguage();
        FileCard.RefreshLanguage();
        AddPanel?.RefreshLanguage();
        if (HasResult && ResultMatched)
        {
            OnPropertyChanged(nameof(MatchedTypeText));
            OnPropertyChanged(nameof(MatchedValueText));
            OnPropertyChanged(nameof(PreviewText));
        }
    }
}
