//! 动作渲染器 —— Go `internal/script/generators` 的同名移植。
//!
//! 覆盖源码：
//! * `generators.go`：`ActionMap`（TypeID → 渲染函数）、`ActionToHotkey`、`renderKeymap`、
//!   `GroupDisableKeyFlux`（`divide`/`concat`/`join`/`escapeAhkHotkey`/`substr`/
//!   `containsOnlyModifier`/`not_blank_lines` 已在 [`crate::generator::text`]，此处复用）；
//! * `type1_activate_or_run.go` … `type9_keyflux.go`：9 个 TypeID 渲染函数；
//! * `abbr_registry.go`：`AbbrRegistryCode`；
//! * `actionscheme.go`：`selectedActionCode`。
//!
//! ⚠️ **跨语言关键陷阱（对账夹具已钉住）**：
//! 1. Go 用包级全局 `generators.Cfg`；本移植**显式传参** `&Config`（迁移纪律：不引入全局
//!    可变态）。故 [`action_to_hotkey`] / [`render_keymap`] / [`abbr_registry_code`] 比 Go
//!    多一个 `config` 参数 —— 这是唯一有意的签名偏差。
//! 2. `renderKeymap` 内部把换行统一成 `\r\n`（先 `\r\n`→`\n` 再 `\n`→`\r\n`）；但
//!    `AbbrRegistryCode` / `selectedActionCode` **不做**该转换（CRLF 由 `SaveAHK` 模板层统一
//!    处理）—— 故本移植同样只在 [`render_keymap`] 里做。
//! 3. `remapKey5` 的 `ctx[2:]` 与 `sendKeys6` 的 `line[4:]`/`line[6:]` 是**字节切片**：仅因
//!    前缀恒为 ASCII（`", "` / `"ahk:"` / `"sleep "`）才安全，此处逐字对齐。
//! 4. `windowActions3` 的 ValueID 4/14 **无视** `inAbbrContext`（恒出完整注册行）；
//!    `mouseActions4` **完全不使用** `inAbbrContext`（快慢双速恒注册两行）。
//! 5. Go `%t` 打印 bool 为 `true`/`false`；Rust `{}` 同形，可直接对齐。
//! 6. `model.Action.RemapInHotIf` 在 Go 是 `json:"-"`（不落盘）⇒ 夹具无法经 JSON 携带它，
//!    故导出测试用旁挂字段 `remapInHotIf` 记录、Rust 单测读回后再赋值（见 tests）。
//!
//! 对账夹具再生成：
//!
//! ```text
//! cd config-server && UPDATE_ACTION_FIXTURE=1 go test ./internal/script/ -run TestExportActionRender
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::generator::behaviors::{self, Catalog};
use crate::generator::model::{
    Action, Config, Keymap, SelectedAction, WindowGroup, group_to_win_tile,
};
use crate::generator::plan::{SELECTED_ACTION_KEY_CAP, sort_actions, sort_hotkeys};
use crate::generator::text::{ahk_string, contains_only_modifier, divide, to_ahk_func_arg};

/// Go `generators.ActionToHotkey`（= `ActionMap[TypeID](action, inAbbrContext)`）。
///
/// 未注册的 TypeID（非 `1..=9`）返回空串（与 Go `ActionMap` 未命中一致）。
///
/// ⚠️ 与 Go 的差异：Go 从全局 `Cfg` 取渲染上下文，本移植显式传入 `config`。
pub fn action_to_hotkey(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    match action.type_id {
        1 => activate_or_run1(config, action, in_abbr_context),
        2 => system_actions2(config, action, in_abbr_context),
        3 => window_actions3(config, action, in_abbr_context),
        4 => mouse_actions4(config, action, in_abbr_context),
        5 => remap_key5(config, action, in_abbr_context),
        6 => send_keys6(config, action, in_abbr_context),
        7 => text_features7(config, action, in_abbr_context),
        8 => builtin_functions8(config, action, in_abbr_context),
        9 => keyflux_actions9(config, action, in_abbr_context),
        _ => String::new(),
    }
}

// --------------------------------------------------------------------------- TypeID 1

