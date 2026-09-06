using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace KeyFlux.Settings.Services;

// ============================================================================
// 双语资源方案 (复刻 config-ui/src/store/language-map.ts 的 label:NNN 机制)
// 溯源: config-ui/ 是历史来源 (Vue 旧前端), 已从 git 与磁盘完全消失, 不在本仓库。
//
// Vue 侧: translate('label:501') -> languageMap[501][options.language] ?? en
// Avalonia 侧: I18n.T("501") -> Map[key][Language] ?? Map[key]["en"] ?? key
//
// 语言切换实时生效:
//   - 单例标签: AXAML 里 Text="{Binding LanguageTick, Converter={StaticResource Tr},
//     ConverterParameter=501}", 语言变化时 ViewModel 递增 LanguageTick 触发重新求值;
//   - 列表项标签: ViewModel 预先翻译进条目模型, 语言变化时重建标签文本。
//
// 文案数据源: Resources/i18n.json (松散文件, 由 csproj 的 Content 项复制到
//   build 输出与 publish 输出的 Resources/ 子目录)。刻意不用 avares:// ——
//   Avalonia 的 AssetLoader 每次调用都现取 AvaloniaLocator 服务, 只有已 Build
//   的 Avalonia 应用里才可用; 而 KeyFlux.Settings.Tests 是纯 xunit 宿主,
//   守卫测试 I18nResourceTests 必须能在无 Avalonia 应用的前提下读到文案表。
//   JSON 形状: { "<key>": { "zh": <string|null>, "en": <string|null> }, ... }
//   zh=null 表示该键无中文 (T() 回退英文); en="" 与 en=null 在 T() 里同样触发
//   回退, 但 JSON 必须 1:1 忠实保留原字典的 null / "" 区别 (见守卫测试)。
//
// 键段索引 (原字典的分节注释; JSON 承载不了注释, 迁移到此):
//   1-16 window | 17-24,2401-2402 system | 2404-2407 总览页 (Home) 编辑
//   25-37 mouse | 38-70 text (62-70 为英文专属键名, zh=null) | 71-78 KeyFlux
//   200-209 Action types | 301-309 App Launcher (含 301err/301hint 两个非数字键)
//   401-406 Other | 501-507 Settings | 601-612 Window Groups
//   701-715 Mouse Options | 721-726 Keyboard Layout | 741-755 Command Window
//   761-763 Modifer Delay | 781 切换语言
//   901-957 Avalonia 移植新增文案 (hideMatrix / pathVariables / 帮助页 等;
//            Vue 侧对应控件被注释隐藏, 移植时按任务规格补齐)
//   959-978 选中动作模块共用文案 (960 说明条 / 962 热键回显 / 964-965 开关 / 967-970 删除确认
//            / 976-978 热键警示与优先级说明; 2026-09 方案 D 后不再有方案名/多方案)
//   987-999 模拟测试条 (988-997 含; 999 条件值未设置回显)
//   1004-1018 映射编辑标签 (匹配类型/条件值/分组快捷填入/行为下拉/模板/工作目录/Options)
//   1026-1027 HotkeyCapture | 1031-1035 匹配类型词表标签与提示
//            (对应 constants.ts 的 MATCH_TYPES / TEXT_TYPES)
//   1059-1063 文本特征词标签 + 快捷键标签 | 1077 热键未保存提示 | 1078-1079 保存后重启失败提示
//   1080-1082 窗口拾取准星 WindowPickButton (按钮 ToolTip / 非提权识别失败提示 /
//            首次无窗口或命中自身进程提示, M3 不再静默)
//   1083-1104 行为库窗口 (BehaviorLibraryWindow, CONTRACTS §3.9; 含 1101_applied/
//            1102_deleted/1103_only/1104_any 等带后缀键)
//   1105-1114 选中动作单屏页 (方案 D, 2026-09): 添加映射/添加行为/上限提示/保留约束/
//            删除映射确认/菜单键位/弹窗说明/空状态
//
// 墓碑 (已删键的来历, 勿复用这些号段):
//   1064/1065/1067~1076 (按键写法说明卡) 已随卡片移除而删除, 内容迁入
//     config_doc.md「 ⌨️ 按键写法说明 」小节;
//   1066 组合示例早前已随 XAML 删除;
//   958 曾用于按键写法说明卡的『更多特殊按键参考』链接, 随卡片移除一并删除
//     (功能由 config_doc.md 外链承接);
//   959/961/963/968-969/971-975/977/980-986/1000-1003/1019-1024/1028 已随
//     2026-09 选中动作方案 D 重构删除 (多方案卡片列表/方案名/导入导出/两级导航);
//     1025 (热键冲突提示) 为保留控件 HotkeyCapture 持续引用, 不在删除之列;
//     页面语义由 1105-1114 与保留键承接; 1037-1058 旧静态行为词表已由
//     行为包 GET /api/behaviors 的 name/nameEn 取代 (CONTRACTS §3.9)。
// ============================================================================

/// <summary>Resources/i18n.json 的单条记录 (DTO)。</summary>
/// <remarks>
/// System.Text.Json 不支持 ValueTuple 反序列化, 故用 record 中转,
/// 加载后再转成 <see cref="I18n"/> 内部沿用的 (Zh, En) 元组字典 —— T() 的逻辑一行都不用改。
/// </remarks>
internal sealed record I18nEntry(
    [property: JsonPropertyName("zh")] string? Zh,
    [property: JsonPropertyName("en")] string? En);

