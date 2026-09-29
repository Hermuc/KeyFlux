//! 插件设置存储 —— Go `internal/plugins/store.go`（233 行）的移植。
//!
//! 文件形态（引擎侧 ConfigProvider.ahk 读同一份）：`{"<pluginId>:<key>": "<value>"}`
//! 扁平键值，值一律字符串。两个必须守住的细节（store.go 文件头）：
//! 1. **关闭 HTML 转义** —— AHK `_Unescape` 只认 `\\ \" \n \r \t` 五种序列，
//!    `\u003c` 会原样留在值里（路径里出现 `<`/`&` 的概率不为零）；
//! 2. **原子落盘** —— 临时文件 + rename，AHK 每次触发全量读，非原子写会读到
//!    半截 JSON 丢掉全部设置。
//!
//! 序列化口径：2 空格缩进 + 尾换行（Encoder.Encode 语义）；键按字典序输出；
//! 空表特判为 `{}` + `\n`（Go `Encoder` 对 nil map 输出 `null`，源码刻意绕开）。
//! 字符串转义自写 [`go_encode_string_no_html`]：与 Go `SetEscapeHTML(false)`
//! 逐字对齐（`\b`/`\f` → `\u0008`/`\u000c`，与 serde_json 的 `\b`/`\f` 不同）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Go `plugins.SettingsFileName`：与 ConfigProvider.ahk 约定一致。
pub(crate) const SETTINGS_FILE_NAME: &str = "plugin-settings.json";

/// Go `plugins.Key`：该插件名下的完整存储键 `"<pluginId>:<key>"`。
pub(crate) fn store_key(plugin_id: &str, key: &str) -> String {
    format!("{plugin_id}:{key}")
}

/// Go `splitKey`：拆出键的属主插件 ID；无冒号（外部手写）视为无主 ⇒ 空串。
fn split_key(full: &str) -> &str {
    match full.find(':') {
        Some(index) if index > 0 => &full[..index],
        _ => "",
    }
}

/// Go `marshalStringNoHTML`：单字符串 JSON 编码且不转义 `< > &`。
/// 与 Go `encoding/json` 的字符串转义逐字对齐：`\"` `\\` `\n` `\r` `\t` 原样
/// 具名，其余 <0x20 走 `\u00xx`（**小写**十六进制）；`\b`/`\f` 不用具名形式
/// （Go 无此二者的 case）；U+2028/2029 仅 escapeHTML=true 时转义（此处关闭）。
pub(crate) fn go_encode_string_no_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    // Rust 字符串恒为合法 UTF-8：按字符迭代与 Go 的字节扫描在合法 UTF-8 上等价
    // （多字节序列原样输出；Go 对非法 UTF-8 的 \ufffd 兜底在 Rust 侧不可达）。
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Go `plugins.SettingsStore`（Path + Mutex）。Rust 侧锁与路径同置，单测可注入。
pub(crate) struct SettingsStore {
    path: PathBuf,
    mutex: std::sync::Mutex<()>,
}

impl SettingsStore {
    /// Go `NewSettingsStore`：路径由调用方给出（便于测试注入临时目录）。
    pub(crate) fn new(path: PathBuf) -> Self {
        SettingsStore {
            path,
            mutex: std::sync::Mutex::new(()),
        }
    }

    /// Go `(*SettingsStore).LoadFor`：读该插件名下的设置（不带前缀）。
    /// 文件不存在 / 读失败 / JSON 坏 ⇒ 空表（不抛错，设置损坏不让插件页打不开）。
    pub(crate) fn load_for(&self, plugin_id: &str) -> BTreeMap<String, String> {
        let raw = self.read_all();
        let mut out = BTreeMap::new();
        for (full, value) in raw {
            if split_key(&full) != plugin_id {
                continue;
            }
            // 非字符串值 = 外人不按约定写的, 忽略而非报错
            match serde_json::from_slice::<String>(&value) {
                Ok(v) => {
                    out.insert(full[plugin_id.len() + 1..].to_string(), v);
                }
                Err(_) => continue,
            }
        }
        out
    }

    /// Go `(*SettingsStore).Save`：只覆盖 values 里出现的键（未提及的原样保留，
    /// 含其它插件的键）；值为空串 = 删除该键；合并在锁内完成（读-改-写）。
    pub(crate) fn save(
        &self,
        plugin_id: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        if !is_valid_plugin_id(plugin_id) {
            return Err(format!("插件 ID {plugin_id:?} 不合法"));
        }
        let _guard = self.mutex.lock().expect("settings 锁不应中毒");
        let mut raw = self.read_all();
        for (key, value) in values {
            let full = store_key(plugin_id, key);
            if value.is_empty() {
                raw.remove(&full);
                continue;
            }
            raw.insert(full, go_encode_string_no_html(value).into_bytes());
        }
        self.write_all(&raw)
    }

