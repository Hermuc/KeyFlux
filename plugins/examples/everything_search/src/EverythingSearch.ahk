; ============================================================
; EverythingSearch —— 编排层: 保证 Everything 已运行 -> 选通道 -> 查询 -> 失败重试。
; 本层不做 UI, 也不解析结果格式 (那是通道层的事); 只处理「时序与降级」。
;
; 静默拉起 (2026-09-20): Everything 未运行时用 `-startup` 在**后台**拉起 —— 不弹主窗口、
;   不抢前台焦点, 避免打断命令框里正在进行的输入。详见 EnsureRunning 与 README。
; ============================================================

class EverythingSearch {
  static ProcessName := "Everything.exe"
  static LaunchWaitMs := 6000     ; 自动拉起后最多等这么久 (冷启动要读 db)
  static LaunchPollMs := 250
  static RetryDelayMs := 400      ; 首次查询失败后的重试间隔 (数据库加载竞态)

  ; 静默启动开关: 官方文档 "Run Everything in the background without showing any search windows"。
  ; 必须带它 —— 裸跑 everything.exe 会弹出主窗口并抢走前台焦点, 打断命令框里正在进行的输入。
  ; 见 README 「未启动时静默拉起」与 docs/CONTRACTS 的插件章节。
  static StartupSwitch := "-startup"
  ; 兜底: -startup 在某些配置下仍可能出窗 (如启动时 force-show 类设置), 故启动后再主动隐藏一次。
  static HideCheckMs := 1200       ; 给主窗口留出创建时间再检查/隐藏
  static HidePollMs := 250         ; 隐藏检查的轮询步长

  /** Everything 进程是否在运行 (不依赖 IPC 窗口, 避免版本差异)。 */
  static IsRunning() {
    return ProcessExist(this.ProcessName) ? true : false
  }

  /**
   * 保证 Everything 已运行: 未运行则按配置路径**静默**拉起并轮询等待。
   *
   * 静默要求 (用户可见行为):
   *   - 不弹出 Everything 主窗口;
   *   - 不抢前台焦点 (命令框输入不被打断);
   *   - 进程在托盘/后台继续索引, 后续 es 查询照常工作。
   * 已在运行时**不做任何额外处理** (不重复启动、不动已有窗口)。
   *
   * @returns {boolean} false = 未运行且无法拉起 (未配置路径 / 路径无效 / 启动失败)
   */
  static EnsureRunning() {
    if (this.IsRunning())
      return true

    exe := EverythingSettings.EverythingPath
    if (exe = "" || !FileExist(exe))
      return false

    try {
      Run('"' exe '" ' this.StartupSwitch)
    } catch {
      return false
    }

    ; 进程探活 (必要条件): 拉起后轮询等待进程出现。
    waited := 0
    while (waited < this.LaunchWaitMs) {
      Sleep this.LaunchPollMs
      waited += this.LaunchPollMs
      if (this.IsRunning()) {
        this.HideMainWindowIfAny()
        return true
      }
    }
    return false
  }

  /**
   * 兜底降级: 若 Everything 主窗口仍然可见, 主动隐藏它。
   *
   * 为什么需要: `-startup` 是官方唯一的「后台启动」开关, 但它作用于**启动瞬间的窗口策略**;
   * 若 Everything.ini 里存在强制显示类配置 (如 maximized/bring_into_view 组合, 或用户装了
   * 会在启动时弹窗的魔改版), 主窗口仍可能出现。此时主动 WinHide 比放任抢焦点好。
   *
   * 2026-09-20 实测 (mini3 探针): 本机 Everything 1.5.0.1418 + `-startup` 启动全程
   *   **只创建 2 个窗口, 均为 visible=0** —— 即 IME 辅助窗 `MSCTFIME UI` / `Default IME`,
   *   没有任何可见主窗口。故在正常配置下本方法是 no-op, 仅作防御。
   */
  static HideMainWindowIfAny() {
    ; 注意: 不切 DetectHiddenWindows —— WinGetList 默认只返回**可见**顶层窗口, 这正是要筛的目标;
    ; 若开了 DetectHiddenWindows 反而会把隐藏的 IME 辅助窗也捞进来 (实测 mini3 那两个 visible=0)。
    deadline := A_TickCount + this.HideCheckMs
    while (A_TickCount < deadline) {
      if (this._HideVisibleMainWindow())
        return true
      Sleep this.HidePollMs
    }
    return false
  }

  /**
   * 枚举 Everything 进程的可见顶层窗口, 隐藏其中的主窗口 (排除已知辅助窗)。
   *
   * 筛选策略 (按类名黑名单, **不以标题为主筛**): 实测 Everything 在 `-startup` 模式下
   * 创建的窗口标题可能为空, 若用「标题非空」当主筛会漏掉真正的主窗口。故改为仅排除
   * 两个确定的 IME 辅助类, 其余可见顶层窗一律视为主窗口隐藏。
   *
   * @returns {boolean} 是否隐藏了至少一个窗口
   */
  static _HideVisibleMainWindow() {
    hidden := false
    try {
      windows := WinGetList("ahk_exe " this.ProcessName)
    } catch {
      return false
    }
    for hwnd in windows {
      ; 排除 IME 辅助窗 (按类名, 不用标题 —— 见方法注释)
      cls := ""
      try cls := WinGetClass(hwnd)
      if (cls = "MSCTFIME UI" || cls = "IME" || cls = "Default IME")
        continue
      ; 只处理**可见**窗口 (WinGetList 已按可见性过滤, 此处再核一次防御)
      if (!DllCall("IsWindowVisible", "ptr", hwnd, "int"))
        continue
      try {
        WinHide(hwnd)
        hidden := true
      }
    }
    return hidden
  }

  /**
   * 执行一次搜索 (同步; 调用方在命令框输入期, 单次 es 调用约 10-60ms)。
   * @param query 检索词
   * @param limit 结果条数上限
   * @returns {{ok:Boolean, error:String, items:Array<{path,name,isFolder}>}}
   */
  static Run(query, limit) {
    if (Trim(query, " `t`r`n") = "")
      return {ok: false, error: ES_ERR_EMPTY, items: []}

    if (!this.EnsureRunning()) {
      ; 未配置路径时给出更精确的提示 (区分「没配」与「配了但拉不起来」)
      err := (EverythingSettings.EverythingPath = "") ? ES_ERR_NO_PATH : ES_ERR_NOT_RUNNING
      return {ok: false, error: err, items: []}
    }

    provider := EverythingProviders.Create()
    res := provider.Search(query, limit)

    ; 冷启动竞态: Everything 进程已在但数据库仍在加载, es 可能返回空/失败 —— 重试一次
    if (!res.ok && (res.error = ES_ERR_NOT_RUNNING || res.error = ES_ERR_QUERY)) {
      Sleep this.RetryDelayMs
      res := provider.Search(query, limit)
    }
    return res
  }

  /** 供 UI 提示: 当前生效的通道名。 */
  static ProviderName() {
    return EverythingProviders.Create().Name
  }
}
