#Requires AutoHotkey v2.0
#SingleInstance Force
#NoTrayIcon

SetWorkingDir A_ScriptDir "\.."

if !A_Args.Length {
  return
}

if A_Args[1] = "GenerateShortcuts" {
  ; 由于 windows 系统不允许存在同名文件和文件夹，故预先删除之
  try FileDelete("shortcuts")
  try DirDelete("shortcuts", true)
  ; 休息 0.05 s，防止 delete 操作未完成引起的 shortcuts 目录被占用问题
  Sleep(50)
  try DirCreate("shortcuts")

  ; 排除特定的快捷方式
  useless := "i)(?:uninstall|卸载|help|iSCSI 发起程序|ODBC 数据源|Data Sources \(ODBC\)"
    . "|ODBC Data|Windows 内存诊断|恢复驱动器|组件服务|碎片整理和优化驱动器|Office 语言首选项"
    . "|手册|更新|帮助|Tools Command Prompt for|license|Website|设置向导|More Games from Microsoft"
    . "|细胞词库|意见反馈|输入法管理器|输入法修复器|皮肤下载|官方网站|Microsoft Office 语言设置"
    . "|Microsoft Office 文档关联中心|Internet Explorer \(No Add-ons\)"
    . "|Windows Easy Transfer Reports|Welcome Center|Microsoft Office 2007 控制中心"

  Loop Files A_StartupCommon . "\*.lnk*"
    useless .= "|" . A_LoopFileName
  Loop Files A_Startup . "\*.lnk*"
    useless .= "|" . A_LoopFileName
  useless .= ")"

  ; 把开始菜单中的快捷方式都拷贝到 shortcuts 目录
  copyFiles(A_ProgramsCommon "\*.lnk", "shortcuts\", useless)
  copyFiles(A_Programs "\*.lnk", "shortcuts\", useless)
  ; 然后再生成 UWP 相关的快捷方式
  oFolder := ComObject("Shell.Application").NameSpace("shell:AppsFolder")
  if Type(oFolder) != 'String' {
    for item in oFolder.Items {
      if item.Name . ".lnk" ~= useless || FileExist("shortcuts\" item.Name ".lnk") {
        continue
      }
      try FileCreateShortcut("shell:appsfolder\" item.Path, "shortcuts\" item.Name ".lnk")
    }
  }
  return
}

if A_Args[1] = "RunAtStartup" {
  ; 2026-09-10 起改用计划任务 (替代 HKCU\Run 注册表方案):
  ; 登录+3s 触发早于 explorer Run 批次, 最高权限免提权重启 (省一段进程链);
  ; 稳定性: 电池可启动/失败重试 1min×3/IgnoreNew/不限时 (见 bin/templates/KeyFlux-task.xml)
  ; 可移植性: RepairStartupTask 子命令自愈 (目录移动/换机后重建任务)
  runKey := "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
  if A_Args[2] = "On" {
    ; 注册 HIGHEST 任务需管理员令牌: 非提权时自提权重跑 (用户 UAC 静默则无感)
    if !A_IsAdmin {
      Run '*RunAs "' A_ScriptFullPath '" RunAtStartup On'
      return
    }
    CreateStartupTask()
    ; 清理旧注册表方案残留 (0KeyFlux/KeyFlux, 2026-09-10 前机制)
    try RegDelete(runKey, "0KeyFlux")
    try RegDelete(runKey, "KeyFlux")
  } else if (A_Args[2] = "Off") {
    RunWait(A_ComSpec ' /c schtasks /delete /tn "KeyFlux" /f', , "Hide")
    try RegDelete(runKey, "0KeyFlux")
    try RegDelete(runKey, "KeyFlux")
  }
  return
}

if A_Args[1] = "RepairStartupTask" {
  ; 自愈: 任务存在但 action 路径与当前安装目录不符 (目录移动/改名/换机) 时重建;
  ; 任务不存在则不动 (由 UI 开关 RunAtStartup On 创建)
  tmpXml := A_WorkingDir "\bin\tmp-task.xml"
  try FileDelete(tmpXml)
  RunWait(A_ComSpec ' /c schtasks /query /tn "KeyFlux" /xml "' tmpXml '"', , "Hide")
  if FileExist(tmpXml) {
    xml := FileRead(tmpXml, "UTF-16")
    if !InStr(StrLower(xml), StrLower(A_WorkingDir)) {
      CreateStartupTask()
    }
    try FileDelete(tmpXml)
  }
  return
}

/**
 * 创建/覆盖自启动计划任务 KeyFlux: 模板替换安装目录后经 schtasks /xml 注册。
 * 文件须为 UTF-16 (与 XML 声明一致), 否则 schtasks 解析失败。
 */
CreateStartupTask() {
  tmpXml := A_WorkingDir "\bin\tmp-task.xml"
  tmpl := FileRead(A_WorkingDir "\bin\templates\KeyFlux-task.xml", "UTF-8")
  xml := StrReplace(StrReplace(tmpl, "{{DIR}}", A_WorkingDir), "{{USER}}", A_UserName)
  try FileDelete(tmpXml)
  FileAppend(xml, tmpXml, "UTF-16")
  RunWait(A_ComSpec ' /c schtasks /create /tn "KeyFlux" /xml "' tmpXml '" /f', , "Hide")
  try FileDelete(tmpXml)
}

copyFiles(pattern, dest, ignore := "") {
  Loop Files pattern, "R" {
    if (A_LoopFileName ~= ignore) {
      continue
    }
    try FileCopy(A_LoopFilePath, dest, true)
  }
}