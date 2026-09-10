; ============================================================
; QuickSwitch.ahk —— QuickSwitch 外三层之「编排」层, 也是唯一持有对话框句柄的模块。
;
; 职责: 800 ms 定时器轮询前台窗口 -> 命中文件对话框才做一次性采集(Shell 枚举/磁盘 IO)
;       -> 组装候选 -> 以「不激活」方式显示浮层 -> 处理跳转; 会话状态机 (recent/history)。
;
; 关键纪律:
;   * 轮询每 tick 只做 WinActive/WinGetClass 级廉价判断; Shell 枚举/磁盘 IO 只在
;     「对话框新出现」这一跳变上各做一次 (设计 D6 / QS-P0-10)。
;   * 热键回调 (QuickSwitchGoto) 内零枚举、零磁盘 IO: 只用已缓存的候选; 落盘经 SetTimer 异步。
;   * 浮层显示期间 Suspend(true), 隐藏时 Suspend(false) (设计 QS-P0-09, 防 ^g 递归);
;     仅当"是我们自己挂起的"才恢复, 绝不误恢复用户的「暂停 KeyFlux」。
;   * 所有路径 try/catch, 绝不把异常抛进定时器/热键链路。
; 依赖方向: 编排 -> DialogInspector / FolderHistory / HistoryStore / FolderRanker / QuickSwitchUI。
; ============================================================
#Warn All, Off

global QSCFG := 0
global QSSTATE := 0

; 默认配置 (设计 §3.1 裁决: autoJumpOpen=true / autoJumpSave=false / 800ms / 200 条 / 8 行)。
QuickSwitchDefaultConfig() {
  return {
    collectEnabled: true,
    autoShow: true,
    autoJumpOpen: true,
    autoJumpSave: false,
    pollIntervalMs: 800,
    maxHistory: 200,
    overlayRows: 8,
    overlayRowsCompact: 4,
    excludedPrefixes: []
  }
}

; 幂等初始化: 未初始化则用默认配置初始化。
QuickSwitchEnsure() {
  global QSCFG
  if (QSCFG = 0)
    InitQuickSwitch()
}

; 初始化/重初始化。cfg 缺省字段用默认值补齐。
InitQuickSwitch(cfg := 0) {
  global QSCFG, QSSTATE
  c := QuickSwitchDefaultConfig()
  if (IsObject(cfg)) {
    ; 注意: AHK v2.0.19 普通 Object 不能直接 `for k, v in cfg` (报 "Value not enumerable"),
    ; 也不能用 `c[k] :=` 下标赋值 (普通 Object 无 __Item); 必须经 OwnProps() + 动态属性语法。
    for k in cfg.OwnProps() {
      c.%k% := cfg.%k%
    }
  }
  QSCFG := c
  QSSTATE := {
    dialogHwnd: 0,
    dialogDir: "",
    mode: "recent",
    candidates: [],
    recentCands: [],
    histCands: [],
    histStore: Map(),
    overlayVisible: false,
    autoJumped: false,
    lastExplorerFg: 0,
    suspendedByUs: false,
    inited: true
  }
  QSUISetOptions(QSCFG.overlayRows, QSCFG.overlayRowsCompact)
  QSUISetCallbacks(_QuickSwitchOnPick, QuickSwitchToggleHistory)
  SetTimer(_QuickSwitchPoll, QSCFG.pollIntervalMs)
}

; ---- 对外动作 ----

