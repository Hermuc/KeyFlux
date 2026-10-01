//! 随包字体的进程内加载（GDI `AddFontResourceExW`）。
//!
//! ⚠️ 仅影响字体表，不写系统字体目录、**不写注册表**；字体文件始终留在发布树内。
//! 字体文件随发布树位于 `<exe 目录>/fonts/*.ttf`（开发态回落
//! `<crate>/resources/fonts`，编译期路径）。
//!
//! ## 为什么标志位可配（2026-10-01）
//!
//! 实测（同进程 A/B）：`FR_PRIVATE` 加载的字体对 DirectWrite 的**系统字体集不可见**
//! （「加载 / 不加载」两种模式渲染结果逐字节相同）。Microsoft 的 Win32→DirectWrite
//! 迁移对照表写明：`AddFontResource` 走「**GDI 字体安装步骤**」进入系统字体集合，
//! DirectWrite 会自动监测并与其同步；而 `AddFontResourceEx(FR_PRIVATE)` 只把字体加进
//! **进程私有**子集，按族名解析看不到。社区同款现象（gamedev.net）：「`FR_PRIVATE`
//! 返回 1，但 `CreateTextFormat` 仍回落字体；换 `FR_NOT_ENUM` 或无标志即命中，
//! 用完记得 `RemoveFontResource`」。
//!
//! ## 实测矩阵（2026-10-01；DirectWrite 同引擎参照图 NCC + GDI 字体表枚举）
//!
//! | 标志 | 面板实际渲染 | 系统字体表 |
//! |---|---|---|
//! | `FR_PRIVATE` 0x10 | 回落**微软雅黑 Bold**（NCC 0.9925） | 不可见 |
//! | `FR_PRIVATE\|FR_NOT_ENUM` 0x30 | 同上（与 0x10 **逐像素 0 差**） | 不可见 |
//! | `FR_NOT_ENUM` 0x20 | **MiSans Bold**（NCC 0.9924） | **不可见** |
//! | 无标志 0x00 | **MiSans Bold**（与 0x20 逐像素 0 差） | **可见**（会污染其他程序的字体列表） |
//!
//! ⇒ 默认取 **`FR_NOT_ENUM`**：唯一同时满足「WinUI3 解析得到 MiSans」与「对系统字体表
//! 隐形」的档位。注意 `FR_PRIVATE` 位会**主导并屏蔽** DirectWrite 的按族名解析
//! （0x30 与 0x10 渲染逐像素相同）—— 即「进程私有」与「可被 XAML 按名解析」在本机互斥。
//! 标志位可用 `KEYFLUX_FONT_FLAGS` 覆盖，供回归取证（0x10 复现旧行为、0 复现公共表）。

#![allow(unsafe_code)] // 理由：GDI 字体加载只有 FFI 通道，封装面收敛到本文件

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// 进程私有字体标志（`AddFontResourceExW`）：只有本进程可用，进程退出后自动移除。
pub const FR_PRIVATE: u32 = 0x10;

/// 不可枚举标志：任何进程（含调用者）都不能枚举，但仍可按族名使用。
pub const FR_NOT_ENUM: u32 = 0x20;

/// 随包字体清单（与旧版 `Assets/Fonts` 一致，Regular/Medium/Semibold/Bold 四档）。
const FONT_FILES: [&str; 4] = [
    "MiSans-Regular.ttf",
    "MiSans-Medium.ttf",
    "MiSans-Semibold.ttf",
    "MiSans-Bold.ttf",
];

/// 生效的字体资源标志。默认 **`FR_NOT_ENUM`**（见模块文档的实测矩阵）。
///
/// 可用 `KEYFLUX_FONT_FLAGS` 覆盖（接受 `0x20` 与十进制 `32` 两种写法），
/// 供回归取证：`0x10` 复现"雅黑回退"旧行为、`0` 复现"进公共字体表"。
pub fn active_flags() -> u32 {
    std::env::var("KEYFLUX_FONT_FLAGS")
        .ok()
        .and_then(|raw| {
            let raw = raw.trim();
            match raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => raw.parse::<u32>().ok(),
            }
        })
        .unwrap_or(FR_NOT_ENUM)
}

