//! UAC 提权启动 —— `ShellExecuteW` 的 `runas` 动词封装（AHK `Run '*RunAs'` 同机制）。
//!
//! 为什么需要（2026-10-07 用户报障「关闭开机自启保存后仍显示开启」的第二根因）：
//! 计划任务 KeyFlux 由提权链创建（runLevel HIGHEST），非提权上下文
//! `schtasks /delete` 直接「拒绝访问」（本机实测）；而 `bin/MiscTools.ahk` 仅
//! On 分支内建 `*RunAs` 自提权（`bin/**` 边界零改动，无法给 Off 补）⇒ 面板后端
//! 对自启命令 3/4 统一经本封装提权 spawn：已提权调用方（引擎托盘拉起的面板链）
//! 无感直启，未提权调用方弹一次 UAC —— 与 On 的既有 UX 对称，且不会二次叠加
//! （提权后的 MiscTools 看到 `A_IsAdmin` 即真，跳过自身 `*RunAs`）。

#![allow(unsafe_code)] // 理由：ShellExecuteW 只有 FFI 通道，封装面已收敛到本文件

use std::path::Path;

use windows_sys::Win32::UI::Shell::ShellExecuteW;

/// ShellExecuteW 的 `nShowCmd` 取值：正常显示。
const SW_SHOWNORMAL: i32 = 1;

/// UTF-16（NUL 结尾）—— ShellExecuteW 入参口径。
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 参数拼进命令行串（含空格/引号者加引号；自启白名单参数不含嵌入引号，取最简口径）。
fn quote_arg(arg: &str) -> String {
    if arg.is_empty() || arg.chars().any(|c| matches!(c, ' ' | '\t' | '"')) {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

/// 经 UAC `runas` 动词提权启动 `exe`（**必须已是绝对路径**，解析归上层负责），
/// `working_dir` 为子进程工作目录。返回是否成功发起（ShellExecuteW 异步返回，
/// 不等子进程退出）。
pub fn spawn_elevated(exe: &Path, args: &[&str], working_dir: &Path) -> bool {
    let (Some(file), Some(directory)) = (exe.to_str(), working_dir.to_str()) else {
        crate::devlog!("spawnElevated: 路径非 UTF-16 可表示: {exe:?}");
        return false;
    };
    let verb = to_wide("runas");
    let file = to_wide(file);
    let parameters = to_wide(
        &args
            .iter()
            .map(|arg| quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let directory = to_wide(directory);

    // SAFETY: 四个字符串均以 NUL 结尾，指针仅在本调用期间使用；ShellExecuteW
    // 不保存入参。返回值（HINSTANCE 按文档截断为 int 比较）≤ 31 为 SE_ERR_*
    // 失败码（文档口径：> 32 成功）。
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            directory.as_ptr(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        crate::devlog!(
            "spawnElevated: 提权启动 {} 失败: SE_ERR {result}",
            exe.display()
        );
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 引语口径：空白/引号者加引号，普通词原样（与标准命令行解析兼容的最简集）。
    #[test]
    fn quote_arg_wraps_whitespace_only() {
        assert_eq!(quote_arg("/script"), "/script");
        assert_eq!(quote_arg("./bin/MiscTools.ahk"), "./bin/MiscTools.ahk");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
    }

    /// UTF-16 转换含 NUL 结尾（ShellExecuteW 的 C 字符串契约）。
    #[test]
    fn to_wide_appends_nul_terminator() {
        assert_eq!(
            to_wide("runas"),
            vec![
                b'r' as u16,
                b'u' as u16,
                b'n' as u16,
                b'a' as u16,
                b's' as u16,
                0
            ]
        );
    }
}
