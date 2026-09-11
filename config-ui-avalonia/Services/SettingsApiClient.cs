using System.Net.Http.Json;
using System.Text.Json;
using System.Text.Json.Serialization;
using KeyFlux.Settings.Models;

namespace KeyFlux.Settings.Services;

// ============================================================================
// settings.exe (Go gin 服务) HTTP 客户端层
//
// 端点清单 (权威来源: config-server/cmd/settings/main.go + selectedaction.go):
//   GET    /config                      完整 Config JSON
//   PUT    /config                      完整 Config JSON -> 200 {"message":"ok"}
//                                       校验失败 -> 400 {"message":"保存失败: ..."}
//   GET    /shortcuts                   [{"path":"shortcuts\\xx.lnk"}] (相对部署根)
//   POST   /server/command/:id          id=2|3|4, 恒 200 {} (会 exec KeyFlux.exe)
//   POST   /api/selected-action/test    模拟测试 (含页面快照语义; 2026-09 方案 D,
//                                       旧 /api/action-schemes 6 路由已随多方案模型退役)
//   GET    /api/plugins                 用户插件目录 (data/plugins)
//   POST   /api/plugins/import          multipart file=zip -> 安装, 返回 manifest
//   DELETE /api/plugins/:id             删除用户插件目录
// ============================================================================

/// <summary>统一响应包装: 强类型结果 + 状态码 + 错误信息 (400/404 时解析 {"message":"..."} 字段)。</summary>
/// <param name="Success">HTTP 2xx 为 true。</param>
/// <param name="StatusCode">HTTP 状态码; 传输层异常 (连接失败/超时) 时为 0。</param>
/// <param name="Value">2xx 且响应体可反序列化时的强类型结果。</param>
/// <param name="ErrorMessage">非 2xx 时来自响应体 message 字段 (解析失败则为原始响应体); 传输层异常时为异常消息。</param>
/// <param name="RawBody">原始响应体文本 (便于诊断)。</param>
public sealed record ApiResponse<T>(
    bool Success,
    int StatusCode,
    T? Value,
    string? ErrorMessage = null,
    string RawBody = "");

/// <summary>形如 {"message":"..."} 的响应体 (PUT /config 校验失败等)。</summary>
public sealed record MessageBody
{
    [JsonPropertyName("message")]
    public string Message { get; set; } = "";

    /// <summary>
    /// 保存后 KeyFlux 进程重启失败时为 true (保存已落盘, 需经托盘「重载」手动生效);
    /// 旧后端无此字段 -&gt; 反序列化为 null, 视同成功, 保持向后兼容。
    /// </summary>
    [JsonPropertyName("restartFailed")]
    public bool? RestartFailed { get; set; }
}

/// <summary>GET /shortcuts 的列表项。</summary>
public sealed record ShortcutInfo
{
    /// <summary>相对部署根的路径, 如 "shortcuts\\微信.lnk"。</summary>
    [JsonPropertyName("path")]
    public string Path { get; set; } = "";
}

/// <summary>形如 {} 的空对象响应体 (POST /server/command/:id)。</summary>
public sealed record EmptyJson;

/// <summary>POST /api/selected-action/test 请求体 (对照 Go selectedActionTestRequest)。</summary>
public sealed class SelectedActionTestRequest
{
    /// <summary>模拟的选中内容 (文本或文件路径列表, 多行用 \n 分隔)。</summary>
    [JsonPropertyName("content")]
    public string Content { get; set; } = "";

    /// <summary>true=内容是文件路径; false=内容是文本。</summary>
    [JsonPropertyName("isFile")]
    public bool IsFile { get; set; }

    /// <summary>页面编辑中的 selectedAction 快照 (未保存的修改也能测试); null 时后端回退读取磁盘配置。</summary>
    [JsonPropertyName("selectedAction")]
    public SelectedAction? SelectedAction { get; set; }
}

/// <summary>POST /api/selected-action/test 响应菜单项 (key 从 1 起, 顺序即映射 entries 顺序)。</summary>
public sealed class SelectedActionMenuEntry
{
    /// <summary>菜单键位序号 (1..9)。</summary>
    [JsonPropertyName("key")]
    public int Key { get; set; }

    /// <summary>行为包 ID。</summary>
    [JsonPropertyName("behavior")]
    public string Behavior { get; set; } = "";

