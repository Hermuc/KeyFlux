#Warn All, Off
; #Warn All, Off: 本文件会被独立 /Validate、也可能被未含依赖的脚本 Include —— 那时类名
; 引用是「只读未赋值」变量, AHK v2 默认 #Warn 会弹**加载期对话框**阻塞进程
; (/ErrorStdOut 无法抑制; FuzzyStrategy.ahk 同款先例)。
/**
 * ResultList.ahk —— 命令框「结果列表面板」的引擎侧唯一端口。
 *
 * 命令框自 2026-10 起是自研 Rust 版 (command-input/), 列表由它**自己**在同一个圆角白框内
 * 向下长高绘制 —— 引擎这边只负责"把数据推进去 / 把高亮挪一下 / 收起来", 以及接收
 * 鼠标交互的回推。本文件就是这三条出、一条入的唯一出口。
 *
 * 消息契约 (与 command-input/src/config.rs 的同名常量逐值一致):
 *   0x406 WM_COPYDATA  整表替换 (lParam = COPYDATASTRUCT, dwData = 载荷魔数)
 *                       wParam = **本脚本窗口** —— 命令框记下它作为回推目标
 *   0x407              移动高亮 (wParam = 1 基行号; 0 = 无高亮)
 *   0x408              收起列表 (窗口回落到基准高)
 *   0x409 (入)         命令框 → 引擎的回推: wParam = 1 基行号, lParam = 1 点选 / 2 悬停
 *
 * 🔴 载荷版本 (KFR3): [magic 'KFR3'][selected i32][count u32][style u32] 后跟
 *   count 个 [title_len][title UTF-8][sub_len][sub UTF-8]。逐字节镜像
 *   `results::encode_payload` —— 那一侧有单测锁字节, 本侧由 tests/result_list_probe.ahk
 *   用同一组样例比对 (两端任何一侧改动而另一侧没跟, 探针必红)。
 *
 * 为什么 style 是**这一批结果**的属性而不是单行属性: 行高/基线/有无图标都跟着它走,
 * 混排必然散。取值见 `STYLE_*`。
 *
 * 为什么 SendMessageTimeout 而不是 PostMessage: WM_COPYDATA 的载荷由系统在**消息存活
 * 期内**编组, Post 会立刻返回, 缓冲随即被复用 ⇒ 对端读到的可能是垃圾。限时 800ms:
 * 命令框侧解码/重排是亚毫秒级, 超时即它已卡死, 引擎不该被拖住 (返回 0 = 失败)。
 *
 * 与 everything_search 插件的关系: 插件**仍按 KFR2 发**(双行) 且自带一份编码器, 两者
 * 可以并存 —— 命令框同时接受 KFR3/KFR2。0x409 用 OnMessage 注册**函数对象**, AHK v2
 * 允许同一消息号挂多个回调 (实测/文档), 故本文件的接收器不会挤掉插件的; 双方各自按
 * "我的列表是否在显示" 过滤即可 (同一时刻只有一个列表)。
 */
class ResultList {
  ; ---- 协议常量 (与 command-input/src/config.rs 逐值一致) ----
  static MSG_DATA := 0x004A        ; WM_COPYDATA
  static MSG_SELECT := 0x0407
  static MSG_CLEAR := 0x0408
  static NOTIFY := 0x0409
  ; 'KFR3' 小端 u32
  static MAGIC := 0x3352464B
  ; 行版式 (与 results::RowStyle 同值)
  static STYLE_STACKED := 0        ; 双行 + 左侧图标 (文件搜索)
  static STYLE_INLINE := 1         ; 单行 + 右侧备注 (缩写候选)

  static SEND_TIMEOUT_MS := 800
  ; SmtoFlags = 0x0008 (SMTO_NOTIMEOUTIFNOTHUNG): 对方不 hung 就一直等 —— 与
  ; everything_search 插件同款取值 (那里已实测可用), 不自创。
  static SMTO_FLAGS := 0x0008

  ; 行交互回调 (可选): (row1based, kind) => ...  kind: 1 点选 / 2 悬停
  static Handler := 0
  static _notifyWired := false

  ; ============================================================
  ; 出: 推列表 / 移高亮 / 收起
  ; ============================================================

