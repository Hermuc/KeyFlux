using System.Collections.ObjectModel;
using System.Text.Json;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 选中动作页的一张聚合卡 (matchType 分区): 卡内列出该分区全部类型 toggle (全部可见),
/// 点亮 = 正在查看; 小圆点 = 已配置; 点击 toggle 切详情; 卡右侧红 ✕ 删除当前点亮类型的配置
/// (确认 + 立即保存, 其他类型不动)。
///
/// 类型 toggle 动态生成:
///   - 文本卡 = <see cref="ActionSchemeCatalog.TextTypes"/> (url/path/magnet/bilibili/plain, plain 恒末)
///              + <c>config.MatchTypes</c> 中 <c>Kind=="text"</c> (引用 <c>type:&lt;id&gt;</c>);
///   - 文件卡 = 全部 <c>config.FileGroups</c> (引用 <c>group:&lt;name&gt;</c>)
///              + <c>config.MatchTypes</c> 中 <c>Kind=="fileExt"</c> (引用 <c>type:&lt;id&gt;</c>);
///   - 任何已配置但不被上述规则覆盖的 mapping (孤儿) 也补一个 toggle, 避免存量数据被隐藏。
///
/// 未配置类型点击进入「待配置」详情态 (不落盘); 用户添加行为后才创建 mapping
/// (满足后端 ValidateSelectedAction 对非空 entries 的要求)。
/// </summary>
public sealed partial class TypeCardVm : ObservableObject
{
    private readonly SelectedActionPageViewModel _page;

    public TypeCardVm(SelectedActionPageViewModel page, string matchType)
    {
        _page = page;
        MatchType = matchType;
        Toggles = new ObservableCollection<TypeToggleVm>();
    }

    public SelectedActionPageViewModel Page => _page;

    /// <summary>"textType" | "fileExt"。</summary>
    public string MatchType { get; }

    /// <summary>卡标题 (文本特征 / 文件后缀)。</summary>
    public string Title => ActionSchemeCatalog.MatchTypeLabel(MatchType);

    /// <summary>语言切换刻度, 卡内绑定读此重译 (委托页级递增计数)。</summary>
    public int LanguageTick => _page.LanguageTick;

    public ObservableCollection<TypeToggleVm> Toggles { get; }

    /// <summary>当前正在查看的类型 id (卡内唯一点亮项)。</summary>
    [ObservableProperty]
    private string _selectedToggleId = "";

    /// <summary>当前查看类型的编辑器 (已配置=真实 mapping; 未配置=transient, 不落盘)。</summary>
    [ObservableProperty]
    private MappingRowVm? _detail;

    partial void OnSelectedToggleIdChanged(string value)
    {
        foreach (var t in Toggles) t.RefreshState();
        BuildDetail();
        OnPropertyChanged(nameof(CanDeleteSelected));
        OnPropertyChanged(nameof(IsPending));
    }

    partial void OnDetailChanged(MappingRowVm? value)
    {
        OnPropertyChanged(nameof(IsPending));
        OnPropertyChanged(nameof(CanDeleteSelected));
    }

    /// <summary>未配置且尚无任何行为的查看态 (「待配置」提示)。</summary>
    public bool IsPending => Detail is { IsTransient: true } && Detail.Mapping.Entries.Count == 0;

    /// <summary>当前点亮类型是否已配置 (卡右侧红 ✕ 是否可点)。</summary>
    public bool CanDeleteSelected
        => !string.IsNullOrEmpty(SelectedToggleId) && IsTypeConfigured(SelectedToggleId);

    // ------------------------------------------------------------- 构建 / 重建 toggle

