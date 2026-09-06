using Avalonia;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// WindowMatchFormatter 纯函数格式化 + EvaluateWinTitleError 校验放宽 单测。
/// 覆盖方案 §八(T9) 所列用例: 各 WindowMatchKind 输出 + 组合串不误报 + 裸 .exe 仍报错。
/// 范式照 HotkeyLogicTests: [Fact]/[Theory]+[InlineData], 无 UI 依赖。
/// </summary>
public sealed class WindowMatchFormatterTests
{
    // ------------------------------------------------------------- 辅助: 构造描述符

    private static readonly PixelRect DummyBounds = new(0, 0, 100, 100);

    private static WindowDescriptor MakeDescriptor(
        IntPtr? hwnd = null,
        string title = "记事本",
        string className = "Notepad",
        int pid = 1234,
        string exeName = "notepad.exe",
        string fullPath = @"C:\Windows\System32\notepad.exe")
        => new(
            hwnd ?? new IntPtr(654321),
            title,
            className,
            pid,
            exeName,
            fullPath,
            DummyBounds);

    // ------------------------------------------------------------- Format: TitleAndExe (默认/组合)

    [Fact]
    public void Format_TitleAndExe_WithTitle_ProducesCombinedIdentifier()
    {
        var d = MakeDescriptor(title: "记事本", exeName: "notepad.exe");
        Assert.Equal("记事本 ahk_exe notepad.exe", WindowMatchFormatter.Format(d, WindowMatchKind.TitleAndExe));
    }

    [Fact]
    public void Format_TitleAndExe_EmptyTitle_FallsBackToExeOnly()
    {
        var d = MakeDescriptor(title: "", exeName: "notepad.exe");
        Assert.Equal("ahk_exe notepad.exe", WindowMatchFormatter.Format(d, WindowMatchKind.TitleAndExe));
    }

    [Fact]
    public void Format_TitleAndExe_WhitespaceTitle_FallsBackToExeOnly()
    {
        var d = MakeDescriptor(title: "   \t  ", exeName: "notepad.exe");
        Assert.Equal("ahk_exe notepad.exe", WindowMatchFormatter.Format(d, WindowMatchKind.TitleAndExe));
    }

    [Fact]
    public void Format_TitleAndExe_TitleWithLeadingTrailingSpaces_IsTrimmed()
    {
        var d = MakeDescriptor(title: "  记事本  ", exeName: "notepad.exe");
        Assert.Equal("记事本 ahk_exe notepad.exe", WindowMatchFormatter.Format(d, WindowMatchKind.TitleAndExe));
    }

    // ------------------------------------------------------------- Format: 各 WindowMatchKind

    [Fact]
    public void Format_Exe_ProducesAhkExe()
    {
        var d = MakeDescriptor(exeName: "notepad.exe");
        Assert.Equal("ahk_exe notepad.exe", WindowMatchFormatter.Format(d, WindowMatchKind.Exe));
    }

    [Fact]
    public void Format_FullPath_ProducesAhkExeWithFullPath()
    {
        var d = MakeDescriptor(fullPath: @"C:\Windows\System32\notepad.exe");
        Assert.Equal(@"ahk_exe C:\Windows\System32\notepad.exe", WindowMatchFormatter.Format(d, WindowMatchKind.FullPath));
    }

    [Fact]
    public void Format_Class_ProducesAhkClass()
    {
        var d = MakeDescriptor(className: "Notepad");
        Assert.Equal("ahk_class Notepad", WindowMatchFormatter.Format(d, WindowMatchKind.Class));
    }

    [Fact]
    public void Format_Title_ProducesRawTitle()
    {
        var d = MakeDescriptor(title: "无标题 - 记事本");
        Assert.Equal("无标题 - 记事本", WindowMatchFormatter.Format(d, WindowMatchKind.Title));
    }

    [Fact]
    public void Format_Pid_ProducesAhkPid()
    {
        var d = MakeDescriptor(pid: 1234);
        Assert.Equal("ahk_pid 1234", WindowMatchFormatter.Format(d, WindowMatchKind.Pid));
    }

    [Fact]
    public void Format_HwndId_ProducesAhkIdDecimal()
    {
        var d = MakeDescriptor(hwnd: new IntPtr(654321));
        Assert.Equal("ahk_id 654321", WindowMatchFormatter.Format(d, WindowMatchKind.HwndId));
    }

    // ------------------------------------------------------------- EvaluateWinTitleError: 组合串放行

    [Fact]
    public void EvaluateWinTitleError_CombinedIdentifier_ReturnsNull()
    {
        // 放宽后: 含 " ahk_" 子串的组合串不应误报
        Assert.Null(ActivateOrRunEditorVm.EvaluateWinTitleError("记事本 ahk_exe notepad.exe"));
    }

    // ------------------------------------------------------------- EvaluateWinTitleError: 裸 .exe 仍报错

    [Fact]
    public void EvaluateWinTitleError_BareExe_ReturnsNonNull()
    {
        Assert.NotNull(ActivateOrRunEditorVm.EvaluateWinTitleError("notepad.exe"));
    }

    [Fact]
    public void EvaluateWinTitleError_BareExeUpperCase_ReturnsNonNull()
    {
        // EndsWith(".exe", OrdinalIgnoreCase) → 大写也报错
        Assert.NotNull(ActivateOrRunEditorVm.EvaluateWinTitleError("NOTEPAD.EXE"));
    }

    // ------------------------------------------------------------- EvaluateWinTitleError: 各放行分支

    [Theory]
    [InlineData("ahk_exe notepad.exe")]
    [InlineData("ahk_class Notepad")]
    [InlineData("ahk-expression: WinExist(\"记事本\")")]
    [InlineData("")]
    [InlineData(null)]
    [InlineData("无标题 - 记事本")]
    public void EvaluateWinTitleError_ValidInputs_ReturnNull(string? input)
    {
        Assert.Null(ActivateOrRunEditorVm.EvaluateWinTitleError(input));
    }

    // ------------------------------------------------------------- EvaluateWinTitleError: ahk_ 前缀放行

    [Theory]
    [InlineData("ahk_exe notepad.exe")]
    [InlineData("ahk_class Notepad")]
    [InlineData("ahk_pid 1234")]
    [InlineData("ahk_id 654321")]
    public void EvaluateWinTitleError_StartsWithAhk_ReturnsNull(string input)
    {
        Assert.Null(ActivateOrRunEditorVm.EvaluateWinTitleError(input));
    }

    // ------------------------------------------------------------- EvaluateWinTitleError: 含 " ahk_" 组合串放行

    [Theory]
    [InlineData("记事本 ahk_exe notepad.exe")]
    [InlineData("My Window ahk_class Chrome_WidgetWin_1")]
    [InlineData("Some Title ahk_pid 9999")]
    public void EvaluateWinTitleError_ContainsSpaceAhk_ReturnsNull(string input)
    {
        Assert.Null(ActivateOrRunEditorVm.EvaluateWinTitleError(input));
    }
}
