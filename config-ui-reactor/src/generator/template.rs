//! 模板渲染 —— Go `config-server/templates/keyflux.tmpl` + `CommandInputSkin.tmpl` 的
//! **逐字改写**（不引入 Go `text/template` 引擎；两个模板都很小，字段访问占绝大多数）。
//!
//! 渲染入口对应 Go `script.SaveAHK`（用 `generators.TemplateFuncMap`）:
//! * [`render_keyflux_ahk`] ⇒ `bin/KeyFlux.ahk`（模板 `keyflux.tmpl`）；
//! * [`render_command_input_skin`] ⇒ `bin/CommandInputSkin.txt`（模板 `CommandInputSkin.tmpl`）。
//!
//! ⚠️ 与 Go 的**有意差异**：Go 用全局 `generators.Cfg` / `BehaviorCatalog` / `PluginsDir`，
//! 本移植把 `config` / `catalog` / `plugins_dir` 显式传参（迁移纪律：不引入进程级可变态）。
//!
//! 字节口径（全部由 `tools/parity` 的 9 份基线守护）：
//! 1. **CRLF**：Go `SaveAHK` 先 `\r\n→\n` 再 `\n→\r\n`，故本模块统一在末尾做同样归一
//!    （`render_keymap` 已产 CRLF，折叠后重展，结果一致）。
//! 2. **BOM**：`keyflux.tmpl` 自身带 U+FEFF（模板首字符），会被原样渲染进产物 ⇒
//!    `KeyFlux.ahk` 带 BOM；`CommandInputSkin.tmpl` **无** BOM ⇒ `CommandInputSkin.txt`
//!    **无** BOM。两件事都来自模板首字符，故此处按要求分别处理。
//! 3. **模板空白控制**（`{{-` / `-}}`）逐处对齐 —— 见各分支处「Go 模板 …」注释。
//!
//! 特殊：`keyflux.tmpl` 的 `.Options.Mouse.TipSymbol`、`.CapslockAbbrKeys`、
//! `.SemicolonAbbrKeys`、`.PathVariables` 等是 Go 模板字段/方法调用，均已由
//! [`Config`] 的同名方法提供（见 `model.rs`）。

use std::collections::HashSet;
use std::path::Path;

use crate::generator::actions::{
    abbr_registry_code, group_disable_keyflux, render_keymap, selected_action_code,
};
use crate::generator::behaviors::Catalog;
use crate::generator::model::Config;
use crate::generator::plugins;

