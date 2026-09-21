; ============================================================
; EverythingDropdown —— 结果下拉浮层 (渲染层)。
;
; 职责边界 (与 QuickSwitchUI.ahk 同款分层约定):
;   * 入参 = items:Array<{path,name,isFolder}> + index + 锚点;
;   * 唯一出参 = 回调 onPick(item);
;   * 不查 Everything、不读配置、不发键、不持有命令框句柄。
;
; 为什么自建浮层而不是「塞进命令框」: 命令框本体是上游预编译二进制
; (bin/KeyFlux-CommandInput.exe, 无源码), 只有 WM_CHAR 单向通道, 没有任何
; 「投递候选列表」接口 (实测其内部无 ListBox/ListView 资源)。故下拉列表只能由
; 引擎侧自绘, 锚定在命令框正下方 —— 视觉上仍是「command 下方的下拉列表」。
;
; 2026-09-19 视觉重构: 把结果列表做成命令框的「向下延伸」, 视觉上只有一个框。
;   * 配色对齐命令框磨砂白: 背景 #FFFFFF、文字近黑、Segoe UI、无表格线/无列头/无 3D 边框;
;   * 顶部「伸进」命令框可见白色内部 (实测透明底边 + 盖圆角) —— 盖住命令框自带的底部
;     圆角与阴影, 于是读成一个「圆顶、向下延伸」的整体, 而不是两个独立圆角卡片;
;   * 宽度贴近命令框内宽 (ED_SIDE), 底部按 borderRadius 收圆角 (ED_RADIUS)。
;
; 「不激活」实现: -Caption + WS_EX_NOACTIVATE(0x08000000) 且 Show("NA"),
; 显示后前台仍是用户原来的窗口, 命令框输入不受影响。
; 错误隔离: 全部 GUI 调用 try/catch, 失败静默 (最坏情况 = 没有下拉, 功能降级但不崩)。
; ============================================================
#Warn All, Off

