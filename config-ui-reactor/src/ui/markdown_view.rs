//! 使用指南渲染层：`services::markdown` 的块模型 → WinUI 原生控件。
//!
//! 对齐旧 `Services/MarkdownRenderer.cs` 的语义，并按用户授权**改用更贴合 WinUI 的形态**：
//!
//! | 项 | 旧（Avalonia/Claude） | 新（WinUI/Fluent） | 依据 |
//! |---|---|---|---|
//! | 标题字号 | 24/22/18/16 | **28/20/16/14**（Title / Subtitle / Body Strong 档） | Fluent 排版层级 |
//! | 链接 | 自绘暖绿 + 下划线 | **`RichTextHyperlink`（系统 Accent 色 + 原生交互）** | Fluent「链接 = Accent」 |
//! | 正文 | 14 | 14（Body） | 一致 |
//! | 行内代码 | Coral + Consolas | **纯文本 run**（`RichTextRun` 仅支持粗体/斜体，无字色/字族） | 0.100.0 API 上限 |
//! | 列表符号 | • / ◦ / ▪ | 同（三级） | 通用符号 |
//! | 缩进 | 按层级 | `(level-1) × 16`（Fluent 4/8/12/16 间距栅格） | Fluent 间距 |
//!
//! 图片走 `Image::source(url)` 直接指后端静态站点（**无需字节中转**）。

use windows_reactor::*;

use crate::services::markdown::{MdBlock, MdInline, MdInlineKind, MdListItem};
use crate::theme;

/// Fluent 标题字号（按 `#` 级数 1..=4）。
const HEADING_SIZES: [f64; 4] = [28.0, 20.0, 16.0, 14.0];
/// 无序列表符号（按层级）。
const BULLETS: [&str; 3] = ["•", "◦", "▪"];
/// 列表每级缩进（Fluent 间距栅格）。
const LEVEL_INDENT: f64 = 16.0;
/// 图片最大宽度（避免文档内截图撑破布局）。
const IMAGE_MAX_WIDTH: f64 = 760.0;

/// 行内片段的「中性计划」——与 UI 类型解耦，便于单元测试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlinePlan {
    Text(String),
    Code(String),
    Link { text: String, uri: String },
}

/// 链接 URI 解析（复刻 `LinkOpener`）：以 `/` 开头的视为**后端站点内部路径**，拼本机地址。
pub fn resolve_link_uri(url: &str, port: u16) -> String {
    if !url.starts_with('/') {
        return url.to_string();
    }
    // CLI 传输（已登记本地静态站）没有端口 ⇒ 内部链接退化为 file:///（交系统默认程序打开）
    if let Some(path) = crate::ui::doc_assets::asset_path(url) {
        return crate::ui::doc_assets::file_uri(&path);
    }
    format!("http://127.0.0.1:{port}{url}")
}

/// 图片 URL（复刻 `HomePageViewModel.LoadImageAsync`）：相对路径拼到后端静态站点根。
pub fn image_url(src: &str, port: u16) -> String {
    let path = if src.starts_with('/') {
        src.to_string()
    } else {
        format!("/{src}")
    };
    format!("http://127.0.0.1:{port}{path}")
}

/// 标题字号（按 `#` 级数，越界钳制）。
pub fn heading_font_size(level: usize) -> f64 {
    HEADING_SIZES[level.clamp(1, 4) - 1]
}

/// 标题段前留白（级数越小留白越大）。
pub fn heading_top_margin(level: usize) -> f64 {
    match level.clamp(1, 4) {
        1 => 20.0,
        2 => 16.0,
        3 => 12.0,
        _ => 10.0,
    }
}

/// 列表符号（有序项用原文编号）。
pub fn list_marker(item: &MdListItem) -> String {
    if item.ordered {
        format!("{}.", item.ordinal)
    } else {
        BULLETS[item.level.clamp(1, 3) - 1].to_string()
    }
}

/// 行内片段 → 中性计划。
pub fn plan_inlines(inlines: &[MdInline], port: u16) -> Vec<InlinePlan> {
    inlines
        .iter()
        .map(|inline| match inline.kind {
            MdInlineKind::Text => InlinePlan::Text(inline.text.clone()),
            MdInlineKind::Code => InlinePlan::Code(inline.text.clone()),
            MdInlineKind::Link => InlinePlan::Link {
                text: inline.text.clone(),
                uri: resolve_link_uri(&inline.url, port),
            },
        })
        .collect()
}

