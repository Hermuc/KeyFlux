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
    /// 打开对话框 —— <b>先把设置项加载完, 再在"窗口整体透明"的状态下让尺寸落定, 最后显形</b>。
    /// 调用方据此替代裸的 <c>ShowDialog(owner)</c>。
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
    /// 布局落定         → 108   (表单未展开时的残缺高度; 真机更高)
    /// </code></para>
    ///
    /// <para><b>⚠ 为什么不用"手动 Measure 定尺寸" (第一版修法, 已废弃)。</b>
    /// 试过在 <c>Show</c> 前 <c>Measure/Arrange</c> 内容根取 <c>DesiredSize</c> 再设死
    /// <c>Height</c>。实测两处硬伤: ① 表单区由 <c>IsVisible="{Binding ShowForm}"</c> 驱动,
    /// <c>Show</c> 前绑定尚未把表单展开出来, 探针读到 <c>ScrollViewer.IsVisible=False</c>、
    /// <c>ItemsControl</c> 根本不存在 ⇒ 量出的高度 <b>严重偏小</b>; ② 据此设死 <c>Height</c>
    /// 会把表单<b>裁掉</b> (用户报障"弹窗显示不完整": 取消/保存按钮浮在输入框上且被底边切掉)。
    /// 结论: 内容由异步绑定驱动时, 手动测量不可靠。</para>
    ///
    /// <para><b>现在的修法: 把"尺寸落定"整段搬进窗口不可见的时期。</b>
    /// 顺序 —— ① 预加载 (<c>LoadAsync</c> 不触碰视觉树, 可在 <c>Show</c> 前调用);
    /// ② <c>Opacity = 0</c> (窗口级, 作用于整个窗口表面);
    /// ③ <c>ShowDialog</c> —— 窗口真的出现了, 但用户看不到; 此时 <c>SizeToContent</c>
    ///    与绑定会照常完成尺寸落定, <b>黑块与跳变全部发生在透明期</b>;
    /// ④ 等一轮布局排空 + <c>CenterToOwner</c> 定位到最终尺寸对应的位置;
    /// ⑤ <c>Opacity = 1</c> 显形 —— 此刻尺寸已稳定, 不再有任何 <c>SetWindowPos</c>。
    /// 窗口级 <c>Opacity</c> 与 <see cref="DialogMotion"/> 的 body 级 <c>Opacity</c> 互不干扰,
    /// 显形后 body 动效照常从起点播放入场。</para>
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

        // 全程不可见地完成尺寸落定 (黑块/跳变都在透明期发生)
        Opacity = 0;
        var shown = ShowDialog(owner);

        // 先让绑定与布局彻底排空 (SizeToContent 会在这里改到最终尺寸), 再定位与显形。
        // 用连续两跳: 第一跳让布局生效, 第二跳确保尺寸变更带来的重定位也已完成。
        await Dispatcher.UIThread.InvokeAsync(() => { }, DispatcherPriority.Background);
        await Dispatcher.UIThread.InvokeAsync(() => { }, DispatcherPriority.Background);
        Services.Win32.DialogPlacer.CenterToOwner(this);
        Opacity = 1;

        await shown;
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
