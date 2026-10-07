//! Rust 生成器（P3：接管 Go `internal/script` + `generators`）。
//!
//! 已迁移：`text`（AHK 文本层）/ `model`（配置模型）/ `behaviors`（行为目录）/
//! `config`（ParseConfig+Preprocess）/ `plan`（注册计划）/ `actions`（动作渲染器）/
//! `plugins`（插件注入）/ `template`（两模板的字节级改写）/ `scripts`（GenerateScripts/
//! ChangeVersion/UseOriginalAHK）。
//!
//! 迁移纪律（与 `docs/plan-rust-migration.md` 一致）：
//! 1. 不许"照着 Go 猜"——语义有疑问的一律以**冻结对账夹具 / parity 基线**为准
//!    （Go 后端 2026-10-06 退役, 36ccb83; 夹具与基线即契约, 溯源 `git show 36ccb83^:config-server/`）。
//! 2. 每个单元迁完即跑端到端 parity；对账不过不推进。

pub mod actions;
pub mod behaviors;
pub mod config;
pub mod font;
pub mod model;
pub mod plan;
pub mod plugins;
pub mod scripts;
pub mod template;
pub mod text;
