/**
 * WindowUtils.ahk —— 窗口查找/激活/位置工具 (从 Functions.ahk 拆分, 2026-09-03, 函数体逐行搬运未修改)。
 * 分组: 查找 (FindWindows/FindHiddenWindows) | 激活 (ActivateWindow/ActivateDesktop/TryTrayRestoreByNav) |
 *       状态与位置 (WindowMaxOrMin/GetWindowPositionOffset/GetMonitorAt) |
 *       桌面判定与文件定位 (IsDesktop/NotActiveWin/ShowFileInFolder)。
 * 均为慢路径辅助, 不参与重映射/发键/鼠标快路径。
 */

ActivateDesktop() {
  tmp := A_DetectHiddenWindows
  DetectHiddenWindows true
  if WinExist("ahk_class ForegroundStaging") {
    WinActivate
  }
  DetectHiddenWindows tmp
}

/**
 * 激活窗口
 * @param winTitle AHK中的WinTitle
 * @param {number} isHide 窗口是否为隐藏窗口
 * @returns {number}
 */
ActivateWindow(winTitle := "", isHide := false) {
  ; 如果匹配不到窗口且认为窗口为隐藏窗口时查找隐藏窗口
  ; 谓词排除桌面壳窗口 (Progman 即 Program Manager, 归属 explorer.exe 且常驻):
  ; 否则 ahk_exe explorer.exe 类匹配会把桌面当成"已打开的窗口"提前返回,
  ; 导致真正的资源管理器窗口永远不会被启动 (fe 命令冷启动失效的根因)
  hwnds := FindWindows(winTitle, (hwnd) => WinGetTitle(hwnd) != "" && WinGetClass(hwnd) != "Progman")
  if ((!hwnds.Length) && isHide) {
    hwnds := FindHiddenWindows(winTitle)
    if hwnds.Length {
      WinShow(hwnds.Get(1))
      WinActivate(hwnds.Get(1))
      return true
    }
  }

  ; 如果匹配到则跳转，匹配不到返回0
  if (!hwnds.Length) {
    return 0
  }

  ; 只有一个窗口为最小化则切换否则最小化
  if (hwnds.Length = 1) {
    hwnd := hwnds.Get(1)
    ; 指定不为活动窗口或窗口被缩小则显示出来
    if (WinExist("A") != hwnd || WinGetMinMax(hwnd) = -1) {
      WinActivate(hwnd)
    } else {
      WinMinimize(hwnd)
    }
  } else {
    ; 如果多个窗口则来回切换
    LoopRelatedWindows(winTitle, hwnds)
  }

  return 1
}

/**
 * 通过托盘图标键盘导航把后台驻留的窗口唤出到前台
 * 方案：Win+B 聚焦通知区 → Enter 展开溢出面板 → 方向键导航 → UIA Invoke 激活
 * （24H2 下从 Shell_TrayWnd 枚举不到应用图标，但键盘导航链路经真机验证可行；
 *   激活等效用户点击托盘图标，绝不重启新实例）
 * @param processName 进程名（如 "Weixin.exe"）
 * @param winTitle AHK WinTitle（用于提取图标匹配关键词）
 * @returns {boolean} 是否成功命中并激活了托盘图标
 */
TryTrayRestoreByNav(processName, winTitle) {
  try {
    ; 匹配关键词候选：清洗后的窗口标题 → 进程名（去 .exe）
    keywords := []
    t := RegExReplace(Trim(winTitle), "ahk_(exe|class|group)\s+\S+", "")
    t := RegExReplace(t, "ahk_(pid|id)\s+\d+", "")
    t := Trim(t)
    if (t)
      keywords.Push(t)
    name := RegExReplace(processName, "\.exe$", "")
    if (name)
      keywords.Push(name)

    script := A_ScriptDir "\tray_nav.ps1"
    if (!FileExist(script))
      return false

    ; 逐个关键词尝试：导航脚本命中并激活（退出码 0）即视为成功
    ; v12：执行引擎切 pwsh 7（冷启动 ~260ms vs powershell 5.1 ~600ms，全链实测 1.3s→0.55s）；
    ;   无 pwsh 环境回退 powershell（脚本本身保持 5.1/7 双兼容）
    for kw in keywords {
      cmd := 'pwsh -NoProfile -ExecutionPolicy Bypass -File "' script '" -Target "' kw '" -Process "' processName '" -LogFile mk_traynav.txt'
      try
        exitCode := RunWait(cmd, , "Hide")
      catch
        exitCode := RunWait(StrReplace(cmd, "pwsh ", "powershell "), , "Hide")
      if (exitCode = 0)
        return true
    }
  }
  return false
}

