//! KeyFlux 设置面板（Rust + windows-reactor / WinUI）
//!
//! 迁移自 `config-ui-avalonia`（.NET 10 + Avalonia 11.3.20）。
//! 边界原则：**换实现，不换边界** —— Go 后端 / AHK 引擎 / `data/config.json` 真源 /
//! HTTP 协议串（KEYFLUX_PORT 等）全部不动，风险限制在「一个 HTTP 客户端 + 一套 UI」。
//!
//! 分层（见 `D:\PortableApps\KeyFlux-WinUI-migration\01-项目规范.md`）：
//!   `app`      组件与导航
//!   `theme`    主题令牌（Color/Brush/CornerRadius/Thickness）
//!   `models`   DTO（字段名必须与 Go/C# 契约逐字一致）
//!   `services` HTTP 客户端 / 配置存取 / i18n / Markdown
//!   `platform` reactor 与原生互操作的**唯一**落点（Ports & Adapters）
//!
//! ⚠️ API 依据纪律：**一律以本机 `cargo registry/src/windows-reactor-0.100.0` 源码为准**，
//!    不要照抄 master 文档（0.100.0 无 `window_frame` / `run_window` / `set_timeout`，
//!    控件用 slot 体系而非 `menu_items()/content()/header()`）。
//!
//! 部署产物为 **GUI 子系统**：面板被引擎（GUI 进程）拉起时，Windows 不再为其新建
//! 控制台窗口（2026-09-28 用户报告的黑窗根因，PE Subsystem 3→2）。
//! debug 构建保留控制台，便于 println! 诊断与 panic 观察；旧 C# 面板
//! （`OutputType=WinExe`）即 GUI 子系统，本属性使两代实现行为对齐。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// 骨架期：主题令牌/隔离层里已有部分条目尚未被页面使用。Phase 3 逐页迁移完成后删除本行，
// 届时 `-D warnings` 闸门（Phase 4）将强制零告警。
#![allow(dead_code)]

mod app;
mod models;
mod platform;
mod services;
mod theme;
mod ui;

fn main() {
    // 随包 MiSans 进程内私有加载（须在首个 DirectWrite 字体解析前完成）；
    // 加载后由 fork 的 install_global_ui_font 覆盖全局字体键生效。
    let _loaded = platform::fonts::load_private_fonts();
    // GUI 子系统下 panic/Err 不可见（无控制台）⇒ 顶层错误先落盘再传播，
    // 否则「面板启动即退出」没有任何现场可查（2026-09-28 黑窗排查的教训）。
    if let Err(error) = windows_reactor::App::run_component::<app::Shell>(()) {
        let path = std::env::temp_dir().join("keyflux-panel-error.log");
        let _ = std::fs::write(&path, format!("{error:?}\n"));
        panic!("panel exited: {error:?}");
    }
}
