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
    /// 打开对话框 —— <b>先把设置项加载完、把窗口高度量准, 再让窗口可见</b>; 调用方据此
    /// 替代裸的 <c>ShowDialog(owner)</c>。
    ///
    /// <para><b>⚠ 为什么不能"先 Show 再在 Opened 里 LoadAsync" (2026-09-24 用户报障
    /// "打开弹窗一瞬间出现大片未绘制的黑块")。</b>本窗口是
    /// <c>SizeToContent="Height"</c> + <c>MaxHeight="620"</c>, 设置项要等 <c>LoadAsync</c>
    /// 走完一次后端往返才知道有几行。窗口一旦以某个尺寸上屏, 之后内容变化会让
    /// <c>SizeToContent</c> 调 <c>SetWindowPos</c> 改窗口大小, 而<b>新暴露的区域在首帧绘制
    /// 完成前是未绘制的窗口表面 (清屏色, 观感即黑块)</b>; <c>DialogPlacer</c> 再按新尺寸挪位,
    /// 叠加成"跳一下"。headless 探针实测尺寸轨迹 (520 宽固定, 高度):
    /// <code>
    /// Show()          → 620   (SizeToContent 先按 MaxHeight 上屏)
    /// Opened          → 620
    /// 布局落定         → 108   (只有 4 行设置项时的真实高度)
    /// </code></para>
    ///
    /// <para><b>修法: 让窗口第一次可见时尺寸就是最终值, 全程不触发 SetWindowPos。</b>
    /// 两步 ——<br/>
    /// ① <b>预加载</b>: <see cref="PluginSettingsDialogViewModel.LoadAsync"/> 只做一次后端请求
    ///    与填充 <c>Rows</c>, <b>不触碰视觉树</b> (无 TopLevel/Screens/Dispatcher 依赖), 故可在
    ///    <c>Show</c> 之前调用。<br/>
    /// ② <b>预测量</b>: 布局在 <c>Show</c> 前不会自动跑, 但可以手动
    ///    <c>Measure</c>/<c>Arrange</c> 内容根拿到真实 <c>DesiredSize</c>
    ///    (探针实测: <c>Measure(520, ∞) -&gt; 172 x 108</c>, 与最终落定高度 <b>108 完全一致</b>)。
    ///    据此显式设 <c>Height</c> 并清掉 <c>SizeToContent</c> —— 窗口首帧即终态尺寸。
    ///    保留 <c>MaxHeight</c> 语义: 内容超高时取 620 封顶 (此时由 ScrollViewer 滚动)。</para>
    ///
    /// <para>代价仅是"弹窗晚一个请求往返出现" —— 本地回环请求通常 &lt;50ms, 用户不可感知,
    /// 远优于必现的黑色撕裂。</para>
    ///
    /// <para>加载失败不阻断显示: VM 的 <c>LoadError</c> 会把原因呈现在窗口里 (与旧行为一致)。</para>
    /// </summary>
    public async Task ShowDialogWhenReadyAsync(Window owner)
    {
        if (DataContext is PluginSettingsDialogViewModel vm)
        {
            Title = vm.DisplayName;
            try { await vm.LoadAsync(); }
            catch { /* 加载失败已由 VM 的 LoadError 呈现 */ }
        }

        SettleHeightBeforeShow();
        await ShowDialog(owner);
    }

    /// <summary>
    /// 显示前把窗口高度量准并固化 —— 消掉 <c>SizeToContent</c> 上屏后的尺寸跳变
    /// (黑块根因, 见 <see cref="ShowDialogWhenReadyAsync"/> 注释)。
    ///
    /// <para>算法: 让内容根以窗口宽度手动跑一次 Measure/Arrange, 取其 <c>DesiredSize.Height</c>
    /// 作为目标高度, 再用 <c>MaxHeight</c> 封顶 (NaN 视为不限), 最后设 <c>Height</c> 并清空
    /// <c>SizeToContent</c>。清理 <c>SizeToContent</c> 是必须的: 否则窗口显示后内容一旦再变,
    /// 又会回到"改尺寸 ⇒ 新区域未绘制"的老路。</para>
    ///
    /// <para>无法测量时 (Content 非 Control / 宽度未定) 原样返回, 退化为旧行为 —— 宁可维持
    /// 原观感也不要把窗口尺寸设成 0。</para>
    /// </summary>
    private void SettleHeightBeforeShow()
    {
        if (Content is not Control content)
        {
            return;
        }

        var width = double.IsNaN(Width) ? 0 : Width;
        if (width <= 0)
        {
            return;
        }

        content.Measure(new Size(width, double.PositiveInfinity));
        var desired = content.DesiredSize.Height;
        if (desired <= 0 || double.IsNaN(desired) || double.IsInfinity(desired))
        {
            return;
        }

        var target = double.IsNaN(MaxHeight) ? desired : Math.Min(desired, MaxHeight);
        content.Arrange(new Rect(0, 0, width, target));

        Height = target;
        SizeToContent = SizeToContent.Manual;
    }

    /// <summary>测试缝: 从测试工程触发"显示前定尺寸"(等价于 <see cref="ShowDialogWhenReadyAsync"/>
    /// 在 <c>ShowDialog</c> 之前执行的那一步, 但不等后端)。</summary>
    internal void SettleHeightBeforeShowForProbe() => SettleHeightBeforeShow();

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