/// Go `type1_activate_or_run.go` 的 `winTitleWarning`：裸写 `xxx.exe` 会被 AHK 当作窗口
/// 标题子串匹配而永远失败，返回警告注释（空串 = 无警告）。
fn win_title_warning(win_title: &str) -> String {
    if win_title.is_empty()
        || win_title.starts_with("ahk_")
        || win_title.starts_with("ahk-expression:")
    {
        return String::new();
    }
    // 组合串 "标题 ahk_exe 名.exe" 含 ahk_ 条件, 放行
    if win_title.contains(" ahk_") {
        return String::new();
    }
    if win_title.to_lowercase().ends_with(".exe") {
        return format!(
            "; [配置警告] winTitle \"{win_title}\" 以 .exe 结尾, 会被当作窗口标题匹配而永远失败, 应写 \"ahk_exe {win_title}\""
        );
    }
    String::new()
}

/// Go `activateOrRun1`（TypeID 1）：激活或运行程序/路径。
fn activate_or_run1(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    let ctx = config.get_hotkey_context(action);
    let win_title = to_ahk_func_arg(&action.win_title);
    let target = to_ahk_func_arg(&action.target);
    let args = to_ahk_func_arg(&action.args);
    let working_dir = to_ahk_func_arg(&action.working_dir);

    let mut call = format!(
        "ActivateOrRun({win_title}, {target}, {args}, {working_dir}, {}, {}, {})",
        action.run_as_admin, action.detect_hidden_window, action.run_in_background
    );
    // 简化形态：args/workingDir 皆空且三开关全 false（Go 用 %t 打印 bool）
    if args == "\"\""
        && working_dir == "\"\""
        && !action.run_as_admin
        && !action.detect_hidden_window
        && !action.run_in_background
    {
        call = format!("ActivateOrRun({win_title}, {target})");
    }

    if in_abbr_context {
        return call;
    }

    let hotkey = &action.hotkey;
    let code = format!("km.Map(\"{hotkey}\", _ => {call}{ctx})");
    let warn = win_title_warning(&action.win_title);
    if warn.is_empty() {
        code
    } else {
        format!("{warn}\n{code}")
    }
}

// --------------------------------------------------------------------------- TypeID 2

/// Go `systemActions2`：系统控制动作。
fn system_actions2(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    let call = match action.value_id {
        1 => "SystemLockScreen()",
        2 => "SystemSleep()",
        3 => "SystemShutdown()",
        4 => "SystemReboot()",
        5 => "SoundControl()",
        6 => "BrightnessControl()",
        7 => "SystemRestartExplorer()",
        8 => "CopySelectedAsPlainText()",
        9 => "MuteActiveApp()",
        10 => "ShowActiveProcessInFolder()",
        _ => return String::new(),
    };
    if in_abbr_context {
        return call.to_string();
    }
    let hotkey = &action.hotkey;
    let ctx = config.get_hotkey_context(action);
    format!("km.Map(\"{hotkey}\", _ => {call}{ctx})")
}

// --------------------------------------------------------------------------- TypeID 3

/// Go `windowActions3`：窗口操作。
///
/// ⚠️ ValueID 4（taskSwitch）/ 14（BindWindow）是特殊完整行，**无视** `in_abbr_context`。
fn window_actions3(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    let hotkey = &action.hotkey;
    let ctx = config.get_hotkey_context(action);

    if action.value_id == 4 {
        return format!("km.Map(\"{hotkey}\", _ => Send(\"^!{{tab}}\"), taskSwitch{ctx})");
    }
    if action.value_id == 14 {
        return format!("km.Map(\"{hotkey}\", BindWindow(){ctx})");
    }

    let call = match action.value_id {
        1 => "SmartCloseWindow()",
        2 => "GoToLastWindow()",
        3 => "LoopRelatedWindows()",
        5 => "GoToPreviousVirtualDesktop()",
        6 => "GoToNextVirtualDesktop()",
        7 => "MoveWindowToNextMonitor()",
        8 => "MinimizeWindow()",
        9 => "MaximizeWindow()",
        10 => "CenterAndResizeWindow(1200, 800)",
        11 => "CenterAndResizeWindow(1370, 930)",
        12 => "ToggleWindowTopMost()",
        13 => "MakeWindowDraggable()",
        15 => "CloseWindowProcesses()",
        16 => "CloseSameClassWindows()",
        _ => return String::new(),
    };
    if in_abbr_context {
        return call.to_string();
    }
    format!("km.Map(\"{hotkey}\", _ => {call}{ctx})")
}

