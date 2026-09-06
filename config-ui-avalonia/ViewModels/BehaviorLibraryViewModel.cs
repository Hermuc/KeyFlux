using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>行为库列表行 VM。</summary>
public sealed partial class BehaviorRowVm : ObservableObject
{
    public BehaviorRowVm(BehaviorPack pack, string sourceLabel, string premiseSummary)
    {
        Pack = pack;
        _name = pack.Name;
        _id = pack.Id;
        _sourceLabel = sourceLabel;
        _premiseSummary = premiseSummary;
        _description = pack.Description ?? "";
    }

    public BehaviorPack Pack { get; }

    [ObservableProperty] private string _name;
    [ObservableProperty] private string _id;
    [ObservableProperty] private string _sourceLabel;
    [ObservableProperty] private string _premiseSummary;
    [ObservableProperty] private string _description;

    /// <summary>内置包只读; 仅用户包可编辑/删除。</summary>
    public bool IsUser => Pack.Source == "user";
}

/// <summary>
/// 行为库窗口 VM (CONTRACTS §3.9): 展示内置+用户行为包, 提供删除与「立即生效」;
/// 新建/编辑由窗口代码后置打开 BehaviorEditWindow (需 ShowDialog owner)。
/// 变更不自动重启引擎 —— IsDirty 时由用户显式点「立即生效」, 避免连续增删的重启风暴。
/// </summary>
public sealed partial class BehaviorLibraryViewModel : ObservableObject
{
    private readonly MainViewModel _main;

    public BehaviorLibraryViewModel(MainViewModel main)
    {
        _main = main;
        I18n.Changed += OnLanguageChanged;
    }

    /// <summary>供窗口代码后置打开编辑表单 (需要 Main 会话构造表单 VM)。</summary>
    public MainViewModel Main => _main;

    private ISettingsApi Api => _main.Session.Api
        ?? throw new InvalidOperationException("后端未就绪");

    public ObservableCollection<BehaviorRowVm> Rows { get; } = [];

    [ObservableProperty] private BehaviorRowVm? _selectedRow;
    [ObservableProperty] private bool _isDirty;
    [ObservableProperty] private string? _statusText;
    [ObservableProperty] private int _languageTick;

    /// <summary>选中内置行时给出只读解释, 避免「按钮灰=坏了」的困惑。</summary>
    partial void OnSelectedRowChanged(BehaviorRowVm? value)
    {
        if (value is { IsUser: false })
        {
            StatusText = I18n.T("1103_only");
        }
    }

    private void OnLanguageChanged() => ++LanguageTick;

    public void UnsubscribeLanguage() => I18n.Changed -= OnLanguageChanged;

    /// <summary>从后端拉取目录快照并重建列表; selectId 优先 (新建/编辑后定位到该行)。</summary>
    public async Task ReloadAsync(string? selectId = null)
    {
        await BehaviorCatalog.LoadAsync(Api);
        var selectedId = selectId ?? SelectedRow?.Id;
        Rows.Clear();
        foreach (var p in BehaviorCatalog.Packs)
        {
            var sourceLabel = p.Source == "user" ? I18n.T("1093") : I18n.T("1092");
            Rows.Add(new BehaviorRowVm(p, sourceLabel, BuildPremiseSummary(p)));
        }
        SelectedRow = Rows.FirstOrDefault(r => r.Id == selectedId) ?? Rows.FirstOrDefault();
        ++LanguageTick;
    }

    /// <summary>前提摘要: 「文本特征 url ｜ 后缀 jpg、png」。</summary>
    internal static string BuildPremiseSummary(BehaviorPack p)
    {
        var parts = new List<string>();
        foreach (var e in p.AppliesTo)
        {
            if (string.Equals(e.Type, "textType", StringComparison.OrdinalIgnoreCase))
            {
                parts.Add($"{I18n.T("1104")} {ActionSchemeCatalog.TextTypeLabel(e.Value ?? "")}");
            }
            else if (e.Exts?.Contains("*") == true)
            {
                parts.Add(I18n.T("1104_any"));
            }
            else
            {
                parts.Add($"{I18n.T("1103")} {string.Join("、", e.Exts ?? [])}");
            }
        }
        return string.Join("　|　", parts);
    }

    /// <summary>删除选中用户行为 (内置包由后端拒绝, 前端按钮已禁用); 返回错误提示或 null。</summary>
    public async Task<string?> DeleteSelectedAsync()
    {
        if (SelectedRow is not { } row) return null;
        if (!row.IsUser) return I18n.T("1103_only");
        var resp = await Api.DeleteBehaviorAsync(row.Id);
        if (!resp.Success) return resp.ErrorMessage;
        IsDirty = true;
        StatusText = string.Format(I18n.T("1102_deleted"), row.Name);
        await ReloadAsync();
        return null;
    }

    /// <summary>显式重启引擎使变更生效; 返回错误提示或 null (restartFailed 折叠为提示)。</summary>
    public async Task<string?> ApplyAsync()
    {
        var resp = await Api.ApplyBehaviorsAsync();
        if (!resp.Success) return resp.ErrorMessage;
        IsDirty = false;
        StatusText = I18n.T("1101_applied");
        return null;
    }
}