/// Go `%t`：bool 打印为 `true` / `false`（模板里 `{{ .Options.X.Y }}` 走这条）。
fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// Go `SaveAHK` 的行尾归一：先 `\r\n`→`\n`，再 `\n`→`\r\n`。
fn normalize_to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// 渲染 `keyflux.tmpl` ⇒ `KeyFlux.ahk` 的字节（CRLF + 前导 UTF-8 BOM）。
///
/// 与 Go `SaveAHK(config, "keyflux.tmpl", out)` 逐字节等价（`config` 必须已经
/// `parse_config` 且 `preprocess`；`catalog` 供选中动作解析；`plugins_dir` =
/// `<config.json 目录>/plugins`）。
pub fn render_keyflux_ahk(
    config: &mut Config,
    catalog: Option<&Catalog>,
    plugins_dir: &Path,
) -> String {
    // 停用/墓碑插件集（Go `disabledPluginSet` / `removedPluginSet` 读
    // `generators.Cfg.Options.Plugins.Disabled|Removed`）。
    let disabled: HashSet<String> = config.options.plugins.disabled.iter().cloned().collect();
    let removed: HashSet<String> = config.options.plugins.removed.iter().cloned().collect();
    let (plugin_includes, plugin_bootstrap) =
        plugins::render_plugin_blocks(plugins_dir, &disabled, &removed);

    let mut out = String::new();

    // ---- 模板首字符 BOM（keyflux.tmpl 带 U+FEFF，Go 原样渲染进产物） ----
    out.push('\u{feff}');

    // ---- L1-L29: 头 + include 列表，末尾到 `#Include lib/plugins/Plugins.ahk` ----
    // `{{ PLUGIN_INCLUDES }}` 是行尾拼接（非空时自带前导 `\n`），故此处紧贴。
    out.push_str(HEAD_INCLUDES);
    out.push_str(&plugin_includes);
    // L29 末尾换行 + L30 空行
    out.push_str("\n\n");

    // ---- L31-L43: 设置项，末尾到 `SetWorkingDir("../")` ----
    // `{{ PLUGIN_BOOTSTRAP }}` 同样行尾拼接（自带前导 `\n`）。
    out.push_str(SETUP_AND_WORKDIR);
    out.push_str(&plugin_bootstrap);
    // L43 末尾换行
    out.push('\n');

    // ---- L44-L60: 引擎初始化（到 `InitKeymap()`，ENGINE_INIT 末尾含 `\n`） ----
    out.push_str(ENGINE_INIT);

    // ---- L61: `{{- PLUGIN_LATE_INIT }}`（插件晚初始化扩展点，模板用 `{{-` 吃前导
    //      换行 ⇒ 行尾拼接约定）。非空时输出对应 late-init 调用行 (首个消费方 quick_switch)（P5 起，
    //      配置由插件运行时自取，见 plugins::render_late_init）；空块 = 零字节。
    let plugin_late_init = plugins::render_late_init(plugins_dir, &disabled, &removed);
    if !plugin_late_init.is_empty() {
        out.push_str(plugin_late_init.trim_start_matches('\n'));
        out.push('\n');
    }

    // ---- L62-L67: 到 `taskSwitch := ...` ----
    out.push_str(INITKEYMAP_HEAD);

    // ---- L69: mouseTip（Go: `{{ if .Options.Mouse.ShowTip }}...{{ else }}false{{ end }}`） ----
    let mouse = &config.options.mouse;
    out.push_str("  mouseTip := ");
    if mouse.show_tip {
        out.push_str(&format!(
            "InputTipWindow(\"{}\",,,, 20, 16)",
            mouse.tip_symbol
        ));
    } else {
        out.push_str("false");
    }
    out.push('\n');

    // ---- L70-L71: slow / fast MouseKeymap ----
    let scroll = &config.options.scroll;
    // slow
    out.push_str("  slow := MouseKeymap(\"slow mouse\", ");
    out.push_str(bool_str(mouse.keep_mouse_mode));
    out.push_str(", mouseTip, ");
    out.push_str(&mouse.slow_single);
    out.push_str(", ");
    out.push_str(&mouse.slow_repeat);
    out.push_str(&format!(
        ", \"T{}\", \"T{}\", {}, \"T{}\", \"T{}\")\n",
        mouse.delay1, mouse.delay2, scroll.once_line_count, scroll.delay1, scroll.delay2
    ));
    // fast（末尾多一个 `, slow`）
    out.push_str("  fast := MouseKeymap(\"fast mouse\", ");
    out.push_str(bool_str(mouse.keep_mouse_mode));
    out.push_str(", mouseTip, ");
    out.push_str(&mouse.fast_single);
    out.push_str(", ");
    out.push_str(&mouse.fast_repeat);
    out.push_str(&format!(
        ", \"T{}\", \"T{}\", {}, \"T{}\", \"T{}\", slow)\n",
        mouse.delay1, mouse.delay2, scroll.once_line_count, scroll.delay1, scroll.delay2
    ));

    // ---- L72 ----
    out.push_str("  slow.Map(\"*space\", slow.LButtonUp())\n");

    // ---- L73-L76: `{{ if .CapslockAbbrEnabled }}`（无 trim；L76 末尾换行被 L77 `{{-` 吃掉） ----
    let capslock_enabled = config.capslock_abbr_enabled();
    let semicolon_enabled = config.semicolon_abbr_enabled();
    if capslock_enabled {
        // body 以 L73 的换行开头（故先落一个空行），以 L75 的换行结尾。
        out.push_str("\n  ; hook 每会话经 MakeCapsHook() 动态创建 (见其函数注释): 透传模式 -> V, 否则历史形态\n");
        out.push_str("  Run(\"bin\\KeyFlux-CommandInput.exe\")\n");
    }

    // ---- L77-L84: `{{- if .SemicolonAbbrEnabled }}`（`{{-` 吃掉 L76 换行；body 以换行开头） ----
    if semicolon_enabled {
        out.push_str("\n  semiHook := InputHook(\"\", \"{CapsLock}{Esc}{;}\", \"");
        out.push_str(&config.semicolon_abbr_keys());
        out.push_str("\")\n");
        out.push_str("  semiHook.KeyOpt(\"{CapsLock}\", \"S\")\n");
        out.push_str("  semiHook.KeyOpt(\"{Backspace}\", \"N\")\n");
        out.push_str("  semiHook.OnChar := (ih, char) => semiHookAbbrWindow.Show(char, true)\n");
        out.push_str("  semiHook.OnKeyDown := (ih, vk, sc) => semiHookAbbrWindow.Backspace()\n");
        out.push_str("  semiHookAbbrWindow := InputTipWindow()\n");
    }
    // L84 `{{ end }}` 后的换行 + L85 空行
    out.push_str("\n\n");

    // ---- L86-L87: 路径变量 ----
    out.push_str("  ; 路径变量\n");
    out.push_str(&config.path_variables());
    // L87 模板换行
    out.push('\n');

    // ---- L88: `{{ if .CapslockAbbrEnabled }}  ; 缩写命令注册表...{{ end }}` ----
    // ⚠️ 该注释行与其**尾随换行**都在 if 体内部：capslock 关闭时 Go 整段跳过（连换行一起）。
    //    故换行必须并进 `if capslock_enabled` 分支 —— 无条件输出会多 1 空行（+2 字节 CRLF，
    //    corpus 的 3 条基线 capslock 全为 true，掩盖了这条 skip 路径）。
    if capslock_enabled {
        out.push_str("  ; 缩写命令注册表 (阶段 4: 取代 ExecCapslockAbbr 内的 switch)\n");
    }

    // ---- L89: 两段 abbr 注册表 + `  ; 窗口组` ----
    if capslock_enabled {
        out.push_str(&abbr_registry_code(
            config,
            &config.capslock_abbr(),
            "capslock",
            "  ",
        ));
    }
    if semicolon_enabled {
        out.push_str(&abbr_registry_code(
            config,
            &config.semicolon_abbr(),
            "semicolon",
            "  ",
        ));
    }
    out.push_str("  ; 窗口组\n");

    // ---- L90: 窗口组 + 自定义匹配类型 ----
    out.push_str(&config.window_groups());
    out.push_str(&config.custom_match_types());
    out.push('\n');

    // ---- L91 ----
    out.push_str("  KeymapManager.GlobalKeymap.DisabledAt := ");
    out.push_str(&group_disable_keyflux(&config.options.window_groups));
    out.push('\n');

    // ---- L92: `{{range .EnabledKeymaps}}{{renderKeymap .}}{{end}}` ----
    // `enabled_keymaps()` 有副作用（ID==1 触发 handle_key_remapping ⇒ 置 remap_in_hot_if
    // 与 key_mapping），必须在 renderKeymap 与末尾 `.KeyMapping` 之前调用。
    let keymaps = config.enabled_keymaps();
    for keymap in &keymaps {
        out.push_str(&render_keymap(keymap, config));
    }
    out.push('\n');

    // ---- L93: 选中动作 ----
    out.push_str(&selected_action_code(
        config.selected_action.as_ref(),
        catalog,
    ));
    out.push('\n');

    // ---- L94-L97 ----
    out.push('\n');
    out.push_str("  KeymapManager.GlobalKeymap.Enable()\n");
    out.push_str("}\n");
    out.push('\n');

    // ---- L98-L142: ExecCapslockAbbr 定义（`{{ if ... -}}` / `{{- else -}}` / `{{- end }}`） ----
    if capslock_enabled {
        out.push_str(CAPS_TRUE_HEAD);
        out.push_str("                  , \"");
        out.push_str(&config.capslock_abbr_keys());
        out.push_str("\")\n");
        out.push_str(CAPS_TRUE_TAIL);
    } else {
        out.push_str("ExecCapslockAbbr(command) {\n}");
    }
    // L142 `{{ end }}` 后的换行
    out.push('\n');

    // ---- L143: 空行 ----
    out.push('\n');

    // ---- L144-L151: ExecSemicolonAbbr 定义 ----
    if semicolon_enabled {
        out.push_str("ExecSemicolonAbbr(command) {\n");
        out.push_str("  CommandResolver.Resolve(\"semicolon\", command)\n");
        out.push('}');
    } else {
        out.push_str("ExecSemicolonAbbr(command) {\n}");
    }
    // L151 `{{ end }}` 后的换行
    out.push('\n');

    // ---- L152: 空行 ----
    out.push('\n');

    // ---- L153-L166: InitTrayMenu + 空行 ----
    out.push_str(TRAY_MENU);

    // ---- L167: {{ .KeyMapping }}（末尾无换行；渲染期由 handle_key_remapping 写入） ----
    out.push_str(&config.key_mapping);

    normalize_to_crlf(&out)
}

