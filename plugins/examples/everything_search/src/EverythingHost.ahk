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
  ; 载荷 = WM_COPYDATA + 自定义 dwData 魔数 'KFR2' (二版: 每项 {标题, 路径} 双字段); 逐字节
  ; 格式定义在 `command-input/src/results.rs::encode_payload` (两端必须一致, 由该文件单测锁定),
  ; 跨进程由系统编组, 不共享指针。

  static WM_COPYDATA := 0x004A
  static MSG_SET_RESULTS := 0x004A      ; = WM_COPYDATA (dwData 区分归属)
  static MSG_SET_SELECTION := 0x0407    ; 只移动高亮 (wParam = 0 基下标; -1 = 无)
  static MSG_CLEAR_RESULTS := 0x0408    ; 收起列表 (窗口回落基准高度)
  static MSG_BADGE_SHOW := 0x040A       ; 显示搜索徽标 (wParam = 字形编号)
  static MSG_BADGE_HIDE := 0x040B       ; 隐藏搜索徽标
  static BADGE_MAGNIFIER := 1           ; 放大镜字形 (与 command-input/src/badge.rs 注册表同值)
  static PAYLOAD_MAGIC := 0x3252464B    ; 'K','F','R','2' 的小端 u32 (二版: 双字段/项)
  static MAX_PAYLOAD_BYTES := 4194304   ; 4MiB (与 command-input 的 MAX_PAYLOAD_BYTES 同值)
  static NO_SELECTION := -1             ; 载荷/0x407 的「无高亮」(与 command-input results.rs 同值)

  /**
   * 推送结果列表 (0x406)。
   * @param entries 条目数组 (每项 {t: 标题/文件名, s: 副标题/路径}; 顺序即列表顺序)
   * @param index 高亮行 (1 基; 0 = 无高亮 —— 提示行)
   * @returns {Boolean} 是否已送达 (命令框未运行 / 载荷构造失败 = false)
   */
  static ShowResults(entries, index) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ShowResults(entries, index)
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    pkg := EverythingHost.BuildResultsPayload(entries, index)
    if (!IsObject(pkg))
      return false
    ; COPYDATASTRUCT 布局 (x64 有对齐填充): dwData@0, cbData@A_PtrSize, lpData@(对齐后)。
    ; 🔴 不能用 A_PtrSize+4 当 lpData 偏移 —— 64 位下 cbData(4B) 后补 4B 填充, lpData 在 16。
    cds := Buffer((A_PtrSize = 8) ? 24 : 12, 0)
    NumPut("Ptr", EverythingHost.PAYLOAD_MAGIC, cds, 0)
    ; 🔴 发送长度用 pkg.size (不含 StrPut 尾部 NUL): Buffer 实际多分配 1 字节给 NUL
    ;   (StrPut 写地址时恒追加 NUL, 按裸长度分配会让最后一个字符串越界 1 字节 —— 2026-10-24 修)。
    NumPut("UInt", pkg.size, cds, A_PtrSize)
    NumPut("Ptr", pkg.buf.Ptr, cds, (A_PtrSize = 8) ? 16 : 8)
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

  ; ---- 0x40C 查询桥 (T0'; 2026-10-24) ----
  ;
  ; 语义: 引擎把整次查询交给命令框进程执行 (它内嵌 Everything 1.5 管道客户端), 取代
  ; "spawn es.exe → 导出文件 → 逐行解析 → 0x406 回推"的旧搬运链。
  ;   引擎 → 命令框: WM_COPYDATA, dwData = 'KFC1', 载荷 = [max u32][sort_prop u32]
  ;                 [sort_desc u32][q_len u32][q UTF-8]; wParam = A_ScriptHwnd (回推目标)。
  ;   命令框 → 引擎: **返回值** (非 0 = 成功) + 共享内存镜像段 (items 数据)。
  ; 为什么镜像走共享内存而不是回发消息: AHK 线程在 DllCall(SendMessage) 期间不泵消息,
  ; 命令框若同步 SendMessage 回推会死锁; PostMessage 又不能携带跨进程内存。命令框在
  ; 返回非 0 前已写好镜像段, 时序天然同步 (单写单读, 无需锁)。
  ; 降级: 任何失败 (框未运行/旧版 exe 不认识 KFC1/Everything 未运行/超时) 返回值 = 0
  ;   ⇒ 通道层报 ES_ERR_IPC ⇒ 编排层回落 es-cli/gui 旧通道, 行为与升级前一致。

  static IPC_QUERY_MAGIC := 0x3143464B    ; 'K','F','C','1' 的小端 u32 (与 command-input search/bridge.rs 同值)
  static IPC_MIRROR_MAGIC := 0x314D464B   ; 'K','F','M','1' 的小端 u32
  static IPC_MIRROR_NAME := "Local\KeyFlux-CommandInput-Search-v1"
  static IPC_SEND_TIMEOUT_MS := 3500      ; > 命令框内部 IO 超时 (3s); 正常查询 ~12ms
  static IPC_MAX_ITEMS := 100000          ; 镜像解析防御上限 (与 bridge.rs MAX_ITEMS 同值)

  /**
   * 执行一次 0x40C 查询。
   * @param query 检索词; @param maxResults 条数上限
   * @param sortProp IPC3 排序属性 ID (EverythingProviders.IpcSortSpec 产出)
   * @param sortDesc 是否降序
   * @returns {Array|0} items[{path,name,isFolder}]; 0 = 失败 (调用方回落旧通道)
   */
  static IpcQuery(query, maxResults, sortProp, sortDesc) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.IpcQuery(query, maxResults, sortProp, sortDesc)
    hwnd := this._FindBoxWindow()
    if (!hwnd) {
      EverythingHost._NoteIpcFail("找不到命令框窗口")
      return 0
    }
    pkg := EverythingHost.BuildIpcQueryPayload(query, maxResults, sortProp, sortDesc)
    if (!IsObject(pkg)) {
      EverythingHost._NoteIpcFail("载荷编码失败")
      return 0
    }
    cds := Buffer((A_PtrSize = 8) ? 24 : 12, 0)
    NumPut("Ptr", EverythingHost.IPC_QUERY_MAGIC, cds, 0)
    NumPut("UInt", pkg.size, cds, A_PtrSize)   ; 发送长度不含尾部 NUL (见 builder 注释)
    NumPut("Ptr", pkg.buf.Ptr, cds, (A_PtrSize = 8) ? 16 : 8)
    r := 0
    try DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", 0x004A
        , "ptr", A_ScriptHwnd, "ptr", cds, "uint", 0x0008
        , "uint", EverythingHost.IPC_SEND_TIMEOUT_MS, "ptr*", &r := 0)
    if (r = 0) {
      ; lastError=5 是 UIPI(权限不对等); 0 常见于对端没处理/超时/崩溃。
      EverythingHost._NoteIpcFail("发送失败 hwnd=" hwnd " lastError=" A_LastError
          " size=" pkg.size)
      return 0
    }
    items := EverythingHost.ReadIpcMirror()
    if (!IsObject(items))
      EverythingHost._NoteIpcFail("已收到应答(非0)但镜像段读不出")
    return items
  }

  /**
   * IPC 失败原因埋点 (2026-10-27)。只在**原因变化**时写一行, 避免逐键刷屏。
   *
   * 为什么需要: 0x40C 失败在会话层只表现为"回落 es-cli", 而失败点有四段
   * (找窗/编码/发送/读镜像), 不埋点就只能靠猜。日志与命令框侧的
   * kf-commandinput-ipc.log 配对使用 —— 一边记"哪一步没走通", 一边记"查询本身报了什么"。
   */
  static _LastIpcFail := ""

  static _NoteIpcFail(reason) {
    if (EverythingHost._LastIpcFail = reason)
      return
    EverythingHost._LastIpcFail := reason
    try FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") " [ipc-fail] " reason "`n"
        , A_Temp "\kf-everything-ipc-fail.log", "UTF-8")
  }

  /**
   * 构造 0x40C 载荷 (纯函数, 探针可测)。全小端:
   *   [0..4) 'KFC1' ; [4..8) max ; [8..12) sort_prop ; [12..16) sort_desc(0/1)
   *   [16..20) q_len ; [20..) query UTF-8
   * @returns {{buf:Buffer, size:Integer}|0} 0 = 编码失败。
   *   🔴 size 才是发送长度 (不含 StrPut 尾部 NUL; Buffer 多分配 1 字节)。
   */
  static BuildIpcQueryPayload(query, maxResults, sortProp, sortDesc) {
    try {
      qb := StrPut(query, "UTF-8") - 1
      buf := Buffer(20 + qb + 1, 0)
      NumPut("UInt", EverythingHost.IPC_QUERY_MAGIC, buf, 0)
      NumPut("UInt", maxResults, buf, 4)
      NumPut("UInt", sortProp, buf, 8)
      NumPut("UInt", sortDesc ? 1 : 0, buf, 12)
      NumPut("UInt", qb, buf, 16)
      if (qb > 0)
        StrPut(query, buf.Ptr + 20, "UTF-8")
      return {buf: buf, size: 20 + qb}
    } catch {
      return 0
    }
  }

  /** 打开并读取镜像段; @returns {Array|0} (段不存在/解析失败 = 0)。 */
  static ReadIpcMirror() {
    h := 0
    try {
      h := DllCall("kernel32\OpenFileMappingW", "uint", 0x0004, "int", false
          , "wstr", EverythingHost.IPC_MIRROR_NAME, "ptr")
    } catch as e {
      EverythingHost._NoteIpcFail("OpenFileMapping 抛异常: " e.Message)
      return 0
    }
    errOpen := A_LastError
    if (!h) {
      EverythingHost._NoteIpcFail("OpenFileMapping 失败 lastError=" errOpen
          " name=" EverythingHost.IPC_MIRROR_NAME)
      return 0
    }
    items := 0
    p := 0
    ; 🔴 末参数类型必须写 "UPtr"，不能写 "usize"（那是 Rust 的写法）：
    ;   AHK 的类型串白名单里没有 usize，DllCall 会**在调用前**抛异常；
    ;   由于外层有 try，异常被吞成"p=0 且 lastError=0"——看起来像系统拒绝映射，
    ;   实际一行代码都没执行到内核（2026-10-27 实测踩到，排查了两轮）。
    try p := DllCall("kernel32\MapViewOfFile", "ptr", h, "uint", 0x0004
        , "uint", 0, "uint", 0, "UPtr", 0, "ptr")
    errMap := A_LastError
    if (p)
      items := EverythingHost.ParseIpcMirrorAt(p)
    else
      EverythingHost._NoteIpcFail("MapViewOfFile 失败 lastError=" errMap)
    if (p)
      DllCall("kernel32\UnmapViewOfFile", "ptr", p)
    DllCall("kernel32\CloseHandle", "ptr", h)
    if (!IsObject(items) && p)
      EverythingHost._NoteIpcFail("镜像解析失败 (魔数/count 不符, 见 ParseIpcMirrorAt)")
    return items
  }

  /**
   * 解析镜像段 (纯函数, 探针可测 —— 传入自造 Buffer 的 Ptr)。
   * 格式 (全小端; 与 command-input search/bridge.rs encode_mirror 逐字节一致):
   *   [0..4) 'KFM1' ; [4..8) count u32
   *   重复 count 次: [flags UChar (bit0=目录)][nlen u32][name UTF-8][NUL][plen u32][path UTF-8][NUL]
   *   (字符串带不计入长度的 NUL 终止 —— AHK StrGet 按指针读时以 NUL 停)
   * @returns {Array|0} items[{path,name,isFolder}]
   */
  static ParseIpcMirrorAt(ptr) {
    try {
      magic := NumGet(ptr, 0, "UInt")
      if (magic != EverythingHost.IPC_MIRROR_MAGIC) {
        EverythingHost._NoteIpcFail("镜像魔数不符: got=" Format("{:#x}", magic)
            " expect=" Format("{:#x}", EverythingHost.IPC_MIRROR_MAGIC))
        return 0
      }
      count := NumGet(ptr, 4, "UInt")
      if (count > EverythingHost.IPC_MAX_ITEMS) {
        EverythingHost._NoteIpcFail("镜像 count 超限: " count)
        return 0
      }
      items := []
      off := 8
      loop count {
        flags := NumGet(ptr, off, "UChar")
        off += 1
        nlen := NumGet(ptr, off, "UInt")
        off += 4
        name := StrGet(ptr + off, "UTF-8")
        off += nlen + 1
        plen := NumGet(ptr, off, "UInt")
        off += 4
        path := StrGet(ptr + off, "UTF-8")
        off += plen + 1
        items.Push({path: path, name: name, isFolder: (flags & 1) = 1})
      }
      return items
    } catch {
      return 0
    }
  }

  /**
   * 构造 0x406 载荷。二版 'KFR2' (2026-10-04 Flow Launcher 双行版式), 全小端:
   *   [0..4) 魔数 'KFR2' ; [4..8) selected i32 ; [8..12) count u32 ;
   *   重复 count 次: [t_len u32][t UTF-8][s_len u32][s UTF-8]
   *   (t = 标题/文件名, s = 副标题/路径 —— 兼作命令框的图标提取键)
   * @returns {{buf:Buffer, size:Integer}|0} 0 = 构造失败 (超限 / 编码异常)。
   *   🔴 size 才是发送长度: Buffer 多分配 1 字节给 StrPut 恒追加的尾部 NUL
   *   (2026-10-24 修: 原按裸长度分配, 最后一个字符串的 NUL 会越界写 1 字节)。
   */
  static BuildResultsPayload(entries, index) {
    try {
      n := entries.Length
      meta := []
      total := 12
      for e in entries {
        tb := StrPut(e.t, "UTF-8") - 1     ; StrPut 返回含 NUL 的字节数
        sb := StrPut(e.s, "UTF-8") - 1
        meta.Push([tb, sb])
        total += 4 + tb + 4 + sb
      }
      if (total > EverythingHost.MAX_PAYLOAD_BYTES)
        return 0
      buf := Buffer(total + 1, 0)
      NumPut("UInt", EverythingHost.PAYLOAD_MAGIC, buf, 0)
      NumPut("Int", (index >= 1) ? index - 1 : EverythingHost.NO_SELECTION, buf, 4)
      NumPut("UInt", n, buf, 8)
      off := 12
      i := 1
      for e in entries {
        tb := meta[i][1]
        sb := meta[i][2]
        NumPut("UInt", tb, buf, off)
        if (tb > 0)
          StrPut(e.t, buf.Ptr + off + 4, "UTF-8")
        off += 4 + tb
        NumPut("UInt", sb, buf, off)
        if (sb > 0)
          StrPut(e.s, buf.Ptr + off + 4, "UTF-8")
        off += 4 + sb
        i += 1
      }
      return {buf: buf, size: total}
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

  /** 种子文本推入命令框 (WM_CHAR 逐码元, 框原生追加+显示; 每字符播键音 = 原版行为)。
   *
   * 🔴 2026-10-20 修类型串缺陷: 原写法 `NumGet(StrPtr(text), …, "u16")` 在 AHK v2 里是
   *   **非法类型串** ⇒ 每次调用都抛 `ValueError: Invalid parameter(s).`（实测：
   *   `"u16"` 抛错、`"UShort"` 返回 65）。它只在"确实取到种子"（有选中文字/文件）时
   *   才走到，故长期表现为"种子静默丢失"，仅引擎日志 logs\engine_error.log 里有
   *   `[EverythingHost.BoxSendText] … NumGet` 的 Return 栈可查（本机 2026-10-20 22:11
   *   一条实录即此因）。v2 的合法类型串是 "UChar"/"UShort"/"UInt"…，不存在 "u16"。
   */
  static BoxSendText(text) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.BoxSendText(text)
    hwnd := this._FindBoxWindow()
    if (!hwnd)
      return false
    len := StrLen(text)
    Loop len {
      u := EverythingHost.CodeUnitAt(text, A_Index - 1)   ; UTF-16 码元 (0 基)
      try PostMessage(0x0102, u, 0, hwnd)
      Sleep 10
    }
    return true
  }

  /**
   * 取字符串第 i 个 UTF-16 码元 (0 基)。
   * 单独抽出是为了**可测**: BoxSendText 需要真实命令框窗口，无窗口环境下测不了；而 2026-10-20
   * 那个长期静默的缺陷 (非法类型串 "u16") 恰恰只出在这一行 —— 抽成纯函数后由
   * tests/open_guard_probe.ahk 的组 18 直接断言码元值。
   * @param text 源串; @param i 0 基码元下标
   * @returns {Integer} 0..0xFFFF
   */
  static CodeUnitAt(text, i) {
    return NumGet(StrPtr(text), i * 2, "UShort")
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
