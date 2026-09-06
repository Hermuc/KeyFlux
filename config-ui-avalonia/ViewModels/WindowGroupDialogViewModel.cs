using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>窗口条件类型下拉项 (复刻 windowGroupConditionTypes: index 1-4 -> label:605-608)。</summary>
public sealed record ConditionOption(int Index, string LabelKey)
{
    public string Display => I18n.T(LabelKey);
    public override string ToString() => Display;
}

/// <summary>窗口组对话框行 (复刻 WindowGroupDialog.vue contents 插槽的三列)。</summary>
public sealed partial class WindowGroupRowVm : ObservableObject
{
    public int Id { get; init; }

    /// <summary>组名 (复刻 :disabled="data.id <= 0": 内置组 0/-1 不可改名)。</summary>
    [ObservableProperty]
    private string _name = "";

    /// <summary>窗口标识符 (复刻 :disabled="data.id == 0": 仅 id0 的默认组锁定)。</summary>
    [ObservableProperty]
    private string _value = "";

    /// <summary>条件类型 (复刻 :disabled="data.id <= 0")。</summary>
    [ObservableProperty]
    private ConditionOption? _condition;

    public bool NameEnabled => Id > 0;
    public bool ValueEnabled => Id != 0;
    public bool ConditionEnabled => Id > 0;
}

/// <summary>
/// 窗口条件组对话框 (复刻 components/dialog/WindowGroupDialog.vue + InputKeyValueDialog):
/// 打开时对 options.windowGroups 做副本编辑, 保存才整体替换 (复刻 @save=save)。
/// </summary>
public sealed partial class WindowGroupDialogViewModel : ObservableObject
{
    private readonly MainViewModel _main;

    public static readonly IReadOnlyList<ConditionOption> ConditionOptions =
    [
        new(1, "605"), new(2, "606"), new(3, "607"), new(4, "608"),
    ];

    public WindowGroupDialogViewModel(MainViewModel main)
    {
        _main = main;
        foreach (var g in main.Config?.Options.WindowGroups ?? [])
        {
            Rows.Add(new WindowGroupRowVm
            {
                Id = g.Id,
                Name = g.Name,
                Value = g.Value,
                Condition = ConditionOptions.FirstOrDefault(c => c.Index == g.ConditionType) ?? ConditionOptions[0],
            });
        }
    }

    public ObservableCollection<WindowGroupRowVm> Rows { get; } = [];

    /// <summary>行模板内条件下拉的候选源 (静态目录从对话框根取)。</summary>
    public IReadOnlyList<ConditionOption> ConditionOptionsView => ConditionOptions;

    /// <summary>注意事项 (label:612)。</summary>
    public string Tip => I18n.T("612");

    /// <summary>语言切换时由视图调用, 刷新提示文案。</summary>
    public void RefreshTip() => OnPropertyChanged(nameof(Tip));

    /// <summary>语言切换刻度 (对话框内 ConverterParameter 文案重算)。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>添加一行 (复刻 addItem: id = 行数 + 1, 条件默认 1)。</summary>
    [RelayCommand]
    private void AddRow() => Rows.Add(new WindowGroupRowVm
    {
        Id = Rows.Count + 1,
        Condition = ConditionOptions[0],
    });

    /// <summary>窗口侦探 (复刻 otherActions 里的 label:309 -> POST /server/command/2)。</summary>
    [RelayCommand]
    private async Task RunWindowSpyAsync()
    {
        var api = _main.Session.Api;
        if (api is null) return;
        var resp = await api.SendServerCommandAsync(2);
        if (!resp.Success)
        {
            _main.ShowMessage(I18n.T("309"), resp.ErrorMessage ?? "command 2 failed");
        }
    }

    /// <summary>已点击保存 (窗口据此关闭)。</summary>
    public bool Saved { get; private set; }

    /// <summary>保存 (复刻 @save=save: 整体替换 options.windowGroups)。</summary>
    [RelayCommand]
    private void Save()
    {
        var config = _main.Config;
        if (config is null) return;
        config.Options.WindowGroups = Rows.Select(r => new WindowGroup
        {
            Id = r.Id,
            Name = r.Name,
            Value = r.Value,
            ConditionType = r.Condition?.Index ?? 1,
        }).ToList();
        Saved = true;
    }
}
