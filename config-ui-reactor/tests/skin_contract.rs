//! 命令框皮肤契约的**跨 crate 对账**（模块化审查 2026-10-08 §5.3 / 问题 #5）。
//!
//! ## 为什么是「对账测试」而不是「抽共享 crate」
//!
//! 审查报告的修法建议是「新建 `crates/keyflux-contract` 共享 18 键名 + 默认值」。
//! 落地勘察否掉了它（证据见下），故取报告并列的退让方案：**跨 crate 契约测试**。
//!
//! 1. **共享 crate 消不掉这里的重复**：18 键名在两侧都必须以**字面量**出现 ——
//!    `models::CommandInputSkin` 的 `#[serde(rename = "…")]` 与
//!    `command-input::skin::Skin::apply` 的 `match` 臂都无法由外部常量生成
//!    （除非把 DTO 改成 `BTreeMap<String, String>`，那是改 wire 契约的高风险改动）。
//!    共享 crate 顶多提供一份「键名数组」供断言用 —— 那正是本文件的第二条断言。
//! 2. **报告提到的「window 类名 / 互斥体名常量」并不跨 crate 重复**：
//!    `CLASS_NAME` / `WINDOW_TITLE` / `MUTEX_NAME` 只存在于
//!    `command-input/src/config.rs`（单点定义，且已有逐字符单测锁定），
//!    reactor 侧只有 `KeyFlux-CommandInput.exe` 这个**进程名**（不是窗口标识）。
//!    共享 crate 因此没有第二个真实收益点。
//! 3. **真正会漂移的是「四处真源」**（本文件的对账对象）：
//!
//! | # | 位置 | 承载什么 |
//! |---|---|---|
//! | 1 | `command-input/src/skin.rs` | 键名（`Skin` 字段序 + `apply` 臂）+ 默认值 `DEFAULT` |
//! | 2 | `config-ui-reactor/src/models/config.rs` | wire 键名与顺序（`rename_all = "camelCase"`） |
//! | 3 | `config-ui-reactor/src/generator/config.rs` | 默认值字面量 `default_command_input_skin()` |
//! | 4 | `templates/CommandInputSkin.tmpl` | 键名顺序 + `else` 兜底字面量（规格文档） |
//!
//! ## 为什么现在必须有它
//!
//! `generator/config.rs:26` 与模板头注释都写着「字面量必须一致，**Go 侧有
//! `skin_defaults_test.go` 逐字段守护**」。Go 后端已于 2026-10-06 退役（36ccb83），
//! 那份守护**随之消失**，两处注释成了失真的引用 —— 于是 4 份手抄的表在 18 字段上
//! **零自动化守护**。本文件把那份守护在 Rust 侧重建，并把面从「默认值」扩到
//! 「键名集合 + 顺序 + `apply` 覆盖 + 默认值」。
//!
//! 漂移后果不对称，故值得守：键名漂移 ⇒ 用户改皮肤时**该键静默失效**
//! （`apply` 对未知键 fail-safe 忽略、`parse` 永不报错）；默认值漂移 ⇒
//! 面板显示值与命令框实际生效值不一致（用户在 UI 上看到的不是真实值）。
//!
//! ## 断言清单
//!
//! * `skin_key_order_is_identical_across_four_sources` —— 四处键名**顺序**全等；
//! * `consumer_applies_every_key_it_declares` —— `apply()` 臂集合 == 字段集合
//!   （漏一个 = 该键静默失效）；
//! * `skin_defaults_agree_across_consumer_generator_and_template` —— 18 键默认值
//!   四处全等（颜色按 `#RRGGBB` 规范化、数值按 f64 规范化，容忍 `3` 与 `3.0` 的书写差）；
//! * `parsers_see_the_full_contract_surface` —— 解析器自检（防静默退化：断言数条数）。

use std::collections::{BTreeMap, BTreeSet};

