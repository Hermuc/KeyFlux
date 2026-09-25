using System.Collections.ObjectModel;
using System.Text.RegularExpressions;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
// System.IO.MatchType (SDK 隐式 using) 与本项目的 Models.MatchType 同名, 显式限定避免 CS0104
using MatchType = KeyFlux.Settings.Models.MatchType;

namespace KeyFlux.Settings.ViewModels;

// ============================================================================
// 匹配类型弹窗 VM —— 方案 C7 的用户创建入口。
//
// 面板按"用户要做的两个决策"分段, 术语全部面向非专业用户:
//   ① 名字和识别方式  —— 叫什么 + 什么时候算命中 (文本内容 / 某类文件)
//   ② 命中后做什么    —— 做什么动作; 底部两个按钮承载两条创建路径:
//        保存，并设置命中后做什么 (同建专属行为并强绑定)  /  先只建类型 (留桩)
//   高级选项 (默认收起) —— 内部代号 / 英文名 / 动作名称 / 命令模板 / 工作目录
//
// 设计要点:
//   - **内部代号对用户隐藏**: 由名称自动生成, 冲突自动加数字后缀; 用户只在高级选项里才看得到。
//   - **「试一下」**: 复用后端 POST /api/selected-action/test, 不在前端重写算子语义
//     (否则会造出与 Go/AHK 分歧的第三份实现)。
//   - 留桩类型在「添加映射」弹窗里仍有继承行为可用 (plain 两段式覆盖) ⇒ 不会出现空列表死胡同。
// ============================================================================

/// <summary>算子下拉项 (值与 Go validMatchOps / AHK MatchCustomRules 逐字一致; 标签为通俗说法)。</summary>
public sealed record MatchOpOption(string Value, string LabelKey)
{
    public string Label => I18n.T(LabelKey);
    public override string ToString() => Label;
}

/// <summary>内容条件行: 算子 + 要匹配的文字。</summary>
public sealed partial class MatchRuleRowVm : ObservableObject
{
    public static readonly IReadOnlyList<MatchOpOption> Ops =
    [
        new("contains", "2515"), // 包含这段文字
        new("equals", "2512"),   // 完全一样
        new("prefix", "2513"),   // 开头是
        new("suffix", "2514"),   // 结尾是
    ];

    public MatchRuleRowVm(string op = "contains", string value = "")
    {
        _op = Ops.FirstOrDefault(o => o.Value == op) ?? Ops[0];
        _value = value;
    }

    [ObservableProperty] private MatchOpOption _op;
    [ObservableProperty] private string _value = "";

    /// <summary>算子候选 (XAML 绑定用实例属性; 静态字段无法直接被 DataTemplate 绑定)。</summary>
    public IReadOnlyList<MatchOpOption> OpOptions => Ops;
}

/// <summary>类型列表行: 内置只读行或自定义类型行。</summary>
public sealed partial class MatchTypeRowVm : ObservableObject
{
    public required string Id { get; init; }
    public required string Kind { get; init; }   // text | fileExt
    public bool IsBuiltin { get; init; }
    public MatchType? Model { get; init; }        // 自定义类型才有

    /// <summary>
    /// 草稿行 (2026-09-15): 新建期间追加在列表末尾的占位项, 代表"正在创建、尚未填写"的类型。
    /// 仅存在于 UI 集合中 (不写入 Config.MatchTypes), 保存/取消后即移除。
    /// </summary>
    public bool IsDraft { get; init; }

    [ObservableProperty] private string _label = "";
    [ObservableProperty] private string _summary = "";
    [ObservableProperty] private int _dedicatedCount;

    /// <summary>还没设置动作 (自定义类型且专属行为数为 0); 草稿行不参与该判定。</summary>
    public bool NeedsBehavior => !IsBuiltin && !IsDraft && DedicatedCount == 0;

    /// <summary>状态文案: 新建中(草稿) / 内置 / 已设置动作 N / 还没设置动作。</summary>
    public string StatusText => IsDraft
        ? I18n.T("2578")
        : IsBuiltin
            ? I18n.T("2539")
            : NeedsBehavior
                ? I18n.T("2537")
                : I18n.T("2538") + " " + DedicatedCount;

