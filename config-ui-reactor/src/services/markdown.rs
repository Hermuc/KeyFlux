//! 使用指南页文档解析器（**纯逻辑，零 UI 依赖**）。
//!
//! 严格复刻 `config-ui-avalonia/Services/MarkdownParser.cs`：
//!
//! * 支持语法子集（文档实际用到的部分）——
//!   块级：`#`~`####` 标题、`- `/`1. ` 列表（2 空格缩进嵌套，最多 3 层）、`![alt](url)` 图片、段落；
//!   行内：`[text](url)` 链接、`` `code` `` 行内代码；
//! * 输出**不可变块模型**（[`MdBlock`]），供渲染层消费；未来若要导出 HTML 可直接复用。
//!
//! 正则语义刻意与 C# 保持一致（如 `#{1,4}` 后必须跟空白 ⇒ `#####` 不是标题；
//! 段落续行遇 `# \d+. - ![` 中断）。差异点：Rust 的 `$` 不匹配结尾换行前位置，
//! 但我们传入的始终是**单行 trim 后**文本，故无影响。

use std::sync::LazyLock;

use regex::Regex;

/// 行内片段种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdInlineKind {
    Text,
    Code,
    Link,
}

/// 行内片段（文本与代码的 `url` 为空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdInline {
    pub kind: MdInlineKind,
    pub text: String,
    pub url: String,
}

impl MdInline {
    fn text(text: impl Into<String>) -> Self {
        Self {
            kind: MdInlineKind::Text,
            text: text.into(),
            url: String::new(),
        }
    }
}

/// 列表项（`ordered` 为 true 时 `ordinal` 是原文编号，否则是 `"-"`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdListItem {
    /// 嵌套层级（1..=3）。
    pub level: usize,
    pub ordered: bool,
    pub ordinal: String,
    pub inlines: Vec<MdInline>,
}

/// 文档块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdBlock {
    /// 标题（level 1..=4）。
    Heading { level: usize, text: String },
    /// 段落。
    Paragraph { inlines: Vec<MdInline> },
    /// 列表（连续列表项合并为一块）。
    List { items: Vec<MdListItem> },
    /// 图片（`src` 为原始路径）。
    Image { src: String, alt: String },
}

static RE_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,4})\s+(.*)$").expect("heading regex"));
static RE_IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^!\[(.*?)\]\((.*?)\)\s*$").expect("image regex"));
static RE_ORDERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)\.\s+(.*)$").expect("ordered list regex"));
static RE_UNORDERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-\s+(.*)$").expect("unordered list regex"));
static RE_LIST_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+\.\s|-\s)").expect("list line regex"));
static RE_PARA_BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,4}\s|!\[|\d+\.\s|-\s)").expect("paragraph break regex"));
static RE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").expect("link regex"));

/// 解析整篇 markdown，返回按文档顺序排列的块模型。
pub fn parse(md: &str) -> Vec<MdBlock> {
    let normalized = md.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        // 标题
        if let Some(captures) = RE_HEADING.captures(trimmed) {
            let level = captures[1].chars().count();
            let text = captures[2].trim().to_string();
            blocks.push(MdBlock::Heading { level, text });
            index += 1;
            continue;
        }

        // 独立图片行
        if let Some(captures) = RE_IMAGE.captures(trimmed) {
            blocks.push(MdBlock::Image {
                src: captures[2].to_string(),
                alt: captures[1].to_string(),
            });
            index += 1;
            continue;
        }

        // 列表：收集连续列表项为一个列表块
        if RE_LIST_LINE.is_match(trimmed) {
            let (block, next) = parse_list(&lines, index);
            blocks.push(block);
            index = next;
            continue;
        }

        // 段落：合并连续普通行（空行/标题/列表/图片中断）
        let mut paragraph = trimmed.to_string();
        while index + 1 < lines.len() {
            let next = lines[index + 1].trim();
            if next.is_empty() || RE_PARA_BREAK.is_match(next) {
                break;
            }
            paragraph.push(' ');
            paragraph.push_str(next);
            index += 1;
        }
        blocks.push(MdBlock::Paragraph {
            inlines: parse_inline(&paragraph),
        });
        index += 1;
    }

    blocks
}

