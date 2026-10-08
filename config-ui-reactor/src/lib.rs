//! KeyFlux 设置面板 —— **库根**（P3 起 crate = lib + bin）。
//!
//! 迁自 `config-ui-avalonia`（.NET 10 + Avalonia 11.3.20）。
//! 边界原则：**换实现，不换边界** —— Go 后端 / AHK 引擎 / `data/config.json` 真源 /
//! HTTP 协议串（KEYFLUX_PORT 等）全部不动，风险限制在「一个 HTTP 客户端 + 一套 UI」。
//!
//! 分层（规范原文归档在仓内 `doc/poc-archive/01-项目规范.md`）：
//!   `app`       组件与导航（根组件 + 视图 + 弹窗 + 状态）
//!   `ui`        可复用视图构件（卡片/行/按钮/指示字等纯渲染单元）
//!   `theme`     主题令牌（Color/Brush/CornerRadius/Thickness）
//!   `models`    DTO（字段名必须与 Go/C# 契约逐字一致）
//!   `generator` 配置生成器（纯逻辑；零 UI 依赖，与 `settings.exe` drop-in 对齐）
//!   `server`    内嵌 localhost HTTP 服务（Rust 版 settings.exe 的路由层）
//!   `services`  HTTP 客户端 / 配置存取 / i18n / Markdown
//!   `platform`  reactor 与原生互操作的**唯一**落点（Ports & Adapters；不得反向依赖上层），
//!               并**托管**毛玻璃背板策略 `platform::glass`（窗口材质 = 平台决策，见其模块文档）
//!
//! 命名词汇（新代码一律照此；同义漂移的阅读税见 2026-10-07 代码审查报告 §2）：
//! * 从一组候选中**选中的项** = `selected`（勿写 `picked` / `chosen`）
//! * 列表中的一个**元素**   = `entry`    （勿写 `item` / `row`）
//! * 容器 / 卡片            = `card`     （勿写 `panel`）
//!
//! 例外（`pick` 作**动词**，指动作而非选中项，故不收敛）：`window_picker::pick`
//! 与 `Message::WindowPicked`（窗口拾取）、`platform::file_dialog::pick_open_file`
//! （弹出选择对话框）、`services::plugins::pick`（按语言择一标签）。
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
//!
//! ⚠️ 本文件此前有 crate 级 `#![allow(dead_code)]`（**2026-10-01 移除**）：它让所有私有
//!   死码对 `clippy -D warnings` 永久隐身 —— 2026-10-01 清理时必须叠加
//!   `--force-warn dead_code` 压过它，才看到当时实存的 9 条死码。**不要恢复它**。
//!   确需保留的未读项（如 RAII 守卫字段）请在**该项就地**写
//!   `#[expect(dead_code, reason = "…")]`：`expect` 会在该项将来被真实消费时报
//!   `unfulfilled_lint_expectations` 迫使清理，`allow` 不会（会一直烂在那里）。

pub mod app;
pub mod generator;
/// 契约 ID / 名称词表的**单一实现**（叶层，零依赖；`generator` 与 `server` 共用）。
pub(crate) mod ids;
pub mod models;
pub mod platform;
pub mod server;
pub mod services;
pub mod theme;
pub mod ui;
