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
        // 四个 Toggle 的勾选态由 MatchValue 派生, 必须在此同步:
        // 只靠 Checked 事件回调时机不可靠 (事件先于绑定推值触发时, 旧项残留点亮,
        // 实测"最多同时亮两个") —— 数据变了必须自己发全通知。
        NotifyTogglesChanged();
        RebindEditorsToPremise();
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
        NotifyTogglesChanged(); // 与 SetTextType 同理: Toggle 视觉态随 MatchValue 同步
        RebindEditorsToPremise();
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

    /// <summary>
    /// 特征切换 (textType 换 url/path/magnet/plain) 联动: 不适用新前提的 entry
    /// 自动换为该前提默认行为 (模板重置与手动换行为同语义; 无默认前提保留脏值),
    /// 展开 = 所见即当前类型生效的行为。直接改底层 Entries: 收起态 Editors 为空,
    /// 展开态重建编辑行。仅供特征切换路径调用 —— fileExt 逐字符输入与分组填入
    /// 不走此链路 (避免打字中途破坏性重置; 分组另有"不兼容行为保持不动"约定)。
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
        OnPropertyChanged(nameof(LanguageTick));
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
