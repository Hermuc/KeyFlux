/**
 * TypeID 2: 系统控制
 * 拆自 Actions.ahk (模块化重构阶段 3), 函数体逐行搬运未做任何修改。
 */

/**
 * 锁屏
 */
SystemLockScreen() {
  Sleep 300
  DllCall("LockWorkStation")
}

/**
 * 关机
 */
SystemShutdown() {
  Run("SlideToShutDown.exe")
}

/**
 * 重启
 */
SystemReboot() {
  Shutdown(2)
}

SystemSleep() {
  DllCall("PowrProf\SetSuspendState")
}

SystemRestartExplorer() {
  Run("tools\Rexplorer_x64.exe /I /R")
}

SoundControl() {
  wnd := WinExist("A")
  if wnd {
    ActivateOrRun(, "bin\SoundControl.exe", "PreviousWindow " wnd)
  } else {
    ActivateOrRun(, "bin\SoundControl.exe")
  }
}

BrightnessControl() {
  Run("MyKeymap.exe /script bin\ChangeBrightness.ahk")
}

CopySelectedAsPlainText() {
  A_Clipboard := ""
  Send "^c"
  if !ClipWait(1) {
    Tip(Translation().copy_failed)
    return
  }
  A_Clipboard := A_Clipboard
  Tip(Translation().copy_ok)
}

MuteActiveApp() {
  code := RunWait("bin\SoundControl.exe ToggleMute " GetActiveProcess("name"))
  switch code {
    case 1: Tip(Translation().mute_on)
    case 2: Tip(Translation().mute_off)
    default: Tip(Translation().mute_falied)
  }
}

ShowActiveProcessInFolder() {
  try path := GetActiveProcess("path")
  catch as e {
    Tip(e.Message)
    return
  }
  ShowFileInFolder(path)
}