use config_ui_reactor::generator::config::default_command_input_skin;
use config_ui_reactor::generator::model::Config;
use config_ui_reactor::generator::template::render_command_input_skin;
use config_ui_reactor::models::config::CommandInputSkin;

/// 消费端源码：`command-input` 是与本 crate **无依赖、无共享 workspace** 的独立 crate，
/// 故只能按源码文本对账（这也是「跨 crate」的含义）。
const CONSUMER_SKIN_RS: &str = include_str!("../../command-input/src/skin.rs");
/// 皮肤模板 —— 真源是仓库根 `templates/`（`make sync-templates` 单向复制到 `bin/templates`，
/// 后者是未跟踪的构建产物）。本文件只对根副本；镜像一致性由 `make sync-templates` 保证。
/// 注：`settings.exe` 按**模板文件名**分派到硬编码渲染器（`generator/template.rs`），
/// 不解析本文件内容 ⇒ 该文件现在是规格文档，本测试就是它的可执行化。
const SKIN_TMPL: &str = include_str!("../../templates/CommandInputSkin.tmpl");

/// 皮肤契约字段数（`Skin` / `CommandInputSkin` / 模板行数三者恒等）。
const KEY_COUNT: usize = 18;

// ---------------------------------------------------------------------------
// 通用小工具
// ---------------------------------------------------------------------------

/// 蛇形标识符 → 皮肤文件键名（camelCase）。
fn camel_case(ident: &str) -> String {
    let mut out = String::with_capacity(ident.len());
    let mut upper_next = false;
    for ch in ident.chars() {
        if ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// 数值默认值的规范化文本：统一走 f64，使 `3` / `3.0` / `700` / `700.0` 视为相等。
/// （两侧书写风格本就不同：消费端是 `f64` 字面量，生成端是用户可见的字符串。）
fn canonical_number(text: &str) -> String {
    let value: f64 = text
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("皮肤默认值应为 f64 字面量，实际是 {text:?}"));
    format!("{value}")
}

/// 字符串形式的默认值规范化：颜色统一大写 `#RRGGBB`，数值走 [`canonical_number`]。
fn normalize_text_value(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('#') {
        return trimmed.to_ascii_uppercase();
    }
    canonical_number(trimmed)
}

/// 剥掉 `//` 行注释后统计大括号净增量（注释里出现 `{` 不应影响配平）。
fn brace_delta(line: &str) -> i32 {
    let code = match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    };
    let mut depth = 0;
    for ch in code.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// 取 `header` 所在行之后、到**同层**收尾 `}` 为止的文本块（按大括号深度配平）。
///
/// 必须是深度配平而非「首个 `}` 行」：`apply()` 的 `match` 臂内嵌 `if let { … }`，
/// 按首个 `}` 会停在第一个内层块上（该坑在编写本文件时实测踩过一次）。
fn brace_body(src: &str, header: &str) -> String {
    let mut out = String::new();
    let mut started = false;
    let mut depth = 0;
    for line in src.lines() {
        if !started {
            if line.contains(header) {
                started = true;
                depth = brace_delta(line);
            }
            continue;
        }
        depth += brace_delta(line);
        if depth <= 0 {
            return out;
        }
        out.push_str(line);
        out.push('\n');
    }
    assert!(
        started,
        "对账器找不到标记 {header:?} —— 消费端源码结构变了，请同步更新 tests/skin_contract.rs"
    );
    out
}

// ---------------------------------------------------------------------------
// 真源 1：command-input/src/skin.rs（消费端）
// ---------------------------------------------------------------------------

/// `pub struct Skin { … }` 的字段声明序 → camelCase 键名。
fn consumer_struct_fields() -> Vec<String> {
    brace_body(CONSUMER_SKIN_RS, "pub struct Skin {")
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub ")?;
            let (ident, _) = rest.split_once(':')?;
            Some(camel_case(ident.trim()))
        })
        .collect()
}

