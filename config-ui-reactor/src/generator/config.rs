//! `ParseConfig` / `SaveConfigFile` 的移植 —— Go `internal/script/config.go`（157 行）。
//!
//! ⚠️ 加载**必须**走 [`parse_config`]（内部经 `model::config_from_json` 容忍 `null`），
//! 不要直接 `serde_json::from_str::<Config>`。
//!
//! 默认值口径（Go 的两条"整段为零才补齐"规则，不可改成逐字段补齐）：
//! * `commandInputSkin` **整段为零** ⇒ 填 [`default_command_input_skin`]（18 字段）；
//! * `quickSwitch` **全零签名** ⇒ 填 [`default_quick_switch_option`]；
//! * `mouse.tipSymbol` 为空 ⇒ `"🐶"`。
//!
//! 未移植：`MigrateSelectedAction` 的**旧 `actionSchemes` 迁移分支**（需要时才做；
//! 当前遇到非空 `actionSchemes` 且无 `selectedAction` 会**显式报错**，绝不静默降级）。

use std::io;
use std::path::Path;

use crate::generator::model::{
    Action, CommandInputSkin, Config, Keymap, QuickSwitchOption, config_from_json,
};

/// Go `script.ConfigRelPath`：运行时配置文件落点（相对进程 cwd，即部署树的 `bin/`）。
pub const CONFIG_REL_PATH: &str = "../data/config.json";

/// Go `script.DefaultCommandInputSkin`：命令输入窗口皮肤 18 字段默认值。
///
/// 字面量必须与 `templates/CommandInputSkin.tmpl` 头部的 else 兜底一致
/// （Go 侧有 `skin_defaults_test.go` 逐字段守护）。
pub fn default_command_input_skin() -> CommandInputSkin {
    CommandInputSkin {
        background_color: "#FFFFFF".into(),
        background_opacity: "0.9".into(),
        border_width: "3".into(),
        border_color: "#FFFFFF".into(),
        border_opacity: "1.0".into(),
        border_radius: "10".into(),
        corner_color: "#000000".into(),
        corner_opacity: "0.0".into(),
        gridline_color: "#2843AD".into(),
        gridline_opacity: "0.04".into(),
        key_color: "#000000".into(),
        key_opacity: "1.0".into(),
        hide_animation_duration: "0.34".into(),
        window_y_pos: "0.25".into(),
        window_width: "700".into(),
        window_shadow_color: "#000000".into(),
        window_shadow_opacity: "0.5".into(),
        window_shadow_size: "3.0".into(),
    }
}

/// Go `script.DefaultQuickSwitchOption`（三端一致，由 Go/AHK/C# 三侧单测守护）。
pub fn default_quick_switch_option() -> QuickSwitchOption {
    QuickSwitchOption {
        collect_enabled: true,
        auto_show: true,
        auto_jump_open: true,
        auto_jump_save: false,
        poll_interval_ms: 800,
        max_history: 200,
        overlay_rows: 8,
        overlay_rows_compact: 4,
        excluded_prefixes: Vec::new(),
    }
}

/// Go `isQuickSwitchZero`：逐字段判定「旧配置缺失该段」的全零签名
/// （含切片故不能用 `==`）—— 绝不逐字段补齐，否则会把用户合法的 `false` 覆盖掉。
fn is_quick_switch_zero(option: &QuickSwitchOption) -> bool {
    !option.collect_enabled
        && !option.auto_show
        && !option.auto_jump_open
        && !option.auto_jump_save
        && option.poll_interval_ms == 0
        && option.max_history == 0
        && option.overlay_rows == 0
        && option.overlay_rows_compact == 0
        && option.excluded_prefixes.is_empty()
}

/// Go `Preprocess`：向 `ID == 1` 的模式注入隐藏全局热键 `!f17`（免疫 suspend）。
///
/// **任何**生成/计划路径都必须调用，否则产物与运行时不一致。注入是幂等赋值。
pub fn preprocess(config: &mut Config) {
    for keymap in config.keymaps.iter_mut() {
        if keymap.id == 1 {
            keymap.hotkeys.insert(
                "!f17".to_string(),
                vec![Action {
                    type_id: 9,
                    value_id: 2,
                    ..Default::default()
                }],
            );
            return;
        }
    }
}

