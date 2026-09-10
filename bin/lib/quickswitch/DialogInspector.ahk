; ============================================================
; DialogInspector.ahk —— QuickSwitch 外三层之「唯一 Win32/窗口」层。
;
; 唯一职责: 判别 #32770 是否是文件对话框、读取对话框当前目录、跳转到指定目录。
; 三条安全红线全部收口在本层, 便于静态审查:
;   ① 全代码路径零 Esc (浮层关闭绝不依赖 Esc, 避免误关用户对话框);
;   ② 任何发键前必须过双闸门 DirExist(path) + WinActive(对话框);
;   ③ 跳转统一 SendText 真实注入, 禁用「控件直写」式赋值 (阶段 0 T3b 实测其静默无效)。
; 错误隔离: 所有原生/窗口调用 try/catch, 失败返回结构化结果, 绝不抛进热键链路。
; 依赖方向: 本层不依赖任何其他 quickswitch 模块 (仅用到全局内建)。
; 判别式依据: 阶段 0 §4 (真实对话框 / 消息框 / AHK 警告框 / 文件夹选择器四类)。
; ============================================================
#Warn All, Off

; 判别结果分类常量 (对应设计 classDiagram 的 kind* 字段)。
class DialogKind {
  static None := "none"
  static FileOpen := "file_open"
  static FileSave := "file_save"
  static FolderPicker := "folder_picker"
  static FileDialogGeneric := "file_dialog_generic"
  static MsgBox := "msgbox"
  static AhkWarning := "ahk_warning"
  static Other := "other"
}

; 读目录诊断计数 (设计 §8 残留项: W/A 双变体都过不了 DirExist 时计数上报)。
class DialogInspectorDiag {
  static ReadFailCount := 0
}

; 枚举缓冲区 (EnumChildWindows 回调无法捕获局部变量, 故用模块级缓冲)。
global DlgEnumBuffer := []

; ---- 公开接口 ----

; 判别窗口类别。返回 {kind, isFileDialog}。
;   kind ∈ DialogKind.*; isFileDialog 为 true 表示「可被 QuickSwitch 处理」的 shell 文件对话框。
DlgClassify(hwnd) {
  if (!hwnd)
    return {kind: DialogKind.None, isFileDialog: false}

  cs := _DlgEnumChildren(hwnd)
  shellView := 0
  edits := 0
  statics := 0
  rich := 0
  ahkBtn := 0
  hasCombo1136 := false
  hasEdit1148 := false
  hasEdit1001 := false
  hasEdit1152 := false

  for i, c in cs {
    cl := _DlgCls(c)
    id := _DlgCtrlId(c)
    if (cl = "DUIViewWndClassName" || cl = "SHELLDLL_DefView" || cl = "SysListView32" || cl = "DirectUIHWND")
      shellView += 1
    else if (cl = "Edit")
      edits += 1
    else if (cl = "Static")
      statics += 1
    else if (cl = "RichEdit20W")
      rich += 1
    ; AHK 自身的警告/错误框: id ∈ {65400,65401,65405,65411} (阶段 0 §4)
    if (id = 65400 || id = 65401 || id = 65405 || id = 65411)
      ahkBtn += 1
    ; 角色判别必须带窗口类: id=1001 同时出现在保存名框(Edit)与地址栏面包屑(ToolbarWindow32)
    if (cl = "ComboBox" && id = 1136)
      hasCombo1136 := true
    if (cl = "Edit" && id = 1148)
      hasEdit1148 := true
    if (cl = "Edit" && id = 1001)
      hasEdit1001 := true
    if (cl = "Edit" && id = 1152)
      hasEdit1152 := true
  }

  if (rich > 0 || ahkBtn > 0)
    return {kind: DialogKind.AhkWarning, isFileDialog: false}

  if (shellView > 0) {
    ; 文件夹选择器: 有 Edit id=1152(文件夹: 显示框) 且 无 ComboBox id=1136(文件类型过滤器)。
    ; 必须先判它, 否则 id=1152 会被下面误当文件名输入框 (设计 §4 明确警告)。
    if (hasEdit1152 && !hasCombo1136)
      return {kind: DialogKind.FolderPicker, isFileDialog: true}
    ; 打开框文件名框 = Edit id=1148; 保存框文件名框 = Edit id=1001 (角色判定必须带窗口类)。
    ; 实测补充: 保存框的过滤器是 ComboBox id=0 而非 1136, 故此处不以 1136 为前置条件。
    if (hasEdit1148)
      return {kind: DialogKind.FileOpen, isFileDialog: true}
    if (hasEdit1001)
      return {kind: DialogKind.FileSave, isFileDialog: true}
    return {kind: DialogKind.FileDialogGeneric, isFileDialog: true}
  }

  if (edits = 0 && statics > 0 && cs.Length <= 8)
    return {kind: DialogKind.MsgBox, isFileDialog: false}

  return {kind: DialogKind.Other, isFileDialog: false}
}

