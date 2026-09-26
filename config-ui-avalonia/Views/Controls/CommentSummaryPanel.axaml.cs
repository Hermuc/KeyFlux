using System.Collections;
using Avalonia;
using Avalonia.Controls;

namespace KeyFlux.Settings.Views.Controls;

/// <summary>
/// 备注汇总控件 (复刻 ActionCommentTable): 标题 + 可独立滚动的条目列表。
/// KeymapPageView 与 AbbrPageView 共用; 使用方经 ItemsSource / LanguageTick
/// 两个 StyledProperty 提供数据, 控件不感知具体页面 VM (两页 VM 均有同名属性)。
/// </summary>
public partial class CommentSummaryPanel : UserControl
{
    /// <summary>备注条目集合 (元素为 CommentEntryVm)。</summary>
    public static readonly StyledProperty<IEnumerable?> ItemsSourceProperty =
        AvaloniaProperty.Register<CommentSummaryPanel, IEnumerable?>(nameof(ItemsSource));

    /// <summary>语言切换计数 (标题文案经 Tr 转换器随计数重算)。</summary>
    public static readonly StyledProperty<int> LanguageTickProperty =
        AvaloniaProperty.Register<CommentSummaryPanel, int>(nameof(LanguageTick));

    public IEnumerable? ItemsSource
    {
        get => GetValue(ItemsSourceProperty);
        set => SetValue(ItemsSourceProperty, value);
    }

    public int LanguageTick
    {
        get => GetValue(LanguageTickProperty);
        set => SetValue(LanguageTickProperty, value);
    }

    public CommentSummaryPanel() => InitializeComponent();
}
