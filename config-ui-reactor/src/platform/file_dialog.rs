//! Win32 经典文件选择对话框（`GetOpenFileNameW`）。
//!
//! reactor 0.100.0 未暴露文件选择 API（WinRT `FileOpenPicker` 需要窗口句柄与 COM 互操作），
//! 故直接用 comdlg32 的 `OPENFILENAMEW`：**无需 COM 初始化、无线程模型要求**，
//! 且这是 Windows 上「导入 .zip」的既有交互。
//!
//! 所有者窗口取当前前台窗口（用户刚点下按钮 ⇒ 即本面板），使对话框正确居中并模态化。

#![allow(unsafe_code)] // 理由：comdlg32 只有 FFI 通道，封装面已收敛到本文件

use std::path::PathBuf;

use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// 文件名缓冲区长度（Windows 上限 32767 个宽字符）。
const BUFFER_LEN: usize = 32768;

/// 弹出「打开文件」对话框，返回所选路径（用户取消时为 `None`）。
///
/// `filter` 采用 Win32 双零结尾格式，例如 `"插件包 (*.zip)\0*.zip\0所有文件 (*.*)\0*.*\0\0"`。
pub fn pick_open_file(title: &str, filter: &str) -> Option<PathBuf> {
    let title_wide = wide_null(title);
    let filter_wide = wide_null(filter);
    let mut buffer = vec![0u16; BUFFER_LEN];

    // SAFETY: 所有指针均指向在本函数栈/堆上存活到调用结束的缓冲；缓冲区长度与 `nMaxFile` 一致；
    // `OPENFILENAMEW` 由系统按 `lStructSize` 校验版本，字段均已按结构体定义初始化。
    let opened = unsafe {
        let mut params: OPENFILENAMEW = std::mem::zeroed();
        params.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        params.hwndOwner = GetForegroundWindow();
        params.lpstrFilter = filter_wide.as_ptr();
        params.lpstrFile = buffer.as_mut_ptr();
        params.nMaxFile = BUFFER_LEN as u32;
        params.lpstrTitle = title_wide.as_ptr();
        params.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
        GetOpenFileNameW(&mut params)
    };

    if opened == 0 {
        return None; // 用户取消或失败
    }

    let length = buffer.iter().position(|unit| *unit == 0)?;
    if length == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
}

/// `&str` → 以 `\0` 结尾的宽字符串（**保留内部 `\0`**，过滤器依赖该特性）。
fn wide_null(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}

/// 插件包（`.zip`）的过滤器串。
pub const ZIP_FILTER: &str = "插件包 (*.zip)\0*.zip\0所有文件 (*.*)\0*.*\0\0";

/// 不限类型的过滤器串（插件设置项未声明 `filter` 时兜底）。
pub const ALL_FILES_FILTER: &str = "所有文件 (*.*)\0*.*\0\0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_null_appends_terminator() {
        let wide = wide_null("ab");
        assert_eq!(wide, vec![b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn zip_filter_keeps_embedded_separators() {
        // Win32 过滤器要求内部以 \0 分隔、整体以 \0\0 结尾
        let wide = wide_null(ZIP_FILTER);
        assert_eq!(*wide.last().unwrap(), 0);
        assert_eq!(wide[wide.len() - 2], 0, "必须以双零结尾");
        assert!(wide.contains(&0), "内部含分隔零");
    }
}