// --------------------------------------------------------------------------- TypeID 4

/// Go `mouseActions4`：鼠标动作（快慢双速注册）。
///
/// ⚠️ Go 版**从不使用** `inAbbrContext`（快慢两行恒注册），故此处以 `_in_abbr_context`
/// 显式标注未使用。
fn mouse_actions4(config: &Config, action: &Action, _in_abbr_context: bool) -> String {
    let hotkey = &action.hotkey;

    // ValueID 1..=4：移动鼠标，携带窗口守卫后缀（无守卫时后缀为空）。
    if (1..=4).contains(&action.value_id) {
        let (win_title, condition_type) = config.get_win_title(action);
        let mut fast_suffix = format!(", {win_title}, {condition_type}");
        let mut slow_suffix = format!(", , {win_title}, {condition_type}");
        if win_title == "\"\"" && condition_type == 0 {
            fast_suffix = String::new();
            slow_suffix = String::new();
        }
        let dir = match action.value_id {
            1 => "Up",
            2 => "Down",
            3 => "Left",
            _ => "Right", // value_id == 4
        };
        return format!(
            "km.Map(\"{hotkey}\", fast.MoveMouse{dir}, slow{fast_suffix}), slow.Map(\"{hotkey}\", slow.MoveMouse{dir}{slow_suffix})"
        );
    }

    // ValueID >= 5：滚轮/按键/输入光标，携带 HotkeyContext。
    let ctx = config.get_hotkey_context(action);
    match action.value_id {
        5 => format!(
            "km.Map(\"{hotkey}\", fast.ScrollWheelUp{ctx}), slow.Map(\"{hotkey}\", slow.ScrollWheelUp{ctx})"
        ),
        6 => format!(
            "km.Map(\"{hotkey}\", fast.ScrollWheelDown{ctx}), slow.Map(\"{hotkey}\", slow.ScrollWheelDown{ctx})"
        ),
        7 => format!(
            "km.Map(\"{hotkey}\", fast.ScrollWheelLeft{ctx}), slow.Map(\"{hotkey}\", slow.ScrollWheelLeft{ctx})"
        ),
        8 => format!(
            "km.Map(\"{hotkey}\", fast.ScrollWheelRight{ctx}), slow.Map(\"{hotkey}\", slow.ScrollWheelRight{ctx})"
        ),
        9 => format!(
            "km.Map(\"{hotkey}\", fast.LButton(){ctx}), slow.Map(\"{hotkey}\", slow.LButton(){ctx})"
        ),
        10 => format!(
            "km.Map(\"{hotkey}\", fast.RButton(){ctx}), slow.Map(\"{hotkey}\", slow.RButton(){ctx})"
        ),
        11 => format!(
            "km.Map(\"{hotkey}\", fast.MButton(){ctx}), slow.Map(\"{hotkey}\", slow.MButton(){ctx})"
        ),
        12 => format!(
            "km.Map(\"{hotkey}\", fast.LButtonDown(){ctx}), slow.Map(\"{hotkey}\", slow.LButtonDown(){ctx})"
        ),
        13 => format!(
            "km.Map(\"{hotkey}\", _ => MoveMouseToCaret(){ctx}), slow.Map(\"{hotkey}\", _ => MoveMouseToCaret(){ctx})"
        ),
        _ => String::new(),
    }
}

// --------------------------------------------------------------------------- TypeID 5

/// Go `remapKey5`：重映射按键。
fn remap_key5(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    if action.hotkey.to_lowercase() == "singlepress" {
        // 单键映射退化为发送按键 {blind}{RemapToKey}
        let mut cloned = action.clone();
        cloned.keys_to_send = format!("{{blind}}{{{}}}", action.remap_to_key);
        return send_keys6(config, &cloned, in_abbr_context);
    }
    let key = action.hotkey.trim_start_matches('*');
    let mut ctx = config.get_hotkey_context(action);
    if !ctx.is_empty() {
        // Go `ctx[2:]`：去掉前导 ", "（GetHotkeyContext 恒为 "" 或 ", , ..." ⇒ 下标 2 必为边界）
        ctx = ctx[2..].to_string();
    }
    let func = if action.remap_in_hot_if {
        "RemapInHotIf"
    } else {
        "RemapKey"
    };
    let remap = to_ahk_func_arg(&action.remap_to_key);
    format!("km.{func}(\"{key}\", {remap}{ctx})")
}

