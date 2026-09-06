/**
 * TypeID 9: KeyFlux 自身动作
 * 拆自 Actions.ahk (模块化重构阶段 3), 函数体逐行搬运未做任何修改。
 */

/**
 * CapsLock 命令框
 */
EnterCapslockAbbr(capsHook) {
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
  PostMessageToCpasAbbr(SHOW_COMMAND_INPUT)

  endReason := StartInputHook(capsHook)
  if (InStr(endReason, "Match")) {
    char := SubStr(capsHook.Match, -1)
    PostCharToCaspAbbr(, char)
    SetTimer(HideCaspAbbr, -1)
  } else {
    if (InStr(endReason, "EndKey")) {
      PostMessageToCpasAbbr(CANCEL_COMMAND_INPUT)
    } else {
      PostMessageToCpasAbbr(HIDE_COMMAND_INPUT)
    }
  }

  if (capsHook.Match) {
    ExecCapslockAbbr(capsHook.Match)
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
