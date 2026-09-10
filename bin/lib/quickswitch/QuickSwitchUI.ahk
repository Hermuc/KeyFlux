; ============================================================
; QuickSwitchUI.ahk —— QuickSwitch 外三层之「浮层」层。
;
; 唯一职责: 把「候选数组 + 锚点矩形」渲染成一个不激活的浮层, 并在用户点选时
;           把选定路径通过回调交还给编排层。
;
; 契约隔离 (设计 §3 QS-P2-06):
;   * 入参 = candidates:Array<Candidate> + anchorRect:{x,y,w,h} + mode:String;
;   * 唯一出参 = 回调 onPick(path) / onToggleHistory();
;   * 本层不读对话框、不判断闸门、不发键、不持有对话框句柄 —— 因此本文件内
;     不存在任何句柄参数 (见 T3 验收 6 的 grep 断言)。
;
; 「不激活」实现 (设计 Q1): -Caption + WS_EX_NOACTIVATE(0x08000000) 且 Show("NA"),
;   显示后对话框保持前台, 用户在文件名框的键入全部落入对话框。
; 错误隔离: 所有 GUI 调用 try/catch, 失败静默。
; 依赖方向: 仅依赖 bin/lib/core 的 Translation() (由宿主 #Include 提供)。
; ============================================================
#Warn All, Off

global QSUI_GUI := 0
global QSUI_LV := 0
global QSUI_HDR := 0
global QSUI_BTN_BACK := 0
global QSUI_BTN_HIST := 0
global QSUI_MODE := "recent"
global QSUI_ONPICK := 0
global QSUI_ONTOGGLE := 0
global QSUI_ROWS := 8
global QSUI_ROWS_COMPACT := 4
global QSUI_BUILT := false

; 注入回调 (由编排层在初始化时调用; UI 不认识对话框, 只认识"选了哪个路径")。
QSUISetCallbacks(onPick, onToggle) {
  global QSUI_ONPICK, QSUI_ONTOGGLE
  QSUI_ONPICK := onPick
  QSUI_ONTOGGLE := onToggle
}

; 注入行数上限 (设计 D7: 默认 8 行; 无空间时降到 4)。
QSUISetOptions(rows, rowsCompact) {
  global QSUI_ROWS, QSUI_ROWS_COMPACT
  if (rows > 0)
    QSUI_ROWS := rows
  if (rowsCompact > 0)
    QSUI_ROWS_COMPACT := rowsCompact
}

; 首次创建 Gui + ListView; 之后复用同一窗口类 (设计 QS-P1-01: 同一 Gui 两种模式)。
QSUIEnsure() {
  global QSUI_GUI, QSUI_LV, QSUI_HDR, QSUI_BTN_BACK, QSUI_BTN_HIST, QSUI_BUILT
  if (QSUI_BUILT)
    return
  g := Gui("+AlwaysOnTop -Caption +ToolWindow +E0x08000000 -DPIScale", "QuickSwitch")
  g.MarginX := 0
  g.MarginY := 0
  try g.SetFont("s9", "Segoe UI")
  hdr := g.Add("Text", "x10 y7 w360 h20", "")
  btnBack := g.Add("Button", "x6 y4 w26 h22 Hidden", "←")
  btnHist := g.Add("Button", "x370 y4 w26 h22", "🕘")
  lv := g.Add("ListView", "x4 y30 w400 h150 -Multi", ["📂", "🕘"])
  try lv.OnEvent("Click", QSUI_OnRowClick)
  try btnHist.OnEvent("Click", QSUI_OnHistClick)
  try btnBack.OnEvent("Click", QSUI_OnBackClick)
  QSUI_GUI := g
  QSUI_LV := lv
  QSUI_HDR := hdr
  QSUI_BTN_BACK := btnBack
  QSUI_BTN_HIST := btnHist
  QSUI_BUILT := true
}

; 设置模式 (只更新标题/返回按钮可见性, 不改变窗口)。
QSUISetMode(mode) {
  global QSUI_MODE, QSUI_HDR, QSUI_BTN_BACK
  QSUI_MODE := mode
  title := (mode = "history") ? Translation().qs_overlay_history_title : Translation().qs_overlay_recent_title
  try QSUI_HDR.Value := title
  try QSUI_BTN_BACK.Visible := (mode = "history")
}

; 计算浮层应放置的矩形 (纯几何, 不含句柄)。
; 优先级: 对话框右侧 -> 下方 -> 上方 -> 兜底覆盖标题栏并降为 4 行。
; 返回 {x, y, w, h, rows, placement}。
QSUIAnchorRect(dlgRect) {
  global QSUI_ROWS, QSUI_ROWS_COMPACT
  x := dlgRect.x
  y := dlgRect.y
  w := dlgRect.w
  h := dlgRect.h
  gap := 8
  wantW := 420
  minW := 240
  rowH := 22
  chromeH := 44
  rows := QSUI_ROWS
  panelH := chromeH + rows * rowH

  ; 目标显示器工作区 (按对话框中心所在显示器)。
  cx := x + w // 2
  cy := y + h // 2
  mi := 1
  count := 1
  try count := MonitorGetCount()
  Loop count {
    ml := 0, mt := 0, mr := 0, mb := 0
    MonitorGet(A_Index, &ml, &mt, &mr, &mb)
    if (cx >= ml && cx < mr && cy >= mt && cy < mb) {
      mi := A_Index
      break
    }
  }
  wl := 0, wt := 0, wr := 0, wb := 0
  try MonitorGetWorkArea(mi, &wl, &wt, &wr, &wb)

  ; 右侧优先 (设计 D3: x=dlg.x+dlg.w+8, w=min(420, 屏宽-x))
  rx := x + w + gap
  if (rx + minW <= wr) {
    aw := Min(wantW, wr - rx - gap)
    ah := Min(panelH, wb - y - gap)
    if (ah < 80)
      ah := 80
    return {x: rx, y: y, w: aw, h: ah, rows: rows, placement: "right"}
  }

  ; 下方
  by := y + h + gap
  if (by + 80 <= wb) {
    aw := Min(w, wantW)
    ah := Min(panelH, wb - by - gap)
    if (ah < 80)
      ah := 80
    return {x: x, y: by, w: aw, h: ah, rows: rows, placement: "below"}
  }

  ; 上方
  ay := y - gap - panelH
  if (ay >= wt) {
    aw := Min(w, wantW)
    return {x: x, y: ay, w: aw, h: panelH, rows: rows, placement: "above"}
  }

  ; 兜底: 覆盖标题栏区域 (y<38, 不遮挡地址栏/列表/文件名框), 降为 4 行。
  rows := QSUI_ROWS_COMPACT
  panelH := chromeH + rows * rowH
  fy := y
  if (fy < wt)
    fy := wt
  aw := Min(w, wantW)
  ah := Min(panelH, wb - fy - gap)
  if (ah < 80)
    ah := 80
  return {x: x, y: fy, w: aw, h: ah, rows: rows, placement: "fallback"}
}

; 显示浮层。candidates:Array<Candidate>, anchorRect:{x,y,w,h}, mode:String。
QSUIShow(candidates, anchorRect, mode) {
  global QSUI_GUI, QSUI_LV, QSUI_HDR, QSUI_BTN_HIST
  QSUIEnsure()
  QSUISetMode(mode)

  w := anchorRect.w
  h := anchorRect.h
  listH := h - 36
  if (listH < 40)
    listH := 40
  try QSUI_HDR.Move(10, 7, w - 50, 20)
  try QSUI_BTN_HIST.Move(w - 30, 4, 26, 22)
  try QSUI_LV.Move(4, 30, w - 8, listH)
  try QSUI_LV.ModifyCol(1, w - 120)
  try QSUI_LV.ModifyCol(2, 104)

  try QSUI_LV.Delete()
  n := 0
  if (IsObject(candidates)) {
    for i, cd in candidates {
      p := (cd != "" && cd.HasProp("path")) ? cd.path : ""
      if (p = "")
        continue
      rel := (cd != "" && cd.HasProp("relativeTime")) ? cd.relativeTime : ""
      QSUI_LV.Add(, p, rel)
      n += 1
    }
  }
  if (n = 0) {
    QSUI_LV.Add(, Translation().qs_empty, "")
  } else {
    try QSUI_LV.Modify(1, "Select Focus")
  }

  ; 不激活显示 (NA = NoActivate; 与 WS_EX_NOACTIVATE 双保险)。
  try QSUI_GUI.Show("NA x" anchorRect.x " y" anchorRect.y " w" w " h" h)
}

; 隐藏浮层 (不销毁窗口, 供复用)。
QSUIHide() {
  global QSUI_GUI, QSUI_BUILT
  if (!QSUI_BUILT)
    return
  try QSUI_GUI.Hide()
}

; 返回当前高亮行的路径 (无高亮则空串)。
QSUIHighlightedPath() {
  global QSUI_LV, QSUI_BUILT
  if (!QSUI_BUILT)
    return ""
  row := 0
  try row := QSUI_LV.GetNext()
  if (row < 1)
    return ""
  p := ""
  try p := QSUI_LV.GetText(row, 1)
  return p
}

; 从浮层移除某失效行 (TOCTOU: 点击后才发现路径已不存在时的视觉处理)。
QSUIRemovePath(path) {
  global QSUI_LV, QSUI_BUILT
  if (!QSUI_BUILT || path = "")
    return
  n := 0
  try n := QSUI_LV.GetCount()
  row := 1
  while (row <= n) {
    t := ""
    try t := QSUI_LV.GetText(row, 1)
    if (t = path) {
      try QSUI_LV.Delete(row)
      n -= 1
    } else {
      row += 1
    }
  }
}

; ---- 控件事件 (仅与 UI 自身交互, 通过回调转交编排层) ----

QSUI_OnRowClick(lv, item, *) {
  global QSUI_ONPICK
  if (item < 1)
    return
  p := ""
  try p := lv.GetText(item, 1)
  if (p != "" && QSUI_ONPICK != 0)
    QSUI_ONPICK(p)
}

QSUI_OnHistClick(*) {
  global QSUI_ONTOGGLE
  if (QSUI_ONTOGGLE != 0)
    QSUI_ONTOGGLE()
}

QSUI_OnBackClick(*) {
  global QSUI_ONTOGGLE
  if (QSUI_ONTOGGLE != 0)
    QSUI_ONTOGGLE()
}
