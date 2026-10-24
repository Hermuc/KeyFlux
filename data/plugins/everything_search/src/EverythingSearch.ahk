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

  ; 检索词的结果条数上限 (通道层经 es.exe -n 落实; 实测 -n 在**排序之后**截断, 不破坏同序)。
  ;
  ; 🔴 为什么必须设上限 (2026-10-05 用户报障「英文检索不出列表 + 命令框有概率卡死」):
  ;   2026-10-04 的「结果不设条数上限」在英文短词上会同时引爆三条管线约束 —— 实测本机
  ;   查询 "ge" 返回 **72,870 条 / 导出 9.4MB** (中文如「是」仅 11 条, 故此前未暴露):
  ;     ① 通道层导出全部匹配 + ParseExport 逐行 FileExist (文件系统 stat × 7 万+) ⇒
  ;        引擎线程阻塞数秒, 输入钩子/轮询全部停摆 —— 用户视角即「命令框卡死」;
  ;     ② 0x406 载荷 = 条目×(标题+路径+长度头), 7 万条约 9-11MB > MAX_PAYLOAD_BYTES (4MiB,
  ;        插件与命令框两侧同值) ⇒ BuildResultsPayload 构造失败**静默拒绝** ⇒ 不出列表;
  ;     ③ 即使载荷侥幸 < 4MiB, 数万条目的编组/解码也是无谓开销。
  ;   取值 1000: 列表可滚动 (命令框按总数画滚动条), 1000 条远超人工浏览范围; 载荷 ≈
  ;   150KB, 通道导出 + 解析 (1000 次 FileExist) 均在几十毫秒级。真要找「第 1000 条以后」,
  ;   正确动作是细化检索词, 而不是滚动千行列表。
  static MAX_RESULTS := 1000

  ; ---- 会话状态缓存 (2026-10-20 首搜延迟批) ----
  ;
  ; 探活缓存: `ProcessExist` 每次都是 CreateToolhelp32Snapshot 全量快照 (本机 300+ 进程),
  ;   而「Everything 是否在跑」在数百毫秒内不会变 —— 每击键一次的全量快照纯属浪费。
  ;   TTL 取 2s: 长于单次输入节奏, 短于人工感知的进程启停间隔; 且**任何一次** es 返回
  ;   ES_ERR_NOT_RUNNING 都会立即让缓存失效 (见 Run 的重试分支) ⇒ 不存在"进程已死仍判活"。
  static RunningTTL := 2000
  static RunningCache := -1        ; -1 = 未知 / 1 = 运行中 / 0 = 未运行
  static RunningTick := 0

  ; 会话内查询计数: 只决定 `-timeout` 的取值, 不影响结果语义 (见 TimeoutFor)。
  static Queries := 0
  ; 首查宽松 (仍在等 DB 加载), 后续收紧 —— 稳态下 es 根本不会等, 这两个值只在
  ; "DB 正在加载/Everything 正在重启" 时决定我们最多陪它等多久 (旧值 4000 恒定)。
  static FirstQueryTimeoutMs := 3000
  static SteadyQueryTimeoutMs := 1000

  ; 最近一次预热异常 (仅内存留痕, 不写日志文件 —— 见 EverythingPrewarm 文件头纪律②)
  static WarmError := ""

  ; 最近一次真实查询的时刻 (A_TickCount); 0 = 本进程内还没查过
  static LastQueryTick := 0
  ; 预热只在「冷」的时候做 (见 WarmUp): 距上次查询 < 5min 说明 es.exe 刚跑过、
  ; 镜像/安全软件缓存都热着 —— 此时再热身纯属白 spawn 一个进程 (实测零收益, 见 README §8)。
  static ColdIdleMs := 300000

  ; 静默启动开关: 官方文档 "Run Everything in the background without showing any search windows"。
  ; 必须带它 —— 裸跑 everything.exe 会弹出主窗口并抢走前台焦点, 打断命令框里正在进行的输入。
  ; 见 README 「未启动时静默拉起」与 docs/CONTRACTS 的插件章节。
  static StartupSwitch := "-startup"
  ; 兜底: -startup 在某些配置下仍可能出窗 (如启动时 force-show 类设置), 故启动后再主动隐藏一次。
  static HideCheckMs := 1200       ; 给主窗口留出创建时间再检查/隐藏
  static HidePollMs := 250         ; 隐藏检查的轮询步长

  /**
   * Everything 进程是否在运行 (不依赖 IPC 窗口, 避免版本差异)。
   * @param force 忽略缓存强制重探 (拉起进程后 / 收到"未运行"错误后必须传 true)
   */
  static IsRunning(force := false) {
    if (!force && this.RunningCache != -1
        && (A_TickCount - this.RunningTick) < this.RunningTTL)
      return this.RunningCache = 1
    running := ProcessExist(this.ProcessName) ? true : false
    this.RunningCache := running ? 1 : 0
    this.RunningTick := A_TickCount
    return running
  }

  /** 新会话开始: 复位会话内计数 (只有 `-timeout` 分级读它)。 */
  static ResetSession() {
    this.Queries := 0
  }

  /**
   * 预热 (2026-10-20 首搜延迟批; 由 EverythingPrewarm.Kick 在会话开始/插件入口触发)。
   *
   * 复用生产同一条查询路径, 只把结果丢弃 —— 于是「预热返回 true」等价于
   * 「首查要付的三笔固定成本已经付过」:
   *   ① es.exe 落点解析 (EverythingProviders.ResolveEs, 含 4 次 FileExist);
   *   ② es.exe 进程镜像 + 首次执行 (实测 121~135ms, 其中查询本体仅 5~15ms);
   *   ③ Everything 数据库加载竞态 (由这里的 -timeout 承担, 而不是首查)。
   *
   * 🔴 只在 Everything **已在运行** 时预热: 未运行时不替你拉起进程 (绿色纪律①, 见
   *    EverythingPrewarm 文件头)。降级通道 (gui-launch) 也不预热 —— 那次调用会弹出
   *    Everything 主窗口, 是"预热"最不该产生的副作用。
   * 🔴 只在**冷**的时候预热 (2026-10-20 实测校准): es.exe 进程启动的固定成本 ~155ms 在
   *    系统文件缓存已热时并不会因为"提前跑一次"而变短 —— 本机 A/B (各 3 轮) 显示
   *    nowarm 235/234/235ms vs warm 235/234/219ms, **无差异**; 收益只在冷路径
   *    (镜像/安全软件首扫, 实测 209~229ms → 126~136ms) 与"Everything 刚被拉起、DB 仍在
   *    加载"的竞态上。故最近 5 分钟内有查询则跳过预热, 把白跑的 spawn 留在门外。
   *
   * @returns {Boolean} 是否真的热身了 es.exe 通道
   */
  static WarmUp() {
    if (this.LastQueryTick != 0 && (A_TickCount - this.LastQueryTick) < this.ColdIdleMs)
      return false
    if (!this.IsRunning())
      return false
    es := EverythingProviders.ResolveEs()
    if (es = "")
      return false
    ; 🔴 预热必须**直连 es-cli** (2026-10-24 修隐藏 bug): 首选通道已是 0x40C 查询桥,
    ;   而它会使命令框**渲染**查询结果 —— 预热在会话开始时跑, 经 Create() 走 IPC 会让
    ;   刚打开的空命令框闪现 "KeyFlux-Prewarm" 结果列表。IPC 通道零冷启动成本 (管道
    ;   连接亚毫秒) 无需预热; 预热的对象本就是**回落通道** es.exe 的固定启动成本。
    try EverythingEsProvider(es).Search("KeyFlux-Prewarm", 1, this.FirstQueryTimeoutMs)
    catch as e
      this.WarmError := e.Message   ; 仅内存留痕 (不写日志文件)
    return true
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
    ; 🔴 这里必须 force 重探: 缓存里刚写下的还是"未运行"(0), 走缓存会一直等到超时。
    waited := 0
    while (waited < this.LaunchWaitMs) {
      Sleep this.LaunchPollMs
      waited += this.LaunchPollMs
      if (this.IsRunning(true)) {
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
   * 执行一次搜索 (同步; 调用方在命令框输入期, 每次追加/退格都会重跑本方法)。
   * 通道优先级 (2026-10-24 T0'): 0x40C 查询桥 (命令框进程内查 Everything, ~11.5ms)
   *   → 失败自动回落 es-cli (spawn, ~115ms) → gui-launch。会话/打开/导航逻辑不变。
   * @param query 检索词
   * @returns {{ok:Boolean, error:String, items:Array<{path,name,isFolder}>}}
   */
  static Run(query) {
    if (Trim(query, " `t`r`n") = "")
      return {ok: false, error: ES_ERR_EMPTY, items: []}

    if (!this.EnsureRunning()) {
      ; 未配置路径时给出更精确的提示 (区分「没配」与「配了但拉不起来」)
      err := (EverythingSettings.EverythingPath = "") ? ES_ERR_NO_PATH : ES_ERR_NOT_RUNNING
      return {ok: false, error: err, items: []}
    }

    ; -timeout 分级 (2026-10-20): 首查宽松 (可能正等 DB 加载), 之后收紧 —— 稳态下 es
    ; 不会真等, 该值只在"DB 加载中/Everything 重启中"决定最多陪它等多久 (旧值恒 4000)。
    timeout := (this.Queries = 0) ? this.FirstQueryTimeoutMs : this.SteadyQueryTimeoutMs
    this.Queries += 1

    active := EverythingProviders.Create()
    res := active.Search(query, this.MAX_RESULTS, timeout)

    ; 0x40C 桥失败 (框未更新/Everything 未运行/超时) ⇒ 回落旧通道, 行为与升级前一致。
    if (!res.ok && res.error = ES_ERR_IPC) {
      es := EverythingProviders.ResolveEs()
      active := (es != "") ? EverythingEsProvider(es)
                           : EverythingGuiProvider(EverythingSettings.EverythingPath)
      res := active.Search(query, this.MAX_RESULTS, timeout)
    }

    ; 冷启动竞态: Everything 进程已在但数据库仍在加载, es 可能返回空/失败 —— 重试一次。
    ; 同时让探活缓存失效: 报"未运行"说明缓存(可能刚被预热写成"运行中")已过时。
    if (!res.ok && (res.error = ES_ERR_NOT_RUNNING || res.error = ES_ERR_QUERY)) {
      this.RunningCache := -1
      Sleep this.RetryDelayMs
      res := active.Search(query, this.MAX_RESULTS, this.SteadyQueryTimeoutMs)
    }
    this.LastQueryTick := A_TickCount   ; 供 WarmUp 判定"是否已经热着" (见其注释)
    return res
  }

  /** 供 UI 提示: 当前生效的通道名。 */
  static ProviderName() {
    return EverythingProviders.Create().Name
  }
}