/// 渲染 `CommandInputSkin.tmpl` ⇒ `CommandInputSkin.txt` 的字节（CRLF，**无** BOM）。
///
/// Go 模板头部是一行 `{{/* 注释 */}}`，渲染为空串但**保留其后的换行** ⇒ 产物以空行开头；
/// 18 个字段各是一对 `{{ if 值 }}{{ 值 }}{{ else }}默认{{ end }}`（空串走默认值）。
pub fn render_command_input_skin(config: &Config) -> String {
    let skin = &config.options.command_input_skin;
    let mut out = String::new();

    // 首行模板注释渲染为空 + 其换行 ⇒ 产物首字符是换行。
    out.push('\n');

    // 逐字段：字面前缀（含对齐空格）严格照抄模板，值空则用默认字面量。
    let fields: [(&str, &str, &str); 18] = [
        (
            "backgroundColor         = ",
            &skin.background_color,
            "#FFFFFF",
        ),
        (
            "backgroundOpacity       = ",
            &skin.background_opacity,
            "0.9",
        ),
        ("borderWidth             = ", &skin.border_width, "3"),
        ("borderColor             = ", &skin.border_color, "#FFFFFF"),
        ("borderOpacity           = ", &skin.border_opacity, "1.0"),
        ("borderRadius            = ", &skin.border_radius, "10"),
        ("cornerColor             = ", &skin.corner_color, "#000000"),
        ("cornerOpacity           = ", &skin.corner_opacity, "0.0"),
        (
            "gridlineColor           = ",
            &skin.gridline_color,
            "#2843AD",
        ),
        ("gridlineOpacity         = ", &skin.gridline_opacity, "0.04"),
        ("keyColor                = ", &skin.key_color, "#000000"),
        ("keyOpacity              = ", &skin.key_opacity, "1.0"),
        (
            "hideAnimationDuration   = ",
            &skin.hide_animation_duration,
            "0.34",
        ),
        ("windowYPos              = ", &skin.window_y_pos, "0.25"),
        ("windowWidth             = ", &skin.window_width, "700"),
        (
            "windowShadowColor       = ",
            &skin.window_shadow_color,
            "#000000",
        ),
        (
            "windowShadowOpacity     = ",
            &skin.window_shadow_opacity,
            "0.5",
        ),
        (
            "windowShadowSize        = ",
            &skin.window_shadow_size,
            "3.0",
        ),
    ];
    for (index, (prefix, value, default)) in fields.iter().enumerate() {
        out.push_str(prefix);
        if value.is_empty() {
            out.push_str(default);
        } else {
            out.push_str(value);
        }
        // 末行无换行（模板最后一行 `{{ end }}` 后无换行）。
        if index + 1 < fields.len() {
            out.push('\n');
        }
    }

    normalize_to_crlf(&out)
}

