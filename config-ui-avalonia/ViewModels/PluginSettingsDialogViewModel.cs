using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;

namespace KeyFlux.Settings.ViewModels;

/// <summary>
/// 用户插件设置对话框 VM —— 按 manifest.settings 的声明渲染表单。
///
/// 数据流: 打开时 GET /api/plugins/:id/settings 取「声明 + 默认值合并后的完整值表」,
/// 逐项落进 <see cref="Rows"/>; 保存时收集各行当前值 PUT 回后端, 后端按同一份声明
/// 校验并写入 data/plugin-settings.json。
///
/// 校验分两层 (口径同源, 与 QuickSwitch 对话框的「副本编辑」不同 —— 这里值不落在
/// config.json 上, 所以没有副本概念): 本 VM 先做一遍即时校验给出快速反馈, 后端仍是
/// 权威 (拒绝时把 message 原样弹给用户)。两层都拦不住的脏值不存在 —— 前端只做
/// 长度/数字/上下限这类与声明一一对应的检查。
/// </summary>
public sealed partial class PluginSettingsDialogViewModel : ObservableObject
{
    private readonly MainViewModel _main;
    private readonly PluginManifest _manifest;

    public PluginSettingsDialogViewModel(MainViewModel main, PluginManifest manifest)
    {
        _main = main;
        _manifest = manifest;
        IsLoading = true;
    }

    /// <summary>插件显示名 (英文界面优先 nameEn, 同卡片口径)。</summary>
    public string DisplayName =>
        I18n.Language == I18n.En && !string.IsNullOrEmpty(_manifest.NameEn)
            ? _manifest.NameEn!
            : _manifest.Name;

    /// <summary>设置行 (按 manifest 声明顺序; 由 <see cref="LoadAsync"/> 填充)。</summary>
    public ObservableCollection<PluginSettingRowVm> Rows { get; } = [];

    [ObservableProperty]
    private bool _isLoading = true;

    /// <summary>加载失败原因 (非空时以红字呈现, 表单为空)。</summary>
    [ObservableProperty]
    private string? _loadError;

    /// <summary>语言切换递增, 驱动窗口静态文案重算。</summary>
    [ObservableProperty]
    private int _languageTick;

    /// <summary>保存成功后置 true (视图据此关闭窗口)。</summary>
    public bool Saved { get; private set; }

    /// <summary>无任何可配置项 (空态提示; 正常情况下页面不会放行到本对话框)。</summary>
    public bool ShowEmptyState => !IsLoading && LoadError is null && Rows.Count == 0;

    public bool ShowForm => !IsLoading && LoadError is null && Rows.Count > 0;

    /// <summary>从后端拉取声明与值。</summary>
    public async Task LoadAsync()
    {
        if (_main.Session.Api is not { } api)
        {
            LoadError = I18n.T("2584");
            IsLoading = false;
            RaiseStateChanged();
            return;
        }

        var resp = await api.GetPluginSettingsAsync(_manifest.Id);
        if (!resp.Success || resp.Value is null)
        {
            LoadError = resp.ErrorMessage ?? $"HTTP {resp.StatusCode}";
            IsLoading = false;
            RaiseStateChanged();
            return;
        }

        var value = resp.Value;
        // 声明以服务端为准 (它才是校验依据); 服务端返回空声明时退回本地 manifest,
        // 保证「卡片带 settings、服务端目录暂时读不到」的窗口期仍能渲染出表单。
        var schema = value.Settings.Count > 0 ? value.Settings : (_manifest.Settings ?? []);
        Rows.Clear();
        foreach (var s in schema)
        {
            var initial = value.Values.TryGetValue(s.Key, out var v) ? v : s.Default ?? "";
            Rows.Add(new PluginSettingRowVm(s, initial));
        }
        IsLoading = false;
        RaiseStateChanged();
    }

    /// <summary>
    /// 收集表单并保存。返回 true = 已落盘 (视图可关闭)。
    /// 校验失败/后端拒绝时弹提示并返回 false, 窗口保持打开供修正。
    /// </summary>
    public async Task<bool> SaveAsync()
    {
        if (_main.Session.Api is not { } api) return false;

        var values = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var row in Rows)
        {
            var err = row.Validate();
            if (err is not null)
            {
                _main.ShowMessage(I18n.T("2585"), $"{row.Label}: {err}");
                return false;
            }
            values[row.Key] = row.Value;
        }

