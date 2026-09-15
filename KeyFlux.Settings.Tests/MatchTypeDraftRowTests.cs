using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
// System.IO.MatchType (SDK 隐式 using) 与本项目 Models.MatchType 同名, 显式限定避免 CS0104
using MatchType = KeyFlux.Settings.Models.MatchType;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 「新建匹配类型」草稿行 (2026-09-15) 的状态守护:
/// 新建期间在列表最末追加一个草稿行表示"正在创建、尚未填写", 并被自动选中且钉住;
/// 取消 → 移除草稿行并恢复进入新建前的选中; 保存 → 由 NoteCreated 把选中交给新建出的真实行。
///
/// 归属 <c>I18nSerial</c> 集合: 本用例读 I18n 与 BehaviorCatalog 两个全局静态 (与 I18nResourceTests 同因)。
/// </summary>
[Collection("I18nSerial")]
public sealed class MatchTypeDraftRowTests
{
    private static MatchTypesPageViewModel CreateVm()
    {
        var main = new MainViewModel(new BackendSessionOptions())
        {
            Config = new Config { Options = new Options() },
        };
        return new MatchTypesPageViewModel(main);
    }

    [Fact]
    public async Task Draft_Row_Appears_Selected_Pinned_Then_Removed_On_Cancel()
    {
        var vm = CreateVm();
        var baseline = vm.Rows.Count;                        // 4 个内置文本特征
        Assert.DoesNotContain(vm.Rows, r => r.IsDraft);

        await vm.OpenCreateCommand.ExecuteAsync(null);

        // 追加在列表最末, 且自动选中
        Assert.Equal(baseline + 1, vm.Rows.Count);
        var draft = vm.Rows[^1];
        Assert.True(draft.IsDraft);
        Assert.Same(draft, vm.SelectedRow);

        // 草稿行不属于"自定义类型": 不可编辑/删除, 也不显示"未配置行为"的补行为入口
        Assert.False(draft.IsCustom);
        Assert.False(vm.CanEditSelected);
        Assert.False(vm.CanRemoveSelected);
        Assert.False(vm.CanSetAction);

        // 新建期间点其它行 → 选择被钉回草稿行 (避免"选中行 ≠ 正在编辑的对象")
        vm.SelectedRow = vm.Rows[0];
        Assert.Same(draft, vm.SelectedRow);

        // 取消: 草稿行移除, 恢复到进入新建前的选中 (原本为空 ⇒ null)
        vm.CloseEditor();
        Assert.Equal(baseline, vm.Rows.Count);
        Assert.DoesNotContain(vm.Rows, r => r.IsDraft);
        Assert.Null(vm.SelectedRow);
    }

    [Fact]
    public async Task Cancel_Restores_Selection_That_Was_Active_Before_Create()
    {
        var vm = CreateVm();
        vm.Config.MatchTypes.Add(new MatchType
        {
            Id = "netdisk",
            Label = "网盘链接",
            Kind = "text",
            Rules = [new MatchRule { Op = "contains", Value = "pan.baidu.com" }],
        });
        vm.ReloadRows();
        vm.SelectedRow = vm.Rows.First(r => r.Id == "netdisk");

        await vm.OpenCreateCommand.ExecuteAsync(null);
        Assert.True(vm.SelectedRow?.IsDraft);                // 新建期间选中草稿行
        vm.CloseEditor();

        Assert.Equal("netdisk", vm.SelectedRow?.Id);         // 取消后回到原选中项
    }

    [Fact]
    public async Task NoteCreated_Hands_Selection_To_The_New_Real_Row_And_Drops_Draft()
    {
        var vm = CreateVm();
        await vm.OpenCreateCommand.ExecuteAsync(null);
        Assert.True(vm.SelectedRow?.IsDraft);

        // 模拟编辑器保存成功: 配置里已有新类型 → 重建列表 (草稿行由重建流程重新追加)
        vm.Config.MatchTypes.Add(new MatchType
        {
            Id = "design",
            Label = "设计稿",
            Kind = "fileExt",
            Exts = ["psd", "ai"],
        });
        vm.ReloadRows();
        vm.NoteCreated("design");                            // 解除钉住并把选中交给新行
        vm.CloseEditor();

        Assert.Equal("design", vm.SelectedRow?.Id);
        Assert.DoesNotContain(vm.Rows, r => r.IsDraft);      // 草稿行已清理
        Assert.Equal(vm.Rows.Count, vm.Rows.Count(r => !r.IsDraft));
    }
}
