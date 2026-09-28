//! 后端会话：连接 `settings.exe`（Go 后端），并读取端口通告。
//!
//! 严格对齐 `config-ui-avalonia/Services/BackendSession.cs`：
//!
//! | 项 | 契约 |
//! |---|---|
//! | 子进程模式（默认） | 拉起 `settings.exe --headless`，从 **stdout 首行** `KEYFLUX_PORT=<端口>` 读实际端口（12333 被占用时 Go 侧退到随机端口，**必须按通告端口连接**）|
//! | 直连模式 | `--port <N>`，连接已运行的后端（开发调试/冒烟）|
//! | 端口通告超时 | 20s（`BackendSession.cs:42`）|
//! | 健康轮询超时 | 15s（`:45`），间隔 25ms（`:477`），探测 `GET /health` |
//! | 后端定位 | `--settings-exe` / `--backend-dir` 优先；缺省尝试 `<exe目录>\settings.exe` → `<exe目录>\..\settings.exe`（`:340-341`）|
//! | 工作目录 | 缺省 = settings.exe 所在目录（Go 依赖相对 `../data`、`./site`、`./templates`）|
//! | 协议串 | `KEYFLUX_GUI_READY port=<p> elapsed_ms=<n>`（`:MainViewModel.cs:134`）、`KEYFLUX_BACKEND_EXITED code=<n>`（`:444`）|
//!
//! ⚠️ **与旧版的已知差异（Phase 3 待补）**：C# 用 **Job Object**（`BREAKAWAY_OK`）保证
//! GUI 进程即使被强杀也会由 OS 连带回收后端子进程树；Rust 侧当前仅用 `Drop` 终止子进程
//! （正常退出路径等价，**强杀 GUI 会留孤儿**）。补齐需 `windows` crate 的
//! `CreateJobObject` / `AssignProcessToJobObject` / `SetInformationJobObject`。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::api::{HttpSettingsApi, SettingsApi};
use crate::platform::job::JobObject;

/// 端口通告前缀（**契约串，禁改**）。
pub const PORT_PREFIX: &str = "KEYFLUX_PORT=";
/// GUI 就绪标记（**契约串，禁改**）。
pub const GUI_READY_PREFIX: &str = "KEYFLUX_GUI_READY";
/// 后端意外退出标记（**契约串，禁改**）。
pub const BACKEND_EXITED_PREFIX: &str = "KEYFLUX_BACKEND_EXITED";
/// headless 启动开关。
pub const HEADLESS_FLAG: &str = "--headless";

/// settings.exe 文件名（部署契约：与 Go 产物同名，不改）。
pub const SETTINGS_EXE: &str = "settings.exe";

/// 从一行 stdout 解析端口通告（`KEYFLUX_PORT=12333`）。
pub fn parse_port_line(line: &str) -> Option<u16> {
    let rest = line.trim().strip_prefix(PORT_PREFIX)?;
    rest.trim().parse::<u16>().ok()
}

/// 会话启动参数（对齐 C# `BackendSessionOptions`）。
#[derive(Debug, Clone)]
pub struct BackendSessionOptions {
    /// 非空时直连该端口（不拉起子进程）。
    pub direct_port: Option<u16>,
    /// settings.exe 路径（子进程模式）。
    pub settings_exe_path: Option<PathBuf>,
    /// settings.exe 工作目录。
    pub working_directory: Option<PathBuf>,
    /// 等待 `KEYFLUX_PORT=` 通告行的超时。
    pub port_announce_timeout: Duration,
    /// 健康轮询超时。
    pub ready_timeout: Duration,
}

impl Default for BackendSessionOptions {
    fn default() -> Self {
        Self {
            direct_port: None,
            settings_exe_path: None,
            working_directory: None,
            port_announce_timeout: Duration::from_secs(20),
            ready_timeout: Duration::from_secs(15),
        }
    }
}

