using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 聚合卡内的一个类型开关 (卡内全部类型 toggle 始终可见):
///   - 点亮 (<see cref="IsLit"/>) = 当前卡正在查看该类型 (卡内唯一点亮项; 点它即切详情);
///   - 小圆点 (<see cref="IsConfigured"/>) = 该类型已配置 (存在对应 mapping);
///   - 点击切换卡当前查看的类型, 不改变任何配置。
/// </summary>
public sealed partial class TypeToggleVm : ObservableObject
{
    private readonly TypeCardVm _card;

    public TypeToggleVm(TypeCardVm card, string id, string label)
    {
        _card = card;
        Id = id;
        Label = label;
    }

    /// <summary>类型标识: 文本特征=特征值本身 (url/path/...); 文件后缀="group:&lt;name&gt;" 或 "type:&lt;id&gt;"; 孤儿匹配="orphan:&lt;index&gt;"。</summary>
    public string Id { get; }

    /// <summary>显示文案 (内置走 i18n; 分组/自定义走用户数据; 孤儿回退 matchValue)。</summary>
    public string Label { get; }

    /// <summary>点亮 = 当前卡正在查看此类型。</summary>
    public bool IsLit => _card.SelectedToggleId == Id;

    /// <summary>小圆点微标 = 该类型已配置 (存在对应 mapping)。</summary>
    public bool IsConfigured => _card.IsTypeConfigured(Id);

    [RelayCommand]
    private void Select() => _card.SelectType(Id);

    /// <summary>卡状态变化时刷新本开关的派生态 (点亮/已配置)。</summary>
    internal void RefreshState()
    {
        OnPropertyChanged(nameof(IsLit));
        OnPropertyChanged(nameof(IsConfigured));
    }
}