    /// <summary>行为显示名 (后端按语言 name/nameEn 推导)。</summary>
    [JsonPropertyName("name")]
    public string Name { get; set; } = "";
}

/// <summary>POST /api/selected-action/test 响应体。</summary>
public sealed class SelectedActionTestResult
{
    [JsonPropertyName("matched")]
    public bool Matched { get; set; }

    /// <summary>matched=true 时命中的映射类型 ("textType" | "fileExt")。</summary>
    [JsonPropertyName("matchType")]
    public string MatchType { get; set; } = "";

    /// <summary>matched=true 时命中的映射值 (文本特征词或后缀串)。</summary>
    [JsonPropertyName("matchValue")]
    public string MatchValue { get; set; } = "";

    /// <summary>matched=true 时的菜单键位表。</summary>
    [JsonPropertyName("menu")]
    public List<SelectedActionMenuEntry> Menu { get; set; } = [];

    /// <summary>matched=true 时的执行预览文本 (取菜单第一项行为展开)。</summary>
    [JsonPropertyName("preview")]
    public string Preview { get; set; } = "";
}

/// <summary>
/// settings.exe API 抽象, 便于 ViewModel 依赖注入与单测替换 (可用假实现替身)。
/// 所有方法永不抛出网络异常: 传输层错误统一折叠为 StatusCode=0 的失败响应。
/// </summary>
public interface ISettingsApi
{
    Task<ApiResponse<Config>> GetConfigAsync(CancellationToken ct = default);
    Task<ApiResponse<MessageBody>> SaveConfigAsync(Config config, CancellationToken ct = default);
    Task<ApiResponse<List<ShortcutInfo>>> GetShortcutsAsync(CancellationToken ct = default);
    Task<ApiResponse<EmptyJson>> SendServerCommandAsync(int id, CancellationToken ct = default);

    Task<ApiResponse<SelectedActionTestResult>> TestSelectedActionAsync(SelectedActionTestRequest request, CancellationToken ct = default);

    // 行为包 (选中动作「行为库」, CONTRACTS §3.9)
    Task<ApiResponse<BehaviorCatalogResponse>> GetBehaviorsAsync(CancellationToken ct = default);
    Task<ApiResponse<BehaviorPack>> CreateBehaviorAsync(BehaviorPack pack, CancellationToken ct = default);
    Task<ApiResponse<BehaviorPack>> UpdateBehaviorAsync(string id, BehaviorPack pack, CancellationToken ct = default);
    Task<ApiResponse<MessageBody>> DeleteBehaviorAsync(string id, CancellationToken ct = default);
    Task<ApiResponse<MessageBody>> ApplyBehaviorsAsync(CancellationToken ct = default);

    // 插件 (插件页: 管理面走专用 API, 启停走 PUT /config)
    Task<ApiResponse<PluginListResponse>> GetPluginsAsync(CancellationToken ct = default);
    Task<ApiResponse<PluginManifest>> ImportPluginAsync(byte[] zipBytes, string fileName, CancellationToken ct = default);
    Task<ApiResponse<MessageBody>> DeletePluginAsync(string id, CancellationToken ct = default);
}

/// <summary>
/// 基于 HttpClient 的 settings.exe API 客户端。
/// 构造时接受端口 (headless settings.exe 通过 "KEYFLUX_PORT=" 行通告实际端口,
/// 12333 被占用时会退到随机端口, 故基址必须按通告端口构造)。
/// </summary>
public sealed class SettingsApiClient : ISettingsApi, IDisposable
{
    private readonly HttpClient _http;
    private readonly bool _ownsHttpClient;

    /// <summary>按端口构造基址: http://localhost:{port}</summary>
    public SettingsApiClient(int port, TimeSpan? timeout = null)
    {
        _http = new HttpClient { BaseAddress = new Uri($"http://localhost:{port}") };
        _ownsHttpClient = true;
        // 保存配置会触发 Go 侧写盘 + 生成逻辑, 预留充足超时
        _http.Timeout = timeout ?? TimeSpan.FromSeconds(10);
    }

    /// <summary>测试替身用: 注入外部 HttpClient (调用方负责其生命周期)。</summary>
    public SettingsApiClient(HttpClient httpClient)
    {
        _http = httpClient;
        _ownsHttpClient = false;
    }

