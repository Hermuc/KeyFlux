//! 文档静态资源基址（**进程级、只写一次**，与 [`crate::services::transport`] 同为进程级配置）。
//!
//! 背景：使用指南的图片与内部链接原本一律指向后端静态站 `http://127.0.0.1:<port>/…`。
//! 实测该静态站当前整体 404（Vue UI 退役后无人验证的旧账，2026-10-05）⇒ **装载时
//! 一律登记** `<deploy>/bin/site`（glue::assemble，两种传输统一）：
//! * 图片改为**直读文件 + `Image::source_data`**（reactor 内建的 WinRT 流式加载，
//!   最稳，不依赖 `BitmapImage` 接受哪种 URI 方案）；
//! * 内部链接改为 `file:///` URI（交系统默认程序打开）。
//!
//! 未登记时保持旧行为（走后端静态站）—— 保留该回退分支作机制完整性。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static SITE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 登记本地静态站目录（`<deploy>/bin/site`）。进程内只生效第一次。
pub fn set_site_dir(dir: Option<PathBuf>) {
    let _ = SITE_DIR.set(dir);
}

/// 当前登记的本地静态站目录（`None` ⇒ 走 HTTP 静态站）。
pub fn site_dir() -> Option<&'static PathBuf> {
    SITE_DIR.get().and_then(|dir| dir.as_ref())
}

/// 资源相对路径（`/img/a.png` 或 `img/a.png`）→ 绝对路径。未登记时返回 `None`。
pub fn asset_path(src: &str) -> Option<PathBuf> {
    let dir = site_dir()?;
    Some(dir.join(src.trim_start_matches('/')))
}

/// 绝对路径 → `file:///` URI（交系统默认程序打开；反斜杠归一化为正斜杠）。
pub fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = text.strip_prefix("//") {
        format!("file://{rest}")
    } else {
        format!("file:///{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_normalizes_backslashes() {
        let uri = file_uri(Path::new(r"D:\PortableApps\KeyFlux\bin\site\img\a.png"));
        assert_eq!(uri, "file:///D:/PortableApps/KeyFlux/bin/site/img/a.png");
    }

    #[test]
    fn asset_path_is_none_when_unregistered() {
        // ⚠️ 本 crate 的测试**不得**调用 `set_site_dir` —— `OnceLock` 是进程级的，
        // 一旦登记会让 `markdown_view` 的 HTTP 断言（http://127.0.0.1:port/…）变成
        // `file:///` 结果，造成跨测试的隐性污染。此断言顺便守住这条纪律。
        assert!(asset_path("/img/a.png").is_none());
    }
}
