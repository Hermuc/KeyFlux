using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>页面级语言刷新契约: 全局语言切换时由 MainViewModel 统一分发。</summary>
public interface ILanguageRefresh
{
    void OnLanguageChanged();
}

/// <summary>
/// 键位图系页面 (Keymap 矩阵 / CustomHotkey / Abbr) 的共享编辑核心。
/// 逐项复刻 config-ui/src/store/config.ts 的选中态逻辑:
///   - SelectedHotkey + SelectedWindowGroupId -> 解析(惰性初始化)当前 Action (复刻 _getAction,
///     含新增 keymap 首次选中 singlePress 的默认动作);
///   - 换键/删键/加键 (复刻 changeHotkey / removeHotkey / addHotKey);
///   - 禁用键集合 (复刻 _disabledKeys: 自身触发键 + 子模式触发键);
///   - 备注汇总 (复刻 ActionCommentTable 的分组前缀 + 翻译拼接)。
/// 所有编辑直接改内存 Config, 保存统一走 MainViewModel.SaveAsync (ConfigSaver 清洗)。
/// </summary>
public sealed partial class KeymapEditorCore : ObservableObject
{
    private readonly MainViewModel _main;

    public KeymapEditorCore(MainViewModel main, Keymap keymap)
    {
        _main = main;
        Keymap = keymap;
        Editor = new ActionEditorViewModel(this);
    }

    public MainViewModel Main => _main;
    public Config Config => _main.Config ?? throw new InvalidOperationException("Config 未加载");
    public Keymap Keymap { get; }
    public ActionEditorViewModel Editor { get; }
    private ISettingsApi? Api => _main.Session.Api;

    /// <summary>Abbr 语境 (复刻 keymap.hotkey.includes("Abbr")): 影响动作类型与备选项过滤。</summary>
    public bool IsAbbr => Keymap.Hotkey.Contains("Abbr", StringComparison.Ordinal);

    // ------------------------------------------------------------- 选中态

    /// <summary>当前点选的键 ("" = 未选中; 复刻 store.hotkey)。</summary>
    [ObservableProperty]
    private string _selectedHotkey = "";

    /// <summary>当前窗口分组 (复刻 store.windowGroupID; 切换后按组重新解析动作)。</summary>
    [ObservableProperty]
    private int _selectedWindowGroupId;

    /// <summary>键绑定/备注等数据变化 (页面据此刷新格子状态、备注表、行表)。</summary>
    public event System.Action? HotkeyDataChanged;

    partial void OnSelectedHotkeyChanged(string value) => Rebind();
    partial void OnSelectedWindowGroupIdChanged(int value) => Rebind();

    /// <summary>点选键 (复刻 Key.vue click; 禁用键不可点)。</summary>
    public void SelectKey(string hotkey)
    {
        if (string.IsNullOrEmpty(hotkey) || IsDisabledKey(hotkey)) return;
        SelectedHotkey = hotkey;
    }

    private void Rebind()
    {
        Editor.BindTo(ResolveCurrentAction());
        HotkeyDataChanged?.Invoke();
    }

    /// <summary>对外通知数据变化 (编辑字段导致绑定状态/备注变化时由编辑器调用)。</summary>
    public void NotifyDataChanged() => HotkeyDataChanged?.Invoke();

    /// <summary>
    /// 复刻 _getAction: 按 (hotkey, windowGroupID) 解析动作, 不存在则惰性初始化
    /// (新 keymap 首次选中 singlePress 时填入「{blind}{触发键}」默认动作)。
    /// </summary>
    public Models.Action? ResolveCurrentAction()
    {
        var hk = SelectedHotkey;
        if (string.IsNullOrEmpty(hk)) return null;

        if (!Keymap.Hotkeys.TryGetValue(hk, out var actions))
        {
            actions = [];
            Keymap.Hotkeys[hk] = actions;
            if (Keymap.IsNew && hk == "singlePress")
            {
                // 复刻: 新增的模式, 让它的 singlePress 默认为输入触发键
                var key = Keymap.Hotkey.TrimStart(' ', '#', '!', '^', '+', '<', '>', '*', '~', '$');
                actions.Add(new Models.Action
                {
                    WindowGroupId = 0,
                    TypeId = 6,
                    IsEmpty = false,
                    KeysToSend = "{blind}{" + key + "}",
                    Comment = "输入 " + key + " 键",
                });
            }
        }

        var found = actions.Find(a => a.WindowGroupId == SelectedWindowGroupId);
        if (found is null)
        {
            found = new Models.Action { WindowGroupId = SelectedWindowGroupId, TypeId = 0, IsEmpty = true };
            actions.Add(found);
            actions.Sort((a, b) => a.WindowGroupId.CompareTo(b.WindowGroupId));
        }
        return found;
    }

    // ------------------------------------------------------------- 禁用键

    /// <summary>
    /// 复刻 _disabledKeys: 触发键在「自身模式」与「其父模式」的格子中禁用
    /// (避免把进入模式的键再绑定动作)。查询键含 "*" 前缀、全小写。
    /// </summary>
    public bool IsDisabledKey(string hotkey)
    {
        if (!DisabledKeymaps.TryGetValue(Keymap.Id, out var set)) return false;
        return set.Contains(hotkey.ToLowerInvariant());
    }

