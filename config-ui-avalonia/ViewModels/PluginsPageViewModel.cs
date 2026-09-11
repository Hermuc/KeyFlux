using CommunityToolkit.Mvvm.ComponentModel;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 插件页 (左侧导航「插件」)。诚实最小版 —— 阶段 0 实测: KeyFlux 插件生态「契约层完成度高、
/// 运行时约 20-30%」, 不足以支撑插件市场, 故本页不做「可安装插件列表」的假象:
///   - 内置插件: 仅列「快速切换 (QuickSwitch)」一行, 展示名称/说明/当前启用状态/配置指引;
///   - 第三方插件: 明确告知「插件市场尚未开放」+ 原因 (契约层已就绪, 运行时接口未完成)。
/// 读 Config.Options.QuickSwitch 展示状态; 点击卡片经 QuickSwitchDialogWindow 编辑配置并落盘。
/// </summary>
public sealed partial class PluginsPageViewModel : ObservableObject
{
    private readonly MainViewModel _main;

    /// <summary>主 VM (打开 QuickSwitch 配置对话框等场景使用)。</summary>
    public MainViewModel Main => _main;

    public PluginsPageViewModel(MainViewModel main) => _main = main;

    /// <summary>语言切换递增, 驱动页面静态文案 (XAML 的 Tr 绑定) 重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>
    /// 内置插件「快速切换」当前是否启用: 读 config.options.quickSwitch.collectEnabled。
    /// Config 经 ConfigReadDefaults.Apply 补齐, Options.QuickSwitch 恒非 null;
    /// 仍以 ?. 兜底, 避免配置尚未加载 (Config 为 null) 时抛异常。
    /// </summary>
    public bool QuickSwitchEnabled => _main.Config?.Options.QuickSwitch.CollectEnabled ?? false;

    /// <summary>内置插件启用状态文案 (2423 已启用 / 2424 已停用)。</summary>
    public string QuickSwitchStatusText => QuickSwitchEnabled ? I18n.T("2423") : I18n.T("2424");

    /// <summary>语言切换: 刷新静态文案与状态文案 (由 MainViewModel 分发)。</summary>
    public void OnLanguageChanged()
    {
        LanguageTick++;
        Refresh();
    }

    /// <summary>配置保存/导航重建后刷新启用状态 (由 MainViewModel.BuildNav 调用)。</summary>
    public void Refresh()
    {
        OnPropertyChanged(nameof(QuickSwitchEnabled));
        OnPropertyChanged(nameof(QuickSwitchStatusText));
    }
}
