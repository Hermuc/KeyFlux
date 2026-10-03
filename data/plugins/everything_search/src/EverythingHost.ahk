; ============================================================
; EverythingHost —— 引擎依赖的**唯一端口** (Ports & Adapters)。
;
; 为什么要有这一层: 本插件与引擎的耦合原本散落在 3 个文件、13 处直接引用
;   (CommandInputHooks / CommandDisplay / CommandImeGuard / SelectionContext /
;   SysLangIsChinese)。引擎改任何一个符号名, 都要全插件 grep。收敛到本类后:
;   * 引擎 API 演进 = 只改本文件;
;   * 插件其余部分只认识 EverythingHost, 不知道引擎存在;
;   * 探针/单测可整缝替换 (Impl), 不再需要逐个 stub 引擎全局类
;     (先例 = EverythingExplorerRunner.Impl, 见其注释)。
;
; 替换纪律 (与 EverythingExplorerRunner 同款):
;   * Impl = 0           → 走原生引擎全局 (生产路径);
;   * Impl = <对象>      → 同名方法派发到该对象 (探针记录/断言)。
;   🔴 类静态字段必须先落到局部变量再调用: `Class.Field(...)` 在 AHK v2 里按**方法调用**
;     解析, 字段值是函数对象时会报 "Too many parameters passed to function." (实测 2.0.19)。
;
; 原生方法**不做** try/catch: 引擎符号缺失应尽早炸出来 (加载/注册期), 而不是被静默吞掉 ——
; 与「插件出错由 PluginManager 隔离」的口径一致。
; (扫描方法例外: 它们本就自带 try/catch, 失败走几何兜底 —— 采样失败不该拖垮检索。)
; ============================================================

class EverythingHost {
  static Impl := 0        ; 0 = 未替换 (走引擎全局); 其余 = 同名方法的可调用对象

  ; ---- 命令框锚点 (可见白框几何) ----
  ;
  ; 为什么在端口层: 命令框的窗口类名 / DWM 阴影边距 / 透明底边都属于「命令框内部知识」,
  ; 原先散在渲染层 (EverythingDropdown) 里 —— 渲染层因此无法复用到别的锚点。收敛到这里后
  ; 渲染层只消费 {x, y, w, bottom}, 对「命令框是什么」零知识。
  ;
  ; 会话级缓存: 像素扫描 (BitBlt) 每次约几 ms, 而检索是**逐击键**的 —— 每击键扫一次纯属
  ; 浪费。命令框会话期间位置不变, 故首查缓存、会话开始 (EverythingSession.__New) 复位。

  static _AnchorCache := 0