/**
 * 查找隐藏窗口返回窗口的Hwnd
 * @param winTitle AHK中的WinTitle
 * @returns {array}
 */
FindHiddenWindows(winTitle) {
  WS_MINIMIZEBOX := 0x00020000
  WS_MINIMIZE := 0x20000000

  ; 窗口过滤条件
  ; 标题不为空、包含最小化按钮
  Predicate(hwnd) {
    if (WinGetTitle(hwnd) = "")
      return false

    style := WinGetStyle(hwnd)
    return style & WS_MINIMIZEBOX
  }

  ; 开启可以查找到隐藏窗口
  DetectHiddenWindows true
  hwnds := FindWindows(winTitle, Predicate)
  DetectHiddenWindows false

  return hwnds
}

/**
 * 返回与指定条件匹配的所有窗口
 * @param winTitle AHK中的WinTitle
 * @param predicate 过滤窗口方法，传过Hwnd，返回bool
 * @returns {array}
 */
FindWindows(winTitle, predicate?) {
  temps := WinGetList(winTitle)
  ; 不需要做任何匹配直接返回
  if not (IsSet(predicate)) {
    return temps
  }

  hwnds := []
  for i, hwnd in temps {
    ; 当有谓词条件且满足时添加这个hwnd
    if predicate(hwnd) {
      hwnds.Push(hwnd)
    }
  }
  return hwnds
}

/**
 * 判断当前窗口是不是桌面
 */
IsDesktop() {
  return WinActive("Program Manager ahk_class Progman") || WinActive("ahk_class WorkerW")
}

/**
 * 获取当前焦点在哪个显示器上
 * @param x 窗口X轴的长度
 * @param y 窗口y轴的长度
 * @param {number} default 显示器下标
 * @returns {string|number} 匹配的显示器下标
 */
GetMonitorAt(x, y, default := 1) {
  m := SysGet(80)
  loop m {
    MonitorGet(A_Index, &l, &t, &r, &b)
    if (x >= l && x <= r && y >= t && y <= b)
      return A_Index
  }
  return default
}

/**
 * 当前窗口是最大化还是最小化
 * @param {string} winTitle AHK中的WinTitle
 * @returns {number}
 */
WindowMaxOrMin(winTitle := "A") {
  return WinGetMinMax(winTitle)
}

/**
 * 获取活动窗口的位置和宽高
 * @param hwnd 窗口句柄
 */
GetWindowPositionOffset(hwnd) {
  rect := Buffer(16, 0)
  er := DllCall("dwmapi\DwmGetWindowAttribute"
    , "UPtr", hwnd  ; HWND  hwnd
    , "UInt", 9     ; DWORD dwAttribute (DWMWA_EXTENDED_FRAME_BOUNDS)
    , "UPtr", rect.Ptr ; PVOID pvAttribute
    , "UInt", rect.size  ; DWORD cbAttribute
    , "UInt")       ; HRESULT
  If er
    DllCall("GetWindowRect", "UPtr", hwnd, "UPtr", rect.Ptr, "UInt")

  r := Object()
  ; 窗口左到屏幕左边
  r.x1 := NumGet(rect, 0, "Int")
  ; 窗口上到屏幕上边
  r.y1 := NumGet(rect, 4, "Int")
  ; 窗口左到屏幕右边
  r.x2 := NumGet(rect, 8, "Int")
  ; 窗口上到屏幕下边
  r.y2 := NumGet(rect, 12, "Int")
  ; 窗口宽度
  r.w := Abs(max(r.x1, r.x2) - min(r.x1, r.x2))
  ; 窗口长度
  r.h := Abs(max(r.y1, r.y2) - min(r.y1, r.y2))

  return r
}

/**
 * 没有活动窗口或是桌面返回True 反之返回false
 */
NotActiveWin() {
  return IsDesktop() || not WinExist("A")
}

ShowFileInFolder(filepath) {
  ; Run Format('explorer.exe /select,"{1}"', filepath)
  DllCall("shell32\SHParseDisplayName", "Str", filepath, "Ptr", 0, "Ptr*", &pidl := 0, "UInt", 0, "Ptr", 0, "HRESULT")
  DllCall("shell32\SHOpenFolderAndSelectItems", "Ptr", pidl, "UInt", 0, "Ptr", 0, "UInt", 0, "HRESULT")
  DllCall("ole32\CoTaskMemFree", "Ptr", pidl)
}
