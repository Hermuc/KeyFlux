using Avalonia.Input;
using MyKeymap.Settings.Models;
// 避免与 Models.Action 歧义 (Keymap 模型引用需要 Models 命名空间)
using Action = System.Action;

namespace MyKeymap.Settings.Services;

// ============================================================================
// 热键捕获与格式化逻辑 (复刻 config-ui/src/components/action/constants.ts
// + HotkeyCapture.vue 的纯逻辑部分)。
//
// Vue 侧用浏览器 KeyboardEvent.code (ControlLeft/ControlRight 等) 区分左右
// 修饰键; Avalonia 侧用物理键枚举 (Key.LeftCtrl / Key.RightCtrl ...) 实现
// 等价逻辑 —— 比浏览器更容易区分左右。
//
// 设计: 全部纯函数 + 无 UI 依赖的状态机 (HotkeyCaptureCore), 便于单元测试。
// ============================================================================

/// <summary>静态热键工具: 物理键 -> AHK 键名、修饰前缀排序、显示格式化、冲突归一化。</summary>
public static class HotkeyLogic
{
    /// <summary>生成 AHK 热键时的修饰键固定顺序 (AHK 中修饰符顺序不影响功能, 固定顺序保证可读性; 复刻 MOD_ORDER)。</summary>
    public static readonly string[] ModOrder = ["<^", ">^", "<!", ">!", "<+", ">+", "<#", ">#"];

    /// <summary>修饰前缀 -> 捕获态显示名 (复刻 MODIFIER_CODE_MAP 的 label 列)。</summary>
    public static readonly IReadOnlyDictionary<string, string> ModPrefixLabels =
        new Dictionary<string, string>
        {
            ["<^"] = "LCtrl", [">^"] = "RCtrl",
            ["<!"] = "LAlt", [">!"] = "RAlt",
            ["<+"] = "LShift", [">+"] = "RShift",
            ["<#"] = "LWin", [">#"] = "RWin",
        };

    // AHK 修饰键前缀 -> 显示名 (复刻 constants.ts MODIFIER_MAP)
    private static readonly Dictionary<string, string> ModifierDisplayMap = new()
    {
        ["^"] = "Ctrl", ["!"] = "Alt", ["+"] = "Shift", ["#"] = "Win",
        ["<^"] = "LCtrl", ["<!"] = "LAlt", ["<+"] = "LShift", ["<#"] = "LWin",
        [">^"] = "RCtrl", [">!"] = "RAlt", [">+"] = "RShift", [">#"] = "RWin",
    };

    // AHK 键名 -> 显示名 (复刻 keyToDisplay)
    private static readonly Dictionary<string, string> KeyDisplayMap =
        new(StringComparer.OrdinalIgnoreCase)
        {
            ["space"] = "Space", ["esc"] = "Esc", ["enter"] = "Enter", ["tab"] = "Tab",
            ["up"] = "↑", ["down"] = "↓", ["left"] = "←", ["right"] = "→",
            ["pgup"] = "PageUp", ["pgdn"] = "PageDown",
            ["home"] = "Home", ["end"] = "End", ["ins"] = "Insert", ["del"] = "Delete",
            ["backspace"] = "Backspace", ["apps"] = "Menu",
            // 单修饰键热键 (2026-09: 捕获支持单按 Ctrl/Alt 等) 的键名显示
            ["ctrl"] = "Ctrl", ["alt"] = "Alt", ["shift"] = "Shift",
            ["lwin"] = "LWin", ["rwin"] = "RWin",
        };

    /// <summary>
    /// 物理修饰键 -> (AHK 前缀, 显示名); 非修饰键返回 null。
    /// 复刻 MODIFIER_CODE_MAP: 左右侧各自独立前缀 (&lt;^/&gt;^ ...)。
    /// </summary>
    public static (string Prefix, string Label)? ModifierFor(Key key) => key switch
    {
        Key.LeftCtrl => ("<^", "LCtrl"),
        Key.RightCtrl => (">^", "RCtrl"),
        Key.LeftAlt => ("<!", "LAlt"),
        Key.RightAlt => (">!", "RAlt"),
        Key.LeftShift => ("<+", "LShift"),
        Key.RightShift => (">+", "RShift"),
        Key.LWin => ("<#", "LWin"),
        Key.RWin => (">#", "RWin"),
        _ => null,
    };

    /// <summary>是否修饰键 (捕获时修饰键暂存等待主键, 不单独成热键)。</summary>
    public static bool IsModifier(Key key) => ModifierFor(key) is not null;

