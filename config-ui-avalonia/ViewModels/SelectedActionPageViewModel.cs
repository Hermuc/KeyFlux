using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

// ============================================================================
// 选中动作单屏页 (方案 D「单键分发」, 2026-09):
//   单一热键 + mappings 匹配前提桶 + entries 行为菜单 (1..9 数字键选择)。
//   旧多方案 ActionScheme 与两级导航 (列表页 -> 编辑页) 已整体退役。
//
// 结构:
//   EntryChipVm                  行为胶囊 ([1 浏览器打开] 式序号, 只读投影)
//   EntryRowVm                   手风琴内行为编辑行 (行为下拉/模板/工作目录/Options)
//   MappingRowVm                 一行映射 (textType/fileExt), chips + 行内手风琴
//   AddMappingVm / BehaviorPickVm 添加映射弹窗 (类型 -> 值 -> 行为勾选)
//   SelectedActionPageViewModel  页面宿主 (热键捕获/开关/两分区/模拟条/保存)
//
// 保存纪律: 启用开关与删除映射立即保存 (沿用旧卡片页语义); 其余修改统一经
// MainViewModel.SaveAsync 咽喉 (Ctrl+S / 侧栏「保存配置」), 避免频繁 PUT 触发
// KeyFlux 进程重启。分组后缀写回 (评审 F1/F2 语义) 由咽喉调用
// ApplyFileGroupWriteBack 完成。
// ============================================================================

/// <summary>行为胶囊 (只读投影; 序号 = entries 下标 + 1, 即菜单数字键位)。</summary>
public sealed record EntryChipVm(int Index, string Label, string ColorHex);

/// <summary>模拟结果菜单键位 (key 从 1 起; 颜色按行为基础动作推导)。</summary>
public sealed record MenuKeyVm(int Key, string Name, string ColorHex);

/// <summary>行为徽章配色: 链接蓝 / 路径绿 / 磁力·注册表紫 / 其余灰 (浅色主题可读; 行类型徽章另用后缀橙)。</summary>
public static class BehaviorBadgeColors
{
    public const string LinkBlue = "#4169E1";
    public const string PathGreen = "#2E7D32";
    public const string MagnetPurple = "#7B1FA2";
    public const string PlainGray = "#5F6368";
    public const string ExtOrange = "#E65100";

    public static string ForBehavior(string id) => BehaviorCatalog.BaseActionOf(id) switch
    {
        "open_url" => LinkBlue,
        "open_path" or "open_folder" => PathGreen,
        "magnet_download" or "open_registry" => MagnetPurple,
        _ => PlainGray,
    };
}

/// <summary>
/// 选中动作单屏页 (方案 D, 复刻重构文档 §三): 主快捷键捕获 + 启用开关 +
/// 两分区映射列表 (文本特征/文件后缀, 组内行序 = 匹配优先级) + 添加映射弹窗 +
/// 底部紧凑模拟测试条。数据真源 = Config.SelectedAction (恒对象, GET/PUT /config 全链路携带)。
/// </summary>
public sealed partial class SelectedActionPageViewModel : ObservableObject, ILanguageRefresh
{
    private readonly MainViewModel _main;