    /// <summary>自定义行才可编辑/删除 (草稿行不算)。</summary>
    public bool IsCustom => !IsBuiltin && !IsDraft;

    /// <summary>「现在设置」按钮可见性 (留桩行专有)。</summary>
    public bool ShowSetAction => NeedsBehavior;

    public void NotifyDerived()
    {
        OnPropertyChanged(nameof(NeedsBehavior));
        OnPropertyChanged(nameof(StatusText));
        OnPropertyChanged(nameof(IsCustom));
        OnPropertyChanged(nameof(ShowSetAction));
    }
}

/// <summary>
/// 新建 / 编辑面板。内部代号创建后不可改 (改名恒安全: 引用走 id), 编辑态锁定。
/// </summary>
public sealed partial class MatchTypeEditorVm : ObservableObject
{
    private readonly MatchTypesDialogViewModel _page;

    /// <summary>自动设置代号期间抑制"用户已手改"标记。</summary>
    private bool _autoSettingId;

    /// <summary>用户是否在高级选项里手改过代号 (手改后不再跟随名称自动生成)。</summary>
    private bool _idTouched;

    public MatchTypeEditorVm(MatchTypesDialogViewModel page, MatchType? existing)
    {
        _page = page;
        IsEdit = existing is not null;
        _id = existing?.Id ?? "";
        _label = existing?.Label ?? "";
        _labelEn = existing?.LabelEn ?? "";
        _isFileExt = existing?.Kind == "fileExt";
        if (_isFileExt)
        {
            _extsText = string.Join(", ", existing?.Exts ?? []);
        }
        else
        {
            foreach (var r in existing?.Rules ?? []) Rules.Add(new MatchRuleRowVm(r.Op, r.Value));
            if (Rules.Count == 0) Rules.Add(new MatchRuleRowVm());
        }
        _behaviorName = existing?.Label ?? "";
        _actionValue = "";
        _workingDir = "";
        EnsureBaseActions();
        _selectedBaseAction = PickDefaultAction();
    }

    /// <summary>编辑既有类型 (代号锁定) 而非新建。</summary>
    public bool IsEdit { get; }

    /// <summary>内容条件行 (文本类)。</summary>
    public ObservableCollection<MatchRuleRowVm> Rules { get; } = [];

    /// <summary>「命中后做什么」候选 = 内置行为包的通俗中文名 (来自包 name)。</summary>
    public ObservableCollection<ComboOption> BaseActionOptions { get; } = [];

    [ObservableProperty] private string _id = "";
    [ObservableProperty] private string _label = "";
    [ObservableProperty] private string _labelEn = "";
    [ObservableProperty] private string _extsText = "";
    [ObservableProperty] private bool _isFileExt;

    [ObservableProperty] private string _behaviorName = "";
    [ObservableProperty] private ComboOption _selectedBaseAction;
    [ObservableProperty] private string _actionValue = "";
    [ObservableProperty] private string _workingDir = "";

    /// <summary>高级选项折叠态 (默认收起)。</summary>
    [ObservableProperty] private bool _showAdvanced;

    [ObservableProperty] private string? _error;
    [ObservableProperty] private bool _saving;
    [ObservableProperty] private int _languageTick;

    // ---- 「试一下」 ----

    [ObservableProperty] private string _tryContent = "";
    [ObservableProperty] private string _tryResult = "";
    [ObservableProperty] private bool _tryOk;
    [ObservableProperty] private bool _trying;
    partial void OnTryResultChanged(string value) => OnPropertyChanged(nameof(HasTryResult));

    /// <summary>试一下结果可见性。</summary>
    public bool HasTryResult => TryResult.Length > 0;

    /// <summary>面板标题 (新建 / 编辑)。</summary>
    public string Title => IsEdit ? I18n.T("2546") : I18n.T("2522");

    partial void OnIsFileExtChanged(bool value)
    {
        if (!value && Rules.Count == 0) Rules.Add(new MatchRuleRowVm());
        // 类别切换: 默认动作跟着变 (文本→搜索类; 文件→打开类), 不再固定落"复制"
        var preferred = PickDefaultAction();
        if (BaseActionOptions.Any(o => o.Value == preferred.Value)) SelectedBaseAction = preferred;
        OnPropertyChanged(nameof(IsTextKind));
        OnPropertyChanged(nameof(IsFileKind));
    }

