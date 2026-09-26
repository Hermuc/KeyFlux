using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;
/// <summary>
/// 手风琴内单个行为的编辑行: 行为下拉、命令模板、工作目录、排序/删除。
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

    /// <summary>语言切换刻度, 行内绑定读此重译。</summary>
    public int LanguageTick => _row.LanguageTick;

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

    // ---- Options 三开关 (UI 本版不呈现, 属性保留兼容存量数据) ----

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
    }

    /// <summary>语言切换: 刷新行为下拉副本与即时拼接文案。</summary>
    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(LanguageTick));
        RefreshOptions();
        OnPropertyChanged(nameof(TemplateHint));
        OnPropertyChanged(nameof(ActionValue));
        OnPropertyChanged(nameof(WorkingDir));
    }
}
