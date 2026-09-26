/**
 * Programs.ahk —— 程序启动与进程信息 (从 Functions.ahk 拆分, 2026-09-03, 函数体逐行搬运未修改)。
 * 分组: 进程名/路径获取 (GetProcessName/GetActiveProcess/CompleteProgramPath/GetTargetProcessName) |
 *       程序启动 (ShellRun/RunAsAdmin/RunPrograms/ShortcutTargetExist)。
 * 均为慢路径辅助, 不参与重映射/发键/鼠标快路径。
 */

/**
 * 获取当前程序名称
 * 自带的WinGetProcessName无法获取到uwp应用的名称
 * 来源：https://www.autohotkey.com/boards/viewtopic.php?style=7&t=112906
 * @returns {string}
 */
GetProcessName() {
  return GetActiveProcess("name")
}

GetActiveProcess(type) {
  fn := (winTitle) => (WinGetProcessName(winTitle) == 'ApplicationFrameHost.exe')

  winTitle := "A"
  if fn(winTitle) {
    for hCtrl in WinGetControlsHwnd(winTitle)
      bool := fn(hCtrl)
    until !bool && winTitle := hCtrl
  }

  if type == "name" {
    return WinGetProcessName(winTitle)
  }
  if type == "id" {
    return WinGetPID(winTitle)
  }
  if type == "path" {
    return WinGetProcessPath(winTitle)
  }
}

/**
 * 从环境中补全程序的绝对路径
 * 来源: https://autohotkey.com/board/topic/20807-fileexist-in-path-environment/
 * @param target 程序路径
 * @returns {string|any}
 */
CompleteProgramPath(target) {
  ; 工作目录下的程序
  PathName := A_WorkingDir "\" target
  if FileExist(PathName)
    return PathName

  ; 本身便是绝对路径
  if FileExist(target)
    return target

  ; 从环境变量 PATH 中获取
  DosPath := EnvGet("PATH")
  loop parse DosPath, "`;" {
    if A_LoopField == ""
      continue

    if FileExist(A_LoopField "\" target)
      return A_LoopField "\" target
  }

  ; 从安装的程序中获取
  try {
    PathName := RegRead("HKLM", "SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\" target)
    if FileExist(PathName)
      return PathName
  }

  return target
}

/**
 * 从程序路径解析出进程名（exe 文件名），用于进程存在性检测
 * 支持 .lnk 快捷方式（解析其指向的目标）
 * @param target 程序路径（相对或绝对）
 * @returns {string} 进程名（如 "WeChat.exe"），解析失败返回空字符串
 */
GetTargetProcessName(target) {
  ; 补全绝对路径
  programPath := CompleteProgramPath(target)

  ; 快捷方式解析指向的真实程序
  if SubStr(programPath, -4) == ".lnk" {
    try FileGetShortcut(programPath, &outTarget)
    catch
      return ""
    if not (outTarget)
      return ""
    programPath := outTarget
  }

  SplitPath(programPath, &name)
  return name
}

/**
 * 通过命令行去启动程序，防止会导致以管理员启动软件的问题
 * @param target 程序路径
 * @param arguments 参数
 * @param directory 工作目录
 * @param operation 选项 (runas/open/edit/print
 * @param show 是否显示
 */
ShellRun(target, arguments?, directory?, operation?, show?) {
  static VT_UI4 := 0x13, SWC_DESKTOP := ComValue(VT_UI4, 0x8)
  ComObject("Shell.Application").Windows.Item(SWC_DESKTOP).Document.Application
  .ShellExecute(target, arguments?, directory?, operation?, show?)
}

/**
 * 以管理员权限打开软件
 * @param target 程序路径
 * @param args 参数
 * @param workingDir 工作目录
 */
RunAsAdmin(target, args, workingDir, options) {
  try {
    Run("*RunAs " target " " args, workingDir, options)
  } catch Error as e {
    Tip("使用管理启动失败 " target ", " e.Message)
  }
}

/**
 * 运行程序或打开目录，用于解决打开的程序无法获取焦点的问题
 * @param target 程序路径
 * @param {string} args 参数
 * @param {string} workingDir 工作目录
 * @param {number} admin 是否为管理员启动
 * @returns {void}
 */
RunPrograms(target, args := "", workingDir := "", admin := false, runInBackground := false) {
  ; 记录当前窗口的hwnd，当软件启动失败时还原焦点
  currentHwnd := WinExist("A")

  if !runInBackground {
    ActivateDesktop()
  }

  try {
    ; 补全程序路径
    programPath := CompleteProgramPath(target)

    ; 如果是文件夹直接打开
    if (InStr(FileExist(programPath), "D")) {
      Run(programPath)
      return
    }

    ; 避免在快捷方式无效，导致的程序卡住
    ShortcutTargetExist(programPath)

    if (admin) {
      runAsAdmin(programPath, args, workingDir, runInBackground ? "Hide" : "")
    } else {
      ; 直接 run "https://example.com" 会让 chrome 以管理员启动
      ; ShellRun 也支持 ms-setting: 或 shell: 或 http: 之类的链接
      ShellRun(programPath, args, workingDir, , runInBackground ? 0 : unset)
    }

  } catch Error as e {
    Tip(e.Message)
    ; 还原窗口焦点
    try WinActivate(currentHwnd)
    return
  }
}

/**
 * 快捷方式指向目标是否存在，不存在抛出异常
 * @param LnkPath 快捷方式路径
 */
ShortcutTargetExist(LnkPath) {
  if SubStr(LnkPath, -4) == ".lnk" {
    FileGetShortcut(LnkPath, &OutTarget)

    ; 没有获取到目标路径可能是因为是uwp应用的快捷方式
    ; 也有可能是ms-setting: 或shell:之类的连接
    if !OutTarget || SubStr(outTarget, 2, 2) != ":\"
      return

    if !FileExist(OutTarget)
      throw Error("快捷方式指向的目标不存在`n快捷方式: " LnkPath "`n指向目标: " OutTarget)
  }
}