    /// <summary>界面用「选中的是某类文件」的反向 (单选组第二项选中态)。</summary>
    public bool IsFileKind
    {
        get => IsFileExt;
        set
        {
            if (IsFileExt == value) return;
            IsFileExt = value;
            OnPropertyChanged();
        }
    }

    /// <summary>单选组第一项选中态 (不选文件即为选文字)。</summary>
    public bool IsTextKind
    {
        get => !IsFileExt;
        set
        {
            if (IsFileExt == !value) return;
            IsFileExt = !value;
            OnPropertyChanged();
        }
    }

    partial void OnLabelChanged(string value)
    {
        if (string.IsNullOrWhiteSpace(BehaviorName)) BehaviorName = value;
        if (IsEdit || _idTouched) return;
        _autoSettingId = true;
        Id = _page.UniqueId(IdHint(value));
        _autoSettingId = false;
    }

    partial void OnIdChanged(string value)
    {
        if (!_autoSettingId) _idTouched = true;
    }

    /// <summary>名称 → 代号基底 (ASCII 化; 全中文等取不到字符时返回空串, 由 UniqueId 兜底)。</summary>
    private static string IdHint(string label)
    {
        var buf = new System.Text.StringBuilder();
        foreach (var ch in label.Trim().ToLowerInvariant())
        {
            if (ch is >= 'a' and <= 'z' or >= '0' and <= '9' or '_') buf.Append(ch);
        }
        var s = buf.ToString().Trim('_');
        if (s.Length > 0 && char.IsAsciiDigit(s[0])) s = "t" + s;
        return s.Length > 20 ? s[..20] : s;
    }

    /// <summary>按当前识别方式挑默认动作 (目录未加载时给安全兜底 id)。</summary>
    private ComboOption PickDefaultAction()
    {
        var fallbackId = IsFileExt ? "open_path" : "search";
        var id = BehaviorCatalog.DefaultFor(IsFileExt ? "fileExt" : "textType", IsFileExt ? "*" : "plain")
                 ?? fallbackId;
        return BaseActionOptions.FirstOrDefault(o => o.Value == id)
               ?? BaseActionOptions.FirstOrDefault(o => o.Value == fallbackId)
               ?? new ComboOption(fallbackId, "");
    }

    /// <summary>行为目录到达后填充"做什么"下拉 (目录未加载时为空, 故页面在打开面板前先加载)。</summary>
    public void EnsureBaseActions()
    {
        if (BaseActionOptions.Count > 0) return;
        foreach (var p in BehaviorCatalog.Packs.Where(p => p.Source == "builtin"))
        {
            BaseActionOptions.Add(new ComboOption(p.Id, BehaviorCatalog.LabelFor(p.Id)));
        }
    }

    [RelayCommand]
    private void AddRule() => Rules.Add(new MatchRuleRowVm());

    [RelayCommand]
    private void RemoveRule(MatchRuleRowVm? row)
    {
        if (row is not null && Rules.Count > 1) Rules.Remove(row);
    }

