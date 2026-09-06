/**
 * Functions.ahk —— 托盘菜单/生命周期 + 选中文本与剪贴板 + 编码杂项
 * (均为慢路径辅助, 不参与重映射/发键/鼠标快路径)。
 * 2026-09-03 按职责拆分 (函数体逐行搬运未修改):
 *   程序启动与进程 -> core/Programs.ahk | 窗口查找激活 -> core/WindowUtils.ahk |
 *   命令输入框消息 -> core/AbbrInput.ahk。
 * 兼容壳: GetSelectedText 实现已迁移至 context/SelectionContext.ahk, 仅保留签名。
 */

/**
 * 托盘菜单被点击
 * @param ItemName 
 * @param ItemPos 
 * @param MyMenu 
 */
TrayMenuHandler(ItemName, ItemPos, MyMenu) {
  ; 注: 不用 switch 是因为 2.0.19 解释器无法编译 case 表达式含函数调用/属性访问
  t := Translation()
  if (ItemName == t.menu_exit) {
    KeyFluxExit()
  } else if (ItemName == t.menu_pause) {
    KeyFluxToggleSuspend()
  } else if (ItemName == t.menu_reload) {
    KeyFluxReload()
  } else if (ItemName == t.menu_settings) {
    KeyFluxOpenSettings()
  } else if (ItemName == t.menu_window_spy) {
    run("KeyFlux.exe /script bin\WindowSpy.ahk")
  }
}

/**
 * 退出程序
 * @param ExitReason 退出原因
 * @param ExitCode 传递给 Exit 或 ExitApp 的退出代码.
 */
KeyFluxExit(ExitReason?, ExitCode?) {
  ProcessClose("KeyFlux-CommandInput.exe")
  ExitApp
}

/**
 * 暂停
 */
KeyFluxToggleSuspend() {
  fn() {
    Suspend(!A_IsSuspended)
    if (A_IsSuspended) {
      TraySetIcon("./bin/icons/logo2.ico")
      A_TrayMenu.Check(Translation().menu_pause)
      Tip(Translation().keyflux_off, -500)
    } else {
      TraySetIcon("./bin/icons/logo.ico")
      A_TrayMenu.UnCheck(Translation().menu_pause)
      Tip(Translation().keyflux_on, -500)
    }
  }

  if A_PriorKey == "RButton" {
    SetTimer(fn, -200)
    return
  }
  fn()
}

/**
 * 打开设置 (原生 Avalonia GUI: bin\ui\KeyFlux.Settings.exe, 与 settings.exe 同处 bin\ 体系,
 * 窗口标题 "Setting"; 不再经 wt.exe 承载, 后端由 GUI 以子进程管理)
 * WinWait 3s 超时不静默返回: 转后台 SetTimer 轮询兜底, 窗口迟到出现时补前台激活
 */
KeyFluxOpenSettings() {
  static fallbackTimer := ""  ; 兜底轮询闭包持有者, 供下一次唤起防重入清理
  launchSettings() {
    Run('"' A_ScriptDir '\ui\KeyFlux.Settings.exe"', A_ScriptDir)
  }
  ; 窗口在 WinWait 后被销毁时各 Win 调用抛 TargetError, try 静默吞掉竞态;
  ; ahk_id 锚定消除三窗口同标题 Setting 的 re-match 错绑
  activateSettings(hwnd) {
    try {
      if (WinGetMinMax("ahk_id " hwnd) = -1)
        WinRestore("ahk_id " hwnd)
      WinActivate("ahk_id " hwnd)
      if !WinActive("ahk_id " hwnd) {
        ; Windows 前台锁拒绝激活时的 Z 序兜底: Topmost 瞬间翻转不需要前台授权, 必达;
        ; 窗口在其它虚拟桌面时无法跨桌面召唤, 与旧行为一致
        WinSetAlwaysOnTop(true, "ahk_id " hwnd)
        WinSetAlwaysOnTop(false, "ahk_id " hwnd)
        WinActivate("ahk_id " hwnd)
      }
    }
  }
  ; 慢冷启动兜底轮询: 500ms 一次非阻塞检查, 窗口迟到出现时补一次前台激活;
  ; 闭包捕获 attempts/winTitle/activateSettings, 计数有界约 10s 后自删
  WaitSettingsWindow() {
    attempts := attempts - 1
    if (attempts < 0) {
      SetTimer(WaitSettingsWindow, 0)
      return
    }
    cur := WinExist(winTitle)
    if !cur
      return
    SetTimer(WaitSettingsWindow, 0)
    activateSettings(cur)
  }
  ; 标题 "Setting" 较通用 (v2 默认含匹配), 叠加 ahk_exe 约束防误中他进程同名窗口
  winTitle := "Setting ahk_exe KeyFlux.Settings.exe"
  ; 防重入: 新一次唤起接管兜底, 先清上一轮遗留的轮询 timer (闭包实例不同, 必须显式清)
  if (fallbackTimer != "")
    SetTimer(fallbackTimer, 0)
  if (!ProcessExist("KeyFlux.Settings.exe")) {
    launchSettings()
  } else if (WinExist(winTitle)) {
    WinActivate(winTitle)  ; 先行激活; 前台保证由下方统一后置段校验兜底
  } else {
    ; 进程存在但设置窗口不可见, 重启设置程序 (其会一并重建后端子进程)
    if ProcessExist("KeyFlux.Settings.exe") {
      ProcessClose("KeyFlux.Settings.exe")
      ; v2.0.19 实测: 返回 0=进程已关闭, 非 0(PID)=超时进程仍在
      if (ProcessWaitClose("KeyFlux.Settings.exe", 2) != 0)
        return  ; 旧进程未被杀干净且持单实例 Mutex, 新实例会自退, 放弃本次唤起
    }
    launchSettings()
  }
  ; 唤起时强制前台一次 (三分支统一后置段): 冷启动/重启后等窗口出现, 有界 3s,
  ; 超时转兜底轮询 (进程可能启动失败, 不阻塞热键语境); 激活后立即撤销 Topmost,
  ; 非常驻置顶, 仍可手动切后台
  hwnd := WinWait(winTitle, , 3)
  if !hwnd {
    attempts := 20  ; 20 次 x 500ms, 有界约 10s 后自删
    fallbackTimer := WaitSettingsWindow
    SetTimer(fallbackTimer, 500)
    return
  }
  activateSettings(hwnd)
}

