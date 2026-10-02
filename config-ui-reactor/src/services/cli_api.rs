//! CLI（进程内桥）适配器 —— 与 [`HttpSettingsApi`](super::api::HttpSettingsApi) **同一端口**
//! （`trait SettingsApi`）的另一种传输实现。
//!
//! 传输方式：一次性调用 `settings.exe Call <METHOD> <PATH> <out-file> [--body f] [--content-type ct]`。
//! 服务端由 `config-server/internal/server/bridge.go` 用 gin 引擎在**进程内**执行同一条 handler，
//! **不开 socket、不占端口、无端口协商、无常驻进程** ⇒ 「换传输不换逻辑」，handler 逻辑仍是单一真源。
//!
//! ⚠️ 工作目录语义与 HTTP 模式一致：Go 依赖相对 `../data`、`./site`、`./templates`，故调用时
//! `current_dir` 必须是 `settings.exe` 所在目录（`bin/`）—— 这是本适配器最容易踩的坑。

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;

use super::api::{
    ApiResponse, EmptyJson, MessageBody, SelectedActionTestResponse, SettingsApi, ShortcutInfo,
    finish_response,
};
use super::backend::settings_exe_candidates;
use crate::models::{
    BehaviorCatalogResponse, BehaviorPack, Config, MatchType, PluginListResponse, PluginManifest,
    PluginSettingsRequest, PluginSettingsResponse, SelectedAction,
};

/// stdout 契约行前缀（与 Go `server.CallStatusPrefix` **逐字一致**）。
const CALL_STATUS_PREFIX: &str = "KEYFLUX_CALL status=";

/// 桥执行器：`(method, path, body, content_type) -> (status, 响应体)`。
/// 抽成可注入的函数，便于单测用替身覆盖（无需真的起进程）。
pub type BridgeRunner = Arc<
    dyn Fn(&str, &str, Option<&[u8]>, Option<&str>) -> Result<(u16, Vec<u8>), String> + Send + Sync,
>;

/// CLI 传输的 `SettingsApi` 实现。
pub struct CliSettingsApi {
    exe: PathBuf,
    /// ⚠️ 刻意**不存**工作目录：它已被 `runner` 闭包捕获（见 `with_paths` 的 `run_dir`）。
    /// 曾并存的 `work_dir` 字段是同一事实的冗余第二副本（永远不读，`with_paths` 只写），
    /// 属 drift 隐患 —— 改它不会影响任何行为 —— 故删除。
    runner: BridgeRunner,
}

impl CliSettingsApi {
    /// 面板运行时构造：从自身 exe 目录解析 `settings.exe`（与 HTTP 模式同一候选顺序）。
    pub fn for_panel() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let exe = settings_exe_candidates(&exe_dir)
            .into_iter()
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| exe_dir.join("..").join("settings.exe"));
        let work_dir = exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| exe_dir.clone());
        Self::with_paths(exe, work_dir)
    }

    /// 指定 exe/工作目录（生产与测试共用）。
    pub fn with_paths(exe: PathBuf, work_dir: PathBuf) -> Self {
        let run_exe = exe.clone();
        let run_dir = work_dir.clone();
        let runner: BridgeRunner = Arc::new(move |method, path, body, content_type| {
            run_bridge(&run_exe, &run_dir, method, path, body, content_type)
        });
        Self { exe, runner }
    }

    /// 测试替身：注入自定义桥执行器（不启进程）。
    #[cfg(test)]
    pub fn with_runner(exe: PathBuf, runner: BridgeRunner) -> Self {
        Self { exe, runner }
    }

    /// 统一收口：把一次桥调用折成 `ApiResponse<T>`（语义映射复用 `finish_response`）。
    fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        body: Option<(&[u8], &str)>,
    ) -> ApiResponse<T> {
        match self.invoke(method, path, body) {
            Ok((status, raw)) => finish_response(status, &String::from_utf8_lossy(&raw)),
            Err(reason) => ApiResponse::transport_error(reason),
        }
    }

    /// 原始桥调用（返回 status + 字节体）。
    fn invoke(
        &self,
        method: &str,
        path: &str,
        body: Option<(&[u8], &str)>,
    ) -> Result<(u16, Vec<u8>), String> {
        let (bytes, content_type) = match body {
            Some((bytes, content_type)) => (Some(bytes), Some(content_type)),
            None => (None, None),
        };
        (self.runner)(method, path, bytes, content_type)
    }

    /// 相对路径 → 绝对路径（gin 路由要求前导 `/`）。
    fn abs_path(path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        }
    }
}

impl SettingsApi for CliSettingsApi {
    fn health_check(&self) -> bool {
        matches!(
            self.invoke("GET", "/health", None),
            Ok((status, _)) if (200..300).contains(&status)
        )
    }

    fn get_config(&self) -> ApiResponse<Config> {
        self.call("GET", "/config", None)
    }

    fn save_config(&self, config: &Config) -> ApiResponse<MessageBody> {
        let payload = match serde_json::to_vec(config) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.call("PUT", "/config", Some((&payload, "application/json")))
    }

