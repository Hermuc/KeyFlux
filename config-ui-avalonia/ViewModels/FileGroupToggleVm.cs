using CommunityToolkit.Mvvm.ComponentModel;

namespace KeyFlux.Settings.ViewModels;
/// <summary>
/// 文件后缀行的分组 Toggle (与 textType 特征 Toggle 同款交互: 所见即所选)。
/// 点亮 = 选中该分组 (复用 FileGroupSelected 填入链路, 保留写回关联语义);
/// 再点已亮项 = 解除关联并清空条件值 (等价旧下拉的「无」)。
/// 勾选态由行 VM 的 FileGroupSelected 派生, 行 VM 变更后经 NotifyChecked 广播,
/// 与 textType 教训一致: 派生视觉态必须由数据变更方主动通知, 不依赖路由事件时序。
/// </summary>
public sealed class FileGroupToggleVm : ObservableObject
{
    private readonly MappingRowVm _row;

    public FileGroupToggleVm(MappingRowVm row, string name, string label)
    {
        _row = row;
        Name = name;
        Label = label;
    }

    /// <summary>分组标识 (Config.FileGroups[].Name, 写回关联键)。</summary>
    public string Name { get; }

    /// <summary>Toggle 显示文本 (分组自定义标签, 如「图片」; 用户数据, 不走 i18n)。</summary>
    public string Label { get; }

    public bool IsChecked
    {
        get => _row.FileGroupSelected?.Value == Name;
        set
        {
            if (value)
            {
                _row.FileGroupSelected = _row.FileGroupOptions.FirstOrDefault(o => o.Value == Name);
            }
            else if (IsChecked)
            {
                // 只响应"熄灭已亮项"的取消, 熄灭别组由行 VM 广播处理 (互斥)
                _row.FileGroupSelected = _row.FileGroupOptions[0]; // 「无」
            }
        }
    }

    /// <summary>行 VM 广播: FileGroupSelected 变化后重估勾选态。</summary>
    internal void NotifyChecked() => OnPropertyChanged(nameof(IsChecked));
}
