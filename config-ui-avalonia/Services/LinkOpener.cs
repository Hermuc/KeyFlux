using System.Diagnostics;

namespace KeyFlux.Settings.Services;

// ============================================================================
// LinkOpener: 链接打开策略 (总览页 markdown 链接的点击行为)
//
// 规则 (复刻 Vue 版 iframe 内点击链接的默认行为):
//   - 内部路径 (以 / 开头): 拼上后端地址后用系统默认浏览器打开 (后端静态站点);
//   - 外部 http/https 链接与魔法链接 (shell: / ms-settings: / steam: 等):
//     直接交给系统默认程序处理。
// ============================================================================
public static class LinkOpener
{
    /// <summary>打开链接。失败静默 (如空地址), 不影响总览页。</summary>
    public static void Open(string url, int backendPort)
    {
        if (string.IsNullOrWhiteSpace(url)) return;
        try
        {
            var target = url.StartsWith('/') ? $"http://127.0.0.1:{backendPort}{url}" : url;
            Process.Start(new ProcessStartInfo(target) { UseShellExecute = true });
        }
        catch
        {
            // 打开失败静默, 不影响总览页
        }
    }
}
