using System.Text.Json;
using KeyFlux.Settings.Models;

namespace KeyFlux.Settings.Services;

// ============================================================================
// 保存链路 (复刻 config-ui/src/store/config.ts):
//   parseKeyboardLayout  -> 按键盘布局推导某 keymap 的合法键集合
//   CleanForSave         -> _saveConfig 的空键清洗: 剔除 isEmpty 动作,
//                           自定义 keymap (id>4) 额外剔除不在布局键集合内的键
//
// 语义对齐要点:
//   - 清洗在「深拷贝出的载荷」上执行, 绝不动用户正在编辑的内存模型;
//   - isEmpty 是前端哨兵 (Action.IsEmpty, [JsonIgnore]), JSON 深拷贝会丢失,
//     因此用「原模型按下标对齐」决定载荷中保留哪些动作;
//   - 键集合匹配与 JS Set.has 一致: 精确大小写敏感。
// ============================================================================

public static class ConfigSaver
{
    /// <summary>
    /// 复刻 parseKeyboardLayout(layout, keymapHotkey):
    /// 布局每行拆为按键列表, 除 singlePress 外均加 "*" 前缀;
    /// 鼠标类 keymap (hotkey 含 "button") 追加缺失的鼠标按钮行。
    /// </summary>
    public static List<List<string>> ParseKeyboardLayout(string layout, string keymapHotkey)
    {
        var rows = new List<List<string>>();
        foreach (var rawLine in layout.Split('\n'))
        {
            if (string.IsNullOrWhiteSpace(rawLine)) continue;
            var keys = rawLine
                .Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries)
                .Where(k => !string.IsNullOrWhiteSpace(k))
                .Select(k => k == "singlePress" ? k : "*" + k)
                .ToList();
            if (keys.Count > 0) rows.Add(keys);
        }

        if (keymapHotkey.Contains("button", StringComparison.OrdinalIgnoreCase))
        {
            var flat = rows.SelectMany(r => r).ToHashSet();
            var missing = new List<string>();
            foreach (var button in new[] { "*LButton", "*MButton", "*RButton", "*WheelUp", "*WheelDown", "*XButton1", "*XButton2" })
            {
                if (!flat.Contains(button)) missing.Add(button);
            }
            if (missing.Count > 0) rows.Add(missing);
        }
        return rows;
    }

    /// <summary>
    /// 复刻 _saveConfig 的清洗: 返回可直接 PUT 的载荷 (深拷贝, 不动原模型)。
    /// 规则: 逐 keymap、逐热键 —— 剔除 isEmpty 动作后,
    /// 非空且 (内置 keymap(id&lt;=4) 或 键在布局键集合内) 才保留。
    /// </summary>
    public static Config CleanForSave(Config live)
    {
        // 结构深拷贝 (序列化字段全覆盖; IsEmpty/IsNew 为 [JsonIgnore] 会被丢弃, 见下方下标对齐)
        var json = JsonSerializer.Serialize(live, SettingsJson.Options);
        var payload = JsonSerializer.Deserialize<Config>(json, SettingsJson.Options)
                      ?? throw new InvalidOperationException("Config 深拷贝失败");

        for (var i = 0; i < live.Keymaps.Count && i < payload.Keymaps.Count; i++)
        {
            var liveKm = live.Keymaps[i];
            var payKm = payload.Keymaps[i];

            var keySet = ParseKeyboardLayout(live.Options.KeyboardLayout, liveKm.Hotkey)
                .SelectMany(r => r)
                .ToHashSet(); // 大小写敏感, 与 JS Set.has 一致

            var keep = new Dictionary<string, List<Models.Action>>();
            foreach (var (hk, liveActions) in liveKm.Hotkeys)
            {
                var normalKeymap = liveKm.Id > 4;
                var surviving = new List<Models.Action>();
                if (payKm.Hotkeys.TryGetValue(hk, out var payActions))
                {
                    // 深拷贝保持列表顺序与数量, 按下标对齐即可还原 IsEmpty 判定
                    var count = Math.Min(liveActions.Count, payActions.Count);
                    for (var j = 0; j < count; j++)
                    {
                        if (!liveActions[j].IsEmpty) surviving.Add(payActions[j]);
                    }
                }

                if (surviving.Count > 0 && (!normalKeymap || keySet.Contains(hk)))
                {
                    keep[hk] = surviving;
                }
            }
            payKm.Hotkeys = keep;
        }
        return payload;
    }
}

// ============================================================================
// 配置动作级共享逻辑 (复刻 config.ts 的 changeAbbrEnable 与 Settings.vue 的
// normalizeKeyName), 供设置页与后续任务 (#10 动作编辑) 复用。
// ============================================================================

public static class ConfigActions
{
    /// <summary>
    /// 复刻 changeAbbrEnable: 遍历启用的非缩写/设置 keymap, 若存在
    /// actionTypeID=9 且 actionValueID=6 (CapsLock 命令) / =5 (缩写) 的动作,
    /// 则相应启用 keymaps 倒数第 3 / 倒数第 2 个 (即 Command / Abbreviation)。
    /// </summary>
    public static void ChangeAbbrEnable(Config config)
    {
        if (config.Keymaps.Count < 3) return;
        var capsAbbr = config.Keymaps[^3];
        var seemAbbr = config.Keymaps[^2];
        var capsAbbrEnable = false;
        var seemAbbrEnable = false;

        foreach (var km in config.Keymaps.Where(k => k.Enable))
        {
            // 不遍历缩写、设置 (Vue: id 2~4 跳过)
            if (km.Id is >= 2 and <= 4) continue;
            if (capsAbbrEnable && seemAbbrEnable) break;

            foreach (var actions in km.Hotkeys.Values)
            {
                foreach (var act in actions)
                {
                    if (act.TypeId == 9 && act.ValueId == 6)
                    {
                        capsAbbrEnable = true;
                        continue;
                    }
                    if (act.TypeId == 9 && act.ValueId == 5)
                    {
                        seemAbbrEnable = true;
                    }
                }
            }
        }

        capsAbbr.Enable = capsAbbrEnable;
        seemAbbr.Enable = seemAbbrEnable;
    }

    private static readonly Dictionary<string, string> KeyNameNormalizeMap = BuildNormalizeMap();

    private static Dictionary<string, string> BuildNormalizeMap()
    {
        var m = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["esc"] = "Escape",
            ["bs"] = "Backspace",
            ["del"] = "Delete",
            ["ins"] = "Insert",
            ["lctrl"] = "LControl",
            ["rctrl"] = "RControl",
        };
        // Vue: 同时登记 "*前缀" 变体
        foreach (var (k, v) in m.ToList())
        {
            m["*" + k] = "*" + v;
        }
        return m;
    }

    /// <summary>复刻 normalizeKeyName: 把 bs/esc/del 等简写规范化为 AHK 键名。</summary>
    public static string NormalizeKeyName(string hotkey)
        => KeyNameNormalizeMap.TryGetValue(hotkey, out var v) ? v : hotkey;
}