    /// <summary>「试一下」: 把当前(可能未保存的)类型送后端模拟匹配, 直接回答"能不能识别"。</summary>
    [RelayCommand]
    private async Task TryAsync()
    {
        if (Trying) return;
        var api = _page.Api;
        if (api is null) return;

        TryResult = "";
        var content = TryContent.Trim();
        if (content.Length == 0)
        {
            TryOk = false;
            TryResult = I18n.T("2570");
            return;
        }

        // 草稿类型: 代号取当前值 (可为空则占位), 规则/扩展名取当前输入 —— 允许半成品
        var draft = new MatchType
        {
            Id = Id.Trim().Length > 0 ? Id.Trim() : "draft",
            Label = Label.Trim(),
            Kind = IsFileExt ? "fileExt" : "text",
        };
        if (IsFileExt)
        {
            draft.Exts = MatchTypesDialogViewModel.NormalizeExts(ExtsText);
        }
        else
        {
            draft.Rules = Rules.Where(r => r.Value.Trim().Length > 0)
                .Select(r => new MatchRule { Op = r.Op.Value, Value = r.Value.Trim() })
                .ToList();
        }

        // 快照说明: enable=false 规避"启用必填热键"; entries 用"覆盖型测试行为"
        // (文本 → plain 覆盖的 search; 文件 → fileExt * 覆盖的 open_path), 否则后端覆盖校验会 400。
        var isFile = IsFileExt;
        var req = new SelectedActionTestRequest
        {
            Content = content,
            IsFile = isFile,
            SelectedAction = new SelectedAction
            {
                Hotkey = "",
                Enable = false,
                Mappings =
                [
                    new SelectedMapping
                    {
                        MatchType = isFile ? "fileExt" : "textType",
                        MatchValue = "type:" + draft.Id,
                        Entries =
                        [
                            new SelectedEntry
                            {
                                Behavior = isFile ? "open_path" : "search",
                                ActionValue = "",
                                WorkingDir = "",
                                Options = new RuleOptions(),
                            },
                        ],
                    },
                ],
            },
            MatchTypes = [draft],
        };

        Trying = true;
        try
        {
            var resp = await api.TestSelectedActionAsync(req);
            if (!resp.Success)
            {
                TryOk = false;
                TryResult = resp.ErrorMessage ?? I18n.T("992");
                return;
            }
            TryOk = resp.Value?.Matched == true;
            TryResult = TryOk ? I18n.T("2558") : I18n.T("2559");
        }
        finally
        {
            Trying = false;
        }
    }

    /// <summary>「保存，并设置命中后做什么」(路径①: 同建专属行为并强绑定)。</summary>
    [RelayCommand]
    private Task SaveWithActionAsync() => SaveAsync(withAction: true);

    /// <summary>「先只建类型，稍后再设」(路径②: 留桩)。</summary>
    [RelayCommand]
    private Task SaveTypeOnlyAsync() => SaveAsync(withAction: false);

    /// <summary>保存: 校验 → 写 matchTypes → (可选) 建专属行为包 → 落配置 → 重启生效。</summary>
    private async Task SaveAsync(bool withAction)
    {
        if (Saving) return;
        Error = null;

        var id = Id.Trim().ToLowerInvariant();
        var label = Label.Trim();
        if (!Regex.IsMatch(id, "^[a-z][a-z0-9_]{0,23}$"))
        {
            // 用户在高级选项里手改过代号: 给出明确的格式要求; 未手改时补一个自动代号兜底
            _autoSettingId = true;
            Id = _page.UniqueId(IdHint(label));
            _autoSettingId = false;
            id = Id.ToLowerInvariant();
            if (label.Length == 0)
            {
                Error = I18n.T("2542");
                return;
            }
        }
        if (label.Length == 0)
        {
            Error = I18n.T("2542");
            return;
        }
        if (!IsEdit && _page.IdTaken(id))
        {
            _autoSettingId = true;
            Id = _page.UniqueId(id);
            _autoSettingId = false;
            id = Id.ToLowerInvariant();
        }

        var mt = _page.Config.MatchTypes.FirstOrDefault(t => t.Id == id);
        var isNew = mt is null;
        mt ??= new MatchType { Id = id };
        mt.Label = label;
        mt.LabelEn = LabelEn.Trim();
        mt.Kind = IsFileExt ? "fileExt" : "text";

        if (IsFileExt)
        {
            var exts = MatchTypesDialogViewModel.NormalizeExts(ExtsText);
            if (exts.Count == 0)
            {
                Error = I18n.T("2544");
                return;
            }
            mt.Exts = exts;
            mt.Rules = [];
        }
        else
        {
            var rules = Rules
                .Where(r => r.Value.Trim().Length > 0)
                .Select(r => new MatchRule { Op = r.Op.Value, Value = r.Value.Trim() })
                .ToList();
            if (rules.Count == 0)
            {
                Error = I18n.T("2543");
                return;
            }
            mt.Rules = rules;
            mt.Exts = [];
        }

        if (isNew) _page.Config.MatchTypes.Add(mt);

        var api = _page.Api;
        var createBound = withAction && api is not null;
        if (createBound && BehaviorCatalog.Packs.Any(p => string.Equals(p.Id, id, StringComparison.OrdinalIgnoreCase)))
        {
            // 专属行为包与类型同名: 已存在时不静默覆盖 (会丢用户对行为的修改), 引导去行为库编辑
            Error = I18n.T("2552");
            if (isNew) _page.Config.MatchTypes.RemoveAll(t => t.Id == id);
            return;
        }
        if (createBound)
        {
            var pack = new BehaviorPack
            {
                Id = id,
                Name = BehaviorName.Trim().Length > 0 ? BehaviorName.Trim() : label,
                SpecVersion = 1,
                Version = "1.0.0",
                AppliesTo =
                [
                    IsFileExt
                        ? new BehaviorAppliesTo { Type = "fileExt", Exts = [.. mt.Exts] }
                        : new BehaviorAppliesTo { Type = "textType", Value = "type:" + id },
                ],
                Entry = new BehaviorEntry
                {
                    Kind = "builtin",
                    Action = SelectedBaseAction.Value,
                    Params = new BehaviorEntryParams { ActionValue = ActionValue, WorkingDir = WorkingDir },
                },
                BoundTypeId = id,
                Source = "user",
            };
            var resp = await api!.CreateBehaviorAsync(pack);
            if (!resp.Success)
            {
                // 失败原因直给 (此前只显示"保存"两个字, 用户无从下手)
                Error = resp.ErrorMessage ?? I18n.T("992");
                if (isNew) _page.Config.MatchTypes.RemoveAll(t => t.Id == id);
                return;
            }
        }

        Saving = true;
        try
        {
            if (!await _page.SaveAsync())
            {
                Error = I18n.T("1079"); // 保存失败 (含后端 400 message 由全局提示条承载)
                return;
            }
            await _page.ReloadAsync();
            await _page.ApplyBehaviorsIfNeeded(createBound);
            if (isNew) _page.NoteCreated(id);
            _page.CloseEditor();
        }
        finally
        {
            Saving = false;
        }
    }

