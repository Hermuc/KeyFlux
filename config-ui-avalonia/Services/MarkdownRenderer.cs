using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Documents;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.TextFormatting;

namespace KeyFlux.Settings.Services;

// ============================================================================
// MarkdownRenderer: 总览页文档渲染层 (块模型 -> Avalonia 控件)
//
// 消费 MarkdownParser 的不可变块模型, 构建展示控件树。
// 外部依赖全部经回调注入, 不感知会话/网络细节:
//   - loadImage: 图片块异步加载 (调用方负责从后端拉取, 失败返回 null 保持空白)
//   - openLink:  链接点击策略 (调用方决定如何打开外部/内部链接)
// ============================================================================
public static class MarkdownRenderer
{
    private const string LinkColor = "#4169E1";
    private const string CodeColor = "#C7254E";
    private const string CodeFont = "Consolas";

    /// <summary>链接文字基线补偿 (14px 字号实测校准): 段落行高 24 用 15.4, 列表行高 23 用 15.2。</summary>
    private const double ParagraphLinkOffset = 15.4;
    private const double ListLinkOffset = 15.2;

    /// <summary>标题字号 (按级别): # 24 / ## 22 / ### 18 / #### 16。</summary>
    private static readonly int[] HeadingSizes = [24, 22, 18, 16];

    /// <summary>无序列表符号 (按层级): • / ◦ / ▪。</summary>
    private static readonly string[] BulletChars = ["•", "◦", "▪"];

    /// <summary>
    /// 渲染整篇文档。返回按文档顺序排列的控件 (标题/列表/图片/段落)。
    /// </summary>
    public static List<Control> Render(IReadOnlyList<MdBlock> blocks, Func<string, Task<byte[]?>> loadImage, Action<string> openLink)
    {
        var controls = new List<Control>(blocks.Count);
        foreach (var block in blocks)
        {
            controls.Add(block switch
            {
                MdHeading h => BuildHeading(h, openLink),
                MdParagraph p => BuildParagraph(p, openLink),
                MdList l => BuildList(l, openLink),
                MdImage img => BuildImage(img, loadImage),
                // 兜底分支 (未知块类型): 空文本也保持可选中, 与其余文档块一致
                _ => new SelectableTextBlock { Text = "" },
            });
        }
        return controls;
    }

    /// <summary>构建标题块: 加粗 + 分级字号 + 段前留白。
    /// 用 SelectableTextBlock (继承 TextBlock, 官方可选中控件) 支持鼠标选词 + Ctrl+C 复制。</summary>
    private static Control BuildHeading(MdHeading heading, Action<string> openLink)
    {
        var size = HeadingSizes[Math.Clamp(heading.Level, 1, 4) - 1];
        var tb = new SelectableTextBlock
        {
            FontSize = size,
            FontWeight = FontWeight.Bold,
            Margin = new Thickness(0, heading.Level == 1 ? 16 : 14, 0, 6),
            TextWrapping = TextWrapping.Wrap,
        };
        AppendInline(tb, MarkdownParser.ParseInline(heading.Text), size, null, openLink);
        return tb;
    }

    /// <summary>构建段落: 14px 正文, 1.7 倍行高 (与旧版 config_doc 正文观感一致)。
    /// SelectableTextBlock 支持选词复制 (同标题块)。</summary>
    private static Control BuildParagraph(MdParagraph paragraph, Action<string> openLink)
    {
        var tb = new SelectableTextBlock
        {
            FontSize = 14,
            LineHeight = 24,
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(0, 2, 0, 2),
        };
        AppendInline(tb, paragraph.Inlines, 14, ParagraphLinkOffset, openLink);
        return tb;
    }

    /// <summary>构建列表块: 每项按层级缩进, 有序沿用原文编号, 无序用层级符号。
    /// 列表项用 SelectableTextBlock 支持选词复制 (同标题块)。</summary>
    private static Control BuildList(MdList list, Action<string> openLink)
    {
        var panel = new StackPanel { Spacing = 3, Margin = new Thickness(0, 4, 0, 4) };
        foreach (var item in list.Items)
        {
            var tb = new SelectableTextBlock
            {
                FontSize = 14,
                LineHeight = 23,
                TextWrapping = TextWrapping.Wrap,
                Margin = new Thickness(20 * (item.Level - 1), 0, 0, 0),
            };
            var bullet = item.Ordered ? item.Ordinal + "." : BulletChars[Math.Clamp(item.Level, 1, 3) - 1];
            tb.Inlines.Add(new Run(bullet + " ")
            {
                Foreground = new SolidColorBrush(Color.Parse(LinkColor)),
                FontWeight = FontWeight.SemiBold,
            });
            AppendInline(tb, item.Inlines, 14, ListLinkOffset, openLink);
            panel.Children.Add(tb);
        }
        return panel;
    }

    /// <summary>构建图片块: 异步加载, 最大宽度 680, 等比缩放, 失败保持空白。</summary>
    private static Control BuildImage(MdImage image, Func<string, Task<byte[]?>> loadImage)
    {
        var img = new Image
        {
            MaxWidth = 680,
            Stretch = Stretch.Uniform,
            StretchDirection = StretchDirection.DownOnly,
            HorizontalAlignment = HorizontalAlignment.Left,
            Margin = new Thickness(0, 8, 0, 8),
        };
        _ = LoadImageAsync(img, image.Src, loadImage);
        return img;
    }