// --------------------------------------------------------------------------- 模板静态片段
//
// 下列 raw 片段与 `keyflux.tmpl` 的静态文本逐字一致（不含任何 `{{ ... }}`）。
// 用 `r#"..."#` 保证反斜杠/引号/中文/emoji 原样保留；模板内无 `"#` 序列，故定界安全。

/// L1-L29（到 `#Include lib/plugins/Plugins.ahk`，**无**末尾换行）。
const HEAD_INCLUDES: &str = r#"#Requires AutoHotkey v2.0
#SingleInstance Force
#UseHook true

#include lib/core/translation.ahk
#Include lib/core/IKeyEventBus.ahk
#Include lib/core/EventBus.ahk
#Include lib/core/Functions.ahk
#Include lib/core/Programs.ahk
#Include lib/core/WindowUtils.ahk
#Include lib/core/AbbrInput.ahk
#Include lib/core/CommandDisplay.ahk
#Include lib/core/ImeInputHost.ahk
#Include lib/core/CommandImeGuard.ahk
#Include lib/core/CommandInputHooks.ahk
#Include lib/actions/Actions.ahk
#Include lib/core/KeymapManager.ahk
#Include lib/core/InputTipWindow.ahk
#Include lib/core/Utils.ahk
#Include lib/context/SelectionContext.ahk
#Include lib/rules/SelectedAction.ahk
#Include lib/commands/CommandResolver.ahk
#Include lib/plugins/Plugins.ahk"#;