; 便捷判据: 是否为可处理的文件对话框。
DlgIsFileDialog(hwnd) {
  return DlgClassify(hwnd).isFileDialog
}

; 读取对话框当前目录。
; 主路径 = 直读地址栏面包屑窗口文本 (免聚焦、免发键); 绝不抢焦点、绝不发键。
; W/A 双变体都读, 择优取第一个能过 DirExist 的; 返回 {path, ok, variant, wFail, aFail}。
DlgReadDir(hwnd) {
  res := {path: "", ok: false, variant: "", wFail: 0, aFail: 0}
  if (!hwnd)
    return res

  cs := _DlgEnumChildren(hwnd)
  for i, c in cs {
    if (_DlgCls(c) != "ToolbarWindow32")
      continue
    pw := _DlgExtractPath(_DlgWinText(c))
    pa := _DlgExtractPath(_DlgWinTextA(c))
    okw := _DlgDirOk(pw)
    oka := _DlgDirOk(pa)
    if (pw != "" && !okw)
      res.wFail += 1
    if (pa != "" && !oka)
      res.aFail += 1
    if (res.ok)
      continue
    if (oka) {
      res.path := pa
      res.variant := "A"
      res.ok := true
    } else if (okw) {
      res.path := pw
      res.variant := "W"
      res.ok := true
    }
  }

  ; 双变体都过不了 DirExist, 但确有路径样文本: 计数上报 (替代盲猜)。
  if (!res.ok && (res.wFail > 0 || res.aFail > 0))
    DialogInspectorDiag.ReadFailCount += 1

  return res
}

; 跳转到 path。双闸门: ① DirExist(path) ② WinActive(对话框)。
; 通过后: Alt+D(首选) / Ctrl+L(回退) 聚焦地址栏 -> SendText -> Enter; 全程零 Esc。
; 返回 {ok, reason}; reason ∈ {ok, empty, dir_missing, win_inactive, focus_fail, send_error}。
DlgJumpTo(hwnd, path) {
  if (path = "")
    return {ok: false, reason: "empty"}

  ; 闸门①: 路径必须是已存在的目录 (阶段 0 §3.3 铁证: 非目录路径在保存框会真保存)。
  okDir := false
  try
    okDir := DirExist(path) ? true : false
  if (!okDir)
    return {ok: false, reason: "dir_missing"}

  ; 闸门②: 对话框必须仍是前台窗口 (否则 Alt+D 会落到别的窗口)。
  active := false
  try
    active := WinActive("ahk_id " hwnd) ? true : false
  if (!active)
    return {ok: false, reason: "win_inactive"}

  try {
    Send("!d")                     ; 主路径: Alt+D 聚焦地址栏 (纯导航语义)
    Sleep 120
    fc := ""
    try fc := ControlGetFocus("ahk_id " hwnd)
    if (fc = "") {
      Send("^l")                   ; 回退: Ctrl+L
      Sleep 120
      try fc := ControlGetFocus("ahk_id " hwnd)
    }
    if (fc = "")
      return {ok: false, reason: "focus_fail"}

    SendText(path)                 ; 真实注入 (禁用控件直写式赋值; T3b: 直写静默无效)
    Sleep 80

    ; 发键前再验一次窗口仍在前台 (TOCTOU 复核)。
    stillActive := false
    try stillActive := WinActive("ahk_id " hwnd) ? true : false
    if (!stillActive)
      return {ok: false, reason: "win_inactive"}

    Send("{Enter}")
    return {ok: true, reason: "ok"}
  } catch {
    return {ok: false, reason: "send_error"}
  }
}

