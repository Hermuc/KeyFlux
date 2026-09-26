using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 缩写命令页 (keymap/2 Command、keymap/3 Abbreviation, 复刻 views/Abbr.vue):
/// 缩写条目 chips (复刻 Key 组件, 无绿色绑定态) + 命令框 (del/rn 指令) +
/// 动作编辑面板 + 右侧备注汇总。
/// </summary>
public sealed partial class AbbrPageViewModel : ObservableObject, ILanguageRefresh
{
    public AbbrPageViewModel(MainViewModel main, Keymap keymap)
    {
        Core = new KeymapEditorCore(main, keymap);
        Core.HotkeyDataChanged += OnDataChanged;
        OnDataChanged();
    }

    public KeymapEditorCore Core { get; }
    public ActionEditorViewModel Editor => Core.Editor;

    /// <summary>语言切换计数 (页内 ConverterParameter 文案重算)。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>缩写条目 (每条一个格子按钮)。</summary>
    public ObservableCollection<KeyCellVm> Chips { get; } = [];

    /// <summary>备注汇总 (keyText 用 formatSpace 格式化)。</summary>
    public ObservableCollection<CommentEntryVm> CommentEntries { get; } = [];

    /// <summary>命令输入框 (label:406 提示; 回车执行)。</summary>
    [ObservableProperty]
    private string _cmdText = "";

    private void OnDataChanged()
    {
        RefreshChips();
        RefreshComments();
    }

    /// <summary>复刻 Abbr.vue 的 Key 列表: 选中=蓝, 其余白色 (Abbr 无绿色绑定态)。</summary>
    private void RefreshChips()
    {
        Chips.Clear();
        foreach (var hotkey in Core.Keymap.Hotkeys.Keys)
        {
            Chips.Add(new KeyCellVm
            {
                Hotkey = hotkey,
                Label = FormatSpace(hotkey),
                Background = hotkey == Core.SelectedHotkey ? ClaudePalette.Sand : ClaudePalette.Ivory,
                IsEnabled = !Core.IsDisabledKey(hotkey),
            });
        }
    }

    /// <summary>点选缩写条目 (复刻 Key.vue click)。</summary>
    [RelayCommand]
    private void SelectChip(KeyCellVm chip) => Core.SelectKey(chip.Hotkey);

    /// <summary>
    /// 复刻 runCmd: 小写化后按前缀分发 —— "del ab" 删除; "rn cd" 把当前选中改名为 cd;
    /// 其余视为选中 (不存在时由编辑核心惰性创建)。执行后清空命令框。
    /// </summary>
    [RelayCommand]
    private void RunCmd()
    {
        var raw = CmdText;
        var cmdStr = raw.ToLowerInvariant();
        if (string.IsNullOrWhiteSpace(cmdStr)) return;

        var isDel = cmdStr.StartsWith("del ", StringComparison.Ordinal);
        if (isDel)
        {
            Core.RemoveHotkey(cmdStr["del ".Length..]);
        }
        else if (cmdStr.StartsWith("rn ", StringComparison.Ordinal))
        {
            cmdStr = cmdStr["rn ".Length..];
            Core.ChangeHotkey(Core.SelectedHotkey, cmdStr);
        }
        Core.SelectedHotkey = isDel ? "" : cmdStr;
        CmdText = "";
    }

    /// <summary>上次备注快照 (KeyText, Comment), 与目标一致时跳过重建 (防备注列滚动跳变)。</summary>
    private List<(string, string)> _lastCommentSnapshot = [];

    private void RefreshComments()
    {
        // 先构建目标快照: 与上次逐项一致则直接返回, 避免集合重建导致 ItemsControl 重置滚动位置
        var snapshot = Core.BuildCommentEntries()
            .Select(e => (FormatSpace(e.Hotkey), e.Comment))
            .ToList();
        if (snapshot.SequenceEqual(_lastCommentSnapshot)) return;

        _lastCommentSnapshot = snapshot;
        CommentEntries.Clear();
        foreach (var (keyText, comment) in snapshot)
        {
            CommentEntries.Add(new CommentEntryVm(keyText, comment));
        }
    }

    /// <summary>复刻 formatSpace: 尾部空格逐个显示为 ◻️。</summary>
    public static string FormatSpace(string hotkey)
    {
        var trimmed = hotkey.TrimEnd(' ');
        return trimmed + string.Concat(Enumerable.Repeat("◻️", hotkey.Length - trimmed.Length));
    }

    public void OnLanguageChanged()
    {
        LanguageTick++;
        Editor.OnLanguageChanged();
        RefreshComments();
    }
}