impl BackendSessionOptions {
    /// 解析启动参数（对齐 C# `Parse`，含"每项消费后一个参数"的语义）：
    /// `--port <N>` / `--settings-exe <路径>` / `--backend-dir <目录>`。
    pub fn parse(args: &[String]) -> Self {
        let mut options = Self::default();
        let mut index = 0;
        // 与 C# 一致：只在还有下一个参数时才判定（避免越界）
        while index + 1 < args.len() {
            match args[index].as_str() {
                "--port" => {
                    if let Ok(port) = args[index + 1].parse::<u16>() {
                        options.direct_port = Some(port);
                        index += 1;
                    }
                }
                "--settings-exe" => {
                    options.settings_exe_path = Some(PathBuf::from(&args[index + 1]));
                    index += 1;
                }
                "--backend-dir" => {
                    options.working_directory = Some(PathBuf::from(&args[index + 1]));
                    index += 1;
                }
                _ => {}
            }
            index += 1;
        }
        options
    }
}

/// 缺省候选路径：`<exe目录>\settings.exe` → `<exe目录>\..\settings.exe`（对齐 `:340-341`）。
pub fn settings_exe_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        exe_dir.join(SETTINGS_EXE),
        exe_dir.join("..").join(SETTINGS_EXE),
    ]
}

/// 解析 settings.exe 路径：显式优先，否则按候选顺序取第一个存在的。
pub fn resolve_settings_exe(options: &BackendSessionOptions, exe_dir: &Path) -> Option<PathBuf> {
    if let Some(explicit) = &options.settings_exe_path {
        return Some(explicit.clone());
    }
    settings_exe_candidates(exe_dir)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// 后端会话失败原因（对齐 C# 的 `FailureReason` 文案语义）。
#[derive(Debug, Clone)]
pub enum BackendError {
    /// 未找到 settings.exe。
    NotFound { searched: Vec<PathBuf> },
    /// 启动子进程失败。
    SpawnFailed(String),
    /// 等待端口通告超时。
    PortAnnounceTimeout,
    /// 健康探测超时。
    NotReady { port: u16 },
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { searched } => {
                let list = searched
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                write!(f, "未找到 settings.exe (查找位置: {list})")
            }
            Self::SpawnFailed(reason) => write!(f, "启动 settings.exe 失败: {reason}"),
            Self::PortAnnounceTimeout => {
                write!(f, "等待 settings.exe 通告端口超时 (未见 KEYFLUX_PORT= 行)")
            }
            Self::NotReady { port } => write!(
                f,
                "settings.exe 已通告端口 {port}, 但健康探测 (GET /health) 超时"
            ),
        }
    }
}

impl std::error::Error for BackendError {}

/// 后端会话：持有子进程归属与 API 句柄。
pub struct BackendSession {
    child: Option<Child>,
    port: u16,
    api: Arc<dyn SettingsApi>,
    diagnostics: Arc<Mutex<Vec<String>>>,
    /// Job Object：GUI 进程死亡（含被强杀）时由 OS 连带回收后端进程树。
    /// 字段在 `Drop::drop` 之后才析构 ⇒ 先 kill 子进程，再关 Job 句柄。
    job: Option<JobObject>,
}

impl BackendSession {
    /// 直连模式：不拉起子进程。
    pub fn connect_direct(
        port: u16,
        options: &BackendSessionOptions,
    ) -> Result<Self, BackendError> {
        let api = Arc::new(HttpSettingsApi::with_timeout(port, Duration::from_secs(10)));
        if !wait_ready(api.as_ref(), options.ready_timeout) {
            return Err(BackendError::NotReady { port });
        }
        Ok(Self {
            child: None,
            port,
            api,
            diagnostics: Arc::new(Mutex::new(Vec::new())),
            job: None,
        })
    }

