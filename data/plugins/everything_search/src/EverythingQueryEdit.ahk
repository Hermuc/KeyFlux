; ============================================================
; EverythingQueryEdit —— 搜索模式的查询输入面 (Flow Launcher 式真实输入控件)。
;
; 为什么存在: 中文检索要求「IME 上屏的中文 = 检索词」。命令框是上游预编译二进制,
;   其文本无任何回读通道 (InputHook 拿不到组合期按键/上屏文本 —— design-ime-guard
;   记录的架构边界; 24H2 又禁跨进程 AttachThreadInput, IMM 捕获 err=87 实测不可用)。
;   Flow Launcher 的解法 = 自家窗口里放一个真正的 TextBox, IME 直接组合进控件,
;   查询 = 控件文本 —— 本模块把同一解法移植过来: 搜索模式激活时, 在命令框的
;   可见白框位置覆盖一个**真实 Edit 控件** (样式复刻命令框皮肤), 命令框隐藏,
;   焦点交给 Edit ⇒ 用户输入 (拼音组合/上屏中文/英文/退格) 全部原生发生在
;   本控件里, 会话层经 GetText() 直接读文本 —— 零跨进程障碍。
;
; 样式来源 = **解析 CommandInputSkin.txt** (可移植: 用户在设置面板改皮肤, 本输入面
;   自动跟随, 无需改代码)。计算链 (与原 exe 的合成方式对齐, 图1 逐像素实测校准):
;   * 窗口不透明度 = backgroundOpacity * 255 (原 exe 同款整窗 alpha) ⇒
;     白底叠深色桌面后净色一致 (实测 #E6E6E6 ✓);
;   * 网格线颜色 = gridlineColor @gridlineOpacity 叠在净背景上再反解回窗口内容色
;     (实测内容 #F6F8FC → 净 #DFE0E4 ✓);
;   * 网格间距 = 20 DIP 按 DPI 缩放 (实测 25px @125% ✓), 首线偏移 = 间距-1 (实测 24px);
;   * 圆角 = borderRadius; 文字色 = keyColor #000000; 字体 = bin/font/font.ttf
;     (用户 commandFont 烘焙副本, 家族名经 PrivateFontCollection 实测
;     = "更纱黑体 SC"; 用户更换命令字体后需同步此常量)。
;
; 职责边界 (渲染层, 同 EverythingDropdown 约定):
;   * 入参 = 锚点矩形 (Host.CommandBoxAnchor) + 初始文本;
;   * 出参 = GetText() (会话层轮询);
;   * 不查 Everything、不读设置面板、不管理会话状态。
;
; 键盘/IME 契约 (与会话层的分工):
;   * 物理键经引擎 InputHook 的 V 透传到达本控件 (搜索模式已 UnlockForSearch);
;   * 字母/中文/退格 = Edit 原生行为 (会话层不再追加/回显/截断 —— 检索词以
;     GetText() 为准, 由会话层轮询同步);
;   * ↑↓/回车/Esc/CapsLock = 引擎 InputHook EndKeys/OnKey 语义, 与本控件无关
;     (单行 Edit 回车不换行, ↑↓ 不移动光标行)。
; ============================================================
#Warn All, Off

global QE_GUI := 0
global QE_EDIT := 0
global QE_BUILT := false
global QE_FONT_DONE := false
global QE_LAST_W := 0
global QE_LAST_H := 0
global QE_FAMILY_RESOLVED := ""
global QE_SKIN := 0

; 字体家族 (bin/font/font.ttf 实测; 用户更换 commandFont 后需同步)
global QE_FAMILY := "更纱黑体 SC"

class EverythingQueryEdit {
  /** 显示输入面: 覆盖 anchor {x,y,w,h}, 载入 initial 文本并取得焦点 (IME 随焦点附着)。 */
  static Show(anchor, initial := "") {
    global QE_GUI, QE_EDIT, QE_BUILT, QE_LAST_W, QE_LAST_H, QE_FAMILY_RESOLVED
    ; 尺寸变了才重建 (网格控件按尺寸布点)
    if (QE_BUILT && (QE_LAST_W != anchor.w || QE_LAST_H != anchor.h)) {
      try QE_GUI.Destroy()
      QE_BUILT := false
    }
    this.Ensure(anchor)
    QE_LAST_W := anchor.w
    QE_LAST_H := anchor.h
    ; 字号/内边距随锚点高度自适应 (anchor.h 为物理像素; 字号换算逻辑像素→磅)
    dpi := (A_ScreenDPI > 0) ? A_ScreenDPI : 96
    logicalH := anchor.h * 96 / dpi
    pts := Round(logicalH * 0.30)
    if (pts < 12)
      pts := 12
    lineH := Round(pts * dpi / 72 * 1.35)
    padX := Round(anchor.h * 0.14)
    edY := Round((anchor.h - lineH) / 2)
    edH := lineH + Round(anchor.h * 0.08)
    try QE_EDIT.SetFont("s" pts " c000000", QE_FAMILY_RESOLVED)
    try QE_EDIT.Move(padX, edY, anchor.w - padX * 2, edH)
    QE_EDIT.Value := initial
    try QE_GUI.Move(anchor.x, anchor.y, anchor.w, anchor.h)
    try QE_GUI.Show("x" anchor.x " y" anchor.y " w" anchor.w " h" anchor.h)
    try this._RoundAll(anchor.w, anchor.h)
    try this.FrameShadow(QE_GUI.Hwnd)   ; 命令框隐藏, 阴影由本浮层延续
    ; 诊断: Edit 实际 HFONT 的家族名 (临时)
    hf := 0
    try hf := SendMessage(0x0031, 0, 0, QE_EDIT.Hwnd)   ; WM_GETFONT
    if (hf) {
      lf := Buffer(116)
      DllCall("gdi32\GetObjectW", "ptr", hf, "int", 116, "ptr", lf)
      face := StrGet(lf.Ptr + 28, 32, "UTF-16")
      try FileAppend(FormatTime(A_Now, "HH:mm:ss") " queryedit: edit hfont face=[" face "] pts=" pts "`n", A_Temp "\kf_es_debug.log", "UTF-8")
    }
    try QE_EDIT.Focus()
  }

  /** 当前输入文本 (会话层轮询用)。 */
  static GetText() {
    global QE_EDIT, QE_BUILT
    if (!QE_BUILT)
      return ""
    t := ""
    try t := QE_EDIT.Value
    return t
  }

  /** 设置输入文本 (种子路径; 用户后续输入不受影响)。 */
  static SetText(text) {
    global QE_EDIT, QE_BUILT
    if (!QE_BUILT)
      return
    try QE_EDIT.Value := text
  }

  /** 隐藏输入面 (可重复调用)。 */
  static Hide() {
    global QE_GUI, QE_BUILT
    if (!QE_BUILT)
      return
    try QE_GUI.Hide()
  }

  ; ---- 内部 ----

  /**
   * 首次创建 Gui + Edit + 皮肤网格线; 之后复用同一窗口 (尺寸变化时由 Show 触发重建)。
   * 字体: AddFontResourceExW **0x00 (公共字体表)** 加载 bin/font/font.ttf ——
   *   NOT_ENUM/PRIVATE 档对 AHK SetFont 的族名枚举校验不可见 (回落空族名默认字体,
   *   诊断日志 edit hfont face=[] 实证); 0x00 按族名解析全路径可用, 副作用 = 字体
   *   本会话对其余程序可见 (它本就是用户的命令框字体, 无实际影响)。
   */
  static Ensure(anchor) {
    global QE_GUI, QE_EDIT, QE_BUILT, QE_FONT_DONE
    if (QE_BUILT)
      return
    this._EnsureFont()
    skin := this._Skin()
    dpi := (A_ScreenDPI > 0) ? A_ScreenDPI : 96
    g := Gui("+AlwaysOnTop -Caption +ToolWindow -DPIScale", "KeyFlux Everything Search")
    g.MarginX := 0
    g.MarginY := 0
    g.BackColor := "FFFFFF"
    g.SetFont("s22 c000000", QE_FAMILY)
    ed := g.Add("Edit", "x20 y0 w600 h60 -E0x200 -VScroll BackgroundFFFFFF")
    QE_GUI := g
    QE_EDIT := ed
    ; 网格线: 间距 20 DIP → 物理 px; 首线偏移 = 间距 - 1 (实测 24px @125%);
    ;   颜色 = 皮肤网格色 @gridlineOpacity 叠净背景后反解的窗口内容色
    step := Round(20 * dpi / 96)
    if (step < 8)
      step := 8
    off := step - 1
    grid := this._GridContentColor(skin)
    x := off
    while (x < anchor.w - 2) {
      g.Add("Progress", "x" x " y2 w1 h" (anchor.h - 4) " Background" grid)
      x += step
    }
    y := off
    while (y < anchor.h - 2) {
      g.Add("Progress", "x2 y" y " w" (anchor.w - 4) " h1 Background" grid)
      y += step
    }
    ; 整窗不透明度 = backgroundOpacity (与原 exe 同款整窗 alpha ⇒ 净观感一致)
    try WinSetTransparent(Round(skin.backgroundOpacity * 255), "ahk_id " g.Hwnd)
    QE_BUILT := true
  }

  /**
   * 解析 CommandInputSkin.txt (bin 下, key = value 行式) 为所需子集。
   * 文件缺失/损坏 → 回落默认皮肤 (与原 exe 默认一致)。
   */
  static _Skin() {
    global QE_SKIN
    if (QE_SKIN != 0)
      return QE_SKIN
    skin := {backgroundOpacity: 0.9, gridlineColor: 0x2843AD, gridlineOpacity: 0.04, borderRadius: 10}
    path := A_ScriptDir "\CommandInputSkin.txt"
    if (FileExist(path)) {
      try {
        for line in StrSplit(FileRead(path, "UTF-8"), "`n", "`r") {
          if !RegExMatch(line, "^(\w+)\s*=\s*(.+)$", &m)
            continue
          k := m[1], v := Trim(m[2])
          switch k {
            case "backgroundOpacity": skin.backgroundOpacity := IsNumber(v) ? Number(v) : 0.9
            case "gridlineOpacity":   skin.gridlineOpacity := IsNumber(v) ? Number(v) : 0.04
            case "borderRadius":      skin.borderRadius := IsNumber(v) ? Number(v) : 10
            case "gridlineColor":     skin.gridlineColor := Integer(RegExReplace(v, "^#", "0x"))
          }
        }
      } catch {
        ; 解析失败 → 保持默认
      }
    }
    QE_SKIN := skin
    return skin
  }

  /**
   * 网格线在窗口内容空间的颜色: 皮肤网格色 @gridlineOpacity 叠「净背景」后,
   * 再按整窗不透明度反解回内容色 (窗口 alpha 由 backgroundOpacity 决定,
   * 叠深色桌面后净背景与原 exe 一致 —— 推导见文件头与图1 实测)。
   */
  static _GridContentColor(skin) {
    bgOp := skin.backgroundOpacity
    gOp := skin.gridlineOpacity
    desk := 13   ; 假定深色桌面 #0D0D0D (与本机使用环境一致; 浅色桌面偏差极小)
    content := []
    for i, ch in [0x28, 0x43, 0xAD] {
      nb := bgOp * 255 + (1 - bgOp) * desk
      ng := gOp * ch + (1 - gOp) * nb
      c := Round((ng - (1 - bgOp) * desk) / bgOp)
      if (c < 0)
        c := 0
      if (c > 255)
        c := 255
      content.Push(c)
    }
    return Format("{:02X}{:02X}{:02X}", content[1], content[2], content[3])
  }

  ; 加载引擎命令框字体 (bin/font/font.ttf = 用户 commandFont 烘焙副本)。
  ; 🔴 标志必须用 **0x00 (公共字体表)**: NOT_ENUM/PRIVATE 档对 AHK SetFont 的族名
  ;   枚举校验不可见 ⇒ 回落默认字体对象 (空族名 → 宋体衬线观感, 2026-10-03 实测,
  ;   诊断日志 edit hfont face=[] 实证)。副作用 = 字体在本会话对其余程序可见
  ;   (它本就是用户的命令框字体, 无实际影响); 引擎退出后残留至重启, 可接受。
  ;   (依据: config-ui-reactor/src/platform/fonts.rs 的 AddFontResourceEx 实测矩阵,
  ;   该矩阵测的是 DirectWrite 路径; AHK Gui.SetFont 走 GDI+枚举校验, 行为更严。)
  static _EnsureFont() {
    global QE_FONT_DONE
    if (QE_FONT_DONE)
      return
    QE_FONT_DONE := true
    fontPath := A_ScriptDir "\font\font.ttf"
    if (FileExist(fontPath))
      try DllCall("gdi32\AddFontResourceExW", "str", fontPath, "uint", 0x00, "ptr", 0)
  }

  ; 整窗圆角 (矩形区域半径 = 皮肤 borderRadius)。
  static _RoundAll(w, h) {
    global QE_GUI, QE_SKIN
    r := QE_SKIN.borderRadius
    hwnd := QE_GUI.Hwnd
    if (hwnd = 0 || w < r || h < r)
      return
    rgn := DllCall("gdi32.dll\CreateRoundRectRgn", "int", 0, "int", 0, "int", w, "int", h, "int", 2 * r, "int", 2 * r, "ptr")
    if (rgn = 0)
      return
    DllCall("user32.dll\SetWindowRgn", "ptr", hwnd, "ptr", rgn, "int", 1)
    ; SetWindowRgn 成功后系统接管/管理 rgn 所有权, 不再手动 DeleteObject(rgn)
  }

  /**
   * 给无边框 AHK 窗口叠加 DWM 系统阴影 (同 EverythingDropdown.FrameShadow, 与命令框
   * 阴影同机制 —— 命令框隐藏期间由本输入面延续阴影观感)。失败静默。
   */
  static FrameShadow(hwnd) {
    try {
      DllCall("dwmapi\DwmIsCompositionEnabled", "Int*", &enabled := false)
      if (!enabled)
        return
      margin := Buffer(16)
      NumPut("UInt", 1, "UInt", 1, "UInt", 1, "UInt", 1, margin)
      DllCall("dwmapi\DwmSetWindowAttribute", "Ptr", hwnd, "UInt", 2, "Int*", 2, "UInt", 4)
      DllCall("dwmapi\DwmExtendFrameIntoClientArea", "Ptr", hwnd, "Ptr", margin)
    }
  }
}
