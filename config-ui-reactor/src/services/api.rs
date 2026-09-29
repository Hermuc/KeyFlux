//! settings.exe（Go）HTTP 客户端 —— 严格对齐 `config-ui-avalonia/Services/SettingsApiClient.cs`。
//!
//! **契约铁律**：
//! * 只作 `http://127.0.0.1:{KEYFLUX_PORT}` 的客户端；**永不因网络异常 panic**，
//!   传输层错误（连接失败/超时）统一折叠为 `status = 0` 的失败响应（对齐 C# 语义）。
//! * 端点清单（权威：`config-server/cmd/settings/main.go` + `selectedaction.go`）：
//!   | 方法 | 路径 | 说明 |
//!   |---|---|---|
//!   | GET | `/health` | 后端零 IO 立即 200（连接基础设施专用，不在 12 端点契约内）|
//!   | GET | `/config` | 完整 Config JSON |
//!   | PUT | `/config` | 完整 Config → `200 {"message":"ok"}`；校验失败 `400 {"message":"保存失败: …"}` |
//!   | GET | `/shortcuts` | `[{"path":"shortcuts\\xx.lnk"}]` |
//!   | POST | `/server/command/:id` | id=2/3/4，恒 `200 {}` |
//!   | POST | `/api/selected-action/test` | 模拟测试 |
//!   | POST | `/api/selected-action/play` | 真实执行（写请求文件，AHK 轮询消费）|
//!   | GET/POST/PUT/DELETE | `/api/behaviors[/:id]`、`/api/behaviors/apply` | 行为包 |
//!   | GET/DELETE | `/api/plugins[/:id]`、POST `/api/plugins/import` | 插件管理 |
//!   | GET/PUT | `/api/plugins/:id/settings` | 插件声明式设置 |

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::{
    BehaviorCatalogResponse, Config, PluginListResponse, PluginManifest, PluginSettingsRequest,
    PluginSettingsResponse,
};

/// 统一响应包装（对齐 C# `ApiResponse<T>`）。
#[derive(Debug, Clone)]
pub struct ApiResponse<T> {
    /// HTTP 2xx 为 true。
    pub success: bool,
    /// HTTP 状态码；传输层异常（连接失败/超时）为 0。
    pub status: u16,
    /// 2xx 且响应体可反序列化时的强类型结果。
    pub value: Option<T>,
    /// 非 2xx 时来自响应体 `message` 字段（解析失败则为原始体）；传输层异常时为异常文本。
    pub error_message: Option<String>,
    /// 原始响应体文本（便于诊断）。
    pub raw_body: String,
}

impl<T> ApiResponse<T> {
    /// 传输层失败（status = 0）。对应 C# 的 `catch` 分支。
    pub fn transport_error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            status: 0,
            value: None,
            error_message: Some(message.into()),
            raw_body: String::new(),
        }
    }

    /// HTTP 层失败（非 2xx）。
    pub fn http_error(status: u16, message: impl Into<String>, raw: impl Into<String>) -> Self {
        Self {
            success: false,
            status,
            value: None,
            error_message: Some(message.into()),
            raw_body: raw.into(),
        }
    }

    /// 成功。
    pub fn ok(status: u16, value: T, raw: impl Into<String>) -> Self {
        Self {
            success: true,
            status,
            value: Some(value),
            error_message: None,
            raw_body: raw.into(),
        }
    }
}

/// 形如 `{"message":"…"}` 的响应体（PUT /config 校验失败等）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageBody {
    #[serde(default)]
    pub message: String,

    /// 保存后 KeyFlux 进程重启失败时为 `true`（保存已落盘，需经托盘「重载」手动生效）。
    /// 旧后端无此字段 ⇒ 反序列化为 `None`，视同成功（向后兼容）。
    #[serde(default, rename = "restartFailed")]
    pub restart_failed: Option<bool>,
}

