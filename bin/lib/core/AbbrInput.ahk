/**
 * AbbrInput.ahk —— 缩写命令输入框 (KeyFlux-CommandInput) 的 InputHook 启动与消息通道
 * (从 Functions.ahk 拆分, 2026-09-03, 函数体逐行搬运未修改)。
 * 分组: InputHook 启动 (StartInputHook) | 命令框窗口消息 (PostMessageToCpasAbbr/Hide/Char/Backspace)。
 */

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
PostMessageToCpasAbbr(msg, wParam := 0) {
  temp := A_DetectHiddenWindows
  DetectHiddenWindows(1)
  ; 调用 WinExist 的耗时都不超过 2ms, 没必要做缓存了
  ; 注意: ahk_class 是命令框 exe 内烧录的 Win32 窗口类名, 属其 ABI, 值仍为
  ; MyKeymap_Command_Input（UTF-16 烧录）。命令框自 2026-10 起由自研 Rust 版取代闭源上游 exe
  ; (源码 command-input/, 部署名 KeyFlux-CommandInput.exe), 但该类名按「drop-in ABI」刻意保留
  ; ⇒ 改名须同时改 command-input/ 源码与引擎这里的匹配串, 故按"二进制实际值优先"保留旧名, 勿随品牌改名同步。
  if WinExist("ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe") {
    PostMessage(msg, wParam, 0)
  } else {
    Tip("无法找到命令框, 可能需要重启 KeyFlux", -3000)
  }
  DetectHiddenWindows(temp)
}

/**
 * 关闭顶部命令提示框
 */
HideCaspAbbr() {
  PostMessageToCpasAbbr(0x0400 + 0x0002)
}

/**
 *  将键入的值发送到输入框
 * @param ih InputHook 对象
 * @param char 发送的字符
 */
PostCharToCaspAbbr(ih?, char?) {
  PostMessageToCpasAbbr(0x0102, Ord(SubStr(char, -1)))
}

PostBackspaceToCaspAbbr(ih, vk, sc) {
  PostMessageToCpasAbbr(0x0102, 0x8)
}
