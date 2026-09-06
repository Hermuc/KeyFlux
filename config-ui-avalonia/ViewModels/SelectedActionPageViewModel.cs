using System.Collections.ObjectModel;
using System.Text.Json;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using MyKeymap.Settings.Models;
using MyKeymap.Settings.Services;

namespace MyKeymap.Settings.ViewModels;

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
// MyKeymap 进程重启。分组后缀写回 (评审 F1/F2 语义) 由咽喉调用
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
/// 手风琴内单个行为的编辑行 (直接持有底层 <see cref="SelectedEntry"/> 引用):
/// 行为下拉 (按 BehaviorCatalog.Covering 过滤, 脏值恒可见)、命令模板、工作目录、
/// Options 三开关与排序/删除。
/// </summary>
public sealed partial class EntryRowVm : ObservableObject
{
    private readonly MappingRowVm _row;

    public EntryRowVm(MappingRowVm row, SelectedEntry entry)
    {
        _row = row;
        Entry = entry;
        _behaviorSelected = BuildBehaviorOptions().FirstOrDefault(o => o.Value == entry.Behavior);
    }

    public SelectedEntry Entry { get; }

    // ---- 位置与可用性 (由 MappingRowVm 在增删/排序后推送刷新) ----

    private int _index = 1;
    private bool _isFirst;
    private bool _isLast;

    /// <summary>菜单键位序号 (1 起)。</summary>
    public int Index
    {
        get => _index;
        private set => SetProperty(ref _index, value);
    }

    public bool CanMoveUp => !_isFirst;
    public bool CanMoveDown => !_isLast;

    /// <summary>至少保留一个行为 (约束: 最后一个行为 ✕ 禁用)。</summary>
    public bool CanRemove => !_isLast || _row.Editors.Count > 1;

    internal void RefreshPosition(int index, bool isFirst, bool isLast)
    {
        Index = index + 1;
        _isFirst = isFirst;
        _isLast = isLast;
        OnPropertyChanged(nameof(CanMoveUp));
        OnPropertyChanged(nameof(CanMoveDown));
        OnPropertyChanged(nameof(CanRemove));
    }

    // ---- 行为选择 ----

    /// <summary>行为下拉 (预翻译副本; 脏值插首位保持可见)。</summary>
    public List<ComboOption> BuildBehaviorOptions()
    {
        var opts = BehaviorCatalog.Covering(_row.Mapping.MatchType, _row.Mapping.MatchValue)
            .Select(p => new ComboOption(p.Id, BehaviorCatalog.LabelFor(p.Id)))
            .ToList();
        if (Entry.Behavior.Length > 0 && opts.All(o => o.Value != Entry.Behavior))
        {
            opts.Insert(0, new ComboOption(Entry.Behavior, BehaviorCatalog.LabelFor(Entry.Behavior)));
        }
        return opts;
    }

    public List<ComboOption> BehaviorOptions => BuildBehaviorOptions();

    [ObservableProperty]
    private ComboOption? _behaviorSelected;

    partial void OnBehaviorSelectedChanged(ComboOption? value)
    {
        if (value is null || value.Value == Entry.Behavior) return;
        Entry.Behavior = value.Value;
        // 切换行为重置命令模板为包默认 (无参行为清空), 与旧编辑器一致
        Entry.ActionValue = BehaviorCatalog.IsNoValue(value.Value)
            ? ""
            : BehaviorCatalog.DefaultTemplateFor(value.Value);
        OnPropertyChanged(nameof(ActionValue));
        OnPropertyChanged(nameof(IsNoValue));
        OnPropertyChanged(nameof(ShowTemplate));
        OnPropertyChanged(nameof(TemplateHint));
        _row.RefreshChips();
    }

    /// <summary>行为提示 (包 description)。</summary>
    public string BehaviorHint => BehaviorCatalog.HintFor(Entry.Behavior);

    // ---- 无参语义与模板 ----

    public bool IsNoValue => BehaviorCatalog.IsNoValue(Entry.Behavior);
    public bool ShowTemplate => !IsNoValue;
    public string TemplateHint => IsNoValue ? I18n.T("1013") + I18n.T("1014") : "";

    /// <summary>目标命令 / URL / 脚本 (命令模板)。</summary>
    public string ActionValue
    {
        get => Entry.ActionValue;
        set
        {
            if (Entry.ActionValue == value) return;
            Entry.ActionValue = value;
            OnPropertyChanged();
        }
    }

