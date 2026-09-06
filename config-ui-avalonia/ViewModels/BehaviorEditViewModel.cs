using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>前提编辑行: 类型 (文件后缀/文本特征) + 值 (逗号分隔后缀或特征枚举值)。</summary>
public sealed partial class BehaviorPremiseRowVm : ObservableObject
{
    [ObservableProperty] private ComboOption _selectedType = new("fileExt", "");
    [ObservableProperty] private string _value = "";
}

/// <summary>
/// 行为新建/编辑表单 VM (一期仅 builtin entry: 基础动作 + 默认模板 + 生效前提)。
/// 合法性真源在后端 (ID 规范/保留字冲突/manifest 结构), 前端错误回显于表单底部状态栏。
/// </summary>
public sealed partial class BehaviorEditViewModel : ObservableObject
{
    private readonly MainViewModel _main;
    private readonly string? _originalId;

    public BehaviorEditViewModel(MainViewModel main, BehaviorPack? existing)
    {
        _main = main;
        _originalId = existing?.Id;
        IsNew = existing is null;
        Title = IsNew ? I18n.T("1085") : I18n.T("1086");
        Name = existing?.Name ?? "";
        Id = existing?.Id ?? "";
        Description = existing?.Description ?? "";
        ActionValue = existing?.Entry.Params?.ActionValue ?? "";
        WorkingDir = existing?.Entry.Params?.WorkingDir ?? "";
        foreach (var e in existing?.AppliesTo ?? [])
        {
            Premises.Add(new BehaviorPremiseRowVm
            {
                SelectedType = new ComboOption(e.Type, ""),
                Value = string.Equals(e.Type, "textType", StringComparison.OrdinalIgnoreCase)
                    ? e.Value ?? ""
                    : string.Join(",", e.Exts ?? []),
            });
        }
        if (Premises.Count == 0) Premises.Add(new BehaviorPremiseRowVm());
        // 目录未加载 (异常时序) 时兜底, 避免空序列 First() 崩溃
        SelectedBaseAction = BaseActionOptions.FirstOrDefault(o => o.Value == existing?.Entry.Action)
                             ?? BaseActionOptions.FirstOrDefault()
                             ?? new ComboOption("run", "");
        I18n.Changed += OnLanguageChanged;
    }

    public bool IsNew { get; }

    /// <summary>保存成功后的包 ID (行为库窗口据此选中新行); 取消/失败为 null。</summary>
    public string? SavedId { get; private set; }

    public ObservableCollection<BehaviorPremiseRowVm> Premises { get; } = [];

    /// <summary>基础动作候选 = 内置包 (entry.action 的合法取值即内置动作 ID)。</summary>
    public ObservableCollection<ComboOption> BaseActionOptions { get; } =
        new(BehaviorCatalog.Packs.Where(p => p.Source == "builtin")
            .Select(p => new ComboOption(p.Id, BehaviorCatalog.LabelFor(p.Id))));

    public ObservableCollection<ComboOption> TypeOptions { get; } =
        [new ComboOption("fileExt", I18n.T("1103")), new ComboOption("textType", I18n.T("1104"))];

    [ObservableProperty] private string _title;
    [ObservableProperty] private string _name;
    [ObservableProperty] private string _id;
    [ObservableProperty] private string _description;
    [ObservableProperty] private string _actionValue;
    [ObservableProperty] private string _workingDir;
    [ObservableProperty] private ComboOption _selectedBaseAction;
    [ObservableProperty] private string? _statusText;
    [ObservableProperty] private int _languageTick;

    private void OnLanguageChanged() => ++LanguageTick;

    public void UnsubscribeLanguage() => I18n.Changed -= OnLanguageChanged;

    [RelayCommand]
    private void AddPremise() => Premises.Add(new BehaviorPremiseRowVm());

    [RelayCommand]
    private void RemovePremise(BehaviorPremiseRowVm? row)
    {
        if (row is not null) Premises.Remove(row);
    }

    /// <summary>保存 (创建或更新); 返回错误提示, null = 成功 (窗口随后关闭)。</summary>
    public async Task<string?> SaveAsync()
    {
        var pack = BuildPack();
        var resp = IsNew
            ? await Api.CreateBehaviorAsync(pack)
            : await Api.UpdateBehaviorAsync(_originalId!, pack);
        if (!resp.Success) return resp.ErrorMessage;
        SavedId = pack.Id;
        return null;
    }

    private BehaviorPack BuildPack()
    {
        var appliesTo = new List<BehaviorAppliesTo>();
        foreach (var row in Premises)
        {
            var value = row.Value.Trim();
            if (value.Length == 0) continue;
            if (string.Equals(row.SelectedType.Value, "textType", StringComparison.OrdinalIgnoreCase))
            {
                appliesTo.Add(new BehaviorAppliesTo { Type = "textType", Value = value.ToLowerInvariant() });
            }
            else
            {
                appliesTo.Add(new BehaviorAppliesTo { Type = "fileExt", Exts = ActionSchemeCatalog.NormalizeExts(value) });
            }
        }
        return new BehaviorPack
        {
            Id = Id.Trim().ToLowerInvariant(),
            Name = Name.Trim(),
            Description = string.IsNullOrWhiteSpace(Description) ? null : Description.Trim(),
            SpecVersion = 1,
            Version = "1.0.0",
            AppliesTo = appliesTo,
            Entry = new BehaviorEntry
            {
                Kind = "builtin",
                Action = SelectedBaseAction.Value,
                Params = new BehaviorEntryParams { ActionValue = ActionValue, WorkingDir = WorkingDir },
            },
            Source = "user",
        };
    }

    private ISettingsApi Api => _main.Session.Api
        ?? throw new InvalidOperationException("后端未就绪");
}
