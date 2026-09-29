//! `/config` 路由的 handler —— Go `internal/server/handlers.go`
//! （`GetConfigHandler` :19-29 / `SaveConfigHandler` :142-191）与开机自启显示态
//! 缓存（:31-74）、命令框外观对比（`script/font.go:219-234`）的移植。
//!
//! 路径口径（与 Go 逐字一致，均相对**进程 cwd**，即部署树 `bin/`）：
//! * 配置文件：`../data/config.json`（`script.ConfigRelPath`）；
//! * 内置行为包：`<settings.exe 目录>/behaviors`（`os.Executable`）；
//! * 用户行为包：`../data/behaviors`。
//!
//! 错误口径：Go 的 handler 在读取/解析/落盘失败时 `panic` → gin Recovery →
//! **500 空 body**；Rust 以 `Err` 显式表达，由 [`super`] 统一映射为 500。

use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::generator::config::{parse_config, save_config_file};
use crate::generator::model::{CommandFontOption, CommandInputSkin};

use super::dto::{bind_config_dto, config_to_dto, dto_to_config, marshal_go_json};
use super::validate::{load_validation_catalog, validate_file_groups, validate_selected_action};
use super::{HttpReply, ServerPaths, VERSION};

/// 开机自启显示态缓存的 TTL。Go `getCachedStartupState`（handlers.go:63-74）。
const STARTUP_CACHE_TTL: Duration = Duration::from_secs(2);

/// 进程内可注入的副作用钩子（默认接 [`super::proc`]；测试注入桩，
/// 避免单测真的拉起 KeyFlux / explorer / taskkill）。
pub(crate) struct Hooks {
    /// Go `proc.ExecCmd("./KeyFlux.exe")`：保存后重启引擎，返回是否成功。
    pub restart_engine: Box<dyn Fn() -> bool + Send + Sync>,
    /// Go `proc.StopProcessByName`：结束命令框进程（外观变化时）。
    pub stop_process: Box<dyn Fn(&str) -> bool + Send + Sync>,
    /// Go `queryStartupFromTask`：schtasks 查询计划任务存在性。
    pub query_startup: Box<dyn Fn() -> bool + Send + Sync>,
}

impl Default for Hooks {
    fn default() -> Self {
        Hooks {
            restart_engine: Box::new(|| super::proc::exec_cmd("./KeyFlux.exe", &[])),
            stop_process: Box::new(super::proc::stop_process_by_name),
            query_startup: Box::new(query_startup_from_task),
        }
    }
}

// --------------------------------------------------------------------------- 开机自启显示态

/// Go `queryStartupFromTask`（handlers.go:42-45）：`schtasks /query /tn KeyFlux`
/// 成功且 stdout 含 "KeyFlux" ⇒ 任务存在。任何失败（任务不存在返回非零/权限等）
/// 均回 `false`，不报错。
fn query_startup_from_task() -> bool {
    match std::process::Command::new("schtasks")
        .args(["/query", "/tn", "KeyFlux"])
        .output()
    {
        Ok(output) => output.status.success() && output.stdout.windows(7).any(|w| w == b"KeyFlux"),
        Err(_) => false,
    }
}

/// 开机自启显示态缓存（Go `startupMu/startupCacheVal/startupCacheAt` 的等价物，
/// 放在 [`super::ServerContext`] 内而非全局静态，使单测互不干扰）：
/// 启动时异步预热 + TTL 2s + 保存路径写时失效。GET 命中缓存零 IO。
#[derive(Default)]
pub(crate) struct StartupCache {
    state: std::sync::Arc<std::sync::Mutex<Option<(bool, Instant)>>>,
}

impl StartupCache {
    /// Go `getCachedStartupState`：未预热/过期时同步查一次并回填
    /// （Go 在持锁状态下查询，此处一致）。
    pub(crate) fn get(&self, hooks: &Hooks) -> bool {
        let mut guard = self.state.lock().expect("startup 缓存锁不应中毒");
        if let Some((value, _at)) = guard
            .as_ref()
            .filter(|(_, at)| at.elapsed() < STARTUP_CACHE_TTL)
        {
            return *value;
        }
        let value = (hooks.query_startup)();
        *guard = Some((value, Instant::now()));
        value
    }

    /// Go `PreloadStartup`：后台预热（与面板 GUI 冷启动并行，首次 GET 免等
    /// schtasks）。预热线程使用默认钩子（真实 schtasks 查询）。
    pub(crate) fn preload(&self) {
        let state = std::sync::Arc::clone(&self.state);
        std::thread::spawn(move || {
            let hooks = Hooks::default();
            let value = (hooks.query_startup)();
            let mut guard = state.lock().expect("startup 缓存锁不应中毒");
            *guard = Some((value, Instant::now()));
        });
    }

