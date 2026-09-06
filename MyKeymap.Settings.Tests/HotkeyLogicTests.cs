using Avalonia.Input;
using MyKeymap.Settings.Models;
using MyKeymap.Settings.Services;

namespace MyKeymap.Settings.Tests;

/// <summary>
/// HotkeyCapture 控件纯逻辑层单测 (对照 config-ui/src/components/action/constants.ts
/// L160-245 与 HotkeyCapture.vue 的规格):
///   - 左右修饰键区分 (浏览器 KeyboardEvent.code -> Avalonia 物理键枚举);
///   - MOD_ORDER 固定排序;
///   - normalizeHotkey 冲突归一化;
///   - Esc 取消 / 失焦取消 / 修饰松开撤暂存。
/// </summary>
public sealed class HotkeyLogicTests
{
    // ------------------------------------------------------------- 左右修饰键区分

    [Theory]
    [InlineData(Key.LeftCtrl, "<^", "LCtrl")]
    [InlineData(Key.RightCtrl, ">^", "RCtrl")]
    [InlineData(Key.LeftAlt, "<!", "LAlt")]
    [InlineData(Key.RightAlt, ">!", "RAlt")]
    [InlineData(Key.LeftShift, "<+", "LShift")]
    [InlineData(Key.RightShift, ">+", "RShift")]
    [InlineData(Key.LWin, "<#", "LWin")]
    [InlineData(Key.RWin, ">#", "RWin")]
    public void ModifierFor_LeftRight_EachMapsToDistinctPrefix(Key key, string prefix, string label)
    {
        var mod = HotkeyLogic.ModifierFor(key);
        Assert.NotNull(mod);
        Assert.Equal(prefix, mod!.Value.Prefix);
        Assert.Equal(label, mod.Value.Label);
    }

    [Fact]
    public void ModifierFor_NonModifier_ReturnsNull()
    {
        Assert.Null(HotkeyLogic.ModifierFor(Key.A));
        Assert.Null(HotkeyLogic.ModifierFor(Key.F12));
        Assert.Null(HotkeyLogic.ModifierFor(Key.Space));
    }

