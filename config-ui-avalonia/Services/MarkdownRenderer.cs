using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Documents;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Media.TextFormatting;
using SkiaSharp;

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

    /// <summary>
    /// 文档字体链: 保持 YaHei UI 首位 (行高/基线度量与历史渲染一致)。
    /// emoji 由 AppendTextRuns 拆成独立段经 Skia 位图内嵌 (见 GetEmojiImage)。
    /// </summary>
    private static readonly FontFamily DocFontFamily =
        new FontFamily("Microsoft YaHei UI, Segoe UI");

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
            ClipToBounds = false,
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
            FontFamily = DocFontFamily,
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(0, 2, 0, 2),
            ClipToBounds = false,
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
                FontFamily = DocFontFamily,
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
                    AppendTextRuns(tb, inline.Text, fontSize);
                    break;
            }
        }
    }

    /// <summary>emoji 基字符 (按码点判断; 覆盖 astral 区、杂项符号、丁贝符、专用变体)。
    /// 2600-26FF 保持全段 (Unicode emoji-data 全覆盖); 2700-27BF 只拆真正的 emoji,
    /// 避免 ➤(27A4) 这类文本符号被拆到 Segoe UI Emoji 后缺字形而消失 (YaHei 有其字形)。</summary>
    private static bool IsEmojiBase(int cp) =>
        (cp >= 0x1F000 && cp <= 0x1FBFF) ||          // astral emoji (含区域指示符/补充符号)
        (cp >= 0x2600 && cp <= 0x26FF) ||            // 杂项符号 ☀⚡⚙⚛⛔ 等
        cp is 0x2705 or 0x2708 or 0x2709 or 0x270A or 0x270B or 0x270C or 0x270D or
              0x270F or 0x2712 or 0x2714 or 0x2716 or 0x271D or 0x2721 or 0x2728 or
              0x2733 or 0x2734 or 0x2744 or 0x2747 or 0x274C or 0x274E or
              0x2753 or 0x2754 or 0x2755 or 0x2757 or 0x2763 or 0x2764 or
              0x2795 or 0x2796 or 0x2797 or 0x27A1 or 0x27B0 or 0x27BF ||  // 丁贝符 emoji 子集
        (cp >= 0x2B00 && cp <= 0x2BFF && cp is 0x2B05 or 0x2B06 or 0x2B07 or 0x2B09 or
              0x2B0A or 0x2B0B or 0x2B0C or 0x2B0D or 0x2B1B or 0x2B1C or
              0x2B50 or 0x2B55) ||           // ⬆⭐⭕ 等 emoji 子集
        cp is 0x203C or 0x2049 or 0x2139 or          // ‼ ⁉ ℹ
              0x231A or 0x231B or 0x2328 or          // ⌚ ⌛ ⌨
              0x23CF or 0x23E9 or 0x23EA or 0x23EB or 0x23EC or  // ⏏ ⏩⏪⏫⏬
              0x23ED or 0x23EE or 0x23EF or 0x23F0 or 0x23F1 or 0x23F2 or 0x23F3 or  // ⏭⏮⏯⏰⏱⏲
              0x23F8 or 0x23F9 or 0x23FA or          // ⏸⏹⏺
              0x3030 or 0x303D or 0x3297 or 0x3299;  // 〰 〽 ㊗ ㊙

    /// <summary>emoji 连接/呈现修饰符 (VS16 / ZWJ / keycap), 仅在紧邻 emoji 时归属 emoji 段。</summary>
    private static bool IsEmojiExtend(int cp) => cp is 0xFE0F or 0x200D or 0x20E3;

    private static readonly System.Collections.Concurrent.ConcurrentDictionary<string, IImage?> EmojiBitmapCache = new();

    /// <summary>
    /// 用 Skia 直接渲染 emoji 文本为透明底位图 (Skia 层 COLR 彩色渲染已实测可用)。
    /// 渲染失败返回 null (调用方回退普通文本 Run)。
    /// </summary>
    private static IImage? GetEmojiImage(string emojiText)
    {
        return EmojiBitmapCache.GetOrAdd(emojiText, _ =>
        {
            try
            {
                using var typeface = SKTypeface.FromFamilyName("Segoe UI Emoji");
                if (typeface == null) return null;
                const float renderSize = 64f;
                using var font = new SKFont(typeface, renderSize);
                // 2.88 的 SKFont.MeasureText 只收 glyphs span, 借传统 SKPaint 求精确 bounds
                using var measurePaint = new SKPaint { Typeface = typeface, TextSize = renderSize };
                var bounds = new SKRect();
                measurePaint.MeasureText(emojiText, ref bounds);
                const float pad = 6f;
                var info = new SKImageInfo((int)Math.Ceiling(bounds.Width) + (int)pad * 2,
                                           (int)Math.Ceiling(bounds.Height) + (int)pad * 2);
                using var surface = SKSurface.Create(info);
                var canvas = surface.Canvas;
                canvas.Clear(SKColors.Transparent);
                // bounds.Left 可能为负 (emoji 字形左伸), 用 pad - bounds.Left 作起点保证字形完整落入画布
                canvas.DrawText(emojiText, pad - bounds.Left, pad - bounds.Top, font, new SKPaint { IsAntialias = true });
                using var img = surface.Snapshot();
                using var data = img.Encode(SKEncodedImageFormat.Png, 100);
                using var ms = new MemoryStream(data.ToArray());
                return new Bitmap(ms);
            }
            catch
            {
                return null;
            }
        });
    }

    /// <summary>
    /// 普通文本按码点拆段: emoji 序列用 Segoe UI Emoji 渲染 (彩色),
    /// 其余文字保持 DocFontFamily (度量与历史渲染一致)。
    /// ZWJ 系列与 VS16 修饰符跟随相邻 emoji 合并为同一段。
    /// </summary>
    private static void AppendTextRuns(TextBlock tb, string text, int fontSize)
    {
        int i = 0, segStart = 0;
        bool inEmoji = false, prevEmoji = false;

        void Flush(int end, bool emoji)
        {
            if (end <= segStart) return;
            var seg = text[segStart..end];
            if (!emoji)
            {
                tb.Inlines.Add(new Run(seg) { FontSize = fontSize });
                return;
            }
            // 彩色 emoji: Avalonia 文本栈画 COLR 字形只出 base outline 不走调色板,
            // 必须经 Skia 预渲染为透明底位图再内嵌; 渲染失败回退普通文本 (走全局 FontFallbacks 取 Segoe UI Emoji 字形, 黑白总比消失好)。
            var img = GetEmojiImage(seg);
            if (img != null)
            {
                double h = fontSize * 1.2;
                double w = h * img.Size.Width / img.Size.Height;
                // InlineUIContainer 底边即基线, 微调下沉近似原生行内观感
                tb.Inlines.Add(new InlineUIContainer(new Image
                {
                    Source = img,
                    Width = w,
                    Height = h,
                    Margin = new Thickness(0, 0, 0, -fontSize * 0.1),
                }));
            }
            else
            {
                tb.Inlines.Add(new Run(seg) { FontSize = fontSize });
            }
        }

        while (i < text.Length)
        {
            int cp = char.IsSurrogatePair(text, i)
                ? char.ConvertToUtf32(text, i) : text[i];
            int len = char.IsSurrogatePair(text, i) ? 2 : 1;

            bool isBase = IsEmojiBase(cp);
            bool isExt = IsEmojiExtend(cp);
            bool emojiChar = isBase || (isExt && prevEmoji);

            if (emojiChar != inEmoji)
            {
                Flush(i, inEmoji);
                segStart = i;
                inEmoji = emojiChar;
            }
            prevEmoji = emojiChar;
            i += len;
        }
        Flush(text.Length, inEmoji);
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
