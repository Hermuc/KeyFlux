using System.Text.RegularExpressions;

namespace KeyFlux.Settings.Services;

// ============================================================================
// MarkdownParser: 总览页文档解析器 (纯逻辑, 零 UI 依赖)
//
// 权威蓝本: bin/site/config_doc.md (config_doc.html 的源, Typora 导出)。
// 仅支持文档实际用到的语法子集:
//   块级: # ~ #### 标题、- / 1. 列表 (含 2 空格缩进嵌套)、![alt](url) 图片、段落;
//   行内: [text](url) 链接、`code` 行内代码。
// 输出不可变块模型 (MdBlock 记录), 由 MarkdownRenderer 消费渲染为控件,
// 未来如需导出 HTML 等目标可直接复用本模型。
// ============================================================================

/// <summary>行内片段种类: 普通文本 / 行内代码 / 链接。</summary>
public enum MdInlineKind { Text, Code, Link }

/// <summary>行内片段 (文本与代码的 Url 为空)。</summary>
public sealed record MdInline(MdInlineKind Kind, string Text, string Url = "");

/// <summary>文档块基类。</summary>
public abstract record MdBlock;

/// <summary>标题块 (Level: 1~4)。</summary>
public sealed record MdHeading(int Level, string Text) : MdBlock;

/// <summary>段落块 (行内片段序列)。</summary>
public sealed record MdParagraph(List<MdInline> Inlines) : MdBlock;

/// <summary>列表项 (Ordered 为 true 时 Ordinal 是原文编号, 否则是符号)。</summary>
public sealed record MdListItem(int Level, bool Ordered, string Ordinal, List<MdInline> Inlines);

/// <summary>列表块 (连续列表项, 含缩进嵌套层级)。</summary>
public sealed record MdList(List<MdListItem> Items) : MdBlock;

/// <summary>图片块 (Src 为原始路径, Alt 为描述)。</summary>
public sealed record MdImage(string Src, string Alt) : MdBlock;

/// <summary>总览文档解析器: markdown 文本 -> 块模型列表。</summary>
public static class MarkdownParser
{
    /// <summary>解析整篇 markdown, 返回按文档顺序排列的块模型。</summary>
    public static List<MdBlock> Parse(string md)
    {
        var blocks = new List<MdBlock>();
        var lines = md.Replace("\r\n", "\n").Split('\n');

        for (var i = 0; i < lines.Length; i++)
        {
            var trimmed = lines[i].Trim();
            if (trimmed.Length == 0) continue;

            // 标题
            var m = Regex.Match(trimmed, "^(#{1,4})\\s+(.*)$");
            if (m.Success)
            {
                blocks.Add(new MdHeading(m.Groups[1].Value.Length, m.Groups[2].Value.Trim()));
                continue;
            }

            // 独立图片行
            m = Regex.Match(trimmed, "^!\\[(.*?)\\]\\((.*?)\\)\\s*$");
            if (m.Success)
            {
                blocks.Add(new MdImage(m.Groups[2].Value, m.Groups[1].Value));
                continue;
            }

            // 列表: 收集连续列表项 (含缩进嵌套) 为一个列表块
            if (IsListLine(trimmed))
            {
                blocks.Add(ParseList(lines, ref i));
                continue;
            }

            // 段落: 合并连续普通行 (空行/标题/列表/图片中断)
            var paragraph = trimmed;
            while (i + 1 < lines.Length)
            {
                var next = lines[i + 1].Trim();
                if (next.Length == 0 || Regex.IsMatch(next, "^(#{1,4}\\s|!\\[|\\d+\\.\\s|-\\s)")) break;
                paragraph += " " + next;
                i++;
            }
            blocks.Add(new MdParagraph(ParseInline(paragraph)));
        }
        return blocks;
    }

    /// <summary>
    /// 收集从当前行起的连续列表项 (2 空格缩进为嵌套, 最多 3 层)。
    /// 有序项沿用原文编号, 无序项由调用方按层级定符号。
    /// </summary>
    private static MdList ParseList(string[] lines, ref int i)
    {
        var items = new List<MdListItem>();
        while (i < lines.Length)
        {
            var raw = lines[i];
            var t = raw.Trim();
            if (t.Length == 0) break;

            var level = Math.Clamp((raw.Length - raw.TrimStart().Length) / 2 + 1, 1, 3);
            var om = Regex.Match(t, "^(\\d+)\\.\\s+(.*)$");
            var um = Regex.Match(t, "^-\\s+(.*)$");
            if (!om.Success && !um.Success) break;

            if (om.Success)
            {
                items.Add(new MdListItem(level, true, om.Groups[1].Value, ParseInline(om.Groups[2].Value.Trim())));
            }
            else
            {
                items.Add(new MdListItem(level, false, "-", ParseInline(um.Groups[1].Value.Trim())));
            }
            i++;
        }
        i--; // 外层 for 循环会再 +1
        return new MdList(items);
    }

    /// <summary>行内解析: 按 [text](url) 链接切分, 剩余部分再拆 `code`。</summary>
    public static List<MdInline> ParseInline(string text)
    {
        var inlines = new List<MdInline>();
        foreach (var (isLink, part, url) in SplitLink(text))
        {
            if (isLink)
            {
                inlines.Add(new MdInline(MdInlineKind.Link, part, url));
            }
            else
            {
                AppendCodeTokens(inlines, part);
            }
        }
        return inlines;
    }

    /// <summary>把文本按 ` 切分为 文本/代码 交替片段。</summary>
    private static void AppendCodeTokens(List<MdInline> inlines, string text)
    {
        var parts = text.Split('`');
        for (var i = 0; i < parts.Length; i++)
        {
            if (parts[i].Length == 0) continue;
            inlines.Add(new MdInline(i % 2 == 1 ? MdInlineKind.Code : MdInlineKind.Text, parts[i]));
        }
    }

    /// <summary>把行内文本按 [text](url) 切分为交替片段。</summary>
    private static List<(bool isLink, string part, string url)> SplitLink(string text)
    {
        var tokens = new List<(bool, string, string)>();
        var pos = 0;
        foreach (Match m in Regex.Matches(text, "\\[([^\\]]*)\\]\\(([^)]*)\\)"))
        {
            if (m.Index > pos) tokens.Add((false, text[pos..m.Index], ""));
            tokens.Add((true, m.Groups[1].Value, m.Groups[2].Value));
            pos = m.Index + m.Length;
        }
        if (pos < text.Length) tokens.Add((false, text[pos..], ""));
        return tokens;
    }

    private static bool IsListLine(string trimmed)
        => Regex.IsMatch(trimmed, "^(\\d+\\.\\s|-\\s)");
}
