//! 纯文件读取（不经后端）：使用指南文档与快捷方式列表。
//!
//! 这两项在 Go 侧**没有任何变换** —— `/config_doc.md` 由静态站直出 `<cwd>/site/config_doc.md`，
//! `/shortcuts` 只是 `<deploy>/shortcuts/*.lnk` 的目录 glob。既然面板与后端同在
//! 一台机器、同一个部署树，直读即可省掉一次后端往返（CLI 传输下每次往返约 150 ms）。
//!
//! ⚠️ **不适用于 `GET /config`**：它经 `ConfigToDTO` 补 `fileGroups` / `matchTypes` /
//! `options.commandFont` / `options.plugins`，并由 `schtasks` 回填 `options.startup`
//! ⇒ 直读 `config.json` 会**静默丢字段**，仍必须走后端。
//!
//! 所有函数在"读不到"时返回 `None`（目录/文件缺失），由调用方回退后端 —— 即最坏情况
//! 等于改动前的行为。

use std::path::{Path, PathBuf};

/// 面板静态站目录：`<deploy>/bin/site`（`Makefile` 由 `site-assets/` 拷入）。
pub fn site_dir(deploy_root: &Path) -> PathBuf {
    deploy_root.join("bin").join("site")
}

/// 读静态站文本（`rel` 可带前导 `/`）。文件缺失或非 UTF-8 ⇒ `None`。
pub fn read_site_text(deploy_root: &Path, rel: &str) -> Option<String> {
    let path = site_dir(deploy_root).join(rel.trim_start_matches('/'));
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一次性临时部署树（测试结束即删）。
    struct TempDeploy {
        root: PathBuf,
    }

    impl TempDeploy {
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("kf-localfs-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("bin").join("site")).unwrap();
            Self { root }
        }

        fn root(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TempDeploy {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn read_site_text_accepts_leading_slash() {
        let tree = TempDeploy::new("site");
        std::fs::write(
            tree.root().join("bin").join("site").join("config_doc.md"),
            "# 指南",
        )
        .unwrap();

        assert_eq!(
            read_site_text(tree.root(), "/config_doc.md").as_deref(),
            Some("# 指南")
        );
        // 不带前导 `/` 同样工作（静态站路径两种写法都出现过）。
        assert_eq!(
            read_site_text(tree.root(), "config_doc.md").as_deref(),
            Some("# 指南")
        );
        // 缺失 ⇒ None（调用方回退后端）。
        assert!(read_site_text(tree.root(), "/missing.md").is_none());
    }
}