// --------------------------------------------------------------------------- TypeID 6

/// Go `sendKeys6`：发送按键（支持多行、`ahk:` 行、`sleep ` 行）。
fn send_keys6(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    let mut res: Vec<String> = Vec::new();
    for line in action.keys_to_send.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("ahk:") {
            res.push(rest.trim().to_string());
            continue;
        }
        if line.starts_with("sleep ") || line.starts_with("Sleep ") {
            // Go `line[6:]`：前缀是 6 个 ASCII 字节 ⇒ 下标 6 必为边界（且**不** trim）
            res.push(format!("Sleep({})", &line[6..]));
            continue;
        }
        let arg = to_ahk_func_arg(line);
        res.push(format!("Send({arg})"));
    }
    if res.is_empty() {
        return String::new();
    }

    let call = res.join(", ");
    if in_abbr_context {
        return call;
    }
    let hotkey = &action.hotkey;
    let ctx = config.get_hotkey_context(action);
    format!("km.Map(\"{hotkey}\", _ => ({call}){ctx})")
}

// --------------------------------------------------------------------------- TypeID 7

/// Go `textFeatures7`：文本编辑特征键（方向/选区/重映射变体）。
fn text_features7(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    // Type == "remap"：改写 RemapToKey 后走 remapKey5。Go 按值传参，故需克隆。
    if let Some(value) = remap_value(action.value_id) {
        let mut cloned = action.clone();
        cloned.remap_to_key = value.to_string();
        return remap_key5(config, &cloned, in_abbr_context);
    }
    // Type == "send"：改写 KeysToSend 后走 sendKeys6。
    if let Some(value) = send_value(action.value_id) {
        let mut cloned = action.clone();
        cloned.keys_to_send = value.to_string();
        return send_keys6(config, &cloned, in_abbr_context);
    }

    let call = match action.value_id {
        19 => "HoldDownModifierKey(\"LShift\")",
        29 => "InsertSpaceBetweenZHAndEn()",
        30 => "HoldDownModifierKey(\"LCtrl\")",
        31 => "HoldDownModifierKey(\"LAlt\")",
        32 => "HoldDownModifierKey(\"LWin\")",
        _ => return String::new(),
    };
    if in_abbr_context {
        return call.to_string();
    }
    let hotkey = &action.hotkey;
    let ctx = config.get_hotkey_context(action);
    format!("km.Map(\"{hotkey}\", _ => {call}{ctx})")
}

/// Go `textFeatures7` 里 `Type == "remap"` 的分支表。
fn remap_value(value_id: i32) -> Option<&'static str> {
    let value = match value_id {
        1 => "up",
        2 => "down",
        3 => "left",
        4 => "right",
        5 => "home",
        6 => "end",
        17 => "appskey",
        20 => "esc",
        21 => "backspace",
        23 => "delete",
        24 => "insert",
        25 => "tab",
        _ => return None,
    };
    Some(value)
}

/// Go `textFeatures7` 里 `Type == "send"` 的分支表。
fn send_value(value_id: i32) -> Option<&'static str> {
    let value = match value_id {
        7 => "{blind}^{left}",
        8 => "{blind}^{right}",
        9 => "{blind}+{up}",
        10 => "{blind}+{down}",
        11 => "{blind}+{left}",
        12 => "{blind}+{right}",
        13 => "{blind}+{home}",
        14 => "{blind}+{end}",
        15 => "^+{left}",
        16 => "^+{right}",
        18 => "^{backspace}",
        33 => "{home}+{end}{backspace}",
        22 => "{blind}{enter}",
        26 => "^{tab}",
        27 => "{blind}+{tab}",
        28 => "^+{tab}",
        _ => return None,
    };
    Some(value)
}

// --------------------------------------------------------------------------- TypeID 8

/// Go `builtinFunctions8`：内置函数 / 自定义 AHK 表达式（AHKCode 直出）。
fn builtin_functions8(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    if in_abbr_context {
        return action.ahk_code.clone();
    }
    let hotkey = &action.hotkey;
    let code = &action.ahk_code;
    let ctx = config.get_hotkey_context(action);
    format!("km.Map(\"{hotkey}\", _ => {code}{ctx})")
}

