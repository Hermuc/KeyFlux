//! `settings.exe` 的 Rust 版（**drop-in 替代**，P3）—— 目前只实现 `DumpPlan`。
//!
//! 契约（**逐字不变**，见 `docs/plan-rust-migration.md` §2）：
//! * 二进制名最终必须叫 `settings.exe`（`bin/Launcher.ahk` / `tools/oracle.ps1` /
//!   `误报病毒时执行这个.bat` / 面板均按名调用）；
//! * 子命令与参数序号与 Go 的 `os.Args` 一致：`settings.exe <Command> <a> <b> …`
//!   （Go 用 `os.Args[2]`、`os.Args[3]`… 取参数，故 `args[1]` 是子命令）；
//! * 相对路径口径同 Go：`builtin behaviors` 取**可执行文件目录**下的 `behaviors/`，
//!   用户包/插件包取 **config.json 所在目录** —— 故本地验证时须把 `bin/behaviors`
//!   拷到本二进制旁边（部署时天然满足：`settings.exe` 与 `behaviors/` 同在 `bin/`）。
//!
//! 本 bin 是 P3 的增量产物：未实现的子命令**显式报错退出 2**，绝不静默成功。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use config_ui_reactor::generator::{behaviors, config as config_parse, plan};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        eprintln!("usage: settings.exe <Command> [args...]");
        return ExitCode::from(2);
    };

    match command {
        "DumpPlan" => dump_plan(&args),
        "GenerateAHK" => unsupported("GenerateAHK"),
        "GenerateScripts" => unsupported("GenerateScripts"),
        "ChangeVersion" => unsupported("ChangeVersion"),
        "UseOriginalAHK" => unsupported("UseOriginalAHK"),
        other => {
            eprintln!("unsupported command: {other}");
            ExitCode::from(2)
        }
    }
}

fn unsupported(command: &str) -> ExitCode {
    eprintln!("{command}: not implemented yet in the Rust settings.exe (P3 incremental)");
    ExitCode::from(2)
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Go `command.DumpPlan`：`DumpPlan <config.json> <plan.json>`。
fn dump_plan(args: &[String]) -> ExitCode {
    if args.len() < 4 {
        eprintln!("DumpPlan requires 2 arguments, for example: DumpPlan ./config.json ./plan.json");
        return ExitCode::from(2);
    }
    let config_file = Path::new(&args[2]);
    let output_file = Path::new(&args[3]);

    // Go: KeyfluxVersion 由 `-ldflags -X settings/internal/script.KeyfluxVersion=…` 注入；
    // 计划产物不含该字段，故此处取构建期可选项即可。
    let version = option_env!("KEYFLUX_VERSION").unwrap_or("");

    let mut config = match config_parse::parse_config(config_file, version) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    // 与运行时路径（GenerateScripts）保持一致：先设行为目录，再 Preprocess
    let catalog = behaviors::load_catalog_for_config(config_file, &exe_dir());
    config_parse::preprocess(&mut config);

    if let Err(error) = plan::write_plan(&mut config, Some(&catalog), output_file) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
