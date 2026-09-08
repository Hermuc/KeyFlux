using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;
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

    /// <summary>语言切换刻度中继 (行内 XAML 的 Tr 绑定直接读此属性重译;
    /// 数值同步页面, 通知由页面 OnLanguageChanged -> RefreshLanguage 推送, 免事件订阅)。</summary>
    public int LanguageTick => _page.LanguageTick;

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
        // 四个 Toggle 的勾选态由 MatchValue 派生, 必须在此同步:
        // 只靠 Checked 事件回调时机不可靠 (事件先于绑定推值触发时, 旧项残留点亮,
        // 实测"最多同时亮两个") —— 数据变了必须自己发全通知。
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

    // ---- 前提切换的行为快照 (UI 会话级记忆) ----

    // 痛点: 行为配置单值存储在 Entries 上, 切走前提时不适用的行为被重绑为通配默认
    // (如 open_path), 切回后通配行为仍覆盖原前提 -> 用户为原前提配的行为丢失
    // (实测: 图片组配专属行为 -> 文档组变 open_path -> 切回图片仍是 open_path)。
    // 方案: 离散切换前把当前 Entries 深拷贝暂存 (键 = 切换前 MatchValue), 切到新
    // 前提时有快照则整体还原, 无则走重绑规则。快照仅会话内存态: 持久化始终为
    // 当前前提的 Entries, 保存后重启其他前提的记忆不保留, 由重绑规则兜底。
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
    /// 前提切换 (textType 换特征 / fileExt 换分组 Toggle 或「无」) 联动: 不适用新前提的 entry
    /// 自动换为该前提默认行为 (模板重置与手动换行为同语义; 无默认前提保留脏值),
    /// 展开 = 所见即当前前提生效的行为。直接改底层 Entries: 收起态 Editors 为空,
    /// 展开态重建编辑行。仅供离散切换路径调用 —— fileExt 逐字符输入不走此链路
    /// (打字中途重置是破坏性的; 手改后缀保持既有行为与关联语义)。
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
    /// 约束: 行为数达 9 时禁用; 覆盖集中的可用行为已全部占用时禁用 —— 再加必为重复行为
    /// (2026-09-08 应用户要求, 首版只挡单行为类型存在漏洞: 覆盖集多项时仍可手选已占用
    /// 行为重复添加, 泛化为查覆盖集剩余未占用项); 空行仍允许加第一个。
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
        // 默认取覆盖集中第一个未占用行为 (全占用已被 CanAddEntry 拦截;
        // 覆盖集为空的脏值行回退 open, 保证空行可加第一个)
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