/// L31-L43（到 `SetWorkingDir("../")`，**无**末尾换行）。
const SETUP_AND_WORKDIR: &str = r#"; #WinActivateForce   ; 先关了遇到相关问题再打开试试
; InstallKeybdHook    ; 这个可以重装 keyboard hook, 提高自己的 hook 优先级, 以后可能会用到
; ListLines False     ; 也许能提升一点点性能 ( 别抱期待 ), 当有这个需求时再打开试试
; #Warn All, Off      ; 也许能提升一点点性能 ( 别抱期待 ), 当有这个需求时再打开试试

try DllCall("SetThreadDpiAwarenessContext", "ptr", -3, "ptr") ; 多显示器不同缩放比例会导致问题: https://www.autohotkey.com/boards/viewtopic.php?f=14&t=13810
SetMouseDelay 0                                           ; SendInput 可能会降级为 SendEvent, 此时会有 10ms 的默认 delay
SetWinDelay 0                                             ; 默认会在 activate, maximize, move 等窗口操作后睡眠 100ms
A_MaxHotkeysPerInterval := 256                            ; 默认 70 可能有点低, 即使没有热键死循环也触发警告
SendMode "Event"                                          ; 执行 SendInput 的期间会短暂卸载 Hook, 这时候松开引导键会丢失 up 事件, 所以 Event 模式更适合 KeyFlux
SetKeyDelay 0                                             ; 默认 10 太慢了, https://www.reddit.com/r/AutoHotkey/comments/gd3z4o/possible_unreliable_detection_of_the_keyup_event/
ProcessSetPriority "High"
SetWorkingDir("../")"#;

/// L44-L61（到 `InitKeymap()`，末尾带换行）。
const ENGINE_INIT: &str = r#"; 引擎级未捕获异常兜底: 替代「错误弹窗 + 线程死亡 + Suspend 残留 (热键全灭)」,
; 记录全文到 logs\engine_error.log (见 Functions.ahk 的 EngineOnError 注释)。
OnError(EngineOnError)
InitTrayMenu()
; 命令框中文输入 + 八角框移除 (CONTRACTS §3.11/§3.12 v4):
; 八角框由 exe 数据 patch 移除 (keycap 白名单串 -> U+0001, 字母走普通字形路径);
; 中文输入由透传 hook 承担: hook 恒 V, 物理键透传 -> 英文原生显示 / IME 原生组合上屏;
; 投递通道整体关闭 (SuppressKeycap), providers 照常派发。
; 本注册即透传模式的启闭开关 (不查任何 IME 状态 —— 跨进程查询已整体证伪, 见 §3.12)。
; 若要回到「命令框历史行为」(吞键 + 投递显示), 注释掉下面两行即可 (零其它改动)。
CommandInputHooks.Register(ImeInputHost)
ImeInputHost.Enable()
; 命令框会话内「锁英文」(CommandImeGuard): 在 OnSessionBegin 强制历史形态(吞键+投递英文,
; 不使用输入法), 从而英文态弹框不会变中文, Shift 也无中文可切。注册顺序必须在
; ImeInputHost 之后(后生效, 覆盖其透传标志)。插件无关, 可注释此行整体关闭。
; 见 docs/design-ime-guard.md。
CommandInputHooks.Register(CommandImeGuard)
InitKeymap()
"#;

/// L63-L68（到 `taskSwitch := ...`，末尾带换行）。
const INITKEYMAP_HEAD: &str = r#"OnExit(KeyFluxExit)
#include ../data/custom_functions.ahk

InitKeymap()
{
  taskSwitch := TaskSwitchKeymap("e", "d", "s", "f", "c", "space")
"#;

/// L99-L121（ExecCapslockAbbr true 分支前半，末尾带换行；随后接动态 CapslockAbbrKeys）。
const CAPS_TRUE_HEAD: &str = r#"ExecCapslockAbbr(command) {
  CommandResolver.Resolve("capslock", command)
}

