; ============================================================
; EverythingDropdown —— 结果下拉浮层 (渲染层)。
;
; 职责边界 (与 QuickSwitchUI.ahk 同款分层约定):
;   * 入参 = items:Array<{path,name,isFolder}> + index + 锚点矩形 (来自 EverythingHost);
;   * 唯一出参 = 回调 onPick(item);
;   * 不查 Everything、不读配置、不发键、**不找命令框窗口** (锚点几何全在 Host 端口)。
;
; 为什么自建浮层而不是「塞进命令框」: 命令框本体是上游预编译二进制
; (bin/KeyFlux-CommandInput.exe, 无源码), 只有 WM_CHAR 单向通道, 没有任何
; 「投递候选列表」接口 (实测其内部无 ListBox/ListView 资源)。命令框窗口不能像
; Flow Launcher 那样整体变高 (SizeToContent) —— 连续延伸只能由第二个窗口拼轮廓。
;
; 2026-10-03 连体重构 (用户需求「列表是命令框的连续延伸, 不是框下另挂一个独立框」,
; 参照 Flow Launcher 单窗长高的轮廓): 命令框底部圆角在两角留出**透明弧口**, 本浮层
; 顶边升起 earR 盖住弧口, 用 SetWindowRgn 把「顶部中段」从区域**豁口裁掉** ——
;   * 两只方角耳朵 (earR×earR) 填平命令框底角 => 侧边轮廓从框顶直线贯通到列表底;
;   * 中段豁口完全不绘制 => 命令框底边像素原样透出, 充当 Flow 式的查询区/结果区分隔线;
;   * 没有大面积半透明重叠 => 旧方案「0.9 盖 0.9 双重混合出白带」的接缝根源消除。
; 区域 = 圆角矩形 ∪ 顶部整条(填平方角) − 顶部中段(豁口), 见 _ApplyRegion。
; ============================================================
#Warn All, Off

global ED_GUI := 0
global ED_LV := 0
global ED_BUILT := false
global ED_ONPICK := 0
global ED_WHEEL := false          ; WM_MOUSEWHEEL 监听已注册 (只注册一次)
global ED_ROWS := 30              ; 可见行数上限 (不滚动, 与锚点高度共同决定)。
                                  ; 🔴 2026-09-21 由 16 提到 30: 用户要求「默认能展示的列表
                                  ;   太少, 再拉长一点」。30 行高 = 30*22+14 = 674px, 命令框
                                  ;   底边 (y≈500) + 674 = 1174 < 1200 (主屏高, 125% 缩放),
                                  ;   且 _AnchorRect 末尾还有「不越屏幕」夹取, 极端情况自动裁。
                                  ;   实际显示行数 = min(搜索返回条数, ED_ROWS) —— 搜索条数
                                  ;   由插件设置 limit (默认 20, 上限 300) 独立控制。
global ED_ROW_H := 22            ; 行高 (像素; 由 s10 字体近似, 与 ListView 实际行高接近)
; 连体几何 (2026-10-03, 见文件头): 耳朵尺寸 = 命令框底角弧口的**物理像素**半径。
; 命令框 skin 的 borderRadius=10 是逻辑 DIP, 物理像素 = 10 * DPI/96 (125% 下 ≈13)。
; 🔴 耳朵必须 ≥ 弧口: 小了弧口残留桌面色缝隙; 大了多盖一点白底 (白上盖白, 不可见)。
;   故 Ceil 取整向上偏。旧的 ED_INSET_FB / ED_COVER / ED_SIDE / ED_BOX_SHADOW_X
;   (命令框阴影边距、透明底边、白框宽度) 属命令框内部知识, 已整体迁入 EverythingHost。
global ED_EAR_DIP := 10          ; 命令框 borderRadius (逻辑 DIP, 与 skin 同源)
global ED_RADIUS := 10           ; 本浮层底部圆角半径 (px, 与命令框 borderRadius 呼应)
global ED_ALPHA := 230           ; 整窗不透明度 (≈0.9, 对齐命令框磨砂白 0.9); 让浮层与命令框同质感

