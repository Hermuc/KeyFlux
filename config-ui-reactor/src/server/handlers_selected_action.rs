//! 选中动作单键分发 API —— Go `internal/server/selectedaction.go`
//! （`TestSelectedActionHandler`）、`selectedaction_play.go`
//! （`PlaySelectedActionHandler`/`resolvePlayTypeId`/`writePlayRequestFile`）与
//! `internal/script/selectedaction.go` 的 `MatchSelectedAction`、
//! `internal/script/actionscheme.go` 的匹配算子（`matchActionRule`/
//! `matchFileExt`/`matchTextType`/`matchCustomRules`/`PreviewAction`）的移植。
//!
//! 路由（server.go:47-49）：`POST /api/selected-action/test`、
//! `POST /api/selected-action/play`。
//!
//! 语义要点（Go 权威）：
//! * test：请求体携带编辑中快照（未保存也可测）；`matchTypes` 非空覆盖磁盘
//!   运行配置；`selectedAction` 为空回退磁盘配置；校验失败 → 400
//!   `{"message":…}`；未命中 → `{"matched":false}`；命中 → menu（gin.H 键
//!   字典序 behavior<key<name）+ preview（ResolveRuleAction 展开包默认模板）。
//! * play：typeId 白名单（内置文本特征 / `type:<id>` / `group:<name>` 折叠为
//!   规范化后缀串）→ 原子写 `%TEMP%\kf_play_request.json`（引擎轮询消费）→
//!   `{"ok":true}`；非法 → 400。
//! * 绑定失败 panic → gin Recovery → **500 空 body**。
//!
//! 文本特征正则与 Go `textfeatures.go` 逐字同源（RE2/`regex` crate 方言交集）。

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;

use crate::generator::config::parse_config;
use crate::generator::model::{Config, MatchRule, SelectedAction, strip_null_fields};

use super::dto::marshal_go_json;
use super::handlers_behaviors::WirePack;
use super::handlers_behaviors::behavior_validation_catalog;
use super::validate::is_custom_ref;
use super::{HttpReply, ServerContext, VERSION};

// --------------------------------------------------------------------------- 文本特征注册表（textfeatures.go）

/// Go `textFeatures` 注册表的匹配面投影（值 / 具名正则 / 兜底标记）；
/// 顺序即行序判定顺序（兜底 plain 恒居末）。
struct FeatureSpec {
    value: &'static str,
    pattern: Option<&'static str>,
    ignore_case: bool,
    fallback: bool,
}

const FEATURES: [FeatureSpec; 5] = [
    FeatureSpec {
        value: "url",
        pattern: Some(r"^(https?|ftp)://"),
        ignore_case: true,
        fallback: false,
    },
    FeatureSpec {
        value: "path",
        pattern: Some(r"^(\\\\[^\\]+\\[^\\]+|[a-zA-Z]:\\)"),
        ignore_case: false,
        fallback: false,
    },
    FeatureSpec {
        value: "magnet",
        pattern: Some(r"^magnet:"),
        ignore_case: true,
        fallback: false,
    },
    FeatureSpec {
        value: "bilibili",
        pattern: Some(r"^(av[0-9]+|bv[0-9a-z]{10})\z"),
        ignore_case: true,
        fallback: false,
    },
    FeatureSpec {
        value: "plain",
        pattern: None,
        ignore_case: false,
        fallback: true,
    },
];

fn compiled_features() -> &'static Vec<(regex::Regex, usize)> {
    static COMPILED: OnceLock<Vec<(regex::Regex, usize)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        FEATURES
            .iter()
            .enumerate()
            .filter(|(index, spec)| {
                let _ = index;
                spec.pattern.is_some()
            })
            .map(|(index, spec)| {
                let mut builder =
                    regex::RegexBuilder::new(spec.pattern.expect("具名特征必有 Pattern"));
                builder.case_insensitive(spec.ignore_case);
                (builder.build().expect("内置正则不应编译失败"), index)
            })
            .collect()
    })
}