    public Uri BaseAddress => _http.BaseAddress ?? throw new InvalidOperationException("BaseAddress 未设置");

    public Task<ApiResponse<Config>> GetConfigAsync(CancellationToken ct = default)
        => SendAsync<Config>(HttpMethod.Get, "config", content: null, ct);

    public Task<ApiResponse<MessageBody>> SaveConfigAsync(Config config, CancellationToken ct = default)
        => SendAsync<MessageBody>(HttpMethod.Put, "config", config, ct);

    public Task<ApiResponse<List<ShortcutInfo>>> GetShortcutsAsync(CancellationToken ct = default)
        => SendAsync<List<ShortcutInfo>>(HttpMethod.Get, "shortcuts", content: null, ct);

    public Task<ApiResponse<EmptyJson>> SendServerCommandAsync(int id, CancellationToken ct = default)
        => SendAsync<EmptyJson>(HttpMethod.Post, $"server/command/{id}", new EmptyJson(), ct);

    public Task<ApiResponse<SelectedActionTestResult>> TestSelectedActionAsync(SelectedActionTestRequest request, CancellationToken ct = default)
        => SendAsync<SelectedActionTestResult>(HttpMethod.Post, "api/selected-action/test", request, ct);

    public Task<ApiResponse<BehaviorCatalogResponse>> GetBehaviorsAsync(CancellationToken ct = default)
        => SendAsync<BehaviorCatalogResponse>(HttpMethod.Get, "api/behaviors", content: null, ct);

    public Task<ApiResponse<BehaviorPack>> CreateBehaviorAsync(BehaviorPack pack, CancellationToken ct = default)
        => SendAsync<BehaviorPack>(HttpMethod.Post, "api/behaviors", pack, ct);

    public Task<ApiResponse<BehaviorPack>> UpdateBehaviorAsync(string id, BehaviorPack pack, CancellationToken ct = default)
        => SendAsync<BehaviorPack>(HttpMethod.Put, $"api/behaviors/{id}", pack, ct);

    public Task<ApiResponse<MessageBody>> DeleteBehaviorAsync(string id, CancellationToken ct = default)
        => SendAsync<MessageBody>(HttpMethod.Delete, $"api/behaviors/{id}", content: null, ct);

    public Task<ApiResponse<MessageBody>> ApplyBehaviorsAsync(CancellationToken ct = default)
        => SendAsync<MessageBody>(HttpMethod.Post, "api/behaviors/apply", content: null, ct);

    public Task<ApiResponse<PluginListResponse>> GetPluginsAsync(CancellationToken ct = default)
        => SendAsync<PluginListResponse>(HttpMethod.Get, "api/plugins", content: null, ct);

    public Task<ApiResponse<PluginManifest>> ImportPluginAsync(byte[] zipBytes, string fileName, CancellationToken ct = default)
        => SendMultipartAsync<PluginManifest>("api/plugins/import", zipBytes, fileName, ct);

    public Task<ApiResponse<MessageBody>> DeletePluginAsync(string id, CancellationToken ct = default)
        => SendAsync<MessageBody>(HttpMethod.Delete, $"api/plugins/{id}", content: null, ct);

