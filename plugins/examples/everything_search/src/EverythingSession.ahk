; ============================================================
; EverythingSession —— 命令框会话状态机 + 控制器 (CommandInputHooks provider)。
;
; 时序 (一次完整的命令框会话):
;   引擎 EnterCapslockAbbr
;     -> CommandInputHooks.BeginSession()        记前台窗口 + 通知本控制器开新会话
;     -> StartInputHook(Suspend true + InputHook.Wait)
;          OnChar/OnKey 逐个进来 -> 本文件
;          (命令框鼠标交互 -> OnBoxNotify, 经 EverythingResults 的 0x409 回推)
;     -> CommandInputHooks.EndSession()          收起结果列表
;
; 状态机:
;   Armed  (会话开始, 未输入任何字符)  --触发键-->  Active (检索中)
;   Armed  --普通字符--> PassThrough   (交给引擎原有语义: 投递字符 + 缩写模糊匹配)
;   Active --字符/退格--> 检索词变化 -> 重查
;   Active --↑↓--> 移动高亮; --回车--> 打开并结束; --Esc--> 引擎 EndKey, 会话结束顺带收列表
;
; 🔴 2026-10-04 列表归属变更 (用户需求「列表必须是命令框本体的向下延伸」): 渲染已整体
;   移交命令框 (command-input/ 的 Rust 自绘), 本层只剩**数据 + 索引**: 出参一律经
;   EverythingResults (→ EverythingHost 端口 → 0x406/0x407/0x408), 入参只剩 0x409 回推。
;   旧的自建 AHK Gui + ListView 浮层 (EverythingDropdown) 已删除。
;
; 三个实测得出的硬约束 (探针 bin/lib 之外的 evidence, 见 README):
;   1. 活动 InputHook 会看到脚本自身 Send 的按键 —— 取选中文字要发 Ctrl+C, 必须加捕获锁,
;      否则注入的 'c' 会进命令框并污染检索词。
;   2. 回车在 OnChar 里以换行字符出现 (Ord=10) —— 必须过滤控制字符, 否则检索词被塞进换行。
;   3. KeyOpt("{Up}/Down/Enter","N") 对非 EndKey 也产生 OnKeyDown 通知 (实测通过),
;      故方向键导航不需要把它们塞进 EndKeys。
;
; 🔴 2026-09-30 补第 3 条的**代价** (用户报障「连点/按住 Enter 后成批弹出『文档 - 文件资源
;   管理器』窗口」): N = 仅保留 OnKeyDown 通知、**不吞键** ⇒ 长按/连击 Enter 会产生多次
;   OnKeyDown ⇒ 本会话的 VK_RETURN 分支被重入多次 ⇒ 多次 explorer.exe; 当高亮项的 path
;   为空或已失效时 `explorer.exe ""` 会打开资源管理器的默认「文档」目录 ⇒ 成批同标题窗口。
;   修法 = OpenSelected 的守卫链 (会话状态 + 重复通知去抖 + 路径校验), 见该方法注释;
;   回归探针: tests/open_guard_probe.ahk。
; ============================================================

; ============================================================
; EverythingExplorerRunner —— explorer 启动的唯一调用缝 (供探针替换)。
;
; 为什么要抽这一层: AHK **无法 monkey-patch 内置函数**, 若 OpenSelected 直接调 Run,
;   探针就只能真去启动 explorer.exe (真弹窗, 且无法计数/断言)。
; 口径:
;   * 生产 (Impl = 0): 走内置 Run, 行为与历史完全一致;
;   * 探针: EverythingExplorerRunner.Impl := (cmd) => ... 记录命令串并计数。
; 为什么用类静态字段而不是文件顶层 `global ExplorerRunner := Run`:
;   ① AHK v2 的 `global` 关键字只允许出现在函数内, 顶层 `global X := Run` 实测为**加载期
;      错误** (脚本起不来), 等价写法是裸赋值 `X := Run`;
;   ② 裸赋值的执行位置取决于本文件被 #Include 后落在 auto-execute 段的哪一段 (插件入口是
;      「生成期 #Include 进 bin/KeyFlux.ahk」的), 而类静态初值在脚本加载期求值,
;      与本文件被包含的位置无关。
; ============================================================

class EverythingExplorerRunner {
  static Impl := 0        ; 0 = 未替换 (用内置 Run); 其余 = 可调用对象 (函数引用/lambda)

