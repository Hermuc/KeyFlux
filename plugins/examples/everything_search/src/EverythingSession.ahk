; ============================================================
; EverythingSession —— 命令框会话状态机 + 控制器 (CommandInputHooks provider)。
;
; 时序 (一次完整的命令框会话):
;   引擎 EnterCapslockAbbr
;     -> CommandInputHooks.BeginSession()        记前台窗口 + 通知本控制器开新会话
;     -> StartInputHook(Suspend true + InputHook.Wait)
;          OnChar/OnKey 逐个进来 -> 本文件
;     -> CommandInputHooks.EndSession()          收起浮层
;
; 状态机:
;   Armed  (会话开始, 未输入任何字符)  --触发键-->  Active (检索中)
;   Armed  --普通字符--> PassThrough   (交给引擎原有语义: 投递字符 + 缩写模糊匹配)
;   Active --字符/退格--> 检索词变化 -> 重查
;   Active --↑↓--> 移动高亮; --回车--> 打开并结束; --Esc--> 引擎 EndKey, 会话结束顺带收浮层
;
; 三个实测得出的硬约束 (探针 bin/lib 之外的 evidence, 见 README):
;   1. 活动 InputHook 会看到脚本自身 Send 的按键 —— 取选中文字要发 Ctrl+C, 必须加捕获锁,
;      否则注入的 'c' 会进命令框并污染检索词。
;   2. 回车在 OnChar 里以换行字符出现 (Ord=10) —— 必须过滤控制字符, 否则检索词被塞进换行。
;   3. KeyOpt("{Up}/Down/Enter","N") 对非 EndKey 也产生 OnKeyDown 通知 (实测通过),
;      故方向键导航不需要把它们塞进 EndKeys。
; ============================================================

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
}

class EverythingSession {
  static VK_BACK := 0x08
  static VK_UP := 0x26
  static VK_DOWN := 0x28
  static VK_RETURN := 0x0D

  ; 会话状态
  chars := 0              ; 本次会话已收到的普通字符数 (0 = 触发键仍处「前置」位置)
  active := false         ; 是否已触发 (检索中)
  closed := false
  capturing := false      ; 取选中文字期间为 true: 吞掉自身 Send 注入的按键
  query := ""
  items := []
  index := 0

  __New(api) {
    this.api := api
    EverythingDropdown.SetCallback(ObjBindMethod(this, "OnPick"))
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
      this.SeedFromSelection()
      this.Refresh()
      return true     ; 消费触发键本身 (不投递到命令框)
    }

    ; 已激活: 继续输入 = 追加检索词; 同时投递字符让命令框显示 (视觉回显)
    this.query .= c
    CommandDisplay.EchoChar(ih, c)
    this.Refresh()
    return true
  }

  OnKey(ih, vk, sc, scope) {
    if (this.closed || !this.active)
      return false
    if (this.capturing)
      return true

    if (vk = EverythingSession.VK_BACK) {
      if (this.query != "")
        this.query := SubStr(this.query, 1, -1)
      CommandDisplay.EchoBackspace(ih, vk, sc)   ; 命令框视觉同步退格
      this.Refresh()
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
      this.OpenSelected()
      this.Close()
      try ih.Stop()      ; 结束输入 -> 引擎走 HIDE 分支隐藏命令框
      return true
    }
    return false
  }

  ; ---- 检索 ----

  /**
   * 用当前选中文字做初始检索词:
   *   - 选中文字 (type=text) -> 原文;
   *   - 选中文件 (type=file) -> 首个文件名 (资源管理器里选中文件时, 用户意图通常是「找同名/同类」);
   *   - 未取到 -> 空 (浮层提示继续输入)。
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
      CommandInputHooks.ActivateBackend()
      sel := SelectionContext.Get(true)
      if (sel.type = "file")
        this.query := this._FirstBaseName(sel.content)
      else
        this.query := sel.content
    } catch {
      this.query := ""
    }
    this.capturing := false
    ; 焦点还原 (返回值忽略: 激活失败只损失显示, 搜索路径不依赖焦点)
    CommandDisplay.ActivateCommandWindow()
  }

  /** 按当前检索词刷新浮层 (空词/失败/无结果分别给引导文案)。 */
  Refresh() {
    if (Trim(this.query, " `t`r`n") = "") {
      this.items := []
      this.index := 0
      EverythingDropdown.ShowHint(EverythingMessages.T("hint_no_selection"))
      return
    }

    res := EverythingSearch.Run(this.query, EverythingSettings.Limit)
    if (!res.ok) {
      this.items := []
      this.index := 0
      EverythingDropdown.ShowHint(EverythingMessages.T(this._ErrorKey(res.error)))
      return
    }
    this.items := res.items
    if (res.items.Length = 0) {
      this.index := 0
      EverythingDropdown.ShowHint(EverythingMessages.T("hint_empty"))
      return
    }
    this.index := 1
    EverythingDropdown.Show(res.items, this.index)
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
    EverythingDropdown.Select(i)
  }

  /** 在资源管理器中打开/定位当前高亮项。 */
  OpenSelected() {
    if (this.index < 1 || this.index > this.items.Length)
      return false
    it := this.items[this.index]
    try {
      if (it.isFolder)
        Run('explorer.exe "' it.path '"')
      else
        Run('explorer.exe /select,"' it.path '"')
      return true
    } catch {
      return false
    }
  }

  /** 鼠标点选浮层某行 (回调来自 EverythingDropdown)。 */
  OnPick(path) {
    for i, it in this.items {
      if (it.path = path) {
        this.index := i
        this.OpenSelected()
        this.Close()
        return
      }
    }
  }

  /** 收尾: 隐藏浮层 (可重复调用)。 */
  Close() {
    this.closed := true
    EverythingDropdown.Hide()
  }

  ; ---- 内部 ----

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
