using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>备注汇总行 (复刻 ActionCommentTable.vue 的条目; KeyText 由页面决定格式)。</summary>
public sealed record CommentEntryVm(string KeyText, string Comment);

/// <summary>
/// 键盘格子单元格 (复刻 Key.vue): 点选键位、已绑定/禁用/选中/空键的视觉状态。
/// </summary>
public sealed partial class KeyCellVm : ObservableObject
{
    public required string Hotkey { get; init; }
    public required string Label { get; init; }

    /// <summary>格子底色 (选中 #2196F3 / 禁用 #AAAAAA / 已绑定 #98FB98 / 空键 白色)。</summary>
    [ObservableProperty]
    private string _background = "#FFFFFF";

    /// <summary>禁用键不可点 (触发键自身, 复刻 v-card :disabled)。</summary>
    [ObservableProperty]
    private bool _isEnabled = true;

    // ------------------------------------------------------------- 单字符键格正方形 (仅模式页键盘网格模板使用)
    // Label 由 KeyText 生成 (去 "*" 前缀 + 首字母大写): 恰好 1 字符即为单字符键 (A/, 等),
    // 多字符键 (F1/Space/Enter...) 保持自然宽度不受影响。

    /// <summary>是否单字符键格 (Label 长度 == 1)。</summary>
    public bool IsSingleChar => Label.Length == 1;

    /// <summary>键格宽度: 单字符 43 (与高度 43 成正方形); 其余 NaN 保持自然宽度。</summary>
    public double CellWidth => IsSingleChar ? 43 : double.NaN;

    /// <summary>最小宽度: 单字符与宽度同为 43 (与 CellWidth 一致, 防止 MinWidth 大于 Width 产生歧义); 其余 58。</summary>
    public double CellMinWidth => IsSingleChar ? 43 : 58;

    /// <summary>内边距: 正方形格清零, 保证大字号单字符在 43×43 内居中不裁切; 其余保持 10,0。</summary>
    public Avalonia.Thickness CellPadding => IsSingleChar ? new Avalonia.Thickness(0) : new Avalonia.Thickness(10, 0);
}

/// <summary>键盘一行 (包装键格列表, 便于 AXAML DataTemplate 声明类型)。</summary>
public sealed class KeyboardRowVm
{
    public required List<KeyCellVm> Keys { get; init; }
}

/// <summary>
/// 键位图页 (复刻 views/Keymap.vue, 路由 keymap/:id, id>4 含子模式):
/// 物理键盘网格 (parseKeyboardLayout) + 动作编辑面板 + 右侧备注汇总。
/// </summary>
public sealed partial class KeymapPageViewModel : ObservableObject, ILanguageRefresh
{
    public KeymapPageViewModel(MainViewModel main, Keymap keymap)
    {
        Core = new KeymapEditorCore(main, keymap);

        var rows = ConfigSaver.ParseKeyboardLayout(main.Config!.Options.KeyboardLayout, keymap.Hotkey);
        Rows = rows
            .Select(r => new KeyboardRowVm
            {
                Keys = r.Select(k => new KeyCellVm { Hotkey = k, Label = KeyText(k) }).ToList(),
            })
            .ToList();
        SmallFont = Rows.Count > 0 && Rows[0].Keys.Count > 10;

        Core.HotkeyDataChanged += OnDataChanged;
        OnDataChanged();
    }

    public KeymapEditorCore Core { get; }
    public Keymap Keymap => Core.Keymap;
    public ActionEditorViewModel Editor => Core.Editor;

    /// <summary>键盘行 (每行一格键, 渲染为横向格子)。</summary>
    public List<KeyboardRowVm> Rows { get; }

    /// <summary>复刻 .small 类: 首行超过 10 键 (如 104 键布局) 时整体缩小字号。</summary>
    public bool SmallFont { get; }

    /// <summary>键格字号: 复刻 font-size 1.5rem 与 .small 的 1.23rem; 整体缩小 4 号 (24→20, 19.7→15.7)。</summary>
    public double KeyFontSize => SmallFont ? 15.7 : 20;

    /// <summary>语言切换计数 (页内 ConverterParameter 文案重算)。</summary>
    [ObservableProperty]
    private int _languageTick;

    // ------------------------------------------------------------- 子模式展示

