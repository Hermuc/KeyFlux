//! 外部 HTTP 客户端的**单一策略点**（TLS 根证书 / 超时 / 状态码语义）。
//!
//! 为什么单独成模块：市场目录（`market`）与发布运维工具（`build_tools`）都要发**外部**
//! HTTPS 请求。把 `ureq::Agent` 的构造留在各自模块，就会出现同一策略两处维护——
//! 本仓库已经为「同一命令抄多份 ⇒ 悄悄漂移」付过代价（见 `tools/lib/kf-tools.ps1` 头注
//! 与 `Makefile`/`release.yml` 的 robocopy 排除集漂移）。
//!
//! 口径（**不得随意改**，改则两处调用方同时受影响）：
//! * `root_certs = PlatformVerifier`：跟随 **OS 信任库**。内置 webpki-roots 不含用户
//!   安装的代理/企业根，国内网络下外部域名被劫持重签时恒报 UnknownIssuer
//!   （2026-10-02 实测，见 `market.rs` 的历史注）；跟随系统库与 git/curl 行为一致。
//! * `http_status_as_error(false)`：状态码由**调用方**判定，避免把 4xx 当传输错误。
//! * 超时由调用方按场景传入（市场 15s / 版本检查 5s）。

use std::time::Duration;

use ureq::Agent;

/// 构造外部请求用的 Agent（TLS 策略 + 全局超时 + 状态码语义）。
pub fn platform_agent(timeout: Duration) -> Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    config.into()
}
