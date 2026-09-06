using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.Tests.Infrastructure;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// POST /api/selected-action/test 契约 (方案 D 单键分发, 对照 Go selectedaction_test.go):
/// 快照优先 (编辑中未保存修改, 不落盘) / 磁盘回退 (不携快照时读部署配置) / isFile 双语义
/// (fileExt 要求 isFile=true, textType 要求 isFile=false) / 未命中 {"matched":false} 无菜单 / 400 {"message"}。
/// 命中响应: {"matched":true,"matchType","matchValue","menu":[{"key":1起,"behavior","name"}],"preview"}。
/// </summary>
public sealed class SelectedActionTestEndpointTests : ServerTestBase
{
    [Fact]
    public async Task Test_Snapshot_TextUrl_Matches_With_NumberedMenu()
    {
        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "https://example.com/page",
            IsFile = false,
            SelectedAction = TestData.ValidSnapshot(),
        });
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.True(resp.Value!.Matched);
        Assert.Equal("textType", resp.Value.MatchType);
        Assert.Equal("url", resp.Value.MatchValue);

        // 菜单键位 1 起, 顺序 = entries 顺序
        Assert.Equal(2, resp.Value.Menu.Count);
        Assert.Equal(1, resp.Value.Menu[0].Key);
        Assert.Equal("open_url", resp.Value.Menu[0].Behavior);
        Assert.False(string.IsNullOrEmpty(resp.Value.Menu[0].Name), "菜单项应携带显示名");
        Assert.Equal(2, resp.Value.Menu[1].Key);
        Assert.Equal("search", resp.Value.Menu[1].Behavior);

        // 预览基于选中内容生成 (open_url 语义)
        Assert.False(string.IsNullOrEmpty(resp.Value.Preview), "命中时应返回执行预览");
        Assert.Contains("https://example.com/page", resp.Value.Preview);
    }

    [Fact]
    public async Task Test_Snapshot_FileContent_MatchesFileExtMapping()
    {
        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = @"C:\图片\照片.jpg",
            IsFile = true,
            SelectedAction = TestData.ValidSnapshot(),
        });
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.True(resp.Value!.Matched);
        Assert.Equal("fileExt", resp.Value.MatchType);
        Assert.Equal("jpg,png", resp.Value.MatchValue);
        Assert.Single(resp.Value.Menu);
        Assert.Equal(1, resp.Value.Menu[0].Key);
        Assert.Equal("open", resp.Value.Menu[0].Behavior);
        // open 模板含 %selected%, 预览应替换为选中内容 (文件名可见)
        Assert.Contains("照片.jpg", resp.Value.Preview);
    }

    [Fact]
    public async Task Test_DiskFallback_MatchesSavedConfig()
    {
        // PUT /config 落盘一份配置, 然后不带快照测试 (回退磁盘语义)
        var get = await Client.GetConfigAsync();
        Assert.True(get.Success, get.ErrorMessage);
        get.Value!.SelectedAction = TestData.ValidSnapshot();
        var put = await Client.SaveConfigAsync(get.Value);
        Assert.True(put.Success, $"PUT /config 失败: {put.ErrorMessage}");

        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "https://example.com/",
            IsFile = false,
        });
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.True(resp.Value!.Matched);
        Assert.Equal("textType", resp.Value.MatchType);
        Assert.Equal("url", resp.Value.MatchValue);
    }

    [Fact]
    public async Task Test_NoMatch_ReturnsMatchedFalse_WithoutMenu()
    {
        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "just plain text",
            IsFile = false,
            SelectedAction = TestData.ValidSnapshot(),
        });
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.Equal(200, resp.StatusCode);
        Assert.False(resp.Value!.Matched);
        // 未命中不携带菜单与预览 (C# 侧反序列化为空集合)
        Assert.Empty(resp.Value.Menu);
        Assert.Equal("", resp.Value.Preview);
    }

    [Fact]
    public async Task Test_FileExtMapping_DoesNotMatchTextContent()
    {
        // isFile=false 时 fileExt 映射必须不命中 (matchActionRule 的 isFile 分支语义)
        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "a.jpg",
            IsFile = false,
            SelectedAction = TestData.FileExtOnlySnapshot(),
        });
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.False(resp.Value!.Matched);
    }

    [Fact]
    public async Task Test_Snapshot_TakesPriority_WithoutPersisting()
    {
        // 磁盘上是 fileExt(jpg) 配置, 请求快照是 textType(url):
        // 命中必须来自快照 (url), 且快照绝不落盘
        var get = await Client.GetConfigAsync();
        Assert.True(get.Success, get.ErrorMessage);
        get.Value!.SelectedAction = TestData.FileExtOnlySnapshot();
        var put = await Client.SaveConfigAsync(get.Value);
        Assert.True(put.Success, put.ErrorMessage);

        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "https://example.com/draft",
            IsFile = false,
            SelectedAction = TestData.ValidSnapshot(),
        });
        Assert.True(resp.Success, resp.ErrorMessage);
        Assert.True(resp.Value!.Matched);
        Assert.Equal("textType", resp.Value.MatchType);
        Assert.Equal("url", resp.Value.MatchValue);

        // 快照未被持久化: 磁盘配置仍是 fileExt(jpg), hotkey/enable 未被快照覆盖
        var after = await Client.GetConfigAsync();
        Assert.True(after.Success, after.ErrorMessage);
        Assert.Equal(">^f", after.Value!.SelectedAction.Hotkey);
        Assert.Single(after.Value.SelectedAction.Mappings);
        Assert.Equal("fileExt", after.Value.SelectedAction.Mappings[0].MatchType);
        Assert.Equal("jpg", after.Value.SelectedAction.Mappings[0].MatchValue);
    }

    [Fact]
    public async Task Test_EmptyEntriesSnapshot_Returns400_WithMessage()
    {
        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "https://example.com",
            IsFile = false,
            SelectedAction = TestData.EmptyEntriesSnapshot(),
        });
        Assert.False(resp.Success);
        Assert.Equal(400, resp.StatusCode);
        Assert.False(string.IsNullOrEmpty(resp.ErrorMessage), "400 应携带 message");
        Assert.Contains("没有行为", resp.ErrorMessage);
    }

    [Fact]
    public async Task Test_UnknownTextFeature_Returns400()
    {
        var resp = await Client.TestSelectedActionAsync(new SelectedActionTestRequest
        {
            Content = "anything",
            IsFile = false,
            SelectedAction = TestData.UnknownTextFeatureSnapshot(),
        });
        Assert.False(resp.Success);
        Assert.Equal(400, resp.StatusCode);
        Assert.Contains("未知的文本特征", resp.ErrorMessage);
    }
}