    [RelayCommand]
    private void Cancel() => _page.CloseEditor();

    public void RefreshLanguage()
    {
        LanguageTick++;
        OnPropertyChanged(nameof(Title));
    }
}

/// <summary>
/// 匹配类型弹窗 VM。数据真源 = Config.MatchTypes (自定义) + 后端内置文本特征注册表
/// (behaviors/textfeatures.go, 界面侧镜像 = ActionSchemeCatalog.TextTypes, 顺序须一致)。
/// </summary>
public sealed partial class MatchTypesDialogViewModel : ObservableObject, ILanguageRefresh
{
    private readonly MainViewModel _main;

    public MatchTypesDialogViewModel(MainViewModel main)
    {
        _main = main;
        ReloadRows();
    }

    public MainViewModel Main => _main;
    public Config Config => _main.Config ?? throw new InvalidOperationException("Config 未加载");
    public ISettingsApi? Api => _main.Session.Api;

    /// <summary>弹窗内提示条 (保存结果 / 生效失败等非阻断反馈)。</summary>
    [ObservableProperty] private string _statusText = "";

    [ObservableProperty] private int _languageTick;

    /// <summary>确认对话框委托 (视图注入; 删除类型时用)。</summary>
    public Func<string, string, Task<bool>>? ConfirmAsync { get; set; }

    /// <summary>
    /// 本次会话内最后新建的类型 id (弹窗关闭后由调用方读取, 用于"自动选中新建类型"这类连贯体验);
    /// 仅新建时置位, 编辑/删除不动它。
    /// </summary>
    public string? LastCreatedTypeId { get; private set; }

    /// <summary>编辑器新建成功后回调 (由编辑面板调用)。</summary>
    /// <summary>
    /// 保存成功回调: 记录新建 id 并把选中指向新建出的真实行 (草稿行随后由 CloseEditor 移除)。
    /// 先清 <c>_draftRow</c> 以解除"选择钉住", 否则新选中会被弹回草稿。
    /// </summary>
    public void NoteCreated(string id)
    {
        LastCreatedTypeId = id;
        _draftRow = null;
        SelectedRow = Rows.FirstOrDefault(r => r.Id == id) ?? SelectedRow;
    }

    // ------------------------------------------------------------- 列表 (主从布局: 左列表 + 右详情)

    /// <summary>列表行 (单一扁平列表): 4 个内置文本特征在前, 自定义类型按 文本类 → 文件类 追加。</summary>
    public ObservableCollection<MatchTypeRowVm> Rows { get; } = [];

