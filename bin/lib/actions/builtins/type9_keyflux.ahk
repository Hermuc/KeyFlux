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
  if (InStr(endReason, "Match")) {
    char := SubStr(capsHook.Match, -1)
    ; try 包裹: 此处运行在热键线程且 StartInputHook 已 Suspend(true), 未捕获异常 =
    ; 错误弹窗 + 热键永久失效 (见 CommandInputHooks.ahk 的 CommandInputOnChar 注释)
    try CommandDisplay.EchoChar(, char)
    catch as e
      CommandInputHooks._log("EchoChar(Match) 异常: " e.Message)
    SetTimer(HideCaspAbbr, -1)
  } else {
    if (InStr(endReason, "EndKey")) {
      PostMessageToCpasAbbr(CANCEL_COMMAND_INPUT)
    } else {
      PostMessageToCpasAbbr(HIDE_COMMAND_INPUT)
    }
    ; 透传会话曾把焦点交给命令框 -> 会话结束把前台还给会话开始时的窗口 (不恢复则
    ; 用户打字继续漏进已隐藏的命令框或不响应)。历史形态会话命令框不在前台,
    ; ActivateBackend 的 WinActive 检查为 false, 零行为变更。
    CommandInputHooks.ActivateBackend()
  }

  if (capsHook.Match) {
    ; 命令体 (step.call.Call) 可能抛 (如 Run 目标无效): 捕获后记录, 不许弹窗+卡死
    try ExecCapslockAbbr(capsHook.Match)
    catch as e {
      CommandInputHooks._log("ExecCapslockAbbr('" capsHook.Match "') 异常: " e.Message)
      Tip("命令执行失败: " capsHook.Match, -2000)
    }
  }
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
 * 快速切换 (QuickSwitch): 跳转到推荐的首个候选文件夹 (默认热键 Ctrl+G)。
 * 薄壳: 仅转调编排层 QuickSwitchRun(); 不含任何窗口/枚举/磁盘逻辑。
 * 注: AHK 全局函数命名空间唯一, 故编排入口在 QuickSwitch.ahk 中名为 QuickSwitchRun,
 *     本薄壳独占 QuickSwitchGoto 这一名字 (生成端 callMap[9] 调用的即本函数)。
 */
QuickSwitchGoto() {
  QuickSwitchRun()
}
