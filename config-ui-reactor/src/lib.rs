//! KeyFlux 设置面板 —— **库根**（P3 起 crate = lib + bin）。
//!
//! 迁自 `config-ui-avalonia`（.NET 10 + Avalonia 11.3.20）。
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
//! **lib 与 bin 的边界**（2026-09-29 起）：
//! * `src/main.rs` = bin `keyflux-settings`（部署时重命名为 `KeyFlux.Settings.exe`）——
//!   只留入口与 GUI 子系统属性；
//! * 本 lib = 面板各层（`app`/`models`/`platform`/`services`/`theme`/`ui`），
//!   并且是 **P3 的 Rust 生成器**（与 `settings.exe` drop-in）的落点 —— 生成器必须能被
//!   bin 与单测共享，故不能留在 bin crate 内（bin 里未被引用的 `pub` 项会触发 `dead_code`，
//!   而 `clippy -D warnings` 是闸门）。
#![allow(dead_code)]

pub mod app;
pub mod generator;
pub mod models;
pub mod platform;
pub mod server;
pub mod services;
pub mod theme;
pub mod ui;
