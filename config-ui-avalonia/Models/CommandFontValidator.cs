using System;
using System.Buffers.Binary;
using System.IO;

using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Models;

/// <summary>
/// 命令框字体源文件的**客户端预检** —— 在用户选定字体后立刻给出结论, 避免"选了没反应"。
///
/// <para>
/// 为什么需要它 (真实报障): 生成端 <c>InstallCommandFont</c> 会因体积超限/轮廓格式不受支持
/// 而**静默跳过** (字体是纯表现层资源, 不阻断脚本生成)。用户选中一个 83 MB 的 <c>.ttc</c>
/// (或 <c>.otf</c>) 后界面毫无反馈, 命令框字体也没变 —— 表现为"选了跟没选一样", 无从
/// 得知原因。本类把同一套判据**前移到 UI 选择时刻**, 让问题当场可见。
/// </para>
/// <para>
/// 🔴 判据必须与 Go 侧保持一致 (单一事实来源: <c>config-server/internal/script/font.go</c>):
/// <list type="bullet">
///   <item><c>FontMaxBytes</c> = 32 MiB —— 超限即拒绝。</item>
///   <item>轮廓格式: 只接受 sfnt 版本 <c>0x00010000</c> / <c>true</c> (glyf); <c>OTTO</c> (CFF)
///   一律拒绝; <c>ttcf</c> (字体集合) 需看首个 face 的轮廓格式。</item>
/// </list>
/// 本类是**纯读**判定, 不复制/不修改任何文件 —— 与生成端"拒绝后沿用现有字体"的行为一致。
/// </para>
/// <para>
/// ⚠ 本类只做**能当场断言**的检查 (体积/轮廓/首 face)。真正的落地仍由生成端执行, 故
/// 判定为"可用"不等于已生效 —— 仍需重启命令框进程 (契约 §3.11.1 硬约束 7)。
/// </para>
/// </summary>
public static class CommandFontValidator
{
    /// <summary>字体文件体积上限, 与 Go 侧 <c>script.FontMaxBytes</c> 必须一致 (32 MiB)。</summary>
    public const long MaxBytes = 32 * 1024 * 1024;

    /// <summary>sfnt 版本标签: TrueType 轮廓 (glyf)。</summary>
    private const uint SfntTrueType = 0x00010000;

    /// <summary>sfnt 版本标签: 'true' (Apple TrueType, 同样 glyf)。</summary>
    private const uint SfntAppleTrueType = 0x74727565;

    /// <summary>sfnt 版本标签: 'OTTO' (CFF 轮廓, 命令框不接受)。</summary>
    private const uint SfntCff = 0x4F54544F;

    /// <summary>sfnt 版本标签: 'ttcf' (字体集合, 含多 face)。</summary>
    private const uint SfntCollection = 0x74746366;

    /// <summary>文本长度上限 (字形数由 maxp 判定, 阈值宽松即可)。</summary>
    private const int FaceCountProbeLimit = 4096;

    /// <summary>校验结论。</summary>
    public enum Verdict
    {
        /// <summary>可用: 体积与轮廓格式都满足生成端要求。</summary>
        Ok,

        /// <summary>文件不存在或不可读 (已删除 / 权限 / 路径无效)。</summary>
        Unreadable,

        /// <summary>超过 32 MiB 上限。</summary>
        TooLarge,

        /// <summary>轮廓格式不受支持 (CFF/OTTO, 或无法识别的 sfnt 版本)。</summary>
        UnsupportedFormat,

        /// <summary>是字体集合 (.ttc), 但**只有第一个 face 会被命令框使用**。</summary>
        CollectionFirstFaceOnly,
    }

    /// <summary>
    /// 校验结果: 结论 + 供 UI 展示的说明 (已 i18n) + 是否可用。
    /// </summary>
    /// <param name="Verdict">结构化结论, 供调用方按需分支 (如只对警告态做高亮)。</param>
    /// <param name="Message">已本地化的说明文本, 可直接绑定到 UI。</param>
    /// <param name="Usable">是否可被生成端采纳 (Ok / CollectionFirstFaceOnly 为 true)。</param>
    public readonly record struct Result(Verdict Verdict, string Message, bool Usable);

    /// <summary>
    /// 校验一份候选字体文件。返回的 <see cref="Result.Message"/> 已本地化, 可直接绑定到 UI。
    /// </summary>
    /// <param name="path">用户选定的字体文件绝对路径。</param>
    public static Result Validate(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return new Result(Verdict.Unreadable, I18n.T("2592"), false);
        }

