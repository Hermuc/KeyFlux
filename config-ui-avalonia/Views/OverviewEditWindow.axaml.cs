using Avalonia.Controls;
using Avalonia.Interactivity;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 总览页编辑窗口: 编辑 markdown 原文, 保存到 config.overviewDocMd (data/config.json 真源,
/// 部署同步不覆盖)。保存成功置 DialogResult=true, 由 HomePageView 负责刷新总览渲染。
/// 清空内容并保存 = 恢复默认文档 (config_doc.md)。
/// </summary>
public partial class OverviewEditWindow : Window
{
    private readonly MainViewModel _main;
    private bool _saving;

    /// <summary>保存成功后置 true (HomePageView 据此刷新总览渲染)。</summary>
    public bool Saved { get; private set; }

    public OverviewEditWindow(MainViewModel main, string initialMd)
    {
        InitializeComponent();
        Services.Win32.DialogChrome.Apply(this);
        _main = main;

        // 标题栏小图标透明化 (与主窗口同一助手, 行为一致)
        TitleBarIconSuppressor.Attach(this);

        Title = I18n.T("2406");
        TipText.Text = I18n.T("2405");
        RestoreButton.Content = I18n.T("2404");
        CancelButton.Content = I18n.T("970");
        SaveButton.Content = I18n.T("929");
        Editor.Text = initialMd;
    }

    private void OnRestoreClick(object? sender, RoutedEventArgs e)
    {
        // 清空自定义内容; 保存后总览页即回到默认文档 (config_doc.md)
        Editor.Text = "";
        Editor.Focus();
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e) => Close();

    private async void OnSaveClick(object? sender, RoutedEventArgs e)
    {
        if (_saving) return;
        _saving = true;
        try
        {
            _main.Config!.OverviewDocMd = Editor.Text ?? "";
            if (await _main.SaveAsync(force: true))
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
}
