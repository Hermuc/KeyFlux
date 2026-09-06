using KeyFlux.Settings.Models;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 行为目录 (CONTRACTS §3.9): 后端 GET /api/behaviors 快照 + 覆盖/默认/显示名推导。
/// 取代旧 ActionSchemeCatalog 的五张静态词表 —— 行为合法性真源在后端 (内置包+用户包),
/// C# 侧仅做展示层推导; 词表纪律不变: 合法性裁决永远以后端 400 为准。
/// </summary>
public static class BehaviorCatalog
{
    private static List<BehaviorPack> _builtin = [];
    private static List<BehaviorPack> _user = [];

    /// <summary>目录是否已成功拉取 (未拉取时行为下拉为空, 由页面 Loaded 触发加载)。</summary>
    public static bool Loaded { get; private set; }

    /// <summary>内置 + 用户包合并视图 (内置在前; 各自 ID 字典序, 决定默认推导的稳定序)。</summary>
    public static IReadOnlyList<BehaviorPack> Packs => [.. _builtin, .. _user];

    /// <summary>从后端拉取快照; 失败时保留旧快照 (异常由调用方提示), 成功后置 Loaded。</summary>
    public static async Task LoadAsync(ISettingsApi api)
    {
        var resp = await api.GetBehaviorsAsync().ConfigureAwait(true);
        if (!resp.Success || resp.Value is null) return;
        _builtin = resp.Value.Builtin;
        _user = resp.Value.User;
        Loaded = true;
    }

    // ------------------------------------------------------------- 覆盖推导

    /// <summary>
    /// 规则值集展开: textType 单值; fileExt 逗号分隔 (NormalizeExts 语义)。
    /// 条件值为空时按通用文件集处理 (与旧 FileActions 回退口径一致, 仅用于展示过滤)。
    /// </summary>
    public static List<string> RuleValues(string matchType, string matchValue)
    {
        if (string.Equals(matchType, "textType", StringComparison.OrdinalIgnoreCase))
        {
            var v = (matchValue ?? "").Trim().ToLowerInvariant();
            return v.Length == 0 ? [] : [v];
        }
        var exts = ActionSchemeCatalog.NormalizeExts(matchValue);
        return exts.Count == 0 ? ["*"] : exts;
    }

    /// <summary>行为是否覆盖规则前提: fileExt 要求规则值集 ⊆ 前提集 ("*" 覆盖任意)。</summary>
    public static bool Covers(BehaviorPack p, string matchType, List<string> values)
        => p.AppliesTo.Any(e => EntryCovers(e, matchType, values));

    private static bool EntryCovers(BehaviorAppliesTo e, string matchType, List<string> values)
    {
        if (!string.Equals(e.Type, matchType, StringComparison.OrdinalIgnoreCase)) return false;
        if (string.Equals(e.Type, "textType", StringComparison.OrdinalIgnoreCase))
        {
            return values.Count == 1 &&
                   string.Equals(e.Value?.Trim(), values[0], StringComparison.OrdinalIgnoreCase);
        }
        var packWildcard = e.Exts?.Any(x => x.Trim() == "*") == true;
        foreach (var v in values)
        {
            if (v == "*")
            {
                if (!packWildcard) return false; // 任意文件规则只能依赖通配前提
                continue;
            }
            if (!packWildcard && !(e.Exts?.Any(x => string.Equals(
                    x.Trim().Trim('.'), v, StringComparison.OrdinalIgnoreCase)) == true))
            {
                return false;
            }
        }
        return true;
    }

    /// <summary>
    /// 覆盖某前提的行为列表, 展示序: 专属前提 (非通配) 在前、通配在后, 各自保持目录序。
    /// 语义与旧静态词表的关键差异: 通配行为 (run/script 等) 对任意后缀规则恒适用 ——
    /// 旧「图片分组隐藏 run」属展示层过度收窄, 由 default 推荐引导替代硬过滤。
    /// </summary>
    public static List<BehaviorPack> Covering(string matchType, string matchValue)
    {
        var values = RuleValues(matchType, matchValue);
        var specific = new List<BehaviorPack>();
        var generic = new List<BehaviorPack>();
        foreach (var p in Packs)
        {
            if (!Covers(p, matchType, values)) continue;
            var isGeneric = p.AppliesTo.Any(e => string.Equals(e.Type, matchType, StringComparison.OrdinalIgnoreCase)
                                                 && e.Exts?.Contains("*") == true);
            (isGeneric ? generic : specific).Add(p);
        }
        return [.. specific, .. generic];
    }

    /// <summary>前提桶默认行为: default 标记优先 (目录序), 回退第一条覆盖包。</summary>
    public static string? DefaultFor(string matchType, string matchValue)
    {
        var values = RuleValues(matchType, matchValue);
        BehaviorPack? first = null;
        foreach (var p in Packs)
        {
            foreach (var e in p.AppliesTo)
            {
                if (!EntryCovers(e, matchType, values)) continue;
                if (e.Default) return p.Id;
                first ??= p;
                break;
            }
        }
        return first?.Id;
    }

    // ------------------------------------------------------------- 显示与语义

    /// <summary>行为显示名: 按语言取 name/nameEn, 未知 ID 回退原值 (脏值恒可见口径)。</summary>
    public static string LabelFor(string id)
    {
        var p = Find(id);
        if (p is null) return id;
        return I18n.Language == I18n.En && !string.IsNullOrEmpty(p.NameEn) ? p.NameEn : p.Name;
    }

    /// <summary>行为提示 (description; 缺失回退空串)。</summary>
    public static string HintFor(string id) => Find(id)?.Description ?? "";

    /// <summary>
    /// 基础动作语义集: 直接作用于选中内容、无命令模板的基础动作
    /// (引擎级知识, 对齐 AHK ExecuteActionRule 分支; 不随用户包增减)。
    /// </summary>
    public static readonly HashSet<string> BaseActionNoValue =
        ["open_url", "open_path", "open_folder", "magnet_download", "open_registry"];

    /// <summary>是否无参行为: 内置语义集, 或包入口未声明默认命令模板。</summary>
    public static bool IsNoValue(string id)
    {
        if (BaseActionNoValue.Contains(id)) return true;
        var p = Find(id);
        return p is not null && string.Equals(p.Entry.Kind, "builtin", StringComparison.OrdinalIgnoreCase)
               && string.IsNullOrEmpty(p.Entry.Params?.ActionValue);
    }

    /// <summary>切换到该行为时的默认命令模板 (包声明; 无则空串)。</summary>
    public static string DefaultTemplateFor(string id)
        => Find(id)?.Entry.Params?.ActionValue ?? "";

    /// <summary>行为展开后的基础动作 ID (内置 ID 直通; 用户包取 entry.action; 未知原样)。</summary>
    public static string BaseActionOf(string id)
    {
        if (BaseActionNoValue.Contains(id) || Packs.All(p => p.Id != id)) return id;
        var p = Find(id);
        return string.Equals(p?.Entry.Kind, "builtin", StringComparison.OrdinalIgnoreCase)
            ? p!.Entry.Action ?? id
            : id;
    }

    private static BehaviorPack? Find(string id) => Packs.FirstOrDefault(p => p.Id == id);

    // ------------------------------------------------------------- 测试种子

    /// <summary>仅供单元测试: 直接注入目录快照 (绕过后端拉取)。</summary>
    internal static void SeedForTests(IEnumerable<BehaviorPack> builtin, IEnumerable<BehaviorPack> user)
    {
        _builtin = [.. builtin];
        _user = [.. user];
        Loaded = true;
    }
}
