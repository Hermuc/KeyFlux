//! 面板后端 HTTP 服务模式 —— Rust `settings.exe --headless`（R1）。
//!
//! Go 参考实现（**已退役** `36ccb83`；本模块为现行实现，语义与冻结基线对齐 —— 溯源
//! `git show 36ccb83^:config-server/internal/server/`）：
//! * `config-server/cmd/settings/main.go:26`（`--headless` 分派）；
//! * `config-server/internal/server/server.go:74-115`（bind `localhost:12333` →
//!   失败退 `localhost:0`；**bind 成功后**打印 `KEYFLUX_PORT=<port>\n`，必须是
//!   stdout 第一行，之前不许有任何输出 —— 面板逐行匹配该前缀，
//!   `config-ui-reactor/src/services/backend.rs:33,44-48`）；
//! * 路由子集（server.go:38-43）：`GET /health`、`GET/PUT /config`、
//!   `POST /server/command/:id`。旧静态站（NoRoute → site/）已随旧 UI 退役，
//!   统一 404 空 body（`indexHandler` 在 `site/index.html` 缺失时即 404）。
//!
//! HTTP 层用 `tiny_http`（轻量、HTTP/1.1 keep-alive、并发连接；线程/连接），
//! 替代 gin 的最小子集。路由分发 [`dispatch`] 是**纯函数**（不含 socket），
//! 单测直接打表；[`run_headless`] 只负责 bind → 通告 → 服务循环。
//!
//! 与 Go 的显式差异（均为 headless 场景的等价或更优行为）：
//! * 双 bind 全失败：Go 在 headless 下也会 `fmt.Scanln()` 阻塞等回车（GUI 交互
//!   残留）再退出 1 —— Rust 直接打印错误并退出 1，不给面板挂 20s 端口超时；
//! * handler panic：Go 依赖 gin Recovery 回 500；Rust 在 handler 内显式返回 500，
//!   服务循环不受单请求异常影响。
//!
//! 架构铁律：本模块组全部在 **lib**，`bin/settings.rs` 只做参数分派。

pub mod bridge;
pub mod dto;
pub mod handlers_behaviors;
pub mod handlers_config;
pub mod handlers_plugins;
pub mod handlers_selected_action;
pub mod handlers_shortcuts;
pub mod proc;
pub mod settings_store;
pub mod validate;

use std::io::Write as _;
use std::sync::Arc;

/// Go `-ldflags -X settings/internal/script.KeyfluxVersion=…` 的构建期注入等价物
/// （`GET /config` 的 `keyfluxVersion` 字段来源）。
pub(crate) const VERSION: &str = match option_env!("KEYFLUX_VERSION") {
    Some(version) => version,
    None => "",
};

/// 单个 HTTP 响应的传输无关表示（路由分发与单测共用；socket 层仅做转换）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpReply {
    pub status: u16,
    /// `None` = 不设置（gin `c.Status(404)` 空 body 同形）。
    pub content_type: Option<&'static str>,
    pub body: Vec<u8>,
}

impl HttpReply {
    /// gin `c.JSON`：`application/json; charset=utf-8`。
    fn json(status: u16, body: String) -> Self {
        HttpReply {
            status,
            content_type: Some("application/json; charset=utf-8"),
            body: body.into_bytes(),
        }
    }

    /// gin `c.String`：`text/plain; charset=utf-8`。
    fn text(status: u16, body: &str) -> Self {
        HttpReply {
            status,
            content_type: Some("text/plain; charset=utf-8"),
            body: body.as_bytes().to_vec(),
        }
    }

    /// gin Recovery / `c.Status(n)`：无 Content-Type、空 body。
    fn empty(status: u16) -> Self {
        HttpReply {
            status,
            content_type: None,
            body: Vec::new(),
        }
    }
}

/// 部署树路径视图（Go 相对路径口径的显式化，全部从 `(cwd, exe_dir)` 推导）。
#[derive(Debug, Clone)]
pub(crate) struct ServerPaths {
    /// Go `script.ConfigRelPath`：`../data/config.json`（相对进程 cwd）。
    pub config_file: std::path::PathBuf,
    /// Go `loadBehaviorCatalog` 内置包：`<exe 目录>/behaviors`。
    pub builtin_behaviors: std::path::PathBuf,
    /// Go `userBehaviorsDir`：`../data/behaviors`（相对进程 cwd）。
    pub user_behaviors: std::path::PathBuf,
    /// Go `userPluginsDir`：`../data/plugins`（相对进程 cwd）。
    pub user_plugins: std::path::PathBuf,
}