    /// <summary>
    /// 物理键 -> AHK 键名 (复刻 keyToAhkName): 字母数字/功能键/常用符号;
    /// 不支持的键 (如反引号 OemTilde —— AHK Hotkey("``") 报 Invalid hotkey) 返回 null。
    /// </summary>
    public static string? KeyToAhkName(Key key)
    {
        if (key >= Key.A && key <= Key.Z) return key.ToString().ToLowerInvariant();
        if (key >= Key.D0 && key <= Key.D9) return ((char)('0' + (key - Key.D0))).ToString();
        if (key >= Key.NumPad0 && key <= Key.NumPad9) return ((char)('0' + (key - Key.NumPad0))).ToString();
        if (key >= Key.F1 && key <= Key.F24) return $"F{key - Key.F1 + 1}";

        return key switch
        {
            Key.Space => "space",
            Key.Escape => "esc",
            Key.Enter => "enter",
            Key.Tab => "tab",
            Key.Up => "up",
            Key.Down => "down",
            Key.Left => "left",
            Key.Right => "right",
            Key.Home => "home",
            Key.End => "end",
            Key.PageUp => "pgup",
            Key.PageDown => "pgdn",
            Key.Insert => "ins",
            Key.Delete => "del",
            Key.Back => "backspace",
            // 主键盘符号键 (与 Vue keyToAhkName 的符号映射一致)
            Key.OemMinus => "-",
            Key.OemPlus => "=",
            Key.OemOpenBrackets => "[",
            Key.OemCloseBrackets => "]",
            Key.OemBackslash => "\\",
            Key.OemSemicolon => ";",
            Key.OemQuotes => "'",
            Key.OemComma => ",",
            Key.OemPeriod => ".",
            Key.OemQuestion => "/",
            _ => null,
        };
    }

    /// <summary>
    /// 修饰键前缀列表 + 主键 -> AHK 格式热键 (复刻 buildAhkFromCodes):
    /// 前缀按 <see cref="ModOrder"/> 固定排序后拼接, 如 ["&gt;^","&lt;+"] + "q" -> "&lt;+&gt;^q"。
    /// </summary>
    public static string BuildAhk(IEnumerable<string> prefixes, string mainKey)
    {
        var ordered = prefixes
            .Where(p => Array.IndexOf(ModOrder, p) >= 0)
            .Distinct()
            .OrderBy(p => Array.IndexOf(ModOrder, p));
        return string.Concat(ordered) + mainKey;
    }

    /// <summary>
    /// 侧别归一化: 把左右侧别前缀 (&lt;^/&gt;^/&lt;!/&gt;!/&lt;+/&gt;+/&lt;#/&gt;#)
    /// 映射为通配两侧的单字符前缀 (^/!/+/#), 如 "&gt;^p" -&gt; "^p"、"&lt;^&gt;+k" -&gt; "^+k"。
    /// 背景: AHK 中带侧别前缀的热键只响应对应物理侧 —— 设置界面录制 "RCtrl+P" 后,
    /// 用户合理期望左 Ctrl+P 也能触发; 捕获提交时统一通配两侧, 与侧别无关的冲突
    /// 检测 (<see cref="NormalizeHotkey"/>) 语义保持一致。确需区分左右侧的进阶场景
    /// 可后续手改 config.json (AHK 语法 &lt;^/&gt;^ 依然合法)。
    /// </summary>
    public static string NormalizeSidePrefixes(string ahk)
    {
        if (string.IsNullOrEmpty(ahk) || ahk.Length < 2) return ahk ?? "";
        var sb = new System.Text.StringBuilder(ahk.Length);
        for (var i = 0; i < ahk.Length;)
        {
            // 两字符侧别前缀 (按 ModOrder 识别): 丢弃侧别标记, 保留修饰符本体
            if (i + 1 < ahk.Length && Array.IndexOf(ModOrder, ahk.Substring(i, 2)) >= 0)
            {
                sb.Append(ahk[i + 1]);
                i += 2;
            }
            else
            {
                sb.Append(ahk[i]);
                i++;
            }
        }
        return sb.ToString();
    }

    /// <summary>AHK 格式热键 -> 可读格式 (复刻 ahkToDisplay): ^+q -> Ctrl+Shift+Q, &lt;^q -> LCtrl+Q。</summary>
    public static string AhkToDisplay(string? ahk)
    {
        if (string.IsNullOrEmpty(ahk)) return "";
        // 自定义组合 (j & k): 各段独立转可读名后以 " + " 连接
        if (ahk.Contains(" & "))
            return string.Join(" + ", ahk.Split(" & ").Select(p => AhkToDisplay(p)));
        var parts = new List<string>();
        var rest = ahk;
        // 依次提取修饰键前缀 (先两字符左右前缀, 再单字符前缀)
        while (rest.Length >= 2 && ModifierDisplayMap.TryGetValue(rest[..2], out var two))
        {
            parts.Add(two);
            rest = rest[2..];
        }
        while (rest.Length >= 1 && ModifierDisplayMap.TryGetValue(rest[..1], out var one))
        {
            parts.Add(one);
            rest = rest[1..];
        }
        // 剩余部分: 去掉 * ~ $ 等前缀
        rest = rest.TrimStart('*', '~', '$');
        if (rest.Length > 0) parts.Add(KeyToDisplay(rest));
        return string.Join("+", parts);
    }

