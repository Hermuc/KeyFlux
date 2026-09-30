//! 面板**私有** UI 偏好（与共享 `config.json` 解耦）。
//!
//! 为什么不进 `options`：`options` 是 Go/Rust **逐字节契约**（字段序 + 无 omitempty，
//! 受 `tools/parity` 与 `tools/api-parity` 基线守护）。而「窗口背景材质」纯属设置面板
//! 自身的观感偏好，Go 侧既不读也不写 —— 塞进契约会为一个 UI 开关牵动双向对账基线。
//!
//! 落点：`<deploy>/data/ui-prefs.json`（随便携部署树移动；**不是** 配置第二真源，
//! 不承载任何引擎/生成器语义）。
//!
//! 容错口径：文件缺失 / 非 UTF-8 / JSON 损坏 / 未知字段 ⇒ 回落默认值（不 panic、不报错）。
//! 写入口径：先写同目录临时文件再 rename（与 `plugins/store.go` 的原子写同思路）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 面板 UI 偏好（当前仅一项：窗口背景材质）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    /// `true` = 亚克力毛玻璃（Acrylic）；`false` = 系统 Mica（默认，与改动前一致）。
    pub acrylic: bool,
}

/// 偏好文件路径：`<deploy>/data/ui-prefs.json`。
pub fn prefs_path(deploy_root: &Path) -> PathBuf {
    deploy_root.join("data").join("ui-prefs.json")
}

/// 读取偏好；任何异常一律回落默认值。
pub fn load(deploy_root: &Path) -> UiPrefs {
    std::fs::read_to_string(prefs_path(deploy_root))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// 原子写入偏好（temp + rename）。父目录不存在时创建。
pub fn save(deploy_root: &Path, prefs: UiPrefs) -> std::io::Result<()> {
    let path = prefs_path(deploy_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&prefs).unwrap_or_else(|_| "{}".to_string())
    );
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一次性临时部署树（测试结束即删）。
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("kf-uiprefs-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("data")).unwrap();
            Self(root)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trip_persists_acrylic() {
        let root = TempRoot::new("roundtrip");
        assert_eq!(load(&root.0), UiPrefs::default(), "缺失文件 ⇒ 默认 false");

        save(&root.0, UiPrefs { acrylic: true }).unwrap();
        assert!(load(&root.0).acrylic);
        assert!(prefs_path(&root.0).is_file());
        assert!(
            !root.0.join("data").join("ui-prefs.json.tmp").exists(),
            "临时文件必须已 rename 掉"
        );

        save(&root.0, UiPrefs { acrylic: false }).unwrap();
        assert!(!load(&root.0).acrylic);
    }

    #[test]
    fn corrupted_or_partial_file_falls_back_to_default() {
        let root = TempRoot::new("corrupt");
        let path = prefs_path(&root.0);

        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(load(&root.0), UiPrefs::default());

        // 未知字段可容忍（向前兼容：未来新增键不回退旧版本读取）。
        std::fs::write(&path, br#"{"acrylic": true, "futureKey": 42}"#).unwrap();
        assert!(load(&root.0).acrylic);

        // 空对象 ⇒ 全默认。
        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(load(&root.0), UiPrefs::default());
    }

    #[test]
    fn save_creates_missing_data_directory() {
        let root = TempRoot::new("mkdir");
        let bare = root.0.join("fresh");
        save(&bare, UiPrefs { acrylic: true }).unwrap();
        assert!(load(&bare).acrylic, "父目录不存在时应自动创建");
    }
}
