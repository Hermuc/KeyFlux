//! 服务层：HTTP 客户端 / 配置存取 / i18n / Markdown。
//!
//! 边界契约（**不得改动**）：
//! * 仅作 `http://127.0.0.1:{KEYFLUX_PORT}` 的客户端；端口从后端 stdout 的
//!   `KEYFLUX_PORT=<port>` 协议串解析（旧版 `BackendSession.cs:459`）；
//! * 协议串 `KEYFLUX_PORT` / `KEYFLUX_GUI_READY` / `KEYFLUX_BACKEND_EXITED` 语义不变；
//! * 12 个端点见 `config-ui-avalonia/Services/SettingsApiClient.cs`；
//! * 配置真源 = 部署目录 `data/config.json`（不引入第二真源）。
//!
//! 进度：
//! * ✅ `i18n`（文案表 + 语义等价单测 + 键数守卫）
//! * ✅ `api`（`SettingsApi` trait + `ureq` 阻塞实现；错误折叠为 status=0）
//! * ✅ `backend`（后端会话：`--headless` 子进程 + `KEYFLUX_PORT` 通告 + 健康轮询）
//! * ✅ `store`（保存清洗 `clean_for_save` / 布局解析 / `change_abbr_enable` / 键名规范化）
//! * ✅ `markdown`（使用指南文档解析器，纯逻辑块模型）
//!
//! Phase 2 服务层**已全部落地**；后续只剩 Phase 3 的 UI 组件消费它们。

pub mod abbr;
pub mod action_editor;
pub mod api;
pub mod backend;
pub mod behaviors_edit;
pub mod cli_api;
pub mod i18n;
pub mod keymap;
pub mod markdown;
pub mod market;
pub mod match_types_edit;
pub mod plugins;
pub mod selected_action;
pub mod settings;
pub mod store;
pub mod transport;