    public SelectedActionPageViewModel(MainViewModel main)
    {
        _main = main;
        UsedHotkeys = BuildUsedHotkeys();
        ReloadRows();
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

    /// <summary>热键卡内提示条文案 (互斥合并): 未保存 (1077) 优先于空热键警示 (976);
    /// 空串隐藏。NoHotkeyWarning/HotkeyPendingSave 保留原属性供测试断言。</summary>
    public string HotkeyHintText
        => HotkeyPendingSave ? I18n.T("1077") : NoHotkeyWarning ? I18n.T("976") : "";

    /// <summary>热键已修改未保存提示条 (1077; 保存成功后由 MainViewModel.SaveAsync 成功分支经 OnConfigSaved 复位)。</summary>
    [ObservableProperty]
    private bool _hotkeyPendingSave;
    partial void OnHotkeyPendingSaveChanged(bool value) => OnPropertyChanged(nameof(HotkeyHintText));

    /// <summary>主配置保存成功后回调 (评审 L4: MainViewModel.SaveAsync 成功分支调用)。
    /// 此前 HotkeyPendingSave 永不复位, 热键改过一次后提示条一直悬挂到关窗。</summary>
    public void OnConfigSaved() => HotkeyPendingSave = false;

    /// <summary>已占用热键 (启用的 keymaps 全部热键; 单方案模型无其他方案冲突源)。</summary>
    [ObservableProperty]
    private HashSet<string> _usedHotkeys = [];

    private HashSet<string> BuildUsedHotkeys() => HotkeyLogic.CollectUsedHotkeys(Config.Keymaps);

    /// <summary>启用开关: 写入内存并立即保存 (沿用旧卡片页语义);
    /// 失败 (400 / 传输层异常) 回滚到原值, 避免开关显示与真实配置不一致 (评审 L3)。</summary>
    public bool Enable
    {
        get => Sa.Enable;
        set
        {
            if (Sa.Enable == value) return;
            var original = Sa.Enable;
            Sa.Enable = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(EnableLabel));
            _ = SaveEnableAsync(original, value);
        }
    }

