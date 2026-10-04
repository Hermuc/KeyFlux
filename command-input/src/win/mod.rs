//! 系统粘合层 (唯一触碰 Win32 符号的模块群)。
//!
//! 模块划分遵从 design C §2.7:
//!   - `app`             进程装配壳 (dpi → COM → 皮肤 → 单实例 → 类注册 → 建窗 → 消息循环);
//!   - `dpi`             DPI awareness (实测必要前提) + R11 采集;
//!   - `single_instance` R30 命名互斥体与接管;
//!   - `resources`       R25 资源基点 = exe 目录;
//!   - `error`           R29 初始化失败显式化 (原版格式弹窗 + 终止);
//!   - `audio`           R24 winmm 音效后端;
//!   - `backend_gdi`     v1 渲染后端 (RenderBackend 实现);
//!   - `shell_icon`      系统文件图标提取 + 缓存 (SHGetFileInfoW; Flow 版式结果行用);
//!   - `wndproc`         唯一 Win32→core 翻译层 (协议速查表逐行)。

pub mod app;
pub mod audio;
pub mod backend_gdi;
pub mod dpi;
pub mod error;
pub mod resources;
pub mod shell_icon;
pub mod single_instance;
pub mod wndproc;

/// UTF-16 + NUL 终结 (Windows 宽字符串参数构造)。
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
