using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 「文件后缀 (fileExt)」语境行为下拉按前提过滤 + 分组关联生命周期 + 后缀修改保存写回 的单元测试。
/// 纯 ViewModel 测试 (无 Avalonia 宿主): 构造链 MainViewModel -> SelectedActionPageViewModel ->
/// MappingRowVm 均为内存对象 (BackendSession 构造只存参不拉起子进程, Config 直接注入, ISettingsApi 不被触达)。
/// 行为选项自 2026-09 起由行为包 appliesTo 覆盖推导 (BehaviorCatalog, CONTRACTS §3.9):
/// 文件语境覆盖集 = 通配内置行为 6 项, 文本专用行为不出现, 脏值恒插首位。
/// 分组关联语义 (评审 F1/F2, 方案 D): 关联存于行 VM 的 AssociatedGroupName —— 选分组建立 /
/// 清空值解除 / 初始按值推导 / 手改后缀保持; 写回经 ApplyFileGroupWriteBack (SaveAsync 咽喉调用)。
/// 行 VM 生命周期 = mapping 生命周期, 无旧编辑器的页面级关联映射 (F5-3 场景由「收起再展开」承接)。
/// </summary>
[Collection("I18nSerial")]
public sealed class FileGroupActionFilterTests
{
    /// <summary>与 data/config.json 默认分组一致的夹具 (image/code 两组足够覆盖过滤与写回)。</summary>
    private static readonly string[] ImageExts = ["jpg", "jpeg", "png", "gif", "bmp", "webp", "svg", "ico"];
    private static readonly string[] CodeExts = ["c", "cpp", "h", "py", "go", "rs", "json"];

