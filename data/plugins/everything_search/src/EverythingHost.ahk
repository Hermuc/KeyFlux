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

  ; ---- 结果列表面板端口 (命令框向下延伸的列表; 2026-10-04) ----
  ;
  ; 为什么经窗口消息而不是让插件自建窗口: 用户需求 —— 列表必须是命令框**本体的**
  ; 向下延伸 (Flow Launcher / uTools 形态)。命令框 (command-input/) 因此自身具备
  ; 「长高 + 绘制列表」的能力, 插件只把数据推过去。
  ; 与既有协议 (0x401-0x405) 同构: 命令框窗口的消息面就是本插件与它之间的唯一契约,
  ; 新增能力**只加消息, 不改既有语义**。
  ; 载荷 = WM_COPYDATA + 自定义 dwData 魔数 'KFR1'; 逐字节格式定义在
  ; `command-input/src/results.rs::encode_payload` (两端必须一致, 由该文件单测锁定),
  ; 跨进程由系统编组, 不共享指针。

  static WM_COPYDATA := 0x004A
  static MSG_SET_RESULTS := 0x004A      ; = WM_COPYDATA (dwData 区分归属)
  static MSG_SET_SELECTION := 0x0407    ; 只移动高亮 (wParam = 0 基下标; -1 = 无)
  static MSG_CLEAR_RESULTS := 0x0408    ; 收起列表 (窗口回落基准高度)
  static MSG_BADGE_SHOW := 0x040A       ; 显示搜索徽标 (wParam = 字形编号)
  static MSG_BADGE_HIDE := 0x040B       ; 隐藏搜索徽标
  static BADGE_MAGNIFIER := 1           ; 放大镜字形 (与 command-input/src/badge.rs 注册表同值)
  static PAYLOAD_MAGIC := 0x3152464B    ; 'K','F','R','1' 的小端 u32
  static MAX_PAYLOAD_BYTES := 4194304   ; 4MiB (与 command-input 的 MAX_PAYLOAD_BYTES 同值)
  static NO_SELECTION := -1             ; 载荷/0x407 的「无高亮」(与 command-input results.rs 同值)

  /**
   * 推送结果列表 (0x406)。
   * @param lines 展示文本数组 (顺序即列表顺序)
   * @param index 高亮行 (1 基; 0 = 无高亮 —— 提示行)
   * @returns {Boolean} 是否已送达 (命令框未运行 / 载荷构造失败 = false)
   */
  static ShowResults(lines, index) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ShowResults(lines, index)
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    buf := EverythingHost.BuildResultsPayload(lines, index)
    if (!IsObject(buf))
      return false
    ; COPYDATASTRUCT 布局 (x64 有对齐填充): dwData@0, cbData@A_PtrSize, lpData@(对齐后)。
    ; 🔴 不能用 A_PtrSize+4 当 lpData 偏移 —— 64 位下 cbData(4B) 后补 4B 填充, lpData 在 16。
    cds := Buffer((A_PtrSize = 8) ? 24 : 12, 0)
    NumPut("Ptr", EverythingHost.PAYLOAD_MAGIC, cds, 0)
    NumPut("UInt", buf.Size, cds, A_PtrSize)
    NumPut("Ptr", buf.Ptr, cds, (A_PtrSize = 8) ? 16 : 8)
    ; wParam = 本脚本窗口 (A_ScriptHwnd) —— 命令框记录它作为鼠标交互的回推目标;
    ; 用 SendMessageTimeout (限时 800ms): 命令框侧解码/重排极快, 超时说明它卡住,
    ; 此时插件不该被拖住 (返回 0 ⇒ false)。
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", EverythingHost.MSG_SET_RESULTS
        , "ptr", A_ScriptHwnd, "ptr", cds, "uint", 0x0008, "uint", 800, "ptr*", &r := 0)
    return (r != 0)
  }

  /** 只移动高亮 (0x407); 命令框**不会**回推本消息 (单向, 防回声环)。@param index 1 基; 0 = 无 */
  static SelectResult(index) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.SelectResult(index)
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    idx := (index >= 1) ? index - 1 : EverythingHost.NO_SELECTION
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", EverythingHost.MSG_SET_SELECTION
        , "ptr", idx, "ptr", 0, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return true
  }

  /** 收起结果列表 (0x408): 命令框窗口回落基准高度, 列表内容清空。 */
  static ClearResults() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ClearResults()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", EverythingHost.MSG_CLEAR_RESULTS
        , "ptr", 0, "ptr", 0, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return true
  }

  /**
   * 显示搜索徽标 (0x40A): 查询区右侧固定放大镜图标。
   * 解耦口径: 命令框只认**字形编号** (本插件传 BADGE_MAGNIFIER), 不知道 everything_search
   * 的存在 —— 本方法即插件侧的全部接入面, 移除插件 = 不再发送 0x40A/0x40B, 命令框零改动
   * (徽标另有「随会话清除」兜底, 插件崩溃也不会残留图标)。
   * @returns {Boolean} 是否已送达
   */
  static ShowBadge() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ShowBadge()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", EverythingHost.MSG_BADGE_SHOW
        , "ptr", EverythingHost.BADGE_MAGNIFIER, "ptr", 0, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return true
  }

  /** 隐藏搜索徽标 (0x40B)。会话收尾 (Close) 时调用; 命令框侧对 0x401/0x402/0x403 也有兜底清除。 */
  static HideBadge() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.HideBadge()
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", EverythingHost.MSG_BADGE_HIDE
        , "ptr", 0, "ptr", 0, "uint", 0x0008, "uint", 300, "ptr*", &r := 0)
    return true
  }

  /**
   * 构造 0x406 载荷 (Buffer)。格式 (全小端):
   *   [0..4) 魔数 'KFR1' ; [4..8) selected i32 ; [8..12) count u32 ;
   *   重复 count 次: [len u32][len 字节 UTF-8]
   * @returns {Buffer|0} 0 = 构造失败 (超限 / 编码异常) —— 调用方据此拒绝发送
   */
  static BuildResultsPayload(lines, index) {
    try {
      n := lines.Length
      lens := []
      total := 12
      for l in lines {
        b := StrPut(l, "UTF-8") - 1     ; StrPut 返回含 NUL 的字节数
        lens.Push(b)
        total += 4 + b
      }
      if (total > EverythingHost.MAX_PAYLOAD_BYTES)
        return 0
      buf := Buffer(total, 0)
      NumPut("UInt", EverythingHost.PAYLOAD_MAGIC, buf, 0)
      NumPut("Int", (index >= 1) ? index - 1 : EverythingHost.NO_SELECTION, buf, 4)
      NumPut("UInt", n, buf, 8)
      off := 12
      i := 1
      for l in lines {
        b := lens[i]
        NumPut("UInt", b, buf, off)
        if (b > 0)
          StrPut(l, buf.Ptr + off + 4, "UTF-8")
        off += 4 + b
        i += 1
      }
      return buf
    } catch {
      return 0
    }
  }

  /** 找命令框窗口: 先查可见, 再查隐藏。找不到 = 0。
   *  (内部 9 处调用方直接用本方法; 曾有公开包装 CommandBoxWindow 因零调用方于 2026-10-04 移除)。
   */
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
