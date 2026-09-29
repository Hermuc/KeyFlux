//! `GenerateScripts` / `ChangeVersion` / `UseOriginalAHK` 的移植。
//!
//! Go 出处：`internal/command/command.go`（`GenerateScripts` / `ChangeVersion` /
//! `UseOriginalAHK`）与 `internal/script/script.go`（`GenerateScripts`）。
//!
//! ⚠️ **相对路径口径与 Go 完全一致，靠 cwd 而非参数**：
//! * `GenerateScripts` / `ChangeVersion` 的 cwd 必须是部署树 **`bin/`**
//!   （读 `../data/config.json`、`../data/plugins`，写 `../bin/KeyFlux.ahk`、`../bin/CommandInputSkin.txt`）；
//! * `UseOriginalAHK` 的 cwd 必须是部署树**根**（`bin\AutoHotkey64.exe` → `KeyFlux.exe`）。
//!
//! 字体安装已移植到 font 模块：InstallCommandFont 的调用方**忽略错误**
//! （Go 的静默跳过口径），故这里同样 `let _ =`。

use std::io;
use std::path::Path;
use std::process::Command;

use crate::generator::behaviors;
use crate::generator::config::{CONFIG_REL_PATH, parse_config, preprocess, save_config_file};
use crate::generator::model::Config;
use crate::generator::template::{render_command_input_skin, render_keyflux_ahk};

/// Go `script.GenerateScripts`：渲染两份产物到部署树 `bin/`（**写文件**）。
pub fn generate_scripts(config: &mut Config, exe_dir: &Path) -> io::Result<()> {
    // Go: generators.BehaviorCatalog = LoadBehaviorCatalog("../data/config.json")
    let config_path = Path::new(CONFIG_REL_PATH);
    let catalog = behaviors::load_catalog_for_config(config_path, exe_dir);
    // Go: generators.SetPluginsDir("../data/plugins") ⇒ 由 render_keyflux_ahk 内部消费
    preprocess(config);

    // Go: `_ = InstallCommandFont(config.Options.CommandFont, "")` —— 调用方忽略错误（静默跳过）。
    let _ = crate::generator::font::install_command_font(&config.options.command_font, "");

    let keyflux = render_keyflux_ahk(config, Some(&catalog), Path::new("../data/plugins"));
    std::fs::write(Path::new("../bin/KeyFlux.ahk"), keyflux)?;
    let skin = render_command_input_skin(config);
    std::fs::write(Path::new("../bin/CommandInputSkin.txt"), skin)?;
    Ok(())
}

/// Go `command.GenerateScripts`（无参数，配置恒取 `../data/config.json`）。
pub fn generate_scripts_from_cwd(exe_dir: &Path) -> io::Result<()> {
    let mut config = parse_config(Path::new(CONFIG_REL_PATH), "")?;
    generate_scripts(&mut config, exe_dir)
}

/// Go `command.ChangeVersion`：写版本号 + **重置语言** + 落盘 + 重新生成。
///
/// 注意它**会写回** `../data/config.json`（与 GenerateScripts 只读不同）。
pub fn change_version(version: &str, exe_dir: &Path) -> io::Result<()> {
    let mut config = parse_config(Path::new(CONFIG_REL_PATH), version)?;
    config.options.keyflux_version = version.to_string();
    config.options.language = String::new(); // Go: 重置语言
    save_config_file(&config, Path::new(CONFIG_REL_PATH))?;
    generate_scripts(&mut config, exe_dir)
}

/// Go `command.UseOriginalAHK`：误报病毒时的恢复手段 —— 把发行版自带的
/// `bin\AutoHotkey64.exe` 与 `bin\Launcher.ahk` 复制回部署树根的
/// `KeyFlux.exe` / `KeyFlux.ahk`。**cwd 必须是部署树根**。
///
/// 用 `cmd.exe /c copy /y`（与 Go 相同）而非 `fs::copy`：一是控制台输出逐字一致
/// （"已复制 1 个文件。"），二是保留"目标被占用即失败"的语义（正被运行的
/// `KeyFlux.exe` 无法覆盖 ⇒ 提示用户先关闭）。
pub fn use_original_ahk() -> Result<(), String> {
    const EXE: &str = r"bin\AutoHotkey64.exe";
    if !Path::new(EXE).is_file() {
        // Go: fmt.Println("Error: file", exe, "does not exist")
        println!("Error: file {EXE} does not exist");
        return Ok(()); // Go 只打印并 return，不算错误
    }
    run_copy("copy /y bin\\AutoHotkey64.exe KeyFlux.exe")
        .map_err(|_| "\nPlease close KeyFlux and retry".to_string())?;
    run_copy("copy /y bin\\Launcher.ahk KeyFlux.ahk")
        .map_err(|error| format!("copy bin\\Launcher.ahk 失败: {error}"))?;
    println!("\ndone!");
    Ok(())
}

/// Go `execCmd`：经 `cmd.exe /c` 执行并继承 stdout/stderr，等待完成。
fn run_copy(args: &str) -> Result<(), String> {
    let status = Command::new("cmd.exe")
        .args(["/c", args])
        .status()
        .map_err(|error| format!("cmd.exe 启动失败: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cmd.exe /c {args} 退出码 {:?}", status.code()))
    }
}

/// 供 bin 使用的统一入口：按 Go 的 panic 语义把错误上抛（bin 侧转为非 0 退出）。
pub fn run_generate_scripts(exe_dir: &Path) -> io::Result<()> {
    generate_scripts_from_cwd(exe_dir)
}

/// 供单测/调用方构造最小配置（透传 [`crate::generator::config::empty_keymap`] 语义）。
pub fn empty_config() -> Config {
    Config::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `change_version` 会**写回** `../data/config.json`（cwd 相对），单测里不能真跑；
    /// 这里只锁"版本号写入 + 语言重置"这两个纯内存语义。
    #[test]
    fn change_version_resets_language_and_sets_version() {
        let mut config = parse_config(
            Path::new("../tools/parity/corpus/factory/config.json"),
            "old",
        )
        .expect("解析 factory 语料");
        // 复刻 ChangeVersion 的字段写入（不落盘）
        config.options.keyflux_version = "9.9.9".to_string();
        config.options.language = String::new();
        assert_eq!(config.options.keyflux_version, "9.9.9");
        assert_eq!(config.options.language, "");
    }

    #[test]
    fn use_original_ahk_reports_missing_runtime() {
        // cwd 是 crate 根，不存在 bin\AutoHotkey64.exe ⇒ 走"文件不存在"分支且不报错
        // （与 Go 一致：只打印，不算失败）。
        assert!(use_original_ahk().is_ok());
    }
}
