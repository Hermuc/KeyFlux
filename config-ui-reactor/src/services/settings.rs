//! 选项页（keymap id=4「Settings」）的**纯逻辑**（零 UI 依赖）。
//!
//! 对照旧 `config-ui-avalonia/Views/SettingsPageView.axaml` + `SettingsPageViewModel.cs`
//! （git `1f3dc9f~1`）逐分区复刻：键盘布局预设、开机自启命令映射、鼠标/滚轮字符串型数值、
//! 命令框皮肤 18 字段表、命令框字重枚举、路径变量行编辑。
//!
//! 保存语义：除「开机自启」（即时 `POST /server/command/3|4`）外，全部随保存链路
//! （Ctrl+S → `PUT /config` → Go 重启引擎）。

use crate::models::{Config, PathVariable};

// ---------------------------------------------------------------- 键盘布局预设

/// 出厂默认布局（旧 `ConfigReadDefaults.DefaultKeyboardLayout`）。
pub const KEYBOARD_LAYOUT_DEFAULT: &str = concat!(
    "1 2 3 4 5 6 7 8 9 0\n",
    "q w e r t y u i o p\n",
    "a s d f g h j k l ;\n",
    "z x c v b n m , . /\n",
    "space enter backspace - [ ' singlePress",
);

/// 74 键布局（旧 `ConfigReadDefaults.KeyboardLayout74`）。
pub const KEYBOARD_LAYOUT_74: &str = concat!(
    "esc f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12\n",
    "` 1 2 3 4 5 6 7 8 9 0 - = backspace\n",
    "tab q w e r t y u i o p [ ] \\\n",
    "capslock a s d f g h j k l ; ' enter\n",
    "LShift z x c v b n m , . / RShift\n",
    "LCtrl LWin LAlt space RAlt RWin RCtrl singlePress",
);

/// 104 键布局（旧 `ConfigReadDefaults.KeyboardLayout104`）。
pub const KEYBOARD_LAYOUT_104: &str = concat!(
    "esc f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12\n",
    "` 1 2 3 4 5 6 7 8 9 0 - = backspace\n",
    "tab q w e r t y u i o p [ ] \\\n",
    "capslock a s d f g h j k l ; ' enter\n",
    "LShift z x c v b n m , . / RShift\n",
    "LCtrl LWin LAlt space RAlt RWin RCtrl singlePress\n",
    "PrintScreen ScrollLock Pause insert home pgup delete end pgdn up down left right\n",
    "numpad0 numpad1 numpad2 numpad3 numpad4 numpad5 numpad6 numpad7 numpad8 numpad9\n",
    "NumpadDot NumpadEnter NumpadAdd NumpadSub NumpadMult NumpadDiv NumLock",
);

/// 鼠标按钮追加行（旧 `ConfigReadDefaults.MouseButtons`，resetKeyboardLayout(1)）。
pub const MOUSE_BUTTONS: &str =
    "LButton RButton MButton XButton1 XButton2 WheelUp WheelDown WheelLeft WheelRight";

