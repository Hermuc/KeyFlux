//! HTTP 服务层的**黑盒集成测试**（多维度优化报告 #4）。
//!
//! ## 为什么需要这一层
//!
//! `src/` 里的 349 个 `#[test]` 几乎全是**单元测试**：它们直接调 `dispatch`（纯函数）
//! 或各 handler 的可测内核，socket 层、进程启动、端口通告、真实 HTTP 往返**从未被覆盖**。
//! `tests/` 在此之前只有 `skin_contract.rs` 一个（跨 crate 文本对账），服务层零黑盒。
//!
//! 本文件补上那层：**真实拉起 `settings.exe --headless` 子进程 → 读 stdout 首行端口通告
//! → 经真实 `ureq` HTTP 请求打契约端点 → 断言响应**。它守护的是单元测试结构上够不到的
//! 东西：`--headless` 参数分派、`bind` + 端口通告协议串、`tiny_http` 服务循环、
//! `Content-Type` 回写、以及 `KEYFLUX_PORT=` 与面板侧共用同一个解析器。
//!
//! ## 为什么是「拉起二进制」而不是「同进程调 run_headless」
//!
//! `run_headless()` 是 `pub`，但它是**永不返回**的服务循环；同进程调用要么阻塞测试，
//! 要么另起线程 + 关停通道（而 `tiny_http` 的关停要拿住 `Server`，`run_headless` 不交出来）。
//! 拉子进程既是真的黑盒，又天然隔离（崩溃/挂死不会拖垮测试进程）。
//! 二进制路径由 Cargo 注入：集成测试引用 `CARGO_BIN_EXE_settings` 即会构建对应 bin。
//!
//! ## 夹具口径
//!
//! * 沙箱 = `%TEMP%/kf-server-api-<pid>-<nanos>/`，内含 `bin/`（子进程 cwd）与
//!   `data/config.json`（拷贝仓库出厂配置）—— 对齐部署树相对路径口径
//!   （`ServerPaths`：`config = cwd/../data/config.json`）；
//! * `KEYFLUX_API_PARITY=1`：跳过 schtasks 真实查询 / 提权 spawn（与 api-parity 同口径），
//!   让测试**无副作用、无机器态依赖** —— 否则开机自启状态会被写进断言、且可能拉起进程。

use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use config_ui_reactor::services::backend::parse_port_line;

/// 子进程不弹控制台（`settings.exe` 是控制台程序，见 `services::backend::spawn` 的注释）。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 已拉起并通告端口的 headless 后端；`Drop` 时杀进程 + 清沙箱。
struct Headless {
    child: Child,
    port: u16,
    sandbox: PathBuf,
}

impl Headless {
    /// 建沙箱 → 拉 `settings.exe --headless` → 读 stdout **首行**端口通告。
    fn start() -> Headless {
        let sandbox = make_sandbox();
        let bin_dir = sandbox.join("bin");
        let exe = env!("CARGO_BIN_EXE_settings");

        let mut child = Command::new(exe)
            .arg("--headless")
            .env("KEYFLUX_API_PARITY", "1")
            .current_dir(&bin_dir)
            .stdout(Stdio::piped())
            // stderr 丢弃：启动期本不该有输出；若进程早退，stdout EOF 即为失败信号，
            // 无需读 stderr（保留 pipe 反而有「缓冲写满 ⇒ 子进程阻塞」的死锁面）。
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {exe} --headless failed: {error}"));

        let stdout = child.stdout.take().expect("stdout was piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .expect("reading the port-announce line should not fail");
        if read == 0 {
            let _ = child.wait();
            panic!(
                "settings.exe --headless exited before announcing a port (no stdout); \
                 run `{exe} --headless` manually to see why"
            );
        }
        let port = parse_port_line(&line).unwrap_or_else(|| {
            panic!("stdout first line is not a KEYFLUX_PORT announce: {line:?}")
        });

        Headless {
            child,
            port,
            sandbox,
        }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// 轮询 `GET /health` 直到 200（通告行写在服务循环开始之前，存在极短窗口）。
    fn wait_ready(&self) {
        let agent = test_agent();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(response) = agent.get(&format!("{}/health", self.base())).call()
                && response.status().as_u16() == 200
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "backend on port {} never became healthy within 10s",
                self.port
            );
            sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        // 刻意忽略返回值（与 services::backend 同口径）：进程可能已自行退出，kill 报错属正常竞态。
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.sandbox);
    }
}

/// 建唯一沙箱：`bin/`（cwd）+ `data/config.json`（仓库出厂配置）。
fn make_sandbox() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!("kf-server-api-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(root.join("bin")).expect("create sandbox bin/");
    std::fs::create_dir_all(root.join("data")).expect("create sandbox data/");

    // 出厂配置真源 = 仓库根 data/config.json（CARGO_MANIFEST_DIR = config-ui-reactor/，其父即仓库根）。
    let factory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/config.json");
    std::fs::copy(&factory, root.join("data/config.json"))
        .unwrap_or_else(|error| panic!("seed factory config from {factory:?} failed: {error}"));
    root
}

/// 与面板同款策略的 HTTP 客户端（非 2xx 不折叠为错误，状态码由断言判定）。
fn test_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// 端口通告 + 健康检查 + 路由边界：`--headless` 全链路的最小骨架。
#[test]
fn headless_announces_port_and_serves_health() {
    let server = Headless::start();
    server.wait_ready();
    assert!(server.port > 0, "announced port must be non-zero");

    let agent = test_agent();
    let mut health = agent
        .get(&format!("{}/health", server.base()))
        .call()
        .expect("GET /health transport");
    assert_eq!(health.status().as_u16(), 200);
    assert_eq!(
        health
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/plain; charset=utf-8"
    );
    assert_eq!(health.body_mut().read_to_string().unwrap(), "ok");

    // 未知路径 → 404 空 body（gin NoRoute 同形）。查询串不参与路由。
    let mut not_found = agent
        .get(&format!("{}/nope?x=1", server.base()))
        .call()
        .expect("GET /nope transport");
    assert_eq!(not_found.status().as_u16(), 404);
    assert_eq!(not_found.body_mut().read_to_string().unwrap(), "");
}

/// `GET /config` 真实往返：200 + JSON DTO（含编译期注入的 `keyfluxVersion` 字段）。
#[test]
fn config_endpoint_returns_json_dto_over_http() {
    let server = Headless::start();
    server.wait_ready();
    let agent = test_agent();

    let mut response = agent
        .get(&format!("{}/config", server.base()))
        .call()
        .expect("GET /config transport");
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json; charset=utf-8"
    );
    let body = response.body_mut().read_to_string().unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&body).expect("GET /config body must be valid JSON");
    // `keyfluxVersion` 在 DTO 的 `options` 段（`dto::OptionsDto`），是 `option_env!("KEYFLUX_VERSION")`
    // 编译期注入路径在 wire 上的落点。不断言具体值（测试/发布两种构建注入不同值），只断言字段在场。
    assert!(
        value.pointer("/options/keyfluxVersion").is_some(),
        "DTO options must carry keyfluxVersion; got keys: {:?}",
        value.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
}

/// `POST /server/command/:id` 黑盒往返：未知 id 亦 200 `{}`，且不触发任何 spawn。
#[test]
fn server_command_returns_empty_object_over_http() {
    let server = Headless::start();
    server.wait_ready();
    let agent = test_agent();

    let mut response = agent
        .post(&format!("{}/server/command/99", server.base()))
        .send_empty()
        .expect("POST /server/command/99 transport");
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body_mut().read_to_string().unwrap(), "{}");
}
