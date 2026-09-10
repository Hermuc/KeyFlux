using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace KeyFlux.Settings.Models;

/// <summary>
/// 全局共享的 System.Text.Json 序列化选项。
/// 属性名完全依赖各模型上的 [JsonPropertyName] (与 Go json tag 对齐),
/// 不启用命名策略; 写空 (null) 字段时省略, 与 Go omitempty 语义近似。
/// </summary>
public static class SettingsJson
{
    public static readonly JsonSerializerOptions Options = CreateOptions();

    private static JsonSerializerOptions CreateOptions()
    {
        var opts = new JsonSerializerOptions
        {
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        };
        return opts;
    }
}

/// <summary>
/// 只读路径上的默认值注入 (复刻前端行为, 不改写磁盘文件)。
/// 对照: config-ui/src/store/config.ts fetchConfig() 的 watch 回调 ——
/// 旧配置文件缺少后加字段时, 读取后在前端内存里补齐默认值。
/// </summary>
public static class ConfigReadDefaults
{
    /// <summary>前端 defaultKeyboardLayout (config-ui/src/store/config.ts)。</summary>
    public const string DefaultKeyboardLayout =
        "1 2 3 4 5 6 7 8 9 0\n" +
        "q w e r t y u i o p\n" +
        "a s d f g h j k l ;\n" +
        "z x c v b n m , . /\n" +
        "space enter backspace - [ ' singlePress";

    /// <summary>前端 keyboardLayout74 (resetKeyboardLayout(74))。</summary>
    public const string KeyboardLayout74 =
        "esc f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12\n" +
        "` 1 2 3 4 5 6 7 8 9 0 - = backspace\n" +
        "tab q w e r t y u i o p [ ] \\\n" +
        "capslock a s d f g h j k l ; ' enter\n" +
        "LShift z x c v b n m , . / RShift\n" +
        "LCtrl LWin LAlt space RAlt RWin RCtrl singlePress";

    /// <summary>前端 keyboardLayout104 (resetKeyboardLayout(104))。</summary>
    public const string KeyboardLayout104 =
        "esc f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12\n" +
        "` 1 2 3 4 5 6 7 8 9 0 - = backspace\n" +
        "tab q w e r t y u i o p [ ] \\\n" +
        "capslock a s d f g h j k l ; ' enter\n" +
        "LShift z x c v b n m , . / RShift\n" +
        "LCtrl LWin LAlt space RAlt RWin RCtrl singlePress\n" +
        "PrintScreen ScrollLock Pause insert home pgup delete end pgdn up down left right\n" +
        "numpad0 numpad1 numpad2 numpad3 numpad4 numpad5 numpad6 numpad7 numpad8 numpad9\n" +
        "NumpadDot NumpadEnter NumpadAdd NumpadSub NumpadMult NumpadDiv NumLock";

    /// <summary>前端 mouseButtons (resetKeyboardLayout(1) 追加行)。</summary>
    public const string MouseButtons =
        "LButton RButton MButton XButton1 XButton2 WheelUp WheelDown WheelLeft WheelRight";

    /// <summary>窗口分组排除项 (windowGroups 首项固定哨兵, id = -1)。</summary>
    public const int ExcludeGroupId = -1;

    /// <summary>「📂 文件对话框」窗口组的固定 id (默认集 {-1,0,1,3}, 自动新增用 Rows.Count+1, 故 2 恒空闲)。</summary>
    public const int FileDialogGroupId = 2;

    /// <summary>「📂 文件对话框」窗口组条件值 (QuickSwitch 以此为生效范围)。</summary>
    public const string FileDialogGroupValue = "ahk_class #32770";

    /// <summary>「📂 文件对话框」窗口组显示名。</summary>
    public const string FileDialogGroupName = "📂 文件对话框";

    /// <summary>
    /// 就地补齐默认值 (内存对象, 不回写磁盘):
    /// 1. keyboardLayout 为空 -> 默认键盘布局;
    /// 2. language 为空 -> 按当前系统语言环境选 "zh" / "en" (前端按 navigator.language);
    /// 3. windowGroups 首项不是 id=-1 的排除项 -> 头部插入 "🚫 Exclude";
    /// 4. 缺少「📂 文件对话框」窗口组 (value == "ahk_class #32770") 且 id=2 未被占用 -> 追加该组;
    ///    (id=2 已被用户占用时不覆盖用户数据, 保持原样);
    /// 5. 旧配置缺少 quickSwitch 段 (全零签名) -> 补齐设计默认值;
    /// 6. selectedAction 缺失 -> 空对象 (恒对象契约; 属性默认值已保证, 此处显式兜底)。
    /// </summary>
    public static Config Apply(Config config)
    {
        var options = config.Options;

        if (string.IsNullOrEmpty(options.KeyboardLayout))
        {
            options.KeyboardLayout = DefaultKeyboardLayout;
        }

        if (string.IsNullOrEmpty(options.Language))
        {
            options.Language =
                System.Globalization.CultureInfo.CurrentUICulture.TwoLetterISOLanguageName == "zh"
                    ? "zh"
                    : "en";
        }

        if (options.WindowGroups.Count == 0 || options.WindowGroups[0].Id != ExcludeGroupId)
        {
            options.WindowGroups.Insert(0, new WindowGroup
            {
                Id = ExcludeGroupId,
                Name = "🚫 Exclude",
                Value = "",
            });
        }

        // 读时补齐「📂 文件对话框」窗口组 (先例: 上方的 Exclude 哨兵)。绝不回写磁盘。
        if (!options.WindowGroups.Any(g => g.Value == FileDialogGroupValue)
            && !options.WindowGroups.Any(g => g.Id == FileDialogGroupId))
        {
            options.WindowGroups.Add(new WindowGroup
            {
                Id = FileDialogGroupId,
                Name = FileDialogGroupName,
                Value = FileDialogGroupValue,
                ConditionType = 1,
            });
        }

        // 读时补齐 quickSwitch 段: 旧配置无该段时反序列化为全零, 以「全零签名」判定缺失。
        if (options.QuickSwitch is null || IsQuickSwitchUnset(options.QuickSwitch))
        {
            options.QuickSwitch = QuickSwitchDefaults();
        }

        config.SelectedAction ??= new SelectedAction();

        return config;
    }

    /// <summary>设计 §3.1 默认值 (autoShow/autoJumpOpen=true、autoJumpSave=false、poll=800、maxHistory=200、rows=8、compact=4)。</summary>
    public static QuickSwitchOption QuickSwitchDefaults() => new()
    {
        CollectEnabled = true,
        AutoShow = true,
        AutoJumpOpen = true,
        AutoJumpSave = false,
        PollIntervalMs = 800,
        MaxHistory = 200,
        OverlayRows = 8,
        OverlayRowsCompact = 4,
        ExcludedPrefixes = [],
    };

    /// <summary>
    /// 判定 quickSwitch 是否为「旧配置缺失」的全零签名。用户合法地把三个开关全关时,
    /// 数值字段仍保留默认 (800/200/8/4), 故不会误判为缺失。
    /// </summary>
    private static bool IsQuickSwitchUnset(QuickSwitchOption q)
        => !q.CollectEnabled && !q.AutoShow && !q.AutoJumpOpen && !q.AutoJumpSave
           && q.PollIntervalMs == 0 && q.MaxHistory == 0
           && q.OverlayRows == 0 && q.OverlayRowsCompact == 0
           && (q.ExcludedPrefixes is null || q.ExcludedPrefixes.Count == 0);
}