    /// <summary>工作目录 (可选)。</summary>
    public string WorkingDir
    {
        get => Entry.WorkingDir;
        set
        {
            if (Entry.WorkingDir == value) return;
            Entry.WorkingDir = value;
            OnPropertyChanged();
        }
    }

    // ---- Options 三开关 (RuleOptions; 评审 D1 定案: 方案 D 下 options 三开关暂缓消费,
    // UI 本版不呈现, 属性保留以兼容存量数据往返, 前向兼容恢复) ----

    public bool CopyToClipboard
    {
        get => Entry.Options.CopyToClipboard;
        set { if (Entry.Options.CopyToClipboard == value) return; Entry.Options.CopyToClipboard = value; OnPropertyChanged(); }
    }

    public bool ClearSelection
    {
        get => Entry.Options.ClearSelection;
        set { if (Entry.Options.ClearSelection == value) return; Entry.Options.ClearSelection = value; OnPropertyChanged(); }
    }

    public bool Confirm
    {
        get => Entry.Options.Confirm;
        set { if (Entry.Options.Confirm == value) return; Entry.Options.Confirm = value; OnPropertyChanged(); }
    }

    // ---- 排序 / 删除 ----

    [RelayCommand]
    private void MoveUp() => _row.MoveEntry(this, -1);

    [RelayCommand]
    private void MoveDown() => _row.MoveEntry(this, 1);

    [RelayCommand]
    private void Remove() => _row.RemoveEntry(this);

    /// <summary>行为下拉候选刷新 (前提/目录变化; 由行 VM 与页面跨实例调用)。</summary>
    public void RefreshOptions()
    {
        OnPropertyChanged(nameof(BehaviorOptions));
        OnPropertyChanged(nameof(BehaviorHint));
    }

    /// <summary>语言切换: 刷新行为下拉副本与即时拼接文案。</summary>
    public void RefreshLanguage()
    {
        RefreshOptions();
        OnPropertyChanged(nameof(TemplateHint));
        OnPropertyChanged(nameof(ActionValue));
        OnPropertyChanged(nameof(WorkingDir));
    }
}

/// <summary>
/// 一行映射 (一个 <see cref="SelectedMapping"/> 的 UI 投影, 直接持有底层对象引用):
/// 类型徽章 + 条件值编辑 + chips 键位表 + 行内手风琴 (同屏只开一个, 由页面 ExpandedRow 统一仲裁)。
/// </summary>
public sealed partial class MappingRowVm : ObservableObject
{
    private readonly SelectedActionPageViewModel _page;
    private bool _applying; // 快捷填入联动区间, 防递归

    public MappingRowVm(SelectedActionPageViewModel page, SelectedMapping mapping)
    {
        _page = page;
        Mapping = mapping;
        if (IsTextType)
        {
            // 特征下拉选中项跟随现有值 (写入处于 _applying 区间, 不触发联动)
            _applying = true;
            _textTypeSelected = TextTypeOptions.FirstOrDefault(o => o.Value == mapping.MatchValue);
            _applying = false;
        }
        ResolveInitialAssociation();
        RefreshChips();
    }

    public SelectedMapping Mapping { get; }

    /// <summary>文件分组 (快捷填入数据源)。</summary>
    public IReadOnlyList<FileGroup> FileGroups => _page.FileGroups;

    // ---- 类型 ----

    public string MatchType => Mapping.MatchType;
    public bool IsTextType => MatchType == "textType";

    /// <summary>类型徽章文本 (文本特征/文件后缀)。</summary>
    public string TypeBadgeText => ActionSchemeCatalog.MatchTypeLabel(MatchType);

    /// <summary>类型徽章色: 文本蓝 / 后缀橙。</summary>
    public string TypeBadgeColorHex => IsTextType ? BehaviorBadgeColors.LinkBlue : BehaviorBadgeColors.ExtOrange;

    // ---- 条件值 ----

    /// <summary>条件值徽章 (空时显示「(未设置)」)。</summary>
    public string MatchValueBadge
        => Mapping.MatchValue.Trim().Length == 0 ? I18n.T("999") : Mapping.MatchValue;

    /// <summary>行摘要 (删除确认等场景)。</summary>
    public string MatchSummary
        => $"{TypeBadgeText}: {(Mapping.MatchValue.Trim().Length == 0 ? I18n.T("999") : Mapping.MatchValue)}";

