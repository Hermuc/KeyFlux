//! `settings.exe` 的 Rust 版（**drop-in 替代**，P3）—— 已实现全部子命令：DumpPlan / GenerateAHK / GenerateScripts / ChangeVersion / UseOriginalAHK。
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

use config_ui_reactor::generator::{behaviors, config as config_parse, plan, scripts, template};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        eprintln!("usage: settings.exe <Command> [args...]");
        return ExitCode::from(2);
    };

    match command {
        "DumpPlan" => dump_plan(&args),
        "GenerateAHK" => generate_ahk(&args),
        "GenerateScripts" => {
            // Go: 无参数，配置恒取 `../data/config.json`，cwd 必须是部署树 `bin/`
            match scripts::run_generate_scripts(&exe_dir()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(1)
                }
            }
        }
        "ChangeVersion" => {
            // Go: `ChangeVersion <version>` ⇒ args[0] = os.Args[2]
            if args.len() < 3 {
                eprintln!("ChangeVersion requires 1 argument, for example: ChangeVersion 1.0.0");
                return ExitCode::from(2);
            }
            match scripts::change_version(&args[2], &exe_dir()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(1)
                }
            }
        }
        "UseOriginalAHK" => match scripts::use_original_ahk() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        other => {
            eprintln!("unsupported command: {other}");
            ExitCode::from(2)
        }
    }
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

/// Go `command.GenerateAHK`：`GenerateAHK <config.json> <template> <output>`。
///
/// **模板路径是参数**（`tools/parity` 正是这样调的：`ahk` 用 `keyflux.tmpl`、`skin` 用
/// `CommandInputSkin.tmpl`），故必须**按模板文件名分派**到对应渲染器；不认识的文件名
/// **显式报错退出 2**（绝不静默用错渲染器 → 产出错字节）。
///
/// 与 Go `SaveAHK` 的字节口径一致：行尾统一 CRLF、`keyflux.tmpl` 产物带 UTF-8 BOM
/// （模板首字符）；这些都在 `generator::template` 内完成。
fn generate_ahk(args: &[String]) -> ExitCode {
    // Go 用 os.Args[2]/[3]/[4]，故 len(os.Args) < 5 即报错（args[0]=exe, args[1]=子命令）。
    if args.len() < 5 {
        eprintln!(
            "GenerateAHK requires 3 arguments, for example: GenerateAHK ./config.json ./templates/script.ahk ./output.ahk"
        );
        return ExitCode::from(2);
    }
    let config_file = Path::new(&args[2]);
    let template_file = Path::new(&args[3]);
    let output_file = Path::new(&args[4]);

    let version = option_env!("KEYFLUX_VERSION").unwrap_or("");
    let mut config = match config_parse::parse_config(config_file, version) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    // 与运行时路径（GenerateScripts）保持一致：先设行为目录，再 Preprocess。
    // 行为目录口径同 Go：内置包取 `<settings.exe 目录>/behaviors`，用户/插件包取 config 同级。
    let catalog = behaviors::load_catalog_for_config(config_file, &exe_dir());
    // 插件注入目录 = `<config.json 目录>/plugins`（与 behaviors 的插件贡献包同口径）。
    let plugins_dir = config_file
        .parent()
        .unwrap_or(Path::new("."))
        .join("plugins");
    config_parse::preprocess(&mut config);

    // ⚠️ `InstallCommandFont`（Go GenerateAHK 会调用但**丢弃返回值**）**有意不实现**：
    // 它只把用户字体复制到 `bin/font/font.ttf`（纯表现层资源），**不影响产物字节**，
    // 对 P3 的逐字节对账无意义，故此处不落盘字体文件。

    let template_name = template_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let rendered = match template_name {
        "keyflux.tmpl" => template::render_keyflux_ahk(&mut config, Some(&catalog), &plugins_dir),
        "CommandInputSkin.tmpl" => template::render_command_input_skin(&config),
        other => {
            eprintln!(
                "GenerateAHK: unsupported template {other:?} (expected keyflux.tmpl or CommandInputSkin.tmpl)"
            );
            return ExitCode::from(2);
        }
    };

    if let Err(error) = std::fs::write(output_file, rendered.as_bytes()) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
