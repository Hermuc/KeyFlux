using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 用户插件设置对话框 (插件页点击卡片打开; 内置 QuickSwitch 走 QuickSwitchDialogWindow)。
/// 表单由 manifest.settings 驱动, 逻辑全在 <see cref="PluginSettingsDialogViewModel"/>;
/// 本文件只保留必须由视图承担的两件事: 窗口外观钩子与文件选择器 (需 StorageProvider)。
/// </summary>
public partial class PluginSettingsDialogWindow : Window
{
    private bool _saving;

    /// <summary>保存成功后置 true (供调用方判断是否需要刷新页面状态)。</summary>
    public bool Saved { get; private set; }

    public PluginSettingsDialogWindow()
    {
        InitializeComponent();
        Services.Win32.DialogChrome.Apply(this);
        TitleBarIconSuppressor.Attach(this);
        I18n.Changed += OnLanguageChanged;
        Closed += (_, _) => I18n.Changed -= OnLanguageChanged;
        // SizeToContent=Height + CenterOwner 的组合缺陷: 打开瞬间按「未加载表单的小高度」
        // 居中, LoadAsync 之后窗口向下长高、锚点不动 ⇒ 弹窗严重偏下 (2026-09-23 用户报障)。
        // 对策: 统一定位模块 (DialogPlacer) —— 加载后的尺寸变化自动重居中。
        Services.Win32.DialogPlacer.AttachAutoCenter(this);
    }

    private void OnLanguageChanged()
    {
        if (DataContext is PluginSettingsDialogViewModel vm) vm.OnLanguageChanged();
    }

    /// <summary>
    /// 显示前的准备 —— 预加载设置项, 并把窗口尺寸<b>在首次可见之前锚定</b>。
    ///
    /// <para><b>⚠ 尺寸跳变/黑块的三种写法 (2026-09-24, 前两种均被用户报障证伪, 勿重蹈)。</b>
    /// 本窗口是 <c>SizeToContent="Height"</c> + <c>MaxHeight="620"</c>, 设置项要等一次后端
    /// 往返才知道有几行。窗口内容变化会让 <c>SizeToContent</c> 调 <c>SetWindowPos</c> 改尺寸,
    /// 而<b>新暴露区域在首帧绘制完成前是未绘制的窗口表面 (观感即黑块)</b>。</para>
    ///
    /// <para><b>写法 A: "Show 前量一次, 设死 Height + 清 SizeToContent" —— 失败。</b>
    /// 在 <c>Rows</c> 还空时量: 表单区由 <c>IsVisible="{Binding ShowForm}"</c> 驱动,
    /// 此刻 <c>ScrollViewer.IsVisible=False</c>、<c>ItemsControl</c> 不存在 ⇒ 量到的是
    /// "标题 + 空态"的残缺值, 设死后把表单<b>裁掉</b> (用户报障"显示不完整")。
    /// 教训: <b>量得准的前提是数据先就绪</b>, 而不是 Measure 本身不可用。</para>
    ///
    /// <para><b>写法 B: "窗口级 Opacity=0 → ShowDialog → 等几跳 → Opacity=1" —— 失败。</b>
    /// 实测更糟: 窗口全透明期间合成器不绘制内容, 恢复 <c>Opacity=1</c> 时布局/绘制尚未就绪
    /// ⇒ 窗口以 <b>620 高的持续黑块</b>呈现 (用户报障, 比原先"一闪"更严重)。</para>
    ///
    /// <para><b>写法 C: "数据就绪后 Measure" —— 也不可行。</b>真机探针实测: <b>窗口首次
    /// <c>Show</c> 之前, <c>ScrollViewer</c> 的内容 <c>ItemsControl</c> 从未进入视觉树</b>
    /// (<c>IC=无</c>、<c>TextBox=0</c>), 无论 <c>Rows</c> 有几行, 量出的高恒为 <c>~107</c>。
    /// 这是 Avalonia 窗口管线固有的 (项容器要等首个渲染周期), 显式
    /// <c>ApplyTemplate</c>/<c>Arrange</c> 都无法绕过。</para>
    ///
    /// <para><b>写法 D (现行): "首帧即终帧" —— 不量高度, 把窗口锚定到 <c>MaxHeight</c>。</b>
    /// 黑块的成因是"<b>首帧尺寸 ≠ 后续尺寸</b>": <c>SizeToContent</c> 让窗口以 <c>MaxHeight</c>
    /// 建立表面, 布局落定后再 <c>SetWindowPos</c> 缩小, 新暴露区域未绘制。既然量不准真实高度,
    /// 那就让<b>第一帧就已经是最终尺寸</b> —— 见 <see cref="DialogMotion.ReserveHeightBeforeShow"/>
    /// (设 <c>Height = MaxHeight</c> + <c>SizeToContent = Manual</c>, 全程不透明)。
    /// 真机实测: <c>SizeChanged: 0x0 -> 520x620</c> 单条, 无 620→130 塌缩。
    /// 入场结束后 <see cref="DialogMotion"/> 再把 <c>SizeToContent</c> 还回去, 窗口收到真实高度
    /// (那时内容已绘制完成, 收缩不产生黑块)。</para>
    ///
    /// <para>加载失败不阻断显示: VM 的 <c>LoadError</c> 会把原因呈现在窗口里, 此时表单为空、
    /// 锚定照常进行 (黑块与数据无关)。</para>
    /// </summary>
    /// <returns>是否成功锚定尺寸 (false = 退化为 SizeToContent 自然工作, 仍可显示)。</returns>
    public async Task<bool> PrepareForShowBeforeShowAsync()
    {
        if (DataContext is PluginSettingsDialogViewModel vm)
        {
            Title = vm.DisplayName;
            try
            {
                if (_loadStep is { } step)
                {
                    await step();
                }
                else
                {
                    await vm.LoadAsync();
                }
            }
            catch { /* 加载失败已由 VM 的 LoadError 呈现 */ }
        }

        // 数据是否就绪不再影响本步: 现行方案不量内容高, 只把窗口锚定到 MaxHeight。
        // 预加载仍保留 —— 让入场的弹簧动画期间内容就是最终形态, 不出现"动画中长高"。
        return DialogMotion.ReserveHeightBeforeShow(this);
    }

