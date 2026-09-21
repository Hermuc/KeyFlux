/**
 * CommandDisplay.ahk —— 命令框 (KeyFlux-CommandInput) 字符投递的唯一收口。
 *
 * 为什么需要这层 (而不是各处直接调 PostCharToCaspAbbr):
 *
 * 命令框本体是上游预编译二进制 (bin/KeyFlux-CommandInput.exe, 无源码)。经反汇编与实测
 * 确认其绘制约束:
 *
 *   1. 命令框的显示 = **纯投递的 WM_CHAR (0x0102)**: capsHook 以 InputHook("", ...) 创建
 *      (无 V ⇒ 默认不可见, 吞掉文本键), 物理键到不了命令框窗口 —— 实测铁证: 上轮
 *      抑制投递后字母全部消失 (CONTRACTS §3.11)。
 *   2. keycap (八角框) 的描边与字符由**同一支** ID2D1SolidColorBrush 绘制
 *      (RTTI: SampleWindow::DrawKeys(..., ComPtr<ID2D1SolidColorBrush>& brush, ...)),
 *      exe 内不存在独立的边框绘制函数。皮肤 keyColor/keyOpacity 无法「只去框留字」
 *      (keyOpacity 是图层级, 置 0 连字符一起消失)。
 *
 * ============================ 八角框的移除: 数据 patch (§3.11) ============================
 * exe .rdata 里的 keycap 白名单 (a-zA-Z0-9, 62 字符 UTF-16LE, 文件偏移 0x1cca0) 是
 * 「哪些字符画八角框」的判定数据。已将其替换为不可匹配字符 U+0001 (保留长度, 前置校验
 * + 回读复核): 字母/数字走普通字形路径 —— **命令框内直接显示, 无八角框**。
 * (此前证伪的是"代码 patch"——NOP 掉白名单比对后的跳转会破坏窗口初始化; 改常量数据
 * 不动控制流, 性质完全不同。)
 *
 * ============================ SuppressKeycap: 透传模式总开关 (§3.12 v4.2) ============================
 * hook 以 InputHook("V", ...) 建可见形态 (ImeInputHost 启用时恒如此), 物理键透传到
 * 命令框窗口 —— 英文字母原生显示, 上屏中文以 WM_CHAR 直达 (非白名单无框)。透传已
 * 覆盖一切显示, 故 SuppressKeycap=true = **字符投递通道关闭**: EchoChar 恒 no-op。
 * 🔴 **退格不受本开关管辖** (2026-09-21 订正): 旧实现把退格一并拦停, 但 exe 的删除
 * 只认 WM_CHAR(0x08)、物理退格无原生删除路径 ⇒ 拦停 = 删除彻底失效 (用户真机报障
 * 「文字已删但命令框还显示」)。故 EchoBackspace **恒投**。
 * 缩写匹配两形态恒开 (v4.2 恢复: 词表 + FuzzySuffixFire —— 用户裁决:
 * 命令全英文字母, 中文意图仅在前置键触发之后, 那时字符已被插件消费, 到不了匹配层)。
 * providers 派发保留 (见 CommandInputHooks)。标志由 ImeInputHost.OnSessionBegin 置位
 * (启用时恒 true), OnSessionEnd 复位 false —— 复位后 hook 回历史形态 (吞键+投递), 自洽。
 * 🔴 v3 的「按 IME 开关状态置位」已证伪退役: 跨进程 IMM/TSF 查询皆不可行 (§3.12),
 * 查询恒 -1 ⇒ v3 的透传分支从未生效 ⇒ 中文打不出的直接死因。
 *
 * 契约:
 *   ShouldEcho(c)     -> bool   该字符是否应投递给命令框窗口
 *   EchoChar(ih, c)   -> bool   投递字符 (内部已过 ShouldEcho; 返回 true = 确实投递了)
 *   EchoTerminalChar(c) -> bool 强制投递「终止字符」—— **唯一**允许绕过 ShouldEcho 的通道
 *                               (命中那一击不会被原生显示, 见 §3.12 硬约束 9)
 *   EchoBackspace(ih) -> void   投递退格 —— **恒投**, 不受 SuppressKeycap 影响
 *                               (2026-09-21: exe 只认 WM_CHAR 的 0x08, 物理退格不删,
 *                                投递是唯一删除来源; 见该函数注释)
 *
 * 🔴 字符与退格的**不对称**是刻意的, 勿"统一" (2026-09-21 证):
 *   新增字符 = 物理键原生直显 (exe 的 WM_CHAR 0x102 分支) ⇒ 再投递会双显, 故 ShouldEcho 拦;
 *   删除字符 = 物理键无任何原生路径 (exe 无 WM_KEYDOWN 分支) ⇒ 不投递就永不删除, 故恒投。
 *
 * 快路径红线不适用: 本模块只在命令框输入期被调用, 不在重映射/发键/鼠标快路径上。
 */

