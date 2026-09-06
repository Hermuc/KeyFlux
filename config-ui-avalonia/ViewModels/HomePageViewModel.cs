using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using KeyFlux.Settings.Services;
using System.Collections.ObjectModel;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 总览页 (复刻 Home.vue): Vue 版是 &lt;iframe src="/config_doc.html"&gt; 展示帮助文档。
/// Avalonia 无内嵌浏览器, 等价实现: 渲染 config_doc.md (config_doc.html 的源,
/// 标题层级/图片/链接全部保留); 用户自定义过总览页 (config.overviewDocMd 非空) 时
/// 优先渲染自定义内容。拉取失败时回退到内置快速上览引导。
/// 编辑入口: HomePageView 底部「编辑区」(EditZoneHint 控件) 打开 OverviewEditWindow。
/// </summary>
public sealed partial class HomePageViewModel : ObservableObject
{
    private readonly BackendSession _session;
    private readonly MainViewModel _main;

    public HomePageViewModel(BackendSession session, MainViewModel main)
    {
        _session = session;
        _main = main;
    }

    /// <summary>供编辑窗口保存配置 (OverviewEditWindow 需要 MainViewModel.SaveAsync)。</summary>
    public MainViewModel Main => _main;

    [ObservableProperty]
    private bool _isLoading = true;

    /// <summary>true = 成功获取到文档 (自定义或默认) 并完成渲染。</summary>
    [ObservableProperty]
    private bool _docAvailable;

    /// <summary>语言切换递增, 驱动静态文案绑定重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>渲染出的文档块 (标题/列表/图片/段落), ItemsControl 展示。</summary>
    public ObservableCollection<Control> DocBlocks { get; } = [];

    /// <summary>当前渲染的 markdown 原文 (编辑窗口的初始内容)。</summary>
    public string CurrentMd { get; private set; } = "";

    public async Task LoadAsync(CancellationToken ct = default)
    {
        IsLoading = true;
        try
        {
            // 用户自定义内容优先; 为空时展示默认文档 (bin/site/config_doc.md)
            var md = _main.Config?.OverviewDocMd ?? "";
            if (string.IsNullOrWhiteSpace(md))
            {
                md = await _session.GetRawTextAsync("/config_doc.md", ct) ?? "";
            }

            if (!string.IsNullOrWhiteSpace(md))
            {
                CurrentMd = md;
                Render(md);
                DocAvailable = true;
            }
            else
            {
                DocAvailable = false;
            }
        }
        catch (Exception)
        {
            DocAvailable = false;
        }
        finally
        {
            IsLoading = false;
        }
    }

    /// <summary>编辑窗口保存后刷新渲染。</summary>
    public async Task ReloadAsync() => await LoadAsync();

    /// <summary>解析 + 渲染 (职责分离: MarkdownParser 纯解析, MarkdownRenderer 纯控件构建)。</summary>
    private void Render(string md)
    {
        var blocks = MarkdownParser.Parse(md);
        DocBlocks.Clear();
        foreach (var control in MarkdownRenderer.Render(blocks, LoadImageAsync, OpenLink))
        {
            DocBlocks.Add(control);
        }
    }

    /// <summary>图片拉取: 相对路径 (img/xxx.png) 拼到后端静态站点根, 绝对路径直接用。</summary>
    private async Task<byte[]?> LoadImageAsync(string src)
    {
        var path = src.StartsWith('/') ? src : "/" + src;
        return await _session.GetBytesAsync(path);
    }

    /// <summary>链接打开策略 (委托 LinkOpener: 外部/魔法链接走系统默认程序, 内部路径拼后端地址)。</summary>
    private void OpenLink(string url) => LinkOpener.Open(url, _session.Port);
}
