using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;
/// <summary>
/// 一行映射: <see cref="SelectedMapping"/> 的 UI 投影, 直接持有底层对象引用。
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
        RefreshGroupToggles(); // 分组 Toggle 初始勾选态 (依赖 ResolveInitialAssociation 的推导结果)
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

    /// <summary>行摘要 (删除确认等场景)。</summary>
    public string MatchSummary
        => $"{TypeBadgeText}: {(Mapping.MatchValue.Trim().Length == 0 ? I18n.T("999") : Mapping.MatchValue)}";

    /// <summary>语言切换刻度, 行内绑定读此重译。</summary>
    public int LanguageTick => _page.LanguageTick;

    /// <summary>条件值 (fileExt 可编辑; textType 经下拉改)。手改后缀保持分组关联并写回。</summary>
    public string MatchValueDisplay
    {
        get => Mapping.MatchValue;
        set
        {
            if (Mapping.MatchValue == value) return;
            Mapping.MatchValue = value;
            OnPropertyChanged();
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
        SnapshotCurrentPremise();
        Mapping.MatchValue = value;
        OnPropertyChanged(nameof(MatchValueDisplay));
        OnPropertyChanged(nameof(MatchSummary));
        // Toggle 勾选态由 MatchValue 派生, 数据变了必须自己发全通知
        // (只靠事件回调时旧项可能残留点亮)
        NotifyTogglesChanged();
        RestoreOrRebindForCurrentPremise();
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
        SnapshotCurrentPremise();
        Mapping.MatchValue = value.Value;
        OnPropertyChanged(nameof(MatchValueDisplay));
        OnPropertyChanged(nameof(MatchSummary));
        NotifyTogglesChanged(); // 与 SetTextType 同理: Toggle 视觉态随 MatchValue 同步
        RestoreOrRebindForCurrentPremise();
    }

    // ---- fileExt 行: 分组快捷填入 (评审 F2 语义; UI 为分组 Toggle, 驱动既有 FileGroupSelected 链路) ----

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

    /// <summary>关联分组名 (null=无)。保存时后缀修改写回该分组。</summary>
    public string? AssociatedGroupName { get; internal set; }

    [ObservableProperty]
    private ComboOption? _fileGroupSelected;

    partial void OnFileGroupSelectedChanged(ComboOption? value)
    {
        if (_applying || value is null) return;
        SnapshotCurrentPremise(); // 切换前暂存当前前提的行为配置
        if (value.Value.Length == 0)
        {
            // 「无」: 解除关联并清空条件值; 前提回退通用文件集
            AssociatedGroupName = null;
            MatchValueDisplay = "";
            RestoreOrRebindForCurrentPremise();
            return;
        }
        var group = FileGroups.FirstOrDefault(g => g.Name == value.Value);
        if (group is null) return;
        MatchValueDisplay = string.Join(", ", group.Exts); // 触发编辑器选项刷新
        AssociatedGroupName = group.Name;
        RestoreOrRebindForCurrentPremise(); // 分组 Toggle = 离散前提切换 (2026-09-10 语义)
        NotifyGroupToggles(); // 互斥: 广播全组重估 (旧亮项熄灭, 派生态不依赖路由事件时序)
    }

    /// <summary>分组 Toggle 集 (每组一个; 勾选态由 FileGroupSelected 派生)。</summary>
    public ObservableCollection<FileGroupToggleVm> GroupToggles { get; } = [];

    /// <summary>重建分组 Toggle (构造/语言切换/分组列表可能变化时; 顺带刷新可见性)。</summary>
    public void RefreshGroupToggles()
    {
        GroupToggles.Clear();
        foreach (var g in FileGroups)
        {
            GroupToggles.Add(new FileGroupToggleVm(this, g.Name, g.Label));
        }
        OnPropertyChanged(nameof(ShowFileGroupFill));
    }

    /// <summary>FileGroupSelected 变化后广播各 Toggle 勾选态 (含「无」路径)。</summary>
    private void NotifyGroupToggles()
    {
        foreach (var t in GroupToggles) t.NotifyChecked();
    }

    private void SyncFileGroupSelected()
    {
        _applying = true;
        FileGroupSelected = AssociatedGroupName is null
            ? null
            : FileGroupOptions.FirstOrDefault(o => o.Value == AssociatedGroupName);
        _applying = false;
        NotifyGroupToggles(); // 勾选态随关联/解除同步 (清空值解除关联路径由此覆盖)
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

    /// <summary>是否为所在分区的首行 (分区标题在卡内首行展示, 多行时不重复)。</summary>
    [ObservableProperty]
    private bool _isFirstInPartition;

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
        OnPropertyChanged(nameof(AddEntryHint));
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
        // 覆盖集随前提变化, 单行为判定随之刷新 (如 url 双行为 <-> magnet 单行为)
        OnPropertyChanged(nameof(CanAddEntry));
        OnPropertyChanged(nameof(AddEntryHint));
    }

    // ---- 前提切换的行为快照 (仅会话内存) ----

    // 切走前提前深拷贝暂存 Entries, 切回时还原; 无快照则走重绑规则。
    // 不持久化: 保存后重启即丢, 由重绑规则兜底。
    private readonly Dictionary<string, List<SelectedEntry>> _premiseSnapshots = new();

    private static List<SelectedEntry> CloneEntries(IEnumerable<SelectedEntry> source)
        => source.Select(e => new SelectedEntry
        {
            Behavior = e.Behavior,
            ActionValue = e.ActionValue,
            WorkingDir = e.WorkingDir,
            Options = new RuleOptions
            {
                CopyToClipboard = e.Options.CopyToClipboard,
                ClearSelection = e.Options.ClearSelection,
                Confirm = e.Options.Confirm,
            },
        }).ToList();

    /// <summary>切换前提前暂存当前行为配置 (键 = 当前 MatchValue; 每次覆盖, 保留最新配置)。</summary>
    private void SnapshotCurrentPremise()
        => _premiseSnapshots[Mapping.MatchValue] = CloneEntries(Mapping.Entries);

    /// <summary>前提已切到 Mapping.MatchValue 后: 有快照则整体还原, 否则按重绑规则落到该前提默认。</summary>
    private void RestoreOrRebindForCurrentPremise()
    {
        if (_premiseSnapshots.TryGetValue(Mapping.MatchValue, out var snapshot))
        {
            Mapping.Entries.Clear();
            foreach (var e in CloneEntries(snapshot)) Mapping.Entries.Add(e);
            if (IsExpanded) OpenEditor(); // 展开态重建编辑行 (绑新 entry 对象)
            RefreshChips();
            RefreshEditorOptions();
        }
        else
        {
            RebindEditorsToPremise();
        }
    }

    /// <summary>
    /// 前提切换联动: 不适用新前提的 entry 换为该前提默认行为。
    /// 直接改底层 Entries。仅供离散切换调用, 逐字符输入不走此链路 (避免打字中途重置)。
    /// </summary>
    private void RebindEditorsToPremise()
    {
        var covering = BehaviorCatalog.Covering(Mapping.MatchType, Mapping.MatchValue)
            .Select(p => p.Id).ToHashSet();
        var def = BehaviorCatalog.DefaultFor(Mapping.MatchType, Mapping.MatchValue);
        foreach (var entry in Mapping.Entries)
        {
            if (covering.Contains(entry.Behavior) || def is null) continue;
            entry.Behavior = def;
            entry.ActionValue = BehaviorCatalog.IsNoValue(def) ? "" : BehaviorCatalog.DefaultTemplateFor(def);
        }
        if (IsExpanded) OpenEditor(); // 展开态重建编辑行 (下拉副本/提示随新行为)
        RefreshChips();
        RefreshEditorOptions();
    }

    // ---- 行为增删 / 排序 (手风琴内) ----

    /// <summary>
    /// 约束: 行为数达 9, 或覆盖集可用行为已全部占用时禁用 (再加必重复); 空行可加第一个。
    /// </summary>
    public bool CanAddEntry
    {
        get
        {
            if (Mapping.Entries.Count >= 9) return false;
            if (Mapping.Entries.Count == 0) return true;
            var used = Mapping.Entries.Select(e => e.Behavior).ToHashSet();
            return BehaviorCatalog.Covering(MatchType, Mapping.MatchValue).Any(p => !used.Contains(p.Id));
        }
    }

    /// <summary>「添加行为」禁用原因提示 (1107 达 9 上限 / 1119 可用行为已全部添加)。</summary>
    public string AddEntryHint => Mapping.Entries.Count >= 9 ? I18n.T("1107") : I18n.T("1119");

    [RelayCommand]
    private void AddEntry()
    {
        if (!CanAddEntry) return;
        // 取覆盖集中第一个未占用行为; 覆盖集为空的脏值行回退 open
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

    [RelayCommand]
    private void ToggleExpand() => _page.ExpandedRow = IsExpanded ? null : this;

    /// <summary>行内 ▶ 测试: 预填底部模拟条并立即执行。</summary>
    [RelayCommand]
    private void TestRow() => _page.RunTestFor(this);

    [RelayCommand]
    private void AskRemove() => _ = _page.AskRemoveAsync(this);

    /// <summary>语言切换: 下拉副本/摘要即时拼接刷新。</summary>
    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(LanguageTick));
        OnPropertyChanged(nameof(TypeBadgeText));
        OnPropertyChanged(nameof(MatchSummary));
        OnPropertyChanged(nameof(MatchHint));
        OnPropertyChanged(nameof(TextTypeOptions));
        OnPropertyChanged(nameof(FileGroupOptions));
        OnPropertyChanged(nameof(ShowFileGroupFill));
        RefreshGroupToggles(); // 分组列表可能已变 (他页增删), 重建 Toggle 集与标签
        RefreshChips();
        foreach (var editor in Editors) editor.RefreshLanguage();
    }
}
