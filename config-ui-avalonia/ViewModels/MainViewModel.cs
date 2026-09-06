using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 主窗口 (应用骨架) 视图模型, 对应 Vue 的 App.vue + NavigationDrawer + router:
///   - 通过 <see cref="BackendSession"/> 连接 settings.exe 后端 (不可达时进入友好错误态);
///   - GET /config -> <see cref="ConfigReadDefaults.Apply"/> -> 构建导航 (启用的 keymap);
///   - 保存链路 (复刻 store/config.ts saveConfig): 1 秒节流 -> 空键清洗 -> PUT /config,
///     400 时弹出后端 message, 传输层失败提示「可能设置程序被关了」;
///   - 语言切换经 <see cref="I18n.Changed"/> 分发到各页面 ViewModel。
/// </summary>
public sealed partial class MainViewModel : ObservableObject
{
    private readonly IMessageService _messages;
    private readonly BackendSessionOptions _sessionOptions;
    private DateTime _lastSaveAtUtc = DateTime.MinValue;
    private bool _saving;

    public MainViewModel(BackendSessionOptions sessionOptions, IMessageService? messages = null)
    {
        _sessionOptions = sessionOptions;
        _messages = messages ?? new DialogMessageService();
        Session = new BackendSession(sessionOptions);
        I18n.Changed += OnLanguageChangedGlobal;
    }

    // ------------------------------------------------------------- 会话与配置

    /// <summary>后端会话 (任务 #6 完整生命周期: 崩溃监测/整树关停/多层退出兜底)。</summary>
    public BackendSession Session { get; private set; }

    /// <summary>消息服务 (主窗口需要给 Dialog 实现注入 Owner)。</summary>
    public IMessageService Messages => _messages;

    [ObservableProperty]
    private Config? _config;

    [ObservableProperty]
    private bool _isLoading = true;

    [ObservableProperty]
    private bool _hasError;

    [ObservableProperty]
    private string _errorMessage = "";

    /// <summary>语言切换递增, 驱动主窗口层的 ConverterParameter 文案绑定重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>保存成功的短暂提示 (导航栏底部)。</summary>
    [ObservableProperty]
    private string? _saveNotice;

    // ------------------------------------------------------------- 页面与导航

    public HomePageViewModel? HomeVm { get; private set; }
    public SettingsPageViewModel? SettingsVm { get; private set; }
    public SelectedActionPageViewModel? ActionVm { get; private set; }

    private readonly Dictionary<int, object> _keymapPages = [];

    public ObservableCollection<NavItem> NavItems { get; } = [];

    [ObservableProperty]
    private NavItem? _currentNavItem;

    [ObservableProperty]
    private object? _currentPage;

    /// <summary>导航选中变化 (ListBox.SelectedItem 双向绑定驱动): 切换内容页。</summary>
    partial void OnCurrentNavItemChanged(NavItem? value)
    {
        CurrentPage = value?.Page;
    }

    // ------------------------------------------------------------- 初始化

    /// <summary>启动流程: 连接后端 -> 加载配置 -> 构建导航 -> 默认打开总览页。永不抛出。</summary>
    public async Task InitializeAsync()
    {
        IsLoading = true;
        HasError = false;
        ErrorMessage = "";

        if (!await Session.StartAsync())
        {
            Fail(Session.FailureReason ?? I18n.T("919"));
            return;
        }

        // 运行期崩溃监测: settings.exe 意外退出 -> 错误态 + 重试(重启后端)按钮。
        // 不自动重启 (防重启风暴); 正常关停 (Shutdown) 不会触发该事件。
        Session.ChildProcessExited += OnBackendCrashed;

        var resp = await Session.Api!.GetConfigAsync();
        if (!resp.Success || resp.Value is null)
        {
            Fail($"GET /config 失败: {resp.ErrorMessage}");
            return;
        }

        Config = ConfigReadDefaults.Apply(resp.Value);
        I18n.ApplyConfigLanguage(Config.Options.Language);

        HomeVm = new HomePageViewModel(Session, this);
        SettingsVm = new SettingsPageViewModel(this);
        ActionVm = new SelectedActionPageViewModel(this);
        _keymapPages.Clear();

        BuildNav();
        IsLoading = false;

        // 启动链路就绪标记 (stdout 重定向时供测试脚本计时; 无控制台时为空操作)
        try
        {
            Console.WriteLine(
                $"KEYFLUX_GUI_READY port={Session.Port} elapsed_ms={(long)(DateTime.UtcNow - Program.ProcessStartTimeUtc).TotalMilliseconds}");
            Console.Out.Flush(); // 重定向到文件时有缓冲, 必须显式刷新才能被外部及时读到
        }
        catch { /* 忽略 */ }

        // 总览文档异步拉取, 不阻塞首屏
        _ = HomeVm.LoadAsync();
    }