    /// Go `invalidateStartupCache`：归零 = 失效，下次 GET 同步重查。
    pub(crate) fn invalidate(&self) {
        let mut guard = self.state.lock().expect("startup 缓存锁不应中毒");
        *guard = None;
    }
}

// --------------------------------------------------------------------------- GET /config

/// Go `GetConfigHandler`（handlers.go:19-29）：ParseConfig → 计划任务真实态回填
/// `options.startup` → ConfigToDTO → 200。
pub(crate) fn get_config(ctx: &super::ServerContext) -> HttpReply {
    let startup = ctx.startup.get(&ctx.hooks);
    build_get_config(&ctx.paths, startup)
}

/// [`get_config`] 的可测内核（startup 由参数注入，避开 schtasks 的非确定性）。
pub(crate) fn build_get_config(paths: &ServerPaths, startup: bool) -> HttpReply {
    match parse_config(&paths.config_file, VERSION) {
        Ok(mut config) => {
            // 以计划任务真实生效态回填开机自启显示态（DTO 转换之前，确保体现在响应中）
            config.options.startup = startup;
            HttpReply::json(200, marshal_go_json(&config_to_dto(&config)))
        }
        // Go: panic(err) → gin Recovery → 500 空 body
        Err(_) => HttpReply::empty(500),
    }
}

// --------------------------------------------------------------------------- 外观探针

/// Go `CommandBoxAppearance`（font.go:205-208）：命令框外观两段 —— 字体与皮肤。
#[derive(Debug, Default)]
struct CommandBoxAppearance {
    font: CommandFontOption,
    skin: CommandInputSkin,
}

/// Go `script.CommandBoxAppearanceFromConfigFile`（font.go:219-234）：
/// 从**已落盘**的 config.json 读出命令框外观两段。容错口径：任何读取/解析失败
/// 一律返回零值（零值与用户的实际选择必然不等 ⇒ 判成「变了」走保守分支）。
/// 必须在覆盖写 config.json **之前**调用。
fn command_box_appearance_from_file(config_path: &std::path::Path) -> CommandBoxAppearance {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Probe {
        options: ProbeOptions,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct ProbeOptions {
        #[serde(rename = "commandFont")]
        command_font: CommandFontOption,
        #[serde(rename = "commandInputSkin")]
        command_input_skin: CommandInputSkin,
    }

    let Ok(raw) = std::fs::read(config_path) else {
        return CommandBoxAppearance::default();
    };
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&raw);
    // Go json.Unmarshal 把 null 解零值；serde 不容忍 → 同样的 null 剥除口径
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return CommandBoxAppearance::default();
    };
    crate::generator::model::strip_null_fields(&mut value);
    match serde_json::from_value::<Probe>(value) {
        Ok(probe) => CommandBoxAppearance {
            font: probe.options.command_font,
            skin: probe.options.command_input_skin,
        },
        Err(_) => CommandBoxAppearance::default(),
    }
}

/// 字体段比较（Go `config.Options.CommandFont != prevAppearance.Font` 的结构体
/// 等值；模型 `CommandFontOption` 未派生 PartialEq，逐字段比）。
fn command_font_eq(a: &CommandFontOption, b: &CommandFontOption) -> bool {
    a.source_path == b.source_path && a.weight == b.weight
}

// --------------------------------------------------------------------------- PUT /config