// --------------------------------------------------------------------------- TypeID 9

/// Go `keyfluxActions9`：KeyFlux 自身动作（暂停/重载/退出/设置/进缩写/大写锁定/锁定）
/// + 插件动作（P7b：actionId = `<pluginId>.<actionId>`，valueID 9 = 子类型标记）。
fn keyflux_actions9(config: &Config, action: &Action, in_abbr_context: bool) -> String {
    let mut ctx = config.get_hotkey_context(action);

    // P7b: 插件动作绑定（双字段过渡 —— actionId 渲染优先；valueID 9 继续写，
    // 供旧版本识别「插件动作」子类型）。核心对具体插件零知识：只按 "." 拆分转发，
    // 寻址由运行时动作注册表完成（插件缺席即静默失败，可删除性保证）。
    if !action.action_id.is_empty() {
        let Some((plugin_id, action_id)) = action.action_id.split_once('.') else {
            return String::new();
        };
        let call = format!("PluginAction(\"{plugin_id}\", \"{action_id}\")");
        if in_abbr_context {
            return call;
        }
        let hotkey = &action.hotkey;
        return format!("km.Map(\"{hotkey}\", _ => {call}{ctx})");
    }

    let call = match action.value_id {
        1 => "KeyFluxToggleSuspend()",
        2 => "KeyFluxReload()",
        3 => "KeyFluxExit()",
        4 => "KeyFluxOpenSettings()",
        5 => "EnterSemicolonAbbr(semiHook, semiHookAbbrWindow)",
        6 => "EnterCapslockAbbr()",
        7 => "ToggleCapslock()",
        8 => "km.ToggleLock",
        // 旧式 valueID 9 且无 actionId 的遗留绑定：生成端无法定位插件动作，跳过
        // （出厂配置 2026-09-23 起已无此绑定；用户重新绑定即写入 actionId）。
        _ => return String::new(),
    };

    if in_abbr_context {
        return call.to_string();
    }

    let hotkey = &action.hotkey;
    // ValueID 1/2（暂停/重载）：免疫 suspend，注册时补 "S" 选项；无 ctx 时补齐占位。
    if action.value_id == 1 || action.value_id == 2 {
        if ctx.is_empty() {
            ctx.push_str(", , , ");
        }
        return format!("km.Map(\"{hotkey}\", _ => {call}{ctx}, \"S\")");
    }
    // ValueID 8（锁定）：直接传方法引用（无 `_ =>` 包装）。
    if action.value_id == 8 {
        return format!("km.Map(\"{hotkey}\", {call}{ctx})");
    }
    format!("km.Map(\"{hotkey}\", _ => {call}{ctx})")
}

// --------------------------------------------------------------------------- 缩写注册表

/// Go `abbr_registry.go` 的 `AbbrRegistryCode`：把缩写表渲染为 `CommandResolver.Register`
/// 注册行。按缩写字典序；动作经 [`sort_actions`]（WindowGroupID==0 挪后）；跳过未注册
/// TypeID；仅 `WindowGroupID != 0` 的动作带窗口组守卫，`conditionType 5` 表达式去包裹单引号。
///
/// ⚠️ 与 Go 一致：本函数**不**做 CRLF 归一（交由 `SaveAHK` 模板层）。
pub fn abbr_registry_code(
    config: &Config,
    abbr_map: &BTreeMap<String, Vec<Action>>,
    scope: &str,
    indent: &str,
) -> String {
    let mut abbr_list: Vec<(String, Vec<Action>)> = abbr_map
        .iter()
        .map(|(abbr, actions)| (abbr.clone(), sort_actions(actions)))
        .collect();
    abbr_list.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    for (abbr, actions) in abbr_list {
        let mut steps: Vec<String> = Vec::new();
        for action in &actions {
            // 未注册 TypeID 跳过（与 Go `ActionMap` 未命中一致）
            if !(1..=9).contains(&action.type_id) {
                continue;
            }
            let call = action_to_hotkey(config, action, true);
            let mut step = format!("CommandStep(() => {call})");
            if action.window_group_id != 0 {
                let (win_title, condition_type) = config.get_win_title(action);
                let win_title = if condition_type == 5 {
                    win_title.trim_matches('\'').to_string()
                } else {
                    win_title
                };
                step = format!("CommandStep(() => {call}, {win_title}, {condition_type})");
            }
            steps.push(step);
        }
        if steps.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "{indent}CommandResolver.Register({}, {}, [{}])",
            ahk_string(scope),
            ahk_string(&abbr),
            steps.join(", ")
        );
    }
    out
}