    /// Go `readAll`：读出全部原始键值（值保留 JSON 原文）。顶层必须是对象，
    /// 数组/标量等异形内容一律忽略（不原地清零）。
    fn read_all(&self) -> BTreeMap<String, Vec<u8>> {
        let Ok(data) = std::fs::read(&self.path) else {
            return BTreeMap::new();
        };
        let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&data); // 防手写文件带 BOM
        if data.iter().all(|b| b.is_ascii_whitespace()) {
            return BTreeMap::new();
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
            return BTreeMap::new();
        };
        let Some(map) = value.as_object() else {
            return BTreeMap::new(); // null/数组/标量 ⇒ 空（Go Unmarshal 到 map 失败/置 nil 同形）
        };
        map.iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    serde_json::to_vec(v).expect("Value 再序列化不应失败"),
                )
            })
            .collect()
    }

    /// Go `writeAll`：原子写入（临时文件 → rename）。键按字典序输出。
    fn write_all(&self, raw: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        if !dir.as_os_str().is_empty() && dir != Path::new(".") {
            std::fs::create_dir_all(dir).map_err(|error| format!("创建设置目录失败: {error}"))?;
        }

        let mut buf = String::new();
        if raw.is_empty() {
            buf.push_str("{}\n");
        } else {
            // Encoder.SetIndent("", "  ")+Encode：紧凑 JSON 重缩进 + 尾换行
            let mut compact = String::from("{");
            for (index, (key, value)) in raw.iter().enumerate() {
                if index > 0 {
                    compact.push(',');
                }
                compact.push_str(&go_encode_string_no_html(key));
                compact.push(':');
                if value.is_empty() {
                    compact.push_str("\"\"");
                } else {
                    compact.push_str(std::str::from_utf8(value).map_err(|e| e.to_string())?);
                }
            }
            compact.push('}');
            buf.push_str(&indent_json(&compact, "  "));
            buf.push('\n');
        }

        // 临时文件 + rename（同目录，Windows MoveFileEx REPLACE_EXISTING 原子覆盖）
        let tmp = dir.join(format!(
            ".plugin-settings-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        let write = |bytes: &[u8]| -> Result<(), String> {
            use std::io::Write;
            let mut file =
                std::fs::File::create(&tmp).map_err(|e| format!("创建设置临时文件失败: {e}"))?;
            file.write_all(bytes)
                .map_err(|e| format!("写设置临时文件失败: {e}"))?;
            file.sync_all().map_err(|e| format!("刷盘失败: {e}"))?;
            Ok(())
        };
        write(buf.as_bytes())?;
        match std::fs::rename(&tmp, &self.path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&tmp); // rename 失败清理临时文件
                Err(format!("替换设置文件失败: {error}"))
            }
        }
    }
}