    fn get_shortcuts(&self) -> ApiResponse<Vec<ShortcutInfo>> {
        self.call("GET", "/shortcuts", None)
    }

    fn send_server_command(&self, id: i32) -> ApiResponse<EmptyJson> {
        self.call(
            "POST",
            &format!("/server/command/{id}"),
            Some((b"{}", "application/json")),
        )
    }

    fn get_behaviors(&self) -> ApiResponse<BehaviorCatalogResponse> {
        self.call("GET", "/api/behaviors", None)
    }

    fn get_plugins(&self) -> ApiResponse<PluginListResponse> {
        self.call("GET", "/api/plugins", None)
    }

    /// multipart 上传：与 HTTP 适配器构造**完全相同的字节体**（同一 boundary 与头布局），
    /// 由桥原样转发给同一个 handler。ureq 侧无内建 multipart，这里同样是手工拼装。
    fn import_plugin(&self, zip: &[u8], file_name: &str) -> ApiResponse<PluginManifest> {
        const BOUNDARY: &str = "----KeyFluxPluginBoundary7f3a1c";

        let head = format!(
            "--{BOUNDARY}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n"
        );
        let mut body: Vec<u8> = Vec::with_capacity(zip.len() + head.len() + 64);
        body.extend_from_slice(head.as_bytes());
        body.extend_from_slice(zip);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        let content_type = format!("multipart/form-data; boundary={BOUNDARY}");

        self.call("POST", "/api/plugins/import", Some((&body, &content_type)))
    }

    fn delete_plugin(&self, id: &str) -> ApiResponse<MessageBody> {
        self.call("DELETE", &format!("/api/plugins/{id}"), None)
    }

    fn get_plugin_settings(&self, id: &str) -> ApiResponse<PluginSettingsResponse> {
        self.call("GET", &format!("/api/plugins/{id}/settings"), None)
    }

    fn save_plugin_settings(
        &self,
        id: &str,
        values: &std::collections::BTreeMap<String, String>,
    ) -> ApiResponse<MessageBody> {
        let payload = match serde_json::to_vec(&PluginSettingsRequest {
            values: values.clone(),
        }) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.call(
            "PUT",
            &format!("/api/plugins/{id}/settings"),
            Some((&payload, "application/json")),
        )
    }

    fn get_raw_text(&self, path: &str) -> ApiResponse<String> {
        match self.invoke("GET", &Self::abs_path(path), None) {
            Ok((status, raw)) => {
                let text = String::from_utf8_lossy(&raw).to_string();
                if (200..300).contains(&status) {
                    ApiResponse {
                        success: true,
                        status,
                        value: Some(text.clone()),
                        error_message: None,
                        raw_body: text,
                    }
                } else {
                    let message = super::api::HttpSettingsApi::extract_error(&text);
                    ApiResponse::http_error(status, message, text)
                }
            }
            Err(reason) => ApiResponse::transport_error(reason),
        }
    }

    fn base_url(&self) -> String {
        format!("cli:{}", self.exe.display())
    }

    fn test_selected_action(
        &self,
        content: &str,
        is_file: bool,
        selected_action: Option<&SelectedAction>,
        match_types: Option<&[MatchType]>,
    ) -> ApiResponse<SelectedActionTestResponse> {
        // 请求体对齐 Go `selectedActionTestRequest`（与 HTTP 适配器同一 json 形状）。
        let body = serde_json::json!({
            "content": content,
            "isFile": is_file,
            "selectedAction": selected_action,
            "matchTypes": match_types,
        });
        let payload = match serde_json::to_vec(&body) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.call(
            "POST",
            "/api/selected-action/test",
            Some((&payload, "application/json")),
        )
    }

    fn play_selected_action(&self, type_id: &str) -> ApiResponse<EmptyJson> {
        let payload = serde_json::to_vec(&serde_json::json!({ "typeId": type_id }))
            .unwrap_or_else(|_| b"{}".to_vec());
        self.call(
            "POST",
            "/api/selected-action/play",
            Some((&payload, "application/json")),
        )
    }

    fn create_behavior(&self, pack: &BehaviorPack) -> ApiResponse<BehaviorPack> {
        let payload = match serde_json::to_vec(pack) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.call(
            "POST",
            "/api/behaviors",
            Some((&payload, "application/json")),
        )
    }

    fn update_behavior(&self, id: &str, pack: &BehaviorPack) -> ApiResponse<BehaviorPack> {
        let payload = match serde_json::to_vec(pack) {
            Ok(payload) => payload,
            Err(error) => return ApiResponse::transport_error(error.to_string()),
        };
        self.call(
            "PUT",
            &format!("/api/behaviors/{id}"),
            Some((&payload, "application/json")),
        )
    }

    fn delete_behavior(&self, id: &str) -> ApiResponse<MessageBody> {
        self.call("DELETE", &format!("/api/behaviors/{id}"), None)
    }
}

