//! 三套配置模型的 **wire 字段契约对账**（模块化审查报告 §5.1 / 问题 #2 的护栏部分）。
//!
//! 同一份 Go 配置契约在 reactor 里有三种形态，且**每一种都自己完整定义了一遍**：
//! * `models/config.rs` —— 面板侧（`app` / `ui` / `services` 消费）；
//! * `generator/model.rs` —— 生成端（`generator` / `server` 消费）；
//! * `server/dto.rs` —— HTTP wire（`tools/api-parity/reference` 的基线快照对象）。
//!
//! 三者之间**没有任何编译期约束**：没有 `From` 桥接、没有共享 crate，只有 JSON 字符串
//! 在中间。后果是「新增一个字段要改 3 处，任一处漏改就是漂移起点」，而漂移的表现是
//! **静默丢字段**（Go/generator 全量落盘会把不认识的键剥掉 —— 见 `models/mod.rs` 头注）。
//!
//! 本模块把这种漂移从「沉默」变成「红灯」。这是那次审查里**唯一不受 api-parity 冻结
//! 限制**的动作：纯新增测试，零行为变更，不合并任何模型。
//!
//! **口径**：只比 **wire 名 × 顺序**（serde 解析后的输出键），不比 Rust 字段名与类型 ——
//! 后者允许有意差异（UI 瞬态字段、渲染期字段），逐条列在下面两张白名单里。
//!
//! 首次运行即抓到一处真实缺陷：`server/dto.rs::ActionDto.run_as_admin` 漏了
//! `rename = "runAsAdmin"`，而 `models::Action` 是 `rename_all = "camelCase"`
//! ⇒ 面板发的 `runAsAdmin` 在服务端被 serde 当未知键丢掉。既有三闸门都看不见它
//! （api-parity 基线里该键恒 false、被 omitempty 省略）。同批已修。

use std::collections::{BTreeMap, BTreeSet};

const MODELS: &str = include_str!("config.rs");
const GENERATOR: &str = include_str!("../generator/model.rs");
const DTO: &str = include_str!("../server/dto.rs");

/// 允许「只在 `generator/model.rs` 侧出现」的 **wire** 字段：类型名 -> 有序列表。
///
/// `Config.actionSchemes` 是存量迁移专用字段：`server/dto.rs` 的 `dto_to_config` 显式
/// 置空、`generator/config.rs` 主动 `clear()`，即**有意不往返**（Go 后端退役后的历史包袱，
/// 见模块化审查报告 §5.1 的「关键澄清」）。除此之外不允许任何单边 wire 字段。
const GENERATOR_ONLY_WIRE: &[(&str, &[&str])] = &[("Config", &["actionSchemes"])];

/// 允许的**非 wire**（`#[serde(skip)]`）字段差异：类型名 -> (仅 models 侧, 仅 generator 侧)。
///
/// 四者都不参与序列化，故不影响落盘契约：
/// * `is_empty` / `is_new` —— Vue 时代的编辑态哨兵（`isNew` / `isEmpty`）；
/// * `remap_in_hot_if` —— 渲染期开关（生成 hotkey 串时才用）；
/// * `key_mapping` —— 生成期映射串（Go 侧 json 标签为 `-`）。
const NON_WIRE_ONLY: &[(&str, &[&str], &[&str])] = &[
    ("Action", &["is_empty"], &["remap_in_hot_if"]),
    ("Config", &[], &["key_mapping"]),
    ("Keymap", &["is_new"], &[]),
];

/// 一个字段的 wire 视角。
#[derive(Debug)]
struct Field {
    /// Rust 标识符（只用于诊断信息）。
    rust: String,
    /// serde 解析后的输出键名。
    wire: String,
    /// `#[serde(skip)]` ⇒ 不参与序列化。
    skipped: bool,
}