/// Go `json.Encoder.SetIndent` 的紧凑→缩进重排（2 空格、`: ` 分隔、数组/对象
/// 元素逐行；空对象/空数组不折行）。仅处理 store 产出的紧凑 JSON（字符串内
/// 结构字符已转义，可安全逐字节扫描）。
fn indent_json(compact: &str, pad: &str) -> String {
    let mut out = String::with_capacity(compact.len() * 2);
    let mut depth = 0usize;
    let mut in_string = false;
    let mut chars = compact.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '{' | '[' => {
                depth += 1;
                out.push(ch);
                if matches!(chars.peek(), Some('}') | Some(']')) {
                    // 空对象/空数组不折行
                    out.push(chars.next().unwrap());
                    depth -= 1;
                } else {
                    out.push('\n');
                    out.push_str(&pad.repeat(depth));
                }
            }
            '}' | ']' => {
                depth -= 1;
                out.push('\n');
                out.push_str(&pad.repeat(depth));
                out.push(ch);
            }
            ',' => {
                out.push(',');
                out.push('\n');
                out.push_str(&pad.repeat(depth));
            }
            ':' => {
                out.push_str(": ");
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Go `plugins.idPattern` `^[a-z][a-z0-9_]{0,31}$`（与 generator::plugins 同款手写）。
pub(crate) fn is_valid_plugin_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("kf-settings-{tag}-{}.json", std::process::id()))
    }

    /// 空 save：空表 → `{}\n`；写读回环；键字典序；空串删键；其他插件键保留。
    #[test]
    fn save_roundtrip_sorted_and_delete_semantics() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let store = SettingsStore::new(path.clone());

        // 空表写盘 → "{}\n"
        let mut values = BTreeMap::new();
        store.save("demo", &values).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");

        // 写两个键（乱序插入）→ 键字典序；缩进 2 空格 + 尾换行
        values.insert("zeta".into(), "值<1>&".into());
        values.insert("alpha".into(), "x\"y\n".into());
        store.save("demo", &values).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        // HTML 不转义口径 + 键字典序 + 2 空格缩进 + 尾换行
        assert!(raw.contains("\"demo:alpha\": \"x\\\"y\\n\""), "{raw}");
        assert!(raw.contains("\"demo:zeta\": \"值<1>&\""), "{raw}");
        assert!(raw.ends_with("}\n"), "{raw}");

        // LoadFor：按插件前缀过滤
        let loaded = store.load_for("demo");
        assert_eq!(loaded.get("alpha").map(String::as_str), Some("x\"y\n"));
        assert_eq!(loaded.get("zeta").map(String::as_str), Some("值<1>&"));

        // 其它插件的键不受影响；空串 = 删键
        let mut other = BTreeMap::new();
        other.insert("keep".into(), "yes".into());
        store.save("other", &other).unwrap();
        let mut empty = BTreeMap::new();
        empty.insert("alpha".into(), String::new());
        store.save("demo", &empty).unwrap();
        assert!(!store.load_for("demo").contains_key("alpha"));
        assert_eq!(
            store.load_for("other").get("keep").map(String::as_str),
            Some("yes")
        );

        let _ = std::fs::remove_file(&path);
    }

    /// 非字符串值 / 异形顶层内容：读侧忽略，不向上抛错。
    #[test]
    fn read_tolerates_foreign_and_malformed_content() {
        let path = temp_path("tolerant");
        let _ = std::fs::remove_file(&path);
        let store = SettingsStore::new(path.clone());

        // 文件不存在 → 空
        assert!(store.load_for("demo").is_empty());

        // 非字符串值忽略；无冒号键忽略
        std::fs::write(
            &path,
            br#"{"demo:a":"ok","demo:b":42,"orphan":"x","demo:c":null}"#,
        )
        .unwrap();
        let loaded = store.load_for("demo");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("a").map(String::as_str), Some("ok"));

        // JSON 坏 → 空
        std::fs::write(&path, b"{oops").unwrap();
        assert!(store.load_for("demo").is_empty());

        // 顶层是数组 → 空
        std::fs::write(&path, b"[]").unwrap();
        assert!(store.load_for("demo").is_empty());

        // BOM 头容忍
        std::fs::write(&path, b"\xEF\xBB\xBF{\"demo:a\":\"ok\"}").unwrap();
        assert_eq!(
            store.load_for("demo").get("a").map(String::as_str),
            Some("ok")
        );

        let _ = std::fs::remove_file(&path);
    }

    /// 键/属主拆分：无冒号或首字符冒号 = 无主。
    #[test]
    fn split_key_semantics() {
        assert_eq!(split_key("demo:key"), "demo");
        assert_eq!(split_key("noowner"), "");
        assert_eq!(split_key(":lead"), "");
    }

    /// Go 转义口径：`\b`/`\f` 走 \u00xx（非 serde 的 \b \f）、小写十六进制、
    /// U+2028 原样保留（SetEscapeHTML(false)）。
    #[test]
    fn string_escaping_matches_go_no_html() {
        assert_eq!(
            go_encode_string_no_html("a\u{8}b\u{c}"),
            "\"a\\u0008b\\u000c\""
        );
        assert_eq!(go_encode_string_no_html("x\u{2028}y"), "\"x\u{2028}y\"");
        assert_eq!(go_encode_string_no_html("<&>"), "\"<&>\"");
        assert_eq!(go_encode_string_no_html("q\"s\\t"), "\"q\\\"s\\\\t\"");
    }

    /// 非法插件 ID 拒绝写入（400 语义由 handler 层保证，此处只锁错误返回）。
    #[test]
    fn save_rejects_invalid_plugin_id() {
        let path = temp_path("badid");
        let _ = std::fs::remove_file(&path);
        let store = SettingsStore::new(path);
        assert!(store.save("Bad-Id", &BTreeMap::new()).is_err());
    }
}