    /// <summary>
    /// 条件值 (fileExt 行可编辑; textType 行经下拉改)。
    /// 手改保持分组关联 (写回语义: 关联分组的后缀修改保存时写回); 清空值解除关联。
    /// </summary>
    public string MatchValueDisplay
    {
        get => Mapping.MatchValue;
        set
        {
            if (Mapping.MatchValue == value) return;
            Mapping.MatchValue = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(MatchValueBadge));
            OnPropertyChanged(nameof(MatchSummary));
            if (!IsTextType && value.Trim().Length == 0)
            {
                AssociatedGroupName = null; // 清空条件值解除关联
                SyncFileGroupSelected();
            }
            RefreshEditorOptions();
        }
    }

    /// <summary>匹配提示 (1034/1035)。</summary>
    public string MatchHint => ActionSchemeCatalog.MatchTypeHint(MatchType);

    // ---- textType 行: 特征四选一 (Toggle 直选, 替代 ComboBox——所见即所选, 无 ComboOption 概念) ----

    /// <summary>四个互斥开关的公共读写: 直接落 Mapping.MatchValue (含 UI 联动)。</summary>
    public bool IsUrl
    {
        get => Mapping.MatchValue == "url";
        set { if (value) SetTextType("url"); }
    }
    public bool IsPath
    {
        get => Mapping.MatchValue == "path";
        set { if (value) SetTextType("path"); }
    }
    public bool IsMagnet
    {
        get => Mapping.MatchValue == "magnet";
        set { if (value) SetTextType("magnet"); }
    }
    public bool IsPlain
    {
        get => Mapping.MatchValue == "plain";
        set { if (value) SetTextType("plain"); }
    }

    private void SetTextType(string value)
    {
        if (Mapping.MatchValue == value) return;
        Mapping.MatchValue = value;
        OnPropertyChanged(nameof(MatchValueDisplay));
        OnPropertyChanged(nameof(MatchValueBadge));
        OnPropertyChanged(nameof(MatchSummary));
        RefreshEditorOptions();
    }

    /// <summary>切换任一 Toggle 时同步其余三个的视觉态。</summary>
    public void NotifyTogglesChanged()
    {
        OnPropertyChanged(nameof(IsUrl));
        OnPropertyChanged(nameof(IsPath));
        OnPropertyChanged(nameof(IsMagnet));
        OnPropertyChanged(nameof(IsPlain));
    }

    // ---- textType 行: 特征下拉 (保留: 旧序列化/兼容路径) ----

    /// <summary>文本特征下拉 (url/path/magnet/plain, 预翻译副本)。</summary>
    public List<ComboOption> TextTypeOptions
        => ActionSchemeCatalog.TextTypes
            .Select(t => new ComboOption(t.Value, I18n.T(t.LabelKey)))
            .ToList();

    [ObservableProperty]
    private ComboOption? _textTypeSelected;

    partial void OnTextTypeSelectedChanged(ComboOption? value)
    {
        if (_applying || value is null || value.Value == Mapping.MatchValue) return;
        Mapping.MatchValue = value.Value;
        OnPropertyChanged(nameof(MatchValueDisplay));
        OnPropertyChanged(nameof(MatchValueBadge));
        OnPropertyChanged(nameof(MatchSummary));
        RefreshEditorOptions();
    }

    // ---- fileExt 行: 分组快捷填入 (评审 F2 语义) ----

    /// <summary>分组快捷填入可见性 (fileExt 且存在分组)。</summary>
    public bool ShowFileGroupFill => !IsTextType && FileGroups.Count > 0;

    /// <summary>分组下拉 (「无」+ 分组; 实时构建, 分组列表变化即生效)。</summary>
    public List<ComboOption> FileGroupOptions
    {
        get
        {
            var opts = new List<ComboOption> { new("", I18n.T("1008")) };
            opts.AddRange(FileGroups.Select(g => new ComboOption(g.Name, g.Label)));
            return opts;
        }
    }

    /// <summary>
    /// 显式关联的分组名 (null=无): 分组填入建立 / 清空值解除 / 初始按值推导;
    /// 手改后缀保持关联, 保存时把修改写回该分组。
    /// </summary>
    public string? AssociatedGroupName { get; internal set; }

    [ObservableProperty]
    private ComboOption? _fileGroupSelected;

    partial void OnFileGroupSelectedChanged(ComboOption? value)
    {
        if (_applying || value is null) return;
        if (value.Value.Length == 0)
        {
            // 「无」: 解除关联并清空条件值
            AssociatedGroupName = null;
            MatchValueDisplay = "";
            return;
        }
        var group = FileGroups.FirstOrDefault(g => g.Name == value.Value);
        if (group is null) return;
        MatchValueDisplay = string.Join(", ", group.Exts); // 触发编辑器选项刷新
        AssociatedGroupName = group.Name;
    }

    private void SyncFileGroupSelected()
    {
        _applying = true;
        FileGroupSelected = AssociatedGroupName is null
            ? null
            : FileGroupOptions.FirstOrDefault(o => o.Value == AssociatedGroupName);
        _applying = false;
    }

    /// <summary>初始关联推导 (按值命中分组; 手改后缀后由 AssociatedGroupName 保持, 不再重推导)。</summary>
    private void ResolveInitialAssociation()
    {
        if (IsTextType) return;
        var parsed = ActionSchemeCatalog.NormalizeExts(Mapping.MatchValue);
        if (parsed.Count > 0)
        {
            var group = FileGroups.FirstOrDefault(g => ActionSchemeCatalog.SameExts(parsed, g.Exts));
            if (group is not null) AssociatedGroupName = group.Name;
        }
        _applying = true;
        FileGroupSelected = AssociatedGroupName is null
            ? null
            : FileGroupOptions.FirstOrDefault(o => o.Value == AssociatedGroupName);
        _applying = false;
    }

    // ---- chips 键位表 ----

    public ObservableCollection<EntryChipVm> Chips { get; } = [];

    /// <summary>重建 chips (entries 增删/排序/换行为后; 序号自动顺延)。</summary>
    public void RefreshChips()
    {
        Chips.Clear();
        for (var i = 0; i < Mapping.Entries.Count; i++)
        {
            var e = Mapping.Entries[i];
            Chips.Add(new EntryChipVm(i + 1, BehaviorCatalog.LabelFor(e.Behavior), BehaviorBadgeColors.ForBehavior(e.Behavior)));
        }
        OnPropertyChanged(nameof(CanAddEntry));
    }

    // ---- 手风琴编辑器 ----

    [ObservableProperty]
    private bool _isExpanded;

    [ObservableProperty]
    private bool _isMatched;

    public ObservableCollection<EntryRowVm> Editors { get; } = [];

    /// <summary>展开: 重建编辑行 (收起态由页面统一仲裁)。</summary>
    internal void OpenEditor()
    {
        Editors.Clear();
        foreach (var entry in Mapping.Entries)
        {
            Editors.Add(new EntryRowVm(this, entry));
        }
        PushPositions();
    }

    /// <summary>收起: 清空编辑行 (释放编辑中状态, 展开即重建)。</summary>
    internal void CloseEditor() => Editors.Clear();

    private void PushPositions()
    {
        for (var i = 0; i < Editors.Count; i++)
        {
            Editors[i].RefreshPosition(i, isFirst: i == 0, isLast: i == Editors.Count - 1);
        }
    }

    /// <summary>条件值/特征变化后刷新已打开编辑器的行为下拉 (覆盖集随前提变化)。</summary>
    private void RefreshEditorOptions()
    {
        foreach (var editor in Editors)
        {
            editor.RefreshOptions();
            // 现选行为不在新覆盖集时由 BuildBehaviorOptions 脏值插首位, 无需改动选中项
        }
    }

    // ---- 行为增删 / 排序 (手风琴内) ----

    /// <summary>约束: 行为数达 9 时「添加行为」禁用。</summary>
    public bool CanAddEntry => Mapping.Entries.Count < 9;

    [RelayCommand]
    private void AddEntry()
    {
        if (!CanAddEntry) return;
        // 默认取覆盖集中第一个未占用的行为; 全占用回退第一条
        var covering = BehaviorCatalog.Covering(MatchType, Mapping.MatchValue);
        var used = Mapping.Entries.Select(e => e.Behavior).ToHashSet();
        var id = covering.FirstOrDefault(p => !used.Contains(p.Id))?.Id
                 ?? covering.FirstOrDefault()?.Id ?? "open";
        Mapping.Entries.Add(new SelectedEntry
        {
            Behavior = id,
            ActionValue = BehaviorCatalog.IsNoValue(id) ? "" : BehaviorCatalog.DefaultTemplateFor(id),
            WorkingDir = "",
            Options = new RuleOptions(),
        });
        RefreshChips();
        var editor = new EntryRowVm(this, Mapping.Entries[^1]);
        Editors.Add(editor);
        PushPositions();
    }

    internal void MoveEntry(EntryRowVm editor, int dir)
    {
        var index = Editors.IndexOf(editor);
        var target = index + dir;
        if (index < 0 || target < 0 || target >= Mapping.Entries.Count) return;
        (Mapping.Entries[index], Mapping.Entries[target]) = (Mapping.Entries[target], Mapping.Entries[index]);
        Editors.Move(index, target); // 编辑器集合同步换位 (Move 保留对象引用, 编辑中状态不丢)
        RefreshChips();
        PushPositions(); // 按新顺序重推序号/边界
    }

    internal void RemoveEntry(EntryRowVm editor)
    {
        if (Mapping.Entries.Count <= 1) return; // 约束: 至少保留一个行为
        var index = Editors.IndexOf(editor);
        if (index < 0) return;
        Mapping.Entries.RemoveAt(index);
        Editors.RemoveAt(index);
        RefreshChips();
        PushPositions();
    }

    // ---- 行级操作 ----

    public bool CanMoveUp => _page.CanMove(this, -1);
    public bool CanMoveDown => _page.CanMove(this, 1);

    internal void NotifyMoveability()
    {
        OnPropertyChanged(nameof(CanMoveUp));
        OnPropertyChanged(nameof(CanMoveDown));
    }

    [RelayCommand]
    private void MoveUp() => _page.MoveMapping(this, -1);

    [RelayCommand]
    private void MoveDown() => _page.MoveMapping(this, 1);

    [RelayCommand]
    private void ToggleExpand() => _page.ExpandedRow = IsExpanded ? null : this;

    /// <summary>行内 ▶ 测试: 预填底部模拟条并立即执行。</summary>
    [RelayCommand]
    private void TestRow() => _page.RunTestFor(this);

    [RelayCommand]
    private void AskRemove() => _ = _page.AskRemoveAsync(this);

    /// <summary>语言切换: 徽章/下拉副本/摘要即时拼接刷新。</summary>
    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(TypeBadgeText));
        OnPropertyChanged(nameof(MatchValueBadge));
        OnPropertyChanged(nameof(MatchSummary));
        OnPropertyChanged(nameof(MatchHint));
        OnPropertyChanged(nameof(TextTypeOptions));
        OnPropertyChanged(nameof(FileGroupOptions));
        OnPropertyChanged(nameof(ShowFileGroupFill));
        RefreshChips();
        foreach (var editor in Editors) editor.RefreshLanguage();
    }
}