/// 解析 `.rs` 源码里所有顶层 `struct` 的字段（剥掉 `mod tests` 之后的代码）。
///
/// 刻意只支持 rustfmt 排版出的**单行属性**与顶格收尾括号；遇到别处即 `panic!`
/// —— 一个静默降级的对账器比没有对账器更危险（会给出假绿灯）。
fn parse(source: &str) -> BTreeMap<String, Vec<Field>> {
    // 单测里的样例结构体会引入同名干扰，且不属于契约面。
    let source = match source.find("\nmod tests") {
        Some(index) => &source[..index],
        None => source,
    };
    let lines: Vec<&str> = source.lines().collect();
    let mut types = BTreeMap::new();
    let mut attrs: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.starts_with("//") {
            // 文档注释常夹在属性与结构体之间 ⇒ 不打断属性累积。
            index += 1;
            continue;
        }
        if let Some(body) = line.strip_prefix("#[") {
            assert!(body.ends_with(']'), "不支持多行属性（请改为单行）: {line}");
            attrs.push(body);
            index += 1;
            continue;
        }
        if let Some(name) = struct_header(line) {
            let camel = attrs
                .iter()
                .any(|a| a.contains(r#"rename_all = "camelCase""#));
            let (fields, consumed) = collect_fields(&lines[index + 1..], camel);
            types.insert(name.to_string(), fields);
            index += 1 + consumed;
            attrs.clear();
            continue;
        }
        // 其它任何一行都终止属性累积（防止把远处属性错误地挂到这个结构体上）。
        attrs.clear();
        index += 1;
    }
    types
}

fn struct_header(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("pub struct ")
        .or_else(|| line.strip_prefix("pub(crate) struct "))
        .or_else(|| line.strip_prefix("struct "))?;
    rest.strip_suffix(" {")
}

/// 返回 (字段表, 消费掉的行数 —— 含顶格收尾 `}`)。
fn collect_fields(lines: &[&str], camel: bool) -> (Vec<Field>, usize) {
    let mut fields = Vec::new();
    let mut attrs: Vec<&str> = Vec::new();
    for (offset, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line == "}" {
            return (fields, offset + 1);
        }
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(body) = line.strip_prefix("#[") {
            assert!(body.ends_with(']'), "不支持多行属性（请改为单行）: {line}");
            attrs.push(body);
            continue;
        }
        let ident = line
            .strip_prefix("pub ")
            .and_then(|rest| rest.split_once(':'))
            .map(|(name, _)| name.trim())
            .unwrap_or_else(|| panic!("无法解析的字段行: {line}"));
        let joined = attrs.join(" ");
        // `skip_serializing_if` 不是 `skip`：按标识符切词后精确比较，避免前缀误判。
        let skipped = joined
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|token| token == "skip");
        let wire = match rename_of(&joined) {
            Some(name) => name,
            None if camel => camel_case(ident),
            None => ident.to_string(),
        };
        fields.push(Field {
            rust: ident.to_string(),
            wire,
            skipped,
        });
        attrs.clear();
    }
    panic!("结构体缺少顶格收尾括号");
}