    private static string KeyToDisplay(string key)
        => KeyDisplayMap.TryGetValue(key, out var mapped)
            ? mapped
            : string.Concat(char.ToUpperInvariant(key[0]).ToString(), key.AsSpan(1));

    /// <summary>
    /// 单修饰键热键的 AHK 键名: Ctrl/Alt/Shift 提交中性名 (两侧均可触发, 与侧别归一化语义一致);
    /// Win 无中性键名, 保留捕获到的物理侧 (LWin/RWin)。
    /// </summary>
    public static string SingleModifierAhk((string Prefix, string Label) mod)
        => mod.Label switch
        {
            "LCtrl" or "RCtrl" => "Ctrl",
            "LAlt" or "RAlt" => "Alt",
            "LShift" or "RShift" => "Shift",
            _ => mod.Label, // LWin / RWin
        };

    /// <summary>
    /// 归一化热键用于冲突比较 (复刻 normalizeHotkey): 去掉开头的 * ~ $ 以及左右修饰前缀 &lt; &gt;, 忽略大小写。
    /// 说明: &lt;^q / &gt;^q / ^q 归一化后均为 ^q, 冲突检测采用保守策略 (宁多报不放过)。
    /// </summary>
    public static string NormalizeHotkey(string hk)
        => hk.TrimStart('*', '~', '$', '<', '>').ToLowerInvariant();

    /// <summary>
    /// 仅去除通配前缀的弱归一化 (复刻 SelectedActionEdit.vue 里对「其他方案热键」的
    /// <c>hotkey.replace(/^[*~$]+/, "").toLowerCase()</c>): 保留左右修饰前缀。
    /// </summary>
    public static string StripWildcardPrefix(string hk)
        => hk.TrimStart('*', '~', '$').ToLowerInvariant();

    /// <summary>
    /// 收集已占用的热键 (复刻 collectUsedHotkeys): 启用的 keymap 的所有绑定键与触发键,
    /// 全部经 <see cref="NormalizeHotkey"/> 归一化 (排除 "settings"/"customHotkeys" 哨兵)。
    /// </summary>
    public static HashSet<string> CollectUsedHotkeys(IEnumerable<Keymap> keymaps)
    {
        var used = new HashSet<string>();
        foreach (var km in keymaps)
        {
            if (!km.Enable) continue;
            foreach (var hk in km.Hotkeys.Keys) used.Add(NormalizeHotkey(hk));
            if (!string.IsNullOrEmpty(km.Hotkey) && km.Hotkey != "settings" && km.Hotkey != "customHotkeys")
            {
                used.Add(NormalizeHotkey(km.Hotkey));
            }
        }
        return used;
    }
}

/// <summary>
/// 热键捕获状态机 (与 UI 解耦便于单测)。2026-09 扩展三种形态:
///   - 修饰键+主键 (Ctrl+P): 按住修饰键再按主键, 立即提交 (原语义不变);
///   - 单修饰键 (Ctrl): 捕获期间只按过一个修饰键且无任何主键, 松开即提交中性键名 (Ctrl/Alt/Shift; Win 保留物理侧);
///   - 非修饰键组合 (J+K): 第一键暂存等待, 第二键提交 "j & k" (AHK 自定义组合);
///   - 单主键 (J): 暂存后按 Enter 确认提交 "j";
///   - Esc 单独按取消; Enter 在无暂存时退出; 焦点丢失由外部调用 <see cref="Cancel"/>。
/// 键位即所见: 暂存期间 DisplayText 实时回显已按部分。
/// </summary>
public sealed class HotkeyCaptureCore
{
    private readonly List<string> _pendingPrefixes = []; // 按住未松的修饰键前缀
    private readonly List<string> _stagedKeys = [];      // 已暂存的非修饰键 (0-2; AHK 自定义组合为两键)
    private readonly HashSet<string> _modsSeen = [];     // 本次捕获按过的全部修饰键 (单修饰键提交判定用)

    /// <summary>是否处于捕获态。</summary>
    public bool Capturing { get; private set; }

    /// <summary>当前按住未松的修饰键前缀 (按下顺序)。</summary>
    public IReadOnlyList<string> PendingPrefixes => _pendingPrefixes;

    /// <summary>当前暂存的非修饰键 (等待 Enter 确认单键或第二键组成组合)。</summary>
    public IReadOnlyList<string> StagedKeys => _stagedKeys;

    /// <summary>捕获态/暂存变化 (UI 据此刷新提示文本)。</summary>
    public event Action? StateChanged;