/// <summary>添加映射弹窗内的行为勾选项 (勾选顺序 = 菜单键位顺序)。</summary>
public sealed partial class BehaviorPickVm : ObservableObject
{
    private readonly AddMappingVm _panel;

    public BehaviorPickVm(AddMappingVm panel, BehaviorPack pack)
    {
        _panel = panel;
        Pack = pack;
    }

    public BehaviorPack Pack { get; }

    /// <summary>行为显示名 (按语言)。</summary>
    public string Label => BehaviorCatalog.LabelFor(Pack.Id);

    /// <summary>行为提示 (description)。</summary>
    public string Hint => BehaviorCatalog.HintFor(Pack.Id);

    /// <summary>键位序号 = 勾选列表中的位置 (位置序, 1 起; 0=未勾选)。</summary>
    public int Order
    {
        get
        {
            var picks = _panel.BehaviorPicks;
            var order = 0;
            foreach (var p in picks)
            {
                if (!p.IsChecked) continue;
                order++;
                if (ReferenceEquals(p, this)) return order;
            }
            return 0;
        }
    }

    private bool _isChecked;

    public bool IsChecked
    {
        get => _isChecked;
        set
        {
            if (_isChecked == value) return;
            // 约束: 最多勾选 9 个 (第 10 个勾选被拒绝回弹)
            if (value && _panel.PickedCount >= 9) return;
            _isChecked = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(Order));
            _panel.OnPickChanged();
        }
    }

    private bool _isEnabled = true;

    /// <summary>勾满 9 个后未勾项禁用。</summary>
    public bool IsEnabled
    {
        get => _isEnabled;
        private set => SetProperty(ref _isEnabled, value);
    }

    internal void RefreshGate() => IsEnabled = _isChecked || _panel.PickedCount < 9;

    /// <summary>标签/序号外部刷新 (列表序重排 / 语言切换; 由弹窗 VM 调用)。</summary>
    public void RefreshDisplay()
    {
        OnPropertyChanged(nameof(Label));
        OnPropertyChanged(nameof(Hint));
        OnPropertyChanged(nameof(Order));
    }
}

