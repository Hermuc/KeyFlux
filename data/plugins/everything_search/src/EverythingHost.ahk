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
   * 命令框窗口句柄 (隐藏窗口也查 —— 命令框「存在但隐藏」是常态)。
   * @returns {Ptr} hwnd; 找不到 = 0。IME 捕获 (EverythingIme) 与锚点几何共用本查找。
   */
  static CommandBoxWindow() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.CommandBoxWindow()
    return this._FindBoxWindow()
  }

  /** 找命令框窗口: 先查可见, 再查隐藏。找不到 = 0。 */
  static _FindBoxWindow() {
    cls := "ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe"
    hwnd := 0
    try {
      hwnd := WinExist(cls)
      if (!hwnd) {
        DetectHiddenWindows true
        try hwnd := WinExist(cls)
        DetectHiddenWindows false
      }
    }
    return hwnd ? hwnd : 0
  }

  /**
   * 隐藏命令框窗口 (搜索模式下由查询输入面覆盖层接管显示; 会话结束引擎自行隐藏)。
   * 可重复调用。失败静默。
   */
  static HideCommandBox() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.HideCommandBox()
    hwnd := this._FindBoxWindow()
    if (hwnd)
      try WinHide(hwnd)
  }

  /**
   * 搜索激活 (0x404): 命令框摘除 NOACTIVATE、自取前台+焦点 ⇒ IME 组合进命令框。
   * 引擎 (提权/同用户) → 命令框: SendMessageTimeout 不受 UIPI 限制 (同完整性级别)。
   */
  static BoxActivateForSearch() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.BoxActivateForSearch()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    ; 🔴 前台锁豁免: 引擎的钩子收到了最近输入事件 → 引擎有权调 AllowSetForegroundWindow
    ;   授予命令框进程前台设置权限 → 命令框 0x404 处理器里的 SetForegroundWindow 才能成功
    ;   (2026-10-04 用户实测: 无此调用时框的自激活静默失败 → 无法输入)
    pid := 0
    DllCall("user32\GetWindowThreadProcessId", "ptr", hwnd, "uint*", &pid := 0)
    if (pid)
      DllCall("user32\AllowSetForegroundWindow", "uint", pid)
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", 0x0404
        , "ptr", 0, "ptr", 0, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return true
  }

  /** 读取命令框当前文本 (WM_GETTEXT, 系统跨进程编组; 含 IME 上屏中文)。 */
  static BoxGetText() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.BoxGetText()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return ""
    buf := Buffer(1024)
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", 0x000D
        , "ptr", 512, "ptr", buf, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return (r > 0) ? StrGet(buf, r, "UTF-16") : ""
  }

  /**
   * 前台移交给命令框 (引擎侧 SetForegroundWindow): 引擎的输入钩子收到了最近输入事件
   *   ⇒ 引擎调 SetForegroundWindow 有权成功 (Windows 前台锁的豁免条件);
   *   而命令框自身进程从未收到输入 ⇒ 它自己的 SetForegroundWindow 会被前台锁拒绝
   *   (2026-10-04 用户实测「输入不了文字」的根因: 0x404 只摘了 NOACTIVATE,
   *   但框进程的前台锁豁免不存在, SetForegroundWindow 静默失败 → 键盘继续流向原窗口)。
   * 前台移交给框后, 框线程队列获得键盘焦点 ⇒ IME 组合窗跟随命令框 ✓。
   */
  static BoxForeground() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.BoxForeground()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    try DllCall("user32\SetForegroundWindow", "ptr", hwnd)
    return true
  }

  /** 查询 IME 组合态 (0x405): 真 = 组合中 (回车是上屏提交, 引擎不得当「打开」)。 */
  static BoxQueryComposing() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.BoxQueryComposing()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", 0x0405
        , "ptr", 0, "ptr", 0, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return (r != 0)
  }

  /** 种子文本推入命令框 (WM_CHAR 逐码元, 框原生追加+显示; 每字符播键音 = 原版行为)。 */
  static BoxSendText(text) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.BoxSendText()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    len := StrLen(text)
    Loop len {
      u := NumGet(StrPtr(text), (A_Index - 1) * 2, "u16")   ; UTF-16 码元
      try PostMessage(0x0102, u, 0, hwnd)
      Sleep 10
    }
    return true
  }

  /**
   * 命令框**可见白框**锚点 (物理像素, 与 WinGetPos 同空间)。
   *
   * 🔴 几何为**实测常数** (2026-10-03, 图1 逐像素复测 + 2026-09-21 三次独立截图一致):
   *   可见白框 = 窗口矩形四周各缩 42px —— 925x200 窗口 → 841x116 白框
   *   (水平 42 与垂直 42 完全对称; 42px 即命令框 DWM 阴影 + 自绘透明外边距)。
   *   旧的启发式像素扫描已删除: 独立验证证明框体未渲染时扫描采到背景亮像素
   *   (inset=126 vs 真实 42), 采样不可靠; 常数更稳。
   *   ⚠ 若上游命令框的 DWM 阴影/皮肤 shadowSize 变化, 需重测此常数。
   * @returns {Object} {x, y, w, h, bottom} —— 可见白框左/上/宽/高/可见底边; 命令框不存在时返回 ""。
   */
  static CommandBoxAnchor() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.CommandBoxAnchor()
    if (this._AnchorCache != 0)
      return this._AnchorCache

    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return ""
    bx := 0, by := 0, bw := 0, bh := 0
    try WinGetPos(&bx, &by, &bw, &bh, hwnd)

    margin := 42   ; DWM 阴影 + 透明外边距, 四面对称 (实测, 见上)
    if (bw < margin * 2 + 180 || bh < margin * 2 + 40)
      return ""
    this._AnchorCache := {x: bx + margin, y: by + margin, w: bw - margin * 2, h: bh - margin * 2, bottom: by + bh - margin}
    return this._AnchorCache
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