  /** 启动/定位资源管理器。@returns 替换实现的返回值 (默认路径下即 Run 的 pid)。 */
  static Launch(cmd) {
    ; 🔴 必须先落到局部变量: `Class.Field(...)` 在 AHK v2 里按**方法调用**解析, 字段值是
    ;   函数对象时会报 "Too many parameters passed to function." (实测 2.0.19); 而
    ;   「变量持有函数对象 + 变量(...)」是合法调用。
    impl := EverythingExplorerRunner.Impl
    if (IsObject(impl))   ; Func/lambda 都是对象; 默认值 0 表示未替换
      return impl.Call(cmd)
    return Run(cmd)
  }
}

class EverythingController {
  static SESSION_TYPE := "EverythingSession"

  __New(api) {
    this.api := api
    this.session := 0
  }

  ; ---- CommandInputHooks provider 接口 ----

  OnSessionBegin() {
    ; 设置热重载: 设置面板点卡片改完设置后**不需要重启引擎** —— 每次命令框会话开始
    ; 重读 plugin-settings.json; 只有值真的变了才让通道探测缓存失效 (见 Load 的注释)。
    if (EverythingSettings.Load(this.api))
      EverythingProviders.Reset()
    ; 上一会话残留 (正常流程 EndSession 已 Close; 兜底防定时器/列表跨会话泄漏)
    if (this.session != 0)
      this.session.Close()
    EverythingResults.Hide()
    this.session := EverythingSession(this.api)
  }

  OnSessionEnd() {
    if (this.session != 0)
      this.session.Close()
    this.session := 0
  }

  OnChar(ih, char, scope) {
    if (this.session = 0)
      this.session := EverythingSession(this.api)
    return this.session.OnChar(ih, char, scope)
  }

  OnKey(ih, vk, sc, scope) {
    if (this.session = 0)
      return false
    return this.session.OnKey(ih, vk, sc, scope)
  }

  /**
   * 命令框回推 (0x409) 的结果行交互 —— 取代旧的浮层回调 (EverythingDropdown.SetCallback)。
   * @param row 1 基行号
   * @param kind 1 = 行被点选 (打开), 2 = 高亮变化 (悬停/滚轮)
   */
  OnBoxNotify(row, kind) {
    if (this.session = 0)
      return
    this.session.OnBoxNotify(row, kind)
  }
}

class EverythingSession {
  static VK_BACK := 0x08
  static VK_UP := 0x26
  static VK_DOWN := 0x28
  static VK_RETURN := 0x0D

  ; 重复通知去抖窗口 (ms)。Enter 是 N 态: 一次长按/连击会产生**多次** OnKeyDown 通知,
  ; 同一批通知之间的间隔实测远小于人工可感知的连击间隔, 400ms 只吃掉「同一按键的重复
  ; 通知」, 不阻断用户真的再按一次回车。
  static DEBOUNCE_MS := 400

  ; 查询输入面文本轮询间隔 (ms): Edit 是检索词的唯一真源 (IME 上屏/字母/退格原生发生
  ; 在控件里), 轮询差分 → 检索词更新 → 重查。40ms 远小于人工输入节奏。
  static QUERY_SYNC_MS := 40

  ; 会话状态
  chars := 0              ; 本次会话已收到的普通字符数 (0 = 触发键仍处「前置」位置)
  active := false         ; 是否已触发 (检索中)
  closed := false
  capturing := false      ; 取选中文字期间为 true: 吞掉自身 Send 注入的按键
  lastOpenTick := 0       ; 上次**成功**启动 explorer 的 A_TickCount (去抖用; 0 = 尚未打开)
  query := ""
  items := []
  index := 0

  __New(api) {
    this.api := api
    ; 结果列表的鼠标交互不再有「回调注入」这一步: 命令框把命中行经 0x409 回推给控制器
    ; (EverythingResultsNotifyForward → EverythingController.OnBoxNotify → 本类 OnBoxNotify)。
    ; GUI 降级通道的「每会话只弹一次」额度在此复位 (见 EverythingGuiProvider 注释:
    ; 2026-10-03 用户报障 —— 降级态下每击键都弹 Everything 主窗口)
    EverythingGuiProvider.AllowLaunch := true
  }

  ; ---- 输入 ----

