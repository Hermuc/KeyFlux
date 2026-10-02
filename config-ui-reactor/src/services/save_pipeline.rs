//! 保存管线 —— 待提交变更队列与统一提交执行器（保存策略的**唯一实现处**）。
//!
//! 策略（2026-10-02 用户定版）：**一切修改只改内存**（config / 行为目录 / 草稿），
//! 需要落盘或生效的副作用——行为库写、插件设置写、插件文件增删、市场安装、
//! 开机自启计划任务、面板 UI 偏好——一律先以 [`PendingChange`] 入队
//! （[`PendingQueue`]），只有页脚「保存配置」（`Message::Save`）才经
//! [`flush_and_save`] 按序提交：
//!
//! ```text
//! 行为库写 → 插件设置写 → 插件文件增删/市场安装 → 自启命令 → PUT /config（重生成脚本
//! + 重启引擎）→ UI 偏好（面板本地文件，最后落）
//! ```
//!
//! 任一步失败即中止，并把**未执行的余项**原序退回队列（调用方 [`PendingQueue::restore`]）；
//! 已执行的项不回滚，重试由用户再次点击「保存配置」承担。
//!
//! 模块边界：只依赖 `services::{api, transport, store, ui_prefs}` 与 `models`，
//! **不感知 Shell/Message** —— 便于单测与移植。

use std::collections::BTreeMap;
use std::path::Path;

use crate::models::{BehaviorPack, Config};
use crate::services::api::{ApiResponse, MessageBody, SettingsApi};
use crate::services::{i18n, store, transport, ui_prefs};

/// 一条待提交变更（staged change）。
#[derive(Debug, Clone)]
pub enum PendingChange {
    /// 新建用户行为包（行为库对话框「保存」/ 匹配类型「保存并创建专属行为」）。
    BehaviorCreate(BehaviorPack),
    /// 编辑用户行为包（键 = 包 id）。
    BehaviorUpdate { id: String, pack: BehaviorPack },
    /// 删除用户行为包（键 = 包 id；对话框删除 / 匹配类型级联删）。
    BehaviorDelete(String),
    /// 整表写回插件设置（键 = 插件 id）。
    PluginSettings {
        id: String,
        values: BTreeMap<String, String>,
    },
    /// 导入插件包（zip 字节已在选择文件时读出校验；键 = `None` ⇒ 按序累积不去重）。
    PluginImport { bytes: Vec<u8>, file_name: String },
    /// 删除用户插件目录（键 = 插件 id）。
    PluginDelete(String),
    /// 市场安装：保存时才下载 zip 并复用导入链路（键 = 插件 id）。
    MarketInstall { id: String, url: String },
    /// 开机自启计划任务（命令 id：3 = 注册 / 4 = 注销，见 `settings::startup_command_id`）。
    StartupCommand(i32),
    /// 面板私有 UI 偏好（`data/ui-prefs.json`，客户端本地写，`PUT /config` 成功后落）。
    UiPrefs(ui_prefs::UiPrefs),
}

impl PendingChange {
    /// 同键去重键：`Some` = 后写覆盖先写（开关来回拨 / 反复保存同一草稿只保留最终意图）；
    /// `None` = 追加不去重（各文件互不相同的导入）。
    fn dedupe_key(&self) -> Option<String> {
        match self {
            PendingChange::BehaviorCreate(pack) => Some(format!("behavior:{}", pack.id)),
            PendingChange::BehaviorUpdate { id, .. } => Some(format!("behavior:{id}")),
            PendingChange::BehaviorDelete(id) => Some(format!("behavior:{id}")),
            PendingChange::PluginSettings { id, .. } => Some(format!("psettings:{id}")),
            PendingChange::PluginImport { .. } => None,
            PendingChange::PluginDelete(id) => Some(format!("pdelete:{id}")),
            PendingChange::MarketInstall { id, .. } => Some(format!("pinstall:{id}")),
            PendingChange::StartupCommand(_) => Some("startup".to_string()),
            PendingChange::UiPrefs(_) => Some("uiprefs".to_string()),
        }
    }
}

/// 待提交队列：同键幂等（后写覆盖先写），按提交顺序执行。
#[derive(Debug, Default)]
pub struct PendingQueue {
    items: Vec<PendingChange>,
}

