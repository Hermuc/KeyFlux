/**
 * TypeID 3: 窗口操作
 * 拆自 Actions.ahk (模块化重构阶段 3), 函数体逐行搬运未做任何修改。
 */

/**
 * 移动活动窗口位置
 */
MakeWindowDraggable() {
  hwnd := WinExist("A")
  if (WindowMaxOrMin())
    WinRestore("A")

  PostMessage("0x0112", "0xF010", 0)
  Sleep 50
  SendInput("{Right}")
}

/**
 * 智能的关闭窗口
 */
SmartCloseWindow() {
  if NotActiveWin() {
    return
  }

  class := WinGetClass("A")
  name := GetProcessName()
  if (WinActive("- Microsoft Visual Studio ahk_exe devenv.exe")) {
    Send("^{F4}")
  } else {
    if (class == "ApplicationFrameWindow" || name == "explorer.exe") {
      Send("!{F4}")
    } else {
      PostMessage(0x112, 0xF060, , , "A")
    }
  }
}

/**
 * 窗口居中并修改其大小
 * @param width 窗口宽度
 * @param height 窗口高度
 * @returns {void}
 */
CenterAndResizeWindow(width, height) {
  if NotActiveWin() {
    return
  }

  ; 在 mousemove 时需要 PER_MONITOR_AWARE (-3), 否则当两个显示器有不同的缩放比例时, mousemove 会有诡异的漂移
  ; 在 winmove 时需要 UNAWARE (-1), 这样即使写死了窗口大小为 1200x800, 系统会帮你缩放到合适的大小
  try DllCall("SetThreadDpiAwarenessContext", "ptr", -1, "ptr")

  WinExist("A")
  if (WindowMaxOrMin())
    WinRestore

  WinGetPos(&x, &y, &w, &h)

  ms := GetMonitorAt(x + w / 2, y + h / 2)
  MonitorGetWorkArea(ms, &l, &t, &r, &b)
  w := r - l
  h := b - t

  winW := Min(width, w)
  winH := Min(height, h)
  winX := l + (w - winW) / 2
  winY := t + (h - winH) / 2

  WinMove(winX, winY, winW, winH)
  try DllCall("SetThreadDpiAwarenessContext", "ptr", -3, "ptr")
}

/**
 * 窗口最大化
 */
MaximizeWindow() {
  if NotActiveWin() {
    return
  }

  if WindowMaxOrMin() {
    WinRestore("A")
  } else {
    WinMaximize("A")
  }
}

/**
 * 窗口最小化
 */
MinimizeWindow() {
  if (NotActiveWin() || WinGetProcessName("A") == "Rainmeter.exe")
    return

  WinMinimize("A")
}

/**
 * 窗口置顶
 */
ToggleWindowTopMost() {
  value := !(WinGetExStyle("A") & 0x8)
  WinSetAlwaysOnTop(value, "A")
  if value {
    Tip(Translation().always_on_top_on)
  } else {
    Tip(Translation().always_on_top_off)
  }
}

/**
 * 关闭窗口（直接杀进程）
 * @returns
 */
CloseWindowProcesses() {
  if NotActiveWin() {
    return
  }

  name := WinGetProcessName("A")
  ; 如果删了explorer会导致桌面白屏
  if (name == "explorer.exe") {
    CloseSameClassWindows()
    return
  }

  Run('taskkill /f /im "' name '"', , "Hide")
}

/**
 * 绑定当前窗口到当前键上
 * @param key 当前键
 * @returns {void}
 */
BindWindow() {
  windowID := false
  windowTitle := false
  handler(thisHotKey) {
    waitkey := ExtractWaitKey(thisHotKey)
    if not (KeyWait(waitkey, "T0.6")) {
      ; 绑定窗口
      windowID := WinGetID("A")
      windowTitle := WinGetTitle("A")
      Tip("已绑定当前窗口")
      KeyWait(waitkey, "T2") ; 避免按住时每隔 0.6 秒重复执行这个动作
      return
    }

    if (windowID && WinExist("ahk_id " windowID)) {
      if WinActive() {
        WinMinimize
        return
      }
      WinActivate(windowID)
      return
    }

    if (windowTitle && WinExist(windowTitle)) {
      if WinActive() {
        WinMinimize
        return
      }
      WinActivate(windowTitle)
      return
    }

    Tip("请长按绑定窗口")
  }

  return handler
}

/**
 * 关闭同应用的所有窗口
 */
CloseSameClassWindows() {
  if NotActiveWin() {
    return
  }

  exe := WinGetProcessName("A")
  if exe == "explorer.exe" {
    windows := FindWindows("ahk_class " WinGetClass("A"))
  } else {
    windows := FindWindows("ahk_exe " exe)
  }

  for i, hwnd in windows {
    WinClose(hwnd)
  }
}

GoToLastWindow() {
  Send("!{tab}")
}

GoToPreviousVirtualDesktop() {
  Send("^#{left}")
}

GoToNextVirtualDesktop() {
  Send("^#{right}")
}

MoveWindowToNextMonitor() {
  Send("#+{right}")
}

/**
 * 设置窗口位置
 * @param x 窗口左上角X轴
 * @param y 窗口左上角Y轴
 * @param width 窗口宽度
 *   default: 不做修改
 * @param height 窗口高度
 *   default: 不做修改
 */
SetWindowPositionAndSize(x, y, width, height) {
  if NotActiveWin() {
    return
  }

  hwnd := WinExist("A")
  state := WinGetMinMax()
  if state
    WinRestore

  if (width == "default" || height == "default") {
    offset := GetWindowPositionOffset(hwnd)
    width := width == "default" ? offset.w : width
    height := height == "default" ? offset.h : height
  }

  WinMove(x, y, width, height)
}
