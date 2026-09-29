//! Rust 生成器（P3：接管 Go `internal/script` + `generators`）。
//!
//! 已迁移：`text`（AHK 文本层）/ `model`（配置模型）/ `behaviors`（行为目录）/
//! `config`（ParseConfig+Preprocess）/ `plan`（注册计划）/ `actions`（动作渲染器）/
//! `plugins`（插件注入）/ `template`（两模板的字节级改写）/ `scripts`（GenerateScripts/
//! ChangeVersion/UseOriginalAHK）。
//!
//! 迁移纪律（与 `docs/plan-rust-migration.md` 一致）：
//! 1. 不许"照着 Go 猜"——语义有疑问的一律以 Go 真实输出为准
//!    （`config-server` 侧有门控导出测试产出对账夹具）。
//! 2. 每个单元迁完即跑端到端 parity；对账不过不推进。

pub mod actions;
pub mod behaviors;
pub mod config;
pub mod model;
pub mod plan;
pub mod plugins;
pub mod scripts;
pub mod template;
pub mod text;