    /// <summary>行为目录种子 (CONTRACTS §3.9): 内置 11 包 + 空用户包, 无条件重播种保证测试顺序无关
    /// (SeedForTests 为整表替换; 若守卫跳过, BehaviorCatalogTests 先跑时其用户包夹具会泄漏进本类断言)。</summary>
    private static void EnsureBehaviorCatalog()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
    }

    private static Config BuildConfig() => new()
    {
        FileGroups =
        [
            new FileGroup { Name = "image", Label = "图片", Exts = [.. ImageExts] },
            new FileGroup { Name = "code", Label = "代码", Exts = [.. CodeExts] },
        ],
    };

    /// <summary>构造页面 VM (空 selectedAction + 分组夹具)。</summary>
    private static (SelectedActionPageViewModel Page, Config Config) CreatePage() => CreatePage(BuildConfig());

    /// <summary>重载: 注入自定义 Config (二义同集分组 / 带点存量分组等场景)。</summary>
    private static (SelectedActionPageViewModel Page, Config Config) CreatePage(Config config)
    {
        EnsureBehaviorCatalog();
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = config;
        return (new SelectedActionPageViewModel(main), config);
    }

    /// <summary>
    /// 构造单条 fileExt 行并展开手风琴 (MatchValue=matchValue, 首个行为=behavior, 模板=actionValue)。
    /// 行挂入 FileMappings 分区 (仲裁/写回均遍历分区集合) 后再展开 (page.ExpandedRow = row)。
    /// </summary>
    private static (SelectedActionPageViewModel Page, MappingRowVm Row, EntryRowVm Editor, Config Config) CreateHost(
        string matchValue, string behavior, string actionValue = "")
        => CreateHost(BuildConfig(), matchValue, behavior, actionValue);

    private static (SelectedActionPageViewModel Page, MappingRowVm Row, EntryRowVm Editor, Config Config) CreateHost(
        Config config, string matchValue, string behavior, string actionValue = "")
    {
        var (page, cfg) = CreatePage(config);
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "fileExt",
            MatchValue = matchValue,
            Entries = [new SelectedEntry { Behavior = behavior, ActionValue = actionValue, Options = new RuleOptions() }],
        });
        page.FileMappings.Add(row); // 行必须挂到分区: 手风琴仲裁与 ApplyFileGroupWriteBack 都遍历分区集合
        page.ExpandedRow = row; // 展开手风琴 -> OpenEditor
        return (page, row, row.Editors[0], cfg);
    }

    /// <summary>行为下拉候选值集合 (无序比较用)。</summary>
    private static HashSet<string> OptionValues(EntryRowVm editor)
        => editor.BehaviorOptions.Select(o => o.Value).ToHashSet();

    private static readonly HashSet<string> FileCovering =
        new() { "copy", "open", "open_folder", "open_path", "run", "script" };

    // ------------------------------------------------------------- 行为下拉过滤

    /// <summary>关联 image: 文件语境覆盖集 = 通配内置行为 6 项 (专属无), 按目录序排列;
    /// 文本专用行为 (magnet_download/open_url/send_keys/search) 不出现。</summary>
    [Fact]
    public void BehaviorOptions_With_Image_Association_Shows_Covering_Generic_Set()
    {
        var (_, row, editor, _) = CreateHost(string.Join(", ", ImageExts), "open");
        Assert.Equal(["copy", "open", "open_folder", "open_path", "run", "script"],
            editor.BehaviorOptions.Select(o => o.Value));
        Assert.Equal("image", row.AssociatedGroupName); // 值 ≡ image 全集 -> 初始关联成立
    }

    /// <summary>关联 code: 覆盖集与 image 相同 (通配行为对任意后缀恒适用, 2026-09 语义)。</summary>
    [Fact]
    public void BehaviorOptions_With_Code_Association_Shows_Same_Covering_Set()
    {
        var (_, _, editor, _) = CreateHost(string.Join(", ", CodeExts), "script");
        Assert.Equal(FileCovering, OptionValues(editor));
    }

    /// <summary>无关联 (手输后缀): 覆盖集同为通配行为 6 项。</summary>
    [Fact]
    public void BehaviorOptions_Without_Association_Shows_Covering_Generic_Set()
    {
        var (_, row, editor, _) = CreateHost("abc,xyz", "run");
        Assert.Equal(FileCovering, OptionValues(editor));
        Assert.Null(row.AssociatedGroupName);
    }

    /// <summary>
    /// 存量脏值 (关联 image 但行为是 magnet_download): 脏值恒插首位保持可见可选,
    /// 其余仍按前提过滤 (send_keys 等文本专用行为不可见; open_registry 已并入 open_path)。
    /// </summary>
    [Fact]
    public void BehaviorOptions_Keeps_Dirty_Action_Visible_In_Associated_Group()
    {
        var (_, _, editor, _) = CreateHost(string.Join(", ", ImageExts), "magnet_download", "magnet:?xt=1");
        var values = editor.BehaviorOptions.Select(o => o.Value).ToList();
        Assert.Equal("magnet_download", values[0]); // 脏值插首位
        Assert.DoesNotContain("send_keys", values);
        Assert.DoesNotContain("open_registry", values); // 行为包已删, 回归护栏
        Assert.DoesNotContain("search", values);
        Assert.Equal("magnet_download", editor.Entry.Behavior);
        Assert.Equal("magnet:?xt=1", editor.ActionValue); // 脏载荷不被构造期清洗
    }

    /// <summary>textType(url) 行: 覆盖集 = 文本专属 {open_url} (2026-09-10 起 search 不适用), 文件行为不可见 (类型过滤)。</summary>
    [Fact]
    public void BehaviorOptions_TextType_Url_Shows_TextOnly_Set()
    {
        var (page, _) = CreatePage();
        var row = new MappingRowVm(page, new SelectedMapping
        {
            MatchType = "textType",
            MatchValue = "url",
            Entries = [new SelectedEntry { Behavior = "open_url", Options = new RuleOptions() }],
        });
        page.ExpandedRow = row;
        Assert.Equal(new HashSet<string> { "open_url" }, OptionValues(row.Editors[0]));
    }

    // ------------------------------------------------------------- 分组选择联动

    /// <summary>选分组: 填充分组后缀串并建立关联 (通配覆盖行为如 open 保留 —— 适用任意文件前提)。</summary>
    [Fact]
    public void OnFileGroupSelected_Fills_ExtList_And_Associates()
    {
        var (_, row, editor, _) = CreateHost("", "open");
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == "image");

        Assert.Equal(string.Join(", ", ImageExts), row.MatchValueDisplay);
        Assert.Equal("image", row.AssociatedGroupName);
        Assert.Equal("open", editor.Entry.Behavior); // 通配覆盖行为保留
    }

    /// <summary>当前行为磁力下载 (仅覆盖文本前提, 不覆盖任何文件前提): 选分组后重绑为该前提
    /// 默认行为, 残留载荷随模板重置 (2026-09-10 语义: 分组 Toggle = 离散前提切换, 同 textType)。</summary>
    [Fact]
    public void OnFileGroupSelected_Rebinds_NonCovering_Entry_To_Premise_Default()
    {
        var (_, row, editor, _) = CreateHost("", "magnet_download", "magnet:?xt=1");
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == "image");

        var def = BehaviorCatalog.DefaultFor("fileExt", string.Join(", ", ImageExts));
        Assert.Equal(def, editor.Entry.Behavior);
        Assert.Equal(BehaviorCatalog.DefaultTemplateFor(def), editor.Entry.ActionValue); // 载荷随模板重置
        Assert.Equal(string.Join(", ", ImageExts), row.MatchValueDisplay);
        Assert.Equal("image", row.AssociatedGroupName);
    }

    /// <summary>选中「无」: 清空条件值并解除关联 (行为过滤回退全文件集)。</summary>
    [Fact]
    public void OnFileGroupSelected_None_Clears_MatchValue_And_Disassociates()
    {
        var (_, row, editor, _) = CreateHost(string.Join(", ", ImageExts), "open");
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == ""); // 「无」

        Assert.Equal("", row.MatchValueDisplay);
        Assert.Null(row.AssociatedGroupName);
        Assert.Null(row.FileGroupSelected); // 下拉回退未选态
        Assert.Equal(FileCovering, OptionValues(editor));
    }

    // ------------------------------------------------------------- 分组 Toggle (行头直选, 替代下拉)

    /// <summary>
    /// 分组 Toggle 互斥点亮: 点亮 image 填入并关联; 再点亮 code 互斥换位;
    /// 再点已亮项熄灭 = 「无」 (清空条件值 + 解除关联)。勾选态由 FileGroupSelected 派生,
    /// 行 VM 广播同步 (与 textType 同一派生属性教训)。
    /// </summary>
    [Fact]
    public void GroupToggles_Mutually_Exclusive_And_Drive_Fill_Clear()
    {
        var (_, row, _, _) = CreateHost("", "open");
        var tImage = row.GroupToggles.First(t => t.Name == "image");
        var tCode = row.GroupToggles.First(t => t.Name == "code");

        tImage.IsChecked = true;
        Assert.True(tImage.IsChecked);
        Assert.False(tCode.IsChecked); // 互斥
        Assert.Equal(string.Join(", ", ImageExts), row.MatchValueDisplay);
        Assert.Equal("image", row.AssociatedGroupName);

        tCode.IsChecked = true; // 换组: 旧项熄灭, 值与关联随新组
        Assert.True(tCode.IsChecked);
        Assert.False(tImage.IsChecked);
        Assert.Equal(string.Join(", ", CodeExts), row.MatchValueDisplay);
        Assert.Equal("code", row.AssociatedGroupName);

        tCode.IsChecked = false; // 熄灭已亮项 = 「无」
        Assert.False(tCode.IsChecked);
        Assert.False(tImage.IsChecked);
        Assert.Equal("", row.MatchValueDisplay);
        Assert.Null(row.AssociatedGroupName);
    }

    /// <summary>初始关联推导同步点亮对应 Toggle (构造链: ResolveInitialAssociation -> RefreshGroupToggles)。</summary>
    [Fact]
    public void GroupToggles_Reflect_Initial_Association()
    {
        var (_, row, _, _) = CreateHost(string.Join(", ", ImageExts), "open");
        Assert.True(row.GroupToggles.First(t => t.Name == "image").IsChecked);
        Assert.False(row.GroupToggles.First(t => t.Name == "code").IsChecked);
    }

    /// <summary>专属后缀行为包随分组 Toggle 联动: ps_edit (jpg/png 专属) 在 image 组覆盖保留,
    /// 切到 code 组后不覆盖 -> 重绑为该前提默认行为 (展开 = 所见即当前前提生效的行为)。</summary>
    [Fact]
    public void GroupToggle_Switch_Rebinds_NonCovering_Specific_Pack()
    {
        var (_, row, _, _) = CreateHost(string.Join(", ", ImageExts), "ps_edit", "ps.exe %{selected%}");
        // CreateHost 内部已重播种内置包; 追加用户专属包 (不影响已构造行的数据引用)
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(),
        [
            new BehaviorPack
            {
                Id = "ps_edit", Name = "PS 编辑图片", NameEn = "PS Edit", SpecVersion = 1,
                AppliesTo = [new BehaviorAppliesTo { Type = "fileExt", Exts = ["jpg", "png"] }],
                Entry = new BehaviorEntry { Kind = "builtin", Action = "run" },
                Source = "user",
            },
        ]);
        Assert.Equal("ps_edit", row.Mapping.Entries[0].Behavior); // image 前提下专属包覆盖保留

        row.GroupToggles.First(t => t.Name == "code").IsChecked = true; // 切到 code 组

        var def = BehaviorCatalog.DefaultFor("fileExt", string.Join(", ", CodeExts));
        Assert.Equal(def, row.Mapping.Entries[0].Behavior);
        Assert.Equal(BehaviorCatalog.DefaultTemplateFor(def), row.Mapping.Entries[0].ActionValue);
        Assert.Equal(string.Join(", ", CodeExts), row.MatchValueDisplay);
        Assert.Equal("code", row.AssociatedGroupName);
    }

    /// <summary>点「无」(熄灭已亮 Toggle): 前提回退通用文件集, 专属包不覆盖通配前提 -> 同样重绑。</summary>
    [Fact]
    public void GroupToggle_None_Rebinds_NonCovering_Specific_Pack()
    {
        var (_, row, _, _) = CreateHost(string.Join(", ", ImageExts), "ps_edit", "ps.exe %{selected%}");
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(),
        [
            new BehaviorPack
            {
                Id = "ps_edit", Name = "PS 编辑图片", NameEn = "PS Edit", SpecVersion = 1,
                AppliesTo = [new BehaviorAppliesTo { Type = "fileExt", Exts = ["jpg", "png"] }],
                Entry = new BehaviorEntry { Kind = "builtin", Action = "run" },
                Source = "user",
            },
        ]);

        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == ""); // 「无」

        var def = BehaviorCatalog.DefaultFor("fileExt", "");
        Assert.Equal(def, row.Mapping.Entries[0].Behavior);
        Assert.Equal(BehaviorCatalog.DefaultTemplateFor(def), row.Mapping.Entries[0].ActionValue);
        Assert.Equal("", row.MatchValueDisplay);
        Assert.Null(row.AssociatedGroupName);
    }

    /// <summary>
    /// 前提快照记忆 (2026-09-10 bug 修复): 图片组配专属行为 -> 切文档组重绑为通配默认
    /// -> 切回图片组应还原之前配的行为, 而非被通配默认 (open_path) 永久吞掉。
    /// </summary>
    [Fact]
    public void GroupToggle_SwitchBack_Restores_Previous_Premise_Snapshot()
    {
        var (_, row, _, _) = CreateHost(string.Join(", ", ImageExts), "ps_edit", "ps.exe %{selected%}");
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(),
        [
            new BehaviorPack
            {
                Id = "ps_edit", Name = "PS 编辑图片", NameEn = "PS Edit", SpecVersion = 1,
                AppliesTo = [new BehaviorAppliesTo { Type = "fileExt", Exts = ["jpg", "png"] }],
                Entry = new BehaviorEntry { Kind = "builtin", Action = "run" },
                Source = "user",
            },
        ]);

        row.GroupToggles.First(t => t.Name == "code").IsChecked = true; // 切走: 重绑为通配默认
        var def = BehaviorCatalog.DefaultFor("fileExt", string.Join(", ", CodeExts));
        Assert.Equal(def, row.Mapping.Entries[0].Behavior);

        row.GroupToggles.First(t => t.Name == "image").IsChecked = true; // 切回: 快照还原
        Assert.Equal("ps_edit", row.Mapping.Entries[0].Behavior);
        Assert.Equal("ps.exe %{selected%}", row.Mapping.Entries[0].ActionValue); // 模板一并还原
        Assert.Equal(string.Join(", ", ImageExts), row.MatchValueDisplay);
        Assert.Equal("image", row.AssociatedGroupName);
    }

    // ------------------------------------------------------------- 初始关联推导

    /// <summary>条件值 = 分组后缀全集的乱序/大小写变体: 构造时重建关联 (SameExts 归一化比较)。</summary>
    [Fact]
    public void Initial_Association_Rebuilt_From_Shuffled_Case_Variant()
    {
        var (_, row, editor, _) = CreateHost("WEBP, svg, ICO, jpg, JPEG, Png, gif, BMP", "open");
        Assert.Equal("image", row.AssociatedGroupName);
        Assert.Equal("image", row.FileGroupSelected?.Value); // 下拉回显分组
        Assert.Equal(FileCovering, OptionValues(editor));
    }

    /// <summary>自定义后缀串 (组内子集, 不与任何分组全集一致): 不关联, 快捷填入保持未选。</summary>
    [Fact]
    public void Initial_No_Association_For_Custom_Ext_List()
    {
        var (_, row, _, _) = CreateHost("jpg, png", "open"); // image 组的子集 != 全集
        Assert.Null(row.AssociatedGroupName);
        Assert.Null(row.FileGroupSelected);
    }

    // ------------------------------------------------------------- 保存写回 (F1 咽喉)

    /// <summary>关联 image 后手改条件值: 写回更新 Config.FileGroups[image].Exts (以当前书写形式归一化)。</summary>
    [Fact]
    public void ApplyFileGroupWriteBack_Saves_Edited_ExtList_To_Group()
    {
        var (page, row, _, config) = CreateHost(string.Join(", ", ImageExts), "open");
        row.MatchValueDisplay = "jpg, png, webp";
        page.ApplyFileGroupWriteBack();

        var group = config.FileGroups.First(g => g.Name == "image");
        Assert.Equal(new[] { "jpg", "png", "webp" }, group.Exts);
        // 其他分组不受影响
        Assert.Equal(CodeExts, config.FileGroups.First(g => g.Name == "code").Exts);
    }

    /// <summary>条件值清空: 不写回 (组定义保持原样)。</summary>
    [Fact]
    public void ApplyFileGroupWriteBack_Empty_MatchValue_Does_Not_Touch_Group()
    {
        var (page, row, _, config) = CreateHost(string.Join(", ", ImageExts), "open");
        row.MatchValueDisplay = "";
        page.ApplyFileGroupWriteBack();

        Assert.Equal(ImageExts, config.FileGroups.First(g => g.Name == "image").Exts);
    }

    /// <summary>无关联 (手输后缀): 不写回任何分组。</summary>
    [Fact]
    public void ApplyFileGroupWriteBack_Without_Association_Does_Not_Touch_Groups()
    {
        var (page, _, _, config) = CreateHost("custom1, custom2", "open");
        page.ApplyFileGroupWriteBack();

        Assert.Equal(ImageExts, config.FileGroups.First(g => g.Name == "image").Exts);
        Assert.Equal(CodeExts, config.FileGroups.First(g => g.Name == "code").Exts);
    }

    /// <summary>条件值与分组内容仅大小写/顺序差异: 视为一致, 不写回。</summary>
    [Fact]
    public void ApplyFileGroupWriteBack_Same_Set_Different_Order_Case_Does_Not_Touch_Group()
    {
        var (page, row, _, config) = CreateHost(string.Join(", ", ImageExts), "open");
        row.MatchValueDisplay = "WEBP, SVG, ICO, JPG, JPEG, PNG, GIF, BMP"; // 同集合, 乱序+大小写
        page.ApplyFileGroupWriteBack();

        Assert.Equal(ImageExts, config.FileGroups.First(g => g.Name == "image").Exts);
    }

    /// <summary>
    /// F4 动机: 带点存量分组与归一化条件值视为一致 —— 建立关联且写回不重写 (保留存量书写形式)。
    /// </summary>
    [Fact]
    public void WriteBack_Does_Not_Rewrite_Dotted_Group_When_Normalized_Equal()
    {
        var config = BuildConfig();
        config.FileGroups[0].Exts = [".jpg", ".png"]; // 存量带点书写
        var (page, row, editor, cfg) = CreateHost(config, "jpg, png", "open");

        // 归一化后一致 -> 建立关联, 行为下拉按前提过滤 (值集与分组一致)
        Assert.Equal("image", row.AssociatedGroupName);
        Assert.Equal(FileCovering, OptionValues(editor));
        page.ApplyFileGroupWriteBack(); // 同集 (归一化意义下) -> 不重写
        Assert.Equal(new[] { ".jpg", ".png" }, cfg.FileGroups.First(g => g.Name == "image").Exts);
    }

    /// <summary>
    /// F5-6: 两组同后缀集二义 —— 无显式关联时按值推导取声明序首个 (FirstOrDefault);
    /// 显式关联后者后, 关联存于行 VM (生命周期 = mapping), 收起再展开不丢失, 写回指向后者。
    /// </summary>
    [Fact]
    public void Ambiguous_Same_Ext_Groups_Explicit_Association_Wins_WriteBack()
    {
        var config = new Config
        {
            FileGroups =
            [
                new FileGroup { Name = "photo", Label = "照片", Exts = ["jpg", "png"] },
                new FileGroup { Name = "image2", Label = "图片二", Exts = ["jpg", "png"] },
            ],
        };
        var (page, row, editor, cfg) = CreateHost(config, "jpg, png", "open");

        // 无显式关联: 首次按值推导二义 -> FirstOrDefault 取声明序首个
        Assert.Equal("photo", row.AssociatedGroupName);

        // 显式关联后者
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == "image2");
        Assert.Equal("image2", row.AssociatedGroupName);

        // 收起再展开 (编辑器重建, 行 VM 不销毁): 关联不丢失
        page.ExpandedRow = null;
        page.ExpandedRow = row;
        Assert.Equal("image2", row.AssociatedGroupName);
        Assert.Equal(FileCovering, OptionValues(row.Editors[0]));

        // 行为断言: 写回指向 image2, photo 不受影响
        row.MatchValueDisplay = "jpg, png, bmp";
        page.ApplyFileGroupWriteBack();
        Assert.Equal(new[] { "jpg", "png", "bmp" }, cfg.FileGroups.First(g => g.Name == "image2").Exts);
        Assert.Equal(new[] { "jpg", "png" }, cfg.FileGroups.First(g => g.Name == "photo").Exts);
    }

    // ------------------------------------------------------------- F2 关联生命周期

    /// <summary>
    /// F5-1: 「无」解除关联后, 手改条件值不再按值重建; 经写回公有行为断言:
    /// 改条件值后分组内容不变 —— 解除意图不被回滚。
    /// </summary>
    [Fact]
    public void Dissociate_By_None_Survives_Subsequent_Edit_And_WriteBack()
    {
        var (page, row, _, config) = CreateHost(string.Join(", ", ImageExts), "open");
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == ""); // 「无」解除
        Assert.Null(row.AssociatedGroupName);

        row.MatchValueDisplay = "jpg, png, bmp"; // 手填新值 (不与分组全集一致, 也无人重建关联)
        page.ApplyFileGroupWriteBack();
        Assert.Equal(ImageExts, config.FileGroups.First(g => g.Name == "image").Exts);
    }

    /// <summary>
    /// F5-2 (1007 承诺): 选分组 → 手改后缀 → 换行为类型 → 写回仍更新分组
    /// (关联不因「值不再 ≡ 分组」或行为变化被解除)。
    /// </summary>
    [Fact]
    public void Edited_ExtList_Survives_Behavior_Change_WriteBack()
    {
        var (page, row, editor, config) = CreateHost("", "open");
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == "image"); // 建立关联
        row.MatchValueDisplay = "jpg, png"; // 手改后缀 (不再 ≡ 分组全集)
        editor.BehaviorSelected = editor.BehaviorOptions.First(o => o.Value == "copy"); // 换行为类型

        page.ApplyFileGroupWriteBack();
        Assert.Equal(new[] { "jpg", "png" }, config.FileGroups.First(g => g.Name == "image").Exts);
        Assert.Equal("copy", editor.Entry.Behavior); // 行为切换生效
    }

    /// <summary>
    /// F5-3 改造: 显式关联 + 手改后缀 → 收起再展开 (编辑器重建) → 行 VM 关联保持 → 再写回仍生效。
    /// (方案 D 无页面级关联映射: 行 VM 生命周期 = mapping 生命周期, 天然满足。)
    /// </summary>
    [Fact]
    public void Association_Survives_Editor_Reopen_And_WriteBack()
    {
        var (page, row, _, config) = CreateHost("", "open");
        row.FileGroupSelected = row.FileGroupOptions.First(o => o.Value == "image");
        row.MatchValueDisplay = "jpg, webp"; // 手改后缀

        page.ExpandedRow = null; // 收起
        page.ExpandedRow = row;  // 再展开 (OpenEditor 重建编辑行)
        Assert.Equal("image", row.AssociatedGroupName);
        Assert.Equal("jpg, webp", row.MatchValueDisplay);

        row.MatchValueDisplay = "jpg, gif";
        page.ApplyFileGroupWriteBack();
        Assert.Equal(new[] { "jpg", "gif" }, config.FileGroups.First(g => g.Name == "image").Exts);
    }

    // ------------------------------------------------------------- NormalizeExts

    /// <summary>混合分隔符/前导点/空 token/重复 (忽略大小写保留首个书写形式)。</summary>
    [Fact]
    public void NormalizeExts_Handles_Separators_Points_Empty_And_Dupes()
    {
        Assert.Equal(new[] { "JPG", "png", "gif", "webp" },
            ActionSchemeCatalog.NormalizeExts("JPG, .png,, gif; webp"));
        Assert.Equal(new[] { "jpg" }, ActionSchemeCatalog.NormalizeExts("jpg、JPG"));   // 去重保留首个
        Assert.Equal(new[] { "a", "b" }, ActionSchemeCatalog.NormalizeExts("，a;；b、")); // 全角分隔符
        Assert.Empty(ActionSchemeCatalog.NormalizeExts(""));
        Assert.Empty(ActionSchemeCatalog.NormalizeExts(null));
        Assert.Empty(ActionSchemeCatalog.NormalizeExts("  , ., ; "));
    }

    /// <summary>F4: NormalizeExts 两端去点 (含尾点+尾随空格); SameExts 双侧归一化。</summary>
    [Fact]
    public void NormalizeExts_Trims_Both_Ends_And_SameExts_Normalizes_Both_Sides()
    {
        Assert.Equal(new[] { "jpg", "png" }, ActionSchemeCatalog.NormalizeExts(".jpg, png."));
        Assert.Equal(new[] { "jpg" }, ActionSchemeCatalog.NormalizeExts("jpg. ")); // 尾点死条目
        // 双侧归一化: 带点书写与归一化值视为一致
        Assert.True(ActionSchemeCatalog.SameExts(new[] { "jpg", "png" }, new[] { ".jpg", "png." }));
        Assert.True(ActionSchemeCatalog.SameExts(new[] { ".jpg." }, new[] { "jpg" }));
        Assert.False(ActionSchemeCatalog.SameExts(new[] { "jpg" }, new[] { "png" }));
    }
}