/// 布局预设按钮（723=恢复默认 / 724=74 键 / 725=104 键 / 726=添加鼠标按钮）。
///
/// `kind` 取值沿用旧 VM 的 `ResetKeyboardLayout(string kind)`：`"0"` / `"74"` / `"104"` / `"1"`。
pub fn keyboard_layout_preset(kind: &str, current: &str) -> Option<String> {
    match kind {
        "0" => Some(KEYBOARD_LAYOUT_DEFAULT.to_string()),
        "74" => Some(KEYBOARD_LAYOUT_74.to_string()),
        "104" => Some(KEYBOARD_LAYOUT_104.to_string()),
        // 追加鼠标按钮行：换行拼接（旧版直接拼接）
        "1" => {
            let mut layout = current.to_string();
            if !layout.is_empty() {
                layout.push('\n');
            }
            layout.push_str(MOUSE_BUTTONS);
            Some(layout)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- 开机自启

/// 开机自启「开」的服务端命令（Go `handlers.go`：`POST /server/command/3` → MiscTools RunAtStartup On）。
pub const STARTUP_ON_COMMAND: i32 = 3;
/// 开机自启「关」的服务端命令（`POST /server/command/4`）。
pub const STARTUP_OFF_COMMAND: i32 = 4;

/// 开关 → 服务端命令 id（旧语义：**即时生效**，不走保存链路；config 值只是回显态）。
pub fn startup_command_id(enabled: bool) -> i32 {
    if enabled {
        STARTUP_ON_COMMAND
    } else {
        STARTUP_OFF_COMMAND
    }
}

/// 回写回显态（真实生效态 = 计划任务 KeyFlux，由服务端命令维护）。
pub fn set_startup(config: &mut Config, enabled: bool) {
    config.options.startup = enabled;
}

// ---------------------------------------------------------------- 数值字段（字符串型数值）

// 说明：`options.mouse.*` / `options.scroll.*` 在 Go 侧均为 **字符串**（"0.13"/"110"），
// 旧版 UI 亦为自由文本框（无 clamp）⇒ reactor 保持原样透传，不在前端做数值整形。

// ---------------------------------------------------------------- 命令框皮肤（18 字段）

/// 皮肤字段种类（决定 UI 控件形态与校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinFieldKind {
    /// 颜色 `#RRGGBB`。
    Color,
    /// 数字（透明度 0-1 / 像素 / 秒）。
    Number,
}

/// 皮肤字段描述（JSON 键 + 文案键 + 种类），驱动 UI 通用渲染。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinField {
    /// `CommandInputSkin` 的 JSON 键（camelCase）。
    pub key: &'static str,
    /// 文案键（6 个透明度共用 748「透明度」，与旧版一致）。
    pub label_key: &'static str,
    pub kind: SkinFieldKind,
}

/// 18 字段渲染顺序（窗口 → 边框 → 网格线 → 按键 → 四角 → 阴影，透明度紧随其宿主字段）。
pub const SKIN_FIELDS: [SkinField; 18] = [
    SkinField {
        key: "windowWidth",
        label_key: "743",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "windowYPos",
        label_key: "744",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "borderRadius",
        label_key: "745",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "hideAnimationDuration",
        label_key: "746",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "backgroundColor",
        label_key: "747",
        kind: SkinFieldKind::Color,
    },
    SkinField {
        key: "backgroundOpacity",
        label_key: "748",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "borderWidth",
        label_key: "750",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "borderColor",
        label_key: "751",
        kind: SkinFieldKind::Color,
    },
    SkinField {
        key: "borderOpacity",
        label_key: "748",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "gridlineColor",
        label_key: "749",
        kind: SkinFieldKind::Color,
    },
    SkinField {
        key: "gridlineOpacity",
        label_key: "748",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "keyColor",
        label_key: "752",
        kind: SkinFieldKind::Color,
    },
    SkinField {
        key: "keyOpacity",
        label_key: "748",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "cornerColor",
        label_key: "753",
        kind: SkinFieldKind::Color,
    },
    SkinField {
        key: "cornerOpacity",
        label_key: "748",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "windowShadowColor",
        label_key: "755",
        kind: SkinFieldKind::Color,
    },
    SkinField {
        key: "windowShadowOpacity",
        label_key: "748",
        kind: SkinFieldKind::Number,
    },
    SkinField {
        key: "windowShadowSize",
        label_key: "754",
        kind: SkinFieldKind::Number,
    },
];

/// 按 JSON 键读取皮肤字段值（未知键 `None`）。
pub fn skin_get<'a>(skin: &'a crate::models::CommandInputSkin, key: &str) -> Option<&'a str> {
    match key {
        "windowWidth" => Some(&skin.window_width),
        "windowYPos" => Some(&skin.window_y_pos),
        "borderRadius" => Some(&skin.border_radius),
        "hideAnimationDuration" => Some(&skin.hide_animation_duration),
        "backgroundColor" => Some(&skin.background_color),
        "backgroundOpacity" => Some(&skin.background_opacity),
        "borderWidth" => Some(&skin.border_width),
        "borderColor" => Some(&skin.border_color),
        "borderOpacity" => Some(&skin.border_opacity),
        "gridlineColor" => Some(&skin.gridline_color),
        "gridlineOpacity" => Some(&skin.gridline_opacity),
        "keyColor" => Some(&skin.key_color),
        "keyOpacity" => Some(&skin.key_opacity),
        "cornerColor" => Some(&skin.corner_color),
        "cornerOpacity" => Some(&skin.corner_opacity),
        "windowShadowColor" => Some(&skin.window_shadow_color),
        "windowShadowOpacity" => Some(&skin.window_shadow_opacity),
        "windowShadowSize" => Some(&skin.window_shadow_size),
        _ => None,
    }
    .map(String::as_str)
}

/// 按 JSON 键写皮肤字段值（未知键忽略，返回 `false`）。
pub fn skin_set(skin: &mut crate::models::CommandInputSkin, key: &str, value: &str) -> bool {
    match key {
        "windowWidth" => skin.window_width = value.to_string(),
        "windowYPos" => skin.window_y_pos = value.to_string(),
        "borderRadius" => skin.border_radius = value.to_string(),
        "hideAnimationDuration" => skin.hide_animation_duration = value.to_string(),
        "backgroundColor" => skin.background_color = value.to_string(),
        "backgroundOpacity" => skin.background_opacity = value.to_string(),
        "borderWidth" => skin.border_width = value.to_string(),
        "borderColor" => skin.border_color = value.to_string(),
        "borderOpacity" => skin.border_opacity = value.to_string(),
        "gridlineColor" => skin.gridline_color = value.to_string(),
        "gridlineOpacity" => skin.gridline_opacity = value.to_string(),
        "keyColor" => skin.key_color = value.to_string(),
        "keyOpacity" => skin.key_opacity = value.to_string(),
        "cornerColor" => skin.corner_color = value.to_string(),
        "cornerOpacity" => skin.corner_opacity = value.to_string(),
        "windowShadowColor" => skin.window_shadow_color = value.to_string(),
        "windowShadowOpacity" => skin.window_shadow_opacity = value.to_string(),
        "windowShadowSize" => skin.window_shadow_size = value.to_string(),
        _ => return false,
    }
    true
}