impl ServerPaths {
    /// 从进程运行参数推导（cwd = Go 的相对路径基准，exe_dir = `os.Executable` 目录）。
    pub fn from_process() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        ServerPaths::new(&cwd, &exe_dir)
    }

    /// 词法拼接（同 Go `filepath.Abs("../…")`：不解析符号链接、保留 `..`）。
    pub(crate) fn new(cwd: &std::path::Path, exe_dir: &std::path::Path) -> Self {
        ServerPaths {
            config_file: cwd.join("../data/config.json"),
            builtin_behaviors: exe_dir.join("behaviors"),
            user_behaviors: cwd.join("../data/behaviors"),
            user_plugins: cwd.join("../data/plugins"),
        }
    }
}

/// 服务上下文：路径 + 启动缓存 + 插件设置存储 + 可注入副作用。
pub(crate) struct ServerContext {
    pub paths: ServerPaths,
    pub startup: handlers_config::StartupCache,
    /// Go 包级单例 `pluginSettings`（内部自带互斥锁；路径 = `../data/plugin-settings.json`）。
    pub plugin_settings: settings_store::SettingsStore,
    pub hooks: handlers_config::Hooks,
}

impl ServerContext {
    pub fn new() -> Self {
        ServerContext::with_hooks(
            ServerPaths::from_process(),
            handlers_config::Hooks::default(),
        )
    }

    pub(crate) fn with_hooks(paths: ServerPaths, hooks: handlers_config::Hooks) -> Self {
        let plugin_settings = settings_store::SettingsStore::new(
            paths
                .config_file
                .with_file_name(settings_store::SETTINGS_FILE_NAME),
        );
        ServerContext {
            paths,
            startup: handlers_config::StartupCache::default(),
            plugin_settings,
            hooks,
        }
    }
}

/// 路由分发（纯函数，socket 无关）。方法/路径之外的部分（查询串）已剥除。
/// 未知路径/方法一律 404 空 body（gin NoRoute → indexHandler 在 site 缺失时同形）。
///
/// `content_type` 仅 `POST /api/plugins/import`（multipart 解析）消费 ——
/// gin `c.FormFile` 从请求头取 boundary；Call 桥与 HTTP 服务分别从
/// `--content-type` / 请求头注入。
pub(crate) fn dispatch(
    ctx: &ServerContext,
    method: &str,
    path: &str,
    body: &[u8],
    content_type: &str,
) -> HttpReply {
    let path = path.split('?').next().unwrap_or(path);
    // gin `:id` 段语义：非空且不含 '/'
    let last_segment = |rest: &str| !rest.is_empty() && !rest.contains('/');
    // `<id>/settings` 尾缀形态（GET/PUT /api/plugins/:id/settings）
    fn plugin_settings_id(rest: &str) -> Option<&str> {
        let id = rest.strip_suffix("/settings")?;
        if !id.is_empty() && !id.contains('/') {
            Some(id)
        } else {
            None
        }
    }
    match (method, path) {
        ("GET", "/health") => HttpReply::text(200, "ok"),
        ("GET", "/config") => handlers_config::get_config(ctx),
        ("PUT", "/config") => handlers_config::put_config(ctx, body),
        ("GET", "/shortcuts") => handlers_shortcuts::get_shortcuts(ctx),
        ("POST", "/api/selected-action/test") => {
            handlers_selected_action::test_selected_action(ctx, body)
        }
        ("POST", "/api/selected-action/play") => {
            handlers_selected_action::play_selected_action(ctx, body)
        }
        ("GET", "/api/behaviors") => handlers_behaviors::get_behaviors(ctx),
        ("POST", "/api/behaviors") => handlers_behaviors::create_behavior(ctx, body),
        ("POST", "/api/behaviors/apply") => handlers_behaviors::apply_behaviors(ctx),
        ("PUT", p) if p.starts_with("/api/behaviors/") => {
            let id = &p["/api/behaviors/".len()..];
            if !last_segment(id) {
                return HttpReply::empty(404);
            }
            handlers_behaviors::update_behavior(ctx, id, body)
        }
        ("DELETE", p) if p.starts_with("/api/behaviors/") => {
            let id = &p["/api/behaviors/".len()..];
            if !last_segment(id) {
                return HttpReply::empty(404);
            }
            handlers_behaviors::delete_behavior(ctx, id)
        }
        ("GET", "/api/plugins") => handlers_plugins::get_plugins(ctx),
        ("POST", "/api/plugins/import") => handlers_plugins::import_plugin(ctx, content_type, body),
        ("DELETE", p) if p.starts_with("/api/plugins/") => {
            let id = &p["/api/plugins/".len()..];
            if !last_segment(id) {
                return HttpReply::empty(404);
            }
            handlers_plugins::delete_plugin(ctx, id)
        }
        ("GET", p) if p.starts_with("/api/plugins/") => {
            match plugin_settings_id(&p["/api/plugins/".len()..]) {
                Some(id) => handlers_plugins::get_plugin_settings(ctx, id),
                None => HttpReply::empty(404),
            }
        }
        ("PUT", p) if p.starts_with("/api/plugins/") => {
            match plugin_settings_id(&p["/api/plugins/".len()..]) {
                Some(id) => handlers_plugins::save_plugin_settings(ctx, id, body),
                None => HttpReply::empty(404),
            }
        }
        ("POST", p) if p.starts_with("/server/command/") => {
            server_command(ctx, &p["/server/command/".len()..])
        }
        _ => HttpReply::empty(404),
    }
}

