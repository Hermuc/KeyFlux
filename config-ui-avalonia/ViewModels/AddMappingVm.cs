using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;
/// <summary>
/// 添加规则弹窗 (页内 overlay 面板): 选类型 -> 填条件值 -> 勾选行为 (顺序即菜单顺序)。
/// </summary>
public sealed partial class AddMappingVm : ObservableObject
{
    private readonly SelectedActionPageViewModel _page;

    public AddMappingVm(SelectedActionPageViewModel page)
    {
        _page = page;
        // 类型下拉 (动态源, 方案 C7): 文件分组 + 分隔项 + 内置 4 文本特征 + 配置派生文本类型
        _typeOptions = ActionSchemeCatalog.BuildTypeOptions(page.Config);
        _typeSelected = _typeOptions.FirstOrDefault(o => !o.IsSeparator);
        if (_typeSelected is not null && IsFileExt)
        {
            MatchValue = GroupMatchValue ?? ""; // 初始即分组项: 同步后缀集到条件值 (构造期回调不触发)
        }
        RebuildPicks();
    }

    /// <summary>弹窗说明 (1111)。</summary>
    public string Hint => I18n.T("1111");

    /// <summary>语言切换刻度, 弹窗内绑定读此重译。</summary>
    public int LanguageTick => _page.LanguageTick;

    // ---- 步骤 1: 类型 ----

    private readonly List<ComboOption> _typeOptions;

    /// <summary>类型下拉项: 分组在前、IsSeparator 分隔项居中、文本特征在后 (分隔线随分组增删动态移动)。</summary>
    public List<ComboOption> TypeOptions => _typeOptions;

    [ObservableProperty]
    private ComboOption? _typeSelected;

    partial void OnTypeSelectedChanged(ComboOption? value)
    {
        if (value is null) return;
        if (value.IsSeparator)
        {
            // 键盘导航落在分隔行: 自动跳到下一个真实类型 (循环回绕); 点击由容器 IsHitTestVisible=false 拦截
            var idx = _typeOptions.IndexOf(value);
            ComboOption? next = null;
            for (var k = idx + 1; k < _typeOptions.Count; k++)
                if (!_typeOptions[k].IsSeparator) { next = _typeOptions[k]; break; }
            if (next is null)
                for (var k = idx - 1; k >= 0; k--)
                    if (!_typeOptions[k].IsSeparator) { next = _typeOptions[k]; break; }
            TypeSelected = next;
            return;
        }
        if (IsFileExt)
        {
            // 分组项: 条件值 = 展开后缀集; type: 文件引用: 条件值 = 引用本身 (不展开)
            MatchValue = TypeSelected?.Value?.StartsWith("group:") == true
                ? GroupMatchValue ?? ""
                : TypeSelected?.Value ?? "";
        }
        else
        {
            // 文本特征 (内置 url/path/magnet/plain 或自定义 type:<id> 引用): 条件值即选中值本身
            MatchValue = TypeSelected?.Value ?? "";
        }
        OnPropertyChanged(nameof(IsFileExt));
        OnPropertyChanged(nameof(ShowStubHint)); // 留桩提示随选中类型切换
        RebuildPicks(); // 无条件重建: 各类型覆盖集不同 (fileExt 按条件值 / 文本特征按特征词)
    }

    /// <summary>当前类型是否文件后缀类 (分组自带后缀集, 或 type: 文件引用); 二者条件值输入框均不出现。</summary>
    public bool IsFileExt
    {
        get
        {
            var v = TypeSelected?.Value;
            if (v?.StartsWith("group:") == true) return true;
            if (v?.StartsWith("type:") == true)
            {
                // type: 引用: 按配置判定 kind —— text 走文本特征语义, fileExt 走文件后缀语义
                var id = v["type:".Length..];
                return _page.Config.MatchTypes.FirstOrDefault(t => t.Id == id)?.Kind == "fileExt";
            }
            return false;
        }
    }

    /// <summary>
    /// 留桩提示可见性: 选中自定义文本类型 (type:&lt;id&gt;, kind=text) 且它"没有任何专属行为"时为真。
    /// 此时继承段 (plain 覆盖的 5 个内置包) 仍非空, CanConfirm 可用 —— 不死胡同 (§D.2.4)。
    /// </summary>
    public bool ShowStubHint
    {
        get
        {
            var v = TypeSelected?.Value;
            if (v is null || !v.StartsWith("type:")) return false;
            var id = v["type:".Length..];
            var mt = _page.Config.MatchTypes.FirstOrDefault(t => t.Id == id);
            if (mt is null || mt.Kind != "text") return false;
            return !BehaviorCatalog.HasDedicatedBehaviorFor("textType", v);
        }
    }

    /// <summary>分组项对应的后缀集 (matchValue), 非分组项为 null。</summary>
    public string? GroupMatchValue
        => IsFileExt
            ? _page.FileGroups.FirstOrDefault(g => "group:" + g.Name == TypeSelected?.Value)?.Exts
                is { Count: > 0 } exts
                ? string.Join(",", exts)
                : null
            : null;

    /// <summary>
    /// 「＋ 新建匹配类型」: 打开匹配类型弹窗 (视图经页面 VM 注入), 新建成功后自动选中该类型。
    /// 弹窗内取消或无新建时返回 null ⇒ 保持当前选择不动。
    /// </summary>
    [RelayCommand]
    private async Task CreateTypeAsync()
    {
        if (_page.MatchTypesDialogAsync is null) return;
        var created = await _page.MatchTypesDialogAsync();
        if (string.IsNullOrEmpty(created)) return;
        RefreshTypeOptions("type:" + created);
    }

    /// <summary>
    /// 类型下拉重建 (匹配类型弹窗可能增删了自定义类型; 原地清空+重填, 保持实例引用)。
    /// selectValue 非空时选中它 (并触发 OnTypeSelectedChanged 同步条件值与勾选列表)。
    /// </summary>
    public void RefreshTypeOptions(string? selectValue = null)
    {
        _typeOptions.Clear();
        _typeOptions.AddRange(ActionSchemeCatalog.BuildTypeOptions(_page.Config));
        OnPropertyChanged(nameof(TypeOptions));
        if (selectValue is null) return;
        var target = _typeOptions.FirstOrDefault(o => o.Value == selectValue);
        if (target is not null) TypeSelected = target;
        else OnPropertyChanged(nameof(IsFileExt));
    }

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
        OnPropertyChanged(nameof(LanguageTick));
        foreach (var t in _typeOptions)
        {
            var idx = _typeOptions.IndexOf(t);
            // 内置 4 文本特征重新翻译 (走 i18n); 分组 / 自定义 type: 标签为用户数据, 不重译
            _typeOptions[idx] = t.Value switch
            {
                "url" => t with { Label = I18n.T("1059") },
                "path" => t with { Label = I18n.T("1060") },
                "magnet" => t with { Label = I18n.T("1061") },
                "plain" => t with { Label = I18n.T("1062") },
                _ => t,
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