// --------------------------------------------------------------------------- 选中动作

/// Go `actionscheme.go` 的 `selectedActionCode`：把 selectedAction（单键分发）渲染为 AHK 代码
/// （供 `keyflux.tmpl` 模板消费）。
///
/// 禁用 / 热键为空 / 未配置（`None`）时输出空串（与旧口径一致，保证出厂默认产物逐字节稳定）。
/// 同一 mapping 内 key 递增超过 9 的 entry 跳过并留注释告警（与 Go `selectedActionKeyCap` 同口径）。
///
/// ⚠️ 与 Go 一致：`catalog == None` 等价于 `BehaviorCatalog == nil`（内置 ID 直通、显示名回退 ID）。
pub fn selected_action_code(sa: Option<&SelectedAction>, catalog: Option<&Catalog>) -> String {
    let Some(sa) = sa else {
        return String::new();
    };
    if !sa.enable || sa.hotkey.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("  ; ===== 选中动作 (单键分发) =====\n");
    out.push_str("  ; SelectedActionData 每项字段: matchType (匹配类型, 输出顺序 = 匹配优先级) / matchValue (条件值) / key (菜单序号 1-9) /\n");
    out.push_str("  ;   behavior (行为库 ID) / action (展开后基础动作) / actionValue (展开后模板) / workingDir (工作目录) / name (显示名)\n");
    out.push_str("  SelectedActionData := Array(\n");
    for mapping in &sa.mappings {
        for (index, entry) in mapping.entries.iter().enumerate() {
            // 评审 L2：超 key cap 的 entry 跳过渲染，留注释告警供产物 diff 排查
            if index + 1 > SELECTED_ACTION_KEY_CAP {
                out.push_str("    ; [selected-action] entry beyond key cap 9 skipped\n");
                continue;
            }
            let (action, action_value, working_dir) = behaviors::resolve_rule_action(
                catalog,
                &entry.behavior,
                &entry.action_value,
                &entry.working_dir,
            );
            let _ = writeln!(
                out,
                "    {{matchType: {}, matchValue: {}, key: {}, behavior: {}, action: {}, actionValue: {}, workingDir: {}, name: {}}},",
                ahk_string(&mapping.match_type),
                ahk_string(&mapping.match_value),
                index + 1,
                ahk_string(&entry.behavior),
                ahk_string(&action),
                ahk_string(&action_value),
                ahk_string(&working_dir),
                ahk_string(&behaviors::behavior_name(catalog, &entry.behavior))
            );
        }
    }
    out.push_str("  )\n");
    let _ = writeln!(
        out,
        "  SelectedActionInit({}, SelectedActionData)",
        ahk_string(&sa.hotkey)
    );
    out
}

// --------------------------------------------------------------------------- 键位映射