global ED_GUI := 0
global ED_LV := 0
global ED_BUILT := false
global ED_ONPICK := 0
global ED_ROWS := 16              ; 可见行数上限 (不滚动, 与锚点高度共同决定)
global ED_ROW_H := 22            ; 行高 (像素; 由 s10 字体近似, 与 ListView 实际行高接近)
; 连体策略: 下拉面板顶部要「伸进」命令框的可见白色内部, 把命令框自身的底部圆角 + 阴影
; 整段盖住, 于是视觉上只剩一个「圆顶的、向下延伸」的整体, 而不是两个独立的圆角卡片。
;   * ED_INSET_FB: 命令框「窗口底边 → 可见白色底边」的透明区高度兜底 (实测 ≈ 24, 含 shadowSize)。
;   * ED_COVER:    再往上多盖「底部圆角+边框+阴影」的高度 (半径 10 + 边框 3 ≈ 13, 取 16 留余量)。
;   * 有真机采样时用 ED_INSET_FB 兜底, 采样成功则用实测值。
global ED_INSET_FB := 24         ; 透明底边估算 (px)
global ED_COVER := 16            ; 额外向上盖住圆角/边框/阴影 (半径10+边框3+余量≈16, px)
global ED_SIDE := 8              ; 左右各收到的内边距 (让下拉比命令框左右更内缩, 形成悬浮错落, px)
global ED_RADIUS := 10           ; 与命令框 borderRadius 相同的圆角半径 (px)
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
    lv := g.Add("ListView", "x0 y0 w360 h200 -Multi -Hdr -Grid", [""])
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
    try ED_LV.Move(0, 0, rect.w, rect.h)
    try ED_LV.ModifyCol(1, rect.w - 4)

    try ED_LV.Delete()
    for it in items {
      p := (it.path = "") ? it.name : it.path
      ED_LV.Add(, p)
    }
    this.Select(index)
    this._ShowAt(rect)
  }

  /** 显示一行提示文本 (无结果 / 通道不可用 / 引导继续输入)。 */
  static ShowHint(text) {
    global ED_LV
    this.Ensure()
    rect := this._AnchorRect(1)
    try ED_LV.Move(0, 0, rect.w, rect.h)
    try ED_LV.ModifyCol(1, rect.w - 4)
    try ED_LV.Delete()
    try ED_LV.Add(, text)
    this._ShowAt(rect)
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
   * 锚点矩形: 命令框正下方, 宽度跟随命令框, 高度按行数收敛到屏幕工作区内。
   * 顶部「伸进」命令框可见白色内部 (测得的透明底边 + 盖圆角), 让命令框 + 本面板
   * 读成一个「圆顶的、向下延伸」的整体, 而非两个独立的圆角卡片。
   */
  static _AnchorRect(rows) {
    global ED_ROWS, ED_ROW_H, ED_INSET_FB, ED_COVER, ED_SIDE
    if (rows < 1)
      rows := 1
    if (rows > ED_ROWS)
      rows := ED_ROWS

    bx := 0, by := 0, bw := 700, bh := 0, found := false, insetLog := ED_INSET_FB
    try {
      hwnd := WinExist("ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe")
      if (hwnd) {
        WinGetPos(&bx, &by, &bw, &bh, hwnd)
        found := true
        m := this._BoxBottomInset(hwnd, bx, by, bw, bh)
        if (m >= 0)
          insetLog := m
      } else {
        ; 🔴 命令框「存在但隐藏」是**常态而非异常** (2026-09-21): 按前置键搜索时,
        ;   SeedFromSelection 会先 ActivateBackend() 把前台切回原窗口, 命令框可能因此隐藏;
        ;   而 WinExist 默认忽略隐藏窗口 ⇒ 返回 0。
        ;   旧实现在这时走「屏幕居中兜底」(屏宽-700)/2, 屏高/3 —— 实测 1920x1200 下即
        ;   (610,404), **恰好落在命令框矩形 (497..1422, 300..500) 正中** ⇒ 浮层直接盖住
        ;   命令框 (用户报障「搜索结果不在命令框下, 而是覆盖了命令框」)。
        ;   改为: 开隐藏检测再查一次。命令框窗口其实一直都在, 拿得到真实矩形 ⇒ 仍按命令框
        ;   正常锚定, 只是**不做底边像素采样** —— 隐藏态的屏幕像素不是命令框本体,
        ;   采了只会得到错误的伸进量, 直接用兜底值 (ED_INSET_FB) 更稳。
        hwndH := 0
        DetectHiddenWindows true
        try hwndH := WinExist("ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe")
        DetectHiddenWindows false
        if (hwndH) {
          WinGetPos(&bx, &by, &bw, &bh, hwndH)
          found := true
          insetLog := ED_INSET_FB
        }
      }
    }
    if (!found) {
      ; 连隐藏窗口也查不到 (命令框进程未起): 仍保持浮层可见, 但**不要放到屏心** ——
      ; 那正是命令框的常驻区域。改放屏幕下方, 与命令框常规位置错开。
      bx := (A_ScreenWidth - 700) // 2
      by := A_ScreenHeight - (ED_ROWS * ED_ROW_H + 40)
      bw := 700
      bh := 0
      insetLog := 0
    }

    x := bx + ED_SIDE
    w := bw - ED_SIDE * 2
    if (w < 180)
      w := 180
    ; 伸进 (insetLog + ED_COVER), 盖住命令框的透明底边 + 圆角 + 阴影。
    ; 🔴 封顶 (2026-09-21): 伸进量再大也不得越过命令框下半部 —— 否则浮层会盖住框体本身。
    ;   上限取设计基准 ED_INSET_FB + ED_COVER (=40)。底边采样是「读屏幕像素找近白行」的
    ;   启发式, 在深色背景/半透明框体上可能给出偏大的值; 封顶后即使采样异常, 浮层顶边
    ;   也不会进入框体上半部 (配合下方 `if (y < by+4)` 的夹取双保险)。
    over := insetLog + ED_COVER
    if (over > ED_INSET_FB + ED_COVER)
      over := ED_INSET_FB + ED_COVER
    y := by + bh - over
    ; 极短窗口时的安全兜底: 至少从窗口顶边之内 4px 起 (正常情况不会触发)
    if (y < by + 4)
      y := by + 4
    h := rows * ED_ROW_H + 14     ; 底部多留白: 让最后一行远离底边, 圆角 + 阴影呼吸

    ; 不越出屏幕 (取主屏高度做保守收敛)
    if (y + h > A_ScreenHeight)
      h := A_ScreenHeight - y - 4
    if (h < ED_ROW_H)
      h := ED_ROW_H
    return {x: x, y: y, w: w, h: h}
  }

  /**
   * 实测命令框「窗口底边 → 可见白色底边」的透明区高度 (逻辑像素), 供顶部伸进量使用。
   * 方法: 采样屏幕中心列, 从窗口底边向上找第一个「近白」像素行 —— 那行即命令框可见底边。
   * DPI 安全: BitBlt 用物理坐标采样, 再按 DPI 比例换算回逻辑像素 (与 WinGetPos 同空间)。
   * @returns {number} 透明区高度(逻辑)。失败返回 -1 (调用方用 ED_INSET_FB 兜底)。
   */
  static _BoxBottomInset(hwnd, bx, by, bw, bh) {
    global ED_INSET_FB
    try {
      if (!hwnd || !IsNumber(bx) || !IsNumber(by) || bw < 40 || bh < 20)
        return -1
      ; DPI 比例
      hdc0 := DllCall("user32.dll\GetDC", "ptr", 0, "ptr")
      dpi := DllCall("gdi32.dll\GetDeviceCaps", "ptr", hdc0, "int", 90, "uint")  ; LOGPIXELSY
      DllCall("user32.dll\ReleaseDC", "ptr", 0, "ptr", hdc0)
      scale := (dpi > 96) ? dpi / 96.0 : 1.0
      px := Round(bx * scale)
      py := Round(by * scale)
      pw := Round(bw * scale)
      ph := Round(bh * scale)
      if (pw < 40 || ph < 20)
        return -1

      hdcScr := DllCall("user32.dll\GetDC", "ptr", 0, "ptr")
      hdcMem := DllCall("gdi32.dll\CreateCompatibleDC", "ptr", hdcScr, "ptr")
      hbmp := DllCall("gdi32.dll\CreateCompatibleBitmap", "ptr", hdcScr, "int", pw, "int", ph, "ptr")
      DllCall("gdi32.dll\SelectObject", "ptr", hdcMem, "ptr", hbmp)
      DllCall("gdi32.dll\BitBlt", "ptr", hdcMem, "int", 0, "int", 0, "int", pw, "int", ph, "ptr", hdcScr, "int", px, "int", py, "uint", 0x00CC0020)

      cx := pw // 2
      inset := ph
      Loop 3 {   ; 中心列 + 左右偏移列, 取 max(近白行 = 最大可见底边) 抗噪
        off := (A_Index - 1) * (pw // 8)
        xc := cx + ((A_Index = 1) ? 0 : ((A_Index = 2) ? -off : off))
        if (xc < 2) xc := 2
        if (xc > pw - 3) xc := pw - 3
        Loop ph {
          yy := ph - A_Index
          if (yy < 0)
            break
          c := DllCall("gdi32.dll\GetPixel", "ptr", hdcMem, "int", xc, "int", yy, "uint")
          r := c & 0xFF, gg := (c >> 8) & 0xFF, bb := (c >> 16) & 0xFF
          if (r >= 160 && gg >= 160 && bb >= 160) {
            if (ph - 1 - yy < inset)
              inset := ph - 1 - yy
            break
          }
        }
      }

      DllCall("gdi32.dll\DeleteObject", "ptr", hbmp)
      DllCall("gdi32.dll\DeleteDC", "ptr", hdcMem)
      DllCall("user32.dll\ReleaseDC", "ptr", 0, "ptr", hdcScr)

      if (inset >= ph)
        return -1
      res := Round(inset / scale)
      if (res < 2 || res > bh)
        return -1
      return res
    }
    return -1
  }

  static _ShowAt(rect) {
    global ED_GUI, ED_RADIUS, ED_ALPHA
    try ED_GUI.Show("NA x" rect.x " y" rect.y " w" rect.w " h" rect.h)
    ; Show 后再设圆角区域; 失败静默 (退化为方角, 功能不受影响)
    if (ED_RADIUS > 0) {
      try this._RoundBottom(rect.w, rect.h)
    }
    ; 同质感延续体: 整窗 0.9 半透明 + DWM 柔和外阴影, 与命令框磨砂白同质感,
    ; 阴影从命令框底部自然延续到本浮层底部, 配合顶部伸进, 读成一个「圆顶向下延伸」的整体,
    ; 而不是「不透明白平板塞在半透明框下」的两个独立块。
    try WinSetTransparent(ED_ALPHA, "ahk_id " ED_GUI.Hwnd)
    try this.FrameShadow(ED_GUI.Hwnd)
  }

  ; 用区域把窗口底部两角做成圆角 (与命令框 borderRadius=10 呼应)。顶边保持直角以无缝对接命令框。
  ; 实现: 圆角矩形 + 顶端 r 高的实心条 OR 合并 => 上边缘两角被填平, 下边缘两角保留圆角。
  ; 失败静默退回方角。
  static _RoundBottom(w, h) {
    global ED_GUI, ED_RADIUS
    r := ED_RADIUS
    hwnd := ED_GUI.Hwnd
    if (hwnd = 0 || w < r || h < r)
      return

    rgn := DllCall("gdi32.dll\CreateRoundRectRgn", "int", 0, "int", 0, "int", w, "int", h, "int", 2 * r, "int", 2 * r, "ptr")
    if (rgn = 0)
      return
    ; 填平顶部 r 高整条, 把上边缘两角变直角 (联动命令框的平底), 下边缘两角仍是圆角
    strip := DllCall("gdi32.dll\CreateRectRgn", "int", 0, "int", 0, "int", w, "int", r, "ptr")
    if (strip != 0) {
      DllCall("gdi32.dll\CombineRgn", "ptr", rgn, "ptr", rgn, "ptr", strip, "int", 2) ; RGN_OR
      DllCall("gdi32.dll\DeleteObject", "ptr", strip)
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
}