using System.Diagnostics;
using System.Net.Http;
using System.Text;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.Tests.Infrastructure;

/// <summary>
/// 契约测试专用「模拟部署目录 + headless settings.exe 子进程」。
///
/// 目录布局 (与真实部署一致, settings.exe 以 bin\ 为工作目录运行):
///   &lt;root&gt;\bin\settings.exe          从 %TEMP%\mk_settings_headless\ 复制的 --headless 测试二进制
///   &lt;root&gt;\bin\templates\            从仓库 config-server/templates 复制
///   &lt;root&gt;\bin\site\                 最小站点目录 (仅占位)
///   &lt;root&gt;\data\config.json          仓库 data\config.json 的独立副本 (每个测试一份)
///   &lt;root&gt;\shortcuts\*.lnk           若干占位快捷方式
///   注意: 刻意不放 KeyFlux.exe —— execCmd 会静默失败, 保证测试无副作用。
///
/// 每个测试方法独占一个实例 (xUnit 每用例新建测试类实例), 目录相互独立,
/// PUT 类测试对 config.json 的修改不会污染其他用例。
/// </summary>
public sealed class SettingsTestServer : IAsyncLifetime
{
    private static readonly string HeadlessExeSource =
        Path.Combine(Path.GetTempPath(), "mk_settings_headless", "settings.exe");

    /// <summary>
    /// 仓库根: 从测试输出目录逐级向上找含 config-ui-avalonia 的目录
    /// (与 I18nResourceTests.RepoRoot 同款; 硬编码盘符路径会在 CI 检出目录下全线失败)。
    /// </summary>
    private static readonly string RepoRoot = FindRepoRoot();

