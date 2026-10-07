//! 行为目录的**最薄**加载 —— 只移植 `generators` 侧真正消费的部分。
//!
//! Go `internal/behaviors` 共 965 行，绝大部分是 CRUD / 校验 / apply（生成端用不到）。
//! 本模块只覆盖：目录扫描 → 读 `behavior.json` → `Get(id)` → `ResolveRuleAction` / `behaviorName`。
//!
//! 未移植（对 `plan` / AHK 渲染无影响）：
//! * `ValidateManifest`（只影响"坏包被拒"的路径；内置包均合法）；
//! * CRUD / `Catalog.Covers` / 应用规则等。
//!
//! 目录口径与 Go 一致：
//! * 内置包 = `<settings.exe 所在目录>/behaviors`，用户包 = `<config.json 所在目录>/behaviors`，
//!   插件贡献包 = `<config.json 目录>/plugins/<id>/behaviors`；
//! * 目录**不存在** = 该来源无包（正常，不报错）；
//! * 目录名必须与 manifest 的 `id` 一致（否则该包被拒）；
//! * `sortPacks` = 按 ID 稳定排序；插件贡献包遇同 ID **跳过**（先到者胜：builtin > user > 插件）。

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Go `behaviors.BuiltinActionIDs`：内置基础动作保留 ID 集（逐字搬运）。
pub const BUILTIN_ACTION_IDS: [&str; 10] = [
    "open_url",
    "open_path",
    "open_folder",
    "magnet_download",
    "open",
    "search",
    "run",
    "send_keys",
    "script",
    "copy",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EntryParams {
    #[serde(rename = "actionValue")]
    pub action_value: String,
    #[serde(rename = "workingDir")]
    pub working_dir: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub kind: String,
    pub action: String,
    pub params: EntryParams,
    pub file: String,
    pub func: String,
}

/// 行为包 manifest（`behavior.json`）。`source` 为加载期附加字段（builtin/user），不在文件里。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Pack {
    pub id: String,
    pub name: String,
    #[serde(rename = "nameEn")]
    pub name_en: String,
    pub version: String,
    #[serde(rename = "specVersion")]
    pub spec_version: i32,
    pub description: String,
    pub entry: Entry,
    #[serde(skip)]
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub packs: Vec<Pack>,
}

impl Catalog {
    /// Go `(*Catalog).Get`：线性按 ID 查找（包数量很小，无需索引）。
    pub fn get(&self, id: &str) -> Option<&Pack> {
        self.packs.iter().find(|pack| pack.id == id)
    }
}

fn is_builtin_action(id: &str) -> bool {
    BUILTIN_ACTION_IDS.contains(&id)
}

/// 读单个包目录（Go `readPack`）：缺 `behavior.json` / 解析失败 / 目录名与 id 不一致 ⇒ 拒绝。
fn read_pack(dir: &Path) -> Option<Pack> {
    let raw = std::fs::read(dir.join("behavior.json")).ok()?;
    // Go 侧显式剥 BOM（encoding/json 不容忍 BOM）
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&raw);
    let pack: Pack = serde_json::from_slice(raw).ok()?;
    if pack.id != dir.file_name()?.to_string_lossy() {
        return None;
    }
    Some(pack)
}

/// 扫描一个来源目录（Go `loadDir`）：目录缺失或不可读 ⇒ 空（正常场景）。
fn load_dir(dir: &Path, source: &str) -> Vec<Pack> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut packs = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Some(mut pack) = read_pack(&entry.path()) {
            pack.source = source.to_string();
            packs.push(pack);
        }
    }
    packs
}

fn sort_packs(packs: &mut [Pack]) {
    packs.sort_by(|a, b| a.id.cmp(&b.id)); // Rust `sort_by` 是稳定排序，与 sort.SliceStable 同
}

/// Go `behaviors.LoadCatalog`：内置在前、用户次之、插件贡献包最后（同 ID 跳过）。
pub fn load_catalog(
    builtin_dir: &Path,
    user_dir: &Path,
    plugin_behaviors_dirs: &[PathBuf],
) -> Catalog {
    let mut builtin = load_dir(builtin_dir, "builtin");
    let mut user = load_dir(user_dir, "user");
    sort_packs(&mut builtin);
    sort_packs(&mut user);

    let mut packs = builtin;
    packs.extend(user);

    for (index, dir) in plugin_behaviors_dirs.iter().enumerate() {
        let mut contributed = load_dir(dir, &format!("plugin{}", index + 1));
        sort_packs(&mut contributed);
        for pack in contributed {
            if packs.iter().any(|existing| existing.id == pack.id) {
                continue; // 同 ID 先到者胜
            }
            packs.push(pack);
        }
    }

    Catalog { packs }
}

