namespace KeyFlux.Settings.Services;

/// <summary>
/// 「常用类型」一键套用预设 (方案 C7 面板简化): 点一下即填好显示名 / 识别方式 / 规则或扩展名,
/// 让不了解"匹配类型"概念的用户零输入完成创建。
/// <para>
/// 数据放 C# 常量的理由: 这是设置界面的**输入便利**（引导性建议），不是运行时数据 ——
/// 运行时的匹配类型定义始终由用户在弹窗里确认后才写入 <c>config.json</c> 的 <c>matchTypes[]</c>。
/// 与行为包 (<c>bin/behaviors/*/behavior.json</c>) 不同层，故不进配置结构。
/// </para>
/// <para>
/// 刻意**不含**「链接 / 路径 / 磁力链接 / 纯文本」: 它们是内置文本特征 (后端 KnownTextTypes),
/// 不落 <c>matchTypes[]</c>, 套用会与内置代号冲突。
/// </para>
/// </summary>
/// <param name="IdHint">代号基底 (内部代号被隐藏, 仅用于生成合法 ASCII 代号; 冲突时自动加数字后缀)。</param>
/// <param name="LabelKey">显示名 i18n 键 (插入后成为用户数据, 故按当前语言取一次)。</param>
/// <param name="Kind">"text" | "fileExt"。</param>
/// <param name="Values">kind=text → 规则右值 (每条 contains); kind=fileExt → 扩展名列表。</param>
/// <param name="DefaultActionId">建议的"命中后做什么" (内置行为包 id, 必须存在)。</param>
public sealed record MatchTypePreset(
    string IdHint,
    string LabelKey,
    string Kind,
    string[] Values,
    string DefaultActionId)
{
    /// <summary>当前语言下的显示名。</summary>
    public string Label => I18n.T(LabelKey);
}

/// <summary>常用类型预设表 (顺序即界面上的排列顺序)。</summary>
public static class MatchTypePresets
{
    public static readonly IReadOnlyList<MatchTypePreset> All =
    [
        // ---- 文本类 (选中一段文字时命中) ----
        new("netdisk", "2571", "text",
            ["pan.baidu.com", "aliyundrive.com", "cloud.189.cn"], "open_url"),
        new("github", "2572", "text",
            ["github.com/"], "open_url"),
        new("bilibili", "2573", "text",
            ["bilibili.com/"], "open_url"),

        // ---- 文件类 (选中某类文件时命中) ----
        new("images", "2574", "fileExt",
            ["jpg", "jpeg", "png", "gif", "bmp", "webp", "svg"], "open_path"),
        new("archives", "2575", "fileExt",
            ["zip", "rar", "7z", "tar", "gz"], "open_path"),
        new("code", "2576", "fileExt",
            ["py", "js", "ts", "go", "rs", "java", "c", "cpp"], "open_path"),
    ];
}
