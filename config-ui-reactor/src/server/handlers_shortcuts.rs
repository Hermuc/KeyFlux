//! `GET /shortcuts` —— Go `internal/server/handlers.go:93-115`
//! （`GetShortcutsHandler`）的移植。
//!
//! 语义：`os.Executable` 目录的**上两级**（部署树 `<root>/bin/settings.exe` ⇒
//! `<root>`）下 `shortcuts/*.lnk` 的 glob，路径以**相对 root**（Windows 分隔符）
//! 返回。`data` 是 `[]shortcut` nil slice：无匹配 ⇒ 输出 **`null`**（4 字节，
//! 基线钉死），非空 ⇒ `[{path:…},…]`。
//!
//! 错误口径：`os.Executable` 失败 / Glob 读取错误（目录存在但不可读）→ panic
//! → gin Recovery → **500 空 body**；`shortcuts/` 目录**缺失** = 空 glob（正常）。

use super::dto::marshal_go_json;
use super::{HttpReply, ServerContext};

/// Go `filepath.Glob(root/shortcuts/*.lnk)` 的等价实现：
/// 目录缺失 ⇒ 空；读取错误 ⇒ Err（500）；名字区分大小写地以 `.lnk` 结尾
/// （Go `Match("*.lnk", name)` 的语义，`*` 匹配含空串的任意无分隔符序列）；
/// 结果按名字节序排序（Glob 返回前排序）。
fn glob_shortcuts(root: &std::path::Path) -> Result<Vec<String>, std::io::Error> {
    let dir = root.join("shortcuts");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".lnk"))
        .collect();
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| format!("shortcuts\\{name}"))
        .collect())
}

/// 响应体：nil slice → `null`，否则 `[{"path":…},…]`（gin c.JSON 口径）。
fn shortcuts_body(files: &[String]) -> String {
    if files.is_empty() {
        return "null".to_string();
    }
    marshal_go_json(&serde_json::json!(
        files
            .iter()
            .map(|path| serde_json::json!({ "path": path }))
            .collect::<Vec<_>>()
    ))
}

/// Go `GetShortcutsHandler`。
pub(crate) fn get_shortcuts(_ctx: &ServerContext) -> HttpReply {
    let Ok(exe) = std::env::current_exe() else {
        return HttpReply::empty(500); // Go panic(err) → Recovery
    };
    let Some(root) = exe.parent().and_then(std::path::Path::parent) else {
        return HttpReply::empty(500);
    };
    match glob_shortcuts(root) {
        Ok(files) => HttpReply::json(200, shortcuts_body(&files)),
        // Go: filepath.Glob 错误 → panic → 500 空 body
        Err(_) => HttpReply::empty(500),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sandbox {
        root: std::path::PathBuf,
    }

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("kf-shortcuts-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            Sandbox { root }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// 空目录/缺目录 ⇒ `null`（4 字节）；有文件 ⇒ 相对路径数组，按名排序。
    #[test]
    fn glob_covers_null_and_sorted_relative_paths() {
        let sandbox = Sandbox::new("null");
        // 缺目录
        assert_eq!(glob_shortcuts(&sandbox.root).unwrap().len(), 0);
        let empty = shortcuts_body(&glob_shortcuts(&sandbox.root).unwrap());
        assert_eq!(empty, "null");

        // 空目录
        std::fs::create_dir_all(sandbox.root.join("shortcuts")).unwrap();
        assert_eq!(
            shortcuts_body(&glob_shortcuts(&sandbox.root).unwrap()),
            "null"
        );

        // 两个文件：按名排序；Windows 分隔符相对路径
        std::fs::write(sandbox.root.join("shortcuts").join("b.lnk"), b"").unwrap();
        std::fs::write(sandbox.root.join("shortcuts").join("a.lnk"), b"").unwrap();
        let body = shortcuts_body(&glob_shortcuts(&sandbox.root).unwrap());
        assert_eq!(
            body,
            r#"[{"path":"shortcuts\\a.lnk"},{"path":"shortcuts\\b.lnk"}]"#
        );
    }
}
