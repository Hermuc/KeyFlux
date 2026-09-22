using System.Collections.ObjectModel;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 一个匹配类型桶的 UI 编辑器 (选中动作聚合卡内, 卡内唯一点亮的类型即为本编辑器对应的类型):
/// 直接持有底层 <see cref="SelectedMapping"/> 对象引用, 行为增删/排序/属性修改天然同步。
///
/// 本版去掉了旧的「前提 (premise) 切换快照/重绑」机制 —— 类型不再在行内互斥切换
/// (那是旧两分区行卡的职责), 类型选择已上移到聚合卡 toggle; 切类型 = 切查看对象,
/// 不会重绑当前映射的 entries。
/// </summary>
public sealed partial class MappingRowVm : ObservableObject
{
    private readonly SelectedActionPageViewModel _page;

    /// <summary>当前查看的类型标识 (卡级 toggle id): group:&lt;name&gt; / type:&lt;id&gt; / orphan:&lt;n&gt;。
    /// 用于解析「条件值编辑器」的来源实体 (分组 / 自定义类型), null 表示未知 (纯 orphan 兼容)。</summary>
    private readonly string? _typeId;

    public MappingRowVm(SelectedActionPageViewModel page, SelectedMapping mapping, string? typeId = null)
    {
        _page = page;
        Mapping = mapping;
        _typeId = typeId;
        RefreshChips();
    }

    public SelectedMapping Mapping { get; }

    /// <summary>是否 transient (未配置类型的「待配置」态): 尚未落盘, 用户添加首个行为后才提交。</summary>
    public bool IsTransient { get; set; }

    /// <summary>entries 集合发生变化 (增/删) 时通知卡级 (用于 transient→real 提交与不变量校验)。</summary>
    public event Action<MappingRowVm>? EntriesChanged;

    // ---- 类型 ----

    public string MatchType => Mapping.MatchType;
    public bool IsTextType => MatchType == "textType";

    /// <summary>类型徽章文本 (文本特征/文件后缀)。</summary>
    public string TypeBadgeText => ActionSchemeCatalog.MatchTypeLabel(MatchType);

    /// <summary>类型徽章色: 文本蓝 / 后缀橙。</summary>
    public string TypeBadgeColorHex => IsTextType ? BehaviorBadgeColors.LinkDarkWarm : BehaviorBadgeColors.ExtTerracotta;

    // ---- 条件值 (只读展示; 条件由卡级 lit toggle / 分组决定, 本编辑器不编辑) ----

    /// <summary>行摘要 (删除确认等场景)。</summary>
    public string MatchSummary
        => $"{TypeBadgeText}: {(Mapping.MatchValue.Trim().Length == 0 ? I18n.T("999") : Mapping.MatchValue)}";

    /// <summary>语言切换刻度, 行内绑定读此重译。</summary>
    public int LanguageTick => _page.LanguageTick;

