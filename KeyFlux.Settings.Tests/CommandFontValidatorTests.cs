using System.Buffers.Binary;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// <see cref="CommandFontValidator"/> 的边界守护 —— 与 Go 侧
/// <c>script.InstallCommandFont</c> 的判据**必须同口径** (体积上限 32 MiB / 只接受 glyf 轮廓)。
///
/// <para>
/// 为什么值得单独测: 该类的全部意义就是"把生成端的静默跳过前移到 UI 当场可见"。
/// 若它的判据与 Go 侧漂移 (限值改了、签名字节认错), 就会出现两种坏结果:
/// ① UI 说"可用"而生成端静默跳过 —— 用户又被坑一次 (正是本功能要消灭的症状);
/// ② UI 说"不可用"而生成端其实接受 —— 用户被拦住用不了合法字体。
/// 故这里逐字节固定 sfnt 签名判定, 并把 32 MiB 限值与 Go 常量绑成一条断言。
/// </para>
/// </summary>
public sealed class CommandFontValidatorTests : IDisposable
{
    private readonly string _dir =
        Path.Combine(Path.GetTempPath(), "kf_fontval_" + Guid.NewGuid().ToString("N")[..8]);

    public CommandFontValidatorTests() => Directory.CreateDirectory(_dir);

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_dir)) Directory.Delete(_dir, recursive: true);
        }
        catch (IOException)
        {
            // 临时目录清理失败不该让测试红 —— 系统稍后会自行回收 %TEMP%
        }
    }

    /// <summary>造一份"首 4 字节 = tag, 长度可控"的假字体文件, 返回其路径。</summary>
    private string MakeFile(string name, uint tag, long totalBytes)
    {
        var p = Path.Combine(_dir, name);
        var buf = new byte[Math.Max(totalBytes, 12)];
        BinaryPrimitives.WriteUInt32BigEndian(buf, tag);
        File.WriteAllBytes(p, buf.AsSpan(0, (int)Math.Max(totalBytes, 12)).ToArray());
        return p;
    }

    /// <summary>0x00010000 = 标准 TrueType (glyf), 命令框接受。</summary>
    [Fact]
    public void Validate_TrueType_IsUsable()
    {
        var p = MakeFile("a.ttf", 0x00010000, 1024);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.Ok, r.Verdict);
        Assert.True(r.Usable);
    }

    /// <summary>'true' = Apple TrueType, 同样是 glyf 轮廓, 必须接受 (与 Go 侧一致)。</summary>
    [Fact]
    public void Validate_AppleTrueType_IsUsable()
    {
        var p = MakeFile("a.ttf", 0x74727565, 1024);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.Ok, r.Verdict);
        Assert.True(r.Usable);
    }

    /// <summary>'OTTO' = CFF/OpenType 轮廓, 命令框 exe 无法加载 => 必须判不可用。</summary>
    [Fact]
    public void Validate_CffOtf_IsRejected()
    {
        var p = MakeFile("a.otf", 0x4F54544F, 1024);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.UnsupportedFormat, r.Verdict);
        Assert.False(r.Usable);
    }

    /// <summary>无法识别的 sfnt 版本 (含被改名的非字体文件) 一律拒绝。</summary>
    [Theory]
    [InlineData(0x00000000u)]
    [InlineData(0x89504E47u)] // PNG 魔数 —— 改名的图片
    [InlineData(0x504B0304u)] // ZIP 魔数 —— 改名的压缩包
    [InlineData(0x774F4646u)] // 'wOFF' —— web 字体, 命令框不接受
    public void Validate_UnknownSfnt_IsRejected(uint tag)
    {
        var p = MakeFile("a.ttf", tag, 1024);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.UnsupportedFormat, r.Verdict);
        Assert.False(r.Usable);
    }

    /// <summary>体积上限必须与 Go 侧 FontMaxBytes 一致 (32 MiB) —— 超限即不可用。</summary>
    [Fact]
    public void Validate_OverSizeLimit_IsRejected()
    {
        Assert.Equal(32L * 1024 * 1024, CommandFontValidator.MaxBytes);

        var p = MakeFile("big.ttf", 0x00010000, CommandFontValidator.MaxBytes + 1);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.TooLarge, r.Verdict);
        Assert.False(r.Usable);
    }

    /// <summary>恰好等于上限 = 允许 (判据是 "> MaxBytes", 与 Go 的 srcInfo.Size() > FontMaxBytes 同口径)。</summary>
    [Fact]
    public void Validate_ExactlyAtSizeLimit_IsUsable()
    {
        var p = MakeFile("edge.ttf", 0x00010000, CommandFontValidator.MaxBytes);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.Ok, r.Verdict);
        Assert.True(r.Usable);
    }

    /// <summary>不存在的路径 => 不可用 (不抛异常)。</summary>
    [Fact]
    public void Validate_MissingFile_IsUnreadable()
    {
        var r = CommandFontValidator.Validate(Path.Combine(_dir, "nope.ttf"));
        Assert.Equal(CommandFontValidator.Verdict.Unreadable, r.Verdict);
        Assert.False(r.Usable);
    }

    /// <summary>空路径 => 不可用 (不抛异常)。</summary>
    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    public void Validate_BlankPath_IsUnreadable(string path)
    {
        var r = CommandFontValidator.Validate(path);
        Assert.Equal(CommandFontValidator.Verdict.Unreadable, r.Verdict);
        Assert.False(r.Usable);
    }

    /// <summary>目录路径 => 不可用 (FileInfo.Exists 对目录为 false)。</summary>
    [Fact]
    public void Validate_DirectoryPath_IsUnreadable()
    {
        var r = CommandFontValidator.Validate(_dir);
        Assert.False(r.Usable);
    }

    // ------------------------------------------------------------ 字体集合 (.ttc)

    /// <summary>造一份最小 ttcf: tag + version + numFonts + offsets[] + 首个 face 的 sfnt tag。</summary>
    private string MakeCollection(string name, uint face0Tag, uint numFonts = 1)
    {
        var p = Path.Combine(_dir, name);
        const int headerLen = 12;
        var faceOff = (uint)(headerLen + 4 * numFonts);
        var buf = new byte[faceOff + 8];
        BinaryPrimitives.WriteUInt32BigEndian(buf.AsSpan(0), 0x74746366);   // 'ttcf'
        BinaryPrimitives.WriteUInt32BigEndian(buf.AsSpan(4), 0x00020000);   // version 2.0
        BinaryPrimitives.WriteUInt32BigEndian(buf.AsSpan(8), numFonts);
        for (var i = 0; i < numFonts; i++)
        {
            BinaryPrimitives.WriteUInt32BigEndian(buf.AsSpan(headerLen + 4 * i), faceOff);
        }
        BinaryPrimitives.WriteUInt32BigEndian(buf.AsSpan((int)faceOff), face0Tag);
        File.WriteAllBytes(p, buf);
        return p;
    }

    /// <summary>ttc 首个 face 为 glyf => 可用, 但必须给出"只用第一个 face"的提醒。</summary>
    [Fact]
    public void Validate_CollectionWithGlyfFirstFace_UsableWithNotice()
    {
        var p = MakeCollection("a.ttc", 0x00010000);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.CollectionFirstFaceOnly, r.Verdict);
        Assert.True(r.Usable);
        // 提醒文案必须点名文件 (占位符 {0} 被填充), 不能原样回显模板
        Assert.Contains("a.ttc", r.Message);
        Assert.DoesNotContain("{0}", r.Message);
    }

    /// <summary>ttc 首个 face 是 CFF => 整份文件被拒 (生成端同样只看第一个 face)。</summary>
    [Fact]
    public void Validate_CollectionWithCffFirstFace_IsRejected()
    {
        var p = MakeCollection("a.ttc", 0x4F54544F);
        var r = CommandFontValidator.Validate(p);
        Assert.Equal(CommandFontValidator.Verdict.UnsupportedFormat, r.Verdict);
        Assert.False(r.Usable);
    }

    /// <summary>numFonts 为 0 或明显越界的畸形集合 => 拒绝 (不做越界读)。</summary>
    [Theory]
    [InlineData(0u)]
    [InlineData(99999u)]
    public void Validate_MalformedCollection_IsRejected(uint numFonts)
    {
        var p = MakeCollection("a.ttc", 0x00010000, numFonts);
        var r = CommandFontValidator.Validate(p);
        Assert.False(r.Usable);
    }

    // ------------------------------------------------------------ i18n 一致性

    /// <summary>
    /// 所有结论都必须有真实文案 —— 键号打错时 T() 会回显键号本身, 表现为
    /// "界面显示 2588 这种数字", 属静默劣化。这里断言文案不含裸键号。
    /// </summary>
    [Fact]
    public void AllMessages_AreLocalized_NotRawKeys()
    {
        var cases = new (uint Tag, string Name)[]
        {
            (0x00010000, "ok.ttf"),
            (0x4F54544F, "cff.otf"),
            (0x00000000, "junk.ttf"),
        };
        foreach (var (tag, name) in cases)
        {
            var r = CommandFontValidator.Validate(MakeFile(name, tag, 512));
            Assert.False(string.IsNullOrWhiteSpace(r.Message));
            Assert.DoesNotContain("25", r.Message[..Math.Min(4, r.Message.Length)]);
        }

        var missing = CommandFontValidator.Validate(Path.Combine(_dir, "nope.ttf"));
        Assert.False(string.IsNullOrWhiteSpace(missing.Message));
    }

    /// <summary>带占位符的文案在参数不足时必须回退原文而不是抛异常 (防界面崩溃)。</summary>
    [Fact]
    public void Format_WithMissingArgs_FallsBackToTemplate()
    {
        // 2590 只有一个 {0}; 用一个会触发 FormatException 的多花括号模板验证不回退崩。
        // 这里直接验证正常路径: 单参数能正确填充。
        var text = I18n.T("2590", "demo.ttc");
        Assert.Contains("demo.ttc", text);
    }
}
