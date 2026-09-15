using System.Collections.Specialized;
using Avalonia.Controls;
using Avalonia.Data;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 「切换输入框焦点导致整窗闪白」的回归锁定 (2026-09-15 用户报)。
///
/// 根因链 (已实测复现): 名称/触发键输入框 `LostFocus` → <see cref="SettingsPageViewModel.CommitKeymapEdit"/>
/// → 无条件 `RefreshKeymapSection()` → `_main.OnNavInvalidated()` → `BuildNav()` 执行
/// `NavItems.Clear()`; 而导航 ListBox 的 `SelectedItem` 双向绑定到 `CurrentNavItem`,
/// 清空瞬间会把选中项置空并把 **null** 推回 → `OnCurrentNavItemChanged(null)`
/// → `CurrentPage = null` → 内容区空白一帧 (实测变化序列 `null -> page`)。
///
/// 修复两层: ① `OnCurrentNavItemChanged` 忽略 null (内容区不再被清空);
///           ② `CommitKeymapEdit` 无实际变化时不重建 (切焦点属此列, 不再拆建整表)。
/// 本文件同时锁定「规范化 / 重复键删行」两条既有行为未被改坏。
/// </summary>
[Collection("I18nSerial")]
public sealed class NavRebuildFlickerTests
{
    /// <summary>造一个最小可用宿主: 复刻 MainWindow.axaml:149-150 的双向绑定对。</summary>
    private static (MainViewModel Main, List<string> PageLog, Window Win) CreateNavHost()
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());

        var settingsPage = new object();
        main.NavItems.Add(new NavItem { Id = "home", Title = "Home", Page = new object() });
        main.NavItems.Add(new NavItem { Id = "keymap-4", Title = "Settings", Page = settingsPage });
        main.CurrentNavItem = main.NavItems[1];

        var log = new List<string>();
        main.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(MainViewModel.CurrentPage))
            {
                log.Add(main.CurrentPage is null ? "null" : "page");
            }
        };

        var list = new ListBox();
        list.Bind(ItemsControl.ItemsSourceProperty, new Binding("NavItems"));
        list.Bind(ListBox.SelectedItemProperty, new Binding("CurrentNavItem") { Mode = BindingMode.TwoWay });
        var content = new ContentControl();
        content.Bind(ContentControl.ContentProperty, new Binding("CurrentPage"));

        var win = new Window
        {
            Width = 600,
            Height = 400,
            Content = new StackPanel { Children = { list, content } },
            DataContext = main,
        };
        win.Show();
        Dispatcher.UIThread.RunJobs();
        return (main, log, win);
    }

    private static SettingsPageViewModel CreateSettings(params (int Id, string Hotkey)[] rows)
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = ConfigReadDefaults.Apply(new Config());
        foreach (var (id, hotkey) in rows)
        {
            main.Config.Keymaps.Add(new Keymap { Id = id, Name = $"k{id}", Hotkey = hotkey, ParentId = 0 });
        }
        var vm = new SettingsPageViewModel(main);
        vm.RefreshKeymapSection();
        return vm;
    }

    /// <summary>
    /// ① 导航重建 (NavItems.Clear + 重填) 不得把内容区置空 ——
    /// 修复前 CurrentPage 会被推成 null (序列含 "null"), 表现为整窗闪白。
    /// </summary>
    [AvaloniaFact]
    public void NavItems_Rebuild_Must_Not_Blank_Current_Page()
    {
        var (main, log, win) = CreateNavHost();
        try
        {
            var before = main.CurrentPage;
            log.Clear();

            // 复刻 BuildNav() 末尾三行 (MainViewModel.cs:213-215)
            var rebuilt = new List<NavItem>
            {
                new() { Id = "home", Title = "Home", Page = main.NavItems[0].Page },
                new() { Id = "keymap-4", Title = "Settings", Page = main.NavItems[1].Page },
            };
            main.NavItems.Clear();
            foreach (var it in rebuilt) main.NavItems.Add(it);
            main.CurrentNavItem = main.NavItems.FirstOrDefault(n => n.Id == "keymap-4")
                                  ?? main.NavItems.FirstOrDefault();
            Dispatcher.UIThread.RunJobs();

            Assert.DoesNotContain("null", log);
            Assert.Same(before, main.CurrentPage);
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ② 仅切换焦点 (内容未改动) 不得重建行集合 ——
    /// 修复前每次失焦都会 Clear + 全量重建, 这是"切输入框就闪"的直接来源。
    /// </summary>
    [AvaloniaFact]
    public void Blur_Without_Content_Change_Must_Not_Rebuild_Rows()
    {
        var vm = CreateSettings((5, "f"), (6, "g"));
        Assert.Equal(2, vm.KeymapRows.Count);

        var rebuilds = 0;
        vm.KeymapRows.CollectionChanged += (_, e) =>
        {
            if (e.Action is NotifyCollectionChangedAction.Reset or NotifyCollectionChangedAction.Remove)
            {
                rebuilds++;
            }
        };

        // 典型场景: 从一个输入框点到另一个输入框, 前一个框失焦提交, 什么都没改
        vm.CommitKeymapEdit(vm.KeymapRows[0]);

        Assert.Equal(0, rebuilds);
        Assert.Equal(2, vm.KeymapRows.Count);
    }

    /// <summary>③ 既有行为不得被改坏: 非标准键名仍被规范化 (bs -> Backspace)。</summary>
    [AvaloniaFact]
    public void Blur_Still_Normalizes_NonStandard_Hotkey()
    {
        var vm = CreateSettings((5, "bs"), (6, "g"));
        vm.KeymapRows[0].Hotkey = "bs"; // TwoWay 绑定下用户输入即写入模型

        vm.CommitKeymapEdit(vm.KeymapRows[0]);

        Assert.Equal("Backspace", vm.Config.Keymaps.First(k => k.Id == 5).Hotkey);
    }

    /// <summary>
    /// ④ 既有行为不得被改坏: 触发键与同上层键重复时, 仍删除【当前编辑的】那一行
    /// (复刻 findLastIndex/首个命中判定, 故编辑第二条重复项才会触发删除)。
    /// </summary>
    [AvaloniaFact]
    public void Blur_Still_Removes_Duplicate_Hotkey_Row()
    {
        var vm = CreateSettings((7, "x"), (8, "x"));
        Assert.Equal(2, vm.KeymapRows.Count);

        vm.CommitKeymapEdit(vm.KeymapRows[1]); // 第二条重复项

        Assert.DoesNotContain(vm.Config.Keymaps, k => k.Id == 8);
        Assert.Contains(vm.Config.Keymaps, k => k.Id == 7);
        Assert.Single(vm.KeymapRows);
    }
}