/// Go `script.ParseConfig`（`keyfluxVersion` 在 Go 由 `-ldflags -X` 注入，此处作参数传入）。
pub fn parse_config(path: &Path, keyflux_version: &str) -> io::Result<Config> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read file {}: {error}", path.display()),
        )
    })?;
    let mut config: Config = config_from_json(&raw).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cannot parse config: {error}"),
        )
    })?;

    config.options.keyflux_version = keyflux_version.to_string();
    if config.options.mouse.tip_symbol.is_empty() {
        config.options.mouse.tip_symbol = "🐶".to_string();
    }
    if config.options.command_input_skin == CommandInputSkin::default() {
        config.options.command_input_skin = default_command_input_skin();
    }
    if is_quick_switch_zero(&config.options.quick_switch) {
        config.options.quick_switch = default_quick_switch_option();
    }

    // 存量迁移（Go `MigrateSelectedAction`）：新契约已存在 ⇒ 旧段废弃不再输出。
    match config.selected_action.as_ref() {
        Some(_) => config.action_schemes.clear(),
        None if config.action_schemes.is_empty() => {
            config.selected_action = Some(Default::default());
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "旧 actionSchemes -> selectedAction 存量迁移尚未移植（配置只有旧段）",
            ));
        }
    }

    Ok(config)
}

/// Go `script.SaveConfigFile`：全量落盘 + **不转义 HTML**（`SetEscapeHTML(false)`）+ 2 空格缩进 + 尾换行。
///
/// 与 [`crate::generator::plan`] 的 `WritePlan` 相反：后者要 Go 默认的 HTML 转义，此处**不要**。
///
/// 落盘改为「同目录临时文件 -> 刷盘 -> 关闭 -> rename」**原子替换**（对齐 Go
/// `script.saveConfigFileTo` 与 `internal/plugins/store.go` 的 `writeAll`）：直接 `fs::write`
/// 是截断写，引擎在写入途中读到就是半截 JSON。同卷 rename 为原子替换，读者要么看到旧内容
/// 要么看到新内容。**落盘字节与改前逐字节一致**（编码口径未动）。
pub fn save_config_file(config: &Config, path: &Path) -> io::Result<()> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write_atomic(path, format!("{json}\n").as_bytes())
}

/// 进程内自增序号：与 pid / 时间戳拼出临时文件名，避免并发保存互相覆盖。
static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 在 `dir` 下创建唯一临时文件（`create_new` 保证不覆盖既有文件）。
fn create_temp_file(
    dir: &Path,
    target_name: &str,
) -> io::Result<(std::fs::File, std::path::PathBuf)> {
    for _ in 0..16 {
        let seq = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let candidate = dir.join(format!(
            ".{target_name}.{}.{seq}.{nanos}.tmp",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((file, candidate)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "无法分配唯一的临时文件名",
    ))
}

/// 原子写入：临时文件落在目标同目录（保证 rename 同卷）→ 写入 → 刷盘 → 关闭 → rename。
/// 失败路径清理临时文件；成功 rename 后目标路径即为新内容。
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write as _;

    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    };
    let target_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "目标路径缺少文件名"))?
        .to_string_lossy()
        .into_owned();

    let (mut file, tmp_path) = create_temp_file(&dir, &target_name)?;
    let write_result = file.write_all(data).and_then(|()| file.sync_all());
    // 必须先关闭句柄再 rename（Windows 不允许替换仍被占用的文件）。
    drop(file);
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error);
    }
    Ok(())
}