    /// <summary>条件值展示 (只读语义: 文本特征=特征值; 文件后缀=后缀集/引用); 旧分组写回逻辑已随卡级重构移除。</summary>
    public string MatchValueDisplay
    {
        get => Mapping.MatchValue;
        set
        {
            if (Mapping.MatchValue == value) return;
            Mapping.MatchValue = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(MatchSummary));
        }
    }

    // ---- 文件后缀: 条件值 (后缀集) 可编辑框 (恢复被 c7b80dd 删除的能力) ----

    /// <summary>后缀编辑器可见性: 仅文件后缀类型 (非文本特征)。文本卡不显示该输入框。</summary>
    public bool ShowExtEditor => !IsTextType;

    /// <summary>
    /// 当前类型对应的后缀列表 (供详情区 TextBox 双向绑定):
    ///   - group:&lt;name&gt; → 该 <see cref="FileGroup.Exts"/> 的逗号串 (来源实体为真源);
    ///   - type:&lt;id&gt;(fileExt) → 该 <see cref="MatchType.Exts"/> 的逗号串;
    ///   - orphan:* / 无 id → 直接 <see cref="SelectedMapping.MatchValue"/>。
    /// 设值时规整为后缀列表后**双写**: 写回来源实体 + 同步本条 mapping 的 MatchValue
    /// (group/orphan 写规整串; type:&lt;id&gt; 保持引用串不变), 保证 FindMapping 的 SameExts 认领不破。
    /// </summary>
    public string ExtsDisplay
    {
        get
        {
            var g = TryResolveFileGroup();
            if (g is not null) return string.Join(", ", g.Exts);
            var t = TryResolveFileExtType();
            if (t is not null) return string.Join(", ", t.Exts);
            return Mapping.MatchValue;
        }
        set
        {
            var normalized = ActionSchemeCatalog.NormalizeExts(value);
            var joined = string.Join(",", normalized);

            var g = TryResolveFileGroup();
            if (g is not null)
            {
                g.Exts = normalized;            // a) 写回来源实体
                Mapping.MatchValue = joined;    // b) 同步 mapping (FindMapping 认领依赖)
                OnPropertyChanged();
                OnPropertyChanged(nameof(MatchSummary));
                _page.OnCardMappingChanged();   // c) 卡片状态刷新
                return;
            }

            var t = TryResolveFileExtType();
            if (t is not null)
            {
                t.Exts = normalized;            // a) 写回来源实体
                // b) type:<id> 保持 MatchValue 引用串不变 (认领按字面 "type:<id>")
                OnPropertyChanged();
                OnPropertyChanged(nameof(MatchSummary));
                _page.OnCardMappingChanged();   // c) 卡片状态刷新
                return;
            }

            Mapping.MatchValue = joined;        // orphan/无 id: 直接写 mapping
            OnPropertyChanged();
            OnPropertyChanged(nameof(MatchSummary));
            _page.OnCardMappingChanged();       // c) 卡片状态刷新
        }
    }

    private FileGroup? TryResolveFileGroup()
    {
        if (_typeId is null || !_typeId.StartsWith("group:")) return null;
        var name = _typeId["group:".Length..];
        return _page.Config.FileGroups.FirstOrDefault(x => x.Name == name);
    }

    private Models.MatchType? TryResolveFileExtType()
    {
        if (_typeId is null || !_typeId.StartsWith("type:")) return null;
        var id = _typeId["type:".Length..];
        return _page.Config.MatchTypes.FirstOrDefault(x => x.Id == id);
    }

    // ---- chips 键位表 ----

    /// <summary>是否为所在分区的首行 (已随卡级重构弃用, 保留兼容旧测试引用)。</summary>
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

    // ---- 编辑器 (卡级展示当前查看类型的详情, 始终展开) ----

    [ObservableProperty]
    private bool _isMatched;

    public ObservableCollection<EntryRowVm> Editors { get; } = [];

    /// <summary>展开: 重建编辑行 (收起态由卡级统一仲裁)。</summary>
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

    /// <summary>条件/特征变化后刷新已打开编辑器的行为下拉 (覆盖集随前提变化)。</summary>
    private void RefreshEditorOptions()
    {
        foreach (var editor in Editors)
        {
            editor.RefreshOptions();
        }
        OnPropertyChanged(nameof(CanAddEntry));
        OnPropertyChanged(nameof(AddEntryHint));
    }

    // ---- 行为增删 / 排序 (编辑器内) ----

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
        EntriesChanged?.Invoke(this);
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
        if (Mapping.Entries.Count <= 1) return; // 约束: 至少保留一个行为 (不变量: 禁止空 entries)
        var index = Editors.IndexOf(editor);
        if (index < 0) return;
        Mapping.Entries.RemoveAt(index);
        Editors.RemoveAt(index);
        RefreshChips();
        PushPositions();
        EntriesChanged?.Invoke(this);
    }

    // ---- 行级操作 ----

    /// <summary>行内 ▶ 测试: 预填底部模拟条并立即执行。</summary>
    [RelayCommand]
    private void TestRow() => _page.RunTestFor(this);

    /// <summary>语言切换: 下拉副本/摘要即时拼接刷新。</summary>
    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(LanguageTick));
        OnPropertyChanged(nameof(TypeBadgeText));
        OnPropertyChanged(nameof(MatchSummary));
        OnPropertyChanged(nameof(ShowExtEditor));
        OnPropertyChanged(nameof(ExtsDisplay));
        RefreshChips();
        foreach (var editor in Editors) editor.RefreshLanguage();
    }
}