  /** 复位锚点缓存 (每会话开始调用; 命令框位置跨会话可能改变)。 */
  static ResetAnchorCache() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ResetAnchorCache()
    this._AnchorCache := 0
  }

  /**
   * 命令框**可见白框**锚点 (物理像素, 与 WinGetPos 同空间)。
   * @returns {Object} {x, y, w, bottom} —— 可见白框左/上/宽/可见底边; 命令框不存在时返回 ""。
   */
  static CommandBoxAnchor() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.CommandBoxAnchor()
    if (this._AnchorCache != 0)
      return this._AnchorCache

    ; ---- 找窗口: 先查可见, 再查隐藏 (命令框「存在但隐藏」是常态, 见旧 _AnchorRect 注释) ----
    cls := "ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe"
    bx := 0, by := 0, bw := 0, bh := 0, found := false
    try {
      hwnd := WinExist(cls)
      if (hwnd) {
        WinGetPos(&bx, &by, &bw, &bh, hwnd)
        found := true
      } else {
        DetectHiddenWindows true
        try hwndH := WinExist(cls)
        DetectHiddenWindows false
        if (IsSet(hwndH) && hwndH) {
          WinGetPos(&bx, &by, &bw, &bh, hwndH)
          found := true
        }
      }
    }
    if (!found)
      return ""

    ; ---- 水平: 可见白框左右边缘 (像素扫描; 失败用实测阴影边距兜底) ----
    ; 🔴 窗口矩形含 DWM 阴影透明外边距 (实测单侧 42px), 直接用会让浮层比白框宽 84px
    ;   (2026-09-21 用户三次报障的根因, 几何依据见 _VisibleRectFallback)。
    cx := bx, cw := bw
    vis := this._ScanVisibleLeftRight(bx, by, bw, bh)
    if (vis.left >= 0 && vis.right > vis.left) {
      cx := vis.left
      cw := vis.right - vis.left + 1
    } else if (bw > 42 * 2 + 180) {
      cx := bx + 42
      cw := bw - 42 * 2
    }

    ; ---- 垂直: 可见白框底边 = 窗口底边 - 透明底边 (像素采样; 失败/异常用实测兜底 24) ----
    inset := this._ScanBottomInset(bx, by, bw, bh)
    ; 🔴 夹取护栏 (2026-10-03): 底边扫描是「从窗口底边向上找近白行」的启发式 —— 框体未
    ;   渲染 (如独立拉起的命令框进程) 时会采到**背景亮像素**, 给出远大于真实值的 inset
    ;   (独立验证实测 126 vs 真实 ≈24), 浮层因此盖进框体下半部。正常渲染的命令框其透明
    ;   底边只有几十物理像素, 故 inset 超出 [4,60] 一律视为坏采样回落 24 (与旧实现的
    ;   「封顶」护栏同目的: 坏采样不允许把浮层推进框体)。
    if (inset < 4 || inset > 60)
      inset := 24

    this._AnchorCache := {x: cx, y: by, w: cw, bottom: by + bh - inset}
    return this._AnchorCache
  }

  ; ---- 以下两个扫描方法自原 EverythingDropdown 原样迁入 (DPI 空间结论等注释见原文件 git 历史) ----

  /** 像素扫描可见白框左右边缘; 失败返回 {left:-1, right:-1}。 */
  static _ScanVisibleLeftRight(bx, by, bw, bh) {
    try {
      if (!IsNumber(bx) || !IsNumber(by) || bw < 40 || bh < 20)
        return {left: -1, right: -1}
      px := Round(bx), py := Round(by), pw := Round(bw), ph := Round(bh)
      if (pw < 40 || ph < 20)
        return {left: -1, right: -1}
      hdcScr := DllCall("user32.dll\GetDC", "ptr", 0, "ptr")
      hdcMem := DllCall("gdi32.dll\CreateCompatibleDC", "ptr", hdcScr, "ptr")
      hbmp := DllCall("gdi32.dll\CreateCompatibleBitmap", "ptr", hdcScr, "int", pw, "int", ph, "ptr")
      DllCall("gdi32.dll\SelectObject", "ptr", hdcMem, "ptr", hbmp)
      DllCall("gdi32.dll\BitBlt", "ptr", hdcMem, "int", 0, "int", 0, "int", pw, "int", ph
            , "ptr", hdcScr, "int", px, "int", py, "uint", 0x00CC0020)
      maxScan := pw // 4
      if (maxScan < 8)
        maxScan := 8
      lefts := [], rights := []
      fy := Round(ph * 0.30)
      while (fy <= Round(ph * 0.70)) {
        L := -1
        xx := 1
        while (xx <= maxScan) {
          c := DllCall("gdi32.dll\GetPixel", "ptr", hdcMem, "int", xx, "int", fy, "uint")
          r := c & 0xFF, g := (c >> 8) & 0xFF, b := (c >> 16) & 0xFF
          if (r >= 200 && g >= 200 && b >= 200) {
            L := xx
            break
          }
          xx += 1
        }
        R := -1
        xx := 1
        while (xx <= maxScan) {
          xr := pw - 1 - xx
          if (xr < 1)
            break
          c := DllCall("gdi32.dll\GetPixel", "ptr", hdcMem, "int", xr, "int", fy, "uint")
          r := c & 0xFF, g := (c >> 8) & 0xFF, b := (c >> 16) & 0xFF
          if (r >= 200 && g >= 200 && b >= 200) {
            R := xr
            break
          }
          xx += 1
        }
        if (L >= 0 && R > L) {
          lefts.Push(L)
          rights.Push(R)
        }
        fy += (ph // 16 > 0) ? (ph // 16) : 4
      }
      DllCall("gdi32.dll\DeleteObject", "ptr", hbmp)
      DllCall("gdi32.dll\DeleteDC", "ptr", hdcMem)
      DllCall("user32.dll\ReleaseDC", "ptr", 0, "ptr", hdcScr)
      if (lefts.Length = 0)
        return {left: -1, right: -1}
      lefts.Sort()
      rights.Sort()
      mi := (lefts.Length + 1) // 2
      Lm := lefts[mi], Rm := rights[mi]
      if (Rm <= Lm)
        return {left: -1, right: -1}
      return {left: bx + Lm, right: bx + Rm}
    }
    return {left: -1, right: -1}
  }

  /** 像素采样命令框「窗口底边 → 可见白底边」透明区高度; 失败返回 -1 (调用方兜底 24)。 */
  static _ScanBottomInset(bx, by, bw, bh) {
    try {
      if (!IsNumber(bx) || !IsNumber(by) || bw < 40 || bh < 20)
        return -1
      px := Round(bx), py := Round(by), pw := Round(bw), ph := Round(bh)
      if (pw < 40 || ph < 20)
        return -1
      hdcScr := DllCall("user32.dll\GetDC", "ptr", 0, "ptr")
      hdcMem := DllCall("gdi32.dll\CreateCompatibleDC", "ptr", hdcScr, "ptr")
      hbmp := DllCall("gdi32.dll\CreateCompatibleBitmap", "ptr", hdcScr, "int", pw, "int", ph, "ptr")
      DllCall("gdi32.dll\SelectObject", "ptr", hdcMem, "ptr", hbmp)
      DllCall("gdi32.dll\BitBlt", "ptr", hdcMem, "int", 0, "int", 0, "int", pw, "int", ph, "ptr", hdcScr, "int", px, "int", py, "uint", 0x00CC0020)
      cx := pw // 2
      inset := ph
      Loop 3 {
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
      if (inset < 2 || inset > bh)
        return -1
      return inset
    }
    return -1
  }

  ; ---- 引擎全局访问端口 ----

  /** 注册命令框输入钩子提供者。@returns 引擎 Register 的返回值 (false = 拦截点缺失/重复注册)。 */
  static RegisterCommandHook(ctrl) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.RegisterCommandHook(ctrl)
    return CommandInputHooks.Register(ctrl)
  }

  /** 把前台切回会话开始时的窗口 (取选中文字前调用, 见 Session.SeedFromSelection 注释)。 */
  static ActivateBackend() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ActivateBackend()
    CommandInputHooks.ActivateBackend()
  }

  /** 命令框视觉回显一个字符 (透传模式下物理键不经命令框, 显示靠投递)。 */
  static EchoChar(ih, char) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.EchoChar(ih, char)
    CommandDisplay.EchoChar(ih, char)
  }

  /** 命令框视觉回显退格。 */
  static EchoBackspace(ih, vk, sc) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.EchoBackspace(ih, vk, sc)
    CommandDisplay.EchoBackspace(ih, vk, sc)
  }

  /** 把前台还给命令框 (取完选中文字后调用; 失败只损失显示, 搜索路径不依赖焦点)。 */
  static ActivateCommandWindow() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ActivateCommandWindow()
    CommandDisplay.ActivateCommandWindow()
  }

  /** 进入搜索模式时放开中文输入 (KeyOpt 文本键透传, 允许中文检索)。 */
  static UnlockForSearch(ih) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.UnlockForSearch(ih)
    CommandImeGuard.UnlockForSearch(ih)
  }

  /**
   * 取当前选中内容。
   * @param wait 传给引擎 SelectionContext.Get (true = 必要时等待选区就绪)
   * @returns {Object} {type: "text"|"file", content: String}
   */
  static GetSelection(wait) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.GetSelection(wait)
    return SelectionContext.Get(wait)
  }

  /** 系统语言是否中文 (插件词表的语言判定依据)。首调固定的语义由调用方 (Messages) 持有。 */
  static IsChinese() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.IsChinese()
    return SysLangIsChinese()
  }
}
