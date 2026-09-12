using Avalonia.Controls;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 动作编辑面板弹窗 (模态): 承载 <see cref="Controls.ActionEditorPanel"/>,
/// 由设置页自定义热键分区的「功能」点击打开 (原 Custom Hotkeys 页右侧内嵌面板, 迁入弹窗)。
/// </summary>
public partial class ActionEditorWindow : Window
{
    public ActionEditorWindow()
    {
        InitializeComponent();
        Services.Win32.DialogChrome.Apply(this);
        // 标题栏小图标透明化 (与主窗口/其他对话框同一助手)
        TitleBarIconSuppressor.Attach(this);
    }
}
