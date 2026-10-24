//! voidtools Everything 1.5 命名管道 IPC（"Everything 3" 协议）的同步客户端。
//!
//! 协议与移植来源（两者均为 MIT；本仓库 GPL-3.0 可兼容容纳，归属保留于此）：
//! - 消息帧 / 命令码 / 响应码 / SEARCH 请求与结果流格式：
//!   voidtools/ES `src/ipc3.{h,c}` 与 `src/es.c`
//!   Copyright (c) 2025 voidtools / David Carpenter
//! - overlapped 传输层形态（事件 + WaitForSingleObject + GetOverlappedResult）：
//!   Chaoses-Ib/IbEverythingLib `everything-ipc/src/pipe/mod.rs`
//!   Copyright (c) 2025 Chaoses-Ib
//!
//! 此处按 KeyFlux 需要重写为纯同步实现：单自动复位事件、带超时等待、超时即
//! CancelIoEx 并报错（对端异常时命令框线程不会被无限挂住），无 tokio/无后台线程。
//!
//! 约定：任何 `Err` 返回后连接状态不可再信，调用方应丢弃该 [`Client`] 并重连。

mod pipe;
mod query;

pub use pipe::Client;
pub use query::{Hit, SearchReply, SearchRequest};

/// IPC 客户端错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Everything 未运行（命名管道不存在）。
    NotRunning,
    /// 管道忙（全部实例被占用，短暂重试后仍忙）。
    Busy,
    /// 一次读写等待超时（已发起取消；应重连）。
    Timeout,
    /// 连接断开（对端关闭或 IO 错误）。
    Disconnected,
    /// Everything 返回错误响应码（含 4xx/5xx）。
    Server(u32),
    /// 响应内容与协议不符。
    BadResponse,
    /// 系统资源不足（创建事件等内核对象失败）。
    OutOfMemory,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotRunning => write!(f, "Everything 未运行"),
            Error::Busy => write!(f, "Everything 管道忙"),
            Error::Timeout => write!(f, "Everything IPC 超时"),
            Error::Disconnected => write!(f, "Everything IPC 连接断开"),
            Error::Server(c) => write!(f, "Everything IPC 错误响应 {c}"),
            Error::BadResponse => write!(f, "Everything IPC 响应格式错误"),
            Error::OutOfMemory => write!(f, "系统资源不足"),
        }
    }
}

impl std::error::Error for Error {}