class CommandDisplay {
  /**
   * 「透传模式」总开关 (§3.12 v4, 由 ImeInputHost 在会话内驱动):
   *   true  = 投递通道整体关闭 —— 物理键已随 V hook 透传并原生显示 (英文直显 / IME
   *           组合上屏), 任何投递都是二次显示; providers 派发仍照常 (插件功能不受影响)。
   *   false = 历史形态 —— 显示靠投递 (数据 patch 后无八角框), 字符/退格照常投。
   * 默认 false = 未启用透传时零行为变更。
   */
  static SuppressKeycap := false

  /**
   * keycap 白名单 (原 exe 行为): 仅 a-zA-Z0-9 会套八角框。白名单串已数据 patch 为
   * U+0001 (§3.11)。v4 起 ShouldEcho 全停投递, 本判定**不再参与任何运行时分支**,
   * 仅保留为排查对照 —— 与 exe 内烧录值不一致是有意为之, 勿"同步修复"。
   * @returns {boolean}
   */
  static IsKeycapChar(c) {
    if (c = "" || StrLen(c) != 1)
      return false
    n := Ord(c)
    return (n >= 0x30 && n <= 0x39)     ; 0-9
        || (n >= 0x41 && n <= 0x5A)     ; A-Z
        || (n >= 0x61 && n <= 0x7A)     ; a-z
  }

  /**
   * 该字符是否应投递给命令框窗口。
   * 透传模式 (true) 下**全部停投**, 不再区分白名单: 字符显示已由物理键透传原生完成
   * (字母直显 / IME 上屏中文直达), 插件与 Match 分支的任何投递都会变成二次显示。
   * 🔴 v3 只停白名单字符是错的: 中文 U+4E00+ 不在白名单, 会漏网双显。
   */
  static ShouldEcho(c) {
    if (this.SuppressKeycap)
      return false
    return true
  }

  /**
   * 投递字符到命令框 (已过 ShouldEcho 判定)。
   *
   * 🔴 **必须两个参数都传** (2026-09-20 事故记录): 全串命中分支曾按历史写法写成
   *   `EchoChar(, char)` (首参 ih 是历史遗留参数, 只转发给 PostCharToCaspAbbr 且不被消费),
   *   而本函数首参是必填 ⇒ 每次命中都在调用边界抛 `Missing a required parameter.`, 被
   *   紧随的 try/catch 吞掉 ⇒ **字符从未被投递** (用户实测「最后一个字母不显示」; 铁证 =
   *   部署树 `logs\command_input_hooks.log` 连发 `EchoChar(Match) 异常`)。
   *   ⇒ **终止字符不走本函数, 走 `EchoTerminalChar`** (它才是唯一允许绕过 ShouldEcho 的
   *   收口点, 见其注释); 本函数的调用处一律显式传两个实参。
   * @returns {boolean} true = 确实投递了; false = 透传模式被抑制跳过 (no-op)
   */
  static EchoChar(ih, c) {
    if (!this.ShouldEcho(c))
      return false
    PostCharToCaspAbbr(ih, c)
    return true
  }

  /**
   * 强制投递「终止字符」—— **唯一允许绕过 ShouldEcho 的回显通道** (§3.12 硬约束 9)。
   *
   * 🔴 为什么必须绕过总开关: 命中缩写的那一击就结束了会话, 该字符**不会被原生显示**
   * (透传模式下所有字符靠物理键原生直显, 唯独这一击没有 —— 它触发的是会话终止);
   * 而 ShouldEcho 在透传模式下恒 false ⇒ 若走 EchoChar 就永远不会投递 ⇒ 用户看到
   * 「最后一个字母不显示」(2026-09-20 用户实测 + 引擎日志坐实)。
   * 两个命中路径都经本函数补投: 全串命中 (EnterCapslockAbbr Match 分支, 无条件补) 与
   * 模糊命中 (FuzzySuffixFire, 仅透传模式补 —— 历史形态那边已由 OnChar 的 EchoChar 投过,
   * 再补会双显)。命令框对投递的 WM_CHAR 与物理键一视同仁 (同一收口, §3.11 硬约束 1)。
   * @returns {boolean} true = 已投递; false = 空串 (防御性, 不投)
   */
  static EchoTerminalChar(c) {
    if (c = "" || StrLen(c) < 1)
      return false
    PostCharToCaspAbbr("", c)
    return true
  }