impl PendingQueue {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 入队。同键去重：先移除旧的同键项再追加。
    ///
    /// 特例：先「新建行为包 X」后「删除 X」（同一会话内建又删）⇒ 两者相抵，
    /// 不向服务端发任何请求（服务端从未见过 X）。
    pub fn push(&mut self, change: PendingChange) {
        if let PendingChange::BehaviorDelete(id) = &change {
            let created_here = self
                .items
                .iter()
                .any(|item| matches!(item, PendingChange::BehaviorCreate(pack) if &pack.id == id));
            let key = change.dedupe_key();
            self.items
                .retain(|item| item.dedupe_key().as_deref() != key.as_deref());
            if !created_here {
                self.items.push(change);
            }
            return;
        }
        // dedupe_key = None 的项（插件导入）不做同键去重，按序累积 ——
        // 去重比较必须以 `Some(key)` 为锚，否则会误删其他 None 键项。
        if let Some(key) = change.dedupe_key() {
            self.items
                .retain(|item| item.dedupe_key().as_deref() != Some(key.as_str()));
        }
        self.items.push(change);
    }

    /// 取空队列（交给后台提交任务）。
    pub fn take(&mut self) -> Vec<PendingChange> {
        std::mem::take(&mut self.items)
    }

    /// 提交失败时把未执行的余项退回（保持原顺序，后续提交重试）。
    pub fn restore(&mut self, remaining: Vec<PendingChange>) {
        self.items.extend(remaining);
    }
}

/// 提交失败：原因 + 未执行的余项（调用方原序退回队列）。
#[derive(Debug, Clone)]
pub struct SaveFailure {
    pub reason: String,
    pub remaining: Vec<PendingChange>,
}

/// 统一提交：按序执行暂存变更，最后 `PUT /config`（重生成脚本 + 重启引擎）。
///
/// 返回 `Ok(提示文案)` 或 `Err((失败原因, 未执行的余项))`——余项交回
/// [`PendingQueue::restore`]，已执行项不回滚。
pub fn flush_and_save(
    port: u16,
    data_root: Option<&Path>,
    queue: Vec<PendingChange>,
    config: &Config,
) -> Result<String, SaveFailure> {
    let api = transport::new_settings_api(port);
    for (index, change) in queue.iter().enumerate() {
        let outcome = match change {
            PendingChange::BehaviorCreate(pack) => require(api.create_behavior(pack)),
            PendingChange::BehaviorUpdate { id, pack } => require(api.update_behavior(id, pack)),
            PendingChange::BehaviorDelete(id) => require(api.delete_behavior(id)),
            PendingChange::PluginSettings { id, values } => {
                require(api.save_plugin_settings(id, values))
            }
            PendingChange::PluginImport { bytes, file_name } => {
                require(api.import_plugin(bytes, file_name))
            }
            PendingChange::PluginDelete(id) => require(api.delete_plugin(id)),
            PendingChange::MarketInstall { id, url } => {
                // 客户端下载 zip → 复用本地导入链路（后端不出网，与旧 MarketInstall 一致）
                market_install(api.as_ref(), id, url)
            }
            PendingChange::StartupCommand(id) => require(api.send_server_command(*id)),
            // UI 偏好在 PUT 成功后客户端本地落盘（见下），不在服务端执行
            PendingChange::UiPrefs(_) => Ok(()),
        };
        if let Err(reason) = outcome {
            return Err(SaveFailure {
                reason,
                remaining: queue[index..].to_vec(),
            });
        }
    }

    match put_config(port, config) {
        Ok(text) => {
            // UI 偏好最后落（面板本地文件）；失败不推翻保存结果，仅附加提示
            let mut extra = String::new();
            if let Some(root) = data_root {
                for change in &queue {
                    if let PendingChange::UiPrefs(prefs) = change
                        && let Err(error) = ui_prefs::save(Path::new(root), *prefs)
                    {
                        extra = format!("\n{}: {error}", i18n::t("2594"));
                    }
                }
            }
            Ok(format!("{text}{extra}"))
        }
        Err(reason) => Err(SaveFailure {
            reason,
            remaining: Vec::new(),
        }),
    }
}

/// 市场安装 = 下载 zip + 导入（提取自旧 `Message::MarketInstall` 后台闭包）。
fn market_install(api: &dyn SettingsApi, id: &str, url: &str) -> Result<(), String> {
    let bytes = crate::services::market::download_zip(url)?;
    require(api.import_plugin(&bytes, &format!("{id}.zip")))
}

