//! `ParseConfig` / `SaveConfigFile` 的移植 —— Go `internal/script/config.go`（157 行）。
//!
//! ⚠️ 加载**必须**走 [`parse_config`]（内部经 `model::config_from_json` 容忍 `null`），
//! 不要直接 `serde_json::from_str::<Config>`。
//!
//! 默认值口径（Go 的"整段为零才补齐"规则，不可改成逐字段补齐）：
//! * `commandInputSkin` **整段为零** ⇒ 填 [`default_command_input_skin`]（18 字段）；
//! * `mouse.tipSymbol` 为空 ⇒ `"🐶"`。
//!
//! （`quickSwitch` 段的默认回填已随 2026-10-02 兼容段移除批次删除：deprecated 段
//! 不再读取也不再输出。）
//!
//! 未移植：`MigrateSelectedAction` 的**旧 `actionSchemes` 迁移分支**（需要时才做；
//! 当前遇到非空 `actionSchemes` 且无 `selectedAction` 会**显式报错**，绝不静默降级）。

use std::io;
use std::path::Path;

use crate::generator::model::{Action, CommandInputSkin, Config, Keymap, config_from_json};

/// Go `script.ConfigRelPath`：运行时配置文件落点（相对进程 cwd，即部署树的 `bin/`）。
pub const CONFIG_REL_PATH: &str = "../data/config.json";

