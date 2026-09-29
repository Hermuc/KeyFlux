//! 传输选择层（`--api=http|cli`）。
//!
//! 为什么单独一层：面板里 `SettingsApi` 的构造点很多（页面/对话框各自 `new`），
//! 若散落写 `HttpSettingsApi::new`，换传输就得改十几处且容易漏。统一走
//! [`new_settings_api`] ⇒ 只在地板一层决定传输，调用点与传输无关（端口/适配器解耦）。

use std::sync::{Arc, OnceLock};

use super::api::{HttpSettingsApi, SettingsApi};
use super::cli_api::CliSettingsApi;

/// 传输模式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Transport {
    /// HTTP：拉起 `settings.exe --headless` 子进程 + `127.0.0.1:<端口>`（现状，**默认**）。
    #[default]
    Http,
    /// CLI：进程内桥（无端口、无常驻后端、无端口协商）。
    Cli,
}

static TRANSPORT: OnceLock<Transport> = OnceLock::new();

/// 设定传输模式（进程启动时一次；重复调用只生效第一次）。
pub fn set_transport(mode: Transport) {
    let _ = TRANSPORT.set(mode);
}

/// 当前传输模式（默认 [`Transport::Http`]，即未显式切换时行为与改动前完全一致）。
pub fn transport() -> Transport {
    *TRANSPORT.get().unwrap_or(&Transport::Http)
}

/// 从启动参数解析传输（`--api=cli` / `--api=http`）。
///
/// 优先级：命令行 > 环境变量 `KEYFLUX_API`（`cli`/`http`）> 默认 [`Transport::Http`]。
/// 环境变量这条通道是**免重建的逃生阀**：面板由引擎拉起（不传参），出问题时
/// 设 `KEYFLUX_API=http` 即可立刻回到旧传输。
pub fn parse_transport(args: &[String]) -> Transport {
    parse_transport_with_env(args, std::env::var("KEYFLUX_API").ok().as_deref())
}

/// 纯函数版（环境变量作为入参传入）—— 便于确定性单测（不碰进程环境）。
fn parse_transport_with_env(args: &[String], env: Option<&str>) -> Transport {
    for arg in args {
        match arg.as_str() {
            "--api=cli" => return Transport::Cli,
            "--api=http" => return Transport::Http,
            _ => {}
        }
    }
    match env {
        Some("cli") => Transport::Cli,
        Some("http") => Transport::Http,
        _ => Transport::default(),
    }
}

/// 统一工厂：所有 `SettingsApi` 构造都应走这里。
pub fn new_settings_api(port: u16) -> Arc<dyn SettingsApi> {
    match transport() {
        Transport::Http => Arc::new(HttpSettingsApi::new(port)),
        Transport::Cli => Arc::new(CliSettingsApi::for_panel()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_transport_defaults_to_http() {
        assert_eq!(parse_transport_with_env(&[], None), Transport::Http);
        assert_eq!(
            parse_transport_with_env(&["--headless".to_string()], Some("bogus")),
            Transport::Http
        );
    }

    #[test]
    fn parse_transport_args_beat_env() {
        // 优先级：命令行 > 环境变量
        assert_eq!(
            parse_transport_with_env(&["--api=http".to_string()], Some("cli")),
            Transport::Http
        );
        assert_eq!(
            parse_transport_with_env(&["--api=cli".to_string()], Some("http")),
            Transport::Cli
        );
    }

    #[test]
    fn parse_transport_reads_env_valve() {
        assert_eq!(parse_transport_with_env(&[], Some("cli")), Transport::Cli);
        assert_eq!(parse_transport_with_env(&[], Some("http")), Transport::Http);
    }
}