; 定时器: 判断前台是否为文件对话框; 新出现时采集一次并(可选)显示浮层。
_QuickSwitchPoll() {
  global QSCFG, QSSTATE
  try {
    h := WinActive("A")
    cls := (h != 0) ? _QSClassOf(h) : ""

    isDialog := false
    if (h != 0 && cls = "#32770")
      isDialog := DlgIsFileDialog(h)

    if (!isDialog) {
      if (QSSTATE.overlayVisible)
        _QuickSwitchHideOverlay()
      QSSTATE.dialogHwnd := 0
      ; 记录"最后一个进入前台的资源管理器目录": 仅在资源管理器前台且发生变化时,
      ; 才触发一次 Shell 枚举 (避免每 tick 枚举; 设计 D6 精神)。
      if (h != 0 && (cls = "CabinetWClass" || cls = "ExploreWClass") && QSSTATE.lastExplorerFg != h) {
        QSSTATE.lastExplorerFg := h
        HistNoteForegroundExplorer()
      }
      return
    }

    ; 对话框实例跳变: 一次性采集 (Shell 枚举 + 磁盘 IO), 绝不在每 tick 重复。
    if (QSSTATE.dialogHwnd != h) {
      QSSTATE.dialogHwnd := h
      QSSTATE.mode := "recent"
      QSSTATE.autoJumped := false
      QSSTATE.candidates := []
      QSSTATE.recentCands := []
      QSSTATE.histCands := []
      QSSTATE.histStore := Map()
      QSSTATE.dialogDir := ""
      if (QSCFG.collectEnabled) {
        QSEnsureDataDir()
        _QuickSwitchCollect(h)
        _QuickSwitchTryAutoJump(h)
      }
    }

    if (QSCFG.autoShow && !QSSTATE.overlayVisible)
      _QuickSwitchShowOverlay(QSSTATE.mode)
  } catch {
    ; 定时器内绝不抛出
  }
}

; 采集: 读对话框当前目录 + 历史 + 当前打开的资源管理器目录 -> 排序成候选。
; 只在对话框新出现时调用一次; 允许 Shell 枚举与磁盘 IO。
_QuickSwitchCollect(h) {
  global QSCFG, QSSTATE
  rd := DlgReadDir(h)
  QSSTATE.dialogDir := rd.ok ? rd.path : ""

  store := HistLoad(QSHistoryPath())
  store := HistFilterExcluded(store, QSCFG.excludedPrefixes)
  QSSTATE.histStore := store

  now := RankNowTs()
  items := Map()
  for p, e in store {
    count := (e != "" && e.HasProp("count")) ? e.count : 0
    lastAccess := (e != "" && e.HasProp("lastAccess")) ? e.lastAccess : 0
    items[p] := {count: count, lastAccess: lastAccess}
  }
  ; 当前打开的资源管理器目录以"刚访问"身份加入候选 (取用其一由 Explorer 加权决定)。
  for i, d in HistCollectExplorerDirs() {
    if (d = "" || d = QSSTATE.dialogDir)
      continue
    if (_QSIsExcluded(d, QSCFG.excludedPrefixes))
      continue
    if (!items.Has(d))
      items[d] := {count: 0, lastAccess: now}
  }

  QSSTATE.recentCands := RankRecent(items, HistLastForegroundExplorerDir(), now)
  QSSTATE.histCands := RankByTime(store, now)
  QSSTATE.candidates := QSSTATE.recentCands
}

; US-3: 打开型对话框出现时自动跳转到"最后一个前台资源管理器目录" (保存型默认关)。
; 同一对话框实例只自动跳转一次。
_QuickSwitchTryAutoJump(h) {
  global QSCFG, QSSTATE
  if (QSSTATE.autoJumped || !QSCFG.collectEnabled)
    return
  info := DlgClassify(h)
  if (!info.isFileDialog)
    return
  isOpen := (info.kind = DialogKind.FileOpen || info.kind = DialogKind.FolderPicker)
  if (isOpen && !QSCFG.autoJumpOpen)
    return
  if (!isOpen && !QSCFG.autoJumpSave)
    return

  target := HistLastForegroundExplorerDir()
  if (target = "" || target = QSSTATE.dialogDir)
    return
  if (_QSIsExcluded(target, QSCFG.excludedPrefixes))
    return
  okDir := false
  try okDir := DirExist(target) ? true : false
  if (!okDir)
    return

  res := DlgJumpTo(h, target)
  QSSTATE.autoJumped := true
  if (res.ok) {
    QSSTATE.dialogDir := target
    _QuickSwitchRecordAsync(target)
  }
}

