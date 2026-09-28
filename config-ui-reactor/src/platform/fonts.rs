//! 随包字体的进程内私有加载（GDI `AddFontResourceExW` + `FR_PRIVATE`）。
//!
//! ⚠️ 仅影响**本进程**字体表，不写系统目录、不写注册表（卸载即消失）。
//! 字体文件随发布树位于 `<exe 目录>/fonts/*.ttf`（开发态回落
//! `<crate>/resources/fonts`，编译期路径）。加载是否对 DirectWrite 生效由
//! 截图字形验收判定；若不可见，fallback 方案为用户级字体安装（见迁移文档 17 号）。

#![allow(unsafe_code)] // 理由：GDI 字体加载只有 FFI 通道，封装面收敛到本文件

use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

/// 进程私有字体标志（`AddFontResourceExW`）。
const FR_PRIVATE: u32 = 0x10;

/// 随包字体清单（与旧版 `Assets/Fonts` 一致，Regular/Medium/Semibold/Bold 四档）。
const FONT_FILES: [&str; 4] = [
    "MiSans-Regular.ttf",
    "MiSans-Medium.ttf",
    "MiSans-Semibold.ttf",
    "MiSans-Bold.ttf",
];

/// 字体目录候选（按序探测）：发布树 `<exe>/fonts` → 开发态 crate `resources/fonts`。
fn font_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        directories.push(dir.join("fonts"));
    }
    // 编译期嵌入 crate 根（仅开发机构建生效；发布树由 exe_dir 命中）
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        directories.push(PathBuf::from(manifest).join("resources").join("fonts"));
    }
    directories
}

/// 把随包 MiSans 私有加载进本进程。返回成功加载的文件数（0 = 无可用字体文件）。
pub fn load_private_fonts() -> usize {
    let mut loaded = 0;
    for directory in font_directories() {
        if !directory.is_dir() {
            continue;
        }
        for file in FONT_FILES {
            let path = directory.join(file);
            if !path.is_file() {
                continue;
            }
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: wide 以 NUL 结尾，AddFontResourceExW 只读该缓冲
            let added = unsafe {
                windows_sys::Win32::Graphics::Gdi::AddFontResourceExW(
                    wide.as_ptr(),
                    FR_PRIVATE,
                    std::ptr::null_mut(),
                )
            };
            if added > 0 {
                loaded += 1;
            }
        }
        // 命中第一个存在的目录即停（避免重复加载同一文件两份）
        if loaded > 0 {
            break;
        }
    }
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_resources_dir_has_misans() {
        // 开发态（本 crate 构建）下 resources/fonts 必然在候选目录里且含四档字体
        let directories = font_directories();
        let dev = directories.iter().find(|dir| {
            dir.file_name() == Some(std::ffi::OsStr::new("fonts"))
                && dir.join("MiSans-Regular.ttf").is_file()
        });
        assert!(dev.is_some(), "应能在 resources/fonts 找到随包 MiSans");
        let files = std::fs::read_dir(dev.expect("已判定存在"))
            .expect("目录可读")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension() == Some(std::ffi::OsStr::new("ttf")))
            .count();
        assert!(files >= 4, "至少 4 个 ttf，实际 {files}");
    }

    #[test]
    fn load_private_fonts_reports_count() {
        // 本机开发态：资源目录存在 ⇒ 应加载 4 个文件（进程私有，无副作用）
        assert_eq!(load_private_fonts(), 4, "四档 MiSans 应全部加载");
    }
}