/// Go `behaviors.ResolveRuleAction`：内置动作 ID **直通**；用户行为包展开为基础动作 +
/// 包默认模板（规则的 actionValue/workingDir 非空时覆盖之）。
pub fn resolve_rule_action(
    catalog: Option<&Catalog>,
    action_type: &str,
    action_value: &str,
    working_dir: &str,
) -> (String, String, String) {
    let passthrough = || {
        (
            action_type.to_string(),
            action_value.to_string(),
            working_dir.to_string(),
        )
    };
    let Some(catalog) = catalog else {
        return passthrough();
    };
    if is_builtin_action(action_type) {
        return passthrough();
    }
    let Some(pack) = catalog.get(action_type) else {
        return passthrough();
    };
    if pack.entry.kind != "builtin" {
        return passthrough();
    }
    let resolved_value = if action_value.is_empty() {
        pack.entry.params.action_value.clone()
    } else {
        action_value.to_string()
    };
    let resolved_dir = if working_dir.is_empty() {
        pack.entry.params.working_dir.clone()
    } else {
        working_dir.to_string()
    };
    (pack.entry.action.clone(), resolved_value, resolved_dir)
}

/// Go `generators.behaviorName`：目录里查不到 ⇒ 回退显示 id。
pub fn behavior_name(catalog: Option<&Catalog>, id: &str) -> String {
    catalog
        .and_then(|catalog| catalog.get(id))
        .map(|pack| pack.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// Go `script.LoadBehaviorCatalog(configPath)`：以**配置文件路径 + 可执行文件目录**推导三处来源
/// （而非 cwd），故 CLI 与运行时两种调用方式结果一致。
///
/// * 内置包：`<exe_dir>/behaviors`（部署树里即 `bin/behaviors`）；
/// * 用户包：`<config 文件所在目录>/behaviors`（即 `<deploy>/data/behaviors`）；
/// * 插件贡献包：`<config 文件所在目录>/plugins/<id>/behaviors`。
pub fn load_catalog_for_config(config_path: &Path, exe_dir: &Path) -> Catalog {
    let config_dir = config_path.parent().unwrap_or(Path::new("."));
    let plugin_root = config_dir.join("plugins");
    let mut plugin_dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&plugin_root) {
        for entry in entries.filter_map(Result::ok) {
            let candidate = entry.path().join("behaviors");
            if candidate.is_dir() {
                plugin_dirs.push(candidate);
            }
        }
    }
    load_catalog(
        &exe_dir.join("behaviors"),
        &config_dir.join("behaviors"),
        &plugin_dirs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_behaviors_dir() -> PathBuf {
        PathBuf::from("../bin/behaviors")
    }

    #[test]
    fn loads_builtin_packs_and_sorts_by_id() {
        let catalog = load_catalog(&repo_behaviors_dir(), Path::new("../data/nonexistent"), &[]);
        assert!(!catalog.packs.is_empty(), "应读到内置行为包");
        // sortPacks 保证按 ID 升序
        let ids: Vec<&str> = catalog.packs.iter().map(|p| p.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);

        let open_url = catalog.get("open_url").expect("应存在 open_url 包");
        assert_eq!(open_url.name, "默认浏览器打开网址");
        assert_eq!(open_url.entry.kind, "builtin");
        assert_eq!(open_url.entry.action, "open_url");
        assert_eq!(open_url.source, "builtin");
    }

    #[test]
    fn resolve_rule_action_passthrough_and_expansion() {
        let catalog = load_catalog(&repo_behaviors_dir(), Path::new("../data/nonexistent"), &[]);

        // 内置 ID ⇒ 直通（即便目录里有同名包）
        assert_eq!(
            resolve_rule_action(Some(&catalog), "open_url", "x", "y"),
            ("open_url".to_string(), "x".to_string(), "y".to_string())
        );
        // catalog 为 None ⇒ 直通
        assert_eq!(
            resolve_rule_action(None, "my_pack", "x", "y"),
            ("my_pack".to_string(), "x".to_string(), "y".to_string())
        );
        // 未知 id ⇒ 直通
        assert_eq!(
            resolve_rule_action(Some(&catalog), "nope", "x", "y"),
            ("nope".to_string(), "x".to_string(), "y".to_string())
        );

        // 用户包（非内置 ID）⇒ 展开为基础动作 + 包默认模板；显式值优先
        let mut user = Catalog::default();
        user.packs.push(Pack {
            id: "my_pack".into(),
            name: "我的包".into(),
            entry: Entry {
                kind: "builtin".into(),
                action: "run".into(),
                params: EntryParams {
                    action_value: "%selected%".into(),
                    working_dir: "C:/wd".into(),
                },
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(
            resolve_rule_action(Some(&user), "my_pack", "", ""),
            (
                "run".to_string(),
                "%selected%".to_string(),
                "C:/wd".to_string()
            )
        );
        assert_eq!(
            resolve_rule_action(Some(&user), "my_pack", "override", "D:/x"),
            (
                "run".to_string(),
                "override".to_string(),
                "D:/x".to_string()
            )
        );
        assert_eq!(behavior_name(Some(&user), "my_pack"), "我的包");
        assert_eq!(behavior_name(Some(&user), "missing"), "missing");
    }
}