/// 供调用方构造最小配置（测试/迁移用）。
pub fn empty_keymap(id: i32) -> Keymap {
    Keymap {
        id,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("kf-cfg-{}-{name}.json", std::process::id()));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn fills_skin_and_quickswitch_defaults_only_when_whole_section_zero() {
        let path = write_temp("defaults", r#"{"keymaps":[]}"#);
        let config = parse_config(&path, "1.0.0").unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            config.options.command_input_skin,
            default_command_input_skin()
        );
        assert!(config.options.quick_switch.collect_enabled);
        assert_eq!(config.options.quick_switch.poll_interval_ms, 800);
        assert_eq!(config.options.mouse.tip_symbol, "🐶");
        assert_eq!(config.options.keyflux_version, "1.0.0");
    }

    #[test]
    fn quick_switch_filled_only_on_all_zero_signature() {
        // (a) 全零签名（旧配置缺失该段）⇒ 补齐默认
        let all_zero = write_temp(
            "qs-zero",
            r#"{"keymaps":[],"options":{"quickSwitch":{"collectEnabled":false,"autoShow":false,"autoJumpOpen":false,"autoJumpSave":false,"pollIntervalMs":0,"maxHistory":0,"overlayRows":0,"overlayRowsCompact":0,"excludedPrefixes":[]}}}"#,
        );
        let filled = parse_config(&all_zero, "").unwrap();
        let _ = std::fs::remove_file(&all_zero);
        assert!(
            filled.options.quick_switch.collect_enabled,
            "全零签名应补齐默认"
        );
        assert_eq!(filled.options.quick_switch.poll_interval_ms, 800);

        // (b) 部分设置（此处只有 collectEnabled=true，其余仍为零）⇒ **原样保留**，
        //     Go 侧口径是「整段为零才补齐」，故不能把剩余字段填成默认值。
        let partial = write_temp(
            "qs-partial",
            r#"{"keymaps":[],"options":{"quickSwitch":{"collectEnabled":true,"autoShow":false,"autoJumpOpen":false,"autoJumpSave":false,"pollIntervalMs":0,"maxHistory":0,"overlayRows":0,"overlayRowsCompact":0,"excludedPrefixes":[]}}}"#,
        );
        let kept = parse_config(&partial, "").unwrap();
        let _ = std::fs::remove_file(&partial);
        assert!(kept.options.quick_switch.collect_enabled);
        assert_eq!(
            kept.options.quick_switch.poll_interval_ms, 0,
            "部分设置不得被补齐"
        );
        assert_eq!(kept.options.quick_switch.max_history, 0);
        assert!(kept.options.quick_switch.excluded_prefixes.is_empty());
    }

    #[test]
    fn preprocess_injects_hidden_hotkey_into_id1() {
        let mut config = Config {
            keymaps: vec![empty_keymap(1), empty_keymap(5)],
            ..Default::default()
        };
        preprocess(&mut config);
        let injected = &config.keymaps[0].hotkeys["!f17"];
        assert_eq!(injected.len(), 1);
        assert_eq!((injected[0].type_id, injected[0].value_id), (9, 2));
        assert!(config.keymaps[1].hotkeys.is_empty(), "只注入 ID==1");

        // 幂等
        preprocess(&mut config);
        assert_eq!(config.keymaps[0].hotkeys["!f17"].len(), 1);
    }

    #[test]
    fn legacy_action_schemes_is_rejected_not_silently_dropped() {
        let path = write_temp(
            "legacy",
            r#"{"keymaps":[],"actionSchemes":[{"id":1,"name":"old","hotkey":"^!a","enable":true,"rules":[]}]}"#,
        );
        let error = parse_config(&path, "").unwrap_err();
        let _ = std::fs::remove_file(&path);
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    /// SaveConfigFile 原子化后：①字节与编码口径逐字节一致（HTML 不转义 + 尾换行）
    /// ②不残留 *.tmp ③覆盖已存在文件成功。固化字面量含 `<>&` 与中文，锚定编码口径。
    #[test]
    fn save_config_file_is_atomic_and_byte_stable() {
        let dir = std::env::temp_dir().join(format!("kf-cfg-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let config = Config {
            keymaps: vec![Keymap {
                id: 1,
                name: "网页<>&\"引号\"".into(),
                enable: true,
                hotkey: "^!a".into(),
                ..Default::default()
            }],
            overview_doc_md: "# 指南\n编辑 <b>配置</b> & 保存\n".into(),
            ..Default::default()
        };
        // 「旧实现」= to_string_pretty + 尾换行（本函数改前口径）
        let expected = format!("{}\n", serde_json::to_string_pretty(&config).unwrap());
        assert!(!expected.contains("\\u003c"), "HTML 不应被转义: {expected}");

        // ③ 覆盖已存在文件
        std::fs::write(&path, "旧内容应被整体替换").unwrap();
        save_config_file(&config, &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        assert!(expected.ends_with("}\n"), "缺少尾换行");

        // ② 不残留 *.tmp（目录内只应有目标文件）
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["config.json".to_string()], "残留临时文件");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