  /**
   * 投递退格。
   *
   * 🔴 2026-09-21 修复「搜索模式退格: 文字已删但命令框还显示」(用户真机报障)。
   *   旧实现在透传模式下**整体拦停**退格, 理由是「物理退格已随 V hook 透传, 再投递即
   *   二次删除」。该前提经静态反汇编证伪 —— 命令框 exe (580KB) 全二进制只有**一处**
   *   WM_CHAR(0x0102) 比较点 (`81 fa 02 01 00 00` @ 0x9687), 且其退格分支
   *   (`66 83 fe 08` = wParam 0x08) **完全嵌套在该 WM_CHAR 分支内部**; 全二进制
   *   WM_KEYDOWN(0x0100) 比较点为 **0 个**。即:
   *     * exe 的删除**只**认「收到 WM_CHAR(wParam=0x08)」这一条路径;
   *     * 物理退格是 WM_KEYDOWN(VK_BACK=0x08), exe 对此**无任何原生删除行为**
   *       (KeyOpt "{Backspace}" "N" 无 S ⇒ V 形态下物理键确实透传到了窗口, 但窗口不处理);
   *     * 故物理退格**不产生删除**, 旧实现拦停投递 = 删除端**完全失效** ⇒ 用户看到的
   *       「query 已删 (插件逻辑删了) 但命令框仍显示原文字」。
   *   ⇒ 结论: 退格投递必须**恒开**。它是命令框显示端**唯一**的删除来源, 拦停即为缺陷。
   *
   * ⚠ 与「双删」的关系 (旧注释担忧的场景): 双删只在「物理键本身能删」时才会发生。
   *   按上述反汇编, exe 对 WM_KEYDOWN 不删, 而 `KeyOpt("{Backspace}", "N")` (无 S) 也
   *   只影响透传可见性、不产生命令框侧删除 ⇒ 不存在双删。唯一例外是 IME 组合期: 那时
   *   退格由 IME 消费删除组合串, 但组合串不在命令框显示文本内 (上屏才进), 两者作用域
   *   不重叠 ⇒ 仍无需拦停。故本函数**无 SuppressKeycap 分支**。
   *
   * 与字符投递 (EchoChar/ShouldEcho) 的不对称是**刻意**的, 勿"统一": 字符新增由物理键
   * 原生直显 (exe 处理 WM_CHAR 的 0x102 分支), 再投递才会双显; 而删除没有任何物理路径,
   * 只能靠投递。见 CONTRACTS §3.12 硬约束 2 的订正。
   */
  static EchoBackspace(ih, vk?, sc?) {
    PostBackspaceToCaspAbbr(ih, vk, sc)
  }

  /**
   * 把键盘焦点交给命令框窗口 (2026-09-19 焦点修复, §3.12 硬约束 6)。
   *
   * 🔴 为什么需要: 命令框窗口带 WS_EX_NOACTIVATE (kf_focus_probe 实测 exStyle=0x8200008,
   * NOACTIVATE=1 + TOPMOST), SHOW 消息只改变可见性、从不带来键盘焦点 —— v4 透传模式下
   * 物理键按「焦点窗口」路由, 焦点滞留在会话开始时的原文本框, 结果是「英文打不进命令框,
   * 全部漏进原窗口」(2026-09-19 用户真机实测)。历史形态不需要本函数: 吞键模式下显示靠
   * AHK 投递 WM_CHAR, 与焦点无关。
   *
   * 探针判定 (kf_focus_probe v5, 独立 AHK 进程):
   *   * WinActivate 可以把 NOACTIVATE+TOPMOST 的命令框带到前台 (winActiveAfterActivate=1);
   *   * WinActivate 失败时兜底 AttachThreadInput + SetFocus (GetGUIThreadInfo 回读
   *     focusHwnd=命令框)。本函数在热键线程执行 (用户输入授权), 前台激活权限比探针更强。
   *
   * 不触碰 SuppressKeycap —— 降级决策由调用方 (EnterCapslockAbbr 编排层) 做, 保持单一职责。
   *
   * @returns {boolean} true = 焦点已确认在命令框 (WinActive 回读); false = 激活失败
   *   (窗口未出现 / 激活与兜底都未生效), 调用方应降级历史形态。
   */
  static ActivateCommandWindow() {
    ; SHOW 是跨进程 PostMessage, 命令框窗口出现有处理延迟, 先等窗口真正可见
    hwnd := 0
    try hwnd := WinWait("ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe", , 0.5)
    if (!hwnd)
      return false
    try WinActivate("ahk_id" hwnd)
    Loop 20 {   ; 最多 ~400ms: 等激活生效 (NOACTIVATE 窗口激活可能慢一拍)
      if WinActive("ahk_id" hwnd)
        return true
      Sleep(20)
    }
    ; 兜底: 跨线程 SetFocus (不受前台激活锁限制; 探针实证 GetGUIThreadInfo 可回读验证)
    tid := DllCall("GetWindowThreadProcessId", "ptr", hwnd, "ptr*", 0, "uint")
    cur := DllCall("GetCurrentThreadId", "uint")
    DllCall("AttachThreadInput", "uint", cur, "uint", tid, "int", 1, "int")
    DllCall("SetFocus", "ptr", hwnd, "ptr")
    DllCall("AttachThreadInput", "uint", cur, "uint", tid, "int", 0, "int")
    Loop 10 {   ; 再等 ~200ms
      if WinActive("ahk_id" hwnd)
        return true
      Sleep(20)
    }
    return false
  }

  /** 复位 (引擎退出时调用, 避免状态泄漏)。 */
  static Reset() {
    this.SuppressKeycap := false
  }
}
