using System.Globalization;
using Avalonia.Data.Converters;
using MyKeymap.Settings.Services;

namespace MyKeymap.Settings.Services;

/// <summary>
/// 分隔行高度转换: IsSeparator=true → 14 (容纳分隔线), false → NaN (自适应内容)。
/// 用于类型下拉中「文件后缀分组 | 文本特征」之间的动态分隔行。
/// </summary>
public sealed class BoolToSeparatorHeightConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is true ? 14d : double.NaN;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