/// <summary>
/// 添加映射弹窗 (页内 overlay 面板, 非独立窗口; VM 可单测):
/// 类型四选一 (文件后缀 + 文本特征 url/path/magnet/plain) -> 条件值 (fileExt 支持分组快捷填入)
/// -> 行为库勾选过滤 (BehaviorCatalog.Covering, 列表顺序即菜单顺序, 与勾选先后无关)。
/// </summary>
public sealed partial class AddMappingVm : ObservableObject
{
    private readonly SelectedActionPageViewModel _page;

    public AddMappingVm(SelectedActionPageViewModel page)
    {
        _page = page;
        // 匹配类型下拉: 具体文件后缀分组在前 (每个分组 = 一级类型), 文本特征四项在后;
        // 分组项 value 编码为 "group:<name>", 选中即定 MatchValue = 该组后缀集 (无需再填条件值)
        _typeOptions = [];
        foreach (var g in page.FileGroups)
        {
            _typeOptions.Add(new("group:" + g.Name, g.Label));
        }
        _typeOptions.Add(new("url", I18n.T("1059")));
        _typeOptions.Add(new("path", I18n.T("1060")));
        _typeOptions.Add(new("magnet", I18n.T("1061")));
        _typeOptions.Add(new("plain", I18n.T("1062")));
        _typeSelected = _typeOptions.FirstOrDefault();
        if (_typeSelected is not null && IsFileExt)
        {
            MatchValue = GroupMatchValue ?? ""; // 初始即分组项: 同步后缀集到条件值 (构造期回调不触发)
        }
        RebuildPicks();
    }