    /// <summary>草稿行的内部 id: 不符合类型 id 正则, 因此不可能与真实类型冲突。</summary>
    private const string DraftRowId = "__draft__";

    /// <summary>新建期间的草稿行 (null = 未在新建); 只存在于 Rows, 不写入 Config.MatchTypes。</summary>
    private MatchTypeRowVm? _draftRow;

    /// <summary>进入新建前的选中项 (取消时恢复)。</summary>
    private MatchTypeRowVm? _selectionBeforeCreate;

    /// <summary>选择钉住期间的重入保护。</summary>
    private bool _isPinningSelection;

    /// <summary>当前选中行 (右侧详情与底部按钮的作用对象)。</summary>
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelection))]
    [NotifyPropertyChangedFor(nameof(CanEditSelected))]
    [NotifyPropertyChangedFor(nameof(CanRemoveSelected))]
    [NotifyPropertyChangedFor(nameof(CanSetAction))]
    private MatchTypeRowVm? _selectedRow;

    public bool HasSelection => SelectedRow is not null;

    /// <summary>列表操作按钮可用性: 内联表单打开时一律禁用 (交给表单自身的两条保存路径)。</summary>
    public bool CanUseActions => Editor is null;

    /// <summary>仅自定义类型可编辑 (且不在编辑态)。</summary>
    public bool CanEditSelected => CanUseActions && SelectedRow?.IsCustom == true;

    /// <summary>仅自定义类型可删除 (且不在编辑态)。</summary>
    public bool CanRemoveSelected => CanUseActions && SelectedRow?.IsCustom == true;

    /// <summary>「配置行为」仅对"尚未配置行为"的自定义类型可用 (且不在编辑态)。</summary>
    public bool CanSetAction => CanUseActions && SelectedRow is { IsCustom: true, NeedsBehavior: true };

    /// <summary>尚无任何自定义类型。</summary>
    public bool HasNoCustomTypes => Config.MatchTypes.Count == 0;

    /// <summary>
    /// 按配置与行为目录重建列表 (行为目录未加载时专属行为数按 0 计 ⇒ 未配置标记可见);
    /// 重建后按 id 恢复原选中行 (保存/删除后不跳走), 原行已不存在则清空选择。
    /// </summary>
    public void ReloadRows()
    {
        var keep = SelectedRow?.Id;
        Rows.Clear();

        foreach (var (value, labelKey) in ActionSchemeCatalog.TextTypes)
        {
            Rows.Add(new MatchTypeRowVm
            {
                Id = value,
                Kind = "text",
                IsBuiltin = true,
                Label = I18n.T(labelKey),
                Summary = I18n.T("2520") + " · " + value,
                DedicatedCount = CountDedicated(value, value),
            });
        }

        foreach (var mt in Config.MatchTypes.Where(t => t.Kind != "fileExt")) Rows.Add(BuildRow(mt));
        foreach (var mt in Config.MatchTypes.Where(t => t.Kind == "fileExt")) Rows.Add(BuildRow(mt));

        foreach (var r in Rows) r.NotifyDerived();
        if (_draftRow is not null)
        {
            // 新建期间列表被重建 (语言切换 / 保存回调): 重新追加草稿行并保持选中
            _draftRow = CreateDraftRow();
            Rows.Add(_draftRow);
            SelectedRow = _draftRow;
        }
        else
        {
            SelectedRow = keep is null ? null : Rows.FirstOrDefault(r => r.Id == keep);
        }
        OnPropertyChanged(nameof(HasNoCustomTypes));
    }

    /// <summary>
    /// 构造草稿行: 列表最末的"正在创建、尚未填写"占位项。
    /// 文案 = 标题「新建匹配类型」/ 状态「新建中」/ 副标题「尚未填写」; 文字用弱化色 (见视图 IsDraft 样式)。
    /// </summary>
    private MatchTypeRowVm CreateDraftRow() => new()
    {
        Id = DraftRowId,
        Kind = "text",
        IsBuiltin = false,
        IsDraft = true,
        Label = I18n.T("2522"),
        Summary = I18n.T("2579"),
    };

    /// <summary>
    /// 新建期间把选择钉在草稿行上 —— 避免出现"选中行 ≠ 正在编辑的对象"的错位
    /// (右窗在编辑草稿, 左列表却高亮别的行)。保存/取消时草稿行移除后即解除。
    /// </summary>
    partial void OnSelectedRowChanged(MatchTypeRowVm? value)
    {
        if (_isPinningSelection || _draftRow is null) return;
        if (ReferenceEquals(value, _draftRow)) return;
        _isPinningSelection = true;
        SelectedRow = _draftRow;
        _isPinningSelection = false;
    }

    /// <summary>构造自定义类型的列表行。</summary>
    private MatchTypeRowVm BuildRow(MatchType mt) => new()
    {
        Id = mt.Id,
        Kind = mt.Kind,
        IsBuiltin = false,
        Model = mt,
        Label = string.IsNullOrWhiteSpace(mt.Label) ? mt.Id : mt.Label,
        Summary = Summarize(mt),
        DedicatedCount = CountDedicated(mt.Id, "type:" + mt.Id),
    };

    /// <summary>行副标题 / 详情第二行: 匹配条件的通俗描述 (与行为库的"生效前提"摘要同风格)。</summary>
    private static string Summarize(MatchType mt) => mt.Kind == "fileExt"
        ? I18n.T("2528") + "：" + string.Join(", ", mt.Exts)
        : I18n.T("2525") + "：" + string.Join(" · ", mt.Rules.Select(r => OpLabel(r.Op) + " " + r.Value));

    /// <summary>算子摘要用通俗说法 (与下拉同源)。</summary>
    private static string OpLabel(string op) =>
        MatchRuleRowVm.Ops.FirstOrDefault(o => o.Value == op)?.Label ?? op;

    /// <summary>
    /// 专属行为数: 行为包的 appliesTo 精确引用本类型 (refValue), 或 boundTypeId 指向本类型 id。
    /// "继承行为" (plain 两段式覆盖命中的内置包) 不计入 —— 它由行为库统一提供, 不属于本类型的专属。
    /// </summary>
    private static int CountDedicated(string id, string refValue) =>
        BehaviorCatalog.Packs.Count(p =>
            p.AppliesTo.Any(a => string.Equals(a.Value, refValue, StringComparison.OrdinalIgnoreCase))
            || string.Equals(p.BoundTypeId, id, StringComparison.OrdinalIgnoreCase));

    /// <summary>内部代号是否已被占用 (内置 4 特征名 / 既有自定义类型 / 文件分组名)。</summary>
    public bool IdTaken(string id) =>
        ActionSchemeCatalog.TextTypes.Any(t => t.Value == id)
        || Config.MatchTypes.Any(t => t.Id == id)
        || Config.FileGroups.Any(g => g.Name == id);

    /// <summary>
    /// 生成一个**未被占用**的内部代号 (对用户隐藏的字段, 无需其理解):
    /// 基底可为空 (中文名称取不到 ASCII) → 退化为 <c>mt1</c>、<c>mt2</c>…; 冲突则追加数字后缀。
    /// </summary>
    public string UniqueId(string? hint)
    {
        var baseId = (hint ?? "").Trim().ToLowerInvariant();
        if (baseId.Length == 0 || !char.IsAsciiLetter(baseId[0]))
        {
            baseId = "mt" + (Config.MatchTypes.Count + 1);
        }
        if (baseId.Length > 22) baseId = baseId[..22];
        if (!IdTaken(baseId)) return baseId;
        for (var n = 2; n < 1000; n++)
        {
            var candidate = baseId + n;
            if (candidate.Length > 24) candidate = baseId[..(24 - n.ToString().Length)] + n;
            if (!IdTaken(candidate)) return candidate;
        }
        return "mt" + Guid.NewGuid().ToString("N")[..8];
    }

    /// <summary>后缀串归一化 (与 Go normalizeExts / ActionSchemeCatalog.NormalizeExts 同语义)。</summary>
    public static List<string> NormalizeExts(string? matchValue) => ActionSchemeCatalog.NormalizeExts(matchValue);

    // ------------------------------------------------------------- 编辑面板

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsEditorOpen))]
    [NotifyPropertyChangedFor(nameof(CanUseActions))]
    [NotifyPropertyChangedFor(nameof(CanEditSelected))]
    [NotifyPropertyChangedFor(nameof(CanRemoveSelected))]
    [NotifyPropertyChangedFor(nameof(CanSetAction))]
    private MatchTypeEditorVm? _editor;

    public bool IsEditorOpen => Editor is not null;

    /// <summary>重拉行为目录并刷新列表 (行为包可能增删 ⇒ 已设置动作数变化)。</summary>
    public async Task ReloadAsync()
    {
        if (Api is not null) await BehaviorCatalog.LoadAsync(Api);
        ReloadRows();
    }

    /// <summary>打开新建面板 (先确保行为目录已加载, 使"命中后做什么"有候选)。</summary>
    [RelayCommand]
    private async Task OpenCreateAsync()
    {
        if (Api is not null && !BehaviorCatalog.Loaded) await BehaviorCatalog.LoadAsync(Api);
        _selectionBeforeCreate = SelectedRow;   // 取消时恢复
        _draftRow = CreateDraftRow();
        Rows.Add(_draftRow);                    // 列表最末: 正在创建、尚未填写
        SelectedRow = _draftRow;
        Editor = new MatchTypeEditorVm(this, null);
    }

    /// <summary>打开编辑面板 (代号锁定; 改名恒安全)。</summary>
    [RelayCommand]
    private void OpenEdit(MatchTypeRowVm? row)
    {
        if (row?.Model is null || row.IsBuiltin) return;
        Editor = new MatchTypeEditorVm(this, row.Model);
    }

    /// <summary>关闭面板 (取消)。</summary>
    /// <summary>
    /// 关闭内联表单: 移除草稿行; 若当前选中行已不在列表 (取消路径) 则恢复进入新建前的选中项。
    /// 保存路径下 NoteCreated 已把选中指向新建出的真实项, 故不会被覆盖。
    /// </summary>
    public void CloseEditor()
    {
        foreach (var draft in Rows.Where(r => r.IsDraft).ToList()) Rows.Remove(draft);
        _draftRow = null;
        if (SelectedRow is null || !Rows.Contains(SelectedRow)) SelectedRow = _selectionBeforeCreate;
        Editor = null;
    }

    // ------------------------------------------------------------- 删除

    /// <summary>
    /// 删除自定义类型 (确认后落配置 + 级联删除同名专属行为包)。
    /// 映射里的 type:<id> 引用不在此清理 —— 由运行时按"悬空引用不命中"处理 (与 Go 同口径)。
    /// </summary>
    public async Task AskRemoveAsync(MatchTypeRowVm? row)
    {
        if (row is null || row.IsBuiltin || row.Model is null) return;

        var confirmed = ConfirmAsync is not null
            && await ConfirmAsync(I18n.T("2536"), string.Format(I18n.T("2545"), row.Label));
        if (!confirmed) return;

        var orphans = BehaviorCatalog.Packs
            .Where(p => p.Source == "user" && string.Equals(p.BoundTypeId, row.Id, StringComparison.OrdinalIgnoreCase))
            .Select(p => p.Id)
            .ToList();

        Config.MatchTypes.RemoveAll(t => t.Id == row.Id);
        if (!await SaveAsync())
        {
            StatusText = I18n.T("1079");
            return;
        }

        if (Api is not null)
        {
            foreach (var packId in orphans) await Api.DeleteBehaviorAsync(packId);
            if (orphans.Count > 0) await ApplyBehaviorsIfNeeded(true);
        }
        await ReloadAsync();
        StatusText = "";
    }

    // ------------------------------------------------------------- 保存 / 生效

    /// <summary>主配置保存 (走 MainViewModel 咽喉; 失败由调用方回显)。</summary>
    public Task<bool> SaveAsync() => _main.SaveAsync(force: true);

    /// <summary>新建/删除专属行为包后显式重启引擎使其生效 (与行为库窗口同语义)。</summary>
    public async Task ApplyBehaviorsIfNeeded(bool changed)
    {
        if (!changed || Api is null) return;
        try
        {
            await Api.ApplyBehaviorsAsync();
        }
        catch
        {
            StatusText = I18n.T("1079"); // 保存成功但重启失败: 提示手动重载
        }
    }

    // ------------------------------------------------------------- 语言刷新

    public void OnLanguageChanged()
    {
        LanguageTick++;
        ReloadRows();
        Editor?.RefreshLanguage();
    }
}