/// 生产桥执行器：写 body 到临时文件 → 启 `settings.exe Call ...` → 读 out 文件与 stdout 契约行。
fn run_bridge(
    exe: &Path,
    work_dir: &Path,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
) -> Result<(u16, Vec<u8>), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let abs = CliSettingsApi::abs_path(path);
    let dir = std::env::temp_dir().join(format!(
        "keyflux-bridge-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建临时目录失败: {error}"))?;
    let out_file = dir.join("out.bin");

    let result = (|| -> Result<(u16, Vec<u8>), String> {
        let mut command = Command::new(exe);
        command
            .arg("Call")
            .arg(method)
            .arg(&abs)
            .arg(&out_file)
            .current_dir(work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // settings.exe 是控制台程序：缺此标志会闪一个黑窗（与 backend.rs 同因）。
            .creation_flags(0x0800_0000); // CREATE_NO_WINDOW

        if let Some(bytes) = body {
            let body_file = dir.join("body.bin");
            std::fs::write(&body_file, bytes).map_err(|error| format!("写请求体失败: {error}"))?;
            command.arg("--body").arg(&body_file);
            if let Some(content_type) = content_type {
                command.arg("--content-type").arg(content_type);
            }
        }

        let output = command
            .output()
            .map_err(|error| format!("运行 {} 失败: {error}", exe.display()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let status = stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix(CALL_STATUS_PREFIX))
            .and_then(|value| value.trim().parse::<u16>().ok())
            .ok_or_else(|| {
                format!(
                    "未找到 {CALL_STATUS_PREFIX} 契约行 (exit={:?})",
                    output.status.code()
                )
            })?;

        let data = std::fs::read(&out_file).map_err(|error| format!("读响应体失败: {error}"))?;
        Ok((status, data))
    })();

    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 记录调用并回放预置响应的替身。
    fn stub(seen: Arc<Mutex<Vec<String>>>, reply: Result<(u16, Vec<u8>), String>) -> BridgeRunner {
        Arc::new(move |method, path, _body, _ct| {
            seen.lock().unwrap().push(format!("{method} {path}"));
            reply.clone()
        })
    }

    fn api_with(runner: BridgeRunner) -> CliSettingsApi {
        // 只需 exe（用于 `describe()` 的标识串）；工作目录由注入的 runner 自理。
        CliSettingsApi::with_runner(PathBuf::from("X:/bin/settings.exe"), runner)
    }

    #[test]
    fn call_normalizes_path_with_leading_slash() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let api = api_with(stub(Arc::clone(&seen), Ok((200, b"[]".to_vec()))));

        let _ = api.get_shortcuts();

        assert_eq!(seen.lock().unwrap().as_slice(), ["GET /shortcuts"]);
    }

    #[test]
    fn non_2xx_folds_message_from_body() {
        let api = api_with(stub(
            Arc::new(Mutex::new(Vec::new())),
            Ok((400, br#"{"message":"boom"}"#.to_vec())),
        ));

        let response = api.get_shortcuts();

        assert!(!response.success);
        assert_eq!(response.status, 400);
        assert_eq!(response.error_message.as_deref(), Some("boom"));
    }

    #[test]
    fn transport_failure_folds_to_status_zero() {
        let api = api_with(stub(
            Arc::new(Mutex::new(Vec::new())),
            Err("spawn failed".to_string()),
        ));

        let response = api.get_config();

        assert!(!response.success);
        assert_eq!(response.status, 0, "桥调用失败必须折叠为 status=0");
        assert_eq!(response.error_message.as_deref(), Some("spawn failed"));
    }

    #[test]
    fn health_check_follows_status_only() {
        let ok = api_with(stub(
            Arc::new(Mutex::new(Vec::new())),
            Ok((200, b"ok".to_vec())),
        ));
        assert!(
            ok.health_check(),
            "/health 返回 200 即视为在线（体不是 JSON）"
        );

        let down = api_with(stub(
            Arc::new(Mutex::new(Vec::new())),
            Ok((503, Vec::new())),
        ));
        assert!(!down.health_check());
    }

    #[test]
    fn save_config_posts_json_body_with_content_type() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = Arc::clone(&seen);
        let captured_in = Arc::clone(&captured);
        let runner: BridgeRunner = Arc::new(move |method, path, body, ct| {
            seen_in.lock().unwrap().push(format!("{method} {path}"));
            captured_in.lock().unwrap().push((
                body.map(|b| String::from_utf8_lossy(b).to_string())
                    .unwrap_or_default(),
                ct.unwrap_or_default().to_string(),
            ));
            Ok((200, br#"{"message":"ok"}"#.to_vec()))
        });
        let api = api_with(runner);

        let config = Config::default();
        let response = api.save_config(&config);

        assert!(response.success);
        assert_eq!(seen.lock().unwrap().as_slice(), ["PUT /config"]);
        let (body, ct) = captured.lock().unwrap().first().cloned().unwrap();
        assert_eq!(ct, "application/json");
        assert!(
            body.starts_with('{'),
            "PUT /config 必须带 JSON 体, 实得: {body}"
        );
    }
}
