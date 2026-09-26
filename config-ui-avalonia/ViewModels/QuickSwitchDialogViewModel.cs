using System.Collections.ObjectModel;
using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 插件页 QuickSwitch 配置对话框 VM: 构造时从 Config.Options.QuickSwitch 深拷贝出 Draft 供编辑
/// (副本编辑, 取消不影响真源); 保存时把 Draft 逐字段写回真源并落盘 (force)。
/// 逻辑自 Settings 页「快速切换」分区迁入 (排除目录行编辑 / 清空历史一次性动作)。
/// </summary>
public sealed partial class QuickSwitchDialogViewModel : ObservableObject
{
    private readonly MainViewModel _main;

    public QuickSwitchDialogViewModel(MainViewModel main)
    {
        _main = main;
        var src = main.Config?.Options.QuickSwitch ?? new QuickSwitchOption();
        src.ExcludedPrefixes ??= [];
        Draft = new QuickSwitchOption
        {
            CollectEnabled = src.CollectEnabled,
            AutoShow = src.AutoShow,
            AutoJumpOpen = src.AutoJumpOpen,
            AutoJumpSave = src.AutoJumpSave,
            PollIntervalMs = src.PollIntervalMs,
            MaxHistory = src.MaxHistory,
            OverlayRows = src.OverlayRows,
            OverlayRowsCompact = src.OverlayRowsCompact,
            ExcludedPrefixes = new List<string>(src.ExcludedPrefixes),
        };
        BuildExcludedPrefixRows();
    }

    /// <summary>编辑中的副本 (保存前不影响真源 Config.Options.QuickSwitch)。</summary>
    public QuickSwitchOption Draft { get; }

    /// <summary>语言切换递增, 驱动 ConverterParameter 式文案绑定重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>保存成功后置 true (视图据此关闭窗口)。</summary>
    public bool Saved { get; private set; }

    /// <summary>排除目录行 (与 Draft.ExcludedPrefixes 索引对齐, 编辑即时回写副本)。</summary>
    public ObservableCollection<ExcludedPrefixRowVm> ExcludedPrefixRows { get; } = [];

    private void BuildExcludedPrefixRows()
    {
        ExcludedPrefixRows.Clear();
        var list = Draft.ExcludedPrefixes;
        for (var i = 0; i < list.Count; i++)
        {
            ExcludedPrefixRows.Add(new ExcludedPrefixRowVm(list, i));
        }
    }

    [RelayCommand]
    private void AddExcludedPrefix()
    {
        Draft.ExcludedPrefixes.Add("");
        BuildExcludedPrefixRows();
    }

    [RelayCommand]
    private void RemoveExcludedPrefix(ExcludedPrefixRowVm? row)
    {
        if (row is null || row.Index < 0 || row.Index >= Draft.ExcludedPrefixes.Count) return;
        Draft.ExcludedPrefixes.RemoveAt(row.Index);
        BuildExcludedPrefixRows();
        OnPropertyChanged(nameof(Draft));
    }

    /// <summary>
    /// 清空历史 (一次性动作, 非持久字段): 截断同部署根 data/quickswitch/history.tsv 为空文件
    /// (文件保留)。引擎在每次对话框实例切换时经 HistLoad 重读该文件, 故截断后历史立即空态。
    /// </summary>
    [RelayCommand]
    private void ClearHistory()
    {
        try
        {
            var dir = Path.GetFullPath(
                Path.Combine(_main.Session.BackendDirectory, "..", "data", "quickswitch"));
            Directory.CreateDirectory(dir);
            File.WriteAllText(Path.Combine(dir, "history.tsv"), "", new System.Text.UTF8Encoding(false));
        }
        catch (Exception ex)
        {
            _main.ShowMessage(I18n.T("2417"), ex.Message);
        }
    }

    /// <summary>
    /// 保存: 钳制 MaxHistory 下限为 1, 把 Draft 的 9 个字段逐一写回真源 Config.Options.QuickSwitch,
    /// 再调 SaveAsync(force) 落盘; 成功置 Saved 并返回 true, 失败返回 false (SaveAsync 内部已弹原因)。
    /// </summary>
    public async Task<bool> SaveAsync()
    {
        Draft.MaxHistory = Math.Max(1, Draft.MaxHistory);
        var target = _main.Config?.Options.QuickSwitch;
        if (target is null) return false;
        target.CollectEnabled = Draft.CollectEnabled;
        target.AutoShow = Draft.AutoShow;
        target.AutoJumpOpen = Draft.AutoJumpOpen;
        target.AutoJumpSave = Draft.AutoJumpSave;
        target.PollIntervalMs = Draft.PollIntervalMs;
        target.MaxHistory = Draft.MaxHistory;
        target.OverlayRows = Draft.OverlayRows;
        target.OverlayRowsCompact = Draft.OverlayRowsCompact;
        target.ExcludedPrefixes = Draft.ExcludedPrefixes;
        if (await _main.SaveAsync(force: true))
        {
            Saved = true;
            return true;
        }
        return false;
    }
}

/// <summary>
/// 「排除目录」表行: 包装 Options.QuickSwitch.ExcludedPrefixes[index],
/// Value 编辑即时回写底层字符串列表 (列表本身不可观测, 故用行 VM 驱动 UI 刷新)。
/// </summary>
public sealed partial class ExcludedPrefixRowVm : ObservableObject
{
    private readonly List<string> _backing;

    public ExcludedPrefixRowVm(List<string> backing, int index)
    {
        _backing = backing;
        Index = index;
    }

    public int Index { get; }

    public string Value
    {
        get => Index >= 0 && Index < _backing.Count ? _backing[Index] : "";
        set
        {
            if (Index < 0 || Index >= _backing.Count || _backing[Index] == value) return;
            _backing[Index] = value;
            OnPropertyChanged();
        }
    }
}