    /// <summary>首次 / 配置变化后重建卡内类型 toggle 集合, 并恢复/挑选当前查看项。</summary>
    public void RebuildToggles()
    {
        var prev = SelectedToggleId;
        Toggles.Clear();
        foreach (var (id, label) in BuildToggleSpecs()) Toggles.Add(new TypeToggleVm(this, id, label));

        // 选择: 保持原选 (仍存在) → 首个已配置 → 首个
        string sel;
        if (Toggles.Any(t => t.Id == prev)) sel = prev;
        else sel = Toggles.FirstOrDefault(t => IsTypeConfigured(t.Id))?.Id
                   ?? Toggles.FirstOrDefault()?.Id ?? "";

        if (sel != SelectedToggleId)
        {
            SelectedToggleId = sel; // 触发 OnSelectedToggleIdChanged → BuildDetail
        }
        else
        {
            // 同一 id (如删除后回退): 刷新派生态并重建详情
            foreach (var t in Toggles) t.RefreshState();
            BuildDetail();
        }
    }

    private IEnumerable<(string Id, string Label)> BuildToggleSpecs()
    {
        if (MatchType == "textType")
        {
            foreach (var (value, labelKey) in ActionSchemeCatalog.TextTypes)
                yield return (value, I18n.T(labelKey));
            foreach (var t in _page.Config.MatchTypes.Where(t => t.Kind == "text"))
                yield return ("type:" + t.Id, ActionSchemeCatalog.CustomTextTypeLabel(t));
        }
        else // fileExt
        {
            foreach (var g in _page.Config.FileGroups)
                yield return ("group:" + g.Name, g.Label);
            foreach (var t in _page.Config.MatchTypes.Where(t => t.Kind == "fileExt"))
                yield return ("type:" + t.Id, ActionSchemeCatalog.CustomTextTypeLabel(t));
        }

        // 孤儿: 已配置但不被上述规则覆盖的 mapping 也补 toggle, 防存量数据隐藏
        int orphan = 0;
        foreach (var m in _page.Config.SelectedAction.Mappings.Where(m => m.MatchType == MatchType))
        {
            if (IsCovered(m)) continue;
            yield return ("orphan:" + orphan++, m.MatchValue);
        }
    }

    private bool IsCovered(SelectedMapping m)
    {
        if (MatchType == "textType")
        {
            if (ActionSchemeCatalog.TextTypes.Any(t => t.Value == m.MatchValue)) return true;
            return m.MatchValue.StartsWith("type:") &&
                   _page.Config.MatchTypes.Any(t => t.Kind == "text" && "type:" + t.Id == m.MatchValue);
        }
        // fileExt
        if (m.MatchValue.StartsWith("type:"))
            return _page.Config.MatchTypes.Any(t => t.Kind == "fileExt" && "type:" + t.Id == m.MatchValue);
        return _page.Config.FileGroups.Any(g =>
            ActionSchemeCatalog.SameExts(ActionSchemeCatalog.NormalizeExts(m.MatchValue), g.Exts));
    }

    // ------------------------------------------------------------- 查询 / 选择

    /// <summary>该类型是否已配置 (存在对应 mapping)。</summary>
    public bool IsTypeConfigured(string id) => FindMappingForType(id) is not null;

    /// <summary>类型标识 → 显示文案 (供删除确认等场景)。</summary>
    public string LabelFor(string id)
    {
        var t = Toggles.FirstOrDefault(x => x.Id == id);
        return t?.Label ?? id;
    }

    /// <summary>选择查看某类型 (切换详情; 不改变配置)。</summary>
    public void SelectType(string id)
    {
        if (SelectedToggleId == id) return;
        SelectedToggleId = id;
    }

    internal SelectedMapping? FindMappingForType(string id)
    {
        var mappings = _page.Config.SelectedAction.Mappings;
        if (MatchType == "textType")
            return mappings.FirstOrDefault(m => m.MatchType == "textType" && m.MatchValue == id);

        if (id.StartsWith("group:"))
        {
            var name = id["group:".Length..];
            var g = _page.Config.FileGroups.FirstOrDefault(x => x.Name == name);
            if (g is null) return null;
            return mappings.FirstOrDefault(m => m.MatchType == "fileExt" &&
                ActionSchemeCatalog.SameExts(ActionSchemeCatalog.NormalizeExts(m.MatchValue), g.Exts));
        }
        // type:<id> 或 orphan: 直接按 matchValue 命中
        return mappings.FirstOrDefault(m => m.MatchType == "fileExt" && m.MatchValue == id);
    }

