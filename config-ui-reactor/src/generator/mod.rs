//! Rust 生成器（P3：逐步接管 Go `internal/script` + `generators`）。
//!
//! 现状：**仅文本层**（[`text`]）。迁移按「先底层纯函数、后端到端 parity 兜底」推进 ——
//! 每个单元都要能在 `tools/parity/` 的 9 份基线上逐字节验证后才算完成。
//!
//! 为什么要独立于面板层：生成器必须能被「面板 bin」与「未来的 `settings.exe` bin」
//! 共享，且要能被单测直接调用 ⇒ 放在 lib 而非 bin（见 `src/lib.rs` 头注）。
//!
//! 迁移纪律（与 `docs/plan-rust-migration.md` 一致）：
//! 1. 不许"照着 Go 猜"——语义有疑问的一律以 Go 真实输出为准
//!    （`config-server` 侧有门控导出测试产出对账夹具）。
//! 2. 每个单元迁完即跑端到端 parity；对账不过不推进。

pub mod text;
