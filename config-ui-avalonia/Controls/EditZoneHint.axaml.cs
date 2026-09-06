using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;

namespace KeyFlux.Settings.Controls;

/// <summary>
/// 可复用「编辑区提示」控件: 虚线分割线 + 可点击说明区域。
/// 使用方仅需设置 Text (说明文案) 并订阅 Click 事件, 即可把页面底部
/// 变成「点击进入编辑」入口 (当前用于总览页替代原右上角「编辑总览」按钮)。
/// 业务逻辑不在本控件内, 由使用方在 Click 中实现, 保持视图与逻辑解耦。
/// </summary>
public partial class EditZoneHint : UserControl
{
    public EditZoneHint() => InitializeComponent();

    /// <summary>说明文案 (如 "点击此区域编辑总览")。</summary>
    public static readonly StyledProperty<string> TextProperty =
        AvaloniaProperty.Register<EditZoneHint, string>(nameof(Text), defaultValue: "");

    public string Text
    {
        get => GetValue(TextProperty);
        set => SetValue(TextProperty, value);
    }

    /// <summary>点击编辑区时触发 (转发内部 Border 的 Tapped)。</summary>
    public event EventHandler<RoutedEventArgs>? Click;

    protected override void OnPropertyChanged(AvaloniaPropertyChangedEventArgs change)
    {
        base.OnPropertyChanged(change);
        // 文案与 Text 属性同步 (x:CompileBindings 下 $parent 绑定类型解析受限, 改代码同步)
        if (change.Property == TextProperty && HintText is not null)
        {
            HintText.Text = change.GetNewValue<string>();
        }
    }

    private void OnZoneTapped(object? sender, TappedEventArgs e) => Click?.Invoke(this, e);
}
