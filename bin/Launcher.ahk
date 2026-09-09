#Requires AutoHotkey v2.0
; 该脚本只能编译成Exe使用，只用做启动KeyFlux.ahk和AHk v2使用
#SingleInstance Force
#NoTrayIcon
;@Ahk2Exe-SetMainIcon ./bin/icons/logo3.ico
;@Ahk2Exe-ExeName KeyFlux
SetWorkingDir(A_ScriptDir)
FileAppend(A_TickCount " bootstrap-enter" (A_Args.Length ? " args:" A_Args[1] : "") "`n", A_WorkingDir "\boot-log.txt")

; 以管理员权限运行
full_command_line := DllCall("GetCommandLine", "str")
if not (A_IsAdmin or RegExMatch(full_command_line, " /restart(?!\S)")) {
  otherArgs := ""
  if (A_Args.Length) {
    otherArgs := A_Args.Get(1)
  }

  try {
    if A_IsCompiled {
      FileAppend(A_TickCount " elevate-request`n", A_WorkingDir "\boot-log.txt")
      Run '*RunAs "' A_ScriptFullPath '" ' otherArgs ' /restart'
    }
    else
      Run '*RunAs "' A_AhkPath '" /restart "' A_ScriptFullPath '"'
    ExitApp
  } catch Error as e {
    hasTip := true
    ToolTip("`n    KeyFlux is running with normal privileges.`n    KeyFlux will not work in a window with admin rights  ( e.g., Taskmgr.exe )    `n ")
  }
}

mainAhkFilePath := "./bin/KeyFlux.ahk"

if (A_Args.Length) {
  Run("KeyFlux.exe /script " A_Args.Get(1))
} else {
  ; 通过配置文件生成脚本 (幂等跳过: 配置/模板均不比产物新时省去重生成,
  ; 免去开机时 settings.exe 冷启动阻塞, 加快热键就绪 2026-09-10)
  if (NeedsRegenerate("./data/config.json", "./bin/templates/keyflux.tmpl", mainAhkFilePath)) {
    RunWait("./bin/settings.exe GenerateScripts", "./bin", "Hide")
  }
  ; 首次运行则生成快捷方式
  if !FileExist(A_WorkingDir "\shortcuts\*.*") {
    Run("KeyFlux.exe /script ./bin/MiscTools.ahk GenerateShortcuts")
  }
  ; 启动脚本
  FileAppend(A_TickCount " launch-engine`n", A_WorkingDir "\boot-log.txt")
  Run("KeyFlux.exe /script " mainAhkFilePath)
  ; 可移植性自愈: 目录移动/换机后自动重建自启任务 (异步, 不阻塞就绪)
  Run("KeyFlux.exe /script ./bin/MiscTools.ahk RepairStartupTask")
}

if IsSet(hasTip) {
  Sleep 7000
}

/**
 * 判断是否需要重新生成主脚本: 产物缺失, 或配置/模板任一比产物新时需重生成。
 * FileGetTime 返回 YYYYMMDDHH24MISS 字符串, 字典序即时间序。
 */
NeedsRegenerate(config, template, genScript) {
  if !FileExist(genScript) {
    return true
  }
  genTime := FileGetTime(genScript)
  for src in [config, template] {
    if FileExist(src) && FileGetTime(src) > genTime {
      return true
    }
  }
  return false
}