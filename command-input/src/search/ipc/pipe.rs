//! 同步 overlapped 管道传输层。
//!
//! 消息帧 = 8 字节头 `{ code: u32, size: u32 }` + `size` 字节数据（均为小端）。
//! 响应码：100 = OK_MORE_DATA（还有后续消息），200 = OK（最后一条），
//! 400/401/404/500/501 = 错误（见 ES `ipc3.h` IPC3_RESPONSE_*）。

use std::mem::{size_of, MaybeUninit};
use std::thread::sleep;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_IO_PENDING, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE,
    HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use super::query::{StreamRead, COMMAND_SEARCH};
use super::Error;

/// 管道基础名（无实例后缀；实例形如 `Everything IPC (1.5a)`，见 ES ipc3_get_pipe_name）。
const PIPE_BASE: &str = r"\\.\pipe\Everything IPC";
/// 依次尝试的实例后缀：默认（无名实例）→ 1.5a → 1.5。
const INSTANCE_SUFFIXES: [&str; 3] = ["", " (1.5a)", " (1.5)"];
/// 单次 IO 等待上限：一次查询完整往返的保守上界（活体实测 ~12ms，见 §4.7）。
const IO_TIMEOUT_MS: u32 = 3000;
/// 管道忙时的重试节奏。
const BUSY_RETRIES: u32 = 2;
const BUSY_RETRY_DELAY: Duration = Duration::from_millis(50);
/// 单条消息数据段的内存上限（防异常响应撑爆内存）。
const MAX_MESSAGE: usize = 64 * 1024 * 1024;
/// 分块读写的块大小。
const CHUNK_SIZE: usize = 64 * 1024;

const RESPONSE_OK: u32 = 200;
const RESPONSE_OK_MORE_DATA: u32 = 100;

/// Everything 1.5 管道客户端（同步；按连接线程使用）。
#[derive(Debug)]
pub struct Client {
    conn: Conn,
}

/// 持有句柄的连接（Drop 关闭管道与事件）。
#[derive(Debug)]
struct Conn {
    pipe: HANDLE,
    event: HANDLE,
}

impl Drop for Conn {
    fn drop(&mut self) {
        // SAFETY: 句柄由本结构独占持有，Drop 时关闭一次；忽略关闭错误。
        unsafe {
            let _ = CloseHandle(self.pipe);
            let _ = CloseHandle(self.event);
        }
    }
}

impl Client {
    /// 连接到运行中的 Everything。
    ///
    /// 依次尝试无名实例与 `1.5a` / `1.5` 实例；管道忙时短暂重试。
    pub fn connect() -> Result<Client, Error> {
        for round in 0..=BUSY_RETRIES {
            for suffix in INSTANCE_SUFFIXES {
                match Conn::open(suffix) {
                    Ok(conn) => return Ok(Client { conn }),
                    Err(Error::Busy) => break, // 所有实例忙 → 等下一轮
                    Err(Error::NotRunning) => continue,
                    Err(e) => return Err(e),
                }
            }
            if round < BUSY_RETRIES {
                sleep(BUSY_RETRY_DELAY);
            }
        }
        Err(Error::NotRunning)
    }

    /// 发送一条无数据命令并读取 4 字节 u32 响应（GET_* 查询类命令）。
    pub(crate) fn simple_u32(&self, code: u32) -> Result<u32, Error> {
        let mut reader = self.conn.request(code, &[])?;
        reader.read_u32()
    }

    /// 发送 SEARCH 命令并返回结果流读取器（格式解析见 query.rs）。
    pub(crate) fn search_stream(&self, packet: &[u8]) -> Result<ReplyReader<'_>, Error> {
        self.conn.request(COMMAND_SEARCH, packet)
    }
}

impl Conn {
    /// 打开指定实例后缀的管道。
    fn open(suffix: &str) -> Result<Conn, Error> {
        let mut name: Vec<u16> = PIPE_BASE.encode_utf16().collect();
        name.extend(suffix.encode_utf16());
        name.push(0);

        // SAFETY: name 以 NUL 结尾的 UTF-16；其余参数为常量。
        let pipe = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                Default::default(),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        };
        let pipe = match pipe {
            Ok(h) => h,
            Err(_) => {
                // SAFETY: 仅读取线程局部错误码。
                let err = unsafe { GetLastError() };
                return if err == ERROR_PIPE_BUSY {
                    Err(Error::Busy)
                } else {
                    Err(Error::NotRunning)
                };
            }
        };

        // 自动复位事件：一次只有一个未决 IO，触发后自动复位，无需手工 ResetEvent。
        // SAFETY: 常量参数创建无名事件。
        let event =
            unsafe { CreateEventW(None, false, false, None) }.map_err(|_| Error::OutOfMemory)?;