; ---- 配色 (对齐命令框磨砂白体系) ----
global ED_BACK := "FFFFFF"       ; 背景: 白 (命令框 backgroundColor #FFFFFF @ 0.9)
global ED_TEXT := "1A1A1A"       ; 文字: 近黑 (命令框 keyColor #000000)
global ED_FONT := "Segoe UI"
global ED_FSIZE := "s10"

class EverythingDropdown {
  /** 注入点选回调 (由会话层提供)。 */
  static SetCallback(onPick) {
    global ED_ONPICK
    ED_ONPICK := onPick
  }

  /** 首次创建 Gui + ListView; 之后复用同一窗口。 */
  static Ensure() {
    global ED_GUI, ED_LV, ED_BUILT
    if (ED_BUILT)
      return

    g := Gui("+AlwaysOnTop -Caption +ToolWindow +E0x08000000 -DPIScale", EverythingMessages.T("title"))
    g.MarginX := 0
    g.MarginY := 0
    try g.BackColor := ED_BACK
    try g.SetFont(ED_FSIZE, ED_FONT)
    ; ListView 默认自带 WS_BORDER + WS_EX_CLIENTEDGE (灰框/3D 内陷), 会立刻破坏"连为一体"的观感,
    ; 创建后统一在下面移除。
    ; 🔴 宽度占位须给足 (2026-09-21): ListView 建好后由 Show/ShowHint 的 `Move(0,0,rect.w,...)`
    ; 统一改成实际宽度, 这里的 w360 只是创建时的占位 —— 但**建得太小会让首次显示前
    ; 的窗口按小尺寸算**, 故直接给到「比任何真实命令框都宽」的安全值 (1920 屏下命令框
    ; 实测 ~841)。真正生效的宽度完全来自 Move。
    lv := g.Add("ListView", "x0 y0 w1400 h200 -Multi -Hdr -Grid", [""])
    ; 关掉 Explorer 主题, 才能让下面的背景/文字颜色真正生效 (否则 Win10/11 会强填系统色)
    try lv.SetExplorerTheme(0)
    try lv.SetBkColor(ED_BACK)
    try lv.SetTextColor(ED_TEXT)
    try lv.SetGridLines(0)
    try {
      ; 移除 ListView 的 WS_BORDER(W) 与 WS_EX_CLIENTEDGE(E) -> 干净平板
      style := DllCall("user32.dll\GetWindowLongPtr", "ptr", lv.Hwnd, "int", -16, "ptr")
      DllCall("user32.dll\SetWindowLongPtr", "ptr", lv.Hwnd, "int", -16, "ptr", style & ~0x00800000)
      ex := DllCall("user32.dll\GetWindowLongPtr", "ptr", lv.Hwnd, "int", -20, "ptr")
      DllCall("user32.dll\SetWindowLongPtr", "ptr", lv.Hwnd, "int", -20, "ptr", ex & ~0x00000200)
      DllCall("user32.dll\SetWindowPos", "ptr", lv.Hwnd, "ptr", 0, "int", 0, "int", 0, "int", 0, "int", 0, "uint", 0x0020 | 0x0001 | 0x0002 | 0x0004) ; SWP_FRAMECHANGED|NOMOVE|NOSIZE|NOZORDER
    }
    try {
      ; FULLROWSELECT(0x20) + DOUBLEBUFFER(0x010000): 整行选中且无闪烁
      SendMessage(0x1004, 0, 0x100020, lv.Hwnd)   ; LVM_SETEXTENDEDLISTVIEWSTYLE
    }
    try lv.OnEvent("Click", EverythingDropdown._OnClick)
    try lv.OnEvent("DoubleClick", EverythingDropdown._OnClick)

    ; Shift+滚轮横向滚动: 全局监听一次 (回调内部按「消息归属 + 浮层可见 + Shift 按下」过滤)
    ; 🔴 回调必须走**文件级转发函数** ED_WheelForward —— OnMessage 对类静态方法直传
    ;   (EverythingDropdown._Wheel) 报 ValueError: Invalid callback (AHK v2 实测;
    ;   自由函数/Bind/ObjBindMethod 均可, 见 engine_error.log 2026-10-03)
    global ED_WHEEL
    if (!ED_WHEEL) {
      ED_WHEEL := true
      OnMessage(0x020A, ED_WheelForward)   ; WM_MOUSEWHEEL
    }

    ED_GUI := g
    ED_LV := lv
    ED_BUILT := true
  }

  /**
   * 显示结果列表。
   * @param items 结果数组
   * @param index 初始高亮行 (1 基)
   */
  static Show(items, index) {
    global ED_LV, ED_ROWS
    this.Ensure()
    rect := this._AnchorRect(items.Length)
    ; 列表内容从「耳朵行」之下开始 (窗口内 y=ear 起), 顶部 earR 高的耳朵带不归 ListView
    try ED_LV.Move(0, rect.ear, rect.w, rect.h - rect.ear)

    try ED_LV.Delete()
    for it in items {
      p := (it.path = "") ? it.name : it.path
      ED_LV.Add(, p)
    }
    this.Select(index)
    this._ShowAt(rect)
    this._FitColumns(rect)       ; 列宽自适应 + 初始滚到文件名端 (须在显示后, 隐藏时滚动范围不生效)
  }

  /** 显示一行提示文本 (无结果 / 通道不可用)。空检索词的初始态**不走这里** (Session 直接 Hide)。 */
  static ShowHint(text) {
    global ED_LV
    this.Ensure()
    rect := this._AnchorRect(1)
    try ED_LV.Move(0, rect.ear, rect.w, rect.h - rect.ear)
    try ED_LV.ModifyCol(1, rect.w - 4)
    try ED_LV.Delete()
    try ED_LV.Add(, text)
    this._ShowAt(rect)
  }

  /**
   * 结果列适配 (2026-10-03 用户定版「优先显示文件名, 目录可显示不全」):
   *   * 列宽取 max(最长行墨迹, 可视宽) ⇒ 长路径可经 Shift+滚轮 (见 _Wheel) 横向滚动看全;
   *   * 显示后滚到最右端 ⇒ 默认视图 = 文件名可见、目录头部被裁 (左对齐 + 滚动右端的
   *     组合, 视觉等同「右对齐」且向左滚动时路径头部自然展开 —— 可滚动列里右对齐
   *     会让尾部粘在视外右缘, 反而无法浏览);
   *   * ⚠ 必须在 _ShowAt **之后**调用 —— 隐藏窗口上 LVM_SCROLL 滚动范围不生效 (实测)。
   *   * ⚠ ModifyCol 第 2 参是**选项串**: `ModifyCol(1, w, "Right")` 会把 "Right" 当
   *     列标题改名而非对齐 (实测, 表头隐藏时不可见); 宽度+对齐应写 `"w Left"`。
   */
  static _FitColumns(rect) {
    global ED_LV
    viewW := rect.w - 4
    maxW := 0
    n := 0
    try n := ED_LV.GetCount()
    Loop n {
      t := ""
      try t := ED_LV.GetText(A_Index, 1)
      w := this._TextWidth(t)
      if (w > maxW)
        maxW := w
    }
    colW := (maxW > 0) ? maxW + 24 : viewW
    if (colW < viewW)
      colW := viewW
    try ED_LV.ModifyCol(1, colW)                 ; 左对齐 (文本列默认)
    dx := colW - viewW
    if (dx > 0)
      try SendMessage(0x1014, dx, 0, ED_LV.Hwnd)   ; LVM_SCROLL(0x1014=LVM_FIRST+20): 滚到最右 (文件名端可见)
  }

  /** 用 ListView 自身字体测量文本墨迹宽 (px); 失败返回 0 (调用方回落可视宽)。 */
  static _TextWidth(s) {
    global ED_LV
    if (s = "")
      return 0
    hdc := 0
    try hdc := DllCall("user32\GetDC", "ptr", ED_LV.Hwnd, "ptr")
    if (!hdc)
      return 0
    hfont := 0
    try hfont := SendMessage(0x0031, 0, 0, ED_LV.Hwnd)   ; WM_GETFONT
    old := 0
    if (hfont)
      old := DllCall("gdi32\SelectObject", "ptr", hdc, "ptr", hfont, "ptr")
    cx := 0, cy := 0
    DllCall("gdi32\GetTextExtentPoint32W", "ptr", hdc, "str", s, "int", StrLen(s), "int*", &cx := 0, "int*", &cy := 0)
    if (hfont)
      DllCall("gdi32\SelectObject", "ptr", hdc, "ptr", old)
    DllCall("user32\ReleaseDC", "ptr", ED_LV.Hwnd, "ptr", hdc)
    return cx
  }

  /** 高亮第 index 行 (越界则收敛到范围内)。 */
  static Select(index) {
    global ED_LV, ED_BUILT
    if (!ED_BUILT)
      return
    n := 0
    try n := ED_LV.GetCount()
    if (n < 1)
      return
    if (index < 1)
      index := 1
    if (index > n)
      index := n
    try ED_LV.Modify(index, "Select Focus")
    ; 🔴 高亮行必须滚进可视区 (2026-09-21): Modify 只改选态**不滚动** —— 结果条数多于
    ;   可视行时 (插件 limit 上限 300 > ED_ROWS 30; 且 LV 实际行高 ≥ ED_ROW_H 的估算值,
    ;   可视行更少) 控件出现滚动条, 上下键把高亮移到视野外用户却看不见, 只能手动拖
    ;   滚动条 (用户报障)。LVM_ENSUREVISIBLE (0x1013) 让控件把该行滚入可视区
    ;   (wParam = 0 基行号, lParam = 1 完全对齐而非贴边)。
    try SendMessage(0x1013, index - 1, 1, , "ahk_id " ED_LV.Hwnd)
  }

  /** 隐藏浮层 (不销毁, 供复用)。 */
  static Hide() {
    global ED_GUI, ED_BUILT
    if (!ED_BUILT)
      return
    try ED_GUI.Hide()
  }

  ; ---- 内部 ----

  /**
   * 布局矩形: 锚点来自 EverythingHost.CommandBoxAnchor() (命令框**可见白框** {x,y,w,bottom})。
   * 浮层顶边 = 白框底边 − earR (升起一只耳朵的高度盖住命令框底角弧口, 见文件头连体几何);
   * 列表内容区从白框底边开始 (窗口内 y = earR 起) —— 中段豁口把命令框底边原样透出当分隔线。
   * 锚点拿不到 (命令框进程未起) 时放屏幕下方, 与命令框常规位置错开 (历史行为)。
   */
  static _AnchorRect(rows) {
    global ED_ROWS, ED_ROW_H
    if (rows < 1)
      rows := 1
    if (rows > ED_ROWS)
      rows := ED_ROWS
    earR := this._EarRadius()

    x := 0, w := 0, y := 0
    a := EverythingHost.CommandBoxAnchor()
    if (IsObject(a) && a.w >= 180) {
      x := a.x
      w := a.w
      y := a.bottom - earR
    } else {
      w := 700
      x := (A_ScreenWidth - w) // 2
      y := A_ScreenHeight - (rows * ED_ROW_H + 40 + earR)
    }

    hList := rows * ED_ROW_H + 14     ; 底部多留白: 让最后一行远离底边, 圆角 + 阴影呼吸
    h := earR + hList
    ; 不越出屏幕 (取主屏高度做保守收敛)
    if (y + h > A_ScreenHeight)
      h := A_ScreenHeight - y - 4
    if (h < earR + ED_ROW_H)
      h := earR + ED_ROW_H
    return {x: x, y: y, w: w, h: h, ear: earR}
  }

  /**
   * 耳朵半径 (物理 px) = 命令框 borderRadius (DIP) × DPI 缩放, 向上取整。
   * 🔴 耳朵必须 ≥ 命令框底角弧口的物理半径: 小了弧口残留桌面色缝隙; 大了多盖一点白底
   *   (白上盖白, 不可见)。故 Ceil 偏大不偏小。
   */
  static _EarRadius() {
    global ED_EAR_DIP
    r := Ceil(ED_EAR_DIP * A_ScreenDPI / 96)
    return (r < 4) ? 4 : r
  }

  static _ShowAt(rect) {
    global ED_GUI, ED_ALPHA
    try ED_GUI.Show("NA x" rect.x " y" rect.y " w" rect.w " h" rect.h)
    ; Show 后再设区域; 失败静默 (退化为方角整矩形, 功能不受影响)
    try this._ApplyRegion(rect.w, rect.h, rect.ear)
    ; 同质感延续体: 整窗 0.9 半透明 + DWM 柔和外阴影, 与命令框磨砂白同质感。
    try WinSetTransparent(ED_ALPHA, "ahk_id " ED_GUI.Hwnd)
    try this.FrameShadow(ED_GUI.Hwnd)
  }

  /**
   * 连体区域: 圆角矩形 ∪ 顶部整条(顶角填平成方耳朵) − 顶部中段(豁口)。
   *   * 两只 earR 方耳朵盖住命令框底角的透明弧口 ⇒ 侧边轮廓从框顶直线贯通到列表底;
   *   * 中段豁口**不绘制** ⇒ 命令框底边像素原样透出, 充当查询区/结果区的天然分隔线;
   *   * 没有大面积半透明重叠 ⇒ 旧方案「0.9 盖 0.9 双重混合出白带」的接缝根源消除。
   * 失败静默退回方角整矩形 (耳朵缺失 = 弧口可见, 功能不受影响)。
   */
  static _ApplyRegion(w, h, earR) {
    global ED_GUI, ED_RADIUS
    hwnd := ED_GUI.Hwnd
    if (hwnd = 0 || w < earR * 2 + 8 || h < earR + ED_RADIUS)
      return
    rgn := DllCall("gdi32.dll\CreateRoundRectRgn", "int", 0, "int", 0, "int", w, "int", h, "int", 2 * ED_RADIUS, "int", 2 * ED_RADIUS, "ptr")
    if (rgn = 0)
      return
    top := DllCall("gdi32.dll\CreateRectRgn", "int", 0, "int", 0, "int", w, "int", earR, "ptr")
    if (top != 0) {
      DllCall("gdi32.dll\CombineRgn", "ptr", rgn, "ptr", rgn, "ptr", top, "int", 2)   ; RGN_OR: 顶角填平方角
      DllCall("gdi32.dll\DeleteObject", "ptr", top)
    }
    mid := DllCall("gdi32.dll\CreateRectRgn", "int", earR, "int", 0, "int", w - earR, "int", earR, "ptr")
    if (mid != 0) {
      DllCall("gdi32.dll\CombineRgn", "ptr", rgn, "ptr", rgn, "ptr", mid, "int", 4)   ; RGN_DIFF: 中段豁口
      DllCall("gdi32.dll\DeleteObject", "ptr", mid)
    }
    DllCall("user32.dll\SetWindowRgn", "ptr", hwnd, "ptr", rgn, "int", 1)
    ; SetWindowRgn 成功后系统接管/管理 rgn 所有权, 不再手动 DeleteObject(rgn)
  }

  /**
   * 给无边框 AHK 窗口叠加 DWM 系统阴影 (柔和的渐变外圈), 与命令框的阴影同机制,
   * 让浮层的阴影能「接住」命令框底部的阴影, 视觉上自然长成一个整体。
   * 参考 bin/lib/core/InputTipWindow.ahk 的 FrameShadow (Apache 场景同源实现)。
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

  static _OnClick(lv, item, *) {
    global ED_ONPICK
    if (item < 1)
      return
    path := ""
    try path := lv.GetText(item, 1)
    if (path != "" && ED_ONPICK != 0)
      ED_ONPICK(path)
  }

  /**
   * Shift+滚轮 = 结果列表横向滚动 (2026-10-03 用户定版): 滚轮向前 (delta<0) 向左看
   * 路径头部, 向后向右看文件名端。返回 0 吞掉消息 ⇒ 不触发默认垂直滚动;
   * 非 Shift / 非本浮层窗口的滚轮一律放行 (return "" = 不干预)。
   */
  static _Wheel(wParam, lParam, msg, hwnd) {
    global ED_GUI, ED_LV, ED_BUILT
    if (!ED_BUILT)
      return ""
    ; 不按 hwnd 过滤: 滚轮可能路由到焦点窗口 (查询输入面) 而非列表 —— 只要浮层可见
    ; 且 Shift 按下, 横滚就是明确意图 (浮层可见期间本进程仅会话相关窗口收得到滚轮)
    if (!DllCall("IsWindowVisible", "ptr", ED_GUI.Hwnd, "int"))
      return ""
    if (!GetKeyState("Shift", "P"))
      return ""
    delta := (wParam >> 16) & 0xFFFF
    if (delta >= 0x8000)
      delta -= 65536
    SendMessage(0x1014, (delta < 0) ? 60 : -60, 0, ED_LV.Hwnd)   ; LVM_SCROLL(0x1014) ±60px
    return 0
  }
}

; 文件级 OnMessage 转发 (类静态方法直传 OnMessage 会报 Invalid callback, 见 _Wheel 注释)
ED_WheelForward(wParam, lParam, msg, hwnd) {
  return EverythingDropdown._Wheel(wParam, lParam, msg, hwnd)
}