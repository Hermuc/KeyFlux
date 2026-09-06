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

    /// <summary>
    /// 就地补齐默认值 (内存对象, 不回写磁盘):
    /// 1. keyboardLayout 为空 -> 默认键盘布局;
    /// 2. language 为空 -> 按当前系统语言环境选 "zh" / "en" (前端按 navigator.language);
    /// 3. windowGroups 首项不是 id=-1 的排除项 -> 头部插入 "🚫 Exclude";
    /// 4. selectedAction 缺失 -> 空对象 (恒对象契约; 属性默认值已保证, 此处显式兜底)。
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

        config.SelectedAction ??= new SelectedAction();

        return config;
    }
}