        Ok(Conn { pipe, event })
    }

    /// 发送一条命令消息并进入响应读取。
    fn request(&self, code: u32, packet: &[u8]) -> Result<ReplyReader<'_>, Error> {
        let mut header = [0u8; 8];
        header[0..4].copy_from_slice(&code.to_le_bytes());
        header[4..8].copy_from_slice(&(packet.len() as u32).to_le_bytes());
        self.write_all(&header)?;
        self.write_all(packet)?;
        Ok(ReplyReader {
            conn: self,
            avail: 0,
            eof: false,
        })
    }

    /// 原样写出全部字节（分块；overlapped + 超时）。
    fn write_all(&self, mut buf: &[u8]) -> Result<(), Error> {
        while !buf.is_empty() {
            let chunk = buf.len().min(CHUNK_SIZE);
            let written = self.io_write(&buf[..chunk])?;
            if written == 0 {
                return Err(Error::Disconnected);
            }
            buf = &buf[written..];
        }
        Ok(())
    }

    /// 原样读满全部字节（overlapped + 超时）。
    fn read_exact(&self, buf: &mut [u8]) -> Result<(), Error> {
        let mut filled = 0;
        while filled < buf.len() {
            let chunk = (buf.len() - filled).min(CHUNK_SIZE);
            let read = self.io_read(&mut buf[filled..filled + chunk])?;
            if read == 0 {
                return Err(Error::Disconnected);
            }
            filled += read;
        }
        Ok(())
    }

    /// 单次 overlapped 写；返回传输字节数。超时先取消再报错。
    fn io_write(&self, buf: &[u8]) -> Result<usize, Error> {
        let mut overlapped: OVERLAPPED =
            // SAFETY: OVERLAPPED 为全零可用的 POD；事件句柄立即赋值。
            unsafe { MaybeUninit::<OVERLAPPED>::zeroed().assume_init() };
        overlapped.hEvent = self.event;
        let mut transferred: u32 = 0;
        // SAFETY: 句柄有效、缓冲区长度匹配、OVERLAPPED 为本次调用独占。
        let immediate = unsafe {
            WriteFile(
                self.pipe,
                Some(buf),
                Some(&mut transferred),
                Some(&mut overlapped),
            )
        };
        self.finish_io(immediate, &mut overlapped, transferred)
    }

    /// 单次 overlapped 读；返回传输字节数。超时先取消再报错。
    fn io_read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut overlapped: OVERLAPPED =
            // SAFETY: OVERLAPPED 为全零可用的 POD；事件句柄立即赋值。
            unsafe { MaybeUninit::<OVERLAPPED>::zeroed().assume_init() };
        overlapped.hEvent = self.event;
        let mut transferred: u32 = 0;
        // SAFETY: 句柄有效、缓冲区长度匹配、OVERLAPPED 为本次调用独占。
        let immediate = unsafe {
            ReadFile(
                self.pipe,
                Some(buf),
                Some(&mut transferred),
                Some(&mut overlapped),
            )
        };
        self.finish_io(immediate, &mut overlapped, transferred)
    }

    /// 等待一次未决 IO 完成（即时完成则直接返回）；超时先取消再报错。
    fn finish_io(
        &self,
        immediate: windows::core::Result<()>,
        overlapped: &mut OVERLAPPED,
        mut transferred: u32,
    ) -> Result<usize, Error> {
        if immediate.is_err() {
            // SAFETY: 仅读取线程局部错误码。
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return Err(Error::Disconnected);
            }
            // SAFETY: 事件为本连接独占，等待不与其他 IO 竞争。
            let wait = unsafe { WaitForSingleObject(self.event, IO_TIMEOUT_MS) };
            if wait.0 == WAIT_TIMEOUT.0 {
                // SAFETY: 取消本句柄全部未决 IO（本连接一次只有一个未决操作）。
                let _ = unsafe { CancelIoEx(self.pipe, None) };
                return Err(Error::Timeout);
            }
            if wait.0 != WAIT_OBJECT_0.0 {
                return Err(Error::Disconnected);
            }
            // SAFETY: overlapped 属本次调用；bWait=false（事件已触发）。
            let ok = unsafe { GetOverlappedResult(self.pipe, overlapped, &mut transferred, false) };
            if ok.is_err() {
                return Err(Error::Disconnected);
            }
        }
        Ok(transferred as usize)
    }
}

/// 响应消息流读取器：把 100/200 消息的数据段拼接为连续字节流。
#[derive(Debug)]
pub(crate) struct ReplyReader<'a> {
    conn: &'a Conn,
    /// 当前消息数据段剩余字节数。
    avail: usize,
    /// 是否已消费完最后一条（200）消息。
    eof: bool,
}

impl StreamRead for ReplyReader<'_> {
    fn read_exact(&mut self, out: &mut [u8]) -> Result<(), Error> {
        let mut filled = 0;
        while filled < out.len() {
            if self.avail == 0 {
                if self.eof {
                    return Err(Error::Disconnected);
                }
                self.pull_message()?;
            }
            let n = self.avail.min(out.len() - filled);
            self.conn.read_exact(&mut out[filled..filled + n])?;
            filled += n;
            self.avail -= n;
        }
        Ok(())
    }
}

impl ReplyReader<'_> {
    /// 读取下一消息头，登记数据段长度；错误码直接映射为 `Error::Server`。
    fn pull_message(&mut self) -> Result<(), Error> {
        let mut header = [0u8; size_of::<u32>() * 2];
        self.conn.read_exact(&mut header)?;
        let code = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        if size > MAX_MESSAGE {
            return Err(Error::BadResponse);
        }
        match code {
            RESPONSE_OK => {
                self.avail = size;
                self.eof = true;
            }
            RESPONSE_OK_MORE_DATA => {
                self.avail = size;
            }
            400..=599 => {
                // 错误消息的数据段不再读取；本连接此后不可复用（见 mod.rs 约定）。
                return Err(Error::Server(code));
            }
            _ => return Err(Error::BadResponse),
        }
        Ok(())
    }
}
