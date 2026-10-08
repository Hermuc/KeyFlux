//! 面板诊断日志的**统一出口**（批 S，2026-10-08）。
//!
//! 为什么需要：release 构建是 GUI 子系统（`main.rs` 的
//! `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`），`eprintln!`
//! 无处可去 —— 「子进程启动失败 / 后端桥接失败」等现场在 release 下**完全不可见**，
//! 排障只能靠猜。本出口在 stderr（debug 构建的控制台仍可见）之外**追加落盘**
//! `%TEMP%\keyflux-panel-dev.log`，让诊断在两种构建下都有去处。
//!
//! 纪律（与引擎 `EngineLogWarn` 同款）：
//! - 落盘自身**零故障面**：打开/写入失败一律静默并置位 `SINK_BROKEN`（日志不能成为
//!   新的故障源）；
//! - 只做**诊断**：`bin/settings.rs` / `bin/build_tools.rs` 的 `println!`/`eprintln!`
//!   是 **CLI 接口输出**（api-parity 等工具读其 stdout），**不适用**本出口；
//!   `bridge.rs` 的 `println!`（`CALL_STATUS_PREFIX` 协议行）同理不迁。
//!
//! 时间戳用 UNIX 秒（std 无 strftime，不为此引依赖）；文件 mtime 可交叉定位。

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 落盘失败后置位：不再反复尝试（避免每条诊断都撞同一个坏路径）。
static SINK_BROKEN: AtomicBool = AtomicBool::new(false);

/// 诊断宏：`crate::devlog!("execCmd: 启动 {exe} 失败: {error}")` —— stderr + 追加落盘。
/// 以全路径调用（`#[macro_export]` 挂在 crate 根），调用点无需 `use`。
#[macro_export]
macro_rules! devlog {
    ($($arg:tt)*) => {
        $crate::services::devlog::log(format_args!($($arg)*))
    };
}

pub fn log(args: std::fmt::Arguments<'_>) {
    // stderr：debug 构建的控制台仍可见；release（GUI 子系统）下为 no-op，无害。
    eprintln!("{args}");

    if SINK_BROKEN.load(Ordering::Relaxed) {
        return;
    }
    let path = std::env::temp_dir().join("keyflux-panel-dev.log");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        SINK_BROKEN.store(true, Ordering::Relaxed);
        return;
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    if writeln!(file, "[{secs}] {args}").is_err() {
        SINK_BROKEN.store(true, Ordering::Relaxed);
    }
}