/// Go `behaviors.TextFeatureHit`：具名特征锚定正则命中；兜底特征（plain）
/// 排除**全部**具名特征（派生，非硬编码 —— 编译表里只含具名特征）。
fn text_feature_hit(index: usize, content: &str) -> bool {
    if !FEATURES[index].fallback {
        return compiled_features()
            .iter()
            .find(|(_, i)| *i == index)
            .expect("具名特征已编译")
            .0
            .is_match(content);
    }
    !compiled_features()
        .iter()
        .any(|(re, _)| re.is_match(content))
}

/// Go `behaviors.MatchTextFeature`：特征名归一化（去空白 + 小写）；
/// content 不做 Trim（具名特征 ^ 锚定，值可能被拼进 URL）；未知特征 false。
fn match_text_feature(value: &str, content: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    let Some(index) = FEATURES.iter().position(|spec| spec.value == normalized) else {
        return false;
    };
    text_feature_hit(index, content)
}

// --------------------------------------------------------------------------- 匹配算子（actionscheme.go）

/// Go `asciiFold`：仅折 ASCII 大写（非 ASCII 不折叠 —— 与 AHK AsciiLower 对齐）。
fn ascii_fold(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                (c as u8 + 32) as char
            } else {
                c
            }
        })
        .collect()
}

/// Go `firstNonEmptyLine`：Trim(content) 的首个非空行。
fn first_non_empty_line(content: &str) -> String {
    content
        .split('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Go `matchRuleOp`：equals/prefix 作用于首个非空行；suffix/contains 作用于
/// 整体 Trim(content)。
fn match_rule_op(op: &str, value: &str, content: &str) -> bool {
    let trimmed = content.trim();
    let first = first_non_empty_line(trimmed);
    let folded_value = ascii_fold(value);
    match op {
        "equals" => ascii_fold(&first) == folded_value,
        "prefix" => ascii_fold(&first).starts_with(&folded_value),
        "suffix" => ascii_fold(trimmed).ends_with(&folded_value),
        "contains" => ascii_fold(trimmed).contains(&folded_value),
        _ => false,
    }
}

/// Go `matchCustomRules`：OR 求值。
fn match_custom_rules(rules: &[MatchRule], content: &str) -> bool {
    rules
        .iter()
        .any(|rule| match_rule_op(&rule.op, &rule.value, content))
}

/// Go `fileExt`：取文件后缀（含点），无后缀返回空串。
fn file_ext(path: &str) -> &str {
    let path = path.trim();
    let Some(index) = path.rfind('.') else {
        return "";
    };
    if index == path.len() - 1 {
        return "";
    }
    &path[index..]
}

/// Go `matchFileExtList`：数组形后缀匹配（去点/忽略大小写/`*` 任意/空扩展名跳过）。
fn match_file_ext_list(exts: &[String], content: &str) -> bool {
    for line in content.split('\n') {
        let ext = file_ext(line).strip_prefix('.').unwrap_or(file_ext(line));
        if ext.is_empty() {
            continue;
        }
        for value in exts {
            let value = value.trim().strip_prefix('.').unwrap_or(value.trim());
            if value == "*" || value.eq_ignore_ascii_case(ext) {
                return true;
            }
        }
    }
    false
}

/// Go `matchFileExt`：逗号分隔多后缀，`*` 精确匹配任意。
fn match_file_ext(match_value: &str, content: &str) -> bool {
    if match_value == "*" {
        return true;
    }
    for line in content.split('\n') {
        let ext = file_ext(line).strip_prefix('.').unwrap_or(file_ext(line));
        if ext.is_empty() {
            continue;
        }
        for value in match_value.split(',') {
            let value = value.trim().strip_prefix('.').unwrap_or(value.trim());
            if value == "*" || value.eq_ignore_ascii_case(ext) {
                return true;
            }
        }
    }
    false
}

/// Go `matchActionRule`：`type:` 引用内部解析（matchTypes kind=fileExt 优先，
/// 再退 fileGroups —— 两者共用同一 type: 命名空间）。
fn match_action_rule(
    cfg: &Config,
    match_type: &str,
    match_value: &str,
    is_file: bool,
    content: &str,
) -> bool {
    match match_type {
        "fileExt" => {
            if !is_file {
                return false;
            }
            if is_custom_ref(match_value) {
                let id = match_value.strip_prefix("type:").unwrap_or("");
                if let Some(mt) = cfg.find_match_type(id)
                    && mt.kind == "fileExt"
                {
                    return match_file_ext_list(&mt.exts, content);
                }
                let Some(exts) = cfg.file_group_exts(id) else {
                    return false;
                };
                return match_file_ext_list(exts, content);
            }
            match_file_ext(match_value, content)
        }
        "textType" => {
            if is_file {
                return false;
            }
            if is_custom_ref(match_value) {
                let id = match_value.strip_prefix("type:").unwrap_or("");
                let Some(mt) = cfg.find_match_type(id) else {
                    return false;
                };
                if mt.kind != "text" {
                    return false;
                }
                return match_custom_rules(&mt.rules, content);
            }
            match_text_feature(match_value, content)
        }
        _ => false,
    }
}

/// Go `MatchSelectedAction`：按 mappings 顺序取首个命中。
fn match_selected_action<'a>(
    sa: &'a SelectedAction,
    is_file: bool,
    content: &str,
    cfg: &Config,
) -> Option<&'a crate::generator::model::SelectedMapping> {
    sa.mappings.iter().find(|mapping| {
        match_action_rule(
            cfg,
            &mapping.match_type,
            &mapping.match_value,
            is_file,
            content,
        )
    })
}