  OnChar(ih, char, scope) {
    if (this.closed)
      return false

    ; 捕获锁: 取选中文字时的 Ctrl+C 会被输入钩子看到 (实测), 期间一律吞掉
    if (this.capturing)
      return true

    ; 控制字符 (回车/制表/换行) 不是检索词的一部分; 回车由 OnKey 处理
    c := SubStr(char, -1)
    if (c = "" || (Ord(c) < 32 && c != " "))
      return false

    if (!this.active) {
      ; 前置键语义: 必须是本次会话输入的第一个字符
      if (this.chars > 0)
        return false
      this.chars += 1
      if (c != EverythingSettings.TriggerKey)
        return false
      this.active := true
      ; 🔴 搜索模式回**命令框本体** (2026-10-04 定版): 命令框已是我们的 Rust 代码,
      ;   0x404 让它摘除 NOACTIVATE 并自取前台+焦点 ⇒ IME 组合窗跟随命令框,
      ;   上屏中文经 WM_CHAR 进入框内文本缓冲, 引擎轮询 WM_GETTEXT 读回 = 检索词
      ;   (此前 QueryEdit 覆盖输入面方案退役 —— 不再"在另一个新的框里输入")。
      ;   透传 (UnlockForSearch) 仍必须: 物理键经 InputHook V 透传到达焦点窗口 (命令框)。
      EverythingHost.UnlockForSearch(ih)
      EverythingHost.BoxActivateForSearch()
      EverythingHost.BoxForeground()
      EverythingResults.Hide()   ; 清上一会话残留列表 (防"变大"残留)
      EverythingHost.ShowBadge() ; 搜索徽标 (0x40A): 查询区右侧放大镜, 搜索模式的可视标识
      this.SeedFromSelection()
      SetTimer(ObjBindMethod(this, "_SyncQuery"), EverythingSession.QUERY_SYNC_MS)
      this.Refresh()
      return true     ; 消费触发键本身 (空间不投递到命令框)
    }

    ; 已激活: 输入 (字母/中文上屏/退格) 由命令框原生持有, 检索词经 _SyncQuery 轮询
    ;   WM_GETTEXT 同步 —— OnChar 只负责消费 (防止引擎做缩写模糊匹配/双份回显)。
    return true
  }

  OnKey(ih, vk, sc, scope) {
    if (this.closed || !this.active)
      return false
    if (this.capturing)
      return true

    if (vk = EverythingSession.VK_BACK) {
      ; 退格由命令框原生处理 (透传直达, ch==8 → 框内 pop_back); 检索词经
      ; _SyncQuery 轮询同步 —— 这里只消费, 不再截断 query / 投递退格 (会双删)。
      return true
    }
    if (vk = EverythingSession.VK_UP) {
      this.Move(-1)
      return true
    }
    if (vk = EverythingSession.VK_DOWN) {
      this.Move(1)
      return true
    }
    if (vk = EverythingSession.VK_RETURN) {
      ; 🔴 IME 组合态判定 (0x405, 确定性非启发式): 组合中 = 该回车是「上屏提交」
      ;   (物理键经透传直达 IME, 提交文本随后进框) —— 消费, 不打开结果。
      if (EverythingHost.BoxQueryComposing())
        return true
      ; 「打开一次 → 收会话」只在**成功**时收尾 (2026-09-30 收尾补丁): 旧写法无条件 Close +
      ; ih.Stop, 于是失败路径 (空/失效路径) 出的那行提示会被紧随其后的 Hide() 立刻收起 ——
      ; 用户视角是「按回车毫无反应」(OpenSelected 的守卫链见其注释)。
      ; 失败时保留列表与输入钩子: 提示留在屏上, 用户可以继续改检索词或按 Esc 退出。
      ; 成功时 Close 掉的会话会以 closed/active 双保险吃掉同一批重复通知 —— 那正是
      ; 「成批弹出资源管理器窗口」的第一道闸; 第二道是 OpenSelected 内的 400ms 去抖。
      if (this.OpenSelected()) {
        this.Close()
        try ih.Stop()    ; 结束输入 -> 引擎走 HIDE 分支隐藏命令框
      }
      return true
    }
    return false
  }

  ; ---- 检索 ----

  /**
   * 用当前选中文字做初始检索词:
   *   - 选中文字 (type=text) -> 原文;
   *   - 选中文件 (type=file) -> 首个文件名 (资源管理器里选中文件时, 用户意图通常是「找同名/同类」);
   *   - 未取到 -> 空 (初始态: 命令框保持原样, 不出列表)。
   * 取文字前先把前台切回会话开始时的窗口 (命令框可能抢了前台, 否则 Ctrl+C 发不到目标程序)。
   *
   * 🔴 取完必须把焦点还给命令框 (2026-09-19 v4.1 透传补丁): ActivateBackend 把前台切到了
   * 原窗口 —— 历史形态无影响 (显示靠投递 WM_CHAR, 与焦点无关, 且当时命令框本就不在
   * 前台, ActivateBackend 实际是 no-op); 透传模式下物理键按「焦点窗口」路由, 焦点不还原
   * 则后续字符全部漏进原窗口 (命令框看不见, 原窗口还会被打字污染)。用户实测: 按空格
   * 触发后能搜索但命令框看不见字符, 即此因。
   */
  SeedFromSelection() {
    this.capturing := true
    try {
      EverythingHost.ActivateBackend()
      sel := EverythingHost.GetSelection(true)
      if (sel.type = "file")
        this.query := this._FirstBaseName(sel.content)
      else
        this.query := sel.content
    } catch {
      this.query := ""
    }
    this.capturing := false
    ; 焦点还原: 搜索模式下命令框自取前台+焦点 (0x404), 种子已在框内
    EverythingHost.BoxActivateForSearch()
  }

