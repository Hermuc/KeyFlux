//! 插件市场：目录拉取 + 条目构建 + 插件包下载。
//!
//! 复刻 `config-ui-avalonia/ViewModels/PluginMarketViewModel.cs`：
//! * 目录走**外部网络**（发布侧 `plugins/marketplace.json`），与本地后端无关；
//! * 安装 = 客户端下载 zip → `POST /api/plugins/import`（与本地导入同链路，**后端不出网**）；
//! * 已安装判定 = 后端插件目录 ID 集（含随包 bundled 插件；P7b 起内置判定 =
//!   manifest.bundled 标记动态真源, 不再依赖硬编码名单）。

use std::time::Duration;

use crate::models::{MarketCatalog, MarketPluginEntry};

/// 市场目录地址（发布侧：仓库 `plugins/marketplace.json`）。
pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Hermuc/KeyFlux/main/plugins/marketplace.json";

/// 外部网络超时（对齐旧版 15s）。
const EXTERNAL_TIMEOUT: Duration = Duration::from_secs(15);

/// 市场条目（UI 无关；`is_installed` 由本地已装集合与内置集推导）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketEntry {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    pub version: Option<String>,
    pub description: String,
    pub author: String,
    /// 插件包 zip 地址。
    pub url: String,
    pub is_installed: bool,
}

impl MarketEntry {
    /// 显示名（英文界面优先 `nameEn`，与插件页卡同口径）。
    pub fn display_name(&self, english: bool) -> String {
        if english
            && let Some(name_en) = &self.name_en
            && !name_en.is_empty()
        {
            return name_en.clone();
        }
        self.name.clone()
    }

    /// 版本徽标。
    pub fn version_text(&self) -> String {
        match &self.version {
            Some(version) if !version.is_empty() => format!("v{version}"),
            _ => String::new(),
        }
    }
}

/// 用例：由目录 + 本地已装 ID 集构建条目（**内置条目恒为已安装**，无独立 zip）。
///
/// `installed` = `GET /api/plugins` 返回的用户插件 ID 集。
pub fn build_entries(catalog: &MarketCatalog, installed: &[String]) -> Vec<MarketEntry> {
    catalog
        .plugins
        .iter()
        .map(|entry| entry_to_market(entry, installed))
        .collect()
}

fn entry_to_market(entry: &MarketPluginEntry, installed: &[String]) -> MarketEntry {
    MarketEntry {
        id: entry.id.clone(),
        name: entry.name.clone(),
        name_en: entry.name_en.clone(),
        version: entry.version.clone(),
        description: entry.description.clone().unwrap_or_default(),
        author: entry.author.clone().unwrap_or_default(),
        url: entry.url.clone(),
        is_installed: installed.iter().any(|id| id == &entry.id),
    }
}

/// 可否安装（已安装条目不可安装；随包 bundled 插件经后端目录进 installed 集合，
/// 冒名 zip 由后端 InstallFromZip 的同名/分发标记检查兜底拒绝）。
pub fn can_install(entry: &MarketEntry) -> bool {
    !entry.is_installed
}

fn external_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(EXTERNAL_TIMEOUT))
        .http_status_as_error(false)
        .build();
    config.into()
}

/// 拉取并解析市场目录（外部网络；失败返回可展示的原因）。
pub fn fetch_catalog() -> Result<MarketCatalog, String> {
    let agent = external_agent();
    let mut response = agent
        .get(CATALOG_URL)
        .call()
        .map_err(|error| error.to_string())?;

    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;

    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    serde_json::from_str(&text).map_err(|error| error.to_string())
}

/// 下载插件包 zip 字节（外部网络）。
pub fn download_zip(url: &str) -> Result<Vec<u8>, String> {
    let agent = external_agent();
    let mut response = agent.get(url).call().map_err(|error| error.to_string())?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    response
        .body_mut()
        .read_to_vec()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MarketPluginEntry;

    fn entry(id: &str, name: &str, name_en: Option<&str>) -> MarketPluginEntry {
        MarketPluginEntry {
            id: id.to_string(),
            name: name.to_string(),
            name_en: name_en.map(str::to_string),
            version: Some("1.0".to_string()),
            description: Some("描述".to_string()),
            author: Some("me".to_string()),
            url: format!("https://example.com/{id}.zip"),
        }
    }

    #[test]
    fn entries_mark_installed_and_builtin() {
        let catalog = MarketCatalog {
            name: "市场".to_string(),
            plugins: vec![
                entry("a", "A", None),
                entry("quick_switch", "快速切换", Some("Quick Switch")),
                entry("b", "B", None),
            ],
        };
        // P7b: 内置判定 = manifest.bundled 动态真源 —— 后端目录含随包插件,
        // 其 ID 经 installed 集合流入 (不再有硬编码名单特判)。
        let installed = vec!["quick_switch".to_string(), "b".to_string()];
        let entries = build_entries(&catalog, &installed);

        assert_eq!(entries.len(), 3);
        assert!(!entries[0].is_installed, "未装");
        assert!(can_install(&entries[0]));
        assert!(
            entries[1].is_installed,
            "随包 bundled 条目经后端目录判为已安装"
        );
        assert!(!can_install(&entries[1]));
        assert!(entries[2].is_installed, "已在本地目录中");
        assert!(!can_install(&entries[2]));
    }

    #[test]
    fn display_name_and_version_follow_plugin_card_rules() {
        let catalog = MarketCatalog {
            name: String::new(),
            plugins: vec![entry("a", "中文名", Some("English"))],
        };
        let entries = build_entries(&catalog, &[]);
        let entry = &entries[0];

        assert_eq!(entry.display_name(false), "中文名");
        assert_eq!(entry.display_name(true), "English");
        assert_eq!(entry.version_text(), "v1.0");

        let mut no_version = entry.clone();
        no_version.version = None;
        assert_eq!(no_version.version_text(), "");
    }

    #[test]
    fn empty_catalog_yields_no_entries() {
        let entries = build_entries(&MarketCatalog::default(), &[]);
        assert!(entries.is_empty());
    }
}