// --------------------------------------------------------------------------- 预览（script.PreviewAction）

/// Go `urlEncode`：非保留字符原样，其余按字节百分号编码（大写十六进制）。
fn url_encode(s: &str) -> String {
    let mut buf = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => buf.push(*b as char),
            _ => buf.push_str(&format!("%{b:02X}")),
        }
    }
    buf
}

/// Go `script.PreviewAction`：执行预览（%selected% 替换语义与内置动作文案逐字）。
fn preview_action(action_type: &str, action_value: &str, content: &str) -> String {
    match action_type {
        "search" => action_value.replace("%selected%", &url_encode(content)),
        "open_url" => format!("用默认浏览器打开: {content}"),
        "open_path" => format!("按系统关联程序打开: {content}"),
        "open_folder" => "打开选中路径所在文件夹".to_string(),
        "magnet_download" => format!("用默认 BT 下载工具下载: {content}"),
        _ => action_value.replace("%selected%", content),
    }
}

/// Go `behaviors.ResolveRuleAction`：内置动作直通；用户包 builtin entry 展开
/// 为基础动作 + 包默认模板（规则的空值被包模板补齐）。
fn resolve_rule_action(
    catalog: &[WirePack],
    action_type: &str,
    action_value: &str,
    working_dir: &str,
) -> (String, String, String) {
    if super::validate::is_builtin_action(action_type) {
        return (
            action_type.to_string(),
            action_value.to_string(),
            working_dir.to_string(),
        );
    }
    let Some(pack) = catalog.iter().find(|pack| pack.id == action_type) else {
        return (
            action_type.to_string(),
            action_value.to_string(),
            working_dir.to_string(),
        );
    };
    if pack.entry.kind != "builtin" {
        return (
            action_type.to_string(),
            action_value.to_string(),
            working_dir.to_string(),
        );
    }
    let action_value = if action_value.is_empty() {
        pack.entry.params.action_value.clone()
    } else {
        action_value.to_string()
    };
    let working_dir = if working_dir.is_empty() {
        pack.entry.params.working_dir.clone()
    } else {
        working_dir.to_string()
    };
    (pack.entry.action.clone(), action_value, working_dir)
}