  /** 按当前检索词刷新结果列表。空词 = 初始态: **不出任何列表** (2026-10-03 需求:
   *  「尚未输入文字时命令框保持初始样式」), 此前会弹一行引导提示框 —— 那正是
   *  「框下另挂一个独立框」观感的来源之一。 */
  Refresh() {
    if (Trim(this.query, " `t`r`n") = "") {
      this.items := []
      this.index := 0
      EverythingResults.Hide()
      return
    }

    res := EverythingSearch.Run(this.query)
    if (!res.ok) {
      ; 无结果/错误 → 收起列表 (不出提示 — 提示会盖住命令框文字区且残留)
      this.items := []
      this.index := 0
      EverythingResults.Hide()
      return
    }
    this.items := res.items
    if (res.items.Length = 0) {
      this.index := 0
      EverythingResults.Hide()
      return
    }
    this.index := 1
    EverythingResults.Show(res.items, this.index)
  }

  /** 移动高亮 (环形)。 */
  Move(delta) {
    n := this.items.Length
    if (n = 0)
      return
    i := this.index + delta
    if (i < 1)
      i := n
    if (i > n)
      i := 1
    this.index := i
    EverythingResults.Select(i)
  }

  /**
   * 在资源管理器中打开/定位当前高亮项。
   *
   * 🔴 守卫链 (2026-09-30: 修「连点/按住 Enter 成批弹出『文档 - 文件资源管理器』窗口」)。
   *   症状根因: Enter 在引擎模板里是 `KeyOpt("{Enter}","N")` —— 只保留 OnKeyDown 通知、
   *   不吞键 (templates/keyflux.tmpl:134 → bin/templates/), 于是长按/连击会产生**多次**通知; 旧实现没有
   *   任何守卫, 每次都 `Run('explorer.exe ...')`; 而高亮项 path 为空或已失效时
   *   `explorer.exe ""` 会打开资源管理器的默认「文档」目录 ⇒ 成批同标题窗口。
   *   四道闸按顺序:
   *     ① 会话状态: 未激活 或 已关闭 => 直接拒绝 (关闭后到达的重复通知走这里);
   *     ② 高亮有效性: index 越界 => 拒绝 (空结果/无高亮时不打开任何东西);
   *     ③ 重复通知去抖: 距上次成功打开 < DEBOUNCE_MS 的通知一律丢弃;
   *     ④ 路径校验: 空路径 / 路径已不存在 => **不调用 explorer**, 只出一行提示
   *        (err_item_missing; 调用方在失败时保留列表, 故这行提示是可见的)。
   *   通过四道闸才经唯一调用缝 EverythingExplorerRunner.Launch 启动 explorer, 且参数用
   *   规范化后的**绝对路径**。
   *
   * @returns {Boolean} 是否真的启动了 explorer (false = 被守卫拦下)
   */
  OpenSelected() {
    if (!this.active || this.closed)
      return false
    if (this.index < 1 || this.index > this.items.Length)
      return false
    if (A_TickCount - this.lastOpenTick < EverythingSession.DEBOUNCE_MS)
      return false

    it := this.items[this.index]
    path := this._AbsPath(it.path)
    ; 空路径 = 最危险的一种 (explorer.exe "" 就是「文档」窗口的来源), 与「路径已失效」
    ; 合并为同一条出口: 不启动 explorer, 出一行提示 (err_item_missing), 返回 false。
    ; 失败不再静默: Enter 分支只在成功时 Close/ih.Stop, 故这行提示会**留在屏上**。
    if (path = "" || (!FileExist(path) && !DirExist(path))) {
      EverythingResults.ShowHint(EverythingMessages.T("err_item_missing"))
      return false
    }

    if (it.isFolder)
      cmd := 'explorer.exe "' path '"'
    else
      cmd := 'explorer.exe /select,"' path '"'
    try {
      EverythingExplorerRunner.Launch(cmd)
      this.lastOpenTick := A_TickCount     ; 只在成功启动后更新去抖时间戳
      return true
    } catch {
      return false
    }
  }