    /// <summary>
    /// 非契约端点的便捷原始文本 GET (如 Home 页的 /config_doc.html 静态资源)。
    /// 不属于 12 个契约端点, 故不进 ISettingsApi 接口; 响应体按纯文本处理。
    /// </summary>
    public async Task<ApiResponse<string>> GetRawTextAsync(string path, CancellationToken ct = default)
    {
        HttpResponseMessage response;
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, path);
            response = await _http.SendAsync(request, ct).ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or InvalidOperationException)
        {
            return new ApiResponse<string>(false, 0, default, ex.Message);
        }

        using (response)
        {
            var raw = await response.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
            var status = (int)response.StatusCode;
            return response.IsSuccessStatusCode
                ? new ApiResponse<string>(true, status, raw, RawBody: raw)
                : new ApiResponse<string>(false, status, default, ExtractErrorMessage(raw), raw);
        }
    }

    // ------------------------------------------------------------------ 内部

    private async Task<ApiResponse<T>> SendAsync<T>(
        HttpMethod method, string path, object? content, CancellationToken ct)
    {
        HttpResponseMessage response;
        try
        {
            using var request = new HttpRequestMessage(method, path);
            if (content is not null)
            {
                request.Content = JsonContent.Create(content, options: SettingsJson.Options);
            }
            response = await _http.SendAsync(request, ct).ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or InvalidOperationException)
        {
            // TaskCanceledException 含超时; 折叠为 StatusCode=0 的失败响应, ViewModel 无需 try/catch
            return new ApiResponse<T>(false, 0, default, ex.Message);
        }

        using (response)
        {
            var raw = await response.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
            var status = (int)response.StatusCode;

            if (response.IsSuccessStatusCode)
            {
                try
                {
                    var value = string.IsNullOrWhiteSpace(raw)
                        ? default
                        : JsonSerializer.Deserialize<T>(raw, SettingsJson.Options);
                    return new ApiResponse<T>(true, status, value, RawBody: raw);
                }
                catch (JsonException ex)
                {
                    return new ApiResponse<T>(false, status, default, $"响应体反序列化失败: {ex.Message}", raw);
                }
            }

            return new ApiResponse<T>(false, status, default, ExtractErrorMessage(raw), raw);
        }
    }

    /// <summary>multipart 上传变体 (POST /api/plugins/import): file 字段携带插件包 zip 字节。</summary>
    private async Task<ApiResponse<T>> SendMultipartAsync<T>(
        string path, byte[] zipBytes, string fileName, CancellationToken ct)
    {
        HttpResponseMessage response;
        try
        {
            using var form = new MultipartFormDataContent();
            var fileContent = new ByteArrayContent(zipBytes);
            fileContent.Headers.ContentType = new System.Net.Http.Headers.MediaTypeHeaderValue("application/zip");
            form.Add(fileContent, "file", fileName);
            response = await _http.PostAsync(path, form, ct).ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or InvalidOperationException)
        {
            return new ApiResponse<T>(false, 0, default, ex.Message);
        }

        using (response)
        {
            var raw = await response.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
            var status = (int)response.StatusCode;
            if (response.IsSuccessStatusCode)
            {
                try
                {
                    var value = string.IsNullOrWhiteSpace(raw)
                        ? default
                        : JsonSerializer.Deserialize<T>(raw, SettingsJson.Options);
                    return new ApiResponse<T>(true, status, value, RawBody: raw);
                }
                catch (JsonException ex)
                {
                    return new ApiResponse<T>(false, status, default, $"响应体反序列化失败: {ex.Message}", raw);
                }
            }
            return new ApiResponse<T>(false, status, default, ExtractErrorMessage(raw), raw);
        }
    }

    /// <summary>非 2xx 时优先取 {"message":"..."}; 解析失败回退原始响应体。</summary>
    private static string ExtractErrorMessage(string raw)
    {
        if (!string.IsNullOrWhiteSpace(raw))
        {
            try
            {
                var body = JsonSerializer.Deserialize<MessageBody>(raw, SettingsJson.Options);
                if (!string.IsNullOrEmpty(body?.Message))
                {
                    return body.Message;
                }
            }
            catch (JsonException)
            {
                // 落入下方回退
            }
        }
        return string.IsNullOrWhiteSpace(raw) ? "(空响应体)" : raw;
    }

    /// <summary>
    /// 非契约端点的便捷原始字节 GET (Home 页 markdown 里的图片静态资源)。
    /// 响应体按二进制处理, 供 Bitmap 解码。
    /// </summary>
    public async Task<ApiResponse<byte[]>> GetBytesAsync(string path, CancellationToken ct = default)
    {
        HttpResponseMessage response;
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, path);
            response = await _http.SendAsync(request, ct).ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or InvalidOperationException)
        {
            return new ApiResponse<byte[]>(false, 0, default, ex.Message);
        }

        using (response)
        {
            var raw = await response.Content.ReadAsByteArrayAsync(ct).ConfigureAwait(false);
            var status = (int)response.StatusCode;
            if (!response.IsSuccessStatusCode)
            {
                var text = await response.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
                var msg = string.IsNullOrWhiteSpace(text) ? $"HTTP {status}" : $"HTTP {status}: {text}";
                return new ApiResponse<byte[]>(false, status, default, msg);
            }
            return new ApiResponse<byte[]>(true, status, raw, "");
        }
    }

    public void Dispose()
    {
        if (_ownsHttpClient)
        {
            _http.Dispose();
        }
    }
}
