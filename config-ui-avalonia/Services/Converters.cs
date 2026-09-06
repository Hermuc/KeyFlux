using System.Globalization;
using Avalonia;
using Avalonia.Data.Converters;

namespace KeyFlux.Settings.Services;

/// <summary>int <-> 字符串 (触发延时等数值输入框; 解析失败时拒绝写回)。</summary>
public sealed class IntToStringConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is int i ? i.ToString(culture) : "";

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is string s && int.TryParse(s.Trim(), NumberStyles.Integer, culture, out var v)
            ? v
            : AvaloniaProperty.UnsetValue;
}

/// <summary>字符串非空判定 (SaveNotice 等短暂提示的显隐)。</summary>
public sealed class StringNotEmptyConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => !string.IsNullOrEmpty(value as string);

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>
/// 双语文案转换器 (App.axaml 里的 x:Key="Tr"): ConverterParameter 为文案键,
/// 绑定源为 ViewModel 的 LanguageTick。LanguageTick 递增 -> 绑定重新求值 ->
/// 按当前语言重新翻译。文案表见 <see cref="I18n"/> 与 Resources/i18n.json。
/// </summary>
public sealed class I18nConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => I18n.T(parameter as string ?? parameter?.ToString());

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