/// Go `generators.go` 的 `renderKeymap`：渲染单个模式（`KeymapManager.NewKeymap` /
/// `AddSubKeymap` + 逐动作注册行）。
///
/// ⚠️ 与 Go 一致：产物内换行统一为 `\r\n`（先 `\r\n`→`\n` 再 `\n`→`\r\n`）。
pub fn render_keymap(km: &Keymap, config: &Config) -> String {
    if km.hotkey.trim().is_empty() {
        return String::new();
    }

    let mut buf = String::new();

    // ; Capslock + F
    let _ = writeln!(buf, "\n  ; {}", km.name);

    // km6 := KeymapManager.AddSubKeymap(km5, "*f", "Capslock + F")
    let mut line = format!("  km{} := KeymapManager.", km.id);
    let hotkey = if contains_only_modifier(&km.hotkey) {
        "customHotkeys".to_string()
    } else {
        km.hotkey.clone()
    };
    if km.parent_id == 0 {
        let _ = writeln!(
            line,
            "NewKeymap({}, {}, {}, {})",
            ahk_string(&hotkey),
            ahk_string(&km.name),
            ahk_string(&divide(km.delay as i64, 1000)),
            ahk_string(&config.get_keymap_disable_at(km.id))
        );
    } else {
        let _ = writeln!(
            line,
            "AddSubKeymap(km{}, {}, {}, {})",
            km.parent_id,
            ahk_string(&hotkey),
            ahk_string(&km.name),
            ahk_string(&divide(km.delay as i64, 1000))
        );
    }
    buf.push_str(&line);

    // km := km6
    let _ = writeln!(buf, "  km := km{}", km.id);

    for mut action in sort_hotkeys(&km.hotkeys) {
        // 纯修饰键模式：跳过 singlePress，其余把触发键与热键拼起来
        if contains_only_modifier(&km.hotkey) {
            if action.hotkey == "singlePress" {
                continue;
            }
            action.hotkey = format!("{}{}", km.hotkey, action.hotkey);
        }
        let _ = writeln!(buf, "  {}", action_to_hotkey(config, &action, false));
    }

    // 替换换行符为 \r\n（Go 先折叠再统一）
    buf.replace("\r\n", "\n").replace('\n', "\r\n")
}

// --------------------------------------------------------------------------- 窗口组

