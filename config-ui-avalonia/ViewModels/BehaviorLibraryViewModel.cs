using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
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

    /// <summary>
    /// 草稿行 (2026-09-15): 新建期间追加在列表末尾的占位项, 代表"正在创建、尚未填写"的行为。
    /// 仅存在于 UI 集合 (不落盘), 保存/取消后即移除; 因 Pack.Source 为空, IsUser 恒 false ⇒ 不可编辑/删除。
    /// </summary>
    public bool IsDraft { get; init; }

    /// <summary>内置包只读; 仅用户包可编辑/删除 (草稿行不算)。</summary>
    public bool IsUser => Pack.Source == "user" && !IsDraft;
}

/// <summary>
/// 行为库窗口 VM (CONTRACTS §3.9): 展示内置+用户行为包, 提供删除与「立即生效」;
/// 新建/编辑在本窗右侧详情区**内联展开表单** (Editor, 2026-09-15 起不再弹 BehaviorEditWindow)。
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

    // ------------------------------------------------------------- 内联新建/编辑表单
    // 2026-09-15: 表单不再以独立弹窗 (BehaviorEditWindow) 弹出 —— 叠加在主窗口之上会形成
    // 「主窗口 → 行为库 → 表单」三层弹窗; 现改为在本窗右侧详情区就地展开 (详情 ↔ 表单切换),
    // 层级与「匹配类型」弹窗保持一致。

    /// <summary>草稿行引用 (仅 UI 集合, 不落盘)。</summary>
    private BehaviorRowVm? _draftRow;

    /// <summary>进入新建前的选中项 (取消时恢复)。</summary>
    private BehaviorRowVm? _selectionBeforeCreate;

    /// <summary>选择钉住期间的重入保护。</summary>
    private bool _isPinningSelection;

    /// <summary>右侧详情区正在编辑的表单 (null = 显示选中项详情)。</summary>
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsEditorOpen))]
    [NotifyPropertyChangedFor(nameof(CanUseActions))]
    [NotifyPropertyChangedFor(nameof(CanEditSelected))]
    [NotifyPropertyChangedFor(nameof(CanDeleteSelected))]
    [NotifyPropertyChangedFor(nameof(CanApply))]
    private BehaviorEditViewModel? _editor;

    public bool IsEditorOpen => Editor is not null;

    /// <summary>表单打开时禁用列表操作按钮, 避免与表单自身的保存/取消语义打架。</summary>
    public bool CanUseActions => Editor is null;

    /// <summary>仅自定义包可编辑 (且未在编辑态)。</summary>
    public bool CanEditSelected => CanUseActions && SelectedRow?.IsUser == true;

    /// <summary>仅自定义包可删除 (且未在编辑态)。</summary>
    public bool CanDeleteSelected => CanUseActions && SelectedRow?.IsUser == true;

    /// <summary>「立即生效」需有未生效变更, 且不在编辑态。</summary>
    public bool CanApply => CanUseActions && IsDirty;

    partial void OnIsDirtyChanged(bool value) => OnPropertyChanged(nameof(CanApply));

    /// <summary>新建: 在列表最末追加草稿行 (正在创建、尚未填写) 并就地展开空白表单 (不弹窗)。</summary>
    public void OpenCreate()
    {
        _selectionBeforeCreate = SelectedRow;   // 取消时恢复
        _draftRow = CreateDraftRow();
        Rows.Add(_draftRow);
        SelectedRow = _draftRow;
        Editor = new BehaviorEditViewModel(_main, null);
    }

    /// <summary>草稿行: 标题「新建行为」/ 状态「新建中」/ 副标题「尚未填写」; 文字弱化见视图 IsDraft 样式。</summary>
    private static BehaviorRowVm CreateDraftRow() =>
        new(new BehaviorPack { Name = I18n.T("1085") }, I18n.T("2578"), I18n.T("2579")) { IsDraft = true };

    /// <summary>编辑: 打开既有包的填充表单 (不弹窗)。</summary>
    public void OpenEdit(BehaviorPack pack) => Editor = new BehaviorEditViewModel(_main, pack);

    /// <summary>
    /// 关闭表单: 移除草稿行, 解除表单 VM 的语言订阅 (它在构造期订阅了 I18n.Changed, 必须显式退订);
    /// 若当前选中行已不在列表 (取消路径) 则恢复进入新建前的选中项。
    /// </summary>
    public void CloseEditor()
    {
        var editor = Editor;
        Editor = null;
        editor?.UnsubscribeLanguage();
        foreach (var draft in Rows.Where(r => r.IsDraft).ToList()) Rows.Remove(draft);
        _draftRow = null;
        if (SelectedRow is null || !Rows.Contains(SelectedRow)) SelectedRow = _selectionBeforeCreate;
    }

    /// <summary>表单「保存」: 后端校验 → 成功则刷新目录并选中新/改的包, 失败回显在表单状态栏。</summary>
    [RelayCommand]
    private async Task SaveEditorAsync()
    {
        if (Editor is not { } editor) return;
        try
        {
            var error = await editor.SaveAsync();
            if (error is not null)
            {
                editor.StatusText = error;
                return;
            }
            IsDirty = true;
            var savedId = editor.SavedId;
            _draftRow = null;          // 草稿使命结束: 先解除"选择钉住", 否则新选中会被弹回草稿
            CloseEditor();
            await ReloadAsync(savedId);
        }
        catch (Exception ex)
        {
            editor.StatusText = ex.Message;
        }
    }

    /// <summary>表单「取消」。</summary>
    [RelayCommand]
    private void CancelEditor() => CloseEditor();

    /// <summary>选中内置行时给出只读解释, 避免「按钮灰=坏了」的困惑。</summary>
    partial void OnSelectedRowChanged(BehaviorRowVm? value)
    {
        // 新建期间把选择钉在草稿行上: 避免"选中行 ≠ 正在编辑的对象"的错位
        if (!_isPinningSelection && _draftRow is not null && !ReferenceEquals(value, _draftRow))
        {
            _isPinningSelection = true;
            SelectedRow = _draftRow;
            _isPinningSelection = false;
            return;
        }
        if (value is { IsUser: false, IsDraft: false })
        {
            StatusText = I18n.T("1103_only");
        }
        // 内联表单门控 (2026-09-15): 选中项变化 ⇒ 编辑/删除按钮可用性随之变化
        OnPropertyChanged(nameof(CanEditSelected));
        OnPropertyChanged(nameof(CanDeleteSelected));
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
        if (_draftRow is not null)
        {
            // 新建期间列表被重建 (语言切换/保存回调): 重新追加草稿行并保持选中
            _draftRow = CreateDraftRow();
            Rows.Add(_draftRow);
            SelectedRow = _draftRow;
        }
        else
        {
            SelectedRow = Rows.FirstOrDefault(r => r.Id == selectedId) ?? Rows.FirstOrDefault();
        }
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