    /// <summary>弹窗说明 (1111)。</summary>
    public string Hint => I18n.T("1111");

    // ---- 步骤 1: 类型 ----

    private readonly List<ComboOption> _typeOptions;

    public List<ComboOption> TypeOptions => _typeOptions;

    [ObservableProperty]
    private ComboOption? _typeSelected;

    partial void OnTypeSelectedChanged(ComboOption? value)
    {
        if (value is null) return;
        if (IsFileExt)
        {
            MatchValue = GroupMatchValue ?? ""; // 分组项: 条件值 = 该组后缀集
        }
        OnPropertyChanged(nameof(IsFileExt));
        OnPropertyChanged(nameof(MatchHint));
        RebuildPicks(); // 无条件重建: 各类型覆盖集不同 (fileExt 按条件值 / 文本特征按特征词)
    }

    /// <summary>当前类型是否文件后缀分组项 (分组自带后缀集, 条件值输入框不出现)。</summary>
    public bool IsFileExt => TypeSelected?.Value?.StartsWith("group:") == true;

    /// <summary>分组项对应的后缀集 (matchValue), 非分组项为 null。</summary>
    public string? GroupMatchValue
        => IsFileExt
            ? _page.FileGroups.FirstOrDefault(g => "group:" + g.Name == TypeSelected?.Value)?.Exts
                is { Count: > 0 } exts
                ? string.Join(",", exts)
                : null
            : null;

    /// <summary>匹配提示 (fileExt=1034 / textType=1035)。</summary>
    public string MatchHint => ActionSchemeCatalog.MatchTypeHint(IsFileExt ? "fileExt" : "textType");