/// 取出 `rename = "xxx"` 的字面量（`rename_all = "…"` 不会被误取）。
fn rename_of(attrs: &str) -> Option<String> {
    let start = attrs.find(r#"rename = ""#)? + r#"rename = ""#.len();
    let end = attrs[start..].find('"')? + start;
    Some(attrs[start..end].to_string())
}

fn camel_case(ident: &str) -> String {
    let mut out = String::new();
    for (index, part) in ident.split('_').enumerate() {
        if index == 0 {
            out.push_str(part);
        } else {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

fn wire_names(fields: &[Field]) -> Vec<&str> {
    fields
        .iter()
        .filter(|f| !f.skipped)
        .map(|f| f.wire.as_str())
        .collect()
}

fn allowed<'a>(table: &'a [(&'a str, &'a [&'a str])], name: &str) -> &'a [&'a str] {
    table
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
        .unwrap_or(&[])
}

/// 面板侧模型与生成端模型：wire 字段名与顺序必须逐位相同（白名单外）。
#[test]
fn models_and_generator_agree_on_wire_fields() {
    let models = parse(MODELS);
    let generator = parse(GENERATOR);

    for (name, model_fields) in &models {
        let gen_fields = generator
            .get(name)
            .unwrap_or_else(|| panic!("generator/model.rs 缺少 models::{name}"));
        let extra = allowed(GENERATOR_ONLY_WIRE, name);
        let gen_wire = wire_names(gen_fields);
        let model_wire = wire_names(model_fields);

        let gen_only: Vec<&str> = gen_wire
            .iter()
            .copied()
            .filter(|wire| !model_wire.contains(wire))
            .collect();
        assert_eq!(
            gen_only, extra,
            "models::{name} / generator::model::{name} 的单边 wire 字段与白名单不符"
        );
        let stripped: Vec<&str> = gen_wire
            .iter()
            .copied()
            .filter(|wire| !extra.contains(wire))
            .collect();
        assert_eq!(
            stripped, model_wire,
            "去掉白名单键后，generator::model::{name} 的 wire 字段名与顺序必须与 models::{name} 逐位相同"
        );
    }

    let only_generator: Vec<&str> = generator
        .keys()
        .filter(|key| !models.contains_key(*key))
        .map(String::as_str)
        .collect();
    assert_eq!(
        only_generator,
        vec!["ActionScheme"],
        "generator/model.rs 的独有类型集合变了 —— 若是有意新增，请同步更新本测试"
    );
}

/// 面板侧模型与 HTTP DTO：wire 字段名与顺序必须逐位相同，**无白名单**。
///
/// 这条断言就是 `ActionDto::run_as_admin` 缺 `rename` 的守门人：两个 Rust 结构体
/// 描述同一个 Go 契约，键名不一致时 serde 会把它当未知键静默丢弃。
#[test]
fn dto_and_models_agree_on_wire_fields() {
    let models = parse(MODELS);
    let dto = parse(DTO);

    for (name, model_fields) in &models {
        let dto_name = format!("{name}Dto");
        let dto_fields = dto
            .get(&dto_name)
            .unwrap_or_else(|| panic!("server/dto.rs 缺少 {dto_name}"));
        assert_eq!(
            wire_names(dto_fields),
            wire_names(model_fields),
            "{dto_name} 与 models::{name} 的 wire 字段名/顺序不一致 —— \
             面板发出的键名会被服务端当未知键丢弃（静默丢字段）"
        );
    }

    let model_names: BTreeSet<&str> = models.keys().map(String::as_str).collect();
    for dto_name in dto.keys() {
        let base = dto_name.strip_suffix("Dto").unwrap_or(dto_name);
        assert!(
            model_names.contains(base),
            "server/dto.rs::{dto_name} 在 models/ 里没有对应的 {base} —— 契约面出现单边类型"
        );
    }
}

/// 非 wire 字段（`#[serde(skip)]`）的差异必须与白名单逐条相符，且白名单本身不得腐烂。
#[test]
fn non_wire_field_differences_are_documented() {
    let models = parse(MODELS);
    let generator = parse(GENERATOR);

    for (name, model_fields) in &models {
        let gen_fields = generator
            .get(name)
            .unwrap_or_else(|| panic!("generator/model.rs 缺少 models::{name}"));
        let (want_models, want_generator) = NON_WIRE_ONLY
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, m, g)| (*m, *g))
            .unwrap_or((&[], &[]));

        let skipped = |fields: &[Field]| -> Vec<String> {
            fields
                .iter()
                .filter(|f| f.skipped)
                .map(|f| f.rust.clone())
                .collect()
        };
        assert_eq!(
            skipped(model_fields),
            want_models.to_vec(),
            "models::{name} 的 serde(skip) 字段集变了 —— 若是有意改动，请同步更新 NON_WIRE_ONLY"
        );
        assert_eq!(
            skipped(gen_fields),
            want_generator.to_vec(),
            "generator::model::{name} 的 serde(skip) 字段集变了 —— 若是有意改动，请同步更新 NON_WIRE_ONLY"
        );
    }

    for (name, _) in GENERATOR_ONLY_WIRE {
        assert!(
            models.contains_key(*name),
            "GENERATOR_ONLY_WIRE 里的 {name} 已不存在 —— 白名单腐烂，请删除该条"
        );
    }
    for (name, models_only, generator_only) in NON_WIRE_ONLY {
        assert!(
            models.contains_key(*name) && generator.contains_key(*name),
            "NON_WIRE_ONLY 里的 {name} 已不再两边都有 —— 白名单腐烂，请删除该条"
        );
        if models_only.is_empty() && generator_only.is_empty() {
            panic!("NON_WIRE_ONLY 里的 {name} 两条都为空 —— 应删除该条");
        }
    }
}

/// 解析器自检：三份源码都能被解析出预期的类型数量。
///
/// 防的是「解析器静默退化 ⇒ 上面三条断言在空集合上通过」这一类假绿灯。
#[test]
fn parser_sees_the_full_contract_surface() {
    let models = parse(MODELS);
    let generator = parse(GENERATOR);
    let dto = parse(DTO);
    assert_eq!(models.len(), 18, "models/config.rs 的结构体数变了");
    assert_eq!(generator.len(), 19, "generator/model.rs 的结构体数变了");
    assert_eq!(dto.len(), 18, "server/dto.rs 的结构体数变了");
    assert!(models.contains_key("CommandInputSkin"));
    assert_eq!(
        wire_names(&models["CommandInputSkin"]).len(),
        18,
        "CommandInputSkin 必须恰好 18 个 wire 字段（命令框皮肤契约）"
    );
}
