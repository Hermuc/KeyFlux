using KeyFlux.Settings.Theming;
using Avalonia.Controls;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Views;

/// <summary>
/// 匹配类型弹窗外壳 (内容为 <see cref="MatchTypesPageView"/>)。
/// 与其余弹窗一致: 保留系统标题栏与原生三键, 但经
/// <see cref="KeyFlux.Settings.Services.Win32.DialogChrome"/> 把标题栏染成暖色表面色,
/// 并在弹窗打开期间开启 owner 的背景模糊 (ModalBlur)。
/// 调用方在关闭后读 <c>MatchTypesPageViewModel.LastCreatedTypeId</c> 决定是否自动选中新类型。
/// </summary>
public partial class MatchTypesDialogWindow : Window
{
    public MatchTypesDialogWindow()
    {
        InitializeComponent();
        WindowSurface.Attach(this); // 真·毛玻璃: 窗口打开时挂 DWM 系统背景材质 (迟开窗口经此补挂)
        // 标题栏文案 (系统绘制; DialogChrome 只负责着色, 不改文字)
        Title = I18n.T("2519");
        // 弹窗必备: DWM 标题栏着色 + owner 背景模糊 (ModalBlur) 的统一入口。
        // 漏调会让本窗口标题栏保持冷白、且打开时没有背景模糊 (2026-09-15 实测缺陷)。
        KeyFlux.Settings.Services.Win32.DialogChrome.Apply(this);
    }
}
