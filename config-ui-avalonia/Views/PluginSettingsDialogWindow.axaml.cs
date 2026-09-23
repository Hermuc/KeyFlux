using System;
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
        Opened += OnOpened;
        // SizeToContent=Height + CenterOwner 的组合缺陷: 打开瞬间按「未加载表单的小高度」
        // 居中, LoadAsync 之后窗口向下长高、锚点不动 ⇒ 弹窗严重偏下 (2026-09-23 用户报障)。
        // 对策: 内容加载完毕与后续尺寸变化 (如语言切换) 时相对宿主重新居中。
        SizeChanged += (_, _) => CenterToOwner();
    }

    private void OnLanguageChanged()
    {
        if (DataContext is PluginSettingsDialogViewModel vm) vm.OnLanguageChanged();
    }

    private async void OnOpened(object? sender, EventArgs e)
    {
        if (DataContext is not PluginSettingsDialogViewModel vm) return;
        Title = vm.DisplayName;
        try { await vm.LoadAsync(); }
        catch { /* 加载失败已由 VM 的 LoadError 呈现 */ }
        // 等布局把表单行撑开后再居中 (RunJobs 让 SizeChanged/布局排空)
        await Dispatcher.UIThread.InvokeAsync(() => { });
        CenterToOwner();
    }

    /// <summary>相对宿主窗口垂直水平居中; 无宿主时退化为屏幕工作区居中。高度超出时贴宿主顶部。</summary>
    private void CenterToOwner()
    {
        var w = Bounds.Width;
        var h = Bounds.Height;
        PixelPoint target;
        if (Owner is Window owner)
        {
            var op = owner.Position;
            var ow = owner.Bounds.Width;
            var oh = owner.Bounds.Height;
            var x = op.X + (ow - w) / 2;
            var y = Math.Max(op.Y + 8, op.Y + (oh - h) / 2); // 高度超出宿主时贴顶, 不再往下顶
            target = new PixelPoint((int)Math.Round(x), (int)Math.Round(y));
        }
        else
        {
            var screen = Screens.ScreenFromWindow(this) ?? Screens.Primary;
            var wa = screen.WorkingArea;
            target = new PixelPoint(
                (int)Math.Round(wa.X + (wa.Width - w) / 2),
                (int)Math.Round(wa.Y + (wa.Height - h) / 2));
        }
        Position = target;
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