/// `ApiResponse` → `Result`（丢弃成功载荷，保留错误文案口径）。
fn require<T>(response: ApiResponse<T>) -> Result<(), String> {
    if response.success {
        Ok(())
    } else {
        Err(response
            .error_message
            .unwrap_or_else(|| format!("HTTP {}", response.status)))
    }
}

/// 清洗后 PUT 配置（`ConfigSaver` 语义），返回保存提示文案。
///
/// 自 `app/glue.rs` 移入：保存管线是 `PUT /config` 的唯一调用方。
pub fn put_config(port: u16, config: &Config) -> Result<String, String> {
    let payload = store::clean_for_save(config);
    let api = transport::new_settings_api(port);
    let response: ApiResponse<MessageBody> = api.save_config(&payload);

    if !response.success {
        return Err(response
            .error_message
            .unwrap_or_else(|| format!("保存失败 (HTTP {})", response.status)));
    }

    // `restartFailed`：保存已落盘但引擎重启失败 ⇒ 引导用户经托盘「重载」手动生效
    // （复刻旧版 1078 标题 + 1079 正文的模态文案，此处合并为提示条文本）
    if response.value.as_ref().and_then(|body| body.restart_failed) == Some(true) {
        return Ok(format!("{}：{}", i18n::t("1078"), i18n::t("1079")));
    }

    Ok(i18n::t("928"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(id: &str) -> BehaviorPack {
        BehaviorPack {
            id: id.to_string(),
            ..Default::default()
        }
    }

    /// 同键后写覆盖先写：开关来回拨 / 反复保存同一草稿只保留最终意图。
    #[test]
    fn same_key_is_replaced_by_latest() {
        let mut queue = PendingQueue::default();
        queue.push(PendingChange::BehaviorCreate(pack("a")));
        queue.push(PendingChange::BehaviorUpdate {
            id: "a".to_string(),
            pack: pack("a"),
        });
        queue.push(PendingChange::BehaviorUpdate {
            id: "a".to_string(),
            pack: pack("a"),
        });
        assert_eq!(queue.take().len(), 1, "同键三项应折叠为一项");
    }

    /// 本会话内「新建 X → 删除 X」相抵：不向服务端发任何请求。
    #[test]
    fn create_then_delete_cancels_out() {
        let mut queue = PendingQueue::default();
        queue.push(PendingChange::BehaviorCreate(pack("new")));
        queue.push(PendingChange::BehaviorDelete("new".to_string()));
        assert!(queue.take().is_empty(), "同会话建又删应相互抵消");
    }

    /// 先「编辑既有包」后「删除」⇒ 只保留删除（服务端确有此包）。
    #[test]
    fn update_then_delete_keeps_delete() {
        let mut queue = PendingQueue::default();
        queue.push(PendingChange::BehaviorUpdate {
            id: "old".to_string(),
            pack: pack("old"),
        });
        queue.push(PendingChange::BehaviorDelete("old".to_string()));
        let items = queue.take();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], PendingChange::BehaviorDelete(_)));
    }

    /// 导入不做同键去重（各文件互不相同），其余键互不串扰。
    #[test]
    fn imports_accumulate_and_keys_do_not_collide() {
        let mut queue = PendingQueue::default();
        queue.push(PendingChange::PluginImport {
            bytes: vec![1],
            file_name: "a.zip".to_string(),
        });
        queue.push(PendingChange::PluginImport {
            bytes: vec![2],
            file_name: "b.zip".to_string(),
        });
        queue.push(PendingChange::PluginDelete("a".to_string()));
        queue.push(PendingChange::StartupCommand(3));
        queue.push(PendingChange::StartupCommand(4));
        let items = queue.take();
        assert_eq!(items.len(), 4, "两条导入保留 + 自启去重为最终值");
    }

    /// 失败余项按原序退回（flush 的错误路径由集成测试覆盖，这里只测队列语义）。
    #[test]
    fn restore_appends_remaining_in_order() {
        let mut queue = PendingQueue::default();
        queue.push(PendingChange::StartupCommand(3));
        queue.restore(vec![
            PendingChange::StartupCommand(4),
            PendingChange::PluginDelete("a".to_string()),
        ]);
        let items = queue.take();
        assert_eq!(items.len(), 3);
        assert!(matches!(items[0], PendingChange::StartupCommand(3)));
    }
}