  /**
   * 命令框回推 (0x409) 的结果行交互。取代旧的浮层回调 OnPick(path) —— 旧回调靠
   *   「path 字符串反查行号」, 新协议直接携带**行号** (命令框才是唯一知道行几何的一方)。
   *
   *   kind = 2 (悬停/滚轮): 只把本层 index 对齐到用户看到的那行 (命令框已自行重绘高亮,
   *     不回推 0x407 —— 那会形成 0x409→0x407→重绘 的回声环且无收益), 保证随后的回车
   *     打开的是屏上高亮项;
   *   kind = 1 (点选): 走 OpenSelected 的守卫链, 与 Enter 分支同款「仅成功才收尾」——
   *     失败 (路径空/已失效/被去抖) 时保留列表, 让 OpenSelected 出的提示留在屏上
   *     (2026-10-01 对齐, 见 OpenSelected 注释)。
   *
   * @param row 1 基行号; @param kind 1 = 点选 (打开), 2 = 高亮变化
   */
  OnBoxNotify(row, kind) {
    if (this.closed || !this.active)
      return
    if (row < 1 || row > this.items.Length)
      return
    this.index := row
    if (kind != 1)
      return
    if (this.OpenSelected())
      this.Close()
  }

  /** 收尾: 收起结果列表 + 隐藏搜索徽标 (幂等 —— 已关闭时直接返回, 不重复发收尾消息)。
   *  同时撤销 active —— 关闭后到达的重复 Enter 通知必须被拒。 */
  Close() {
    if (this.closed)
      return
    this.closed := true
    this.active := false
    try SetTimer(ObjBindMethod(this, "_SyncQuery"), 0)
    EverythingResults.Hide()
    EverythingHost.HideBadge()   ; 0x40B: 徽标与搜索模式同生命周期 (命令框侧另有会话兜底)
  }

  ; ---- 查询轮询 (命令框内文本 = 检索词唯一真源, 经 WM_GETTEXT 读回) ----

  _SyncQuery() {
    if (this.closed || !this.active) {
      SetTimer(ObjBindMethod(this, "_SyncQuery"), 0)
      return
    }
    t := EverythingHost.BoxGetText()
    if (t = this.query)
      return
    this.query := t
    this.Refresh()
  }

  ; ---- 内部 ----

  /**
   * 路径规范化: 去首尾空白 -> 正斜杠转反斜杠 -> 相对路径补 A_WorkingDir -> 去尾部反斜杠。
   * 前三步保证喂给 explorer 的是**绝对路径**; 最后一步是引号安全 —— 命令行里 `"C:\dir\"`
   * 的 `\"` 会被解析成转义引号, 参数被截断 (explorer 于是又回落到默认目录)。
   * @returns {String} 绝对路径; 入参为空时返回空串 (调用方据此拒绝启动)
   */
  _AbsPath(p) {
    p := Trim(p, " `t`r`n")
    if (p = "")
      return ""
    p := StrReplace(p, "/", "\")
    if (!RegExMatch(p, "^(?:[A-Za-z]:\\|\\\\)"))
      p := RTrim(A_WorkingDir, "\") "\" p
    if (StrLen(p) > 3)          ; 保护盘根 "C:\" 不被削成 "C:"
      p := RTrim(p, "\")
    return p
  }

  _ErrorKey(err) {
    if (err = ES_ERR_EMPTY)
      return "hint_no_selection"
    if (err = ES_ERR_NO_PATH)
      return "err_no_path"
    if (err = ES_ERR_NOT_RUNNING)
      return "err_not_running"
    if (err = ES_ERR_NOT_FOUND)
      return "err_es_not_found"
    if (err = ES_ERR_NO_ES_LAUNCHED)
      return "err_launched_gui"
    if (err = ES_ERR_LAUNCH_FAILED)
      return "err_not_running"
    return "err_query"
  }

  _FirstBaseName(paths) {
    line := ""
    for l in StrSplit(StrReplace(paths, "`r`n", "`n"), "`n") {
      if (Trim(l, " `t") != "") {
        line := Trim(l, " `t")
        break
      }
    }
    i := InStr(line, "\", , -1)
    n := (i > 0) ? SubStr(line, i + 1) : line
    SplitPath(n, , , &ext)
    return (ext != "") ? SubStr(n, 1, StrLen(n) - StrLen(ext) - 1) : n
  }
}
