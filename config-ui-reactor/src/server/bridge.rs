//! CLI 进程内 HTTP 桥 —— Go `internal/server/bridge.go`（119 行）全文件移植。
//!
//! 为什么需要（bridge.go 文件头注释）：设置面板要「不连 localhost、不起常驻
//! 后端进程」，但后端逻辑（handler 链）必须保持**单一真源**。本桥用进程内
//! dispatch 直接执行一次请求，**不开 socket、不占端口** —— CLI
//! （`settings.exe Call ...`）与 HTTP（`--headless`）共用同一套 handler，
//! 「换传输不换逻辑」。
//!
//! 契约（面板侧依赖，**禁改**）：
//! ```text
//! settings.exe Call <METHOD> <PATH> <out-file> [--body <file>] [--content-type <ct>]
//! stdout 末行: KEYFLUX_CALL status=<code>
//! 响应体原始字节写入 <out-file>
//! ```
//!
//! 工作目录语义与 HTTP 模式**一致**（Go 依赖相对 `../data`、`./behaviors`）：
//! 调用方必须把 cwd 设为 settings.exe 所在目录（即 `bin/`）。

use crate::server::{ServerContext, dispatch};

/// Go `bridge.CallStatusPrefix`：stdout 契约行前缀（**禁改**）。
const CALL_STATUS_PREFIX: &str = "KEYFLUX_CALL status=";

/// Go `http.NewRequest` 的 method token 校验（RFC 7230 tchar；非法方法报错
/// exit 2 —— 对应 Go CallOnce 的 NewRequest 错误路径）。
fn is_valid_method(method: &str) -> bool {
    !method.is_empty()
        && method.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

/// Go `Call`：CLI 入口（读 os.Args）。状态码/错误路径逐字对照：
/// * 参数不足 → stderr 用法行，exit 2；
/// * `--body` 读文件失败 → stderr，exit 2；
/// * 方法非法（NewRequest 失败）→ stderr，exit 2；
/// * out-file 写失败 → stderr，exit 2；
/// * 成功 → stdout 末行 `KEYFLUX_CALL status=<code>`，exit 0。
pub fn run_call(args: &[String]) -> std::process::ExitCode {
    // Go: len(os.Args) < 5 —— 程序名 + Call + METHOD + PATH + out-file
    if args.len() < 5 {
        crate::devlog!(
            "Call requires: Call <METHOD> <PATH> <out-file> [--body <file>] [--content-type <ct>]"
        );
        return std::process::ExitCode::from(2);
    }
    let method = &args[2];
    let path = &args[3];
    let out_file = &args[4];

    let mut body: Vec<u8> = Vec::new();
    let mut content_type = String::new();
    let mut index = 5;
    while index < args.len() {
        match args[index].as_str() {
            "--body" => {
                if index + 1 < args.len() {
                    match std::fs::read(&args[index + 1]) {
                        Ok(data) => body = data,
                        Err(error) => {
                            crate::devlog!("Call: read body failed: {error}");
                            return std::process::ExitCode::from(2);
                        }
                    }
                    index += 1;
                }
            }
            "--content-type" if index + 1 < args.len() => {
                content_type = args[index + 1].clone();
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }

    // CallOnce：进程内执行一次（debug 恒 false —— 与生产 HTTP 路径一致）
    if !is_valid_method(method) {
        crate::devlog!("Call: request failed: net/http: invalid method {method:?}");
        return std::process::ExitCode::from(2);
    }
    let ctx = ServerContext::new();
    let reply = dispatch(&ctx, method, path, &body, &content_type);

    if let Err(error) = std::fs::write(out_file, &reply.body) {
        crate::devlog!("Call: write output failed: {error}");
        return std::process::ExitCode::from(2);
    }
    println!("{CALL_STATUS_PREFIX}{}", reply.status);
    std::process::ExitCode::SUCCESS
}
