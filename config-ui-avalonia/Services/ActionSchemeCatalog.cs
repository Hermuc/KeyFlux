using KeyFlux.Settings.Models;

namespace KeyFlux.Settings.Services;

// ============================================================================
// 选中动作系统常量 (匹配类型/文本特征词表 + 共享工具; 复刻 config-ui constants.ts)。
//
// 词表纪律: 合法性裁决永远以后端 400 为准。
// 行为类型词表 (2026-09 起) 不再是静态副本 —— 由 BehaviorCatalog 从行为包 appliesTo
// 推导 (后端 GET /api/behaviors 下发, 真源 = 内置包 bin/behaviors + 用户包 data/behaviors,
// 见 CONTRACTS §3.9), 本文件只保留匹配类型/文本特征词表与共享工具方法。
//
// 文案键走 I18n (959+ 为移植新增编号), 语言切换时由页面 ViewModel 重建。
// ============================================================================

/// <summary>下拉选项条目 (Value 为配置值, Label 为显示文案; record 值相等便于 ComboBox 选中匹配)。</summary>
/// <param name="IsSeparator">分隔行标记: true 时该项在下拉中渲染为分隔线且不可选 (类型分组与文本特征之间)。</param>
public sealed record ComboOption(string Value, string Label, bool IsSeparator = false);

/// <summary>选中动作常量与共享工具方法 (2026-09 方案 D: 多方案 CRUD 工厂已随 ActionScheme 退役)。</summary>
public static class ActionSchemeCatalog
{
    // 匹配条件类型 (复刻 MATCH_TYPES; 「文本正则」与「文件分组」已按 2026-08 改造移除,
    // 「默认 (兜底)」已按 2026-09 移除, 现仅剩 fileExt / textType 两类)
    // (Value, LabelKey, HintKey)
    public static readonly (string Value, string LabelKey, string HintKey)[] MatchTypes =
    [
        ("fileExt", "1031", "1034"),
        ("textType", "1032", "1035"),
    ];

    // 行为类型词表已于 2026-09 迁移至 BehaviorCatalog: 由后端 GET /api/behaviors 下发的
    // 行为包 appliesTo 推导覆盖与默认 (CONTRACTS §3.9)。旧五张静态表
    // (ActionTypes / TextTypeActions / TextTypeDefaultAction / TextActions /
    //  FileGroupActions / FileGroupDefaultAction / FileActions / DefaultSearchUrl) 全部退役,
    // 覆盖语义: 行为前提 ⊇ 规则前提 (专属前提排前、通配排后), 合法性仍以后端 400 为准。

    // 文本特征 (复刻 TEXT_TYPES)
    public static readonly (string Value, string LabelKey)[] TextTypes =
    [
        ("url", "1059"),
        ("path", "1060"),
        ("magnet", "1061"),
        ("plain", "1062"),
    ];

    // (textType / fileExt 的可选行为与默认推荐全部由 BehaviorCatalog 从行为包 appliesTo 推导,
    //  见文件顶部说明; 此处不再保留任何静态词表副本。)

    /// <summary>切换文本特征时的默认行为: 由 BehaviorCatalog 按 default 标记推导。</summary>
    public static string? TextTypeDefaultAction(string textType) => BehaviorCatalog.DefaultFor("textType", textType);

    // ---- 文件后缀 (fileExt) 语境行为过滤 ----
    // 展示层过滤副本; 后端不校验 fileExt; 未知分组回退 FileActions。
    // 背景: fileExt 规则的组合合法性 Go 侧 ValidateActionSchemeRules 不裁决 (仅校验 textType),
    // 若不过滤会出现「图片分组 -> 磁力链接下载」等语义错配选项 (2026-09-15 修复)。

    /// <summary>文件语境 (fileExt) 的默认行为: 由 BehaviorCatalog 按 default 标记推导。</summary>
    public static string? FileGroupDefaultAction => BehaviorCatalog.DefaultFor("fileExt", "*");

    /// <summary>切换行为/分组时的旧硬编码默认 (search 补搜索模板 / run 补 %selected%) 现由
    /// 行为包 entry.params 声明, 见 BehaviorCatalog.DefaultTemplateFor。</summary>

    /// <summary>
    /// 归一化后缀串: 按逗号/顿号/分号 (含全角) 分割、Trim、去空、去两端点
    /// (评审 F4: 尾点如 "jpg." 一并去除, 防死条目经写回进共享分组);
    /// 去重 (忽略大小写) 但保留首个书写形式。供编辑器分组关联重建与保存写回共用。
    /// </summary>
    public static List<string> NormalizeExts(string? matchValue)
    {
        var result = new List<string>();
        if (string.IsNullOrWhiteSpace(matchValue)) return result;
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var token in matchValue.Split([',', '，', '、', ';', '；']))
        {
            var ext = token.Trim().Trim('.'); // 两端去点 (评审 F4)
            if (ext.Length == 0) continue;
            if (seen.Add(ext)) result.Add(ext);
        }
        return result;
    }

    /// <summary>
    /// 两个后缀集合是否等价 (评审 F4: 两侧先各自 <see cref="NormalizeExts"/> 归一化再比较,
    /// 忽略大小写/顺序/两端点 —— 防带点存量分组被「无变化重写」; 供分组关联重建与保存写回共用)。
    /// </summary>
    public static bool SameExts(IEnumerable<string> a, IEnumerable<string> b)
        => new HashSet<string>(
               NormalizeExts(string.Join(',', a)), StringComparer.OrdinalIgnoreCase)
            .SetEquals(NormalizeExts(string.Join(',', b)));

    // ------------------------------------------------------------- 标签查询

    public static string MatchTypeLabel(string value)
        => MatchTypes.FirstOrDefault(x => x.Value == value) is var t && t.Value is not null
            ? I18n.T(t.LabelKey) : value;

    /// <summary>行为显示名: 由 BehaviorCatalog 按包名推导 (按语言 name/nameEn, 未知 ID 回退原值)。</summary>
    public static string ActionTypeLabel(string value) => BehaviorCatalog.LabelFor(value);

    public static string TextTypeLabel(string value)
        => TextTypes.FirstOrDefault(x => x.Value == value) is var t && t.Value is not null
            ? I18n.T(t.LabelKey) : value;

    public static string MatchTypeHint(string value)
        => MatchTypes.FirstOrDefault(x => x.Value == value) is var t && t.Value is not null
            ? I18n.T(t.HintKey) : "";

    /// <summary>行为提示: 由 BehaviorCatalog 按包 description 推导 (未知 ID 回退空串)。</summary>
    public static string ActionTypeHint(string value) => BehaviorCatalog.HintFor(value);
}