/// Go `ServerCommandHandler`（handlers.go:117-140）：白名单 2=WindowSpy /
/// 3=自启开 / 4=自启关。未知 id 也 200 空 `{}`。
/// （Go 路由 `:id` 不匹配空段或含 `/` 的段 → 落 NoRoute 404。）
///
/// 与 Go 的**显式差异**（2026-10-07）：3/4 改经 [`proc::exec_cmd_elevated`]
/// 提权 spawn。计划任务 KeyFlux 由提权链创建（runLevel HIGHEST），非提权上下文
/// `schtasks /delete` 直接「拒绝访问」，而 MiscTools.ahk 仅 On 分支内建 `*RunAs`
/// 自提权（`bin/**` 边界零改动，无法给 Off 补）——引擎托盘拉起的面板链本就提权
/// 无感，直启未提权面板弹一次 UAC，与 On 自提权的既有 UX 对称。
/// spawn 结果照旧被忽略（Go 同）：HTTP 契约恒 `200 {}`，失败只进 stderr 诊断。
fn server_command(_ctx: &ServerContext, id: &str) -> HttpReply {
    if id.is_empty() || id.contains('/') {
        return HttpReply::empty(404);
    }
    // 夹具模式（api-parity）：3/4 不真正 spawn —— 基线沙箱里 stub 引擎「吞参数
    // 即退」，而提权 spawn 只会对账一次 UAC 噪音；与 query_startup_from_task 的
    // 夹具分支（handlers_config）同一口径。响应字节不受影响，恒 `200 {}`。
    let fixture = std::env::var("KEYFLUX_API_PARITY").as_deref() == Ok("1");
    // proc.ExecCmd 的返回值在此被忽略（Go 同）
    if let Some((elevated, args)) = command_spec(id)
        && !fixture
    {
        if elevated {
            proc::exec_cmd_elevated("./KeyFlux.exe", args);
        } else {
            proc::exec_cmd("./KeyFlux.exe", args);
        }
    }
    HttpReply::json(200, "{}".to_string())
}

/// 白名单命令表（KeyFlux.exe 参数，与 Go 逐字一致）。
const WINDOWSPY_ARGS: &[&str] = &["/script", "bin/WindowSpy.ahk"];
const STARTUP_ON_ARGS: &[&str] = &["/script", "./bin/MiscTools.ahk", "RunAtStartup", "On"];
const STARTUP_OFF_ARGS: &[&str] = &["/script", "./bin/MiscTools.ahk", "RunAtStartup", "Off"];

/// 白名单命令表：id → (是否需提权, KeyFlux.exe 参数)。
/// 3/4 写计划任务需要提权（见 [`server_command`] 边界说明），2=WindowSpy 不需要。
fn command_spec(id: &str) -> Option<(bool, &'static [&'static str])> {
    match id {
        "2" => Some((false, WINDOWSPY_ARGS)),
        "3" => Some((true, STARTUP_ON_ARGS)),
        "4" => Some((true, STARTUP_OFF_ARGS)),
        _ => None,
    }
}