/// `impl Skin { fn apply }` 里 `"键名" => { … }` 的键字面量（出现序）。
fn consumer_apply_keys() -> Vec<String> {
    brace_body(CONSUMER_SKIN_RS, "fn apply(&mut self")
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('"') || !trimmed.contains("=>") {
                return None;
            }
            let (key, _) = trimmed[1..].split_once('"')?;
            Some(key.to_string())
        })
        .collect()
}

/// `pub const DEFAULT: Skin = Skin { … }` 的 (键名, 规范化默认值)。
fn consumer_defaults() -> BTreeMap<String, String> {
    brace_body(CONSUMER_SKIN_RS, "pub const DEFAULT: Skin = Skin {")
        .lines()
        .filter_map(|line| {
            let entry = line.trim().trim_end_matches(',');
            let (ident, expr) = entry.split_once(':')?;
            Some((
                camel_case(ident.trim()),
                normalize_consumer_value(expr.trim()),
            ))
        })
        .collect()
}

/// 消费端默认值字面量 → 规范化文本（`Rgb(0xFF, 0xFF, 0xFF)` ⇒ `#FFFFFF`）。
fn normalize_consumer_value(expr: &str) -> String {
    if let Some(inner) = expr.strip_prefix("Rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<u8> = inner.split(',').map(parse_rgb_part).collect();
        assert_eq!(parts.len(), 3, "Rgb 应有 3 个分量，实际 {expr:?}");
        return format!("#{:02X}{:02X}{:02X}", parts[0], parts[1], parts[2]);
    }
    canonical_number(expr)
}

fn parse_rgb_part(part: &str) -> u8 {
    let trimmed = part.trim();
    let result = match trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        Some(hex) => u8::from_str_radix(hex, 16),
        None => trimmed.parse(),
    };
    result.unwrap_or_else(|_| panic!("Rgb 分量应为 0x?? 或十进制字节，实际是 {part:?}"))
}

// ---------------------------------------------------------------------------
// 真源 2：models/config.rs（走真实 serde，不解析源码）
// ---------------------------------------------------------------------------

/// `models::CommandInputSkin` 的 **wire 键发射序**。
///
/// 不用 `serde_json::to_value` + 遍历 Map：`serde_json::Map` 默认是 `BTreeMap`，
/// 会把键按字典序重排 —— 顺序信息在那里丢失（`models/mod.rs` 已有的 18 字段断言
/// 只数个数，因此看不见顺序漂移）。改为序列化成**文本**后按发射序取顶层键。
fn models_wire_order() -> Vec<String> {
    let json = serde_json::to_string(&CommandInputSkin::default()).expect("skin → json");
    json_top_level_keys(&json)
}

/// 取扁平平铺 JSON 对象的顶层键（按文本出现序）。值恒为字符串、无嵌套。
fn json_top_level_keys(json: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut current = String::new();
    let mut expecting_key = false;

    for ch in json.chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
                if depth == 1 && expecting_key {
                    keys.push(std::mem::take(&mut current));
                    expecting_key = false;
                }
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.clear();
            }
            '{' => {
                depth += 1;
                expecting_key = depth == 1;
            }
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 1 => expecting_key = true,
            _ => {}
        }
    }
    keys
}

// ---------------------------------------------------------------------------
// 真源 3：生成端默认值（调用真实函数）
// ---------------------------------------------------------------------------