    private static string FindRepoRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, "config-ui-avalonia"))) return dir.FullName;
        }
        throw new InvalidOperationException(
            "找不到仓库根 (含 config-ui-avalonia 的目录), BaseDirectory=" + AppContext.BaseDirectory);
    }

    private Process? _process;
    private Task? _stdoutDrain;
    private Task? _stderrDrain;
    private readonly TaskCompletionSource<int> _portTcs = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private readonly List<string> _stdoutLines = [];

    /// <summary>模拟部署根目录 (含 bin\、data\、shortcuts\)。</summary>
    public string RootDir { get; private set; } = "";

    /// <summary>settings.exe 的工作目录 = 模拟部署根\bin。</summary>
    public string BinDir => Path.Combine(RootDir, "bin");

    /// <summary>配置文件路径 (PUT 测试的字节级核验目标)。</summary>
    public string ConfigPath => Path.Combine(RootDir, "data", "config.json");

    /// <summary>settings.exe 通告的实际监听端口 (12333 被占时为随机端口)。</summary>
    public int Port { get; private set; }

    /// <summary>按通告端口构造好基址的 API 客户端。</summary>
    public SettingsApiClient Client { get; private set; } = null!;

    /// <summary>子进程全部 stdout (诊断用)。</summary>
    public IReadOnlyList<string> StdoutLines => _stdoutLines;

    public async Task InitializeAsync()
    {
        Assert.True(File.Exists(HeadlessExeSource),
            $"缺少 headless 测试二进制: {HeadlessExeSource} (前置任务产物)");

        // 前置二进制陈旧度守护 (Kim H2): 上面只查存在不查新旧, check-cs 不刷新
        // %TEMP%\mk_settings_headless\settings.exe, 可长期拿陈旧后端跑出假绿 (本轮实际
        // 发生过: 旧构建 DAA20E28… 被手工覆盖为新构建 5D406656… 才真绿)。
        // 以只读、零构建代价的方式断言 exe 不比 Go 源/模板旧; 不在测试内自动 go build。
        AssertBackendNotStale();

        RootDir = Path.Combine(Path.GetTempPath(), "mk_contract_tests", Guid.NewGuid().ToString("N"));
        var dataDir = Path.Combine(RootDir, "data");
        var shortcutsDir = Path.Combine(RootDir, "shortcuts");
        Directory.CreateDirectory(BinDir);
        Directory.CreateDirectory(Path.Combine(BinDir, "site"));
        Directory.CreateDirectory(dataDir);
        Directory.CreateDirectory(shortcutsDir);

        // headless 二进制
        File.Copy(HeadlessExeSource, Path.Combine(BinDir, "settings.exe"));

        // templates (GenerateAHK 等脚本生成依赖)
        CopyDirectory(Path.Combine(RepoRoot, "config-server", "templates"), Path.Combine(BinDir, "templates"));

        // 内置行为包 (CONTRACTS §3.9): 保存校验覆盖检查与脚本生成展开的运行时依赖
        CopyDirectory(Path.Combine(RepoRoot, "bin", "behaviors"), Path.Combine(BinDir, "behaviors"));

        // 最小站点占位 (NoRoute 静态服务指向 ./site)
        File.WriteAllText(Path.Combine(BinDir, "site", "index.html"), "<!doctype html><title>mk-test</title>\n");

        // 配置真源副本 (仓库 data\config.json)
        File.Copy(Path.Combine(RepoRoot, "data", "config.json"), ConfigPath);

        // 若干快捷方式 (空文件即可, GET /shortcuts 仅按文件名 glob)
        File.WriteAllBytes(Path.Combine(shortcutsDir, "alpha.lnk"), []);
        File.WriteAllBytes(Path.Combine(shortcutsDir, "beta.lnk"), []);
        File.WriteAllBytes(Path.Combine(shortcutsDir, "快捷方式.lnk"), []);

        // 禁令守护: 模拟目录绝不允许出现 KeyFlux.exe
        Assert.False(File.Exists(Path.Combine(RootDir, "KeyFlux.exe")), "模拟目录不得包含 KeyFlux.exe");

        var psi = new ProcessStartInfo
        {
            FileName = Path.Combine(BinDir, "settings.exe"),
            Arguments = "--headless",
            WorkingDirectory = BinDir, // 强依赖相对路径: ../data、./site、./templates
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8,
        };

        _process = Process.Start(psi) ?? throw new InvalidOperationException("无法启动 headless settings.exe");

        // 后台持续抽干两个管道, 避免缓冲区写满导致子进程阻塞; 同时从首行提取端口通告
        _stdoutDrain = Task.Run(DrainStdoutAsync);
        _stderrDrain = Task.Run(async () =>
        {
            while (await _process.StandardError.ReadLineAsync() is { } line)
            {
                // execCmd 找不到 KeyFlux.exe 的日志会到这里, 忽略即可
            }
        });

        int port;
        try
        {
            port = await _portTcs.Task.WaitAsync(TimeSpan.FromSeconds(20));
        }
        catch (TimeoutException)
        {
            throw new TimeoutException("等待 KEYFLUX_PORT= 通告行超时");
        }
        Port = port;
        Client = new SettingsApiClient(port);

        // 健康探测: 端口通告早于 router.RunListener 之后的极短窗口, 轮询至真正可服务
        await WaitForReadyAsync();
    }

    private async Task DrainStdoutAsync()
    {
        while (await _process!.StandardOutput.ReadLineAsync() is { } line)
        {
            lock (_stdoutLines) { _stdoutLines.Add(line); }
            // 端口通告行: headless 模式约定为 stdout 第一行 "KEYFLUX_PORT=<端口>"
            if (line.StartsWith("KEYFLUX_PORT=") && int.TryParse(line["KEYFLUX_PORT=".Length..], out var p))
            {
                _portTcs.TrySetResult(p);
            }
        }
        _portTcs.TrySetException(new InvalidOperationException(
            "settings.exe stdout 结束仍未收到 KEYFLUX_PORT= 通告行"));
    }

    private async Task WaitForReadyAsync()
    {
        using var probe = new HttpClient { Timeout = TimeSpan.FromSeconds(2) };
        var deadline = DateTime.UtcNow.AddSeconds(15);
        while (DateTime.UtcNow < deadline)
        {
            try
            {
                var resp = await probe.GetAsync($"http://localhost:{Port}/config");
                if (resp.IsSuccessStatusCode)
                {
                    return;
                }
            }
            catch (Exception)
            {
                // 尚未就绪, 继续轮询
            }
            await Task.Delay(100);
        }
        throw new TimeoutException($"settings.exe (port={Port}) 健康探测超时");
    }

    public async Task DisposeAsync()
    {
        Client?.Dispose();

        if (_process is not null)
        {
            try
            {
                // 整树终止, 防止孤儿进程占用模拟目录
                if (!_process.HasExited)
                {
                    _process.Kill(entireProcessTree: true);
                }
                await _process.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(5));
            }
            catch (Exception)
            {
                // 尽力而为
            }
            _process.Dispose();
        }

        if (_stdoutDrain is not null) { await Task.WhenAny(_stdoutDrain, Task.Delay(1000)); }
        if (_stderrDrain is not null) { await Task.WhenAny(_stderrDrain, Task.Delay(1000)); }

        // 清理模拟目录 (位于 %TEMP%, 可写); 文件锁残留时重试几次
        for (var i = 0; i < 3; i++)
        {
            try
            {
                Directory.Delete(RootDir, recursive: true);
                break;
            }
            catch (Exception) when (i < 2)
            {
                await Task.Delay(300);
            }
        }
    }

    /// <summary>
    /// 前置二进制陈旧度断言 (Kim H2, 只读、零构建代价): 取 config-server/**/*.go 与
    /// config-server/templates/** 的最大 LastWriteTime, 若 headless exe 的 mtime 早于它,
    /// 说明 %TEMP% 里的后端是旧构建, 契约测试会拿陈旧后端跑出假绿 —— 直接 Fail 并
    /// 打印刷新命令。刻意不在测试内 go build: 刷新是 make buildServer 的职责。
    /// </summary>
    private static void AssertBackendNotStale()
    {
        var exeMtime = File.GetLastWriteTime(HeadlessExeSource);
        var serverDir = Path.Combine(RepoRoot, "config-server");
        if (!Directory.Exists(serverDir)) return; // 源树缺失时不做陈旧度判定, 交给上层存在性断言

        var sources = Directory.EnumerateFiles(serverDir, "*.go", SearchOption.AllDirectories);
        var templateDir = Path.Combine(serverDir, "templates");
        if (Directory.Exists(templateDir))
        {
            sources = sources.Concat(
                Directory.EnumerateFiles(templateDir, "*", SearchOption.AllDirectories));
        }

        var newest = DateTime.MinValue;
        var newestPath = "";
        foreach (var f in sources)
        {
            var m = File.GetLastWriteTime(f);
            if (m > newest) { newest = m; newestPath = f; }
        }

        if (newest != DateTime.MinValue && exeMtime < newest)
        {
            Assert.Fail(
                "前置后端陈旧: " + HeadlessExeSource + " 的 mtime=" + exeMtime.ToString("yyyy-MM-dd HH:mm:ss") +
                " 早于 config-server 最新源 " + newestPath + " 的 mtime=" + newest.ToString("yyyy-MM-dd HH:mm:ss") +
                "。请先 make buildServer 并复制 bin/settings.exe 到 %TEMP%\\mk_settings_headless\\ 后重跑。");
        }
    }

    private static void CopyDirectory(string source, string target)
    {
        Directory.CreateDirectory(target);
        foreach (var file in Directory.GetFiles(source))
        {
            File.Copy(file, Path.Combine(target, Path.GetFileName(file)));
        }
        foreach (var dir in Directory.GetDirectories(source))
        {
            CopyDirectory(dir, Path.Combine(target, Path.GetFileName(dir)));
        }
    }
}

/// <summary>
/// 集成测试基类: 每个测试方法独占一个模拟部署目录与 headless 子进程,
/// 用例之间完全隔离 (PUT 改写 config.json 不影响他人), 可重复运行。
/// </summary>
public abstract class ServerTestBase : IAsyncLifetime
{
    protected SettingsTestServer Server { get; } = new();

    protected SettingsApiClient Client => Server.Client;

    public Task InitializeAsync() => Server.InitializeAsync();

    public Task DisposeAsync() => Server.DisposeAsync();
}
