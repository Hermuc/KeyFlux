using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit.Abstractions;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 模式页 (KeymapPageView) 点选键格后整页缩放抖动的诊断/回归锁定:
/// Viewbox Stretch=Uniform 的缩放比由内容自然尺寸决定, 选中键若改变内容自然尺寸
/// (编辑器出现导致高度变化等), 整页 (键盘网格/编辑框) 会被等比缩小。
/// 回归目标: 选中键前后键格渲染尺寸与 Viewbox 内容自然尺寸必须不变。
/// </summary>
public sealed class KeymapPageViewLayoutTests
{
    private readonly ITestOutputHelper _output;

    public KeymapPageViewLayoutTests(ITestOutputHelper output) => _output = output;

    private static (KeymapPageView View, KeymapPageViewModel Vm, Window Window) CreateHost()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        var config = ConfigReadDefaults.Apply(new Config());
        config.Keymaps.Add(new Keymap { Id = 10, Hotkey = "F", Name = "F", Enable = true });
        main.Config = config;
        var vm = new KeymapPageViewModel(main, config.Keymaps[0]);
        var view = new KeymapPageView { DataContext = vm };
        var window = new Window { Width = 1200, Height = 820, Content = view };
        window.Show();
        return (view, vm, window);
    }

    /// <summary>键盘键格按钮 (Height=43, 页内其他按钮高度不同)。</summary>
    private static Button FirstKeyButton(KeymapPageView view)
        => view.GetVisualDescendants().OfType<Button>().First(b => b.Height == 43);

    /// <summary>Viewbox 内容自然尺寸 (缩放前的逻辑尺寸)。</summary>
    private static (double W, double H) NaturalSize(KeymapPageView view)
    {
        var child = view.GetVisualDescendants().OfType<Viewbox>().First().Child!;
        return (child.Bounds.Width, child.Bounds.Height);
    }

    /// <summary>对类型 1-9 逐个绑定后选中: 编辑器出现时 Viewbox 内容自然尺寸不得变化。</summary>
    [AvaloniaTheory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    [InlineData(8)]
    [InlineData(9)]
    public void SelectBoundKey_Does_Not_Change_Viewbox_Content_Size(int typeId)
    {
        var (view, vm, window) = CreateHost();
        vm.Keymap.Hotkeys["*a"] = [new Models.Action { WindowGroupId = 0, TypeId = typeId }];
        Dispatcher.UIThread.RunJobs();

        var keyBefore = FirstKeyButton(view).Bounds.Height;
        var (wBefore, hBefore) = NaturalSize(view);

        vm.Core.SelectKey("*a");
        Dispatcher.UIThread.RunJobs();

        var keyAfter = FirstKeyButton(view).Bounds.Height;
        var (wAfter, hAfter) = NaturalSize(view);

        _output.WriteLine($"type {typeId}: natural {wBefore:F1}x{hBefore:F1} -> {wAfter:F1}x{hAfter:F1}, key {keyBefore:F2} -> {keyAfter:F2}");

        Assert.Equal(wBefore, wAfter, 1);
        Assert.Equal(hBefore, hAfter, 1);
        Assert.Equal(keyBefore, keyAfter, 1); // 缩放比不得变化
    }
}