// ------------------------------------------------------------------ 视图构建

/// 渲染整篇文档为一个纵向容器。
///
/// ⚠️ 0.100.0 的 `IntoViews` 只实现了 `()`/`[T; N]`/元组，**不含 `Vec`** ⇒
/// 动态长度的集合一律走 `keyed_children`（`KeyedView: From<(K, V)>` 是泛型的）。
pub fn render(blocks: &[MdBlock], port: u16) -> View {
    let children: Vec<(usize, View)> = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (index, block_view(block, port)))
        .collect();
    StackPanel::new()
        .spacing(10.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .keyed_children(children)
}

fn block_view(block: &MdBlock, port: u16) -> View {
    match block {
        MdBlock::Heading { level, text } => RichTextBlock::new()
            .text_wrapping(TextWrapping::Wrap)
            .font_size(heading_font_size(*level))
            .margin(Thickness::new(0.0, heading_top_margin(*level), 0.0, 6.0))
            .paragraphs(RichText::single_paragraph([RichTextInline::Run(bold_run(
                text.clone(),
            ))]))
            .into(),
        MdBlock::Paragraph { inlines } => RichTextBlock::new()
            .text_wrapping(TextWrapping::Wrap)
            .font_size(theme::FONT_BODY)
            .paragraphs(RichText::single_paragraph(
                plan_inlines(inlines, port).into_iter().map(to_rich_inline),
            ))
            .into(),
        MdBlock::List { items } => StackPanel::new().spacing(6.0).keyed_children(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| (index, list_item_view(item, port)))
                .collect::<Vec<_>>(),
        ),
        MdBlock::Image { src, alt } => image_view(src, alt, port),
    }
}

fn list_item_view(item: &MdListItem, port: u16) -> View {
    let indent = (item.level.clamp(1, 3) - 1) as f64 * LEVEL_INDENT;
    let marker: View = TextBlock::new()
        .text(list_marker(item))
        .font_size(theme::FONT_BODY)
        .foreground(theme::solid(theme::CHARCOAL))
        .width(24.0)
        .into();
    let body: View = RichTextBlock::new()
        .text_wrapping(TextWrapping::Wrap)
        .font_size(theme::FONT_BODY)
        .paragraphs(RichText::single_paragraph(
            plan_inlines(&item.inlines, port)
                .into_iter()
                .map(to_rich_inline),
        ))
        .into();

    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .margin(Thickness::new(indent, 0.0, 0.0, 0.0))
        .children((marker, body))
}

fn image_view(src: &str, alt: &str, port: u16) -> View {
    // `source()` 返回 `Result<Image, _>` ⇒ 必须作为**链尾**调用（否则后续 builder 落在 Result 上）。
    let builder = Image::new()
        .max_width(IMAGE_MAX_WIDTH)
        .horizontal_alignment(HorizontalAlignment::Left)
        .margin(Thickness::new(0.0, 8.0, 0.0, 12.0));

    // CLI 传输（已登记本地静态站）：无端口 ⇒ 直读文件 + `source_data`
    // （reactor 内建的 WinRT 流式加载，不依赖 `BitmapImage` 接受哪种 URI 方案）。
    if let Some(path) = crate::ui::doc_assets::asset_path(src) {
        return match std::fs::read(&path) {
            Ok(bytes) => builder.source_data(EncodedImage::new(bytes)).into(),
            Err(_) => image_placeholder(alt),
        };
    }

    match builder.source(image_url(src, port)) {
        Ok(image) => image.into(),
        Err(_) => image_placeholder(alt),
    }
}

/// 图片不可用占位（旧版行为 = 失败返回 null 保持空白；此处保留 alt 更可诊断）。
fn image_placeholder(alt: &str) -> View {
    TextBlock::new()
        .text(format!("[图片不可用：{alt}]"))
        .font_size(theme::FONT_CAPTION)
        .foreground(theme::stone_gray())
        .into()
}