/**
 * 重启程序
 */
KeyFluxReload() {
  Tip("Reload")
  Run("KeyFlux.exe")
}

/**
 *  将程序路径或参数中的{selected} 替换为选中的文字
 * @param target 程序路径的引用
 * @param args 参数的引用
 * @returns {void|number} 
 */
ReplaceSelectedText(&target, &args) {
  text := GetSelectedText()
  if not (text) {
    text := ""
  }

  ; 如果是划词搜索且选中了 http 链接那么跳转链接
  if InStr(args, "https://") == 1 || InStr(target, "https://") == 1 {
    if InStr(text, "https://") || InStr(text, "http://") {
      args := InStr(args, "{selected}") ? Trim(text) : args
      target := InStr(target, "{selected}") ? Trim(text) : target
      return 1
    }
  }

  if InStr(args, "://") || InStr(target, "://") {
    text := URIEncode(text)
  }
  args := strReplace(args, "{selected}", text)
  target := strReplace(target, "{selected}", text)

  return 1
}

/**
 * 获取选中的文字 (阶段2: 实现已迁移到 context/SelectionContext.ahk, 保留函数签名兼容存量调用)
 * @returns {void|string} 
 */
GetSelectedText() {
  sel := SelectionContext.Get(false)
  if not (sel.content) {
    return
  }
  return sel.content
}

/**
 * url 编码
 * 来源: https://www.autohotkey.com/boards/viewtopic.php?t=112741
 * @param Uri 需要编码的文本
 * @param {string} encoding 编码格式
 * @returns {string} 
 */
URIEncode(Uri, encoding := "UTF-8") {
  res := ""
  var := Buffer(StrPut(Uri, encoding), 0)
  StrPut(Uri, var, encoding)
  pos := 1
  ; 按字节遍历 buffer 中的 utf-8 字符串, 注意字符串有  null-terminator
  While pos < var.Size {
    code := NumGet(var, pos - 1, "UChar")
    if (code >= 0x30 && code <= 0x39) || (code >= 0x41 && code <= 0x5A) || (code >= 0x61 && code <= 0x7A)
      res .= Chr(code)
    else
      res .= "%" . Format("{:02X}", code)
    pos++
  }
  return res
}

/**
 * 将文本转换为Html
 * @param text 需要转换的文本
 * @param color HEX颜色值
 * @param fontFamily 字体
 * @returns {string} 
 */
FormatHtmlStyle(text, color, fontFamily) {
  style := "Color: '" color "'; font-family: '" fontFamily ";"

  text := HtmlEncode(text)
  html := "<HTML> <head><meta http-equiv='Content-type' content='text/html;charset=UTF-8'></head> <body> <!--StartFragment-->"
  if (InStr(text, "`n")) {
    html .= "<span style='" style "'><pre>" text "</pre></span>"
  } else {
    html .= "<span style='" style "'>" text "</span>"
  }
  html .= "<!--EndFragment--></body></HTML>"
  return html
}

/**
 * Html编码
 * @param text 需要编码的文本
 * @returns {void} 
 */
HtmlEncode(text) {
  text := strReplace(text, "&", "&amp;")
  text := strReplace(text, "<", "&lt;")
  text := strReplace(text, ">", "&gt;")
  ; 注意: 双引号必须用 Chr(34), 写 "" 会被词法器解析为空字符串, 导致替换失效
  text := strReplace(text, Chr(34), "&quot;")
  text := strReplace(text, " ", "&nbsp;")
  return text
}

/**
 * 将文本粘贴到当前程序中
 * @param text 文本
 */
PasteToPrograms(text) {
  A_Clipboard := text
  Send("{LShift down}{Insert down}{Insert up}{LShift up}")
}