    // ---- 步骤 2: 条件值 (仅 fileExt) ----

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanConfirm))] // 手输条件值 / 分组填值后刷新确认按钮可用性
    private string _matchValue = "";

    /// <summary>分组下拉 (「无」+ 分组)。</summary>
    public List<ComboOption> FileGroupOptions
    {
        get
        {
            var opts = new List<ComboOption> { new("", I18n.T("1008")) };
            opts.AddRange(_page.FileGroups.Select(g => new ComboOption(g.Name, g.Label)));
            return opts;
        }
    }

    [ObservableProperty]
    private ComboOption? _fileGroupSelected;

    partial void OnFileGroupSelectedChanged(ComboOption? value)
    {
        if (value is null) return;
        if (value.Value.Length == 0)
        {
            MatchValue = "";
            return;
        }
        var group = _page.FileGroups.FirstOrDefault(g => g.Name == value.Value);
        if (group is null) return;
        MatchValue = string.Join(", ", group.Exts);
    }

    // ---- 步骤 3: 行为勾选 ----

    public ObservableCollection<BehaviorPickVm> BehaviorPicks { get; } = [];

    /// <summary>按当前类型/条件值重建勾选列表 (Covering 过滤; 已勾状态保留)。</summary>
    private void RebuildPicks()
    {
        var matchType = IsFileExt ? "fileExt" : "textType";
        var matchValue = IsFileExt ? (GroupMatchValue ?? MatchValue) : TypeSelected?.Value ?? "";
        var covering = BehaviorCatalog.Covering(matchType, matchValue);
        var checkedIds = BehaviorPicks.Where(p => p.IsChecked).Select(p => p.Pack.Id).ToHashSet();
        BehaviorPicks.Clear();
        foreach (var pack in covering)
        {
            var pick = new BehaviorPickVm(this, pack) { IsChecked = checkedIds.Contains(pack.Id) };
            BehaviorPicks.Add(pick);
        }
        RefreshGates();
        OnPropertyChanged(nameof(CanConfirm));
    }
    
    /// <summary>行为目录变化后重建勾选列表 (保留已勾状态); 供页面 RefreshBehaviorOptions 调用。</summary>
    public void RefreshPicks() => RebuildPicks();

    /// <summary>已勾选行为数。</summary>
    public int PickedCount => BehaviorPicks.Count(p => p.IsChecked);

    /// <summary>确认可用: fileExt 需要非空条件值, 且至少勾选一个行为。</summary>
    public bool CanConfirm
        => (!IsFileExt || MatchValue.Trim().Length > 0) && PickedCount > 0;

    internal void OnPickChanged()
    {
        RefreshGates();
        OnPropertyChanged(nameof(CanConfirm));
    }

    private void RefreshGates()
    {
        foreach (var p in BehaviorPicks) p.RefreshGate();
        foreach (var p in BehaviorPicks) p.RefreshDisplay();
    }

    /// <summary>确认: 构造 SelectedMapping 插入对应分区 (由页面关闭弹窗并刷新)。</summary>
    [RelayCommand]
    private void Confirm() => _page.AddMapping(this);

    /// <summary>语言切换: 类型/分组/勾选标签刷新。</summary>
    public void RefreshLanguage()
    {
        foreach (var t in _typeOptions)
        {
            var idx = _typeOptions.IndexOf(t);
            _typeOptions[idx] = t.Value switch
            {
                "fileExt" => t with { Label = I18n.T("1031") },
                "url" => t with { Label = I18n.T("1059") },
                "path" => t with { Label = I18n.T("1060") },
                "magnet" => t with { Label = I18n.T("1061") },
                _ => t with { Label = I18n.T("1062") },
            };
        }
        OnPropertyChanged(nameof(TypeOptions));
        OnPropertyChanged(nameof(Hint));
        OnPropertyChanged(nameof(FileGroupOptions));
        foreach (var p in BehaviorPicks)
        {
            p.RefreshDisplay();
        }
    }
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
            HotkeyPendingSave = true; // 未保存提示条 (与旧编辑器语义一致: 保存后才生效)
        }
    }

    /// <summary>空热键警示 (976)。</summary>
    public bool NoHotkeyWarning => string.IsNullOrEmpty(Sa.Hotkey);

    /// <summary>热键已修改未保存提示条 (1077; 保存成功后由 MainViewModel.SaveAsync 成功分支经 OnConfigSaved 复位)。</summary>
    [ObservableProperty]
    private bool _hotkeyPendingSave;

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

    private void NotifyMoveability()
    {
        foreach (var row in TextMappings.Concat(FileMappings)) row.NotifyMoveability();
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

    /// <summary>模拟选中内容 (复刻旧默认 "https://example.com")。</summary>
    [ObservableProperty]
    private string _testContent = "https://example.com";

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
                "url" => "https://example.com",
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