    /// <summary>页头标题: keymap 名称 (无名称时显示触发键)。</summary>
    public string HeaderTitle =>
        string.IsNullOrEmpty(Keymap.Name) ? Keymap.Hotkey : Keymap.Name;

    /// <summary>子模式信息: 上层模式名称 (label:503); 非子模式返回 null。</summary>
    public string? ParentInfo
    {
        get
        {
            if (Keymap.ParentId == 0) return null;
            var parent = Core.Config.Keymaps.FirstOrDefault(k => k.Id == Keymap.ParentId);
            var name = parent is null ? "" : string.IsNullOrEmpty(parent.Name) ? parent.Hotkey : parent.Name;
            return $"{I18n.T("503")}: {name}";
        }
    }

    // ------------------------------------------------------------- 备注汇总

    public ObservableCollection<CommentEntryVm> CommentEntries { get; } = [];

    /// <summary>上次备注快照 (KeyText, Comment), 与目标一致时跳过重建 (防备注列滚动跳变)。</summary>
    private List<(string, string)> _lastCommentSnapshot = [];

    private void OnDataChanged()
    {
        RefreshCells();
        RefreshCommentEntries();
    }

    private void RefreshCommentEntries()
    {
        // 先构建目标快照: 与上次逐项一致则直接返回, 避免集合重建导致 ItemsControl 重置滚动位置
        var snapshot = Core.BuildCommentEntries()
            .Select(e => (KeyText(e.Hotkey), e.Comment))
            .ToList();
        if (snapshot.SequenceEqual(_lastCommentSnapshot)) return;

        _lastCommentSnapshot = snapshot;
        CommentEntries.Clear();
        foreach (var (keyText, comment) in snapshot)
        {
            CommentEntries.Add(new CommentEntryVm(keyText, comment));
        }
    }

    // ------------------------------------------------------------- 格子状态

    /// <summary>
    /// 复刻 Key.vue keyColor/disabled 计算:
    /// 选中=蓝; 禁用键=灰且不可点; 已绑定=绿 (Abbr 键除外); 其余白色。
    /// </summary>
    private void RefreshCells()
    {
        var abbr = Core.IsAbbr;
        foreach (var row in Rows)
        {
            foreach (var cell in row.Keys)
            {
                var disabled = Core.IsDisabledKey(cell.Hotkey);
                var selected = cell.Hotkey == Core.SelectedHotkey;
                cell.IsEnabled = !disabled;
                cell.Background = selected ? "#2196F3"
                    : disabled ? "#AAAAAA"
                    : !abbr && IsBound(cell.Hotkey) ? "#98FB98"
                    : "#FFFFFF";
            }
        }
    }

    /// <summary>复刻 !getAction(hotkey).isEmpty (按当前窗口分组判定, 只读不改模型)。</summary>
    private bool IsBound(string hotkey)
        => Keymap.Hotkeys.TryGetValue(hotkey, out var list)
           && list.Exists(a => a.WindowGroupId == Core.SelectedWindowGroupId && !a.IsEmpty);

    /// <summary>点选格子 (复刻 Key.vue click)。</summary>
    [RelayCommand]
    private void SelectKey(KeyCellVm cell) => Core.SelectKey(cell.Hotkey);

    /// <summary>复刻 getKeyText: 去掉 "*" 前缀并首字母大写。
    /// 用户圈定的非键帽词与常用键名显示中文 (网格不随语言切换重建, 故硬编码);
    /// 其余键帽词 (alt/ctrl/tab/pgup...) 保持原样不动。</summary>
    public static string KeyText(string hotkey)
    {
        var s = hotkey.TrimStart('*');
        var cn = s.ToLowerInvariant() switch
        {
            "space" => "空格",
            "enter" => "回车",
            "backspace" => "退格",
            "singlepress" => "短按",
            "LButton" => "左键",
            "MButton" => "中键",
            "RButton" => "右键",
            "WheelUp" => "滚轮上",
            "WheelDown" => "滚轮下",
            "XButton1" => "侧键1",
            "XButton2" => "侧键2",
            _ => null,
        };
        if (cn != null) return cn;
        return s.Length == 0 ? s : char.ToUpperInvariant(s[0]) + s[1..];
    }

    // ------------------------------------------------------------- 语言刷新

    public void OnLanguageChanged()
    {
        LanguageTick++;
        Editor.OnLanguageChanged();
        RefreshCommentEntries();
        OnPropertyChanged(nameof(ParentInfo));
    }
}
