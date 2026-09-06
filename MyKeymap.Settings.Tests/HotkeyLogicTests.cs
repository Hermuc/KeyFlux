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

    /// <summary>8 个左右修饰前缀 × 主键抽样 (字母/功能键): 捕获提交统一侧别归一化,
    /// 左右修饰键通配两侧 (复刻诉求: 录制 "RCtrl+P" 后左 Ctrl+P 也能触发)。</summary>
    [Theory]
    [InlineData(Key.LeftCtrl, Key.A, "^a")]
    [InlineData(Key.RightCtrl, Key.A, "^a")]
    [InlineData(Key.LeftAlt, Key.F5, "!F5")]
    [InlineData(Key.RightAlt, Key.F5, "!F5")]
    [InlineData(Key.LeftShift, Key.D7, "+7")]
    [InlineData(Key.RightShift, Key.D7, "+7")]
    [InlineData(Key.LWin, Key.Q, "#q")]
    [InlineData(Key.RWin, Key.Q, "#q")]
    public void Capture_LeftRightModifiers_AreNormalizedToSideAgnosticHotkeys(Key modifier, Key main, string expected)
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(modifier, anyModifierHeld: true);
        core.HandleKeyDown(main, anyModifierHeld: true);
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
        Assert.Equal("^+k", committed); // 输出通配两侧 (^+k), 归一化保留 MOD_ORDER 归并后的顺序
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
    public void Capture_EscWithModifierHeld_DoesNotCancel_CommitsInstead()
    {
        // 复刻: Esc 仅在无任何修饰键时取消; Ctrl+Esc 是合法热键 ^esc
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.Escape, anyModifierHeld: true);
        Assert.Equal("^esc", committed);
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
    public void Capture_SingleModifier_Committed_On_Release()
    {
        // 2026-09: 单按 Ctrl (捕获期间唯一按键) 松开即提交中性键名
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyUp(Key.LeftCtrl);
        Assert.Equal("Ctrl", committed);
        Assert.False(core.Capturing);
    }

    [Fact]
    public void Capture_TwoModifiers_Released_Does_Not_Commit_Single()
    {
        // Ctrl+Shift+X 意图保护: 按过两个修饰键再全部松开, 不误提交单修饰键
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.LeftShift, anyModifierHeld: true);
        core.HandleKeyUp(Key.LeftCtrl);
        Assert.Null(committed);
        core.HandleKeyUp(Key.LeftShift);
        Assert.Null(committed);
        Assert.True(core.Capturing); // 仍在捕获, 可继续按 X
        core.HandleKeyDown(Key.X, anyModifierHeld: false);
        core.HandleKeyDown(Key.Enter, anyModifierHeld: false);
        Assert.Equal("x", committed); // Enter 确认单键
    }

    [Fact]
    public void Capture_Staged_SecondKey_Commits_CustomCombo()
    {
        // J+K: 首键暂存, 第二键提交 AHK 自定义组合
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.J, anyModifierHeld: false);
        Assert.Null(committed); // 首键等待
        Assert.Single(core.StagedKeys);
        core.HandleKeyUp(Key.J); // 松开不丢暂存
        Assert.Single(core.StagedKeys);
        core.HandleKeyDown(Key.K, anyModifierHeld: false);
        Assert.Equal("j & k", committed);
    }

    [Fact]
    public void Capture_Staged_Enter_Commits_Single_Key()
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
    public void Capture_FastPath_ModifierPlusKey_Unchanged()
    {
        var core = new HotkeyCaptureCore();
        string? committed = null;
        core.HotkeyCommitted += ahk => committed = ahk;
        core.StartCapture();
        core.HandleKeyDown(Key.LeftCtrl, anyModifierHeld: true);
        core.HandleKeyDown(Key.P, anyModifierHeld: true);
        Assert.Equal("^p", committed); // 快路径不变
        core.HandleKeyUp(Key.LeftCtrl); // 提交后松开修饰键无副作用
        Assert.Equal("^p", committed);
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