    /// <summary>成功组合出热键 (AHK 格式) 时提交。</summary>
    public event Action<string>? HotkeyCommitted;

    /// <summary>进入捕获态。</summary>
    public void StartCapture()
    {
        Capturing = true;
        _pendingPrefixes.Clear();
        _stagedKeys.Clear();
        _modsSeen.Clear();
        StateChanged?.Invoke();
    }

    /// <summary>取消捕获 (Esc 取消与 onFocusOut), 未捕获时无操作。</summary>
    public void Cancel()
    {
        if (!Capturing) return;
        Capturing = false;
        _pendingPrefixes.Clear();
        _stagedKeys.Clear();
        StateChanged?.Invoke();
    }

    /// <summary>
    /// 处理按键按下。<param name="anyModifierHeld">系统层面是否有修饰键处于按住状态 (Esc/Enter 单独按下才生效)。</param>
    /// </summary>
    public void HandleKeyDown(Key key, bool anyModifierHeld)
    {
        if (!Capturing) return;

        // Esc 单独按下: 取消捕获 (不变)
        if (key == Key.Escape && !anyModifierHeld)
        {
            Cancel();
            return;
        }

        // Enter 单独按下: 有暂存则确认提交 (单键/组合), 无暂存直接退出
        if (key == Key.Enter && !anyModifierHeld)
        {
            if (_stagedKeys.Count > 0) CommitStaged();
            else Cancel();
            return;
        }

        var mod = HotkeyLogic.ModifierFor(key);
        if (mod is not null)
        {
            _modsSeen.Add(mod.Value.Prefix);
            if (!_pendingPrefixes.Contains(mod.Value.Prefix))
            {
                _pendingPrefixes.Add(mod.Value.Prefix);
                StateChanged?.Invoke();
            }
            return;
        }

        var name = HotkeyLogic.KeyToAhkName(key);
        if (name is null) return;

        // 快路径: 修饰键按住 + 主键 -> 立即提交 (Ctrl+P, 原语义)
        if (_pendingPrefixes.Count > 0)
        {
            Commit(HotkeyLogic.NormalizeSidePrefixes(HotkeyLogic.BuildAhk(_pendingPrefixes, name)));
            return;
        }

        // 非修饰键暂存 (J+K 支持): 首键等待, 异键提交组合, 同键去重
        if (_stagedKeys.Count == 0)
        {
            _stagedKeys.Add(name);
            StateChanged?.Invoke();
            return;
        }
        if (_stagedKeys[^1] == name) return;  // 自动重复不重复暂存
        if (_stagedKeys.Count >= 2) return;   // AHK 自定义组合为两键
        _stagedKeys.Add(name);
        CommitStaged();
    }

    /// <summary>修饰键松开: 若它是本次捕获唯一的键且无任何暂存 -> 提交单修饰键热键; 否则撤销暂存继续等待。</summary>
    public void HandleKeyUp(Key key)
    {
        if (!Capturing) return;
        var mod = HotkeyLogic.ModifierFor(key);
        if (mod is null) return;
        if (!_pendingPrefixes.Remove(mod.Value.Prefix)) return;

        // 单修饰键提交仅当: 该修饰键是本次捕获唯一按过的键 (按过 Ctrl 再按 Shift 再松开
        // 不会把 Shift 误提交为单键热键 —— Ctrl+Shift+X 意图可存活)
        if (_pendingPrefixes.Count == 0 && _stagedKeys.Count == 0 && _modsSeen.Count == 1)
        {
            Commit(HotkeyLogic.SingleModifierAhk(mod.Value));
            return;
        }
        StateChanged?.Invoke();
    }

    /// <summary>捕获态提示文本: 非捕获态显示当前热键可读形式; 捕获态实时回显已按部分。</summary>
    public string DisplayText(string currentAhk, string waitingHint)
    {
        if (!Capturing) return HotkeyLogic.AhkToDisplay(currentAhk);
        var parts = new List<string>();
        parts.AddRange(_pendingPrefixes.Select(p => HotkeyLogic.ModPrefixLabels[p]));
        parts.AddRange(_stagedKeys.Select(k => HotkeyLogic.AhkToDisplay(k)));
        if (parts.Count == 0) return waitingHint;
        return string.Join(" + ", parts) + " + ...";
    }

    private void CommitStaged()
    {
        var ahk = string.Join(" & ", _stagedKeys);
        Capturing = false;
        _pendingPrefixes.Clear();
        _stagedKeys.Clear();
        HotkeyCommitted?.Invoke(ahk);
        StateChanged?.Invoke();
    }

    private void Commit(string ahk)
    {
        Capturing = false;
        _pendingPrefixes.Clear();
        _stagedKeys.Clear();
        HotkeyCommitted?.Invoke(ahk);
        StateChanged?.Invoke();
    }
}