/// 字体目录候选（按序探测）：发布树 `<exe>/fonts` → 开发态 crate `resources/fonts`。
fn font_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        directories.push(dir.join("fonts"));
    }
    // 编译期嵌入 crate 根：**仅限 debug 构建**。
    // 🔴 release exe 绝不能带开发机绝对路径：一来会泄露构建机目录结构（实测可在 exe 里
    //    grep 到 `D:\PortableApps\KeyFlux-main\config-ui-reactor`），二来若目标机器恰好存在
    //    同名路径，面板会去读**别人的**字体目录 —— 与"绿色便携"的承诺相悖。
    //    发布树永远由上面的 exe_dir/fonts 命中，本条只是开发态 `cargo test/run` 的兜底。
    #[cfg(debug_assertions)]
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        directories.push(PathBuf::from(manifest).join("resources").join("fonts"));
    }
    directories
}

/// UTF-16 + NUL 结尾的宽字符串（GDI 只读该缓冲）。
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 把一个字体文件按给定标志加入本进程字体表。
fn add_font_resource(path: &Path, flags: u32) -> bool {
    let name = wide(path);
    // SAFETY: name 以 NUL 结尾；pdv 按 API 约定必须为 null。
    let added = unsafe {
        windows_sys::Win32::Graphics::Gdi::AddFontResourceExW(
            name.as_ptr(),
            flags,
            std::ptr::null_mut(),
        )
    };
    added > 0
}

/// 把随包 MiSans 加入本进程字体表。返回**成功加载的文件路径**（空 = 无可用字体文件）。
///
/// 返回值供 [`unload_fonts`] 对称清理：无标志加载的字体**不会**随进程退出消失
/// （它进的是 GDI 公共字体表），必须显式移除。
pub fn load_private_fonts() -> Vec<PathBuf> {
    load_fonts(active_flags())
}

/// 按指定标志加载（标志显式传入，便于测试用保守档、不污染系统字体表）。
fn load_fonts(flags: u32) -> Vec<PathBuf> {
    let mut loaded = Vec::new();
    for directory in font_directories() {
        if !directory.is_dir() {
            continue;
        }
        for file in FONT_FILES {
            let path = directory.join(file);
            if !path.is_file() {
                continue;
            }
            if add_font_resource(&path, flags) {
                loaded.push(path);
            }
        }
        // 命中第一个存在的目录即停（避免重复加载同一文件两份）
        if !loaded.is_empty() {
            break;
        }
    }
    loaded
}

/// 移除先前加载的字体（`AddFontResourceExW` 的对称操作）。
///
/// 传 `flags` 必须与添加时一致，否则计数不会归零、字体也不撤销。
pub fn unload_fonts(paths: &[PathBuf], flags: u32) {
    for path in paths {
        let name = wide(path);
        // SAFETY: name 以 NUL 结尾；pdv 按 API 约定必须为 null。
        unsafe {
            windows_sys::Win32::Graphics::Gdi::RemoveFontResourceExW(
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
            );
        }
    }
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
    fn load_and_unload_four_fonts() {
        // 测试一律走 FR_PRIVATE（进程私有、退出即清），避免污染系统字体表
        let loaded = load_fonts(FR_PRIVATE);
        assert_eq!(loaded.len(), 4, "四档 MiSans 应全部加载");
        unload_fonts(&loaded, FR_PRIVATE);
    }

    #[test]
    fn flags_default_to_not_enum() {
        // 未设 KEYFLUX_FONT_FLAGS 时取 FR_NOT_ENUM：既让 WinUI3 解析到 MiSans，
        // 又不把字体暴露给系统字体枚举（见模块文档的实测矩阵）
        if std::env::var_os("KEYFLUX_FONT_FLAGS").is_none() {
            assert_eq!(active_flags(), FR_NOT_ENUM, "缺省应为 FR_NOT_ENUM");
        }
    }
}
