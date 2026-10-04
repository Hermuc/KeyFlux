//! 系统文件图标提取 (SHGetFileInfoW) + 进程内缓存。
//!
//! 解耦口径 (2026-10-04, Flow Launcher 结果行版式): 命令框只认「**路径 → 系统图标**」
//! 这一通用渲染能力 (与 DrawTextW 同级), 不知道任何插件 —— 插件只经 0x406 把路径
//! 作为 `Item::subtitle` 推进来, 图标即随之出现; 移除插件 = 不发消息, 本模块零改动。
//! 与 Flow Launcher 的架构一致: 图标由 UI 层自行从文件路径提取 (Result.Image → WPF Image),
//! 数据源只提供路径。
//!
//! 资源纪律: 每个图标 `DestroyIcon` 归还; 缓存超容量整体清空 (下次重取, 正确性不变);
//! `Drop` 兜底销毁。线程约束: 仅 UI 线程使用 (与 GDI 后端同线程, RefCell 足够)。

use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

/// 缓存容量上限 (每次会话结果 ≤ 数十行; 超限 = 整体清空重来, 简单且正确)。
const CACHE_CAP: usize = 512;

/// 图标缓存 (path → HICON)。borrow 粒度 = 单次 `get`。
pub struct IconCache {
    map: RefCell<HashMap<String, HICON>>,
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IconCache {
    pub fn new() -> Self {
        Self {
            map: RefCell::new(HashMap::new()),
        }
    }

    /// 取路径的系统图标 (32px 大图标, 绘制时由 `DrawIconEx` 缩放到行内图标盒)。
    /// 提取失败 → `None` (行内只画文字, 不带崩渲染)。
    pub fn get(&self, path: &str) -> Option<HICON> {
        if let Some(&h) = self.map.borrow().get(path) {
            return Some(h);
        }
        let h = Self::extract(path)?;
        let mut map = self.map.borrow_mut();
        if map.len() >= CACHE_CAP {
            // 先清空 (归还全部旧图标) 再插入 —— 新图标尚未入表, 不受影响
            for (_, old) in map.drain() {
                unsafe {
                    let _ = DestroyIcon(old);
                }
            }
        }
        map.insert(path.to_string(), h);
        Some(h)
    }

    /// 清空缓存并归还全部图标。
    pub fn clear(&self) {
        for (_, h) in self.map.borrow_mut().drain() {
            unsafe {
                let _ = DestroyIcon(h);
            }
        }
    }

    fn extract(path: &str) -> Option<HICON> {
        if path.is_empty() {
            return None;
        }
        // NUL 结尾 UTF-16 (SHGetFileInfoW 不接受长度参数)
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let mut fi = SHFILEINFOW::default();
            let ok = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut fi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            if ok != 0 && !fi.hIcon.is_invalid() {
                return Some(fi.hIcon);
            }
            // 文件可能不在盘上 (Everything 索引态/已删除): 按扩展名兜底取关联图标
            let mut fi2 = SHFILEINFOW::default();
            let ok2 = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut fi2),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES,
            );
            if ok2 != 0 && !fi2.hIcon.is_invalid() {
                return Some(fi2.hIcon);
            }
            None
        }
    }
}

impl Drop for IconCache {
    fn drop(&mut self) {
        self.clear();
    }
}
