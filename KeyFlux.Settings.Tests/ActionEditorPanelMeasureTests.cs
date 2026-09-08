using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views.Controls;
using Xunit.Abstractions;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 诊断: ActionEditorPanel 各动作类型编辑器的自然 (期望) 高度, 用于确定
/// 固定高度常量 (消除 Viewbox 页选中键后的整页缩放抖动)。
/// </summary>
public sealed class ActionEditorPanelMeasureTests
{
    private readonly ITestOutputHelper _output;

    public ActionEditorPanelMeasureTests(ITestOutputHelper output) => _output = output;

    [AvaloniaFact]
    public void Measure_Panel_Natural_Height_Per_Type()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        var keymap = new Keymap { Id = 10, Hotkey = "F", Enable = true };
        var core = new KeymapEditorCore(main, keymap);

        foreach (var width in new[] { 480.0, 541.0, 612.0 })
        {
            // 空态 (Editor=null)
            var empty = new ActionEditorPanel { Width = width };
            empty.DataContext = core.Editor;
            var w0 = new Window { Content = empty, Width = 200, Height = 200 };
            w0.Show();
            empty.Measure(new Size(width, double.PositiveInfinity));
            _output.WriteLine($"width {width:F0} empty: {empty.DesiredSize.Height:F1}");

            foreach (var typeId in new[] { 1, 2, 3, 4, 5, 6, 7, 8, 9 })
            {
                core.Editor.BindTo(new Models.Action { WindowGroupId = 0, TypeId = typeId });
                var panel = new ActionEditorPanel { Width = width };
                panel.DataContext = core.Editor;
                var w = new Window { Content = panel, Width = 200, Height = 200 };
                w.Show();
                panel.Measure(new Size(width, double.PositiveInfinity));
                _output.WriteLine($"width {width:F0} type {typeId}: {panel.DesiredSize.Height:F1}");
            }
        }
    }
}
