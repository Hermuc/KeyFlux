//! 用系统默认关联程序打开路径 —— `ShellExecuteW`（默认 `open` 动词）封装。
//!
//! 为什么单独成文件：`platform` 是「reactor 与原生互操作」的**唯一**落点，且一个能力一个
//! 文件（与 [`elevation`](super::elevation) 同款）。elevation 用 `runas` 动词提权，这里用
//! 默认动词打开目标 —— 供「查看引擎日志」入口使用，避免引入 `cmd /c start` 这类间接层。

#![allow(unsafe_code)] // 理由：ShellExecuteW 只有 FFI 通道，封装面已收敛到本文件

use std::path::Path;

use windows_sys::Win32::UI::Shell::ShellExecuteW;

/// ShellExecuteW 的 `nShowCmd` 取值：正常显示。
const SW_SHOWNORMAL: i32 = 1;

/// UTF-16（NUL 结尾）—— ShellExecuteW 入参口径。
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 用系统默认关联程序打开 `path`（文件或目录；**应为绝对路径**，解析归上层负责）。
/// 返回是否成功发起（ShellExecuteW 异步返回，不等目标程序退出）。
pub fn open_path(path: &Path) -> bool {
    let Some(target) = path.to_str() else {
        crate::devlog!("openPath: 路径非 UTF-16 可表示: {path:?}");
        return false;
    };
    let verb = to_wide("open");
    let target = to_wide(target);

    // SAFETY: 两个字符串均以 NUL 结尾，指针仅在本调用期间使用；ShellExecuteW 不保存入参。
    // 返回值（HINSTANCE 按文档截断为 int 比较）≤ 32 为 SE_ERR_* 失败码（> 32 成功）。
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        crate::devlog!("openPath: 打开 {} 失败: SE_ERR {result}", path.display());
        return false;
    }
    true
}