    /// 子进程模式：拉起 `settings.exe --headless` 并读取端口通告。
    pub fn spawn(
        exe: &Path,
        working_directory: Option<&Path>,
        options: &BackendSessionOptions,
    ) -> Result<Self, BackendError> {
        let mut command = Command::new(exe);
        command
            .arg(HEADLESS_FLAG)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        if let Some(dir) = working_directory {
            command.current_dir(dir);
        } else if let Some(parent) = exe.parent() {
            command.current_dir(parent);
        }

        // Job Object 须在子进程启动后立即加入（与 C# 同策略）。
        // 创建失败时降级为「无 Job」运行：仅损失「强杀面板时回收后端」的保护，不影响功能。
        let job = JobObject::new_kill_on_close().ok();

        let mut child = command
            .spawn()
            .map_err(|error| BackendError::SpawnFailed(error.to_string()))?;

        if let Some(job) = job.as_ref() {
            let _ = job.assign(&child);
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::SpawnFailed("无法获取 stdout 管道".to_string()))?;
        let stderr = child.stderr.take();

        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let (sender, receiver) = mpsc::channel::<String>();

        // stdout 读取线程：首行给出端口通告，其后所有输出进入诊断环形缓冲。
        {
            let diagnostics = Arc::clone(&diagnostics);
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    if sender.send(line.clone()).is_err() {
                        break;
                    }
                    push_diagnostic(&diagnostics, line);
                }
            });
        }
        // stderr 读取线程：仅收集诊断（对齐 C# 的 stderr drain）。
        if let Some(stderr) = stderr {
            let diagnostics = Arc::clone(&diagnostics);
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    push_diagnostic(&diagnostics, line);
                }
            });
        }

        // 等待端口通告行（超时后终止子进程，避免留孤儿）。
        let deadline = Instant::now() + options.port_announce_timeout;
        let mut port = None;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(remaining) {
                Ok(line) => {
                    if let Some(found) = parse_port_line(&line) {
                        port = Some(found);
                        break;
                    }
                    if line.starts_with(BACKEND_EXITED_PREFIX) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        let Some(port) = port else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BackendError::PortAnnounceTimeout);
        };

        // 端口通告后仍有 stdout 输出，继续由读取线程排空（已被上面的循环持有 receiver），
        // 故这里再起一个排空线程以免管道写满阻塞后端。
        {
            let diagnostics = Arc::clone(&diagnostics);
            thread::spawn(move || {
                while let Ok(line) = receiver.recv() {
                    push_diagnostic(&diagnostics, line);
                }
            });
        }

        let api = Arc::new(HttpSettingsApi::with_timeout(port, Duration::from_secs(10)));
        if !wait_ready(api.as_ref(), options.ready_timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BackendError::NotReady { port });
        }

        Ok(Self {
            child: Some(child),
            port,
            api,
            diagnostics,
            job,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn api(&self) -> Arc<dyn SettingsApi> {
        Arc::clone(&self.api)
    }

    /// 诊断输出快照（后端异常退出时给用户看；对齐 C# `_diagnostics` 语义）。
    pub fn diagnostics(&self) -> Vec<String> {
        self.diagnostics
            .lock()
            .map(|buffer| buffer.clone())
            .unwrap_or_default()
    }
}

impl Drop for BackendSession {
    /// 终止本会话拉起的子进程本体（不整树，超时保底，幂等）——对齐 C# `Shutdown()`。
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn push_diagnostic(buffer: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut buffer) = buffer.lock() {
        buffer.push(line);
        // 有界保留尾部，避免长跑内存增长
        const MAX: usize = 200;
        if buffer.len() > MAX {
            let overflow = buffer.len() - MAX;
            buffer.drain(0..overflow);
        }
    }
}

/// 健康轮询：`GET /health`，间隔 25ms（对齐 C# `:476-477`）。
fn wait_ready(api: &dyn SettingsApi, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if api.health_check() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_port_announcement() {
        assert_eq!(parse_port_line("KEYFLUX_PORT=12333"), Some(12333));
        assert_eq!(parse_port_line("KEYFLUX_PORT=54321\r"), Some(54321));
        assert_eq!(parse_port_line("  KEYFLUX_PORT=8080  "), Some(8080));
    }

    #[test]
    fn rejects_non_port_lines() {
        assert_eq!(parse_port_line(""), None);
        assert_eq!(parse_port_line("KEYFLUX_GUI_READY port=1"), None);
        assert_eq!(parse_port_line("KEYFLUX_PORT=abc"), None);
        assert_eq!(parse_port_line("KEYFLUX_PORT="), None);
        // 端口越界（> 65535）不可解析为 u16
        assert_eq!(parse_port_line("KEYFLUX_PORT=70000"), None);
    }

    #[test]
    fn protocol_constants_are_frozen() {
        // 契约串：AI 以后若有人"顺手改名"，此测试会红。
        assert_eq!(PORT_PREFIX, "KEYFLUX_PORT=");
        assert_eq!(GUI_READY_PREFIX, "KEYFLUX_GUI_READY");
        assert_eq!(BACKEND_EXITED_PREFIX, "KEYFLUX_BACKEND_EXITED");
        assert_eq!(HEADLESS_FLAG, "--headless");
        assert_eq!(SETTINGS_EXE, "settings.exe");
    }

    #[test]
    fn parse_args_mirrors_csharp_semantics() {
        let args: Vec<String> = ["--port", "54321"].iter().map(|s| s.to_string()).collect();
        let options = BackendSessionOptions::parse(&args);
        assert_eq!(options.direct_port, Some(54321));

        let args: Vec<String> = [
            "--settings-exe",
            "D:\\bin\\settings.exe",
            "--backend-dir",
            "D:\\bin",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let options = BackendSessionOptions::parse(&args);
        assert_eq!(
            options.settings_exe_path.as_deref(),
            Some(Path::new("D:\\bin\\settings.exe"))
        );
        assert_eq!(
            options.working_directory.as_deref(),
            Some(Path::new("D:\\bin"))
        );

        // 无效端口：不设置 direct_port
        let args: Vec<String> = ["--port", "not-a-number"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(BackendSessionOptions::parse(&args).direct_port, None);

        // 尾随的孤立开关不越界
        let args: Vec<String> = ["--port"].iter().map(|s| s.to_string()).collect();
        assert_eq!(BackendSessionOptions::parse(&args).direct_port, None);
    }

    #[test]
    fn candidate_order_matches_csharp() {
        let candidates = settings_exe_candidates(Path::new("D:\\deploy\\bin\\ui"));
        assert_eq!(
            candidates[0],
            Path::new("D:\\deploy\\bin\\ui\\settings.exe")
        );
        assert_eq!(
            candidates[1],
            Path::new("D:\\deploy\\bin\\ui\\..\\settings.exe")
        );
    }

    #[test]
    fn explicit_path_wins_over_candidates() {
        let options = BackendSessionOptions {
            settings_exe_path: Some(PathBuf::from("X:\\custom\\settings.exe")),
            ..Default::default()
        };
        let resolved = resolve_settings_exe(&options, Path::new("D:\\deploy\\bin\\ui"));
        assert_eq!(
            resolved.as_deref(),
            Some(Path::new("X:\\custom\\settings.exe"))
        );
    }

    #[test]
    fn default_timeouts_match_csharp() {
        let options = BackendSessionOptions::default();
        assert_eq!(options.port_announce_timeout, Duration::from_secs(20));
        assert_eq!(options.ready_timeout, Duration::from_secs(15));
    }

    #[test]
    fn backend_error_messages_are_actionable() {
        let error = BackendError::NotFound {
            searched: vec![PathBuf::from("D:\\bin\\ui\\settings.exe")],
        };
        let text = error.to_string();
        assert!(text.contains("未找到 settings.exe"));
        assert!(text.contains("D:\\bin\\ui\\settings.exe"));

        assert!(
            BackendError::PortAnnounceTimeout
                .to_string()
                .contains("KEYFLUX_PORT=")
        );
        assert!(
            BackendError::NotReady { port: 12333 }
                .to_string()
                .contains("12333")
        );
    }
}
