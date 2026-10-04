//! R29: 初始化失败显式化 —— 几何/DPI、COM、渲染设备、动画等核心初始化失败时
//! 弹原版格式错误框并**立即终止** (错误分支无正常返回路径, spec.md:141);
//! 例外按各自条目: 音效缺失 → 静默 (R24), 皮肤损坏 → 默认回落 (R27)。
//!
//! 格式串复刻原版断言宏 @0x1BC40:
//! `Error: {msg}\n\nHRESULT: {:#X}\n\nCode: {}\n\nFile: {}\n\nLine: {}`

use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

use crate::win::wide;

/// 弹错误详情框并终止进程 (永不返回)。`file`/`line` 由调用方传 `file!()`/`line!()`。
pub fn fatal(file: &str, line: u32, message: &str, hresult: u32) -> ! {
    let text = format!(
        "Error: {message}\n\nHRESULT: {hresult:#X}\n\nCode: {}\n\nFile: {file}\n\nLine: {line}",
        hresult as i32
    );
    let text_w = wide(&text);
    let caption_w = wide("KeyFlux-CommandInput");
    unsafe {
        MessageBoxW(
            None,
            PCWSTR::from_raw(text_w.as_ptr()),
            PCWSTR::from_raw(caption_w.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
    std::process::exit(1);
}