    /// <summary>8 个左右修饰前缀 × 主键抽样: 2026-09 起捕获区分左右 ——
    /// 提交保留侧别前缀 (&lt;^a 只响应左 Ctrl+A), 显示 LCtrl/RCtrl。</summary>
    [Theory]
    [InlineData(Key.LeftCtrl, Key.A, "<^a")]
    [InlineData(Key.RightCtrl, Key.A, ">^a")]
    [InlineData(Key.LeftAlt, Key.F5, "<!F5")]
    [InlineData(Key.RightAlt, Key.F5, ">!F5")]
    [InlineData(Key.LeftShift, Key.D7, "<+7")]
    [InlineData(Key.RightShift, Key.D7, ">+7")]
    [InlineData(Key.LWin, Key.Q, "<#q")]
    [InlineData(Key.RWin, Key.Q, ">#q")]
    public void Capture_LeftRightModifiers_ArePreservedAsSideSpecific(Key modifier, Key main, string expected)
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(modifier, anyModifierHeld: true);
        core.HandleKeyDown(main, anyModifierHeld: true);
        Assert.Null(committed);      // 只暂存, 不自动提交 (硬性要求 Enter/Esc 退出)
        Assert.True(core.Capturing);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal(expected, committed);
        Assert.False(core.Capturing);
    }

    // ------------------------------------------------------------- MOD_ORDER 排序

    [Fact]
    public void BuildAhk_PrefixesOrderedByModOrder()
    {
        // 乱序输入 -> 按 MOD_ORDER ("<^",">^","<!",">!","<+",">+","<#",">#") 升序输出
        Assert.Equal(">^<+<#q", HotkeyLogic.BuildAhk([">^", "<#", "<+"], "q"));
        Assert.Equal("<^>!<+f", HotkeyLogic.BuildAhk([">!", "<^", "<+"], "f"));
    }

    [Fact]
    public void BuildAhk_DeduplicatesAndIgnoresUnknownPrefixes()
    {
        Assert.Equal("<^a", HotkeyLogic.BuildAhk(["<^", "<^", "???"], "a"));
    }

    [Fact]
    public void Capture_MultipleModifiers_OrderedByModOrder_RegardlessOfPressOrder()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.RightShift, anyModifierHeld: true); // 先按右 Shift
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);  // 后按左 Ctrl
        core.HandleKeyDown(Key.K, anyModifierHeld: true);
        Assert.Null(committed); // 只暂存
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("<^>+k", committed); // 区分左右: 保留侧别前缀, MOD_ORDER 归并顺序 (<^ 在 >+ 前)
    }

    // ------------------------------------------------------------- 侧别归一化 (NormalizeSidePrefixes)

    [Theory]
    [InlineData(">^p", "^p")]
    [InlineData("<^p", "^p")]
    [InlineData("^p", "^p")]           // 已是通配形式, 原样保留
    [InlineData("<^>!x", "^!x")]        // 多前缀逐个映射, 顺序不变
    [InlineData("<+7", "+7")]
    [InlineData(">#q", "#q")]
    [InlineData("p", "p")]              // 无前缀
    [InlineData("", "")]
    public void NormalizeSidePrefixes_MapsSidePrefixedModifiersToAgnosticOnes(string input, string expected)
        => Assert.Equal(expected, HotkeyLogic.NormalizeSidePrefixes(input));

    // ------------------------------------------------------------- normalizeHotkey

    [Theory]
    [InlineData("^+q", "^+q")]
    [InlineData("*~$^+Q", "^+q")]          // 去 *~$ 前缀 + 小写
    [InlineData("<^q", "^q")]               // 去左右前缀 (保守归一化)
    [InlineData(">^Q", "^q")]
    [InlineData("~<!f5", "!f5")]           // TrimStart 只去开头连续的 *~$<> 字符
    public void NormalizeHotkey_StripsWildcardAndLeftRightPrefixes_CaseInsensitive(string input, string expected)
        => Assert.Equal(expected, HotkeyLogic.NormalizeHotkey(input));

    [Theory]
    [InlineData("*~$<^Q", "<^q")]           // 只去 *~$, 保留左右前缀
    [InlineData("<^q", "<^q")]
    [InlineData("^Q", "^q")]
    public void StripWildcardPrefix_KeepsLeftRightPrefixes(string input, string expected)
        => Assert.Equal(expected, HotkeyLogic.StripWildcardPrefix(input));

    // ------------------------------------------------------------- AhkToDisplay

    [Theory]
    [InlineData("^+q", "Ctrl+Shift+Q")]
    [InlineData("<^q", "LCtrl+Q")]
    [InlineData(">+!up", "RShift+Alt+↑")]
    [InlineData("~^space", "^space")] // 复刻: ~ 在修饰符提取后才去除, ^ 在 ~ 后不被识别为修饰符 (Vue 同款怪癖)
    [InlineData("", "")]
    public void AhkToDisplay_MatchesVueSemantics(string ahk, string expected)
        => Assert.Equal(expected, HotkeyLogic.AhkToDisplay(ahk));

    // ------------------------------------------------------------- 捕获状态机: 取消语义

    [Fact]
    public void Capture_EscAlone_Cancels()
    {
        var core = new HotkeyCaptureCore();
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true); // 已有暂存也应取消
        core.HandleKeyDown(Key.Escape, anyModifierHeld: false);
        Assert.False(core.Capturing);
        Assert.Empty(core.PendingPrefixes);
    }

    [Fact]
    public void Capture_EscWithModifierHeld_Stages_Then_Enter_Commits()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.Escape, anyModifierHeld: true); // 修饰键按住: Esc 是普通键
        Assert.True(core.Capturing);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("<^esc", committed);
    }


    [Fact]
    public void Cancel_OnFocusLoss_ClearsPending()
    {
        var core = new HotkeyCaptureCore();
        core.StartCapture();
        core.HandleKeyDown(Key.LeftAlt, anyModifierHeld: true);
        core.Cancel(); // 视图层 OnLostFocus 调用
        Assert.False(core.Capturing);
        Assert.Empty(core.PendingPrefixes);

        // 取消后按键无效
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.HandleKeyDown(Key.A, anyModifierHeld: false);
        Assert.Null(committed);
    }

    [Fact]
    public void Capture_SingleModifier_Committed_Via_Enter()
    {
        // 2026-09: 单按 Ctrl 只暂存; Enter 确认提交物理侧键名 (区分左右)
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        Assert.True(core.Capturing);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("LCtrl", committed);
        Assert.False(core.Capturing);
    }


    [Fact]
    public void Capture_TwoModifiers_NoMain_Enter_Exits_Without_Commit()
    {
        // 多修饰键无主键不可表示: Enter 退出且不提交
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.LeftShift, anyModifierHeld: true);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Null(committed);
        Assert.False(core.Capturing);
    }


    [Fact]
    public void Capture_TwoKeys_Combo_Via_Enter()
    {
        // J+K: 首键暂存, 第二键集齐 (不自动提交), Enter 确认 AHK 自定义组合
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        Assert.Null(committed);
        Assert.Single(core.StagedKeys);
        core.HandleKeyDown(Key.K, anyModifierHeld: false);
        Assert.Null(committed);            // 集齐也不自动提交
        Assert.Equal(2, core.StagedKeys.Count);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("j & k", committed);
    }


    [Fact]
    public void Capture_SingleKey_Via_Enter()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("j", committed);
    }


    [Fact]
    public void Capture_Backspace_Deletes_Last_Staged_Key()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        core.HandleKeyDown(Key.K, anyModifierHeld: false);
        Assert.Equal(2, core.StagedKeys.Count);
        core.HandleKeyDown(Key.Back, anyModifierHeld: false); // 删除刚输入的 K
        Assert.Single(core.StagedKeys);
        Assert.Equal("j", core.StagedKeys[0]);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("j", committed);
    }

    [Fact]
    public void Capture_Backspace_With_Caret_Moved_Deletes_Before_Caret()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        core.HandleKeyDown(Key.K, anyModifierHeld: false); // staged [j,k], caret=2
        core.HandleKeyDown(Key.Left, anyModifierHeld: false);  // caret=1
        core.HandleKeyDown(Key.Back, anyModifierHeld: false);  // 删除光标前的 j
        Assert.Equal(["k"], core.StagedKeys);
        Assert.Equal(0, core.Caret);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("k", committed);
    }

    [Fact]
    public void Capture_Arrows_Move_Caret_And_Never_Staged()
    {
        var core = new HotkeyCaptureCore();
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        core.HandleKeyDown(Key.K, anyModifierHeld: false);
        Assert.Equal(2, core.Caret);
        core.HandleKeyDown(Key.Left, anyModifierHeld: false);
        Assert.Equal(1, core.Caret);
        core.HandleKeyDown(Key.Right, anyModifierHeld: false);
        Assert.Equal(2, core.Caret);
        core.HandleKeyDown(Key.Up, anyModifierHeld: false);
        core.HandleKeyDown(Key.Down, anyModifierHeld: false);
        Assert.Equal(2, core.Caret);
        // 方向键/Backspace 永不出现在暂存键里
        Assert.Equal(["j", "k"], core.StagedKeys);
    }

    [Fact]
    public void Capture_SidePrefix_SingleModifier_Commits_Physical_Side()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.RightCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("RCtrl", committed); // 区分左右: 右 Ctrl 提交 RCtrl
    }

    [Fact]
    public void Capture_Staged_Esc_Cancels_And_Empty_Enter_Exits()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        core.HandleKeyDown(Key.Escape, anyModifierHeld: false); // Esc 取消
        Assert.False(core.Capturing);
        Assert.Null(committed);

        core.StartCapture();
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false); // 空暂存 Enter = 退出
        Assert.False(core.Capturing);
        Assert.Null(committed);
    }


    [Fact]
    public void Capture_ModifierPlusKey_No_Auto_Commit()
    {
        // 2026-09 定稿: 无快路径 —— 修饰键+主键同样只暂存, Enter 才提交
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.P, anyModifierHeld: true);
        Assert.Null(committed);
        Assert.True(core.Capturing);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("<^p", committed);
    }


    [Fact]
    public void Capture_UnsupportedKey_KeepsWaiting()
    {
        // 反引号 (OemTilde) 不支持: AHK Hotkey("``") 报 Invalid hotkey (复刻 key==null 忽略)
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.OemTilde, anyModifierHeld: false);
        Assert.Null(committed);
        Assert.True(core.Capturing);
    }

    // ------------------------------------------------------------- 已占用热键收集

    [Fact]
    public void CollectUsedHotkeys_EnabledKeymaps_NormalizedAndSentinelExcluded()
    {
        var keymaps = new List<Keymap>
        {
            new()
            {
                Enable = true,
                Hotkey = "<^F9",
                Hotkeys = { ["^+q"] = [], ["~!a"] = [] },
            },
            new()
            {
                Enable = true,
                Hotkey = "settings", // 哨兵排除
            },
            new()
            {
                Enable = false, // 禁用的 keymap 不参与
                Hotkey = "<^F10",
            },
        };

        var used = HotkeyLogic.CollectUsedHotkeys(keymaps);
        Assert.Contains("^f9", used);
        Assert.Contains("^+q", used);
        Assert.Contains("!a", used);
        Assert.DoesNotContain("settings", used);
        Assert.DoesNotContain("^f10", used);
    }
}
