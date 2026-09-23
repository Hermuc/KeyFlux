/**
 * TypeID 9: KeyFlux 自身动作
 * 拆自 Actions.ahk (模块化重构阶段 3), 函数体逐行搬运未做任何修改。
 */

/**
 * CapsLock 命令框
 *
 * 🔴 hook 不是启动时固定创建的单一对象, 而是每会话经 MakeCapsHook() 动态创建
 *   (InputHook 对象一次性): 透传模式 (ImeInputHost 启用时恒如此) 下以 V 可见形态
 *   建立 —— 物理键透传给命令框窗口, 英文原生显示 / IME 原生组合上屏 (§3.12 v4)。
 *   ImeInputHost.OnSessionBegin 已置 CommandDisplay.SuppressKeycap, 本函数在其后
 *   调用 MakeCapsHook() 读该标志定形态。
 *
 * 🔴 焦点修复 (2026-09-19 v4.1, §3.12 硬约束 6): 命令框窗口带 WS_EX_NOACTIVATE,
 *   SHOW 只改可见性不带来键盘焦点 —— 透传的物理键按「焦点窗口」路由, 会全部打进
 *   会话开始时的原文本框 (用户真机实测: 英文无法输入 + 焦点滞留)。故 SHOW 之后
 *   必须显式激活命令框 (CommandDisplay.ActivateCommandWindow); 激活失败则降级
 *   历史形态 (吞键 + 投递显示, 英文仍可用), 保证自洽回退。会话结束 (非 Match 分支)
 *   经 CommandInputHooks.ActivateBackend 把前台还给会话开始时的窗口。
 *
 * 🔴 收尾延后 (2026-09-20, §3.12 硬约束 9): 命中后不当场执行/隐藏 —— 见
 *   CommandInputHooks.FinishDelayMs。命中这一击的字符随物理键抵达命令框后需要一次绘制
 *   周期才显示; 当场执行 + 隐藏会让它永远看不见 (用户真机实测「最后一个字母不显示」)。
 */
EnterCapslockAbbr() {
  static WM_USER := 0x0400
  static SHOW_COMMAND_INPUT := WM_USER + 0x0001
  static HIDE_COMMAND_INPUT := WM_USER + 0x0002
  static CANCEL_COMMAND_INPUT := WM_USER + 0x0003

  ; 高级键盘设置 > 输入语言热键, 用户勾选了用 Shift 键关闭大写
  ; if GetKeyState("Shift", "P") {
  ;   Tip("bug: Shift key is pressed down")
  ;   return
  ; }

  ; 显示命令框窗口
  ; 先开会话 (记下当前前台窗口 —— 命令框显示后可能抢走前台, 插件取选中文字要切回去)
  CommandInputHooks.BeginSession()
  PostMessageToCpasAbbr(SHOW_COMMAND_INPUT)

  ; 🔴 焦点修复 (v4.1): 透传模式必须把键盘焦点显式交给命令框 (窗口 NOACTIVATE, SHOW
  ; 不带焦点, 物理键会打进原窗口 —— 探针 kf_focus_probe 实证); 激活失败降级历史形态
  ; (吞键+投递, 英文仍可用), OnSessionEnd 会复位标志, 降级自洽无泄漏。
  if (CommandDisplay.SuppressKeycap && !CommandDisplay.ActivateCommandWindow())
    CommandDisplay.SuppressKeycap := false

  ; 会话已开始 (含可能的降级) -> 此刻建 hook 才能拿到正确形态
  capsHook := MakeCapsHook()
  endReason := StartInputHook(capsHook)
  ; 输入结束: 让插件收起自建浮层 (下拉列表等)
  CommandInputHooks.EndSession()

  ; 🔴 命中后的收尾一律**延后** (2026-09-20, 见 CommandInputHooks.FinishDelayMs):
  ; 命中这一击的字符 (终止字符) 随物理键抵达命令框后需要一次绘制周期才显示; 旧行为在同线程
  ; 里立即「执行命令 + 隐藏命令框」⇒ 绘制还没发生窗口就没了, 用户看到「最后一个字母不显示,
  ; 命令被立即直接执行」(真机实测)。延后 150ms 让字符先显示, 命令仍无需用户确认即执行。
  if (InStr(endReason, "Match")) {
    char := SubStr(capsHook.Match, -1)
    ; try 包裹: 此处运行在热键线程且 StartInputHook 已 Suspend(true), 未捕获异常 =
    ; 错误弹窗 + 热键永久失效 (见 CommandInputHooks.ahk 的 CommandInputOnChar 注释)
    ; 🔴 终止字符必须**绕过 ShouldEcho** 投递 (2026-09-20 实测缺陷): 命中这一击就结束了会话,
    ; 该字符**不会被原生显示**; 而透传模式下 ShouldEcho 恒 false ⇒ 走 EchoChar 等于不投递
    ; ⇒ 用户看到「最后一个字母不显示」(铁证 = logs\command_input_hooks.log 连发
    ; `EchoChar(Match) 异常` —— 旧写法连实参个数都是错的)。故改走唯一允许绕过总开关的收口
    ; `CommandDisplay.EchoTerminalChar`; 它是该字符**唯一**的显示来源, 两种形态都必须投。
    try CommandDisplay.EchoTerminalChar(char)
    catch as e
      CommandInputHooks._log("EchoTerminalChar(Match) 异常: " e.Message)
    ; 全串命中: 命中文本经 capsHook.Match 传下去 (模糊路径改走 PendingAbbr)
    SetTimer(FinishCapslockAbbr.Bind(capsHook.Match, "capslock", false)
           , -CommandInputHooks.FinishDelayMs)
    return
  }

  if (InStr(endReason, "EndKey")) {
    PostMessageToCpasAbbr(CANCEL_COMMAND_INPUT)
    CommandInputHooks.ActivateBackend()
    return
  }

  ; 模糊命中 (FuzzySuffixFire 已停钩并记录) -> 同样延后收尾
  pending := CommandInputHooks.TakePending()
  if (IsObject(pending)) {
    SetTimer(FinishCapslockAbbr.Bind(pending[2], pending[1], true)
           , -CommandInputHooks.FinishDelayMs)
    return
  }

  ; 无命中的普通收尾 (其它停止原因)
  PostMessageToCpasAbbr(HIDE_COMMAND_INPUT)
  ; 透传会话曾把焦点交给命令框 -> 会话结束把前台还给会话开始时的窗口 (不恢复则
  ; 用户打字继续漏进已隐藏的命令框或不响应)。历史形态会话命令框不在前台,
  ; ActivateBackend 的 WinActive 检查为 false, 零行为变更。
  CommandInputHooks.ActivateBackend()
}