    private string BuildTransientMatchValue(string id)
    {
        if (MatchType == "textType") return id; // 文本特征: matchValue = 特征值本身
        if (id.StartsWith("group:"))
        {
            var name = id["group:".Length..];
            var g = _page.Config.FileGroups.FirstOrDefault(x => x.Name == name);
            return g is null ? "" : string.Join(",", g.Exts);
        }
        return id; // type:<id> / orphan: 原样
    }

    // ------------------------------------------------------------- 详情 (编辑器)

    private void BuildDetail()
    {
        // 丢弃旧 transient 编辑器的订阅
        if (Detail is { IsTransient: true } oldTransient)
            oldTransient.EntriesChanged -= OnEditorEntriesChanged;
        Detail = null;

        var id = SelectedToggleId;
        if (string.IsNullOrEmpty(id)) return;

        var mapping = FindMappingForType(id);
        if (mapping is not null)
        {
            var real = new MappingRowVm(_page, mapping, id);
            real.OpenEditor();
            Detail = real;
            return;
        }

        // 未配置 → transient 编辑器 (不落盘)
        var transient = new SelectedMapping
        {
            MatchType = MatchType,
            MatchValue = BuildTransientMatchValue(id),
            Entries = [],
        };
        var editor = new MappingRowVm(_page, transient, id) { IsTransient = true };
        editor.EntriesChanged += OnEditorEntriesChanged;
        editor.OpenEditor();
        Detail = editor;
    }

    private void OnEditorEntriesChanged(MappingRowVm editor)
    {
        // 用户为未配置类型添加首个行为 → 创建 mapping (落盘由保存链路统一处理)
        if (!editor.IsTransient) return;
        if (editor.Mapping.Entries.Count == 0) return; // 不变量: 禁止空 entries

        editor.IsTransient = false;
        editor.EntriesChanged -= OnEditorEntriesChanged;
        _page.Config.SelectedAction.Mappings.Add(editor.Mapping);
        RebuildToggles(); // 该 toggle 变为已配置, 并重建详情 (包裹真实 mapping)
        _page.OnCardMappingChanged();
    }

    // ------------------------------------------------------------- 删除当前类型配置

    /// <summary>删除当前点亮类型的配置 (确认 + 立即保存; 其他类型不动)。</summary>
    [RelayCommand]
    private async Task DeleteSelectedAsync()
    {
        var id = SelectedToggleId;
        if (string.IsNullOrEmpty(id)) return;
        var mapping = FindMappingForType(id);
        if (mapping is null) return; // 未配置, 无删除目标

        var confirmed = _page.ConfirmAsync is not null
            ? await _page.ConfirmAsync(I18n.T("967"), string.Format(I18n.T("1109"), LabelFor(id)))
            : false;
        if (!confirmed) return;

        _page.Config.SelectedAction.Mappings.Remove(mapping);
        await _page.SaveConfigAsync(); // 立即保存
        RebuildToggles();
        _page.OnCardMappingChanged();
    }

    // ------------------------------------------------------------- 新建匹配类型

    /// <summary>卡内「+ 新建匹配类型」入口: 复用页面 MatchTypesDialogAsync; 关闭后重建 toggle。</summary>
    [RelayCommand]
    private async Task NewTypeAsync()
    {
        if (_page.MatchTypesDialogAsync is null) return;
        var created = await _page.MatchTypesDialogAsync();
        RebuildToggles();
        if (!string.IsNullOrEmpty(created))
        {
            var id = "type:" + created;
            if (Toggles.Any(t => t.Id == id)) SelectedToggleId = id;
        }
    }

    /// <summary>语言切换: 标题与全部 toggle 文案重译 (内置走 i18n, 用户数据不变)。</summary>
    public void RefreshLanguage()
    {
        RebuildToggles(); // 重建 toggle 文案 (内置走 i18n) 并保持当前查看项
        Detail?.RefreshLanguage();
    }
}
