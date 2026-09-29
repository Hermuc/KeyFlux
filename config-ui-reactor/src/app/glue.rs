//! `app` 的后台连接/保存**自由函数**（自原 `app.rs` 拆分）。
//!
//! 非 UI 线程侧：连接后端（直连或子进程）、拉配置、保存。

use super::*;

// ------------------------------------------------------------- 后台工作（非 UI 线程）

/// 连接后端（直连或子进程）、拉取配置与使用指南文档；会话移交到组件持有的槽。
pub fn load_backend(slot: SessionSlot) -> Message {
    let args: Vec<String> = std::env::args().collect();
    let options = BackendSessionOptions::parse(&args);

    // CLI 传输（`--api=cli`）：无端口、无常驻后端进程。**探测失败自动回退 HTTP** ——
    // 面板在任何情况下都必须可用（这条安全网使切换传输不会把用户挡在设置之外）。
    if crate::services::transport::transport() == crate::services::transport::Transport::Cli {
        let cli = crate::services::cli_api::CliSettingsApi::for_panel();
        if cli.health_check() {
            return assemble(&cli, 0, &options);
        }
        // 落到下面的 HTTP 分支
    }

    let session = match connect(&options) {
        Ok(session) => session,
        Err(reason) => return Message::Failed(reason),
    };
    let port = session.port();
    let api = session.api();
    // 会话必须存活（持有子进程，drop 即终止）⇒ 移交到组件持有的槽。
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(session);
    }
    assemble(api.as_ref(), port, &options)
}

/// 连接成功后统一装载：配置 + 使用指南 + 快捷方式。
///
/// 抽出的理由：CLI 与 HTTP 两条传输的**装载语义必须逐字一致** —— 放在一处避免第二真源。
fn assemble(api: &dyn SettingsApi, port: u16, options: &BackendSessionOptions) -> Message {
    let response = api.get_config();
    let Some(config) = response.value.clone() else {
        return Message::Failed(
            response
                .error_message
                .unwrap_or_else(|| format!("读取配置失败 (status {})", response.status)),
        );
    };

    // 使用指南文档：自定义内容优先，为空时取后端静态站的默认文档
    // （对齐旧 `HomePageViewModel.LoadAsync`）。
    let mut doc_md = config.overview_doc_md.clone();
    if doc_md.trim().is_empty() {
        doc_md = api.get_raw_text("/config_doc.md").value.unwrap_or_default();
    }

    // 快捷方式列表（`GET /shortcuts`；空目录后端返回 null ⇒ 容忍为空）
    let shortcuts = api
        .get_shortcuts()
        .value
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.path)
        .collect::<Vec<_>>();

    Message::Ready {
        config: Box::new(config),
        port,
        doc_md,
        shortcuts,
        data_root: resolve_deployment_root(options),
    }
}

/// 解析部署根（`<deploy>`）：`<deploy>/bin/settings.exe` ⇒ 其祖父目录。
///
/// 直连模式（`--port`）下无从推断，退回 `--backend-dir` 指定的工作目录。
pub fn resolve_deployment_root(options: &BackendSessionOptions) -> Option<std::path::PathBuf> {
    if options.direct_port.is_some() {
        return options.working_directory.clone();
    }
    let exe_dir = std::env::current_exe()
        .ok()?
        .parent()
        .map(std::path::Path::to_path_buf)?;
    let exe = resolve_settings_exe(options, &exe_dir)?;
    exe.parent()?.parent().map(std::path::Path::to_path_buf)
}

pub fn connect(options: &BackendSessionOptions) -> Result<BackendSession, String> {
    if let Some(port) = options.direct_port {
        return BackendSession::connect_direct(port, options).map_err(|error| error.to_string());
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .ok_or_else(|| "无法确定当前可执行文件目录".to_string())?;
    let exe = resolve_settings_exe(options, &exe_dir).ok_or_else(|| {
        let searched = crate::services::backend::settings_exe_candidates(&exe_dir)
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("; ");
        format!("未找到 settings.exe (查找位置: {searched})")
    })?;

    BackendSession::spawn(&exe, options.working_directory.as_deref(), options)
        .map_err(|error| error.to_string())
}

/// 清洗后 PUT 配置（`ConfigSaver` 语义），返回保存提示文案。
pub fn save(port: u16, config: &Config) -> Result<String, String> {
    let payload = store::clean_for_save(config);
    let api = crate::services::transport::new_settings_api(port);
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