    /// <summary>后端子进程意外退出: 切到错误态 (展示原因), 重试按钮即「重启后端」。</summary>
    private void OnBackendCrashed(string reason)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() => Fail($"{I18n.T("919")}\n{reason}"));
    }

    private void Fail(string reason)
    {
        IsLoading = false;
        HasError = true;
        ErrorMessage = reason;
    }

    /// <summary>错误页「重试」: 结束旧会话后用原始参数重新初始化。</summary>
    [RelayCommand]
    private async Task RetryAsync()
    {
        await Session.DisposeAsync();
        Session = new BackendSession(_sessionOptions);
        await InitializeAsync();
    }

    // ------------------------------------------------------------- 导航构建

    /// <summary>设置页改动 (启用状态等) 后由 SettingsPageViewModel 调用。</summary>
    public void OnNavInvalidated() => BuildNav();

    /// <summary>
    /// 复刻 NavigationDrawer: 总览 + 选中动作 + 启用的 keymap 列表。
    /// 重建时保持原选中项 (按稳定 Id)。
    /// </summary>
    private void BuildNav()
    {
        if (Config is null || HomeVm is null) return;
        var selectedId = CurrentNavItem?.Id ?? "home";

        // 清理已删除 keymap 的页面缓存 (设置页删除行后不再持有死模型)
        var gone = _keymapPages.Keys.Where(id => Config.Keymaps.All(k => k.Id != id)).ToList();
        foreach (var id in gone) _keymapPages.Remove(id);

        var items = new List<NavItem>
        {
            new()
            {
                Id = "home",
                Title = I18n.T("913"),
                IconName = "home-outline",
                BadgeColorHex = "#4169E1",
                Page = HomeVm,
            },
            new()
            {
                Id = "action",
                Title = I18n.T("914"),
                IconName = "gesture-tap",
                BadgeColorHex = "#4169E1",
                Page = ActionVm!,
            },
        };

        foreach (var km in Config.Keymaps.Where(k => k.Enable))
        {
            var hotkey = NavBadge.EffectiveHotkey(km, Config.Keymaps);
            items.Add(new NavItem
            {
                Id = $"keymap-{km.Id}",
                Title = string.IsNullOrEmpty(km.Name) ? hotkey : km.Name,
                IconName = MdiIcon.IconFor(hotkey),
                BadgeColorHex = NavBadge.ColorFor(hotkey),
                Page = PageForKeymap(km),
            });
        }

        NavItems.Clear();
        foreach (var item in items) NavItems.Add(item);
        CurrentNavItem = NavItems.FirstOrDefault(n => n.Id == selectedId) ?? NavItems.FirstOrDefault();
    }

    /// <summary>keymap 条目 -> 页面 (复刻 router: id4=设置页, id1=自定义热键, id2|3=缩写, 其余=矩阵页)。</summary>
    private object PageForKeymap(Keymap km)
    {
        if (km.Id == 4) return SettingsVm!;
        if (_keymapPages.TryGetValue(km.Id, out var page)) return page;
        page = km.Id switch
        {
            1 => new CustomHotkeyPageViewModel(this, km),
            2 or 3 => new AbbrPageViewModel(this, km),
            _ => new KeymapPageViewModel(this, km),
        };
        _keymapPages[km.Id] = page;
        return page;
    }

    /// <summary>窗口分组保存后重建全部键位图系页面 (分组下拉/备注随之刷新)。</summary>
    public void RecreateKeymapPages()
    {
        _keymapPages.Clear();
        BuildNav();
    }

    // ------------------------------------------------------------- 保存链路

    /// <summary>保存命令 (导航栏按钮 / Ctrl+S): 带 1 秒节流 (复刻 useThrottleFn 1000)。</summary>
    [RelayCommand]
    private void Save() => _ = SaveAsync();

    public async Task<bool> SaveAsync(bool force = false)
    {
        if (Config is null || Session.Api is null || _saving) return false;

        // 评审 F1: 统一保存咽喉 —— 选中动作页关联分组的后缀修改先写回 fileGroups
        // (Ctrl+S / 侧栏「保存」/ 各页 SaveConfigAsync / 删除映射全部经此)。
        // 置于节流与 CleanForSave 之前: 被节流跳过的保存也已把内存 Config 写一致,
        // 下次真实保存自然落盘。(2026-09 方案 D: 写回源从编辑页单实例改为两分区行 VM)
        ActionVm?.ApplyFileGroupWriteBack();

        // 评审 C1: 两分区行 VM 直接持有底层 Mapping 对象, 但「添加/排序映射」只动
        // ObservableCollection, Sa.Mappings 须投影后才进入保存载荷 —— 页面自身的
        // SaveConfigAsync 已带 SyncToModel, 但 Ctrl+S / 侧栏「保存」等外部入口此前
        // 跳过了这一步, 新增映射在纯 Ctrl+S 流程下不落盘。与写回同理置于节流之前:
        // 被节流跳过的保存也已把内存 Config 写一致, 下次真实保存自然落盘。
        ActionVm?.SyncToModel();

        var now = DateTime.UtcNow;
        if (!force && (now - _lastSaveAtUtc).TotalMilliseconds < 1000) return false;
        _lastSaveAtUtc = now;

        _saving = true;
        try
        {
            // 空键清洗 (复刻 _saveConfig): 剔除 isEmpty 动作与布局外的自定义键
            var payload = ConfigSaver.CleanForSave(Config);
            var resp = await Session.Api.SaveConfigAsync(payload);

            if (resp.Success)
            {
                if (resp.Value?.RestartFailed == true)
                {
                    // 保存已落盘但 KeyFlux 重启失败: 明确提示手动重载,
                    // 避免「UI 显示保存成功但新热键/配置不生效」的困惑
                    _messages.Show(I18n.T("1078"), I18n.T("1079"));
                }
                else
                {
                    SaveNotice = I18n.T("928");
                }
                BuildNav(); // 启用状态可能变化
                ActionVm?.OnConfigSaved(); // 评审 L4: 保存成功后复位「热键未保存」提示条 (此前永不复位)
                _ = ClearNoticeAsync();
                return true;
            }

            if (resp.StatusCode == 0)
            {
                // 传输层失败: 复刻 Vue 的 "保存失败，可能设置程序被关了"
                _messages.Show(I18n.T("929"), $"{I18n.T("922")}\n{resp.ErrorMessage}");
            }
            else
            {
                // 400: 弹出后端 message (PUT /config 带「保存失败: 」前缀)
                _messages.Show(I18n.T("929"), resp.ErrorMessage ?? $"HTTP {resp.StatusCode}");
            }
            return false;
        }
        finally
        {
            _saving = false;
        }
    }

    private async Task ClearNoticeAsync()
    {
        await Task.Delay(2000);
        SaveNotice = null;
    }

    // ------------------------------------------------------------- 语言与消息

    private void OnLanguageChangedGlobal()
    {
        LanguageTick++;
        SettingsVm?.OnLanguageChanged();
        if (HomeVm is not null) HomeVm.LanguageTick++;
        ActionVm?.OnLanguageChanged();
        foreach (var page in _keymapPages.Values.OfType<ILanguageRefresh>()) page.OnLanguageChanged();
        BuildNav();
    }

    /// <summary>供页面 ViewModel 弹出提示 (如开机自启命令失败)。</summary>
    public void ShowMessage(string title, string message) => _messages.Show(title, message);

    /// <summary>窗口关闭: 终止会话 (多层兜底: Closing -> App.Exit -> Program.Main finally)。</summary>
    public async Task ShutdownAsync()
    {
        I18n.Changed -= OnLanguageChangedGlobal;
        await Session.DisposeAsync();
    }
}
