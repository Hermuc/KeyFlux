using Avalonia.Controls;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 插件页视图 (左侧导航「插件」): 内置插件「快速切换」现状 + 第三方插件市场未开放说明。
/// 内容全部由 <see cref="PluginsPageViewModel"/> 提供, 本视图不含业务逻辑。
/// </summary>
public partial class PluginsPageView : UserControl
{
    public PluginsPageView() => InitializeComponent();
}