// --------------------------------------------------------------------------- handlers

fn json_message(status: u16, message: &str) -> HttpReply {
    HttpReply::json(
        status,
        marshal_go_json(&serde_json::json!({ "message": message })),
    )
}

/// Go `selectedActionTestRequest`。
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SelectedActionTestRequest {
    content: String,
    #[serde(rename = "isFile")]
    is_file: bool,
    #[serde(rename = "selectedAction")]
    selected_action: Option<SelectedAction>,
    #[serde(rename = "matchTypes")]
    match_types: Option<Vec<crate::generator::model::MatchType>>,
}

/// Go `TestSelectedActionHandler`：模拟测试（纯内存零副作用）。
pub(crate) fn test_selected_action(ctx: &ServerContext, body: &[u8]) -> HttpReply {
    // Go ShouldBindJSON 失败 panic → 500
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(
        body.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(body),
    ) else {
        return HttpReply::empty(500);
    };
    strip_null_fields(&mut value);
    let Ok(req) = serde_json::from_value::<SelectedActionTestRequest>(value) else {
        return HttpReply::empty(500);
    };

    // 注册表: 优先用请求体携带的未保存类型, 否则回退磁盘运行配置 (容忍缺失)
    let mut cfg = parse_config(&ctx.paths.config_file, VERSION).unwrap_or_default();
    if let Some(match_types) = req.match_types {
        cfg.match_types = match_types;
    }
    let disk_selected = cfg.selected_action.clone();
    let sa = req.selected_action.as_ref().or(disk_selected.as_ref());

    // 校验组合合法性（与保存链路同一实现）
    let catalog = behavior_validation_catalog(ctx);
    if let Err(error) = super::validate::validate_selected_action(sa, Some(&catalog), &cfg) {
        return json_message(400, &error);
    }
    let Some(sa) = sa else {
        // Go: sa == nil 时 MatchSelectedAction(nil,…) 返回 nil → {"matched":false}
        return HttpReply::json(
            200,
            marshal_go_json(&serde_json::json!({ "matched": false })),
        );
    };
    let Some(matched) = match_selected_action(sa, req.is_file, &req.content, &cfg) else {
        return HttpReply::json(
            200,
            marshal_go_json(&serde_json::json!({ "matched": false })),
        );
    };

    let full_catalog = super::handlers_behaviors::load_behavior_catalog(&ctx.paths);
    let entry_name = |id: &str| -> String {
        full_catalog
            .packs
            .iter()
            .find(|pack| pack.id == id)
            .map(|pack| pack.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let menu: Vec<serde_json::Value> = matched
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            serde_json::json!({
                "key": index + 1,
                "behavior": entry.behavior,
                "name": entry_name(&entry.behavior),
            })
        })
        .collect();
    let mut preview = String::new();
    if let Some(first) = matched.entries.first() {
        let (action_type, action_value, _working_dir) = resolve_rule_action(
            &full_catalog.packs,
            &first.behavior,
            &first.action_value,
            &first.working_dir,
        );
        preview = preview_action(&action_type, &action_value, &req.content);
    }
    HttpReply::json(
        200,
        marshal_go_json(&serde_json::json!({
            "matchType": matched.match_type,
            "matchValue": matched.match_value,
            "matched": true,
            "menu": menu,
            "preview": preview,
        })),
    )
}

// --------------------------------------------------------------------------- play

/// Go `playBuiltinTextFeatures`：内置文本特征白名单（plain 恒兜底）。
const PLAY_BUILTIN_TEXT_FEATURES: [&str; 5] = ["url", "path", "magnet", "bilibili", "plain"];

/// Go `playSeq`：每条请求单调递增（供引擎 seq 去重）。
static PLAY_SEQ: AtomicU64 = AtomicU64::new(0);