/// Go `net.Listen("tcp", addr)` 的语义复刻：**只绑一个地址**。
///
/// Go 对 `localhost` 这类多址名只选一个（`favoriteAddrFamily` 偏好 IPv4，
/// 本机实证 `net.Listen("tcp","localhost:12333")` 绑定 `127.0.0.1:12333`），
/// 该地址被占 ⇒ 整次 bind 失败 ⇒ 退随机端口 —— 这正是面板「按通告端口连接」
/// 契约的前提（第二实例通告随机端口，绝不允许两个实例通告同一端口）。
/// tiny_http 的 `Server::http` 会把解析出的全部地址逐个尝试（两个实例分别绑
/// `::1` 与 `127.0.0.1` 后都通告 12333），故必须自持 listener 再 `from_listener`。
///
/// 地址选择：解析结果中取**首个 IPv4**；无 IPv4（纯 IPv6 环境）取首个。
fn bind_single(addr_spec: &str) -> std::io::Result<std::net::TcpListener> {
    let addrs: Vec<std::net::SocketAddr> =
        std::net::ToSocketAddrs::to_socket_addrs(&addr_spec)?.collect();
    let Some(addr) = addrs.iter().find(|a| a.is_ipv4()).or_else(|| addrs.first()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            format!("no address resolved for {addr_spec}"),
        ));
    };
    std::net::TcpListener::bind(addr)
}

/// `--headless` 入口：bind → stdout 端口通告（第一行，flush）→ 永久服务。
/// 仅在 bind 全失败时返回（退出码 1）。
pub fn run_headless() -> std::process::ExitCode {
    // 先试 12333（可能被占用/被防火墙禁），失败退随机端口（server.go:77-91）
    let listener = match bind_single("localhost:12333") {
        Ok(listener) => listener,
        Err(_) => match bind_single("localhost:0") {
            Ok(listener) => listener,
            Err(error) => {
                println!("Error: {error}");
                return std::process::ExitCode::from(1);
            }
        },
    };
    let port = listener
        .local_addr()
        .map(|addr| addr.port())
        .unwrap_or_default();
    let server = tiny_http::Server::from_listener(listener, None)
        .expect("已 bind 的 listener 包装为 tiny_http Server 不应失败");

    // 端口通告行：必须为 stdout 第一行输出，供面板逐行匹配 "KEYFLUX_PORT=" 前缀
    // （不打印任何装饰文本）。bind 成功后才打印。
    {
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "KEYFLUX_PORT={port}");
        let _ = stdout.flush();
    }

    let ctx = Arc::new(ServerContext::new());
    // 异步预热开机自启缓存：首次 GET /config 免等 schtasks（Go PreloadStartup）
    ctx.startup.preload();

    // 线程/连接：面板可能并发请求（ureq 连接池），单请求异常不拖垮服务循环
    for request in server.incoming_requests() {
        let ctx = Arc::clone(&ctx);
        std::thread::spawn(move || handle_request(ctx, request));
    }
    // incoming_requests 只在服务端 socket 关闭时返回；与 Go RunListener 同为不可达
    std::process::ExitCode::SUCCESS
}