fn bold_run(text: impl Into<String>) -> RichTextRun {
    let mut run = RichTextRun::plain(text);
    run.is_bold = true;
    run
}

fn to_rich_inline(plan: InlinePlan) -> RichTextInline {
    match plan {
        InlinePlan::Text(text) => RichTextInline::Run(RichTextRun::plain(text)),
        // 0.100.0 的 RichTextRun 只支持粗体/斜体，无法表达字色与字族 ⇒ 行内代码退化为纯文本
        InlinePlan::Code(code) => RichTextInline::Run(RichTextRun::plain(code)),
        InlinePlan::Link { text, uri } => {
            RichTextInline::Hyperlink(RichTextHyperlink { text, uri })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::markdown::{MdInline, MdInlineKind, MdListItem};

    fn inline(kind: MdInlineKind, text: &str, url: &str) -> MdInline {
        MdInline {
            kind,
            text: text.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn internal_links_are_resolved_against_backend_port() {
        assert_eq!(
            resolve_link_uri("/site/faq.html", 12333),
            "http://127.0.0.1:12333/site/faq.html"
        );
        assert_eq!(
            resolve_link_uri("https://example.com/a", 12333),
            "https://example.com/a"
        );
    }

    #[test]
    fn image_urls_are_resolved_against_backend_root() {
        assert_eq!(
            image_url("assets/a.png", 12333),
            "http://127.0.0.1:12333/assets/a.png"
        );
        assert_eq!(
            image_url("/assets/b.png", 12333),
            "http://127.0.0.1:12333/assets/b.png"
        );
    }

    #[test]
    fn heading_sizes_follow_fluent_scale() {
        assert_eq!(heading_font_size(1), 28.0, "H1 = Fluent Title");
        assert_eq!(heading_font_size(2), 20.0, "H2 = Fluent Subtitle");
        assert_eq!(heading_font_size(3), 16.0);
        assert_eq!(heading_font_size(4), 14.0, "H4 = Fluent Body");
        // 越界钳制
        assert_eq!(heading_font_size(0), 28.0);
        assert_eq!(heading_font_size(9), 14.0);
    }

    #[test]
    fn heading_margins_shrink_with_level() {
        assert!(heading_top_margin(1) > heading_top_margin(2));
        assert!(heading_top_margin(2) > heading_top_margin(3));
        assert!(heading_top_margin(3) >= heading_top_margin(4));
    }

    #[test]
    fn list_markers_switch_between_bullet_and_ordinal() {
        let bullet = MdListItem {
            level: 1,
            ordered: false,
            ordinal: "-".to_string(),
            inlines: vec![],
        };
        assert_eq!(list_marker(&bullet), "•");

        let nested = MdListItem {
            level: 2,
            ordered: false,
            ordinal: "-".to_string(),
            inlines: vec![],
        };
        assert_eq!(list_marker(&nested), "◦");

        let deep = MdListItem {
            level: 3,
            ordered: false,
            ordinal: "-".to_string(),
            inlines: vec![],
        };
        assert_eq!(list_marker(&deep), "▪");

        let ordered = MdListItem {
            level: 1,
            ordered: true,
            ordinal: "3".to_string(),
            inlines: vec![],
        };
        assert_eq!(list_marker(&ordered), "3.", "有序项沿用原文编号");
    }

    #[test]
    fn planning_preserves_inline_order_and_kinds() {
        let inlines = vec![
            inline(MdInlineKind::Text, "见 ", ""),
            inline(MdInlineKind::Link, "文档", "/doc"),
            inline(MdInlineKind::Text, " 与 ", ""),
            inline(MdInlineKind::Code, "es.exe", ""),
        ];
        let plans = plan_inlines(&inlines, 12333);
        assert_eq!(
            plans,
            vec![
                InlinePlan::Text("见 ".to_string()),
                InlinePlan::Link {
                    text: "文档".to_string(),
                    uri: "http://127.0.0.1:12333/doc".to_string(),
                },
                InlinePlan::Text(" 与 ".to_string()),
                InlinePlan::Code("es.exe".to_string()),
            ]
        );
    }

    #[test]
    fn empty_inline_list_yields_no_plans() {
        assert!(plan_inlines(&[], 1).is_empty());
    }
}