/// 收集从 `start` 起的连续列表项（2 空格缩进为嵌套，最多 3 层）。
/// 返回 `(列表块, 下一个未消费行号)`。
fn parse_list(lines: &[&str], start: usize) -> (MdBlock, usize) {
    let mut items = Vec::new();
    let mut index = start;

    while index < lines.len() {
        let raw = lines[index];
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            break;
        }

        // 缩进按「前导空白字符数 / 2 + 1」定层级（缩进为空格时字节数 == 字符数）
        let indent = raw.len() - raw.trim_start().len();
        let level = ((indent / 2) + 1).clamp(1, 3);

        let ordered = RE_ORDERED.captures(trimmed);
        let unordered = RE_UNORDERED.captures(trimmed);
        if ordered.is_none() && unordered.is_none() {
            break;
        }

        if let Some(captures) = ordered {
            items.push(MdListItem {
                level,
                ordered: true,
                ordinal: captures[1].to_string(),
                inlines: parse_inline(captures[2].trim()),
            });
        } else if let Some(captures) = unordered {
            items.push(MdListItem {
                level,
                ordered: false,
                ordinal: "-".to_string(),
                inlines: parse_inline(captures[1].trim()),
            });
        }

        index += 1;
    }

    (MdBlock::List { items }, index)
}

/// 行内解析：按 `[text](url)` 链接切分，剩余部分再拆 `` `code` ``。
pub fn parse_inline(text: &str) -> Vec<MdInline> {
    let mut inlines = Vec::new();
    for token in split_link(text) {
        match token {
            LinkToken::Link { text, url } => inlines.push(MdInline {
                kind: MdInlineKind::Link,
                text,
                url,
            }),
            LinkToken::Plain(part) => append_code_tokens(&mut inlines, &part),
        }
    }
    inlines
}

/// 把文本按反引号切分为 文本/代码 交替片段。
fn append_code_tokens(inlines: &mut Vec<MdInline>, text: &str) {
    for (index, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        inlines.push(MdInline {
            kind: if index % 2 == 1 {
                MdInlineKind::Code
            } else {
                MdInlineKind::Text
            },
            text: part.to_string(),
            url: String::new(),
        });
    }
}

enum LinkToken {
    Plain(String),
    Link { text: String, url: String },
}

/// 把行内文本按 `[text](url)` 切分为交替片段。
fn split_link(text: &str) -> Vec<LinkToken> {
    let mut tokens = Vec::new();
    let mut position = 0;

    for captures in RE_LINK.captures_iter(text) {
        let whole = captures.get(0).expect("whole match");
        if whole.start() > position {
            tokens.push(LinkToken::Plain(text[position..whole.start()].to_string()));
        }
        tokens.push(LinkToken::Link {
            text: captures[1].to_string(),
            url: captures[2].to_string(),
        });
        position = whole.end();
    }

    if position < text.len() {
        tokens.push(LinkToken::Plain(text[position..].to_string()));
    }

    tokens
}

/// 扁平化辅助：取块模型中的纯文本（供无障碍名/搜索/测试对账使用）。
pub fn plain_text(blocks: &[MdBlock]) -> String {
    let mut out = String::new();
    let push_inlines = |inlines: &[MdInline], out: &mut String| {
        for inline in inlines {
            out.push_str(&inline.text);
        }
    };
    for block in blocks {
        match block {
            MdBlock::Heading { text, .. } => out.push_str(text),
            MdBlock::Paragraph { inlines } => push_inlines(inlines, &mut out),
            MdBlock::List { items } => {
                for item in items {
                    push_inlines(&item.inlines, &mut out);
                }
            }
            MdBlock::Image { alt, .. } => out.push_str(alt),
        }
        out.push('\n');
    }
    out
}