/// `GET /shortcuts` 的列表项。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShortcutInfo {
    /// 相对部署根的路径，如 `shortcuts\微信.lnk`。
    #[serde(default)]
    pub path: String,
}

/// 形如 `{}` 的空对象响应体（`POST /server/command/:id`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmptyJson {}

/// `POST /api/selected-action/test` 的菜单项（`gin.H{"key","behavior","name"}`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectedActionMenuEntry {
    #[serde(default)]
    pub key: i32,
    #[serde(default)]
    pub behavior: String,
    #[serde(default)]
    pub name: String,
}

/// `POST /api/selected-action/test` 响应（未命中 = `{"matched":false}`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectedActionTestResponse {
    #[serde(default)]
    pub matched: bool,
    #[serde(default)]
    pub match_type: Option<String>,
    #[serde(default)]
    pub match_value: Option<String>,
    #[serde(default)]
    pub menu: Vec<SelectedActionMenuEntry>,
    #[serde(default)]
    pub preview: Option<String>,
}

/// settings.exe API 抽象 —— ViewModel/组件只依赖本 trait，便于注入假实现。
pub trait SettingsApi: Send + Sync {
    /// 健康探测：语义 = 「HTTP 活着」，读配置能力由主加载兜底。
    fn health_check(&self) -> bool;
    fn get_config(&self) -> ApiResponse<Config>;
    fn save_config(&self, config: &Config) -> ApiResponse<MessageBody>;
    fn get_shortcuts(&self) -> ApiResponse<Vec<ShortcutInfo>>;
    fn send_server_command(&self, id: i32) -> ApiResponse<EmptyJson>;
    /// `GET /api/behaviors`：行为目录快照（内置 + 用户两组）。
    fn get_behaviors(&self) -> ApiResponse<BehaviorCatalogResponse>;
    /// `GET api/plugins`：用户插件目录 + 逐包加载告警。
    fn get_plugins(&self) -> ApiResponse<PluginListResponse>;
    /// `POST api/plugins/import`：multipart 上传插件包 zip，返回其 manifest。
    fn import_plugin(&self, zip: &[u8], file_name: &str) -> ApiResponse<PluginManifest>;
    /// `DELETE api/plugins/{id}`：删除用户插件目录。
    fn delete_plugin(&self, id: &str) -> ApiResponse<MessageBody>;
    /// `GET api/plugins/{id}/settings`：声明 + 默认值合并后的完整值表。
    fn get_plugin_settings(&self, id: &str) -> ApiResponse<PluginSettingsResponse>;
    /// `PUT api/plugins/{id}/settings`：整表写回（后端按声明校验并落盘）。
    fn save_plugin_settings(
        &self,
        id: &str,
        values: &BTreeMap<String, String>,
    ) -> ApiResponse<MessageBody>;
    /// 非契约端点的原始文本 GET（如首页 `/config_doc.html` 静态资源）。
    fn get_raw_text(&self, path: &str) -> ApiResponse<String>;
    /// 当前 BaseAddress（诊断用）。
    fn base_url(&self) -> String;
    /// `POST /api/selected-action/test`：模拟测试（携带编辑中快照，未保存也能测）。
    fn test_selected_action(
        &self,
        content: &str,
        is_file: bool,
        selected_action: Option<&crate::models::SelectedAction>,
        match_types: Option<&[crate::models::MatchType]>,
    ) -> ApiResponse<SelectedActionTestResponse>;
    /// `POST /api/selected-action/play`：▶ 真实执行（typeId 白名单校验后写请求文件）。
    fn play_selected_action(&self, type_id: &str) -> ApiResponse<EmptyJson>;
    /// `POST /api/behaviors`：新建用户行为包（返回回写后的 manifest）。
    fn create_behavior(
        &self,
        pack: &crate::models::BehaviorPack,
    ) -> ApiResponse<crate::models::BehaviorPack>;
    /// `PUT /api/behaviors/{id}`：编辑用户行为包（内置包后端 404 拒绝）。
    fn update_behavior(
        &self,
        id: &str,
        pack: &crate::models::BehaviorPack,
    ) -> ApiResponse<crate::models::BehaviorPack>;
    /// `DELETE /api/behaviors/{id}`：删除用户行为包。
    fn delete_behavior(&self, id: &str) -> ApiResponse<MessageBody>;
    /// `POST /api/behaviors/apply`：让行为变更立即生效（重启引擎；`restartFailed` 见 MessageBody）。
    fn apply_behaviors(&self) -> ApiResponse<MessageBody>;
}

