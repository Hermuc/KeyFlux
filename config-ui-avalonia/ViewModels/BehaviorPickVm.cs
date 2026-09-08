using System.Collections.ObjectModel;
using System.Text.Json;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;
/// <summary>添加映射弹窗内的行为勾选项 (勾选顺序 = 菜单键位顺序)。</summary>
public sealed partial class BehaviorPickVm : ObservableObject
{
    private readonly AddMappingVm _panel;

    public BehaviorPickVm(AddMappingVm panel, BehaviorPack pack)
    {
        _panel = panel;
        Pack = pack;
    }

    public BehaviorPack Pack { get; }

    /// <summary>行为显示名 (按语言)。</summary>
    public string Label => BehaviorCatalog.LabelFor(Pack.Id);

    /// <summary>行为提示 (description)。</summary>
    public string Hint => BehaviorCatalog.HintFor(Pack.Id);

    /// <summary>键位序号 = 勾选列表中的位置 (位置序, 1 起; 0=未勾选)。</summary>
    public int Order
    {
        get
        {
            var picks = _panel.BehaviorPicks;
            var order = 0;
            foreach (var p in picks)
            {
                if (!p.IsChecked) continue;
                order++;
                if (ReferenceEquals(p, this)) return order;
            }
            return 0;
        }
    }

    private bool _isChecked;

    public bool IsChecked
    {
        get => _isChecked;
        set
        {
            if (_isChecked == value) return;
            // 约束: 最多勾选 9 个 (第 10 个勾选被拒绝回弹)
            if (value && _panel.PickedCount >= 9) return;
            _isChecked = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(Order));
            _panel.OnPickChanged();
        }
    }

    private bool _isEnabled = true;

    /// <summary>勾满 9 个后未勾项禁用。</summary>
    public bool IsEnabled
    {
        get => _isEnabled;
        private set => SetProperty(ref _isEnabled, value);
    }

    internal void RefreshGate() => IsEnabled = _isChecked || _panel.PickedCount < 9;

    /// <summary>标签/序号外部刷新 (列表序重排 / 语言切换; 由弹窗 VM 调用)。</summary>
    public void RefreshDisplay()
    {
        OnPropertyChanged(nameof(Label));
        OnPropertyChanged(nameof(Hint));
        OnPropertyChanged(nameof(Order));
    }
}
