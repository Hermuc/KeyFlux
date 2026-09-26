using System.Text;

namespace KeyFlux.Settings.Services;

// ============================================================================
// 拾取器诊断日志 (M1): 「准星单击不回填」修复包的可观测性底座。
//
// 背景: 拾取链路横跨 UI 线程 (WindowPickButton) 与专用 pump 线程 (PickSession,
// 钩子回调 + 节流探测), 失败模式多为静默 (Cancelled / 投递丢失 / 探测降级),
// Debug.WriteLine 在 Release 用户机上不可见。本日志提供一次会话的完整事件链证据:
// 钩子安装结果 / 提交投递返回值 / 探测结果 / 兑现状态 / 控件写回。
//
// 约束:
//   - Release 可见 (刻意不加 [Conditional("DEBUG")]);
//   - 全部 IO 包 try/catch 静默失败 —— 日志绝不影响拾取功能本身;
//   - lock 保证 pump 线程与 UI 线程并发安全;
//   - BeginSession 截断重写 (每次 PickAsync 一轮), Log 追加单行, 控制体量;
//   - 文件含窗口标题, 属本机诊断文件。
// ============================================================================

/// <summary>拾取器诊断日志 (静态, 无配置, 写 <c>AppContext.BaseDirectory\picker-debug.log</c>)。</summary>
public static class PickLog
{
    private static readonly object Gate = new();

    private static string FilePath => Path.Combine(AppContext.BaseDirectory, "picker-debug.log");

    /// <summary>会话开始: 截断重写 (每次 PickAsync 一轮, 避免文件无限增长)。</summary>
    public static void BeginSession(string header)
    {
        try
        {
            lock (Gate)
            {
                File.WriteAllText(
                    FilePath,
                    $"{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff} === {header} ==={Environment.NewLine}",
                    new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
            }
        }
        catch
        {
            // 诊断日志绝不影响功能
        }
    }

    /// <summary>追加一行 (时间戳 + 文本)。调用方自行保证单行摘要、避免过长。</summary>
    public static void Log(string message)
    {
        try
        {
            lock (Gate)
            {
                File.AppendAllText(
                    FilePath,
                    $"{DateTime.Now:HH:mm:ss.fff} {message}{Environment.NewLine}",
                    new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
            }
        }
        catch
        {
            // 诊断日志绝不影响功能
        }
    }
}
