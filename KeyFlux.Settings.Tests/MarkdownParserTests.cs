using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// MarkdownParser 契约守护: 总览页文档 (config_doc.md 子集) 的解析结果。
/// 覆盖标题层级/段落合并/列表嵌套/图片/行内链接与代码。
/// </summary>
public sealed class MarkdownParserTests
{
    [Fact]
    public void Parse_Headings_LevelAndText()
    {
        var blocks = MarkdownParser.Parse("## 😀 欢迎\n\n### 概述\n\n#### ➤ 第一行");

        Assert.Collection(blocks,
            b => Assert.Equal(new MdHeading(2, "😀 欢迎"), b),
            b => Assert.Equal(new MdHeading(3, "概述"), b),
            b => Assert.Equal(new MdHeading(4, "➤ 第一行"), b));
    }

    [Fact]
    public void Parse_Paragraph_ConsecutiveLinesMerged()
    {
        var blocks = MarkdownParser.Parse("第一行文字\n第二行文字\n\n空行后的段落");

        Assert.Collection(blocks,
            b =>
            {
                var p = Assert.IsType<MdParagraph>(b);
                Assert.Equal("第一行文字 第二行文字", p.Inlines[0].Text);
            },
            b =>
            {
                var p = Assert.IsType<MdParagraph>(b);
                Assert.Equal("空行后的段落", p.Inlines[0].Text);
            });
    }

    [Fact]
    public void Parse_List_OrderedAndNestedUnordered()
    {
        var blocks = MarkdownParser.Parse(
            "1. 第一项\n   - 子项一\n   - 子项二\n2. 第二项");

        var list = Assert.IsType<MdList>(Assert.Single(blocks));
        Assert.Equal(4, list.Items.Count);
        Assert.Equal((1, true, "1"), (list.Items[0].Level, list.Items[0].Ordered, list.Items[0].Ordinal));
        Assert.Equal((2, false, "-"), (list.Items[1].Level, list.Items[1].Ordered, list.Items[1].Ordinal));
        Assert.Equal("子项一", list.Items[1].Inlines[0].Text);
        Assert.Equal((1, true, "2"), (list.Items[3].Level, list.Items[3].Ordered, list.Items[3].Ordinal));
    }

    [Fact]
    public void Parse_Image_Line_ExtractsSrcAndAlt()
    {
        var blocks = MarkdownParser.Parse("![image-20230911094609620](img/example01.png)");

        var img = Assert.IsType<MdImage>(Assert.Single(blocks));
        Assert.Equal("img/example01.png", img.Src);
        Assert.Equal("image-20230911094609620", img.Alt);
    }

    [Fact]
    public void Parse_Inline_LinkAndCodeTokens()
    {
        var inlines = MarkdownParser.ParseInline("用 `Capslock + Z` 复制, 或 [去这下载](https://bilibili.com/x) 查看");

        Assert.Collection(inlines,
            i => Assert.Equal(new MdInline(MdInlineKind.Text, "用 "), i),
            i => Assert.Equal(new MdInline(MdInlineKind.Code, "Capslock + Z"), i),
            i => Assert.Equal(new MdInline(MdInlineKind.Text, " 复制, 或 "), i),
            i => Assert.Equal(new MdInline(MdInlineKind.Link, "去这下载", "https://bilibili.com/x"), i),
            i => Assert.Equal(new MdInline(MdInlineKind.Text, " 查看"), i));
    }

    [Fact]
    public void Parse_FullSample_BlockSequenceMatchesConfigDoc()
    {
        // config_doc.md 开头真实片段: 欢迎(标题+有序列表) -> 浏览器字体(标题+列表) -> 启动程序(标题+子标题+列表+图片)
        var md = """
            ## 😀 欢迎

            1. [项目 GitHub](https://github.com/xianyukang/KeyFlux)
            2. [视频介绍](https://www.bilibili.com/video/BV1Sf4y1c7p8/)

            ## 🚀 启动程序或激活窗口 

            ### 概述

            - 此功能用来启动程序或激活窗口

            ![image-20230911094609620](img/example01.png) 
            """;

        var blocks = MarkdownParser.Parse(md);

        Assert.Collection(blocks,
            b => Assert.Equal(new MdHeading(2, "😀 欢迎"), b),
            b =>
            {
                var l = Assert.IsType<MdList>(b);
                Assert.Equal(2, l.Items.Count);
                Assert.Equal(MdInlineKind.Link, l.Items[0].Inlines[0].Kind);
                Assert.Equal("项目 GitHub", l.Items[0].Inlines[0].Text);
            },
            b => Assert.Equal(new MdHeading(2, "🚀 启动程序或激活窗口"), b),
            b => Assert.Equal(new MdHeading(3, "概述"), b),
            b =>
            {
                var l = Assert.IsType<MdList>(b);
                Assert.Single(l.Items);
                Assert.Equal("此功能用来启动程序或激活窗口", l.Items[0].Inlines[0].Text);
            },
            b => Assert.IsType<MdImage>(b));
    }
}