/// Go `SaveConfigHandler`（handlers.go:142-191）：
/// 绑定 DTO → DTOToConfig → ValidateSelectedAction + ValidateFileGroups（失败
/// 400 `{"message":"保存失败: …"}`）→ 读旧外观 → SaveConfigFile → 缓存失效 →
/// 外观变了结束命令框 → 重启引擎 → 200 `{"message":"ok","restartFailed":bool}`。
pub(crate) fn put_config(ctx: &super::ServerContext, body: &[u8]) -> HttpReply {
    let dto = match bind_config_dto(body) {
        Ok(dto) => dto,
        // Go: ShouldBindJSON 失败 panic → gin Recovery → 500 空 body
        Err(_) => return HttpReply::empty(500),
    };
    // DTO→model 映射在校验与落盘之前
    let config = dto_to_config(&dto);

    // 校验选中动作组合合法性（entry 引用的行为必须存在且覆盖匹配前提）
    let catalog = load_validation_catalog(&ctx.paths.builtin_behaviors, &ctx.paths.user_behaviors);
    if let Err(error) =
        validate_selected_action(config.selected_action.as_ref(), Some(&catalog), &config)
    {
        return save_failure(format!("保存失败: {error}"));
    }
    // 校验文件分组表结构（名称/显示名/后缀列表非空）
    if let Err(error) = validate_file_groups(&config.file_groups) {
        return save_failure(format!("保存失败: {error}"));
    }

    // 命令框外观「保存即生效」：必须在**覆盖写之前**记下旧值才能对比。
    // 字体与皮肤合并判断：二者生效条件一致，且同一张设置卡承载。
    let prev_appearance = command_box_appearance_from_file(&ctx.paths.config_file);

    if save_config_file(&config, &ctx.paths.config_file).is_err() {
        // Go: panic → 500 空 body
        return HttpReply::empty(500);
    }
    // 配置已变（含开机自启），显示态缓存失效待重查
    ctx.startup.invalidate();

    if !command_font_eq(&config.options.command_font, &prev_appearance.font)
        || config.options.command_input_skin != prev_appearance.skin
    {
        (ctx.hooks.stop_process)("KeyFlux-CommandInput.exe");
    }
    // 重启引擎；启动失败经 restartFailed 告知前端（旧前端不读该字段，向后兼容）。
    // gin.H map 的 JSON 键按字典序输出：message < restartFailed（serde_json Map
    // 默认 BTreeMap，同序）。
    let restart_failed = !(ctx.hooks.restart_engine)();

    HttpReply::json(
        200,
        marshal_go_json(&serde_json::json!({
            "message": "ok",
            "restartFailed": restart_failed,
        })),
    )
}