/// Go `playRequestFile`：请求文件落点（%TEMP%\kf_play_request.json）。
const PLAY_REQUEST_FILE: &str = "kf_play_request.json";

/// Go `resolvePlayTypeId`：白名单校验 + group 折叠。
///   - 内置文本特征 → 原样；"type:<id>" → 必须存在（任意 kind）→ 原样；
///   - "group:<name>" → 必须存在 → strings.Join(Exts, ",")；其余 → ""。
fn resolve_play_type_id(type_id: &str, cfg: &Config) -> String {
    if PLAY_BUILTIN_TEXT_FEATURES.contains(&type_id) {
        return type_id.to_string();
    }
    if let Some(id) = type_id.strip_prefix("type:") {
        let full = format!("type:{id}");
        if cfg
            .match_types
            .iter()
            .any(|mt| format!("type:{}", mt.id) == full)
        {
            return type_id.to_string();
        }
        return String::new();
    }
    if let Some(name) = type_id.strip_prefix("group:") {
        if let Some(group) = cfg.file_groups.iter().find(|g| g.name == name) {
            return group.exts.join(",");
        }
        return String::new();
    }
    String::new()
}

/// Go `writePlayRequestFile`：原子写请求文件（temp + rename）。
/// 内容 `{"seq":<自增>,"typeId":"…"}` —— gin.H 键字典序，不含任何命令/路径参数。
fn write_play_request_file(type_id: &str) -> Result<(), String> {
    let seq = PLAY_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let payload = marshal_go_json(&serde_json::json!({
        "seq": seq,
        "typeId": type_id,
    }));
    let dir = std::env::temp_dir();
    let final_path = dir.join(PLAY_REQUEST_FILE);
    let tmp = dir.join(format!(
        "kf_play_{}-{}.json.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    let write = || -> Result<(), String> {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(payload.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(())
    };
    write()?;
    match std::fs::rename(&tmp, &final_path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&tmp); // 失败时清理临时文件
            Err(error.to_string())
        }
    }
}

/// Go `playRequest`。
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PlayRequest {
    #[serde(rename = "typeId")]
    type_id: String,
}

