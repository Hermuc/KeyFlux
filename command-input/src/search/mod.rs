//! Everything 搜索后端（T0' 第一步：进程内 IPC 客户端，见 memory-export-2026-10-24 §4.7）。
//!
//! 设计约束：
//! - 纯 std + windows crate，零新增第三方依赖（cmdinput 依赖纪律，设计文档 C §3）；
//! - 自包含模块：本目录可整体迁移/删除，不影响命令框其余部分；
//! - 本步只提供客户端能力（连接 / 查询 / 解析）；协议接线（0x40C/0x40D）与
//!   SearchProvider 调度属于后续批次，此处不触碰现有行为。

pub mod ipc;

pub mod bridge;