/// 皮肤字段校验（保存前；`None` = 通过）。
pub fn validate_skin_field(field: &SkinField, value: &str) -> Option<String> {
    match field.kind {
        SkinFieldKind::Color => {
            let trimmed = value.trim();
            let ok = trimmed.len() == 7
                && trimmed.starts_with('#')
                && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit());
            if ok {
                None
            } else {
                Some("#RRGGBB".to_string())
            }
        }
        SkinFieldKind::Number => {
            let ok = value
                .trim()
                .parse::<f64>()
                .map(|n| n >= 0.0)
                .unwrap_or(false);
            if ok { None } else { Some("number".to_string()) }
        }
    }
}

// ---------------------------------------------------------------- 命令框字体

/// 字重档位（Go `CommandFontOption.Weight`："thin"/"light"/"regular"/"semibold"/"bold"）。
pub const FONT_WEIGHTS: [&str; 5] = ["thin", "light", "regular", "semibold", "bold"];

/// 字重档位 → 文案键（2526 极细 / 2549 细 / 2509 常规 / 2511 半粗 / 2516 粗体）。
pub fn font_weight_label_key(weight: &str) -> &'static str {
    match weight {
        "thin" => "2526",
        "light" => "2549",
        "semibold" => "2511",
        "bold" => "2516",
        _ => "2509",
    }
}

/// 「恢复默认」（2507）：清空自定义字体路径并回到常规字重（空 = 未自定义，用内置字体）。
pub fn font_reset(config: &mut Config) {
    config.options.command_font.source_path = String::new();
    config.options.command_font.weight = String::new();
}

// ---------------------------------------------------------------- 路径变量

/// 追加一个空行（返回新行下标）。
pub fn add_path_variable(config: &mut Config) -> usize {
    config.options.path_variables.push(PathVariable::default());
    config.options.path_variables.len() - 1
}