; 显示浮层 (计算锚点 -> 渲染 -> 挂起自身热键)。
_QuickSwitchShowOverlay(mode) {
  global QSCFG, QSSTATE
  if (!QSSTATE.dialogHwnd)
    return
  dr := DlgWindowRect(QSSTATE.dialogHwnd)
  if (dr.w <= 0)
    return
  anchor := QSUIAnchorRect(dr)
  if (mode = "history")
    QSUIShow(QSSTATE.histCands, anchor, "history")
  else
    QSUIShow(QSSTATE.recentCands, anchor, "recent")
  _QuickSwitchSuspendOn()
  QSSTATE.overlayVisible := true
  QSSTATE.mode := mode
}

; 隐藏浮层 + 恢复热键。
_QuickSwitchHideOverlay() {
  global QSSTATE
  QSUIHide()
  if (QSSTATE.overlayVisible) {
    _QuickSwitchSuspendOff()
    QSSTATE.overlayVisible := false
  }
}

; 浮层内点击 🕘 进入历史模式 (普通鼠标回调, 非热键回调, 允许磁盘 IO)。
_QuickSwitchSwitchToHistory() {
  global QSCFG, QSSTATE
  if (!QSSTATE.overlayVisible || !QSSTATE.dialogHwnd)
    return
  try {
    store := HistLoad(QSHistoryPath())
    store := HistFilterExcluded(store, QSCFG.excludedPrefixes)
    QSSTATE.histStore := store
    QSSTATE.histCands := RankByTime(store, RankNowTs())
  } catch {
  }
  QSUIShow(QSSTATE.histCands, QSUIAnchorRect(DlgWindowRect(QSSTATE.dialogHwnd)), "history")
  QSSTATE.mode := "history"
}

; ---- 热键入口 (被 type9_keyflux.ahk 的薄壳 QuickSwitchGoto() 转调) ----
; 约束: 本函数内零 Shell 枚举、零磁盘 IO; 只使用已缓存候选。
QuickSwitchRun() {
  global QSCFG, QSSTATE
  QuickSwitchEnsure()
  if (!QSCFG.collectEnabled)
    return                                 ; D10: 完全禁用采集 -> 不动作、不发键

  h := QSSTATE.dialogHwnd
  if (!h) {
    ah := WinActive("A")
    if (ah != 0 && _QSClassOf(ah) = "#32770" && DlgIsFileDialog(ah))
      h := ah
    QSSTATE.dialogHwnd := h
  }
  if (!h)
    return

  target := ""
  if (QSSTATE.overlayVisible)
    target := QSUIHighlightedPath()
  if (target = "") {
    cands := QSSTATE.candidates
    if (IsObject(cands) && cands.Length >= 1 && cands[1] != "" && cands[1].HasProp("path"))
      target := cands[1].path
  }
  if (target = "")
    target := HistLastForegroundExplorerDir()   ; 零成本回退: 已缓存的 Explorer 目录
  if (target = "")
    return                                  ; 候选为空 -> 不动作
  _QuickSwitchDoJump(h, target)
}

; 对话框出现/消失 显式通知 (供上层在已知事件时调用; 轮询路径亦会自行处理)。
QuickSwitchOnDialogSeen(hwnd) {
  global QSCFG, QSSTATE
  if (hwnd = QSSTATE.dialogHwnd)
    return
  QSSTATE.dialogHwnd := hwnd
  QSSTATE.mode := "recent"
  QSSTATE.autoJumped := false
  if (QSCFG.collectEnabled) {
    QSEnsureDataDir()
    _QuickSwitchCollect(hwnd)
  }
}

QuickSwitchOnDialogGone() {
  global QSSTATE
  if (QSSTATE.overlayVisible)
    _QuickSwitchHideOverlay()
  QSSTATE.dialogHwnd := 0
}

