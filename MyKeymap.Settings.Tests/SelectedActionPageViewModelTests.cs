using MyKeymap.Settings.Models;
using MyKeymap.Settings.Services;
using MyKeymap.Settings.ViewModels;

namespace MyKeymap.Settings.Tests;

/// <summary>
/// 选中动作单屏页 (方案 D) 纯 ViewModel 单测:
///   - 键位推导: chips 序号 1 起自动顺延 / 增删排序后重排 / 9 上限 / 单行为删除禁用;
///   - 类型过滤: 行为下拉与弹窗勾选列表按 BehaviorCatalog.Covering 前提过滤;
///   - 校验回显: 弹窗 CanConfirm (fileExt 需非空值+至少一勾) / 行为增删约束;
///   - 弹窗落位: fileExt -> FileMappings / textType -> TextMappings, 列表顺序 = 菜单键位顺序;
///   - 热键与手风琴仲裁: Hotkey 写模型+未保存标记 / UsedHotkeys 收集 / 同屏只开一个。
/// </summary>
public sealed class SelectedActionPageViewModelTests
{
    private static void EnsureBehaviorCatalog()
    {
        if (BehaviorCatalog.Loaded) return;
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
    }

    private static Config BuildConfig(int mappings = 0) => new()
    {
        FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
    };

    private static (SelectedActionPageViewModel Page, Config Config) CreatePage(Config? config = null)
    {
        EnsureBehaviorCatalog();
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = config ?? BuildConfig();
        return (new SelectedActionPageViewModel(main), main.Config!);
    }

    /// <summary>bug2 回归: 迁移来的 >^p 应在 VM.Hotkey 中, 且 AhkToDisplay 能渲染 (RCtrl+P)。</summary>
    [Fact]
    public void Hotkey_FromMigratedConfig_IsExposedAndDisplayable()
    {
        var config = BuildConfig();
        config.SelectedAction = new SelectedAction
        {
            Hotkey = ">^p",
            Enable = false,
            Mappings = [],
        };
        var (page, _) = CreatePage(config);
        Assert.Equal(">^p", page.Hotkey);
        Assert.Equal("RCtrl+P", HotkeyLogic.AhkToDisplay(page.Hotkey));
    }