  /**
   * 整表替换并展开列表。
   * @param items   数组, 每项 = [title, subtitle]
   * @param selected 1 基行号; 0 = 无高亮
   * @param style   STYLE_*
   * @returns {boolean} 命令框是否确认收到 (false ⇒ 调用方应回落其它显示方式)
   */
  static Show(items, selected := 1, style := 1) {
    if (items.Length = 0) {
      this.Hide()
      return false
    }
    pkg := this.BuildPayload(items, selected, style)
    if (!IsObject(pkg))
      return false
    hwnd := this.Hwnd()
    if (!hwnd)
      return false
    ; COPYDATASTRUCT 布局 (x64 有对齐填充): dwData@0, cbData@A_PtrSize, lpData@(对齐后)。
    ; 🔴 不能用 A_PtrSize+4 当 lpData 偏移 —— 64 位下 cbData(4B) 后补 4B 填充, lpData 在 16。
    cds := Buffer((A_PtrSize = 8) ? 24 : 12, 0)
    NumPut("Ptr", this.MAGIC, cds, 0)
    ; 发送长度用 pkg.size (不含 StrPut 尾部 NUL); Buffer 多分配的 1 字节只给 NUL 用
    NumPut("UInt", pkg.size, cds, A_PtrSize)
    NumPut("Ptr", pkg.buf.Ptr, cds, (A_PtrSize = 8) ? 16 : 8)
    r := 0
    ; 🔴 用 `try { }` 块形: 多行 try 体后面直接跟 `catch` 时, 仓库的风格检查器只看
    ;   try 的**下一行**(那行是续行的实参), 会误判成"try 无 catch"。块形可被正确识别。
    try {
      DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", this.MSG_DATA
          , "ptr", A_ScriptHwnd, "ptr", cds, "uint", this.SMTO_FLAGS
          , "uint", this.SEND_TIMEOUT_MS, "ptr*", &r := 0)
    } catch as e {
      EngineLogWarn("ResultList.Show", e.Message)   ; 投递失败 = 调用方回落其它显示方式
    }
    this.InstallNotify()
    return (r != 0)
  }

  /**
   * 只移动高亮 (0x407)。命令框**不会**回推本消息 (单向, 防回声环)。
   * @param index 1 基; 0 = 无高亮
   */
  static Select(index) {
    hwnd := this.Hwnd()
    if (!hwnd)
      return false
    idx := (index >= 1) ? index - 1 : -1
    r := 0
    try {
      DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", this.MSG_SELECT
          , "ptr", idx, "ptr", 0, "uint", this.SMTO_FLAGS
          , "uint", this.SEND_TIMEOUT_MS, "ptr*", &r := 0)
    } catch as e {
      EngineLogWarn("ResultList.Select", e.Message)
    }
    return (r != 0)
  }

  /** 收起列表 (0x408; 窗口回落到基准高)。幂等 —— 本来没列表时命令框零动作。 */
  static Hide() {
    hwnd := this.Hwnd()
    if (!hwnd)
      return false
    r := 0
    try {
      DllCall("user32\SendMessageTimeoutW", "ptr", hwnd, "uint", this.MSG_CLEAR
          , "ptr", 0, "ptr", 0, "uint", this.SMTO_FLAGS
          , "uint", this.SEND_TIMEOUT_MS, "ptr*", &r := 0)
    } catch as e {
      EngineLogWarn("ResultList.Hide", e.Message)
    }
    return (r != 0)
  }

  /**
   * 载荷编码 (逐字节镜像 `results::encode_payload`)。
   * @returns {Object|0} {buf, size}; 0 = 超限 (调用方当作失败)
   */
  static BuildPayload(items, selected, style) {
    n := items.Length
    meta := []
    total := 16   ; magic(4) + selected(4) + count(4) + style(4)
    for it in items {
      tb := StrPut(it[1], "UTF-8") - 1   ; StrPut 含 NUL ⇒ 减 1 得裸字节数
      sb := StrPut(it[2], "UTF-8") - 1
      meta.Push([tb, sb])
      total += 4 + tb + 4 + sb
    }
    if (total > 4 * 1024 * 1024)
      return 0
    ; 多分配 1 字节: StrPut 写地址时恒追加 NUL, 按裸长度分配会让最后一串越界 1 字节
    buf := Buffer(total + 1, 0)
    NumPut("UInt", this.MAGIC, buf, 0)
    NumPut("Int", (selected >= 1) ? selected - 1 : -1, buf, 4)
    NumPut("UInt", n, buf, 8)
    NumPut("UInt", style, buf, 12)
    off := 16
    i := 1
    for it in items {
      tb := meta[i][1]
      sb := meta[i][2]
      NumPut("UInt", tb, buf, off)
      off += 4
      StrPut(it[1], buf.Ptr + off, "UTF-8")
      off += tb
      NumPut("UInt", sb, buf, off)
      off += 4
      StrPut(it[2], buf.Ptr + off, "UTF-8")
      off += sb
      i += 1
    }
    return { buf: buf, size: total }
  }

  ; ============================================================
  ; 入: 命令框回推 (0x409)
  ; ============================================================

  /** 安装回推接收器 (幂等)。用**函数对象**注册 —— 不占用"传统"单一监听名额。 */
  static InstallNotify() {
    if (this._notifyWired)
      return
    this._notifyWired := true
    try OnMessage(this.NOTIFY, ResultListNotifyForward)
    catch as e
      EngineLogWarn("ResultList.InstallNotify", e.Message)
  }

  /** @param row1 1 基行号; @param kind 1 点选 / 2 悬停 */
  static _Dispatch(row1, kind) {
    if (!IsObject(this.Handler))
      return
    try this.Handler.Call(row1, kind)
    catch as e
      EngineLogWarn("ResultList._Dispatch", e.Message)
  }

  ; ============================================================
  ; 窗口定位
  ; ============================================================

  /** 命令框窗口句柄 (隐藏态也算)。0 = 找不到 (未启动/已退出)。 */
  static Hwnd() {
    temp := A_DetectHiddenWindows
    DetectHiddenWindows(true)
    h := 0
    try h := WinExist(CommandInputWin.FULL)
    catch as e
      EngineLogWarn("ResultList.Hwnd", e.Message)   ; 找不到窗口 ⇒ h 保持 0 (调用方当作失败)
    DetectHiddenWindows(temp)
    return h
  }
}

; OnMessage 不接受类静态方法直传 (AHK v2 实测 ValueError: Invalid callback),
; 故用文件级函数转发 —— 与 everything_search 的 EverythingResultsNotifyForward 同款。
ResultListNotifyForward(wParam, lParam, msg, hwnd) {
  ResultList._Dispatch(wParam, lParam)
}