        var resp = await api.SavePluginSettingsAsync(_manifest.Id, values);
        if (!resp.Success)
        {
            _main.ShowMessage(I18n.T("2585"), resp.ErrorMessage ?? $"HTTP {resp.StatusCode}");
            return false;
        }
        Saved = true;
        return true;
    }

    /// <summary>语言切换: 刷新行文案 (标签/提示来自 manifest 的中英字段)。</summary>
    public void OnLanguageChanged()
    {
        LanguageTick++;
        foreach (var row in Rows) row.RefreshLanguage();
        OnPropertyChanged(nameof(DisplayName));
    }

    private void RaiseStateChanged()
    {
        OnPropertyChanged(nameof(ShowEmptyState));
        OnPropertyChanged(nameof(ShowForm));
    }
}

/// <summary>
/// 单行设置项: 声明 + 编辑中的值。
/// 标签与提示取自 manifest 的 label/labelEn 与 hint/hintEn (数据驱动, 非 i18n 键)。
/// </summary>
public sealed partial class PluginSettingRowVm : ObservableObject
{
    public PluginSettingRowVm(PluginSetting setting, string value)
    {
        Setting = setting;
        _value = value;
    }

    public PluginSetting Setting { get; }

    public string Key => Setting.Key;

    /// <summary>编辑中的值 (保存时原样提交, 空串语义 = 清除覆盖回落默认值)。</summary>
    [ObservableProperty]
    private string _value;

    partial void OnValueChanged(string value) => OnPropertyChanged(nameof(ShowSpaceToken));

    public string Label => Pick(Setting.Label, Setting.LabelEn, Setting.Label);

    public string Hint => Pick(Setting.Hint, Setting.HintEn, Setting.Hint ?? "");

    private static bool IsEn => I18n.Language == I18n.En;

    /// <summary>英文界面优先英文, 缺失回退中文 (同 nameEn 口径)。</summary>
    private static string Pick(string? zh, string? en, string? fallback) =>
        IsEn && !string.IsNullOrEmpty(en) ? en! : (string.IsNullOrEmpty(zh) ? fallback ?? "" : zh!);

    // ---- 编辑器选型 (XAML 按这些标志切换控件) ----

    public bool IsChar => Setting.Type == PluginSettingTypes.Char;

    public bool IsFile => Setting.Type == PluginSettingTypes.File;

    public bool IsNumber => Setting.Type == PluginSettingTypes.Number;

    public bool IsText => !IsChar && !IsFile && !IsNumber;

    /// <summary>输入框字符上限 (与后端 Setting.ValueLimit 口径一致)。</summary>
    public int MaxLength => IsChar ? 1 : Setting.MaxLength > 0 ? Setting.MaxLength : 1024;

    /// <summary>数字项的上下限说明 (无边界时为空)。</summary>
    public string RangeHint
    {
        get
        {
            if (!IsNumber) return "";
            if (Setting.Min is not null && Setting.Max is not null) return $"{Setting.Min:0} – {Setting.Max:0}";
            if (Setting.Min is not null) return $">= {Setting.Min:0}";
            if (Setting.Max is not null) return $"<= {Setting.Max:0}";
            return "";
        }
    }

    public bool HasRange => RangeHint.Length > 0;

    /// <summary>
    /// 当前值是空格 (或已清空 → 实际回落默认空格) 时, 在输入框下方补一行可读回显。
    /// 单字符输入框里放不下「空格」的视觉线索, 否则用户看到的是一个空框。
    /// </summary>
    public bool ShowSpaceToken => IsChar && (Value is "" or " ");

    /// <summary>
    /// 本地即时校验 (后端仍是权威, 这里只是少一次往返)。
    /// 返回 null = 通过; 否则为可直接展示的原因。
    /// </summary>
    public string? Validate()
    {
        var v = Value ?? "";
        if (v.Length == 0) return null; // 空 = 清除覆盖, 永远合法

        if (v.Length > MaxLength) return $"≤ {MaxLength}";
        if (v.Contains('\0')) return "NUL";

        if (IsChar)
        {
            var r = v[0];
            if (r < 0x20 || r == 0x7F) return "printable only";
        }
        if (IsNumber)
        {
            if (!long.TryParse(v, out var n)) return "integer";
            if (Setting.Min is { } min && n < min) return $">= {min:0}";
            if (Setting.Max is { } max && n > max) return $"<= {max:0}";
        }
        return null;
    }

    public void RefreshLanguage()
    {
        OnPropertyChanged(nameof(Label));
        OnPropertyChanged(nameof(Hint));
    }
}