    /// <summary>构造 fileExt 行 (entries 逐个给 behavior) 并展开。</summary>
    private static MappingRowVm NewFileRow(SelectedActionPageViewModel page, params string[] behaviors)
    {
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "fileExt",
            MatchValue = "jpg",
            Entries = [.. behaviors.Select(b => new SelectedEntry { Behavior = b, Options = new RuleOptions() })],
        });
        page.FileMappings.Add(row); // 行必须挂到分区: 仲裁/调序/写回都遍历分区集合
        page.ExpandedRow = row;
        return row;
    }

    /// <summary>构造 textType(url) 行并展开。</summary>
    private static MappingRowVm NewUrlRow(SelectedActionPageViewModel page, params string[] behaviors)
    {
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "textType",
            MatchValue = "url",
            Entries = [.. behaviors.Select(b => new SelectedEntry { Behavior = b, Options = new RuleOptions() })],
        });
        page.TextMappings.Add(row); // 行必须挂到分区: 仲裁/调序都遍历分区集合
        page.ExpandedRow = row;
        return row;
    }

    // ------------------------------------------------------------- 键位推导 (chips)

    /// <summary>chips 序号 1 起随 entries 顺延, 增删后重排; 标签 = 行为显示名。</summary>
    [Fact]
    public void Chips_Numbers_AutoIncrement_On_Add_And_Remove()
    {
        var (page, _) = CreatePage();
        var row = NewFileRow(page, "open", "run");

        Assert.Equal([1, 2], row.Chips.Select(c => c.Index));
        Assert.Equal("open", row.Chips[0].Label);
        Assert.Equal("run", row.Chips[1].Label);

        row.AddEntryCommand.Execute(null); // 覆盖集第一个未占用行为
        Assert.Equal([1, 2, 3], row.Chips.Select(c => c.Index));

        row.RemoveEntry(row.Editors[1]); // 删中间 -> 后续顺延
        Assert.Equal([1, 2], row.Chips.Select(c => c.Index));
        Assert.Equal(2, row.Mapping.Entries.Count);
    }

    /// <summary>行为排序: entries 交换 + chips 重排, 编辑器对象保持 (编辑中状态不丢) 且序号刷新。</summary>
    [Fact]
    public void MoveEntry_Swaps_Chip_Order_Keeps_Editor_Identity()
    {
        var (page, _) = CreatePage();
        var row = NewFileRow(page, "open", "run", "copy");
        var first = row.Editors[0];

        row.MoveEntry(first, +1);

        // chips 与 entries 同步交换
        Assert.Equal(["run", "open", "copy"], row.Chips.Select(c => c.Label));
        Assert.Equal(["run", "open", "copy"], row.Mapping.Entries.Select(e => e.Behavior));
        // 编辑器对象不动, 序号刷新 (键位 = 2)
        Assert.Equal(2, first.Index);
        Assert.Same(first, row.Editors[1]);
    }

    /// <summary>约束: 唯一行为 ✕ 禁用 (CanRemove=false), 删除命令不生效; 边界键位禁用。</summary>
    [Fact]
    public void Single_Entry_Remove_Disabled_And_Boundary_Keys_Disabled()
    {
        var (page, _) = CreatePage();
        var row = NewFileRow(page, "open");
        var editor = row.Editors[0];

        Assert.False(editor.CanRemove);
        Assert.False(editor.CanMoveUp);
        Assert.False(editor.CanMoveDown);

        row.RemoveEntry(editor); // 守卫: 至少保留一个行为
        Assert.Single(row.Mapping.Entries);
        Assert.Single(row.Chips);
    }

    /// <summary>约束: 行为数达 9 时「添加行为」禁用 (CanAddEntry=false, 命令守卫不追加)。</summary>
    [Fact]
    public void AddEntry_Disabled_At_Nine()
    {
        var (page, _) = CreatePage();
        var row = NewFileRow(page,
            "copy", "open", "open_folder", "open_path", "run", "script"); // 通配覆盖集 6 项
        // 追加 3 个重复行为直充到 9 (绕过覆盖集耗尽; 键位推导只关心数量)
        for (var i = 0; i < 3; i++)
        {
            row.Mapping.Entries.Add(new SelectedEntry { Behavior = "open", Options = new RuleOptions() });
            row.Editors.Add(new EntryRowVm(row, row.Mapping.Entries[^1]));
        }
        row.RefreshChips();
        row.OpenEditor(); // 重开以刷新位置推送
        Assert.Equal(9, row.Mapping.Entries.Count);

        Assert.False(row.CanAddEntry);
        row.AddEntryCommand.Execute(null);
        Assert.Equal(9, row.Mapping.Entries.Count); // 守卫生效不追加
    }

    /// <summary>添加行为默认取覆盖集中第一个未占用行为 (url: open_url 已占 -> search)。</summary>
    [Fact]
    public void AddEntry_Picks_First_Unused_Covering_Behavior()
    {
        var (page, _) = CreatePage();
        var row = NewUrlRow(page, "open_url");

        row.AddEntryCommand.Execute(null);
        Assert.Equal(["open_url", "search"], row.Mapping.Entries.Select(e => e.Behavior));

        // 占满覆盖集后再加: 回退第一条 (open_url)
        row.AddEntryCommand.Execute(null);
        Assert.Equal(3, row.Mapping.Entries.Count);
        Assert.Equal("open_url", row.Mapping.Entries[2].Behavior);
    }

    // ------------------------------------------------------------- 类型过滤

    /// <summary>行为下拉按行前提过滤: fileExt 行 = 通配 6 项; textType(url) 行 = 文本专属 2 项。</summary>
    [Fact]
    public void BehaviorOptions_Filtered_By_Mapping_Premise()
    {
        var (page, _) = CreatePage();
        var fileRow = NewFileRow(page, "open");
        var fileOptions = fileRow.Editors[0].BehaviorOptions.Select(o => o.Value).ToHashSet(); // urlRow 展开前抓取
        var urlRow = NewUrlRow(page, "open_url"); // 仲裁收起 fileRow (同屏只开一个)

        Assert.Equal(new HashSet<string> { "copy", "open", "open_folder", "open_path", "run", "script" },
            fileOptions);
        Assert.Equal(new HashSet<string> { "open_url", "search" },
            urlRow.Editors[0].BehaviorOptions.Select(o => o.Value).ToHashSet());
    }

    // ------------------------------------------------------------- 弹窗校验与勾选

    /// <summary>弹窗勾选列表按类型重建: fileExt -> 通配 6 项; 切 url -> 文本专属 2 项 (已勾状态不保留跨类型)。</summary>
    [Fact]
    public void AddMappingVm_Picks_Rebuilt_On_Type_Change()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;

        // 默认选中第一项 = 第一个文件后缀分组 (image): 通配覆盖集
        Assert.Equal("group:image", Assert.IsType<ComboOption>(panel.TypeOptions[0]).Value);
        Assert.Equal(6, panel.BehaviorPicks.Count);
        Assert.True(panel.IsFileExt);

        panel.TypeSelected = panel.TypeOptions.OfType<ComboOption>().First(o => o.Value == "url");
        Assert.Equal(["open_url", "search"], panel.BehaviorPicks.Select(p => p.Pack.Id));
        Assert.False(panel.IsFileExt);

        // 切回分组项: 分组在前、真 Separator 控件、文本特征在后 (用户要求排序)
        panel.TypeSelected = panel.TypeOptions[0];
        Assert.True(panel.IsFileExt);
        Assert.Equal(6, panel.BehaviorPicks.Count);
        Assert.Equal("group:image", panel.TypeOptions[0].Value);
        Assert.True(panel.TypeOptions[1].IsSeparator); // 动态分隔项 (跟随分组数量)
        Assert.Equal("url", panel.TypeOptions[2].Value);
    }

    /// <summary>键位序号 = 勾选列表位置序 (1 起; 与勾选先后无关), 取消勾选后后续顺延。</summary>
    [Fact]
    public void BehaviorPick_Order_Follows_List_Position()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;

        var picks = panel.BehaviorPicks;
        picks[2].IsChecked = true;
        picks[0].IsChecked = true;
        picks[4].IsChecked = true;
        Assert.Equal(1, picks[0].Order); // 列表位置序: 勾选先后不改变下标
        Assert.Equal(2, picks[2].Order);
        Assert.Equal(3, picks[4].Order);
        Assert.Equal(0, picks[1].Order);

        picks[0].IsChecked = false; // 首项取消 -> 后续顺延
        Assert.Equal(1, picks[2].Order);
        Assert.Equal(2, picks[4].Order);
        Assert.Equal(2, panel.PickedCount);
    }

    /// <summary>校验回显: CanConfirm 要求 fileExt 非空条件值 + 至少勾选一个行为 (两条件独立回显)。</summary>
    [Fact]
    public void AddMappingVm_CanConfirm_Requires_Value_And_Pick()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;

        // 新语义: 默认选中第一个分组 (图片), 条件值已自动填充 = 该组后缀集; 只需勾行为
        Assert.False(string.IsNullOrEmpty(panel.MatchValue)); // 分组项自带后缀集
        Assert.False(panel.CanConfirm); // 有值但未勾行为

        panel.BehaviorPicks[0].IsChecked = true;
        Assert.True(panel.CanConfirm);
    }

    /// <summary>分组快捷填入: 选分组填后缀串 (值变化驱动 CanConfirm 刷新)。</summary>
    [Fact]
    public void AddMappingVm_FileGroup_Fill_Drives_CanConfirm()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;

        panel.FileGroupSelected = panel.FileGroupOptions.First(o => o.Value == "image");
        Assert.Equal("jpg, png", panel.MatchValue);
        Assert.False(panel.CanConfirm); // 有值但未勾行为

        panel.BehaviorPicks[0].IsChecked = true;
        Assert.True(panel.CanConfirm);
    }

    // ------------------------------------------------------------- 弹窗落位

    /// <summary>确认落位: fileExt -> FileMappings 尾部, entries 顺序 = 勾选顺序 (即菜单键位), 弹窗关闭。</summary>
    [Fact]
    public void AddMapping_FileExt_Goes_To_FilePartition_With_PickOrder()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;
        panel.MatchValue = "jpg";
        panel.BehaviorPicks.First(p => p.Pack.Id == "open").IsChecked = true;
        panel.BehaviorPicks.First(p => p.Pack.Id == "run").IsChecked = true;

        page.AddMapping(panel);

        Assert.Null(page.AddPanel);
        Assert.True(page.HasAnyMappings);
        Assert.Empty(page.TextMappings);
        var row = Assert.Single(page.FileMappings);
        Assert.Equal("jpg", row.Mapping.MatchValue);
        Assert.Equal(["open", "run"], row.Mapping.Entries.Select(e => e.Behavior)); // 列表位置序
    }

    /// <summary>确认落位: 文本特征类型 -> TextMappings, MatchValue = 特征词本身。</summary>
    [Fact]
    public void AddMapping_TextType_Goes_To_TextPartition()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;
        panel.TypeSelected = panel.TypeOptions.OfType<ComboOption>().First(o => o.Value == "magnet");
        panel.BehaviorPicks.First(p => p.Pack.Id == "magnet_download").IsChecked = true;

        page.AddMapping(panel);

        Assert.Empty(page.FileMappings);
        var row = Assert.Single(page.TextMappings);
        Assert.Equal("textType", row.Mapping.MatchType);
        Assert.Equal("magnet", row.Mapping.MatchValue);
        Assert.Equal(["magnet_download"], row.Mapping.Entries.Select(e => e.Behavior));
    }

    /// <summary>零勾选确认: 不创建映射, 弹窗保持打开 (由用户显式取消)。</summary>
    [Fact]
    public void AddMapping_Without_Pick_Does_Nothing()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;
        panel.MatchValue = "jpg"; // 有值但零勾选

        page.AddMapping(panel);

        Assert.Same(panel, page.AddPanel); // 弹窗未关闭
        Assert.False(page.HasAnyMappings);
        Assert.Empty(page.FileMappings);
        Assert.Empty(page.TextMappings);
    }

    // ------------------------------------------------------------- 热键 / 开关 / 仲裁

    /// <summary>热键 setter 写模型并置未保存标记; 空热键警示联动。</summary>
    [Fact]
    public void Hotkey_Set_Writes_Model_And_Marks_PendingSave()
    {
        var (page, config) = CreatePage();
        Assert.True(page.NoHotkeyWarning); // 出厂默认空热键

        page.Hotkey = ">^p";
        Assert.Equal(">^p", config.SelectedAction.Hotkey);
        Assert.True(page.HotkeyPendingSave);
        Assert.False(page.NoHotkeyWarning);
    }

    /// <summary>UsedHotkeys 收集启用 keymaps 的热键 (禁用的不收)。</summary>
    [Fact]
    public void UsedHotkeys_Collected_From_EnabledKeymaps()
    {
        var config = new Config
        {
            Keymaps =
            [
                new Keymap { Id = 1, Name = "主", Enable = true, Hotkey = "CapsLock" },
                new Keymap { Id = 2, Name = "禁用", Enable = false, Hotkey = "F9" },
            ],
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg"] }],
        };
        var (page, _) = CreatePage(config);

        Assert.Contains(HotkeyLogic.NormalizeHotkey("CapsLock"), page.UsedHotkeys);
        Assert.DoesNotContain(HotkeyLogic.NormalizeHotkey("F9"), page.UsedHotkeys);
    }

    /// <summary>手风琴仲裁: 同屏只开一个 —— 新展开收起旧行 (编辑器清空), 再展开即重建。</summary>
    [Fact]
    public void ExpandedRow_Only_One_Open_At_A_Time()
    {
        var (page, _) = CreatePage();
        var row1 = NewFileRow(page, "open");
        Assert.True(row1.IsExpanded); // NewFileRow 展开 row1
        var row2 = NewUrlRow(page, "open_url");

        Assert.True(row2.IsExpanded); // 展开 row2
        Assert.Same(row2, page.ExpandedRow);
        Assert.False(row1.IsExpanded); // row1 被仲裁收起
        Assert.Empty(row1.Editors);    // 编辑器清空
        Assert.NotEmpty(row2.Editors);

        page.ExpandedRow = null; // 全部收起
        Assert.False(row2.IsExpanded);
        Assert.Empty(row2.Editors);
    }

    /// <summary>删除映射: 未注入确认框委托时不删除 (防御性回退, 不静默丢数据)。</summary>
    [Fact]
    public async Task AskRemove_Without_ConfirmDelegate_Does_Not_Remove()
    {
        var (page, _) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;
        panel.MatchValue = "jpg";
        panel.BehaviorPicks[0].IsChecked = true;
        page.AddMapping(panel);
        var row = Assert.Single(page.FileMappings);

        await page.AskRemoveAsync(row); // ConfirmAsync == null -> 拒绝

        // AddMapping 未同步模型 (SyncToModel 在保存咽喉处), 仅断言页面分区
        Assert.Single(page.FileMappings);
    }

    /// <summary>删除映射: 确认后移除并立即保存 (SaveAsync 无 Api 时安全返回 false, 内存态已一致)。</summary>
    [Fact]
    public async Task AskRemove_Confirmed_Removes_Row()
    {
        var (page, config) = CreatePage();
        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;
        panel.MatchValue = "jpg";
        panel.BehaviorPicks[0].IsChecked = true;
        page.AddMapping(panel);
        var row = Assert.Single(page.FileMappings);
        page.ConfirmAsync = (_, _) => Task.FromResult(true);

        await page.AskRemoveAsync(row);

        Assert.Empty(page.FileMappings);
        Assert.False(page.HasAnyMappings);
        // SyncToModel 已把空分区写回模型
        Assert.Empty(config.SelectedAction.Mappings);
    }

    /// <summary>行排序: 分区内移动 (组内行序 = 优先级), 不跨分区; 边界禁用。</summary>
    [Fact]
    public void MoveMapping_Within_Partition_Only()
    {
        var (page, _) = CreatePage();
        var f1 = NewFileRow(page, "open");
        var t1 = NewUrlRow(page, "open_url");
        var f2 = NewFileRow(page, "run");

        // 边界: 首行不能上移, 尾行不能下移
        Assert.False(f1.CanMoveUp);
        Assert.True(f1.CanMoveDown);
        Assert.False(f2.CanMoveDown);

        page.MoveMapping(f2, -1);
        Assert.Equal([f2, f1], page.FileMappings);
        // textType 分区不受影响
        Assert.Equal([t1], page.TextMappings);
    }

    // ------------------------------------------------------------- 保存链路投影 (评审 C1)

    /// <summary>
    /// 评审 C1 语义锁定: 新增映射只动页面集合 (SyncToModel 位于保存咽喉处, AddMapping 不投影);
    /// MainViewModel.SaveAsync 现于节流判断前代调 ActionVm.SyncToModel —— 本测试锁定该投影步骤:
    /// 调用后新增 mapping 进入 config.SelectedAction.Mappings, 即 Ctrl+S 保存载荷与 GET /config 可见。
    /// </summary>
    [Fact]
    public void SyncToModel_Projects_NewMapping_Into_Model()
    {
        var (page, config) = CreatePage();
        Assert.Empty(config.SelectedAction.Mappings);

        page.OpenAddPanelCommand.Execute(null);
        var panel = page.AddPanel!;
        panel.MatchValue = "jpg";
        panel.BehaviorPicks.First(p => p.Pack.Id == "open").IsChecked = true;
        page.AddMapping(panel);
        Assert.Single(page.FileMappings);
        // 投影前: 模型尚无该 mapping (AddMapping 只动集合, 与删除测试注释口径一致)
        Assert.Empty(config.SelectedAction.Mappings);

        page.SyncToModel();

        var m = Assert.Single(config.SelectedAction.Mappings);
        Assert.Equal("fileExt", m.MatchType);
        Assert.Equal("jpg", m.MatchValue);
        Assert.Equal(["open"], m.Entries.Select(e => e.Behavior));
    }
}