    /// <summary>启用开关的立即保存 (fire-and-forget 包裹 try/catch, 评审 L3):
    /// 保存失败或抛异常时还原 Sa.Enable 并刷新 UI 态; 若期间用户再次拨动
    /// (内存值已不是本次尝试写入的值), 尊重最新意图不回滚。</summary>
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
        OnPropertyChanged(nameof(EnableLabel));
    }

    public string EnableLabel => Sa.Enable ? I18n.T("964") : I18n.T("965");

    // ------------------------------------------------------------- 两分区映射列表

    /// <summary>文本特征分区 (matchType=textType; 组内行序 = 优先级)。</summary>
    public ObservableCollection<MappingRowVm> TextMappings { get; } = [];

    /// <summary>文件后缀分区 (matchType=fileExt; 组内行序 = 优先级)。</summary>
    public ObservableCollection<MappingRowVm> FileMappings { get; } = [];

    public bool HasAnyMappings => TextMappings.Count > 0 || FileMappings.Count > 0;

    private void ReloadRows()
    {
        TextMappings.Clear();
        FileMappings.Clear();
        foreach (var mapping in Sa.Mappings)
        {
            var row = new MappingRowVm(this, mapping);
            (mapping.MatchType == "textType" ? TextMappings : FileMappings).Add(row);
        }
        NotifyMoveability();
        OnPropertyChanged(nameof(HasAnyMappings));
    }

    /// <summary>分区与分区内移动边界判断。</summary>
    public bool CanMove(MappingRowVm row, int dir)
    {
        var list = row.IsTextType ? TextMappings : FileMappings;
        var index = list.IndexOf(row);
        var target = index + dir;
        return index >= 0 && target >= 0 && target < list.Count;
    }

    /// <summary>分区行排序 (组内行序 = 优先级; 不跨分区)。</summary>
    public void MoveMapping(MappingRowVm row, int dir)
    {
        var list = row.IsTextType ? TextMappings : FileMappings;
        var index = list.IndexOf(row);
        var target = index + dir;
        if (index < 0 || target < 0 || target >= list.Count) return;
        list.Move(index, target);
        NotifyMoveability();
    }

    /// <summary>行序相关视觉态刷新 (移动边界 + 分区首行标记; internal 供测试挂行后触发)。</summary>
    internal void NotifyMoveability()
    {
        foreach (var row in TextMappings.Concat(FileMappings)) row.NotifyMoveability();
        RefreshPartitionTitles();
    }

    /// <summary>分区首行标记 (标题在卡内首行展示; 加载/增删/移动后首行可能变化)。</summary>
    private void RefreshPartitionTitles()
    {
        foreach (var row in TextMappings) row.IsFirstInPartition = ReferenceEquals(row, TextMappings[0]);
        foreach (var row in FileMappings) row.IsFirstInPartition = ReferenceEquals(row, FileMappings[0]);
    }

    /// <summary>删除映射 (确认后立即保存, 1109 文案语义)。</summary>
    public async Task AskRemoveAsync(MappingRowVm row)
    {
        var confirmed = ConfirmAsync is not null
            ? await ConfirmAsync(I18n.T("967"), string.Format(I18n.T("1109"), row.MatchSummary))
            : false;
        if (!confirmed) return;

        if (ReferenceEquals(ExpandedRow, row)) ExpandedRow = null;
        var list = row.IsTextType ? TextMappings : FileMappings;
        list.Remove(row);
        OnPropertyChanged(nameof(HasAnyMappings));
        NotifyMoveability();
        await SaveConfigAsync();
    }

    // ------------------------------------------------------------- 手风琴仲裁

    /// <summary>当前展开的手风琴行 (同屏只开一个; null=全部收起)。</summary>
    [ObservableProperty]
    private MappingRowVm? _expandedRow;

    partial void OnExpandedRowChanged(MappingRowVm? value)
    {
        foreach (var row in TextMappings.Concat(FileMappings))
        {
            if (ReferenceEquals(row, value)) continue;
            if (row.IsExpanded)
            {
                row.IsExpanded = false;
                row.CloseEditor();
            }
        }
        if (value is not null)
        {
            value.IsExpanded = true;
            value.OpenEditor();
        }
    }

    // ------------------------------------------------------------- 添加映射弹窗

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsAddPanelOpen))]
    private AddMappingVm? _addPanel;

    public bool IsAddPanelOpen => AddPanel is not null;

    [RelayCommand]
    private void OpenAddPanel() => AddPanel = new AddMappingVm(this);

    [RelayCommand]
    private void CloseAddPanel() => AddPanel = null;

    /// <summary>弹窗确认: 构造 SelectedMapping 插入对应分区尾部 (组内行序 = 优先级, 新行排最后)。</summary>
    public void AddMapping(AddMappingVm panel)
    {
        var typeValue = panel.TypeSelected?.Value ?? "";
        if (string.IsNullOrEmpty(typeValue)) return; // 分隔项/未选中防御
        var isGroup = typeValue.StartsWith("group:");
        var isFileExt = isGroup; // 分组项即 fileExt 类 mapping (matchValue = 组后缀集)
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

        // 同 (matchType, matchValue) 分组去重: 该分组已配置时不重复添加 (后端校验同款规则)
        if ((isGroup ? FileMappings : TextMappings).Any(r =>
                r.Mapping.MatchType == mapping.MatchType && r.Mapping.MatchValue == mapping.MatchValue))
        {
            AddPanel = null;
            StatusText = I18n.T("1115"); // 该匹配条件已存在
            return;
        }

        AddPanel = null;
        var row = new MappingRowVm(this, mapping);
        (isFileExt ? FileMappings : TextMappings).Add(row);
        OnPropertyChanged(nameof(HasAnyMappings));
        NotifyMoveability();
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
    /// 模拟测试: 先 SyncToModel 再深拷贝快照随请求发出 (未保存修改也能测试);
    /// 命中时高亮对应行并展示菜单键位预览; 400 展示后端 message。
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

        // 评审 C2: 深拷贝快照前先投影两分区回模型 (与本注释口径一致),
        // 否则「新加映射后直接 ▶ 测试」时快照里没有新映射
        SyncToModel();
        var snapshot = JsonSerializer.Deserialize<SelectedAction>(
            JsonSerializer.Serialize(Sa, SettingsJson.Options), SettingsJson.Options);
        var resp = await Api.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = TestContent,
            IsFile = TestIsFile,
            SelectedAction = snapshot,
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
                _ => "hello world",
            }
            : "C:\\example\\photo.jpg";
        _ = RunTestCommand.ExecuteAsync(null);
    }

    /// <summary>命中回显: 按后端返回的 matchType/matchValue 找到对应行 (按值匹配, 忽略大小写)。</summary>
    private MappingRowVm? FindRow(string matchType, string matchValue)
    {
        var list = matchType == "textType" ? TextMappings : FileMappings;
        foreach (var row in list)
        {
            if (string.Equals(row.Mapping.MatchValue, matchValue, StringComparison.OrdinalIgnoreCase)) return row;
        }
        return null;
    }

    private void SetMatchedRow(MappingRowVm? row)
    {
        foreach (var r in TextMappings) r.IsMatched = ReferenceEquals(r, row);
        foreach (var r in FileMappings) r.IsMatched = ReferenceEquals(r, row);
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
        foreach (var row in TextMappings.Concat(FileMappings))
        {
            row.RefreshChips();
            foreach (var editor in row.Editors)
            {
                editor.RefreshOptions();
            }
        }
        AddPanel?.RefreshPicks();
    }

    // ------------------------------------------------------------- 保存 / 写回

    /// <summary>两分区投影回模型 (textType 在前; 行 VM 直接持有底层对象, 属性修改天然同步)。
    /// internal: 页面自身 SaveConfigAsync 调用之外, MainViewModel.SaveAsync 咽喉 (评审 C1)
    /// 也要在节流判断前调用, 保证 Ctrl+S / 侧栏「保存」等外部入口把新加映射写进载荷。</summary>
    internal void SyncToModel()
    {
        Sa.Mappings.Clear();
        foreach (var row in TextMappings) Sa.Mappings.Add(row.Mapping);
        foreach (var row in FileMappings) Sa.Mappings.Add(row.Mapping);
    }

    /// <summary>主配置保存 (启用开关/删除映射语义为「立即保存」故跳过节流)。</summary>
    public Task<bool> SaveConfigAsync()
    {
        SyncToModel();
        return _main.SaveAsync(force: true);
    }

    /// <summary>
    /// 保存前把关联分组的条件值修改写回 Config.FileGroups 对应条目
    /// (评审 F1: 调用点在 MainViewModel.SaveAsync 统一咽喉; F2: 关联意图存于行 VM 的
    /// AssociatedGroupName —— 选分组建立 / 清空值解除 / 初始按值推导 / 手改后缀保持)。
    /// 多行关联同一分组时后写者胜, 无需加锁。
    /// </summary>
    internal void ApplyFileGroupWriteBack()
    {
        foreach (var row in FileMappings)
        {
            if (row.AssociatedGroupName is null) continue;
            var parsed = ActionSchemeCatalog.NormalizeExts(row.Mapping.MatchValue);
            if (parsed.Count == 0) continue;
            var group = Config.FileGroups.FirstOrDefault(g => g.Name == row.AssociatedGroupName);
            if (group is null) continue;
            if (ActionSchemeCatalog.SameExts(parsed, group.Exts)) continue;
            group.Exts = parsed;
        }
    }

    // ------------------------------------------------------------- 语言刷新

    public void OnLanguageChanged()
    {
        LanguageTick++;
        OnPropertyChanged(nameof(EnableLabel));
        OnPropertyChanged(nameof(HotkeyHintText)); // 条件拼接文案, 语言切换需重算
        foreach (var row in TextMappings) row.RefreshLanguage();
        foreach (var row in FileMappings) row.RefreshLanguage();
        AddPanel?.RefreshLanguage();
        if (HasResult && ResultMatched)
        {
            // 结果文案为即时拼接, 触发重算
            OnPropertyChanged(nameof(MatchedTypeText));
            OnPropertyChanged(nameof(MatchedValueText));
            OnPropertyChanged(nameof(PreviewText));
        }
    }
}

