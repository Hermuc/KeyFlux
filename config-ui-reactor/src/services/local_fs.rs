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

/// 快捷方式列表：`<deploy>/shortcuts/*.lnk`，返回**相对部署根**的路径。
///
/// 与 `GetShortcutsHandler` **逐字同口径**（2026-09-29 实测对账后修正两处易踩差异）：
/// * 分隔符 —— Go 用 `filepath.Glob` 得到的是 `<root>\shortcuts\X.lnk`，再按
///   `f[len(root)+1:]` 截取 ⇒ 结果是**反斜杠**（`shortcuts\X.lnk`）。此处用
///   [`Path::join`] 生成同样的反斜杠形态；**不能手写 `shortcuts/{name}`**（正斜杠会
///   与后端返回值不等，实测已复现）。
/// * 条目类型 —— `filepath.Glob` 只按**名字**匹配，不 stat、因此**目录条目也算命中**。
///   这里刻意不加 `is_file()` 过滤以保持同口径（多过滤一个 `foo.lnk` 目录即行为变更）。
///
/// 排序与 `filepath.Glob` 一致（字典序）。目录不存在 ⇒ `None`（回退后端）。
pub fn list_shortcuts(deploy_root: &Path) -> Option<Vec<String>> {
    let dir = deploy_root.join("shortcuts");
    if !dir.is_dir() {
        return None;
    }
    let mut names = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".lnk"))
        .collect::<Vec<_>>();
    names.sort();
    Some(
        names
            .into_iter()
            .map(|name| {
                Path::new("shortcuts")
                    .join(name)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect(),
    )
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

    #[test]
    fn list_shortcuts_returns_sorted_backslash_paths() {
        let tree = TempDeploy::new("lnk");
        let dir = tree.root().join("shortcuts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.lnk"), b"x").unwrap();
        std::fs::write(dir.join("a.lnk"), b"x").unwrap();
        std::fs::write(dir.join("note.txt"), b"x").unwrap(); // 非 .lnk 忽略
        std::fs::create_dir_all(dir.join("sub.lnk")).unwrap(); // 目录：与 Go `filepath.Glob` 同口径，**计入**

        // 分隔符必须是 `\`（后端口径）；`sub.lnk` 目录也计入 ⇒ 三项目。
        assert_eq!(
            list_shortcuts(tree.root()).unwrap(),
            vec![r"shortcuts\a.lnk", r"shortcuts\b.lnk", r"shortcuts\sub.lnk"]
        );
    }

    #[test]
    fn list_shortcuts_is_none_when_directory_absent() {
        let tree = TempDeploy::new("none");
        assert!(list_shortcuts(tree.root()).is_none());
    }

    #[test]
    fn list_shortcuts_empty_directory_is_some_empty() {
        // 目录存在但为空 ⇒ `Some(vec![])`（与后端返回 null 后 `unwrap_or_default()` 等价，
        // 但**不**触发回退 —— 空目录是合法状态，不应再往返一次后端）。
        let tree = TempDeploy::new("empty");
        std::fs::create_dir_all(tree.root().join("shortcuts")).unwrap();
        assert_eq!(list_shortcuts(tree.root()), Some(Vec::new()));
    }
}