/// 基于 `ureq` 的阻塞式实现（在 `spawn_background` 中调用）。
pub struct HttpSettingsApi {
    base: String,
    agent: ureq::Agent,
}

impl HttpSettingsApi {
    /// 对齐 C# 默认超时（10s）。
    pub fn new(port: u16) -> Self {
        Self::with_timeout(port, Duration::from_secs(10))
    }

    pub fn with_timeout(port: u16, timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            // 非 2xx 也要拿到状态码与响应体（用于解析 {"message":"…"}）
            .http_status_as_error(false)
            .build();
        Self {
            base: format!("http://127.0.0.1:{port}"),
            agent: config.into(),
        }
    }

    /// 注入外部 Agent（测试替身用）。
    pub fn with_agent(base: impl Into<String>, agent: ureq::Agent) -> Self {
        Self {
            base: base.into(),
            agent,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    /// 非 2xx 时优先取响应体的 `message` 字段，取不到则回退原始体（对齐 C# `ExtractErrorMessage`）。
    ///
    /// 公开的理由：**HTTP 与 CLI（进程内桥）两个适配器共用**同一份错误提取语义。
    pub fn extract_error(raw: &str) -> String {
        if let Ok(body) = serde_json::from_str::<MessageBody>(raw)
            && !body.message.is_empty()
        {
            return body.message;
        }
        raw.to_string()
    }

    /// 无请求体的 GET。
    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> ApiResponse<T> {
        let url = self.url(path);
        self.finish::<T>(self.agent.get(&url).call())
    }

    /// 无请求体的 DELETE。
    fn delete<T: for<'de> Deserialize<'de>>(&self, path: &str) -> ApiResponse<T> {
        let url = self.url(path);
        self.finish::<T>(self.agent.delete(&url).call())
    }

    /// 带 JSON 请求体的 PUT。
    fn put_json<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResponse<T> {
        let url = self.url(path);
        let payload = match serde_json::to_string(body) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.finish::<T>(
            self.agent
                .put(&url)
                .header("Content-Type", "application/json")
                .send(payload),
        )
    }

    /// 带 JSON 请求体的 POST。
    fn post_json<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResponse<T> {
        let url = self.url(path);
        let payload = match serde_json::to_string(body) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.finish::<T>(
            self.agent
                .post(&url)
                .header("Content-Type", "application/json")
                .send(payload),
        )
    }

    /// 空 JSON 体（`{}`）的 POST。
    fn post_empty<T: for<'de> Deserialize<'de>>(&self, path: &str) -> ApiResponse<T> {
        self.post_json(path, &EmptyJson {})
    }

    /// 统一收口：状态码 + 响应体 → `ApiResponse<T>`；传输层失败折叠为 `status = 0`。
    fn finish<T: for<'de> Deserialize<'de>>(
        &self,
        result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> ApiResponse<T> {
        let mut response = match result {
            Ok(response) => response,
            // 传输层失败（连接被拒/超时）：折叠为 status = 0
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };

        let status = response.status().as_u16();
        let raw = match response.body_mut().read_to_string() {
            Ok(text) => text,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };

        finish_response(status, &raw)
    }
}

/// 状态码 + 原始响应体 → `ApiResponse<T>`。
///
/// **HTTP 与 CLI（进程内桥）两个适配器共用**：传输层只负责拿到 (status, 原始体)，
/// 语义映射只此一份，避免「第二真源」。
pub fn finish_response<T: for<'de> Deserialize<'de>>(status: u16, raw: &str) -> ApiResponse<T> {
    if (200..300).contains(&status) {
        if raw.trim().is_empty() {
            // 空体：尝试以默认值构造（如 EmptyJson）
            match serde_json::from_str::<T>("{}") {
                Ok(value) => ApiResponse::ok(status, value, raw),
                Err(_) => ApiResponse {
                    success: true,
                    status,
                    value: None,
                    error_message: None,
                    raw_body: raw.to_string(),
                },
            }
        } else {
            match serde_json::from_str::<T>(raw) {
                Ok(value) => ApiResponse::ok(status, value, raw),
                Err(error) => ApiResponse::http_error(status, error.to_string(), raw),
            }
        }
    } else {
        ApiResponse::http_error(status, HttpSettingsApi::extract_error(raw), raw)
    }
}

impl SettingsApi for HttpSettingsApi {
    fn health_check(&self) -> bool {
        self.agent
            .get(self.url("health"))
            .call()
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    fn get_config(&self) -> ApiResponse<Config> {
        self.get("config")
    }

    fn save_config(&self, config: &Config) -> ApiResponse<MessageBody> {
        self.put_json("config", config)
    }

    fn get_shortcuts(&self) -> ApiResponse<Vec<ShortcutInfo>> {
        self.get("shortcuts")
    }

    fn send_server_command(&self, id: i32) -> ApiResponse<EmptyJson> {
        self.post_empty(&format!("server/command/{id}"))
    }

    fn get_behaviors(&self) -> ApiResponse<BehaviorCatalogResponse> {
        self.get("api/behaviors")
    }

    fn get_plugins(&self) -> ApiResponse<PluginListResponse> {
        self.get("api/plugins")
    }

    /// multipart 上传（`file` 字段携带 zip 字节）。手工拼装 body：
    /// ureq 无内建 multipart 构造器，而该端点只接受 `file` 一个字段 ⇒ 直接构造最稳。
    fn import_plugin(&self, zip: &[u8], file_name: &str) -> ApiResponse<PluginManifest> {
        const BOUNDARY: &str = "----KeyFluxPluginBoundary7f3a1c";

        let url = self.url("api/plugins/import");
        let head = format!(
            "--{BOUNDARY}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n"
        );
        let mut body: Vec<u8> = Vec::with_capacity(zip.len() + head.len() + 64);
        body.extend_from_slice(head.as_bytes());
        body.extend_from_slice(zip);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());

        self.finish::<PluginManifest>(
            self.agent
                .post(&url)
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .send(body),
        )
    }

    fn delete_plugin(&self, id: &str) -> ApiResponse<MessageBody> {
        self.delete(&format!("api/plugins/{id}"))
    }

    fn get_plugin_settings(&self, id: &str) -> ApiResponse<PluginSettingsResponse> {
        self.get(&format!("api/plugins/{id}/settings"))
    }

    fn save_plugin_settings(
        &self,
        id: &str,
        values: &BTreeMap<String, String>,
    ) -> ApiResponse<MessageBody> {
        self.put_json(
            &format!("api/plugins/{id}/settings"),
            &PluginSettingsRequest {
                values: values.clone(),
            },
        )
    }

    fn get_raw_text(&self, path: &str) -> ApiResponse<String> {
        let url = self.url(path);
        let mut response = match self.agent.get(&url).call() {
            Ok(response) => response,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        let status = response.status().as_u16();
        let raw = match response.body_mut().read_to_string() {
            Ok(text) => text,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        if (200..300).contains(&status) {
            ApiResponse {
                success: true,
                status,
                value: Some(raw.clone()),
                error_message: None,
                raw_body: raw,
            }
        } else {
            let message = Self::extract_error(&raw);
            ApiResponse::http_error(status, message, raw)
        }
    }

    fn base_url(&self) -> String {
        self.base.clone()
    }

    fn test_selected_action(
        &self,
        content: &str,
        is_file: bool,
        selected_action: Option<&crate::models::SelectedAction>,
        match_types: Option<&[crate::models::MatchType]>,
    ) -> ApiResponse<SelectedActionTestResponse> {
        // 请求体对齐 Go `selectedActionTestRequest`（编辑中快照优先，缺省回退磁盘配置）
        let body = serde_json::json!({
            "content": content,
            "isFile": is_file,
            "selectedAction": selected_action,
            "matchTypes": match_types,
        });
        self.post_json("api/selected-action/test", &body)
    }

    fn play_selected_action(&self, type_id: &str) -> ApiResponse<EmptyJson> {
        self.post_json(
            "api/selected-action/play",
            &serde_json::json!({ "typeId": type_id }),
        )
    }

    fn create_behavior(
        &self,
        pack: &crate::models::BehaviorPack,
    ) -> ApiResponse<crate::models::BehaviorPack> {
        self.post_json("api/behaviors", pack)
    }

    fn update_behavior(
        &self,
        id: &str,
        pack: &crate::models::BehaviorPack,
    ) -> ApiResponse<crate::models::BehaviorPack> {
        self.put_json(&format!("api/behaviors/{id}"), pack)
    }

    fn delete_behavior(&self, id: &str) -> ApiResponse<MessageBody> {
        self.delete(&format!("api/behaviors/{id}"))
    }

    fn apply_behaviors(&self) -> ApiResponse<MessageBody> {
        self.post_empty("api/behaviors/apply")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_error_has_zero_status() {
        let r: ApiResponse<Config> = ApiResponse::transport_error("connection refused");
        assert!(!r.success);
        assert_eq!(r.status, 0, "传输层失败必须折叠为 status=0");
        assert!(r.value.is_none());
        assert_eq!(r.error_message.as_deref(), Some("connection refused"));
    }

    #[test]
    fn message_body_restart_failed_is_optional() {
        // 旧后端无该字段 ⇒ None，视同成功
        let old: MessageBody = serde_json::from_str(r#"{"message":"ok"}"#).unwrap();
        assert_eq!(old.message, "ok");
        assert_eq!(old.restart_failed, None);

        let new: MessageBody =
            serde_json::from_str(r#"{"message":"ok","restartFailed":true}"#).unwrap();
        assert_eq!(new.restart_failed, Some(true));
    }

    #[test]
    fn shortcut_info_reads_path() {
        let list: Vec<ShortcutInfo> =
            serde_json::from_str(r#"[{"path":"shortcuts\\微信.lnk"}]"#).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, "shortcuts\\微信.lnk");
    }

    #[test]
    fn extract_error_prefers_message_field() {
        assert_eq!(
            HttpSettingsApi::extract_error(r#"{"message":"保存失败: 校验不通过"}"#),
            "保存失败: 校验不通过"
        );
        // 非 JSON：回退原始体
        assert_eq!(HttpSettingsApi::extract_error("boom"), "boom");
        // JSON 但无 message：回退原始体
        assert_eq!(
            HttpSettingsApi::extract_error(r#"{"other":1}"#),
            r#"{"other":1}"#
        );
    }

    #[test]
    fn plugin_settings_request_serializes_values() {
        // ⚠️ 有序映射：同一份值表多次序列化逐字节可复现（诊断/测试都更稳）
        let values = BTreeMap::from([(
            "everything_search:es_path".to_string(),
            "C:\\es.exe".to_string(),
        )]);
        let body = serde_json::to_value(PluginSettingsRequest { values }).unwrap();
        assert_eq!(body["values"]["everything_search:es_path"], "C:\\es.exe");
    }
}
