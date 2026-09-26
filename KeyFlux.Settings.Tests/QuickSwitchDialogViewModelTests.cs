using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// QuickSwitchDialogViewModel 纯 VM 单测:
///   - 构造后 ExcludedPrefixRows 计数;
///   - Draft 副本隔离 (编辑不影响真源);
///   - ExcludedPrefixes 深拷贝;
///   - AddExcludedPrefix / RemoveExcludedPrefix 命令;
///   - SaveAsync 无后端时安全返回 false。
/// </summary>
[Collection("I18nSerial")]
public sealed class QuickSwitchDialogViewModelTests
{
    private static (QuickSwitchDialogViewModel Vm, MainViewModel Main) CreateVm(
        List<string>? excludedPrefixes = null)
    {
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            Options = new Options
            {
                QuickSwitch = new QuickSwitchOption
                {
                    CollectEnabled = true,
                    AutoShow = false,
                    MaxHistory = 50,
                    PollIntervalMs = 300,
                    OverlayRows = 8,
                    OverlayRowsCompact = 5,
                    ExcludedPrefixes = excludedPrefixes ?? ["C:\\Temp", "D:\\Cache", "E:\\Logs"],
                },
            },
        };
        return (new QuickSwitchDialogViewModel(main), main);
    }

    /// <summary>① 构造后 ExcludedPrefixRows 计数 == 源列表长度。</summary>
    [Fact]
    public void Constructor_ExcludedPrefixRows_Count_Matches_Source()
    {
        var (vm, _) = CreateVm(["A", "B", "C"]);
        Assert.Equal(3, vm.ExcludedPrefixRows.Count);

        var (vm2, _) = CreateVm(["X"]);
        Assert.Single(vm2.ExcludedPrefixRows);

        var (vm3, _) = CreateVm([]);
        Assert.Empty(vm3.ExcludedPrefixRows);
    }

    /// <summary>② 副本隔离: 修改 Draft 字段后, 真源 Config.Options.QuickSwitch 不变。</summary>
    [Fact]
    public void Draft_Is_Isolated_From_Source()
    {
        var (vm, main) = CreateVm();
        var src = main.Config!.Options.QuickSwitch;

        // 修改 Draft 各字段
        vm.Draft.CollectEnabled = !src.CollectEnabled;
        vm.Draft.MaxHistory = 9999;
        vm.Draft.PollIntervalMs = 1234;
        vm.Draft.AutoShow = !src.AutoShow;
        vm.Draft.OverlayRows = 42;
        vm.Draft.OverlayRowsCompact = 7;
        vm.Draft.ExcludedPrefixes.Add("NEW_ENTRY");

        // 真源不受影响
        Assert.True(src.CollectEnabled);       // 原始 true
        Assert.Equal(50, src.MaxHistory);
        Assert.Equal(300, src.PollIntervalMs);
        Assert.False(src.AutoShow);
        Assert.Equal(8, src.OverlayRows);
        Assert.Equal(5, src.OverlayRowsCompact);
        Assert.Equal(3, src.ExcludedPrefixes.Count); // 原始 3 项
    }

    /// <summary>③ ExcludedPrefixes 深拷贝: Draft 与源列表不是同一引用。</summary>
    [Fact]
    public void Draft_ExcludedPrefixes_Is_Deep_Copy()
    {
        var prefixes = new List<string> { "Alpha", "Beta" };
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            Options = new Options
            {
                QuickSwitch = new QuickSwitchOption { ExcludedPrefixes = prefixes },
            },
        };
        var vm = new QuickSwitchDialogViewModel(main);

        Assert.NotSame(prefixes, vm.Draft.ExcludedPrefixes);
        Assert.Equal(prefixes, vm.Draft.ExcludedPrefixes); // 内容相同
    }

    /// <summary>④ AddExcludedPrefix 命令: Draft.ExcludedPrefixes.Count +1, ExcludedPrefixRows.Count +1。</summary>
    [Fact]
    public void AddExcludedPrefix_Increases_Counts()
    {
        var (vm, _) = CreateVm(["A", "B"]);
        Assert.Equal(2, vm.Draft.ExcludedPrefixes.Count);
        Assert.Equal(2, vm.ExcludedPrefixRows.Count);

        vm.AddExcludedPrefixCommand.Execute(null);

        Assert.Equal(3, vm.Draft.ExcludedPrefixes.Count);
        Assert.Equal(3, vm.ExcludedPrefixRows.Count);
        Assert.Equal("", vm.Draft.ExcludedPrefixes[2]); // 新行为空串
        Assert.Equal("", vm.ExcludedPrefixRows[2].Value);
    }

    /// <summary>⑤ RemoveExcludedPrefix 命令: 移除指定行后计数 -1, 索引对齐。</summary>
    [Fact]
    public void RemoveExcludedPrefix_Decreases_Count_And_Reindexes()
    {
        var (vm, _) = CreateVm(["First", "Second", "Third"]);
        Assert.Equal(3, vm.ExcludedPrefixRows.Count);

        // 移除中间行 (Index=1)
        vm.RemoveExcludedPrefixCommand.Execute(vm.ExcludedPrefixRows[1]);

        Assert.Equal(2, vm.Draft.ExcludedPrefixes.Count);
        Assert.Equal(2, vm.ExcludedPrefixRows.Count);
        Assert.Equal("First", vm.ExcludedPrefixRows[0].Value);
        Assert.Equal("Third", vm.ExcludedPrefixRows[1].Value);
        // 索引对齐
        Assert.Equal(0, vm.ExcludedPrefixRows[0].Index);
        Assert.Equal(1, vm.ExcludedPrefixRows[1].Index);
    }

    /// <summary>⑥ SaveAsync 在无后端 (Session.Api == null) 时不抛异常且返回 false, Saved 仍为 false。</summary>
    [Fact]
    public async Task SaveAsync_Without_Backend_Returns_False()
    {
        var (vm, main) = CreateVm();
        Assert.Null(main.Session.Api); // 无后端连接

        var result = await vm.SaveAsync();

        Assert.False(result);
        Assert.False(vm.Saved);
    }
}