    private Dictionary<int, HashSet<string>> DisabledKeymaps
    {
        get
        {
            var m = new Dictionary<int, HashSet<string>>();
            HashSet<string> Set(int id)
            {
                if (!m.TryGetValue(id, out var s)) m[id] = s = [];
                return s;
            }
            foreach (var km in Config.Keymaps.Where(k => k.Enable))
            {
                var hk = km.Hotkey.ToLowerInvariant();
                Set(km.Id).Add(hk);
                Set(km.Id).Add("*" + hk);
                Set(km.ParentId).Add(hk);
                Set(km.ParentId).Add("*" + hk);
            }
            return m;
        }
    }

    // ------------------------------------------------------------- 换键/删键/加键

    /// <summary>
    /// 复刻 changeHotkey: 目标键已存在时 —— 当前键未配置(首动作 typeID=0)则删当前键保留目标键,
    /// 否则删目标键后把当前键改名过去 (重建字典保持插入顺序)。
    /// </summary>
    public string ChangeHotkey(string oldHotkey, string newHotkey)
    {
        var hotkeys = Keymap.Hotkeys;
        if (string.IsNullOrEmpty(oldHotkey) || !hotkeys.ContainsKey(oldHotkey)) return newHotkey;

        if (hotkeys.ContainsKey(newHotkey) && newHotkey != oldHotkey)
        {
            var oldActions = hotkeys[oldHotkey];
            if (oldActions.Count > 0 && oldActions[0].TypeId == 0)
            {
                RemoveHotkey(oldHotkey);
                return newHotkey;
            }
            RemoveHotkey(newHotkey);
        }
        else if (hotkeys.ContainsKey(newHotkey)) // old == new: 与 Vue 同路径 (未配置则删自身)
        {
            var oldActions = hotkeys[oldHotkey];
            if (oldActions.Count > 0 && oldActions[0].TypeId == 0)
            {
                RemoveHotkey(oldHotkey);
                return newHotkey;
            }
            return newHotkey;
        }

        var rebuilt = new Dictionary<string, List<Models.Action>>();
        foreach (var (k, v) in hotkeys)
        {
            rebuilt[k == oldHotkey ? newHotkey : k] = v;
        }
        Keymap.Hotkeys = rebuilt;
        HotkeyDataChanged?.Invoke();
        return newHotkey;
    }

    /// <summary>复刻 removeHotkey。</summary>
    public void RemoveHotkey(string hotkey)
    {
        if (Keymap.Hotkeys.Remove(hotkey))
        {
            if (SelectedHotkey == hotkey)
            {
                SelectedHotkey = "";
            }
            HotkeyDataChanged?.Invoke();
        }
    }

    /// <summary>复刻 addHotKey: 新增一个空热键条目 (默认空串)。</summary>
    public void AddHotKey(string key = "")
    {
        Keymap.Hotkeys[key] = [new Models.Action { TypeId = 0, IsEmpty = true }];
        HotkeyDataChanged?.Invoke();
    }

    // ------------------------------------------------------------- 备注汇总

    /// <summary>
    /// 复刻 ActionCommentTable.getActionAllComment: 每键的备注 = 各非空动作
    /// 「分组名: 翻译后的备注」按行拼接 (分组 0 无前缀)。条目按备注排序, 空备注剔除。
    /// </summary>
    public IReadOnlyList<(string Hotkey, string Comment)> BuildCommentEntries()
    {
        var list = new List<(string Hotkey, string Comment)>();
        foreach (var (hk, actions) in Keymap.Hotkeys)
        {
            var sb = new System.Text.StringBuilder();
            foreach (var act in actions)
            {
                if (string.IsNullOrEmpty(act.Comment)) continue;
                var prefix = act.WindowGroupId == 0
                    ? ""
                    : Config.Options.WindowGroups.Find(w => w.Id == act.WindowGroupId)?.Name + ": ";
                sb.Append(prefix).Append(I18n.T(act.Comment)).Append("\r\n");
            }
            var comment = sb.ToString().TrimEnd('\n', '\r');
            if (comment.Length > 0) list.Add((hk, comment));
        }
        list.Sort((a, b) => string.Compare(a.Comment, b.Comment, StringComparison.CurrentCulture));
        return list;
    }

    // ------------------------------------------------------------- 缩写启用联动

    /// <summary>
    /// 复刻 store 的 watch: 类型 9 与取值 5/6 相互变化时重算缩写/命令 keymap 的启用状态。
    /// </summary>
    public void MaybeRefreshAbbrEnable(int oldTypeId, int newTypeId, int oldValueId, int newValueId)
    {
        var typeInvolved = oldTypeId == 9 || newTypeId == 9;
        var valueInvolved = oldValueId is 5 or 6 || newValueId is 5 or 6;
        if (!typeInvolved || !valueInvolved) return;
        ConfigActions.ChangeAbbrEnable(Config);
        _main.OnNavInvalidated();
    }

    /// <summary>窗口侦探: POST /server/command/2 (复刻 server.runWindowSpy)。失败时弹提示。</summary>
    public async Task RunWindowSpyAsync()
    {
        if (Api is null) return;
        var resp = await Api.SendServerCommandAsync(2);
        if (!resp.Success)
        {
            _main.ShowMessage(I18n.T("309"), resp.ErrorMessage ?? "command 2 failed");
        }
    }
}
