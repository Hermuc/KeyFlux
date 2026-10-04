//! R30: 命名互斥体与重启式接管。
//!
//! - `CreateMutexW(NULL, TRUE, 互斥名)` (bInitialOwner=TRUE);互斥名 = config::MUTEX_NAME
//!   (strings.txt:7638 逐字符, 外部识别 / 兼容面, 保持原值);
//! - `GetLastError() == ERROR_ALREADY_EXISTS(183)` → `FindWindowW(类名)` →
//!   `PostMessageW(旧hwnd, WM_CLOSE)` → **不检查发送结果** → 新进程**不退出**,
//!   照常完成初始化并接管运行;
//! - 互斥体并不阻止第二进程存活 (跨完整性级别并存是实测常态, R31)。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

use crate::config;
use crate::win::wide;

/// 启动即调用。互斥句柄有意泄漏 (释放即失互斥, 进程生命周期 = 互斥生命周期)。
pub fn acquire_and_notify() {
    unsafe {
        let name = wide(config::MUTEX_NAME);
        let mutex = CreateMutexW(None, true, PCWSTR::from_raw(name.as_ptr()));
        // GetLastError 时序: CreateMutexW 之后、任何其他可置 LastError 的调用之前立即读
        // (design A 实现要点备忘 6)。windows crate 的 Result 包装不消费 ALREADY_EXISTS。
        let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;
        // HANDLE 非 RAII (无 Drop): 不关闭即进程生命周期持有, 无需 forget
        drop(mutex);
        if already_exists {
            let class = wide(config::CLASS_NAME);
            // FindWindowW 命中隐藏窗口 (注册即存在, R5); 未命中 (旧实例刚退出) 静默跳过
            if let Ok(old) = FindWindowW(PCWSTR::from_raw(class.as_ptr()), PCWSTR::null()) {
                // R30: 不检查发送结果 —— 握手失败是静默的, 无任何错误提示
                let _ = PostMessageW(Some(old), WM_CLOSE, Default::default(), Default::default());
            }
        }
    }
}
