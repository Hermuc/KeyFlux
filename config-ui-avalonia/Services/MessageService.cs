using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 消息提示抽象: 保存 400 时弹出后端 message 等。
/// 界面运行时用 <see cref="DialogMessageService"/> (模态对话框);
/// 无界面场景 (冒烟/单测) 注入自定义测试替身实现。
/// </summary>
public interface IMessageService
{
    void Show(string title, string message);
}

/// <summary>
/// 轻量模态对话框 (不引入额外依赖)。Owner 未设置或弹窗失败 (如 headless) 时静默降级。
/// </summary>
public sealed class DialogMessageService : IMessageService
{
    /// <summary>由主窗口在 Loaded 后注入。</summary>
    public Window? Owner { get; set; }

    public void Show(string title, string message)
    {
        if (Owner is null) return;
        // 异步弹出, 不阻塞保存链路; 失败 (无视觉根等) 静默忽略
        Dispatcher.UIThread.Post(async () =>
        {
            try
            {
                if (Owner is null) return;

                var okButton = new Button
                {
                    Content = "OK",
                    HorizontalAlignment = HorizontalAlignment.Right,
                    Margin = new Avalonia.Thickness(0, 16, 0, 0),
                    Padding = new Avalonia.Thickness(24, 6),
                };

                var textBox = new TextBox
                {
                    Text = message,
                    IsReadOnly = true,
                    TextWrapping = TextWrapping.Wrap,
                    BorderThickness = new Avalonia.Thickness(0),
                    Background = Brushes.Transparent,
                    AcceptsReturn = true,
                };

                var panel = new DockPanel { Margin = new Avalonia.Thickness(20) };
                okButton.SetValue(DockPanel.DockProperty, Dock.Bottom);
                panel.Children.Add(okButton);
                panel.Children.Add(textBox);

                var dialog = new Window
                {
                    Title = title,
                    Width = 480,
                    MinHeight = 200,
                    WindowStartupLocation = WindowStartupLocation.CenterOwner,
                    CanResize = false,
                    Content = panel,
                };
                okButton.Click += (_, _) => dialog.Close();

                await dialog.ShowDialog(Owner);
            }
            catch (Exception)
            {
                // headless 等场景无有效视觉根, 忽略
            }
        });
    }
}
