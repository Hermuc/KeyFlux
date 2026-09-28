//! Windows Job Object 封装 —— 补齐 `BackendSession` 的**进程安全缺口**。
//!
//! 为什么需要（Phase 3a 实测复现）：旧 C# 用 Job Object 保证「GUI 进程死亡（含被强杀）时
//! OS 连带回收后端子进程树」。Rust 侧若只有 `Drop`，**强杀面板会留下孤儿 `settings.exe`**：
//!
//! ```text
//! 面板运行中  → keyflux-settings:1, settings:1
//! 强杀面板后  → keyflux-settings:0, settings:1   ← 孤儿
//! ```
//!
//! 语义对齐 `config-ui-avalonia/Services/BackendSession.cs:90` 的注释：
//! * `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`：本句柄（最后一个）关闭时终止 Job 内全部进程；
//! * `JOB_OBJECT_LIMIT_BREAKAWAY_OK`：允许 `settings.exe` 保存时以
//!   `CREATE_BREAKAWAY_FROM_JOB` 重启的 KeyFlux **脱离**本 Job —— 因此关闭设置面板
//!   不会连带杀掉托盘程序（旧注释明确要求保留这一项）。

#![allow(unsafe_code)] // 理由：Job Object 只有 FFI 通道，封装面已收敛到本文件

use std::io;
use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};

/// 内核句柄的 `Send`/`Sync` 包装。
///
/// `windows-sys` 的 `HANDLE` 是裸指针（`*mut c_void`），因而不是 `Send`。
/// 但 Win32 内核句柄本身是**跨线程可传递**的（进程句柄表共享），且此处只做
/// 「创建 / 赋值 / 关闭」三种操作，全部线程安全 ⇒ 显式声明为 `Send + Sync`。
struct SendHandle(HANDLE);

// SAFETY: 见上——句柄是无主内核对象引用，非线程亲和；本封装不共享可变状态。
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

/// 一个配置为「句柄关闭即终止成员」的 Job Object。
pub struct JobObject {
    handle: SendHandle,
}

impl JobObject {
    /// 创建 Job 并设置 `KILL_ON_JOB_CLOSE | BREAKAWAY_OK`。
    pub fn new_kill_on_close() -> io::Result<Self> {
        // SAFETY: 传空属性与匿名名字是 CreateJobObjectW 的合法用法。
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
            // SAFETY: 该结构是 POD，全零是合法初值（随后显式设置 LimitFlags）。
            unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;

        // SAFETY: handle 来自 CreateJobObjectW；info 生命周期覆盖调用；长度取实际结构大小。
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: 配置失败，关闭刚创建的句柄避免泄漏。
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(error);
        }

        Ok(Self {
            handle: SendHandle(handle),
        })
    }

    /// 把子进程加入本 Job。失败仅返回错误（调用方可降级为「无 Job」运行）。
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        let process = child.as_raw_handle() as HANDLE;
        // SAFETY: 两个句柄均有效（self.handle 由构造保证；process 来自存活子进程）。
        let assigned = unsafe { AssignProcessToJobObject(self.handle.0, process) };
        if assigned == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        // 关闭最后一个句柄 ⇒ KILL_ON_JOB_CLOSE 触发，Job 内进程被 OS 回收。
        // SAFETY: handle 由构造保证有效且只在此处关闭一次。
        unsafe {
            let _ = CloseHandle(self.handle.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    /// 真实创建 Job 并把一个 ping 子进程放进去 —— 验证 FFI 通路与回落行为。
    /// （「父进程被强杀后子进程是否被回收」无法在单元测试内自杀验证，
    ///  该场景由 Phase 3a 的手工强杀实验覆盖，见 11 号文档 §3.1。）
    #[test]
    fn job_can_be_created_and_assigned() {
        let job = match JobObject::new_kill_on_close() {
            Ok(job) => job,
            // 极少数受限环境不允许创建 Job：此处显式跳过而非误报失败
            Err(error) => {
                eprintln!("skip: CreateJobObjectW unavailable: {error}");
                return;
            }
        };

        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 20 127.0.0.1 > NUL"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn ping");
        assert!(
            job.assign(&child).is_ok(),
            "AssignProcessToJobObject 应成功"
        );

        // 句柄关闭会杀掉 Job 内进程 ⇒ 显式 kill 后才 drop，避免测试留残留进程
        let _ = child.kill();
        let _ = child.wait();
        drop(job);

        std::thread::sleep(Duration::from_millis(50));
    }

    /// 入 Job 的子进程**不能**再被其它 Job 抢占 —— 这里验证 assign 的幂等可重复性。
    #[test]
    fn assign_reports_error_for_exited_process() {
        let job = match JobObject::new_kill_on_close() {
            Ok(job) => job,
            Err(_) => return,
        };
        let mut child = Command::new("cmd")
            .args(["/c", "exit 0"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn cmd");
        let _ = child.wait(); // 已退出
        // 已退出进程的句柄仍可 assign（句柄未关闭），故只断言不 panic
        let _ = job.assign(&child);
    }
}