/// `default_command_input_skin()` 的 (wire 键, 规范化默认值)。
///
/// 直接**调用**函数而非解析 `generator/config.rs` 源码：少一处解析器、且对账的是
/// 运行期真值。（键序由 [`models_wire_order`] 与模板共同锁定，此处只需逐键取值。）
fn generator_defaults() -> BTreeMap<String, String> {
    let value = serde_json::to_value(default_command_input_skin()).expect("skin → json value");
    let object = value
        .as_object()
        .expect("CommandInputSkin 应序列化为 JSON 对象");
    object
        .iter()
        .map(|(key, raw)| {
            let text = raw.as_str().expect("18 字段均为字符串");
            (key.clone(), normalize_text_value(text))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 真源 4：渲染端 + 模板
// ---------------------------------------------------------------------------

/// 皮肤**全空**时 `render_command_input_skin` 的输出行 —— 即模板 `else` 兜底字面量。
///
/// 调真实渲染器（而非解析 `generator/template.rs` 里那份 18 元组字面量表）：
/// 渲染器的元组表既决定键序也决定兜底值，用它的**输出**做对账即同时锁定两者。
fn rendered_default_lines() -> Vec<(String, String)> {
    let rendered = render_command_input_skin(&Config::default());
    rendered
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), normalize_text_value(value.trim())))
        })
        .collect()
}

/// `templates/CommandInputSkin.tmpl` 的 (键, `{{ else }}` 兜底字面量)，按文件顺序。
fn template_default_lines() -> Vec<(String, String)> {
    SKIN_TMPL
        .lines()
        .filter(|line| line.contains("{{ else }}"))
        .filter_map(|line| {
            let (key, _) = line.split_once('=')?;
            let after_else = line.split_once("{{ else }}")?.1;
            let (default, _) = after_else.split_once("{{ end }}")?;
            Some((key.trim().to_string(), normalize_text_value(default.trim())))
        })
        .collect()
}

fn order_of(pairs: &[(String, String)]) -> Vec<String> {
    pairs.iter().map(|(key, _)| key.clone()).collect()
}

// ---------------------------------------------------------------------------
// 断言
// ---------------------------------------------------------------------------

/// 四处真源的键名**顺序**必须完全相同。
///
/// 顺序不是审美问题：`render_command_input_skin` 按元组表顺序逐行输出
/// `CommandInputSkin.txt`，用户手里的皮肤文件行序即由此决定；`models::CommandInputSkin`
/// 的字段序则决定 `GET /config` 的 `commandInputSkin` 对象键序（Go 基线逐字节比对）。
#[test]
fn skin_key_order_is_identical_across_four_sources() {
    let consumer = consumer_struct_fields();
    let models = models_wire_order();
    let rendered = order_of(&rendered_default_lines());
    let template = order_of(&template_default_lines());

    assert_eq!(
        consumer.len(),
        KEY_COUNT,
        "消费端 Skin 字段数：{consumer:?}"
    );
    assert_eq!(
        models, consumer,
        "models::CommandInputSkin 的 wire 键序与命令框 Skin 字段序不一致\n\
         消费端（skin.rs）：{consumer:?}\nmodels（serde）：{models:?}"
    );
    assert_eq!(
        rendered, consumer,
        "渲染端键序与消费端不一致（CommandInputSkin.txt 会与皮肤解析器错位）\n\
         消费端（skin.rs）：{consumer:?}\n渲染端：{rendered:?}"
    );
    assert_eq!(
        template, consumer,
        "模板键序与消费端不一致\n消费端（skin.rs）：{consumer:?}\n模板：{template:?}"
    );
}

/// `apply()` 必须覆盖 `Skin` 声明的**每一个**键。
///
/// 少写一个臂不会有编译错误：`apply` 的兜底是 `_ => {}`（R27 fail-safe：未知键忽略），
/// 于是该键在皮肤文件里写了也**静默不生效**（用户视角 = 改皮肤没反应）。
#[test]
fn consumer_applies_every_key_it_declares() {
    let declared: BTreeSet<String> = consumer_struct_fields().into_iter().collect();
    let applied: BTreeSet<String> = consumer_apply_keys().into_iter().collect();

    assert_eq!(
        applied.len(),
        KEY_COUNT,
        "apply() 臂数应为 {KEY_COUNT}，实际 {}：{applied:?}",
        applied.len()
    );
    assert_eq!(
        applied,
        declared,
        "apply() 的键集合与 Skin 字段集合不一致 —— 差集里的键会被 fail-safe 静默忽略\n\
         已声明未实现：{:?}\n已实现未声明：{:?}",
        declared.difference(&applied).collect::<Vec<_>>(),
        applied.difference(&declared).collect::<Vec<_>>()
    );
}