/**
 * 动态创建 CapsLock 命令框 InputHook (每会话一次, 在 EnterCapslockAbbr 内调用)。
 *
 * 🔴 为什么动态: InputHook 对象一次性, 每会话新建。可见性按透传开关二态:
 *   透传模式 (SuppressKeycap=true, ImeInputHost 启用时恒如此) => InputHook("V"):
 *   物理键透传到命令框窗口 —— 英文字母原生显示; 上屏中文以 WM_CHAR 直达 (非白名单,
 *   无框)。历史形态 (false) => InputHook(""): 吞文本键, 显示靠投递 (数据 patch 后无八角框)。
 *
 * 词表 (v4.2 恢复, 两形态共用): 设置面板的命令全部由英文字母组成, 且命令框内需要
 * 输入中文的唯一场景是前置键 (如空格) 触发插件之后 —— 那时字符已被插件 OnChar
 * 消费 (DispatchChar 提前 return), 到不了匹配层; MatchList 为**全串匹配**, 搜索期
 * 的 Input 形如 " se" (带空格前缀) ≠ "se", 永不误触发。故 v4 的「透传词表置空」
 * 废除, 两形态同词表: 全串命中走 Match 分支 (ExecCapslockAbbr), 带前缀后缀命中走
 * FuzzySuffixFire —— 双通道行为与历史形态完全一致。
 * 时序: EnterCapslockAbbr 先 BeginSession() (OnSessionBegin 已置 SuppressKeycap),
 * 再调本函数, 此刻读该标志即拿到正确形态。
 */
MakeCapsHook() {
  ih := InputHook(CommandDisplay.SuppressKeycap ? "V" : "", "{CapsLock}{Esc}"
"#;

/// L123-L138（ExecCapslockAbbr true 分支后半，**无**末尾换行：L138 换行被 `{{- else -}}` 吃掉）。
const CAPS_TRUE_TAIL: &str = r#"  ih.KeyOpt("{CapsLock}", "S")
  ih.KeyOpt("{Esc}", "S")
  ; S = V 模式下抑制透传 (EndKey 默认透传): CapsLock 防切大小写状态, Esc 防触发 exe
  ; 原生行为; EndKey 检测不受 S 影响, 会话照常结束。历史形态下 S 无副作用 (本来就吞)。
  ih.KeyOpt("{Backspace}", "N")
  ; 🔴 Up/Down/Enter/Backspace 在 V 模式下必须保持透传 (只有 N 无 S): IME 组合期它们是
  ; 选候选/翻页/确认拼音原文/删组合串的原生操作, 吞掉即毁 IME 交互。
  ; N 仅保留 OnKeyDown 通知 (供插件下拉列表导航, 由 CommandInputHooks 分发);
  ; exe 是纯镜像显示窗口 (只处理 WM_CHAR), 对非字符键无原生行为, 透传噪声可忽略。
  ih.KeyOpt("{Up}", "N")
  ih.KeyOpt("{Down}", "N")
  ih.KeyOpt("{Enter}", "N")
  ih.OnChar := (ih2, char) => CommandInputOnChar(ih2, char, "capslock")
  ih.OnKeyDown := (ih2, vk, sc) => CommandInputOnKeyDown(ih2, vk, sc, "capslock")
  return ih
}"#;

/// L153-L166（InitTrayMenu 整段 + 末尾空行，末尾带换行）。
const TRAY_MENU: &str = r#"InitTrayMenu() {
  A_TrayMenu.Delete()
  A_TrayMenu.Add(Translation().menu_pause, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_exit, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_reload, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_settings, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_window_spy, TrayMenuHandler)
  A_TrayMenu.Default := Translation().menu_pause
  A_TrayMenu.ClickCount := 1

  A_IconTip := "KeyFlux"
  TraySetIcon("./bin/icons/logo.ico", , true)
}

