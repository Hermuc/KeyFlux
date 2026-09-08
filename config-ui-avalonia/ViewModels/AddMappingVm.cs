using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;
/// <summary>
/// 添加映射弹窗 (页内 overlay 面板, 非独立窗口; VM 可单测):
/// 类型四选一 (文件后缀 + 文本特征 url/path/magnet/plain) -> 条件值 (fileExt 支持分组快捷填入)
/// -> 行为库勾选过滤 (BehaviorCatalog.Covering, 列表顺序即菜单顺序, 与勾选先后无关)。
/// </summary>
public sealed partial class AddMappingVm : ObservableObject
{
    private readonly SelectedActionPageViewModel _page;

    public AddMappingVm(SelectedActionPageViewModel page)
    {
        _page = page;
        // 匹配类型下拉: 具体文件后缀分组在前 (每个分组 = 一级类型), 文本特征四项在后;
        // 分组项 value 编码为 "group:<name>", 选中即定 MatchValue = 该组后缀集 (无需再填条件值)
        _typeOptions = [];
        foreach (var g in page.FileGroups)
        {
            _typeOptions.Add(new ComboOption("group:" + g.Name, g.Label));
        }
        // 分组与文本特征之间的动态分隔项: 分组增删后线始终跟随最后一组 (用户要求);
        // 容器压制 (无 hover/不可点/透明背景) 在 View 层 DropDownOpened 后处理
        _typeOptions.Add(new("", "", IsSeparator: true));
        _typeOptions.Add(new("url", I18n.T("1059")));
        _typeOptions.Add(new("path", I18n.T("1060")));
        _typeOptions.Add(new("magnet", I18n.T("1061")));
        _typeOptions.Add(new("plain", I18n.T("1062")));
        _typeSelected = _typeOptions.FirstOrDefault(o => !o.IsSeparator);
        if (_typeSelected is not null && IsFileExt)
        {
            MatchValue = GroupMatchValue ?? ""; // 初始即分组项: 同步后缀集到条件值 (构造期回调不触发)
        }
        RebuildPicks();
    }

    /// <summary>弹窗说明 (1111)。</summary>
    public string Hint => I18n.T("1111");

    /// <summary>语言切换刻度中继 (弹窗内 Tr 绑定重译触发源; 数值同步页面, 通知随 RefreshLanguage 推送)。</summary>
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
            MatchValue = GroupMatchValue ?? ""; // 分组项: 条件值 = 该组后缀集
        }
        OnPropertyChanged(nameof(IsFileExt));
        OnPropertyChanged(nameof(MatchHint));
        RebuildPicks(); // 无条件重建: 各类型覆盖集不同 (fileExt 按条件值 / 文本特征按特征词)
    }

    /// <summary>当前类型是否文件后缀分组项 (分组自带后缀集, 条件值输入框不出现)。</summary>
    public bool IsFileExt => TypeSelected?.Value?.StartsWith("group:") == true;

    /// <summary>分组项对应的后缀集 (matchValue), 非分组项为 null。</summary>
    public string? GroupMatchValue
        => IsFileExt
            ? _page.FileGroups.FirstOrDefault(g => "group:" + g.Name == TypeSelected?.Value)?.Exts
                is { Count: > 0 } exts
                ? string.Join(",", exts)
                : null
            : null;

    /// <summary>匹配提示 (fileExt=1034 / textType=1035)。</summary>
    public string MatchHint => ActionSchemeCatalog.MatchTypeHint(IsFileExt ? "fileExt" : "textType");

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
            _typeOptions[idx] = t.Value switch
            {
                "fileExt" => t with { Label = I18n.T("1031") },
                "url" => t with { Label = I18n.T("1059") },
                "path" => t with { Label = I18n.T("1060") },
                "magnet" => t with { Label = I18n.T("1061") },
                _ => t with { Label = I18n.T("1062") },
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
