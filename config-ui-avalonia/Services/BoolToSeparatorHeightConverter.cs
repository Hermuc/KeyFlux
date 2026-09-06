using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 分隔行高度转换: IsSeparator=true → 7 (紧凑分隔行, 视觉上只是一条线的占位), false → NaN (自适应内容)。
/// 用于类型下拉中「文件后缀分组 | 文本特征」之间的动态分隔行。
/// </summary>
public sealed class BoolToSeparatorHeightConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is true ? 7d : double.NaN;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>
/// 分隔行背景转换: IsSeparator=true → 弹层同色 (Fluent 浅灰 #F9F9F9),
/// 使分隔行的 ComboBoxItem 容器与弹层背景无缝衔接 (消除「白色一圈」), 
/// 同时压掉 pointerover 主题高亮的可见性; false → 透明 (正常项保留主题反馈)。
/// </summary>
public sealed class PopupBackgroundConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is true
            ? new SolidColorBrush(Color.Parse("#F9F9F9"))
            : Brushes.Transparent;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