"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{behaviors, config as config_parse};
    use std::path::PathBuf;

    /// 内置行为包目录（与部署树 `settings.exe` 同层的 `bin/behaviors`）。
    fn builtin_catalog() -> behaviors::Catalog {
        behaviors::load_catalog(
            &PathBuf::from("../bin/behaviors"),
            &PathBuf::from("../data/no-user-behaviors"),
            &[],
        )
    }

    fn parse(corpus: &str) -> Config {
        let path = PathBuf::from(format!("../tools/parity/corpus/{corpus}/config.json"));
        config_parse::parse_config(&path, "").expect("语料应可解析")
    }

    fn reference(name: &str) -> Vec<u8> {
        std::fs::read(format!("../tools/parity/reference/{name}"))
            .unwrap_or_else(|error| panic!("读取基线 {name} 失败: {error}"))
    }

    fn render_keyflux(corpus: &str, plugins_dir: &str) -> Vec<u8> {
        let mut config = parse(corpus);
        config_parse::preprocess(&mut config);
        let catalog = builtin_catalog();
        render_keyflux_ahk(&mut config, Some(&catalog), Path::new(plugins_dir)).into_bytes()
    }

    /// 出厂语料：与 Go 基线逐字节相同（BOM + CRLF + 全部字段/缩写/窗口组/KeyMapping）。
    #[test]
    fn keyflux_factory_matches_go_reference() {
        assert_eq!(
            render_keyflux("factory", "../data/no-plugins-here"),
            reference("factory.keyflux.ahk")
        );
    }

    /// 全矩阵语料：9 个 TypeID / 缩写 ct5 / hotifHeader 0-5 / 选中动作。
    #[test]
    fn keyflux_synthetic_matches_go_reference() {
        assert_eq!(
            render_keyflux("synthetic", "../data/no-plugins-here"),
            reference("synthetic.keyflux.ahk")
        );
    }

    /// 插件注入（示例插件目录直接复用仓库内 `plugins/examples`，与 harness 拷贝等价）。
    #[test]
    fn keyflux_factory_plugins_matches_go_reference() {
        assert_eq!(
            render_keyflux("factory", "../plugins/examples"),
            reference("factory-plugins.keyflux.ahk")
        );
    }

    /// 皮肤产物：**无** BOM + CRLF；空皮肤段走模板 else 默认字面量。
    #[test]
    fn skin_factory_matches_go_reference() {
        let config = parse("factory");
        assert_eq!(
            render_command_input_skin(&config).into_bytes(),
            reference("factory.skin.txt")
        );
    }

    /// 皮肤产物不含 BOM（与 keyflux 不同：模板本身无 BOM）。
    #[test]
    fn skin_has_no_bom() {
        let config = parse("factory");
        let bytes = render_command_input_skin(&config).into_bytes();
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    }

    /// keyflux 产物带 BOM（模板首字符 U+FEFF）。
    #[test]
    fn keyflux_has_bom() {
        let bytes = render_keyflux("factory", "../data/no-plugins-here");
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    }

    /// 回归（corpus 盲区）：capslock 缩写关闭时，`keyflux.tmpl` L88 的 `{{ if .CapslockAbbrEnabled }}`
    /// 整段（含其尾随换行）被 Go 跳过。曾有 bug 把该换行无条件输出 ⇒ 多 1 空行（+2 字节 CRLF）；
    /// 3 条基线语料的 `CapslockAbbrEnabled` 全为 true，故 corpus / parity 从未覆盖这条 skip 路径。
    ///
    /// 锚点：关闭 capslock 后，semicolon 注册段紧接 pathVariables 段（两者间恰好 1 个空行）；
    /// 若多输出 1 个换行，注册行前会出现**连续 2 个空行**。
    #[test]
    fn capslock_disabled_skips_comment_block_without_extra_blank_line() {
        let mut config = parse("factory");
        // keymap 5 承载 TypeID9/ValueID6（capslock 缩写触发）；关闭它使 CapslockAbbrEnabled 为 false。
        for keymap in &mut config.keymaps {
            if keymap.id == 5 {
                keymap.enable = false;
            }
        }
        assert!(!config.capslock_abbr_enabled(), "探针应关闭 capslock 缩写");

        config_parse::preprocess(&mut config);
        let catalog = builtin_catalog();
        let out = render_keyflux_ahk(
            &mut config,
            Some(&catalog),
            Path::new("../data/no-plugins-here"),
        );
        let text = out.strip_prefix('\u{feff}').unwrap_or(&out);

        // capslock 注释块与 capslock 注册段都应整体消失（Go 跳过整个 if 体）
        assert!(
            !text.contains("缩写命令注册表"),
            "capslock 关闭时不应输出注释块"
        );
        assert!(
            !text.contains("CommandResolver.Register(\"capslock\""),
            "capslock 关闭时不应注册 capslock 缩写"
        );

        // 回归锚点：semicolon 注册行前不得出现连续空行
        let lines: Vec<&str> = text.split("\r\n").collect();
        let idx = lines
            .iter()
            .position(|line| line.starts_with("  CommandResolver.Register(\"semicolon\""))
            .expect("应存在 semicolon 注册段");
        assert!(
            !lines[idx - 2].trim().is_empty(),
            "semicolon 注册行前出现连续空行（回归：capslock if 体的换行被无条件输出）: {:?}",
            &lines[idx - 2..idx]
        );
    }
}