/// 18 键默认值在消费端 / 生成端 / 渲染端 / 模板四处必须全等。
///
/// 这是 Go `skin_defaults_test.go`（已随 Go 后端退役消失）的 Rust 重建，并把面扩到
/// 命令框侧：生成端默认值是**用户可见值**（面板上写的就是它），消费端默认值是
/// **命令框空皮肤时真正生效的值** —— 两者不等即「面板显示 ≠ 实际生效」。
///
/// 颜色按 `#RRGGBB` 规范化后比较（消费端是 `Rgb`、其余是字符串），
/// 数值按 f64 规范化（容忍 `3` vs `3.0`、`700` vs `700.0` 的书写差）。
#[test]
fn skin_defaults_agree_across_consumer_generator_and_template() {
    let consumer = consumer_defaults();
    let generator = generator_defaults();
    let rendered: BTreeMap<String, String> = rendered_default_lines().into_iter().collect();
    let template: BTreeMap<String, String> = template_default_lines().into_iter().collect();

    assert_eq!(consumer.len(), KEY_COUNT, "消费端 DEFAULT 条目数");
    for (label, map) in [
        ("生成端 default_command_input_skin", &generator),
        ("渲染端 render_command_input_skin", &rendered),
        ("模板 CommandInputSkin.tmpl", &template),
    ] {
        assert_eq!(
            map.len(),
            KEY_COUNT,
            "{label} 条目数应为 {KEY_COUNT}，实际 {}",
            map.len()
        );
        assert_eq!(
            map.keys().collect::<Vec<_>>(),
            consumer.keys().collect::<Vec<_>>(),
            "{label} 的键集合与消费端不一致"
        );
    }

    let mut drift = Vec::new();
    for (key, want) in &consumer {
        for (label, actual) in [
            ("生成端", generator.get(key)),
            ("渲染端", rendered.get(key)),
            ("模板", template.get(key)),
        ] {
            match actual {
                Some(value) if value == want => {}
                Some(value) => drift.push(format!(
                    "  {key}: {label}={value:?}，命令框 DEFAULT={want:?}"
                )),
                None => drift.push(format!("  {key}: {label}缺失")),
            }
        }
    }
    assert!(
        drift.is_empty(),
        "皮肤默认值四处漂移（面板显示值 ≠ 命令框生效值）：\n{}",
        drift.join("\n")
    );
}

/// 解析器自检：四处真源各应恰好解析出 18 项。
///
/// 防的是「解析器静默退化」—— 若某天真源的书写形态变了而解析器扫不到，
/// 前面三个测试会以「集合为空 ⇒ 相等」的方式**假绿**（空集互相相等）。
#[test]
fn parsers_see_the_full_contract_surface() {
    assert_eq!(
        consumer_struct_fields().len(),
        KEY_COUNT,
        "skin.rs struct 字段"
    );
    assert_eq!(consumer_apply_keys().len(), KEY_COUNT, "skin.rs apply 臂");
    assert_eq!(consumer_defaults().len(), KEY_COUNT, "skin.rs DEFAULT 条目");
    assert_eq!(models_wire_order().len(), KEY_COUNT, "models wire 键");
    assert_eq!(
        rendered_default_lines().len(),
        KEY_COUNT,
        "渲染端输出行（空皮肤）"
    );
    assert_eq!(template_default_lines().len(), KEY_COUNT, "模板 else 行");
    assert_eq!(
        consumer_struct_fields().len(),
        models_wire_order().len(),
        "消费端与 models 的字段数应相等"
    );
}