/// Go `generators.go` 的 `GroupDisableKeyFlux`：取 `ID == -1` 的窗口组转 `GroupToWinTile`；
/// 无则返回 `AhkString("")`（即 `""`）。
pub fn group_disable_keyflux(groups: &[WindowGroup]) -> String {
    for group in groups {
        if group.id == -1 {
            return group_to_win_tile(group);
        }
    }
    ahk_string("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::model::config_from_json;
    use serde::Deserialize;

    /// Go 侧导出的对账夹具（见模块头注的再生成命令）。
    #[derive(Deserialize)]
    struct Fixture {
        /// Go `syntheticConfig()` 的 JSON（两端共用同一份配置）。
        config: serde_json::Value,
        cases: Vec<ActionCase>,
        #[serde(rename = "abbrRegistry")]
        abbr_registry: Vec<AbbrCase>,
        #[serde(rename = "renderKeymap")]
        render_keymap: Vec<RenderKeymapCase>,
        #[serde(rename = "groupDisableKeyFlux")]
        group_disable_keyflux: Vec<GroupCase>,
        #[serde(rename = "selectedAction")]
        selected_action: Vec<SelectedCase>,
    }

    #[derive(Deserialize)]
    struct ActionCase {
        #[serde(rename = "in")]
        action: Action,
        /// Go `Action.RemapInHotIf` 是 `json:"-"` ⇒ 夹具旁挂记录，读回后再赋值。
        #[serde(rename = "remapInHotIf", default)]
        remap_in_hot_if: bool,
        #[serde(rename = "inAbbrContext")]
        in_abbr_context: bool,
        out: String,
    }

    #[derive(Deserialize)]
    struct AbbrCase {
        #[serde(rename = "in")]
        abbr_map: BTreeMap<String, Vec<Action>>,
        scope: String,
        indent: String,
        out: String,
    }

    #[derive(Deserialize)]
    struct RenderKeymapCase {
        #[serde(rename = "in")]
        keymap: Keymap,
        out: String,
    }

    #[derive(Deserialize)]
    struct GroupCase {
        /// Go 的 nil slice ⇒ JSON `null` ⇒ 读成 `None`（与 Go range nil 等价）。
        #[serde(rename = "in", default)]
        groups: Option<Vec<WindowGroup>>,
        out: String,
    }

    #[derive(Deserialize)]
    struct SelectedCase {
        #[serde(rename = "in", default)]
        selected: Option<SelectedAction>,
        out: String,
    }

    fn fixture() -> (Config, Fixture) {
        let path = "tests/fixtures/action_render.json";
        let raw = std::fs::read_to_string(path).unwrap_or_else(|error| {
            panic!(
                "读取对账夹具 {path} 失败: {error}\n\
                 重新生成: cd config-server && UPDATE_ACTION_FIXTURE=1 go test ./internal/script/ -run TestExportActionRender"
            )
        });
        // ⚠️ 跨语言陷阱：Go 的 nil map/slice 落进 JSON 是 `null`（如空 Keymap 的 `hotkeys: null`），
        // serde 不认 ⇒ 先剔除对象字段里的 null（等价于字段缺失 ⇒ 走 serde default），
        // 与 `config_from_json` 的容错口径一致。
        let mut value: serde_json::Value = serde_json::from_str(&raw).expect("夹具 JSON 解析失败");
        crate::generator::model::strip_null_fields(&mut value);
        let fixture: Fixture = serde_json::from_value(value).expect("夹具反序列化失败");
        // 配置经 config_from_json（容忍 Go 的 null ⇒ 零值），与真实加载路径一致
        let config_json = serde_json::to_string(&fixture.config).expect("配置再序列化失败");
        let config = config_from_json(&config_json).expect("夹具配置解析失败");
        (config, fixture)
    }

    /// 与 Go 实现逐值对账：`ActionToHotkey`（含 inAbbrContext 两态）、改写分支、未注册 TypeID。
    #[test]
    fn matches_go_action_render_fixture() {
        let (config, fixture) = fixture();
        assert!(!fixture.cases.is_empty(), "夹具应含动作用例");

        for case in &fixture.cases {
            let mut action = case.action.clone();
            action.remap_in_hot_if = case.remap_in_hot_if;
            assert_eq!(
                action_to_hotkey(&config, &action, case.in_abbr_context),
                case.out,
                "type={} value={} hotkey={:?} inAbbr={}",
                action.type_id,
                action.value_id,
                action.hotkey,
                case.in_abbr_context
            );
        }
    }

    /// 未注册 TypeID（0 / 42）恒空串（与 Go `ActionMap` 未命中一致）。
    #[test]
    fn unregistered_type_ids_render_empty() {
        let (config, _) = fixture();
        for type_id in [0, 42] {
            let action = Action {
                type_id,
                hotkey: "*a".into(),
                ..Default::default()
            };
            assert_eq!(action_to_hotkey(&config, &action, false), "");
            assert_eq!(action_to_hotkey(&config, &action, true), "");
        }
    }

    /// `AbbrRegistryCode` 逐值对账（字典序 / sortActions / 守卫 / ct5 去单引号 / 未注册跳过）。
    #[test]
    fn matches_go_abbr_registry_fixture() {
        let (config, fixture) = fixture();
        assert!(!fixture.abbr_registry.is_empty(), "夹具应含缩写用例");
        for case in &fixture.abbr_registry {
            assert_eq!(
                abbr_registry_code(&config, &case.abbr_map, &case.scope, &case.indent),
                case.out,
                "abbr scope={:?}",
                case.scope
            );
        }
    }

    /// `renderKeymap` 逐值对账（含 CRLF 归一 / 修饰键 customHotkeys / 子模式 AddSubKeymap）。
    #[test]
    fn matches_go_render_keymap_fixture() {
        let (config, fixture) = fixture();
        assert!(
            !fixture.render_keymap.is_empty(),
            "夹具应含 renderKeymap 用例"
        );
        for case in &fixture.render_keymap {
            assert_eq!(
                render_keymap(&case.keymap, &config),
                case.out,
                "renderKeymap id={} hotkey={:?}",
                case.keymap.id,
                case.keymap.hotkey
            );
        }
    }

    /// `GroupDisableKeyFlux` 逐值对账（含 nil slice ⇒ null 的容错）。
    #[test]
    fn matches_go_group_disable_keyflux_fixture() {
        let (_, fixture) = fixture();
        assert!(
            !fixture.group_disable_keyflux.is_empty(),
            "夹具应含窗口组用例"
        );
        for case in &fixture.group_disable_keyflux {
            let groups: &[WindowGroup] = case.groups.as_deref().unwrap_or(&[]);
            assert_eq!(group_disable_keyflux(groups), case.out);
        }
    }

    /// `selectedActionCode` 逐值对账（nil / 禁用 / 空热键 / key cap / 目录缺失回退）。
    #[test]
    fn matches_go_selected_action_fixture() {
        let (_, fixture) = fixture();
        assert!(!fixture.selected_action.is_empty(), "夹具应含选中动作用例");
        for case in &fixture.selected_action {
            assert_eq!(selected_action_code(case.selected.as_ref(), None), case.out);
        }
    }
}