; 对话框主按钮文本 (打开(&O) / 保存(&S) / 选择文件夹)。供上层做只读展示/诊断。
DlgMainButtonText(hwnd) {
  cs := _DlgEnumChildren(hwnd)
  for i, c in cs {
    if (_DlgCls(c) = "Button" && _DlgCtrlId(c) = 1)
      return _DlgWinText(c)
  }
  return ""
}

; 对话框窗口矩形 (屏幕坐标)。返回 {x, y, w, h}。
DlgWindowRect(hwnd) {
  x := 0, y := 0, w := 0, h := 0
  r := Buffer(16, 0)
  try {
    if (DllCall("GetWindowRect", "ptr", hwnd, "ptr", r.Ptr)) {
      x := NumGet(r, 0, "Int")
      y := NumGet(r, 4, "Int")
      right := NumGet(r, 8, "Int")
      bottom := NumGet(r, 12, "Int")
      w := right - x
      h := bottom - y
    }
  } catch {
  }
  return {x: x, y: y, w: w, h: h}
}

; ---- 私有辅助 ----

_DlgEnumProc(h, lp) {
  global DlgEnumBuffer
  if (DlgEnumBuffer.Length < 600)
    DlgEnumBuffer.Push(h)
  return 1
}

_DlgEnumChildren(h) {
  global DlgEnumBuffer
  DlgEnumBuffer := []
  try {
    cb := CallbackCreate(_DlgEnumProc, "F", 2)
    DllCall("EnumChildWindows", "ptr", h, "ptr", cb, "ptr", 0)
    CallbackFree(cb)
  } catch {
  }
  return DlgEnumBuffer
}

_DlgCls(h) {
  buf := Buffer(512, 0)
  n := 0
  try n := DllCall("GetClassName", "ptr", h, "ptr", buf.Ptr, "int", 256)
  return (n > 0) ? StrGet(buf.Ptr, n, "UTF-16") : ""
}

_DlgCtrlId(h) {
  id := -1
  try id := DllCall("GetDlgCtrlID", "ptr", h)
  return id
}

; Unicode (W) 变体读窗口文本。
_DlgWinText(h) {
  len := 0
  try len := DllCall("SendMessage", "ptr", h, "uint", 0x000E, "ptr", 0, "ptr", 0, "int")
  if (len <= 0 || len > 8192)
    return ""
  buf := Buffer((len + 1) * 2, 0)
  try DllCall("SendMessage", "ptr", h, "uint", 0x000D, "ptr", len + 1, "ptr", buf.Ptr, "ptr")
  return StrGet(buf.Ptr, "UTF-16")
}

; ANSI (A) 变体读窗口文本。地址栏面包屑是 ANSI 窗口, W 变体会把中文逐字节加宽 (阶段 0 §11 陷阱 20)。
_DlgWinTextA(h) {
  len := 0
  try len := DllCall("SendMessageA", "ptr", h, "uint", 0x000E, "ptr", 0, "ptr", 0, "int")
  if (len <= 0 || len > 8192)
    return ""
  buf := Buffer(len + 1, 0)
  try DllCall("SendMessageA", "ptr", h, "uint", 0x000D, "ptr", len + 1, "ptr", buf.Ptr, "ptr")
  return StrGet(buf.Ptr, "CP0")
}

; 从任意文本中抽取盘符/UNC 起始的路径 (面包屑文本带本地化前缀 "地址: ")。
_DlgExtractPath(s) {
  if (s = "")
    return ""
  if (RegExMatch(s, "[A-Za-z]:[\\/]", &m))
    return Trim(SubStr(s, m.Pos))
  if (RegExMatch(s, "\\\\[^\\]", &m))
    return Trim(SubStr(s, m.Pos))
  return ""
}

; 路径存在性 (目录) 强校验。
_DlgDirOk(p) {
  if (p = "")
    return false
  try
    return DirExist(p) ? true : false
  return false
}
