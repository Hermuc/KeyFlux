using CommunityToolkit.Mvvm.ComponentModel;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 导航条目 (复刻 NavigationDrawer.vue 的条目模型):
/// 标题 + MDI 图标 (图标语义见 <see cref="MdiIcon"/>) + 目标页。<br/>
/// 图标配色不在此承载: 单色 + 选中强调由 MainWindow.axaml 的样式提供 (见 <see cref="NavBadge"/> 说明)。
/// </summary>
public sealed partial class NavItem : ObservableObject
{
    /// <summary>稳定标识 (重建导航时保持选中态): "home" / "action" / "keymap-{id}"。</summary>
    public required string Id { get; init; }

    /// <summary>显示标题 (keymap 条目为 config 里的 keymap.name, 数据驱动)。</summary>
    public required string Title { get; init; }

    /// <summary>MDI 图标名 (复刻 NavigationDrawer.vue 的 getIcon 语义, 如 "gesture-tap" / "cog-outline")。</summary>
    public string IconName { get; init; } = "";

    /// <summary>MDI 图标渲染字符 (由 <see cref="IconName"/> 查表得出, 供 XAML 绑定)。</summary>
    public string IconChar => string.IsNullOrEmpty(IconName) ? "" : MdiIcon.CharFor(IconName);

    /// <summary>点击后展示的内容区页面对象 (各页面 ViewModel)。</summary>
    public required object Page { get; init; }
}

/// <summary>
/// 导航徽标配色 —— **已废止**: 原 Vue 版按热键哈希给每个条目分配高饱和 Material 色
/// (pink/blue/purple/deep-orange), 违反 DESIGN.md「不要引入 Terracotta 之外的饱和色」。
/// 现改为导航整体单色 (Stone Gray), 选中态由 MainWindow.axaml 的
/// <c>ListBoxItem:selected TextBlock.navIcon</c> 样式接管 Terracotta 强调。
/// <para>
/// 之所以由样式而非 VM 提供配色: 控件上的 <c>Foreground="{Binding ...}"</c> 是**本地值**,
/// 而本地值优先级高于样式 Setter —— 只要保留绑定, 选中态样式就永远无法覆盖。
/// 故 NavItem 不再暴露颜色属性。
/// </para>
/// </summary>
public static class NavBadge
{
    /// <summary>父键优先: 有 parentID 时展示父 keymap 的热键 (复刻 Vue 版 getHotkey)。</summary>
    public static string EffectiveHotkey(Models.Keymap keymap, IReadOnlyList<Models.Keymap> allKeymaps)
    {
        if (keymap.ParentId != 0)
        {
            var parent = allKeymaps.FirstOrDefault(k => k.Id == keymap.ParentId);
            if (parent is not null) return parent.Hotkey;
        }
        return keymap.Hotkey;
    }
}