    /// <summary>
    /// 加载步骤 —— 抽成方法以便测试替换 (VM 是密封类, 无法用子类桩注入"数据已就绪"状态)。
    /// 生产路径就是 <see cref="PluginSettingsDialogViewModel.LoadAsync"/>。
    /// </summary>
    private Func<Task>? _loadStep;

    /// <summary>
    /// 替换加载步骤的测试缝 (public 而非 internal —— 真机探针工程不是 friend assembly,
    /// 而"有数据时窗口首帧是否稳定"恰恰只能在真机验证, 不能只靠 headless)。
    /// </summary>
    public void SetLoadStepForProbe(Func<Task>? step) => _loadStep = step;

    /// <summary>
    /// 打开对话框 (完整流程) —— 先锚定尺寸再显示, 消除首帧与最终尺寸不一致造成的黑块。
    /// </summary>
    public async Task ShowDialogWhenReadyAsync(Window owner)
    {
        await PrepareForShowBeforeShowAsync();
        await ShowDialog(owner);
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e) => Close();

    private async void OnSaveClick(object? sender, RoutedEventArgs e)
    {
        if (_saving) return;
        _saving = true;
        try
        {
            if (DataContext is PluginSettingsDialogViewModel vm && await vm.SaveAsync())
            {
                Saved = true;
                Close();
            }
            // 保存失败时 SaveAsync 内部已弹出原因, 窗口保持打开供修正
        }
        finally
        {
            _saving = false;
        }
    }

    /// <summary>
    /// 文件类设置项的「浏览」: 选一个文件, 把绝对路径写回该行的值。
    /// 过滤名取自 manifest 的 filter 字段 (如 "everything.exe"); 为空则不过滤 ——
    /// 刻意不强制后缀: 用户可能想指向一个改过名的可执行文件, 由插件在运行期判定可达性。
    /// </summary>
    private async void OnBrowseClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: PluginSettingRowVm row } button) return;
        if (TopLevel.GetTopLevel(this) is not Window owner) return;

        var filter = row.Setting.Filter;
        var options = new FilePickerOpenOptions
        {
            Title = I18n.T("2583"),
            AllowMultiple = false,
            FileTypeFilter = string.IsNullOrWhiteSpace(filter)
                ? null
                : [new FilePickerFileType(filter!) { Patterns = [$"*{ExtOf(filter!)}"] }],
        };
        var files = await owner.StorageProvider.OpenFilePickerAsync(options);
        if (files.Count == 0) return;

        // TryGetLocalPath 对非本地 (云盘/虚拟) 条目返回 null —— 那种路径引擎侧也读不了,
        // 直接忽略而不是写入一个插件永远打不开的 URI。
        var path = files[0].TryGetLocalPath();
        if (!string.IsNullOrEmpty(path)) row.Value = path;
        button.Focus();
    }

    /// <summary>从过滤名里截出后缀 ("everything.exe" -> ".exe"); 无后缀时给出不过滤的兜底。</summary>
    private static string ExtOf(string filter)
    {
        var dot = filter.LastIndexOf('.');
        return dot >= 0 ? filter[dot..] : filter;
    }
}