/// 便捷：解析并直接取纯文本。
pub fn parse_plain_text(md: &str) -> String {
    plain_text(&parse(md))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_inlines(inlines: &[MdInline]) -> Vec<&str> {
        inlines.iter().map(|i| i.text.as_str()).collect()
    }

    #[test]
    fn headings_level_1_to_4_only() {
        let blocks = parse("# A\n## B\n### C\n#### D\n##### E");
        let levels: Vec<usize> = blocks
            .iter()
            .filter_map(|b| match b {
                MdBlock::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![1, 2, 3, 4], "`#####` 不是标题（与 C# 一致）");
    }

    #[test]
    fn heading_requires_whitespace_after_hashes() {
        // `#A` 不构成标题 ⇒ 落为段落
        let blocks = parse("#A");
        assert!(matches!(blocks[0], MdBlock::Paragraph { .. }));
    }

    #[test]
    fn image_block_captures_src_and_alt() {
        let blocks = parse("![示意](assets/img/a.png)");
        assert_eq!(
            blocks[0],
            MdBlock::Image {
                src: "assets/img/a.png".to_string(),
                alt: "示意".to_string()
            }
        );
    }

    #[test]
    fn unordered_and_ordered_lists_with_nesting() {
        let blocks = parse("- 一级\n  - 二级\n    1. 三级\n- 回到一级");
        let MdBlock::List { items } = &blocks[0] else {
            panic!("应为列表块");
        };
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].level, 1);
        assert!(!items[0].ordered);
        assert_eq!(items[1].level, 2);
        assert_eq!(items[2].level, 3);
        assert!(items[2].ordered);
        assert_eq!(items[2].ordinal, "1");
        assert_eq!(items[3].level, 1);
    }

    #[test]
    fn nesting_is_clamped_to_three_levels() {
        let blocks = parse("            1. 深缩进");
        let MdBlock::List { items } = &blocks[0] else {
            panic!("应为列表块");
        };
        assert_eq!(items[0].level, 3, "层级上限为 3");
    }

    #[test]
    fn consecutive_lines_merge_into_one_paragraph() {
        let blocks = parse("第一行\n第二行\n\n第三行");
        assert_eq!(blocks.len(), 2);
        let MdBlock::Paragraph { inlines } = &blocks[0] else {
            panic!("应为段落");
        };
        assert_eq!(text_inlines(inlines), vec!["第一行 第二行"]);
    }

    #[test]
    fn paragraph_breaks_on_heading_and_list_and_image() {
        for breaker in ["# 标题", "- 项", "1. 项", "![a](b)"] {
            let source = format!("段落首行\n{breaker}");
            let blocks = parse(&source);
            assert_eq!(blocks.len(), 2, "`{breaker}` 应中断段落");
            assert!(matches!(blocks[0], MdBlock::Paragraph { .. }));
        }
    }

    #[test]
    fn inline_link_and_code_split() {
        let inlines = parse_inline("见 [文档](https://x.y) 与 `code` 结束");
        let kinds: Vec<MdInlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MdInlineKind::Text,
                MdInlineKind::Link,
                MdInlineKind::Text,
                MdInlineKind::Code,
                MdInlineKind::Text
            ]
        );
        assert_eq!(inlines[1].text, "文档");
        assert_eq!(inlines[1].url, "https://x.y");
        assert_eq!(inlines[3].text, "code");
    }

    #[test]
    fn link_with_empty_text_and_url_is_kept() {
        let inlines = parse_inline("[](x)");
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0].kind, MdInlineKind::Link);
        assert_eq!(inlines[0].text, "");
        assert_eq!(inlines[0].url, "x");
    }

    #[test]
    fn code_fence_pairs_by_index_not_by_content() {
        // 反引号数量为奇数时，最后一段仍是代码（与 C# 的 i % 2 一致）
        let inlines = parse_inline("a`b`c`d");
        let kinds: Vec<MdInlineKind> = inlines.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MdInlineKind::Text,
                MdInlineKind::Code,
                MdInlineKind::Text,
                MdInlineKind::Code
            ]
        );
    }

    #[test]
    fn crlf_is_normalized() {
        let blocks = parse("# 标题\r\n\r\n段落\r\n");
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks[0],
            MdBlock::Heading {
                level: 1,
                text: "标题".to_string()
            }
        );
    }

    #[test]
    fn blank_and_whitespace_lines_are_skipped() {
        let blocks = parse("\n   \n# A\n\t\n");
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn real_world_sample_parses_into_expected_shape() {
        let md = "\
# KeyFlux 使用指南

本工具用于键盘映射。详见 [官方文档](https://example.com/doc)。

## 快速开始

- 打开设置面板
- 选择 `模式`
  1. 选择按键
  2. 绑定动作

![截图](assets/example01.png)
";
        let blocks = parse(md);
        assert_eq!(blocks.len(), 5, "标题/段落/标题/列表/图片");
        assert!(matches!(blocks[0], MdBlock::Heading { level: 1, .. }));
        assert!(matches!(blocks[2], MdBlock::Heading { level: 2, .. }));
        let MdBlock::List { items } = &blocks[3] else {
            panic!("应为列表块");
        };
        assert_eq!(items.len(), 4);
        assert_eq!(items[1].level, 1);
        assert_eq!(items[2].level, 2, "2 空格缩进为二级");
        assert!(matches!(blocks[4], MdBlock::Image { .. }));

        let plain = parse_plain_text(md);
        assert!(plain.contains("官方文档"));
        assert!(plain.contains("截图"));
    }

    #[test]
    fn plain_text_flattens_all_blocks() {
        let plain = parse_plain_text("# A\n段落\n- 项 1\n![alt](x)");
        assert_eq!(plain, "A\n段落\n项 1\nalt\n");
    }
}
