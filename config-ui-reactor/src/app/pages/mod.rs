//! `app::pages` —— [`Shell`](crate::app::Shell) 的**页面装配**方法（2026-10-08 自
//! `app/views.rs` 拆出）。
//!
//! 拆分动机（模块化审查报告 §3.2 / 问题 #4）：`app/views.rs` 曾是一个 1801 行的
//! 上帝对象（单个 `impl Shell` / 22 个方法 / 7 个页面，`settings_page` 单函数长达数百行；
//! 2026-10-09 又按页面内分区边界把各超长装配函数拆小）。
//! 现在按「一个页面一个文件」收敛：外壳与路由留在 `app/views.rs`，页面在此目录。
//!
//! 约定：
//! * 每个文件一个 `impl Shell` 块，方法体是**逐字搬移**的结果；仅可见性从
//!   `pub(super)` 改为 `pub(in crate::app)`（本模块比 `crate::app` 低一层）。
//! * 页面之间互不调用；跨页共享的字段仍旧来自 `Shell`（字段可见性未动 ——
//!   `crate::app::pages::*` 是 `crate::app` 的后代模块，私有字段本就可读）。
//! * 加新页面 = 本目录加文件 + 在此登记 + `app.rs` 的 `PageKind` 路由加一支。

pub(super) mod abbr;
pub(super) mod action_editor;
pub(super) mod guide;
pub(super) mod keymap;
pub(super) mod plugins;
pub(super) mod selected_action;
pub(super) mod settings;
