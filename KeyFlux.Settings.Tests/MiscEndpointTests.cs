using KeyFlux.Settings.Tests.Infrastructure;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// GET /shortcuts 与 POST /server/command/:id 契约。
/// 注意: 模拟目录不含 KeyFlux.exe, execCmd 静默失败 —— 命令端点恒 200 且无副作用。
/// </summary>
public sealed class MiscEndpointTests : ServerTestBase
{
    [Fact]
    public async Task GetShortcuts_ReturnsRelativeLnkPaths()
    {
        var resp = await Client.GetShortcutsAsync();
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.NotNull(resp.Value);
        Assert.Equal(3, resp.Value!.Count);

        var paths = resp.Value.Select(s => s.Path).ToHashSet();
        Assert.All(paths, p => Assert.StartsWith("shortcuts\\", p));
        Assert.Contains("shortcuts\\alpha.lnk", paths);
        Assert.Contains("shortcuts\\beta.lnk", paths);
        Assert.Contains("shortcuts\\快捷方式.lnk", paths);
    }

    [Theory]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    public async Task ServerCommand_Returns200_EmptyJson_NoSideEffects(int id)
    {
        var resp = await Client.SendServerCommandAsync(id);
        Assert.True(resp.Success, $"POST /server/command/{id} 失败: {resp.ErrorMessage}");
        Assert.Equal(200, resp.StatusCode);
        Assert.Equal("{}", resp.RawBody.Trim());

        // 给子进程启动留一点时间, 然后确认模拟目录内没有凭空多出 KeyFlux.exe
        await Task.Delay(300);
        Assert.False(File.Exists(Path.Combine(Server.RootDir, "KeyFlux.exe")),
            "模拟目录不应出现 KeyFlux.exe (execCmd 找不到目标应静默失败)");
        Assert.Empty(Directory.GetFiles(Server.RootDir, "KeyFlux.exe", SearchOption.AllDirectories));
    }
}
