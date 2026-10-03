; ============================================================
; EverythingIme —— 命令框 IME 上屏文本的捕获通道 (2026-10-03)。
;
; 为什么存在: 命令框透传模式下 (CommandImeGuard.UnlockForSearch), 中文上屏以 WM_CHAR
;   直达命令框窗口、不经 InputHook 回调 (design-ime-guard.md 记录的架构边界) —— 会话层
;   的 OnChar 只能拿到**拼音原始按键**, 于是检索词变成 "linshi" 而命令框显示 "临时",
;   搜索结果匹配拼音而非中文 (用户报障 2026-10-03)。本模块补上缺失的另一半:
;   从命令框窗口的**输入法上下文**读上屏字符串 (GCS_RESULTSTR), 交给会话层并入检索词。
;
; 跨进程原理: ImmGetContext 只对本线程窗口有效 —— 先 AttachThreadInput(本线程, 命令框
;   线程) 加入其输入队列, 再对命令框 hwnd 取 context。引擎与命令框同提权同桌面, 实测
;   可行性由生产会话验证 (本模块所有调用失败均静默降级: Open()=false ⇒ 会话层回落到
;   旧行为「拼音直入检索词」, 与历史一致, 不劣化)。
;
; 生命周期 (会话层驱动):
;   Attach()   进入搜索模式时调用 —— 建立线程附着 + 取 hwnd (幂等);
;   Detach()   会话结束时调用 —— 解除附着, 复位排水状态;
;   Open()     IME 是否处于打开状态 (打开 ⇒ 字母键是 IME 输入, 会话层不得并入检索词);
;   Composing() 组合串是否非空 (非空 ⇒ 回车是「上屏提交」, 不是「打开选中项」);
;   PendingResult() 排水上屏文本: 提交发生时返回一次结果串, 其余时刻返回 ""。
;   轮询由会话层 SetTimer 驱动 (40ms; 提交 → 检索词更新 → 重查)。
;
; 提交判定: (组合串空 ∧ 结果串非空 ∧ 未排干) ⇒ 返回一次。_drained 在「组合串非空」时
;   复位 —— 连续两次提交相同文本 (结果串内容相同) 也不会漏计。
; ============================================================

class EverythingIme {
  ; GCS_* 常量 (imm32.h)
  static GCS_COMPSTR := 0x0008
  static GCS_RESULTSTR := 0x0800

  static _attached := false
  static _hwnd := 0
  static _drained := false

  /** 建立与命令框线程的输入附着 (幂等; 失败静默, Open/Composing/PendingResult 全走降级)。 */
  static Attach() {
    if (this._attached)
      return
    hwnd := EverythingHost.CommandBoxWindow()
    if (!hwnd)
      return
    tidBox := 0
    DllCall("user32\GetWindowThreadProcessId", "ptr", hwnd, "uint*", &tidBox := 0)
    tidMine := DllCall("kernel32\GetCurrentThreadId", "uint", 0)
    if (!tidBox || tidBox = tidMine)
      return
    if (!DllCall("user32\AttachThreadInput", "uint", tidMine, "uint", tidBox, "int", true)) {
      try FileAppend(A_Now " everything_ime: AttachThreadInput failed`n", A_Temp "\kf_everything_ime.log", "UTF-8")
      return
    }
    this._hwnd := hwnd
    this._attached := true
    this._drained := false
  }

  /** 解除附着并复位 (幂等)。 */
  static Detach() {
    if (!this._attached)
      return
    tidBox := 0
    DllCall("user32\GetWindowThreadProcessId", "ptr", this._hwnd, "uint*", &tidBox := 0)
    tidMine := DllCall("kernel32\GetCurrentThreadId", "uint", 0)
    if (tidBox && tidBox != tidMine)
      DllCall("user32\AttachThreadInput", "uint", tidMine, "uint", tidBox, "int", false)
    this._attached := false
    this._hwnd := 0
    this._drained := false
  }

  /** IME 是否打开 (打开 = 字母/空格是 IME 输入, 会话层不得并入检索词)。失败 = false (降级)。 */
  static Open() {
    himc := this._Context()
    if (!himc)
      return false
    open := DllCall("imm32\ImmGetOpenStatus", "ptr", himc, "int")
    DllCall("imm32\ImmReleaseContext", "ptr", this._hwnd, "ptr", himc)
    return open ? true : false
  }

  /** 组合串是否非空 (非空 = 回车/空格是提交动作, 会话层不得当「打开选中项」处理)。 */
  static Composing() {
    return (this._Read(this.GCS_COMPSTR) != "")
  }

  /**
   * 排水上屏文本: 提交发生时返回一次结果串 (此后返回 "" 直至下一次提交)。
   * @returns {String} 上屏文本; 无提交/不可用 = ""
   */
  static PendingResult() {
    comp := this._Read(this.GCS_COMPSTR)
    result := this._Read(this.GCS_RESULTSTR)
    if (comp != "") {
      this._drained := false        ; 新组合开始: 允许下一次提交被排水
      return ""
    }
    if (result = "" || this._drained)
      return ""
    this._drained := true
    return result
  }

  ; ---- 内部 ----

  /** 取命令框的 IME 上下文 (须已 Attach); 失败返回 0 (调用方降级)。 */
  static _Context() {
    if (!this._attached || !this._hwnd)
      return 0
    himc := 0
    try himc := DllCall("imm32\ImmGetContext", "ptr", this._hwnd, "ptr")
    return himc
  }

  /** 读组合/结果串 (UTF-16)。失败或空 = ""。 */
  static _Read(gcs) {
    himc := this._Context()
    if (!himc)
      return ""
    len := 0
    try len := DllCall("imm32\ImmGetCompositionStringW", "ptr", himc, "uint", gcs, "ptr", 0, "uint", 0, "int")
    if (len <= 0) {
      DllCall("imm32\ImmReleaseContext", "ptr", this._hwnd, "ptr", himc)
      return ""
    }
    buf := Buffer(len + 2)
    try DllCall("imm32\ImmGetCompositionStringW", "ptr", himc, "uint", gcs, "ptr", buf, "uint", len)
    DllCall("imm32\ImmReleaseContext", "ptr", this._hwnd, "ptr", himc)
    return StrGet(buf, len, "UTF-16")
  }
}