        FileInfo fi;
        try
        {
            fi = new FileInfo(path);
            if (!fi.Exists)
            {
                return new Result(Verdict.Unreadable, I18n.T("2592"), false);
            }
            if (fi.Length > MaxBytes)
            {
                // 体积先判: 与 Go 侧顺序一致 (先 Stat.Size, 再读头部嗅探) ——
                // 避免为一个 83 MB 的文件白读头部。
                return new Result(Verdict.TooLarge, I18n.T("2588"), false);
            }
        }
        catch (Exception)
        {
            // UnauthorizedAccessException / PathTooLongException 等一律按"读不了"处理
            return new Result(Verdict.Unreadable, I18n.T("2592"), false);
        }

        uint tag;
        try
        {
            using var fs = File.OpenRead(path);
            Span<byte> head = stackalloc byte[4];
            if (fs.Read(head) != 4)
            {
                return new Result(Verdict.Unreadable, I18n.T("2592"), false);
            }
            tag = BinaryPrimitives.ReadUInt32BigEndian(head);
        }
        catch (Exception)
        {
            return new Result(Verdict.Unreadable, I18n.T("2592"), false);
        }

        return tag switch
        {
            SfntTrueType or SfntAppleTrueType => new Result(Verdict.Ok, I18n.T("2591"), true),
            SfntCff => new Result(Verdict.UnsupportedFormat, I18n.T("2589"), false),
            SfntCollection => ValidateCollection(path),
            _ => new Result(Verdict.UnsupportedFormat, I18n.T("2589"), false),
        };
    }

    /// <summary>
    /// 字体集合 (.ttc): 读取首个 face 的 sfnt 版本判定轮廓格式, 并提示"只有第一个 face 生效"。
    ///
    /// <para>
    /// ttc 头部格式: <c>tag(4) version(4) numFonts(4) offsets[numFonts](4 各)</c>,
    /// 每个 offset 指向一个完整 sfnt 表目录, 其首 4 字节即该 face 的 sfnt 版本。
    /// 仅读首个 offset —— 与命令框行为一致 (它同样只用第一个 face)。
    /// </para>
    /// </summary>
    private static Result ValidateCollection(string path)
    {
        try
        {
            using var fs = File.OpenRead(path);
            Span<byte> hdr = stackalloc byte[12];
            if (fs.Read(hdr) != 12)
            {
                return new Result(Verdict.Unreadable, I18n.T("2592"), false);
            }

            uint numFonts = BinaryPrimitives.ReadUInt32BigEndian(hdr[8..12]);
            if (numFonts == 0 || numFonts > FaceCountProbeLimit)
            {
                return new Result(Verdict.UnsupportedFormat, I18n.T("2589"), false);
            }

            Span<byte> first = stackalloc byte[4];
            if (fs.Read(first) != 4)
            {
                return new Result(Verdict.Unreadable, I18n.T("2592"), false);
            }

            long off = BinaryPrimitives.ReadUInt32BigEndian(first);
            // 判据用 `>` 而非 `>=`: 恰好剩 4 字节时读取合法。原写 `>=` 比必要值严 1 字节,
            // 会把这类文件误判为"格式不支持" —— 而 Go 侧同口径判据接受它 (2026-09-21 统一)。
            if (off <= 0 || off > fs.Length - 4)
            {
                return new Result(Verdict.UnsupportedFormat, I18n.T("2589"), false);
            }

            fs.Seek(off, SeekOrigin.Begin);
            Span<byte> faceTag = stackalloc byte[4];
            if (fs.Read(faceTag) != 4)
            {
                return new Result(Verdict.Unreadable, I18n.T("2592"), false);
            }

            uint faceSfnt = BinaryPrimitives.ReadUInt32BigEndian(faceTag);
            if (faceSfnt is not (SfntTrueType or SfntAppleTrueType))
            {
                // 首个 face 就是 CFF -> 生成端会拒绝整份文件
                return new Result(Verdict.UnsupportedFormat, I18n.T("2589"), false);
            }

            // 可用, 但要提醒"只有第一个 face 生效" —— 这是用户最容易误解的点
            // (选了中文版字体, 实际落地却是集合里的拉丁版 face)。
            string name = Path.GetFileName(path);
            return new Result(Verdict.CollectionFirstFaceOnly,
                              I18n.T("2590", name), true);
        }
        catch (Exception)
        {
            return new Result(Verdict.Unreadable, I18n.T("2592"), false);
        }
    }
}