/**
 * 缩写命中后的「延后收尾」—— 先让命令框把终止字符画出来, 再执行命令、再隐藏窗口
 * (2026-09-20, §3.12 硬约束 9)。
 *
 * 🔴 为什么必须延后 (用户实测缺陷根因): 命中这一击的字符是随 V 透传抵达命令框窗口的
 *   (探针实测其 WM_KEYDOWN/KEYUP 与无钩形态完全一致 —— 键**没有**被 InputHook 拦下),
 *   但命令框需要一次绘制周期才会把该字符显示出来。旧行为同线程立即「执行命令 + 隐藏
 *   命令框」⇒ 绘制还没发生窗口就没了, 用户看到「最后一个字母不显示, 命令被立即直接
 *   执行」。延后 CommandInputHooks.FinishDelayMs (150ms) 让字符先显示; 命令仍无需用户
 *   确认即执行 —— 只把执行推迟到字符可见之后, 不改「打完即执行」语义。
 * 顺序与历史一致: 先执行命令、再隐藏命令框 (命令自带的窗口不被命令框遮挡)。
 * 期间用户若又开了新的命令框会话 (SessionActive=true) 则不隐藏, 免关掉新会话的窗口。
 *
 * @param abbr 命中的缩写命令 (空串 = 无命中, 直接返回)
 * @param scope "capslock" | "semicolon"
 * @param fuzzy true = 后缀模糊命中 (经 Resolve, 事件带 fuzzy 标记); false = 全串命中
 */
FinishCapslockAbbr(abbr, scope, fuzzy) {
  if (abbr = "")
    return
  ; 命令体 (step.call.Call) 可能抛 (如 Run 目标无效): 捕获后记录, 不许弹窗+卡死
  try {
    if (fuzzy)
      CommandResolver.Resolve(scope, abbr, , true)
    else
      ExecCapslockAbbr(abbr)
  } catch as e {
    CommandInputHooks._log("缩写命令执行失败 ('" abbr "', fuzzy=" (fuzzy ? 1 : 0) "): " e.Message)
    Tip("命令执行失败: " abbr, -2000)
  }
  ; 模糊命中的历史收尾顺序是「先执行、再归还前台」; 全串命中不归还 (命令体自会接管前台)
  if (fuzzy)
    CommandInputHooks.ActivateBackend()
  if (!CommandInputHooks.SessionActive)
    HideCaspAbbr()
}

/**
 * semi缩写框
 */
EnterSemicolonAbbr(semiHook, semiHookAbbrWindow) {
  semiHookAbbrWindow.Show(" ")
  endReason := StartInputHook(semiHook)
  if (InStr(endReason, "Match")) {
    char := SubStr(semiHook.Match, -1)
    semiHookAbbrWindow.Show(char, true)
    SetTimer(() => semiHookAbbrWindow.Hide(), -100)
  } else {
    semiHookAbbrWindow.Hide()
  }

  if (semiHook.Match)
    ExecSemicolonAbbr(semiHook.Match)
}

/**
 * 切换Capslock状态
 */
ToggleCapslock() {
  if GetKeyState("Alt", "P")
    send("{blind}{LCtrl}{LAlt Up}")
  send("{blind}{CapsLock}")
}

/**
 * 快速切换 (QuickSwitch): 跳转到推荐的首个候选文件夹。
 * 无默认热键 (2026-09-23 起 ^g 绑定已从配置移除, 触发靠轮询自动路径); 本入口经
 * 生成端 callMap[9] 保留, 供将来托盘/其它非热键通道复用。
 * 薄壳: 仅转调编排层 QuickSwitchRun(); 不含任何窗口/枚举/磁盘逻辑。
 * 注: AHK 全局函数命名空间唯一, 故编排入口在 QuickSwitch.ahk 中名为 QuickSwitchRun,
 *     本薄壳独占 QuickSwitchGoto 这一名字 (生成端 callMap[9] 调用的即本函数)。
 */
QuickSwitchGoto() {
  QuickSwitchRun()
}
