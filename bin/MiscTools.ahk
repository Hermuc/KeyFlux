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
  ; 开机自启使用 HKCU\Run 注册表键 (2026-08-23 迁移, 替代旧版启动文件夹快捷方式)
  ; 键名 0 开头: Run 键按值名字母序枚举启动, 0KeyFlux 抢到第一批 (KeyFlux 的 K 恰排常见自启软件末位, 2026-09-10)
  runKey := "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
  if A_Args[2] = "On" {
    ; 值数据为带引号的完整路径, 路径含空格时也能正确启动
    RegWrite('"' A_WorkingDir '\KeyFlux.exe"', "REG_SZ", runKey, "0KeyFlux")
    try RegDelete(runKey, "KeyFlux") ; 清理旧键名 (迁移, 2026-09-10 前的键名)
  } else if (A_Args[2] = "Off") {
    try RegDelete(runKey, "0KeyFlux")
    try RegDelete(runKey, "KeyFlux")
  }
  return
}

copyFiles(pattern, dest, ignore := "") {
  Loop Files pattern, "R" {
    if (A_LoopFileName ~= ignore) {
      continue
    }
    try FileCopy(A_LoopFilePath, dest, true)
  }
}