/// Go `script.DefaultCommandInputSkin`：命令输入窗口皮肤 18 字段默认值。
///
/// 字面量必须与另三处逐字段一致：消费端 `command-input/src/skin.rs` 的 `DEFAULT`、
/// [`crate::generator::template::render_command_input_skin`] 的 else 兜底字面量、
/// `templates/CommandInputSkin.tmpl` 头部的 else 兜底。
/// 守护 = `config-ui-reactor/tests/skin_contract.rs`（跨 crate 契约测试，2026-10-08 建立）。
/// 本注释此前写的是「Go 侧有 `skin_defaults_test.go` 逐字段守护」—— 那份 Go 单测已随
/// Go 后端于 2026-10-06 退役（`36ccb83`），两处注释成了引用不存在文件的失真守护，
/// 直到 `skin_contract.rs` 在 Rust 侧把这份守护重建（且面扩到键名 + 顺序 + `apply` 覆盖）。
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
    fn fills_skin_defaults_only_when_whole_section_zero() {
        let path = write_temp("defaults", r#"{"keymaps":[]}"#);
        let config = parse_config(&path, "1.0.0").unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            config.options.command_input_skin,
            default_command_input_skin()
        );
        assert_eq!(config.options.mouse.tip_symbol, "🐶");
        assert_eq!(config.options.keyflux_version, "1.0.0");
    }

    #[test]
    fn deprecated_quick_switch_section_is_ignored() {
        // 2026-10-02 兼容段移除：deprecated 段不再读取（未知键容忍）也不回填默认值。
        let path = write_temp(
            "qs-legacy",
            r#"{"keymaps":[],"options":{"quickSwitch":{"collectEnabled":true,"pollIntervalMs":800}}}"#,
        );
        let config = parse_config(&path, "").unwrap();
        let _ = std::fs::remove_file(&path);
        let json = serde_json::to_string(&config.options).unwrap();
        assert!(
            !json.contains("quickSwitch"),
            "deprecated 段不得出现在模型输出: {json}"
        );
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

    /// 键序确定化回归（改动前 `Keymap.hotkeys` 是 `HashMap`，两处断言均为 false）：
    /// ① 同一输入的两次独立 parse→save 必须逐字节相同（`HashMap` 随机迭代序 ⇒ 不同）；
    /// ② 落盘文件中 hotkeys 键的**出现顺序 = 字节序**（与 Go `encoding/json` 的 map 键序
    ///    一致）。用例覆盖四类字节序陷阱：标点(`*,` 先于 `*0`/`,`)、大小写(`A`<`a`)、
    ///    前缀(`*z` 早于 `A`)、非 ASCII(`网盘`/`🔥key` 恒在全部 ASCII 之后)。
    #[test]
    fn hotkeys_key_order_is_deterministic_and_byte_sorted() {
        let input = r#"{
  "keymaps": [{
    "id": 5, "name": "键序", "enable": true, "hotkey": "*CapsLock",
    "parentID": 0, "delay": 0, "disableAt": "",
    "hotkeys": {
      "z": [], "A": [], "*z": [], "singlePress": [], "*,"
      : [], "网盘": [], "a": [], "*0": [], ",": [], "*A": [], "🔥key": []
    }
  }],
  "options": {},
  "selectedAction": {"hotkey": "", "enable": false, "mappings": []}
}"#;
        let dir = std::env::temp_dir().join(format!("kf-cfg-keyorder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let in_path = dir.join("config.json");
        std::fs::write(&in_path, input).unwrap();

        // ① 两次独立 parse→save 字节完全相同
        let out_a = dir.join("a.json");
        let out_b = dir.join("b.json");
        let cfg_a = parse_config(&in_path, "").unwrap();
        let cfg_b = parse_config(&in_path, "").unwrap();
        save_config_file(&cfg_a, &out_a).unwrap();
        save_config_file(&cfg_b, &out_b).unwrap();
        let a = std::fs::read(&out_a).unwrap();
        let b = std::fs::read(&out_b).unwrap();
        assert_eq!(a, b, "同一配置两次落盘字节不同（键序不确定）");
        let text = String::from_utf8(a).unwrap();

        // ② 落盘键序 = 字节序（Rust `str` 的 Ord 即 UTF-8 字节序，等于 Go 的 map 键序口径）
        let mut expected: Vec<&str> = vec![
            "z",
            "A",
            "*z",
            "singlePress",
            "*,",
            "网盘",
            "a",
            "*0",
            ",",
            "*A",
            "🔥key",
        ];
        expected.sort_unstable();
        assert_eq!(
            expected,
            vec![
                "*,",
                "*0",
                "*A",
                "*z",
                ",",
                "A",
                "a",
                "singlePress",
                "z",
                "网盘",
                "🔥key"
            ],
            "用例本身应覆盖四类字节序陷阱"
        );

        let mut cursor = 0usize;
        for key in &expected {
            let needle = format!("\"{key}\":");
            let rel = text[cursor..].find(&needle).unwrap_or_else(|| {
                panic!("键 {key:?} 未按字节序出现（cursor={cursor}）：\n{text}")
            });
            cursor += rel + needle.len();
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 跨语言逐字节：同一输入经 Go / Rust 的「ParseConfig → 落盘」后字节必须完全相同。
    /// 夹具 `tests/fixtures/config_save.json` = **Go 冻结契约快照**（来源
    /// `config-server/internal/script/config_save_export_test.go`；Go 后端 2026-10-06 退役, 36ccb83
    /// ⇒ 不再有再生成入口, 溯源用 `git show 36ccb83^:config-server/internal/script/`）。
    /// 覆盖 16 keymap / 55KB 的真实出厂配置 + 覆盖字节序陷阱的合成用例。
    ///
    /// 已知且**先于本次改动**存在的形态差异（本次不修，已单独上报）：Go 的 nil slice 落盘为
    /// `null`，Rust 的 `Vec` 无 nil 概念、落盘为 `[]`（亦见 `server/dto.rs` 模块头）。下表把两种
    /// 形态归一后再比对，且**两侧都归一** —— 一旦出现别的任何差异（键序 / 转义 / 缩进 / 尾换行 /
    /// 字段集 / 数值）仍会红；nil 语义统一后本表自动退化为无操作，可从表中清空。
    #[test]
    fn save_bytes_match_go_across_languages() {
        /// (Go 的 nil 形态, Rust 的空集合形态)
        const NIL_SLICE_FORMS: [(&str, &str); 1] = [("\"disabled\": null", "\"disabled\": []")];

        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/config_save.json");
        let raw = std::fs::read_to_string(&fixture_path).unwrap_or_else(|error| {
            panic!(
                "读取跨语言夹具 {} 失败: {error}\n\
                 夹具 = Go 冻结契约快照 (仓库自带文件, 缺失即仓库不完整); \
                 历史实现在 git 36ccb83^:config-server/internal/script/",
                fixture_path.display()
            )
        });
        let fixture: serde_json::Value = serde_json::from_str(&raw).expect("夹具 JSON 解析失败");
        let cases = fixture["cases"].as_array().expect("夹具缺少 cases");
        assert!(!cases.is_empty(), "夹具没有用例");

        let normalize = |text: &str| -> String {
            let mut out = text.to_string();
            for (go_form, rust_form) in NIL_SLICE_FORMS {
                out = out.replace(go_form, rust_form);
            }
            out
        };

        for case in cases {
            let name = case["name"].as_str().unwrap();
            let version = case["version"].as_str().unwrap();
            let input = case["input"].as_str().unwrap();
            let want = normalize(case["saved"].as_str().unwrap());

            let dir =
                std::env::temp_dir().join(format!("kf-cfgsave-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let in_path = dir.join("config.json");
            std::fs::write(&in_path, input).unwrap();

            let config = parse_config(&in_path, version)
                .unwrap_or_else(|error| panic!("[{name}] parse_config 失败: {error}"));
            let out_path = dir.join("saved.json");
            save_config_file(&config, &out_path).unwrap();
            let got = normalize(&std::fs::read_to_string(&out_path).unwrap());

            if got != want {
                // 首处差异 + 前后窗口：把「键序漂移」这类差异定位到具体字节
                let (gb, wb) = (got.as_bytes(), want.as_bytes());
                let at = (0..gb.len().min(wb.len()))
                    .find(|&i| gb[i] != wb[i])
                    .unwrap_or(gb.len().min(wb.len()));
                let lo = at.saturating_sub(120);
                panic!(
                    "[{name}] Rust 落盘字节与 Go 参考字节不一致（{} vs {} 字节，首处差异 @{at}）\n\
                     rust: ...{}...\n  go: ...{}...",
                    gb.len(),
                    wb.len(),
                    String::from_utf8_lossy(&gb[lo..(at + 120).min(gb.len())]),
                    String::from_utf8_lossy(&wb[lo..(at + 120).min(wb.len())]),
                );
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
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