/// Go `PlaySelectedActionHandler`：白名单校验 → 折叠 group → 原子写请求文件 →
/// `{"ok":true}`；非法 typeId → 400。
pub(crate) fn play_selected_action(ctx: &ServerContext, body: &[u8]) -> HttpReply {
    // Go ShouldBindJSON 失败 panic → 500
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(
        body.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(body),
    ) else {
        return HttpReply::empty(500);
    };
    strip_null_fields(&mut value);
    let Ok(req) = serde_json::from_value::<PlayRequest>(value) else {
        return HttpReply::empty(500);
    };
    let type_id = req.type_id.trim().to_string();
    if type_id.is_empty() {
        return json_message(400, "typeId 不能为空");
    }

    // 白名单校验 + group 折叠 (需读运行配置确认类型已存在; 容忍缺失)
    let cfg = parse_config(&ctx.paths.config_file, VERSION).unwrap_or_default();
    let resolved = resolve_play_type_id(&type_id, &cfg);
    if resolved.is_empty() {
        return json_message(400, &format!("非法的 typeId: {type_id}"));
    }

    match write_play_request_file(&resolved) {
        Ok(()) => HttpReply::json(200, marshal_go_json(&serde_json::json!({ "ok": true }))),
        Err(error) => json_message(500, &format!("写入请求文件失败: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 匹配算子与文本特征：Go 向量口径抽样钉死。
    #[test]
    fn text_features_and_ops_match_go() {
        assert!(match_text_feature("url", "https://x.com/a"));
        assert!(match_text_feature("URL", "HTTPS://X.COM"));
        assert!(!match_text_feature("url", "ftp.x")); // 非 url 前缀
        assert!(match_text_feature("path", r"\\server\share\x"));
        assert!(match_text_feature("path", r"C:\Windows"));
        assert!(match_text_feature("magnet", "magnet:?xt=urn:btih:x"));
        assert!(match_text_feature("bilibili", "av170001"));
        assert!(match_text_feature("bilibili", "BV1xx411c7mD"));
        assert!(match_text_feature("plain", "随便什么"));
        assert!(!match_text_feature("plain", "https://x.com")); // 具名命中 ⇒ plain 不命中
        assert!(!match_text_feature("nope", "anything"));

        assert!(match_rule_op("equals", "KFX", "KFX\nsecond"));
        assert!(match_rule_op("prefix", "KFX", "KFXdemo"));
        assert!(match_rule_op("suffix", "demo", "x\nKFXdemo "));
        assert!(match_rule_op("contains", "fx", "KFXdemo"));
        // 非 ASCII（零宽空格 U+200B）不折叠 ⇒ equals 严格不等
        assert!(!match_rule_op("equals", "kfx", "KFX\u{200B}"));

        assert!(match_file_ext("*", "a.txt"));
        assert!(match_file_ext("jpg,png", "photo.JPG\n"));
        assert!(match_file_ext(".jpg", "x.jpg"));
        assert!(!match_file_ext("jpg", "no-ext"));
    }

    /// resolvePlayTypeId：白名单 / type 引用 / group 折叠 / 拒绝。
    #[test]
    fn resolve_play_type_id_covers_whitelist_and_folding() {
        let cfg = Config {
            file_groups: vec![crate::generator::model::FileGroup {
                name: "image".into(),
                label: "图片".into(),
                exts: vec!["jpg".into(), "png".into()],
            }],
            match_types: vec![crate::generator::model::MatchType {
                id: "netdisk".into(),
                label: "网盘".into(),
                kind: "text".into(),
                rules: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(resolve_play_type_id("url", &cfg), "url");
        assert_eq!(resolve_play_type_id("plain", &cfg), "plain");
        assert_eq!(resolve_play_type_id("type:netdisk", &cfg), "type:netdisk");
        assert_eq!(resolve_play_type_id("group:image", &cfg), "jpg,png");
        assert!(resolve_play_type_id("group:missing", &cfg).is_empty());
        assert!(resolve_play_type_id("type:missing", &cfg).is_empty());
        assert!(resolve_play_type_id("notepad.exe", &cfg).is_empty());
        assert!(resolve_play_type_id("", &cfg).is_empty());
    }

    /// 预览：内置动作文案 + %selected% 替换 + search URL 编码。
    #[test]
    fn preview_action_matches_go_copies() {
        assert_eq!(
            preview_action("open_url", "", "a<b>&"),
            "用默认浏览器打开: a<b>&"
        );
        assert_eq!(
            preview_action("open_folder", "", "x"),
            "打开选中路径所在文件夹"
        );
        assert_eq!(
            preview_action("search", "https://bing.com/search?q=%selected%", "中文 x"),
            "https://bing.com/search?q=%E4%B8%AD%E6%96%87%20x"
        );
        assert_eq!(
            preview_action("run", "notepad.exe %selected%", "a.txt"),
            "notepad.exe a.txt"
        );
    }

    /// 请求文件：原子落盘、键字典序（seq < typeId）、seq 自增。
    #[test]
    fn play_request_file_is_atomic_and_sorted() {
        let path = std::env::temp_dir().join(PLAY_REQUEST_FILE);
        write_play_request_file("url").unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.starts_with("{\"seq\":"), "{first}");
        assert!(first.contains("\"typeId\":\"url\""), "{first}");
        assert!(!first.ends_with('\n'), "Marshal 无尾换行: {first:?}");
        write_play_request_file("path").unwrap();
        let second = std::fs::read_to_string(&path).unwrap();
        let seq = |text: &str| -> u64 {
            text.trim_start_matches("{\"seq\":")
                .split(',')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        assert_eq!(seq(&second), seq(&first) + 1, "seq 应单调递增");
        let _ = std::fs::remove_file(&path);
    }
}