    private static async Task LoadImageAsync(Image image, string src, Func<string, Task<byte[]?>> loadImage)
    {
        try
        {
            var bytes = await loadImage(src);
            if (bytes is null || bytes.Length == 0) return;
            using var ms = new MemoryStream(bytes);
            image.Source = new Avalonia.Media.Imaging.Bitmap(ms);
        }
        catch
        {
            // 图片加载失败: 保持空白占位, 不影响文档其余部分
        }
    }

    /// <summary>行内渲染: 按片段种类追加 Run / 代码 Run / 链接控件。</summary>
    private static void AppendInline(TextBlock tb, IReadOnlyList<MdInline> inlines, int fontSize, double? linkBaselineOffset, Action<string> openLink)
    {
        foreach (var inline in inlines)
        {
            switch (inline.Kind)
            {
                case MdInlineKind.Code:
                    tb.Inlines.Add(new Run(inline.Text)
                    {
                        FontFamily = new FontFamily(CodeFont),
                        FontSize = fontSize - 1,
                        Foreground = new SolidColorBrush(Color.Parse(CodeColor)),
                    });
                    break;
                case MdInlineKind.Link:
                    // 关键: InlineUIContainer 内嵌控件不继承外层 TextBlock 的字体, 必须显式传入,
                    // 否则不同字体度量 (Ascent) 会导致链接文字比同行普通文字更高。
                    tb.Inlines.Add(new InlineUIContainer
                    {
                        BaselineAlignment = BaselineAlignment.Baseline,
                        Child = BuildLink(inline.Text, inline.Url, fontSize, tb.FontFamily, linkBaselineOffset, openLink),
                    });
                    break;
                default:
                    tb.Inlines.Add(new Run(inline.Text) { FontSize = fontSize });
                    break;
            }
        }
    }

    /// <summary>
    /// 链接控件: 蓝色下划线 + 手型光标, 点击回调 openLink。
    /// 与所在行同字体同字号, 文字度量 (Ascent/基线) 一致, 避免内嵌控件位置偏移。
    /// 指针交互: SelectableTextBlock 的 OnPointerPressed 无条件捕获指针 (e.Pointer.Capture(this))
    /// 且类处理器注册为 handledEventsToo=false —— 内嵌链接收不到 PointerReleased, Tapped 手势
    /// 永不触发 (此前改 Tapped 导致链接完全点不了)。故链接自行捕获指针并判断:
    /// 「原地释放 = 点击跳转; 移出链接 = 拖动/误触, 不跳转」; 按下时 Handled=true 阻止
    /// SelectableTextBlock 接管文本选择 (仅限链接区域, 链接外选词不受影响)。
    /// </summary>
    private static TextBlock BuildLink(string text, string url, int fontSize, FontFamily fontFamily, double? baselineOffset, Action<string> openLink)
    {
        var link = new TextBlock
        {
            Text = text,
            FontSize = fontSize,
            FontFamily = fontFamily,
            // LineHeight 是继承属性: 继承外层 24/23 会使控件高度=行盒高度,
            // EmbeddedControlRun 基线对齐时把控件顶到行顶之上, 链接文字明显偏高。
            // 取消继承, 让控件按自身文字行高布局, 由 BaselineOffset 精确对齐。
            LineHeight = double.NaN,
            Foreground = new SolidColorBrush(Color.Parse(LinkColor)),
            TextDecorations = TextDecorations.Underline,
            Cursor = new Cursor(StandardCursorType.Hand),
        };
        // 关键: BaselineAlignment=Baseline 时 EmbeddedControlRun 按控件 BaselineOffset
        // 对齐行基线。TextLayout.Baseline 是纯文字基线, 实际渲染还有行盒补偿,
        // 直接使用会偏低; 段落/列表的补偿值已按 14px 字号肉眼校准 (见上方常量),
        // 其余字号 (标题) 按字体度量等比折算。
        using var layout = new TextLayout(text, new Typeface(fontFamily), fontSize, null);
        link.BaselineOffset = baselineOffset ?? layout.Baseline + 1.5;

        // 指针交互: SelectableTextBlock 的 OnPointerPressed 无条件捕获指针 (e.Pointer.Capture(this))
        // 且类处理器注册为 handledEventsToo=false —— 内嵌链接收不到 PointerReleased, Tapped 手势
        // 永不触发 (此前改 Tapped 导致链接完全点不了)。故链接按下时自行捕获指针并 Handled,
        // 阻止 SelectableTextBlock 接管; 链接收到完整按下/释放序列后 Tapped 正常触发:
        // 「原地释放 = 点击跳转; 拖动超过手势阈值 = Tapped 不触发, 不误跳」。链接外选词不受影响。
        link.PointerPressed += (_, e) =>
        {
            if (!e.GetCurrentPoint(link).Properties.IsLeftButtonPressed) return;
            e.Pointer.Capture(link);
            e.Handled = true; // 阻止 SelectableTextBlock 的指针捕获与文本选择接管 (仅链接区域)
        };
        link.Tapped += (_, _) => openLink(url);
        return link;
    }
}