/// <summary>
/// MDI 图标: 图标名 -> 字形码位 (Material Design Icons 7.0.96, 仅收录导航用到的图标),
/// 以及复刻 NavigationDrawer.vue getIcon 的热键 -> 图标名语义。
/// </summary>
public static class MdiIcon
{
    // 图标名 -> Unicode 码位 (materialdesignicons.ttf, 提取自 @mdi/font css;
    // 码位超出 BMP (U+F0000+), 渲染时用 char.ConvertFromUtf32 转代理对)
    private static readonly Dictionary<string, int> Codepoints = new()
    {
        { "gesture-tap", 0xF0741 },
        { "home-outline", 0xF06A1 },
        { "cog-outline", 0xF08BB },
        { "keyboard-outline", 0xF097B },
        { "rocket-launch-outline", 0xF14DF },
        { "format-text-variant-outline", 0xF150F },
        { "cursor-default-outline", 0xF01BF },
        { "play", 0xF040A },
        { "rhombus", 0xF070B },
        { "content-save-outline", 0xF0818 },
        // 插件页 (Plugins) 导航图标; 码位直接实测自内嵌 materialdesignicons.ttf 的 post/cmap 表
        { "puzzle-outline", 0xF0A66 },
        { "alpha-a-box", 0xF0B08 }, { "alpha-b-box", 0xF0B09 },
        { "alpha-c-box", 0xF0B0A }, { "alpha-d-box", 0xF0B0B },
        { "alpha-e-box", 0xF0B0C }, { "alpha-f-box", 0xF0B0D },
        { "alpha-g-box", 0xF0B0E }, { "alpha-h-box", 0xF0B0F },
        { "alpha-i-box", 0xF0B10 }, { "alpha-j-box", 0xF0B11 },
        { "alpha-k-box", 0xF0B12 }, { "alpha-l-box", 0xF0B13 },
        { "alpha-m-box", 0xF0B14 }, { "alpha-n-box", 0xF0B15 },
        { "alpha-o-box", 0xF0B16 }, { "alpha-p-box", 0xF0B17 },
        { "alpha-q-box", 0xF0B18 }, { "alpha-r-box", 0xF0B19 },
        { "alpha-s-box", 0xF0B1A }, { "alpha-t-box", 0xF0B1B },
        { "alpha-u-box", 0xF0B1C }, { "alpha-v-box", 0xF0B1D },
        { "alpha-w-box", 0xF0B1E }, { "alpha-x-box", 0xF0B1F },
        { "alpha-y-box", 0xF0B20 }, { "alpha-z-box", 0xF0B21 },
        { "numeric-0-box-outline", 0xF03A3 },
        { "numeric-1-box-outline", 0xF03A6 },
        { "numeric-2-box-outline", 0xF03A9 },
        { "numeric-3-box-outline", 0xF03AC },
        { "numeric-4-box-outline", 0xF03AE },
        { "numeric-5-box-outline", 0xF03B0 },
        { "numeric-6-box-outline", 0xF03B5 },
        { "numeric-7-box-outline", 0xF03B8 },
        { "numeric-8-box-outline", 0xF03BB },
        { "numeric-9-box-outline", 0xF03BE },
    };

    /// <summary>图标名 -> 渲染字符 (未收录时回退菱形)。</summary>
    public static string CharFor(string iconName) =>
        Codepoints.TryGetValue(iconName, out var cp) ? char.ConvertFromUtf32(cp) : char.ConvertFromUtf32(0xF070B);

    /// <summary>
    /// 热键 -> MDI 图标名, 复刻 NavigationDrawer.vue 的 getIcon 语义:
    /// settings→cog-outline, customHotkeys→keyboard-outline, capslockAbbr→rocket-launch-outline,
    /// semicolonAbbr→format-text-variant-outline, button→cursor-default-outline,
    /// 数字→numeric-N-box-outline, 字母→alpha-X-box, 其余→rhombus。
    /// </summary>
    public static string IconFor(string hotkey)
    {
        switch (hotkey)
        {
            case "settings": return "cog-outline";
            case "customHotkeys": return "keyboard-outline";
            case "capslockAbbr": return "rocket-launch-outline";
            case "semicolonAbbr": return "format-text-variant-outline";
        }
        if (hotkey.Contains("button", StringComparison.OrdinalIgnoreCase)) return "cursor-default-outline";

        // 去除开头的非修饰符/非字母数字字符 (复刻 hotkey.replace(/^[^!#^+\w]/, ''))
        var h = hotkey;
        if (h.Length > 0 && "!#^+".IndexOf(h[0]) < 0 && !char.IsAsciiLetterOrDigit(h[0]) && h[0] != '_')
            h = h[1..];

        // 首字符 (L/R 前缀的按键取第二个字符, 复刻 getIcon)
        var key = h.Length > 0 ? h[..1] : "";
        if (key.Length > 0 && "LRlr".Contains(key[0]) && h.Length > 1)
            key = h[1..2];
        key = key.ToLowerInvariant();

        // 非字母数字或修饰符 → rhombus
        if (key.Length == 0 || !(char.IsAsciiLetterOrDigit(key[0]) || "!#^+".Contains(key[0])))
            return "rhombus";

        // 数字 → numeric-N-box-outline
        if (char.IsAsciiDigit(key[0])) return $"numeric-{key}-box-outline";

        // 修饰键符号映射为字母 (复刻 getIcon: !→a, #→w, ^→c, +→s)
        key = key switch
        {
            "!" => "a",
            "#" => "w",
            "^" => "c",
            "+" => "s",
            _ => key,
        };
        return $"alpha-{key}-box";
    }
}