/// 删除第 `index` 行（越界忽略）。
pub fn remove_path_variable(config: &mut Config, index: usize) {
    if index < config.options.path_variables.len() {
        config.options.path_variables.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CommandInputSkin, Options};

    #[test]
    fn keyboard_layout_presets_match_legacy_strings() {
        assert!(KEYBOARD_LAYOUT_DEFAULT.starts_with("1 2 3 4 5 6 7 8 9 0\n"));
        assert!(KEYBOARD_LAYOUT_DEFAULT.ends_with("singlePress"));
        assert!(KEYBOARD_LAYOUT_74.starts_with("esc f1 f2"));
        assert!(KEYBOARD_LAYOUT_104.contains("PrintScreen ScrollLock Pause"));
        assert_eq!(
            MOUSE_BUTTONS,
            "LButton RButton MButton XButton1 XButton2 WheelUp WheelDown WheelLeft WheelRight"
        );

        assert_eq!(
            keyboard_layout_preset("0", ""),
            Some(KEYBOARD_LAYOUT_DEFAULT.to_string())
        );
        assert_eq!(
            keyboard_layout_preset("74", ""),
            Some(KEYBOARD_LAYOUT_74.to_string())
        );
        assert_eq!(
            keyboard_layout_preset("104", ""),
            Some(KEYBOARD_LAYOUT_104.to_string())
        );
        assert_eq!(keyboard_layout_preset("zzz", ""), None);

        // 「添加鼠标按钮」= 换行追加
        let appended = keyboard_layout_preset("1", "a b").expect("kind=1 恒有结果");
        assert!(appended.starts_with("a b\n"));
        assert!(appended.ends_with(MOUSE_BUTTONS));
        // 空布局不产生前导换行
        assert_eq!(
            keyboard_layout_preset("1", "").as_deref(),
            Some(MOUSE_BUTTONS)
        );
    }

    #[test]
    fn startup_maps_to_server_commands() {
        assert_eq!(startup_command_id(true), STARTUP_ON_COMMAND);
        assert_eq!(startup_command_id(false), STARTUP_OFF_COMMAND);
        assert_eq!(STARTUP_ON_COMMAND, 3);
        assert_eq!(STARTUP_OFF_COMMAND, 4);

        let mut config = Config::default();
        set_startup(&mut config, true);
        assert!(config.options.startup);
    }

    #[test]
    fn skin_number_fields_reject_negative_and_non_numeric() {
        // 数值字段走 validate_skin_field（透明度/像素/秒均 >= 0）
        let number = SkinField {
            key: "windowWidth",
            label_key: "743",
            kind: SkinFieldKind::Number,
        };
        assert!(validate_skin_field(&number, "700").is_none());
        assert_eq!(
            validate_skin_field(&number, "-1").as_deref(),
            Some("number")
        );
        assert_eq!(
            validate_skin_field(&number, "abc").as_deref(),
            Some("number")
        );
    }

    #[test]
    fn skin_table_covers_exactly_18_unique_fields() {
        assert_eq!(SKIN_FIELDS.len(), 18);
        let mut keys: Vec<&str> = SKIN_FIELDS.iter().map(|field| field.key).collect();
        keys.sort_unstable();
        let unique = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), unique, "字段键不得重复");

        // 透明度 6 个共用文案键 748（与旧版一致）
        assert_eq!(
            SKIN_FIELDS
                .iter()
                .filter(|field| field.label_key == "748")
                .count(),
            6
        );
        // 每个键都能在模型上取到值（缺一不可）
        let skin = CommandInputSkin::default();
        for field in &SKIN_FIELDS {
            assert!(skin_get(&skin, field.key).is_some(), "缺字段 {}", field.key);
        }
    }

    #[test]
    fn skin_get_set_round_trips_every_field() {
        let mut skin = CommandInputSkin::default();
        for field in &SKIN_FIELDS {
            assert!(skin_set(&mut skin, field.key, "x1"));
            assert_eq!(skin_get(&skin, field.key), Some("x1"));
        }
        assert!(!skin_set(&mut skin, "nope", "v"), "未知键被拒绝");
    }

    #[test]
    fn skin_validation_by_kind() {
        let color = SkinField {
            key: "backgroundColor",
            label_key: "747",
            kind: SkinFieldKind::Color,
        };
        assert!(validate_skin_field(&color, "#FFFFFF").is_none());
        assert_eq!(
            validate_skin_field(&color, "FFFFFF").as_deref(),
            Some("#RRGGBB")
        );
        assert_eq!(
            validate_skin_field(&color, "#GGG").as_deref(),
            Some("#RRGGBB")
        );

        let number = SkinField {
            key: "windowWidth",
            label_key: "743",
            kind: SkinFieldKind::Number,
        };
        assert!(validate_skin_field(&number, "700").is_none());
        assert_eq!(
            validate_skin_field(&number, "-3").as_deref(),
            Some("number")
        );
        assert_eq!(
            validate_skin_field(&number, "abc").as_deref(),
            Some("number")
        );
    }

    #[test]
    fn font_weights_cover_go_enum_with_labels() {
        assert_eq!(
            FONT_WEIGHTS,
            ["thin", "light", "regular", "semibold", "bold"]
        );
        assert_eq!(font_weight_label_key("thin"), "2526");
        assert_eq!(font_weight_label_key("light"), "2549");
        assert_eq!(font_weight_label_key("regular"), "2509");
        assert_eq!(font_weight_label_key("semibold"), "2511");
        assert_eq!(font_weight_label_key("bold"), "2516");
        // 未知档位回退「常规」
        assert_eq!(font_weight_label_key("zzz"), "2509");
    }

    #[test]
    fn font_reset_clears_customization() {
        let mut config = Config::default();
        config.options.command_font.source_path = "D:/fonts/mi.ttf".to_string();
        config.options.command_font.weight = "bold".to_string();

        font_reset(&mut config);
        assert_eq!(config.options.command_font.source_path, "");
        assert_eq!(config.options.command_font.weight, "");
    }

    #[test]
    fn path_variable_row_editing() {
        let mut config = Config::default();
        let index = add_path_variable(&mut config);
        assert_eq!(index, 0);
        config.options.path_variables[0] = PathVariable {
            name: "programs".to_string(),
            value: "C:/ProgramData/Microsoft/Windows/Start Menu/Programs/".to_string(),
        };
        assert_eq!(config.options.path_variables.len(), 1);

        remove_path_variable(&mut config, 0);
        assert!(config.options.path_variables.is_empty());
        remove_path_variable(&mut config, 9); // 越界忽略，不 panic
    }

    #[test]
    fn options_defaults_carry_every_mouse_scroll_field() {
        // UI 逐字段绑定依赖这些字段存在且为字符串型（与 Go 契约一致）
        let options = Options::default();
        assert_eq!(options.mouse.delay1, "");
        assert_eq!(options.mouse.fast_single, "");
        assert_eq!(options.scroll.once_line_count, "");
        assert_eq!(options.language, "");
        assert!(!options.hide_matrix);
    }
}