; 切换 recent <-> history (浮层按钮回调)。
QuickSwitchToggleHistory() {
  global QSCFG, QSSTATE
  if (!QSCFG.collectEnabled)
    return
  if (!QSSTATE.overlayVisible || !QSSTATE.dialogHwnd)
    return
  if (QSSTATE.mode = "history")
    QuickSwitchBackToRecent()
  else
    _QuickSwitchSwitchToHistory()
}

QuickSwitchBackToRecent() {
  global QSSTATE
  if (!QSSTATE.overlayVisible || !QSSTATE.dialogHwnd)
    return
  QSUIShow(QSSTATE.recentCands, QSUIAnchorRect(DlgWindowRect(QSSTATE.dialogHwnd)), "recent")
  QSSTATE.mode := "recent"
}

; ---- 内部: 跳转 / 记录 / 挂起 ----

; 浮层条目点击回调 (path 由 UI 交还; 编排层此刻才做闸门判断 = TOCTOU 收敛点)。
_QuickSwitchOnPick(path) {
  global QSCFG, QSSTATE
  if (!QSCFG.collectEnabled)
    return
  h := QSSTATE.dialogHwnd
  if (!h)
    return
  res := DlgJumpTo(h, path)
  if (res.ok) {
    _QuickSwitchHideOverlay()
    _QuickSwitchRecordAsync(path)
  } else {
    if (res.reason = "dir_missing") {
      Tip(Translation().qs_cancel_jump, -1500)
      QSUIRemovePath(path)
    }
  }
}

_QuickSwitchDoJump(h, path) {
  global QSCFG, QSSTATE
  res := DlgJumpTo(h, path)
  if (res.ok) {
    _QuickSwitchHideOverlay()
    _QuickSwitchRecordAsync(path)
  } else if (res.reason = "dir_missing") {
    Tip(Translation().qs_cancel_jump, -1500)
  }
  return res
}

; 异步落盘 (绝不在热键回调内做磁盘 IO)。
_QuickSwitchRecordAsync(path) {
  SetTimer(() => _QuickSwitchRecord(path), -10)
}

_QuickSwitchRecord(path) {
  global QSCFG, QSSTATE
  if (!QSCFG.collectEnabled || path = "")
    return
  if (_QSIsExcluded(path, QSCFG.excludedPrefixes))
    return
  try {
    QSEnsureDataDir()
    store := HistLoad(QSHistoryPath())
    HistRecord(store, path, HistNowTs())
    HistSave(QSHistoryPath(), store, QSCFG.maxHistory)
    QSSTATE.histStore := store
  } catch {
  }
}

; 挂起自身热键 (防浮层显示期间 ^g 递归); 仅当是我们挂起的才恢复。
_QuickSwitchSuspendOn() {
  global QSSTATE
  if (!A_IsSuspended) {
    Suspend(true)
    QSSTATE.suspendedByUs := true
  }
}

_QuickSwitchSuspendOff() {
  global QSSTATE
  if (QSSTATE.suspendedByUs) {
    Suspend(false)
    QSSTATE.suspendedByUs := false
  }
}

; ---- 内部: 路径/检查辅助 ----

QSHistoryPath() {
  return "data\quickswitch\history.tsv"
}

QSEnsureDataDir() {
  try DirCreate("data\quickswitch")
}

_QSClassOf(h) {
  c := ""
  try c := WinGetClass("ahk_id " h)
  return c
}

_QSIsExcluded(p, prefixes) {
  if (!IsObject(prefixes) || p = "")
    return false
  for i, pre in prefixes {
    if (pre != "" && SubStr(p, 1, StrLen(pre)) = pre)
      return true
  }
  return false
}

; 自动初始化 (随 #Include 载入即生效; 使用默认配置, 后续可被 InitQuickSwitch(cfg) 覆盖)。
InitQuickSwitch()
