//! 面板可执行入口 —— bin `keyflux-settings`（部署阶段重命名为 `KeyFlux.Settings.exe`，
//! 引擎按该文件名拉起面板）。
//!
//! 各层实体在 lib（crate `config_ui_reactor`）中，见 `src/lib.rs`。
//! 本文件**只**保留：入口 + GUI 子系统属性 + 启动期一次性初始化。
//!
//! 部署产物为 **GUI 子系统**：面板被引擎（GUI 进程）拉起时，Windows 不再为其新建
//! 控制台窗口（2026-09-28 用户报告的黑窗根因，PE Subsystem 3→2）。
//! debug 构建保留控制台，便于 println! 诊断与 panic 观察；旧 C# 面板
//! （`OutputType=WinExe`）即 GUI 子系统，本属性使两代实现行为对齐。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use config_ui_reactor::{app, platform, services};

fn main() {
    // 随包 MiSans 进程内加载（须在首个 DirectWrite 字体解析前完成）；
    // 加载后由 fork 的 install_global_ui_font 覆盖全局字体键生效。
    // 标志位可经 `KEYFLUX_FONT_FLAGS` 覆盖，见 platform::fonts 模块注释。
    let font_flags = platform::fonts::active_flags();
    let loaded = platform::fonts::load_private_fonts();
    // GUI 子系统无控制台 ⇒ 无法用 println! 取证字体加载结果。
    // **仅**在调试模式（显式设了 KEYFLUX_FONT_FLAGS）时落盘，默认零副作用、不留文件。
    if std::env::var_os("KEYFLUX_FONT_FLAGS").is_some() {
        let _ = std::fs::write(
            std::env::temp_dir().join("keyflux-font.log"),
            format!(
                "flags=0x{font_flags:x} loaded={} files={}\n",
                loaded.len(),
                loaded
                    .iter()
                    .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
    }

    // 传输选择：`--api=cli` 走进程内 CLI 桥（无端口 / 无常驻后端）；缺省 http（行为与改动前一致）。
    let args: Vec<String> = std::env::args().collect();
    services::transport::set_transport(services::transport::parse_transport(&args));

    // GUI 子系统下 panic/Err 不可见（无控制台）⇒ 顶层错误先落盘再传播，
    // 否则「面板启动即退出」没有任何现场可查（2026-09-28 黑窗排查的教训）。
    let outcome = windows_reactor::App::run_component::<app::Shell>(());
    // 无标志加载的字体**不会**随进程退出消失（进的是 GDI 公共字体表），须对称清理。
    platform::fonts::unload_fonts(&loaded, font_flags);
    if let Err(error) = outcome {
        let path = std::env::temp_dir().join("keyflux-panel-error.log");
        let _ = std::fs::write(&path, format!("{error:?}\n"));
        panic!("panel exited: {error:?}");
    }
}
