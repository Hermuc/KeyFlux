//! DPI awareness (design C §1 #8 实测的**必要**步骤) + R11 DPI 采集。
//!
//! 实测依据 (design C §1 #8 探针): 非 DPI-aware 进程 `GetDpiForMonitor` 返回 96、
//! `GetSystemMetrics` 虚拟化为 1536×960, R11 几何公式全错; 显式 PMv2 后 =120、
//! 1920×1200, 公式闭环 925×200 / X=497 / Y=300 精确复现。
//! 无清单工具链 (无 build.rs / rc.exe), 故用运行时 API 三级声明 (design A §5 R11)。

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwareness, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI, PROCESS_PER_MONITOR_DPI_AWARE,
};
use windows::Win32::UI::WindowsAndMessaging::SetProcessDPIAware;

/// 进程第一步 (先于一切窗口 / DPI 查询): PMv2 → shcore PMDA → SetProcessDPIAware
/// 三级兜底。失败不硬退 (读 awareness 已无意义, 兜底后按实得值继续 ——
/// 若三级全失败, 进程保持 unaware, 几何公式按 96 折算, 不崩溃)。
pub fn make_process_dpi_aware() {
    // SAFETY: 三个 API 都是进程级、无指针入参；本函数在进程初始化最早期被调用
    // （窗口创建之前），返回值仅用于探测哪一级可用，不构成内存安全前置条件。
    unsafe {
        // 1) Win10 1703+: PER_MONITOR_AWARE_V2 (探针实测有效)
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() {
            return;
        }
        // 2) Win8.1+: shcore PROCESS_PER_MONITOR_DPI_AWARE
        if SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).is_ok() {
            return;
        }
        // 3) Vista+: 系统 aware (GetSystemMetrics 起效, GetDpiForMonitor 不可用)
        let _ = SetProcessDPIAware();
    }
}

/// R11: `MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST=2)` →
/// `GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI=0, &dpiX, &dpiY)`。
/// 失败返回 Err(HRESULT), 调用方按 R29/R11 失败路径弹窗终止。
pub fn monitor_dpi(hwnd: HWND) -> Result<(f64, f64), u32> {
    // SAFETY: MonitorFromWindow 接受任意 hwnd（无效时返回无效 HMONITOR，下面显式判
    // is_invalid）；dx/dy 是本函数持有的可写局部变量，生命周期覆盖调用期间。
    unsafe {
        let mon: HMONITOR = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if mon.is_invalid() {
            return Err(0x8007_0009); // E_HANDLE: 无最近显示器 (异常环境)
        }
        let (mut dx, mut dy) = (0u32, 0u32);
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy)
            .map_err(|e| e.code().0 as u32)?;
        Ok((dx as f64, dy as f64))
    }
}