/// 单请求处理：读 body → dispatch → 回写（tiny_http 负责 Content-Length 与
/// keep-alive）。
fn handle_request(ctx: Arc<ServerContext>, mut request: tiny_http::Request) {
    let method = request.method().as_str().to_ascii_uppercase();
    let url = request.url().to_string();
    let mut body = Vec::new();
    // 必须读完 body 才能响应（keep-alive 连接上的下一请求依赖流复位）
    if std::io::Read::read_to_end(request.as_reader(), &mut body).is_err() {
        body.clear();
    }
    let reply = dispatch(
        &ctx,
        &method,
        &url,
        &body,
        request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Content-Type"))
            .map(|header| header.value.as_str())
            .unwrap_or(""),
    );

    let mut response = tiny_http::Response::from_data(reply.body).with_status_code(reply.status);
    if let Some(header) = reply.content_type.and_then(|content_type| {
        tiny_http::Header::from_bytes("Content-Type".as_bytes(), content_type.as_bytes()).ok()
    }) {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ServerContext {
        // 无副作用桩：dispatch 的路由级测试不触发进程/文件访问
        // （/config 路径的完整行为在 handlers_config / dto 的测试里覆盖）
        ServerContext::with_hooks(
            ServerPaths::new(
                std::path::Path::new("definitely-missing-cwd"),
                std::path::Path::new("also-missing"),
            ),
            handlers_config::Hooks {
                restart_engine: Box::new(|| false),
                stop_process: Box::new(|_| false),
                query_startup: Box::new(|| false),
            },
        )
    }

    /// GET /health → 200 "ok"（text/plain），gin c.String 同形。
    #[test]
    fn health_returns_ok() {
        let reply = dispatch(&ctx(), "GET", "/health", b"", "");
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, b"ok");
        assert_eq!(reply.content_type, Some("text/plain; charset=utf-8"));
    }

    /// 未知路径/方法 → 404 空 body（含查询串剥离、:id 边界）。
    #[test]
    fn unknown_routes_return_404() {
        assert_eq!(dispatch(&ctx(), "GET", "/", b"", "").status, 404);
        assert_eq!(dispatch(&ctx(), "GET", "/nope", b"", "").status, 404);
        // 查询串被剥除后 /config 仍命中（沙箱无配置 → handler 层 500 而非 404）
        assert_eq!(dispatch(&ctx(), "GET", "/config?x=1", b"", "").status, 500);
        let health = dispatch(&ctx(), "GET", "/health?probe=1", b"", "");
        assert_eq!(health.status, 200);
        // 方法不匹配 → 404（gin 只注册了对应方法）
        assert_eq!(dispatch(&ctx(), "POST", "/config", b"", "").status, 404);
        assert_eq!(dispatch(&ctx(), "PUT", "/health", b"", "").status, 404);
        // :id 空段 / 含斜杠 → 404（gin 路由参数边界）
        assert_eq!(
            dispatch(&ctx(), "POST", "/server/command/", b"", "").status,
            404
        );
        assert_eq!(
            dispatch(&ctx(), "POST", "/server/command/2/extra", b"", "").status,
            404
        );
    }

    /// POST /server/command/未知 id → 200 `{}`（不触发 spawn）。
    #[test]
    fn unknown_command_id_returns_empty_object() {
        let reply = dispatch(&ctx(), "POST", "/server/command/99", b"", "");
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, b"{}");
        assert_eq!(reply.content_type, Some("application/json; charset=utf-8"));
    }

    /// 白名单 id 的参数表与 Go 逐字一致（字节锁定，防止参数漂移）；
    /// 提权分类：3/4 写计划任务需提权（见 `server_command` 边界说明），2 不需要。
    #[test]
    fn whitelist_command_args_match_go() {
        assert_eq!(command_spec("2"), Some((false, WINDOWSPY_ARGS)));
        assert_eq!(command_spec("3"), Some((true, STARTUP_ON_ARGS)));
        assert_eq!(command_spec("4"), Some((true, STARTUP_OFF_ARGS)));
        assert_eq!(command_spec("99"), None);
        for (id, spec) in [
            ("2", command_spec("2")),
            ("3", command_spec("3")),
            ("4", command_spec("4")),
        ] {
            let (_, args) = spec.expect("白名单 id 恒有表");
            assert_eq!(args.first(), Some(&"/script"), "id={id} 应以 /script 开头");
        }
    }

    /// 端口通告解析与面板契约（backend.rs parse_port_line 同口径）。
    #[test]
    fn port_announce_line_matches_panel_contract() {
        let line = format!("KEYFLUX_PORT={}", 12333u16);
        let rest = line.trim().strip_prefix("KEYFLUX_PORT=").unwrap();
        assert_eq!(rest.trim().parse::<u16>().unwrap(), 12333);
    }

    /// bind 地址选择：Go favoriteAddrFamily 偏好 IPv4 —— 多址解析结果取首个
    /// IPv4；纯 IPv6 结果取首个。每轮只绑**一个**地址（契约：占用即整体失败）。
    #[test]
    fn bind_address_selection_prefers_single_ipv4() {
        let addrs = |items: &[&str]| -> Option<String> {
            let parsed: Vec<std::net::SocketAddr> =
                items.iter().map(|s| s.parse().unwrap()).collect();
            parsed
                .iter()
                .find(|a| a.is_ipv4())
                .or_else(|| parsed.first())
                .map(|a| a.to_string())
        };
        // localhost 的典型解析序（IPv6 在前）⇒ 仍选 IPv4
        assert_eq!(
            addrs(&["[::1]:12333", "127.0.0.1:12333"]).as_deref(),
            Some("127.0.0.1:12333")
        );
        // IPv4 在前 ⇒ 选它
        assert_eq!(
            addrs(&["127.0.0.1:12333", "[::1]:12333"]).as_deref(),
            Some("127.0.0.1:12333")
        );
        // 纯 IPv6 ⇒ 首个
        assert_eq!(
            addrs(&["[::1]:12333", "[fe80::1]:12333"]).as_deref(),
            Some("[::1]:12333")
        );
        // 空解析 ⇒ None（整体失败 → 退随机端口）
        let empty: Vec<std::net::SocketAddr> = Vec::new();
        assert!(
            empty
                .iter()
                .find(|a| a.is_ipv4())
                .or_else(|| empty.first())
                .is_none()
        );
    }
}
