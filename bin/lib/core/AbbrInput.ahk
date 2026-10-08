/**
 * AbbrInput.ahk —— 缩写命令输入框 (KeyFlux-CommandInput) 的 InputHook 启动与消息通道
 * (从 Functions.ahk 拆分, 2026-09-03, 函数体逐行搬运未修改)。
 * 分组: InputHook 启动 (StartInputHook) | 命令框窗口消息 (PostMessageToCapsAbbr/Hide/Char/Backspace)。
 */

/**
 * 命令框（`command-input/`，部署名 `KeyFlux-CommandInput.exe`）的窗口标识常量。
 *
 * 🔴 类名是**命令框 exe 内烧录的 Win32 窗口类名**（UTF-16，值 `MyKeymap_Command_Input`），
 *    属其 ABI。命令框自 2026-10 起由自研 Rust 版取代闭源上游 exe（源码 `command-input/`），
 *    但该类名按「drop-in ABI」**刻意保留** ⇒ 改名须同时改 `command-input/` 源码与这里，
 *    **勿随品牌改名同步**（按"二进制实际值优先"）。
 *
 * 本 class 是命令框窗口匹配字面量的**唯一来源**（2026-10-08 批 O 收编此前散落的 3 处副本：
 * `AbbrInput` 消息投递 / `CommandDisplay` 等待窗口 / `CommandInputHooks` 前台判定）。
 */
class CommandInputWin {
  ; 仅类名（`CommandInputHooks` 判前台用：那里已确信是命令框进程）。
  static CLASS := "ahk_class MyKeymap_Command_Input"
  ; 类名 + exe 限定（防同名窗口类被其它进程占用）。
  static FULL := CommandInputWin.CLASS " ahk_exe KeyFlux-CommandInput.exe"
}

/**
 * 启动InputHook，并返回EndReason
 * @param ih InputHook对象
 * @returns {void}
 */
StartInputHook(ih) {
  ; 禁用所有热键
  Suspend(true)

  ; RAlt 映射到 LCtrl 后,  按下 RAlt 再触发 Capslock 命令会导致 LCtrl 键一直处于按下状态
  if GetKeyState("LCtrl") {
    Send("{LCtrl Up}")
  }

  ; 启动监听等待输入匹配后关闭监听
  ih.Start()
  endReason := ih.Wait()
  ih.Stop()
  ; 恢复所有热键
  Suspend(false)

  return endReason
}

/**
 * 发送消息到命令提示框
 * @param msg 消息编号
 * @param {number} wParam 消息参数
 */
PostMessageToCapsAbbr(msg, wParam := 0) {
  temp := A_DetectHiddenWindows
  DetectHiddenWindows(1)
  ; 调用 WinExist 的耗时都不超过 2ms, 没必要做缓存了
  ; 窗口标识（类名属命令框 ABI）见本文件顶部 `CommandInputWin` 的文档。
  if WinExist(CommandInputWin.FULL) {
    PostMessage(msg, wParam, 0)
  } else {
    Tip("无法找到命令框, 可能需要重启 KeyFlux", -3000)
  }
  DetectHiddenWindows(temp)
}

/**
 * 关闭顶部命令提示框
 */
HideCapsAbbr() {
  PostMessageToCapsAbbr(0x0400 + 0x0002)
}

/**
 *  将键入的值发送到输入框
 * @param ih InputHook 对象
 * @param char 发送的字符
 */
PostCharToCapsAbbr(ih?, char?) {
  PostMessageToCapsAbbr(0x0102, Ord(SubStr(char, -1)))
}

PostBackspaceToCapsAbbr(ih, vk, sc) {
  PostMessageToCapsAbbr(0x0102, 0x8)
}