/// <summary>静态双语服务: 全局当前语言 + 文案表 + 翻译查询。</summary>
public static class I18n
{
    public const string Zh = "zh";
    public const string En = "en";

    /// <summary>文案表相对目录 (输出目录下的散部署物)。</summary>
    private const string ResourceDir = "Resources";

    /// <summary>文案表文件名。</summary>
    private const string ResourceFile = "i18n.json";

    private static string _language = Zh;

    /// <summary>当前语言 ("zh"/"en")。赋值触发 <see cref="Changed"/>。</summary>
    public static string Language
    {
        get => _language;
        set
        {
            var normalized = string.Equals(value, Zh, StringComparison.OrdinalIgnoreCase) ? Zh : En;
            if (_language == normalized) return;
            _language = normalized;
            Changed?.Invoke();
        }
    }

    /// <summary>语言变化事件 (ViewModel 订阅后递增 LanguageTick 以刷新绑定)。</summary>
    public static event Action? Changed;

    /// <summary>
    /// 翻译。key 可带 "label:" 前缀 (与 Vue comment 兼容)。
    /// 查不到当前语言时回退英文, 再查不到原样返回。
    /// </summary>
    public static string T(string? key)
    {
        if (string.IsNullOrEmpty(key)) return "";
        var k = key.StartsWith("label:", StringComparison.Ordinal) ? key["label:".Length..] : key;
        if (Map.TryGetValue(k, out var pair))
        {
            var text = _language == Zh ? pair.Zh : pair.En;
            if (!string.IsNullOrEmpty(text)) return text;
            // 回退: 目标语言缺失时用另一语言
            text = _language == Zh ? pair.En : pair.Zh;
            if (!string.IsNullOrEmpty(text)) return text;
        }
        return k;
    }

    /// <summary>把 <see cref="Language"/> 设为 config 里的 language 字段值 (不触发 UI 时可直接调)。</summary>
    public static void ApplyConfigLanguage(string? configLanguage)
    {
        Language = string.Equals(configLanguage, En, StringComparison.OrdinalIgnoreCase) ? En : Zh;
    }

    /// <summary>已加载的文案键数量 (供守卫测试对账, 不参与 UI 逻辑)。</summary>
    internal static int KeyCount => Map.Count;

    private static readonly Dictionary<string, (string? Zh, string? En)> Map = Load();

    /// <summary>
    /// 载入文案表。<b>永不抛异常</b>: Map 是静态字段初始化器, 一旦抛出会变成
    /// TypeInitializationException 毒化全部消费者。三级探测 (见 <see cref="Locate"/>)
    /// 全失败时返回空表, 此时 T() 退化为回显键号 —— 界面立刻可见异常但不崩溃。
    /// </summary>
    private static Dictionary<string, (string? Zh, string? En)> Load()
    {
        var empty = new Dictionary<string, (string? Zh, string? En)>();
        try
        {
            var path = Locate();
            if (path is null)
            {
                Debug.WriteLine($"[I18n] 未找到 {ResourceDir}/{ResourceFile}, 文案表为空 (T() 将回显键号)。");
                return empty;
            }

            // 独立的 JsonSerializerOptions: 不得复用 SettingsJson.Options ——
            // 后者设了 DefaultIgnoreCondition.WhenWritingNull, 会抹掉 null 与 "" 的区别。
            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true,
                AllowTrailingCommas = true,
                ReadCommentHandling = JsonCommentHandling.Skip,
            };
            // 走字符串重载: File.ReadAllText 能吃 BOM, Deserialize(Stream) 遇 BOM 会解析失败。
            var dto = JsonSerializer.Deserialize<Dictionary<string, I18nEntry?>>(File.ReadAllText(path), options);
            if (dto is null)
            {
                Debug.WriteLine($"[I18n] {path} 反序列化为 null, 文案表为空。");
                return empty;
            }

            var map = new Dictionary<string, (string? Zh, string? En)>(dto.Count);
            foreach (var (key, entry) in dto) map[key] = (entry?.Zh, entry?.En);
            return map;
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[I18n] 载入文案表失败, 退化为空表: {ex}");
            return empty;
        }
    }

    /// <summary>
    /// 三级探测: ① 输出目录 Resources/i18n.json (正常 build / publish 布局);
    /// ② 从输出目录逐级向上找源码树 config-ui-avalonia/Resources/i18n.json
    ///    (兜住 Content 项未流转到某个宿主输出目录的情形); ③ 都找不到返回 null。
    /// </summary>
    private static string? Locate()
    {
        var baseDir = AppContext.BaseDirectory;
        var primary = Path.Combine(baseDir, ResourceDir, ResourceFile);
        if (File.Exists(primary)) return primary;

        const int maxUpwardDepth = 12;
        var dir = new DirectoryInfo(baseDir);
        for (var depth = 0; dir is not null && depth < maxUpwardDepth; depth++, dir = dir.Parent)
        {
            var fromSourceTree = Path.Combine(dir.FullName, "config-ui-avalonia", ResourceDir, ResourceFile);
            if (File.Exists(fromSourceTree)) return fromSourceTree;
        }
        return null;
    }
}
