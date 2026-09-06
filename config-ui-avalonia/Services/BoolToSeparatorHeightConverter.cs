using System.Globalization;
using Avalonia.Data.Converters;
using MyKeymap.Settings.Services;

namespace MyKeymap.Settings.Services;

/// <summary>
/// 分隔行高度转换: IsSeparator=true → 5 (细线紧贴, 不占整行), false → NaN (自适应内容)。
/// 用于类型下拉中「文件后缀分组 | 文本特征」之间的动态分隔行。
/// </summary>
public sealed class BoolToSeparatorHeightConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is true ? 5d : double.NaN;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>
/// 分隔行背景转换: IsSeparator=true → 不透明白 (压掉 ComboBoxItem 的 hover/选中高亮),
/// false → 透明 (正常项保留主题 hover 反馈)。
/// </summary>
public sealed class SeparatorItemBackgroundConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is true
            ? new Avalonia.Media.SolidColorBrush(Avalonia.Media.Color.Parse("White"))
            : Avalonia.Media.Brushes.Transparent;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