/// 校验失败响应：400 `{"message":"保存失败: …"}`。文案含用户输入（分组名/
/// 匹配值），必须走 gin `c.JSON` 同款 JSON 转义（含 HTML 转义口径）。
fn save_failure(message: String) -> HttpReply {
    HttpReply::json(
        400,
        marshal_go_json(&serde_json::json!({ "message": message })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试沙箱：temp 下的伪部署树（`bin/` = 进程 cwd，`data/` = ../data），
    /// 用后由调用方清理。
    struct Sandbox {
        root: std::path::PathBuf,
    }

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("kf-server-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("bin")).unwrap();
            std::fs::create_dir_all(root.join("data")).unwrap();
            Sandbox { root }
        }

        fn paths(&self) -> ServerPaths {
            // 进程 cwd = 伪 bin/；config 落在 ../data/config.json
            ServerPaths::new(&self.root.join("bin"), &self.root.join("bin"))
        }

        fn write_config(&self, body: &str) {
            std::fs::write(self.root.join("data").join("config.json"), body).unwrap();
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn counting_hooks(
        restart_result: bool,
    ) -> (Hooks, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&calls);
        let hooks = Hooks {
            restart_engine: Box::new(move || {
                sink.lock().unwrap().push("restart".into());
                restart_result
            }),
            stop_process: Box::new(|name| {
                // 不会真的调 taskkill：由 restart 的 sink 记录（stop 的调用在测试里单列）
                let _ = name;
                true
            }),
            query_startup: Box::new(|| false),
        };
        (hooks, calls)
    }

    /// GET /config：合法配置 → 200，startup 回填、空集合恒 []、keyfluxVersion 注入。
    #[test]
    fn get_config_returns_dto_with_startup_backfill() {
        let sandbox = Sandbox::new("get-ok");
        sandbox.write_config(r#"{"keymaps":[],"options":{"hideMatrix":true}}"#);
        let reply = build_get_config(&sandbox.paths(), false);
        assert_eq!(reply.status, 200);
        assert_eq!(reply.content_type, Some("application/json; charset=utf-8"));
        let body = String::from_utf8(reply.body).unwrap();
        assert!(body.contains("\"keymaps\":[]"), "{body}");
        assert!(body.contains("\"fileGroups\":[]"), "{body}");
        assert!(body.contains("\"matchTypes\":[]"), "{body}");
        assert!(body.contains("\"startup\":false"), "{body}");
        assert!(body.contains("\"hideMatrix\":true"), "{body}");
        assert!(body.contains("\"keyfluxVersion\":\"\""), "{body}");

        let reply = build_get_config(&sandbox.paths(), true);
        assert!(
            String::from_utf8(reply.body)
                .unwrap()
                .contains("\"startup\":true")
        );
    }

    /// GET /config：配置缺失/损坏 → 500 空 body（Go panic → Recovery 口径）。
    #[test]
    fn get_config_returns_500_on_broken_config() {
        let sandbox = Sandbox::new("get-bad");
        std::fs::write(sandbox.root.join("data").join("config.json"), b"{oops").unwrap();
        let reply = build_get_config(&sandbox.paths(), false);
        assert_eq!(reply.status, 500);
        assert!(reply.body.is_empty());

        let sandbox = Sandbox::new("get-missing");
        let reply = build_get_config(&sandbox.paths(), false);
        assert_eq!(reply.status, 500);
    }

    /// PUT /config：校验失败 → 400 `{"message":"保存失败: …"}`，且**不落盘**。
    #[test]
    fn put_config_rejects_invalid_file_groups_with_400() {
        let sandbox = Sandbox::new("put-bad-groups");
        sandbox.write_config(r#"{"keymaps":[]}"#);
        let (hooks, calls) = counting_hooks(true);
        let ctx = super::super::ServerContext::with_hooks(sandbox.paths(), hooks);
        let body =
            r#"{"keymaps":[],"fileGroups":[{"name":"img","label":"图","exts":[]}]}"#.as_bytes();
        let reply = super::put_config(&ctx, body);
        assert_eq!(reply.status, 400);
        assert_eq!(
            String::from_utf8(reply.body).unwrap(),
            "{\"message\":\"保存失败: 文件分组「img」的后缀列表 (exts) 为空\"}"
        );
        // 校验失败先于任何文件写/进程副作用
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(sandbox.root.join("data").join("config.json")).unwrap(),
            r#"{"keymaps":[]}"#
        );
    }

    /// PUT /config：selectedAction 组合非法（重复 mapping）→ 400。
    #[test]
    fn put_config_rejects_duplicate_mapping_with_400() {
        let sandbox = Sandbox::new("put-bad-sa");
        sandbox.write_config(r#"{"keymaps":[]}"#);
        let (hooks, _) = counting_hooks(true);
        let ctx = super::super::ServerContext::with_hooks(sandbox.paths(), hooks);
        let body = br#"{"keymaps":[],"selectedAction":{"hotkey":"^!s","enable":false,"mappings":[
            {"matchType":"textType","matchValue":"url","entries":[{"behavior":"copy","options":{
            "copyToClipboard":false,"clearSelection":false,"confirm":false}}]},
            {"matchType":"texttype","matchValue":" URL ","entries":[{"behavior":"copy","options":{
            "copyToClipboard":false,"clearSelection":false,"confirm":false}}]}]}}"#;
        let reply = super::put_config(&ctx, body);
        assert_eq!(reply.status, 400);
        // Go 文案取**当前（重复的第二个）**mapping 的 matchType/matchValue 原文：
        // "texttype" 不等于内置名 "textType" ⇒ matchTypeName 回退「文件后缀」
        assert_eq!(
            String::from_utf8(reply.body).unwrap(),
            "{\"message\":\"保存失败: 文件后缀「 URL 」的映射条件重复, 请合并为同一映射\"}"
        );
    }

    /// PUT /config：合法保存 → 200 ok + 落盘（不转义 HTML）+ 缓存失效 + 重启；
    /// 外观未变不结束命令框。
    #[test]
    fn put_config_saves_and_restarts_without_touching_command_input() {
        let sandbox = Sandbox::new("put-ok");
        sandbox.write_config(r#"{"keymaps":[],"options":{"commandFont":{"sourcePath":"C:/old.ttf","weight":"regular"}}}"#);
        let (hooks, calls) = counting_hooks(true);
        let ctx = super::super::ServerContext::with_hooks(sandbox.paths(), hooks);
        // 预热缓存，之后应被保存动作失效
        let _ = ctx.startup.get(&ctx.hooks);
        let body = r#"{"keymaps":[],"options":{"commandFont":{"sourcePath":"C:/old.ttf","weight":"regular"}},"fileGroups":[{"name":"img","label":"图","exts":["jpg"]}]}"#.as_bytes();
        let reply = super::put_config(&ctx, body);
        assert_eq!(reply.status, 200);
        assert_eq!(
            String::from_utf8(reply.body).unwrap(),
            "{\"message\":\"ok\",\"restartFailed\":false}"
        );
        assert_eq!(calls.lock().unwrap().len(), 1, "应恰好重启引擎一次");

        // 落盘内容：SaveConfigFile 口径（2 空格缩进、不转义 HTML、尾换行）
        let saved = std::fs::read_to_string(sandbox.root.join("data").join("config.json")).unwrap();
        assert!(saved.contains("\"fileGroups\""), "{saved}");
        assert!(
            saved.contains("\"name\" : \"img\"") || saved.contains("\"name\": \"img\""),
            "{saved}"
        );

        // 缓存已被保存动作失效 ⇒ 下次 get 走同步重查（此处桩恒 false）
        assert!(!ctx.startup.get(&ctx.hooks));
    }

    /// PUT /config：字体外观变化 → 结束命令框进程；皮肤变化同理（合并判断）。
    #[test]
    fn put_config_stops_command_input_when_appearance_changes() {
        let sandbox = Sandbox::new("put-appearance");
        sandbox.write_config(r#"{"keymaps":[]}"#);
        let stops = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&stops);
        let hooks = Hooks {
            restart_engine: Box::new(|| true),
            stop_process: Box::new(move |name| {
                sink.lock().unwrap().push(name.to_string());
                true
            }),
            query_startup: Box::new(|| false),
        };
        let ctx = super::super::ServerContext::with_hooks(sandbox.paths(), hooks);
        let body =
            r##"{"keymaps":[],"options":{"commandInputSkin":{"backgroundColor":"#000000"}}}"##
                .as_bytes();
        let reply = super::put_config(&ctx, body);
        assert_eq!(reply.status, 200);
        assert_eq!(
            *stops.lock().unwrap(),
            vec!["KeyFlux-CommandInput.exe".to_string()]
        );
    }

    /// PUT /config：绑定失败（类型不匹配）→ 500 空 body（Go panic → Recovery）。
    #[test]
    fn put_config_returns_500_on_bind_failure() {
        let sandbox = Sandbox::new("put-bind");
        let (hooks, _) = counting_hooks(true);
        let ctx = super::super::ServerContext::with_hooks(sandbox.paths(), hooks);
        let reply = super::put_config(&ctx, br#"{"keymaps":"oops"}"#);
        assert_eq!(reply.status, 500);
        assert!(reply.body.is_empty());
    }

    /// 外观探针：读旧值生效；文件缺失 → 零值（判成「变了」的保守口径）。
    #[test]
    fn appearance_probe_reads_file_or_defaults_to_zero() {
        let sandbox = Sandbox::new("appearance");
        let path = sandbox.root.join("data").join("config.json");
        sandbox.write_config(
            r##"{"keymaps":[],"options":{"commandFont":{"sourcePath":"C:/f.ttf","weight":"bold"},
                "commandInputSkin":{"backgroundColor":"#123456"}}}"##,
        );
        let appearance = command_box_appearance_from_file(&path);
        assert_eq!(appearance.font.source_path, "C:/f.ttf");
        assert_eq!(appearance.font.weight, "bold");
        assert_eq!(appearance.skin.background_color, "#123456");

        let appearance =
            command_box_appearance_from_file(&sandbox.root.join("data").join("nope.json"));
        assert_eq!(appearance.font.source_path, "");
        assert_eq!(appearance.skin.background_color, "");
    }

    /// 开机自启缓存：TTL 内不重查、invalidate 后重查。
    #[test]
    fn startup_cache_honors_ttl_and_invalidate() {
        let count = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let sink = std::sync::Arc::clone(&count);
        let hooks = Hooks {
            restart_engine: Box::new(|| true),
            stop_process: Box::new(|_| true),
            query_startup: Box::new(move || {
                *sink.lock().unwrap() += 1;
                true
            }),
        };
        let cache = StartupCache::default();
        assert!(cache.get(&hooks));
        assert!(cache.get(&hooks));
        assert_eq!(*count.lock().unwrap(), 1, "TTL 内不应重查");
        cache.invalidate();
        assert!(cache.get(&hooks));
        assert_eq!(*count.lock().unwrap(), 2, "invalidate 后应重查");
    }

    /// 配置解析经由 generator::parse_config（null 容错口径），此处冒烟确认接线。
    #[test]
    fn get_config_uses_null_tolerant_parse() {
        let sandbox = Sandbox::new("get-null");
        sandbox.write_config(
            r#"{"keymaps":null,"options":{"plugins":{"disabled":null},"quickSwitch":null}}"#,
        );
        let reply = build_get_config(&sandbox.paths(), false);
        assert_eq!(reply.status, 200);
        let body = String::from_utf8(reply.body).unwrap();
        // 旧配置缺失段 → 默认值补齐（ParseConfig 语义）+ 空集合恒 []
        assert!(body.contains("\"collectEnabled\":true"), "{body}");
        assert!(body.contains("\"pollIntervalMs\":800"), "{body}");
        assert!(body.contains("\"disabled\":[]"), "{body}");
    }
}
