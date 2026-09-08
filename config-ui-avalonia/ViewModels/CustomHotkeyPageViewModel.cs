using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 自定义热键行 (复刻 CustomHotkey.vue 表格行): 热键可编辑 (失焦提交) + 备注展示。
/// </summary>
public sealed partial class CustomHotkeyRowVm : ObservableObject
{
    public CustomHotkeyRowVm(string hotkey, string comment)
    {
        _hotkey = hotkey;
        OriginalKey = hotkey;
        _comment = comment;
    }

    /// <summary>构建行时的原始键名 (提交改名时用于定位 hotkeys 字典条目)。</summary>
    public string OriginalKey { get; }

    /// <summary>热键文本 (复刻 v-text-field @change: 失焦时提交)。</summary>
    [ObservableProperty]
    private string _hotkey;

    /// <summary>当前窗口分组下首个非空动作的备注 (复刻 getActionComment, 已翻译)。</summary>
    [ObservableProperty]
    private string _comment;

    /// <summary>是否当前选中行 (复刻 tr :class="currHotkey == hotkey")。</summary>
    [ObservableProperty]
    private bool _isSelected;

    partial void OnIsSelectedChanged(bool value) => OnPropertyChanged(nameof(RowBackground));

    /// <summary>行底色: 选中=浅蓝 (复刻 bg-blue-lighten-4), 其余透明。</summary>
    public string RowBackground => IsSelected ? "#DCEBFF" : "Transparent";
}

/// <summary>
/// 全局自定义热键分区 (keymap/1, 复刻 views/CustomHotkey.vue; 2026-09-08 自独立页迁入
/// 设置页「其他设置」卡片): 行表 (热键/备注/删除) + 新增; 动作编辑经 ActionEditorWindow
/// 弹窗承载本类 Editor (原页右侧内嵌面板)。类名保留 Page 字样以兼容历史命名。
/// </summary>
public sealed partial class CustomHotkeyPageViewModel : ObservableObject, ILanguageRefresh
{
    public CustomHotkeyPageViewModel(MainViewModel main, Keymap keymap)
    {
        Core = new KeymapEditorCore(main, keymap);
        Core.HotkeyDataChanged += RebuildRows;
        RebuildRows();
    }

    public KeymapEditorCore Core { get; }
    public ActionEditorViewModel Editor => Core.Editor;

    /// <summary>语言切换计数 (页内 ConverterParameter 文案重算)。</summary>
    [ObservableProperty]
    private int _languageTick;

    public ObservableCollection<CustomHotkeyRowVm> Rows { get; } = [];

    private void RebuildRows()
    {
        Rows.Clear();
        var groupId = Core.SelectedWindowGroupId;
        foreach (var (hotkey, actions) in Core.Keymap.Hotkeys)
        {
            // 复刻 getActionComment: 找当前分组下首个非空动作的备注
            var comment = actions.Find(a => !a.IsEmpty && a.WindowGroupId == groupId)?.Comment ?? "";
            Rows.Add(new CustomHotkeyRowVm(hotkey, I18n.T(comment))
            {
                IsSelected = hotkey == Core.SelectedHotkey,
            });
        }
    }

    /// <summary>行点击选中 (复刻 checkRow(hotkey))。</summary>
    public void SelectRow(CustomHotkeyRowVm row) => Core.SelectedHotkey = row.OriginalKey;

    /// <summary>
    /// 热键编辑提交 (复刻 @change=changeCustomHotkey): 改名后选中新键
    /// (目标已存在时的冲突处理在 core.ChangeHotkey 内)。
    /// </summary>
    public void CommitRow(CustomHotkeyRowVm row)
    {
        if (row.Hotkey == row.OriginalKey) return;
        var result = Core.ChangeHotkey(row.OriginalKey, row.Hotkey);
        Core.SelectedHotkey = result;
    }

    /// <summary>删除行 (复刻 removeCustomHotkey)。</summary>
    [RelayCommand]
    private void RemoveRow(CustomHotkeyRowVm row) => Core.RemoveHotkey(row.OriginalKey);

    /// <summary>新增一个 (复刻 addHotKey, Vue 不自动选中)。</summary>
    [RelayCommand]
    private void AddRow() => Core.AddHotKey();

    public void OnLanguageChanged()
    {
        LanguageTick++;
        Editor.OnLanguageChanged();
        RebuildRows();
    }
}
