/**
 * TypeID 1: 激活或运行程序/路径
 * 拆自 Actions.ahk (模块化重构阶段 3), 函数体逐行搬运未做任何修改。
 */

/**
 * 启动程序或切换到程序
 * @param {string} winTitle AHK中的WinTitle
 * @param {string} target 程序的路径
 * @param {string} args 参数
 * @param {string} workingDir 工作文件夹
 * @param {bool} admin 是否为管理员启动
 * @param {bool} isHide 窗口是否为隐藏窗口
 * @returns {void}
 */
ActivateOrRun(winTitle := "", target := "", args := "", workingDir := "", admin := false, isHide := false, runInBackground := false) {
  ; 如果是程序或参数中带有“选中的文件” 则通过该程序打开该连接
  if (InStr(target, "{selected}") || InStr(args, "{selected}")) {
    ; 没有获取到文字直接返回
    if not (ReplaceSelectedText(&target, &args))
      return
  }

  ; 切换程序
  winTitle := Trim(winTitle)
  if (winTitle && activateWindow(winTitle, isHide))
    return

  ; 窗口没找到但程序可能还在后台/托盘运行（如微信/QQ 关闭主窗口后驻留托盘），
  ; 此时直接启动会弹出“登录新账号”等错误行为，先检测目标进程是否仍存在
  if (winTitle) {
    processName := GetTargetProcessName(target)
    if (processName && ProcessExist(processName)) {
      ; 🔴 按**进程名**再匹配一次窗口 (2026-09-21)。
      ; 动机: winTitle 常与真实窗口标题不一致 —— 实测微信 4.0 主窗口标题是**用户昵称**
      ;   (如「安和壹」), 而配置里写的是 winTitle="微信" ⇒ 永远匹配不上。此时窗口其实就
      ;   开着, 但按标题找不到 ⇒ 白白走一趟托盘导航, 用户体感"点了要等好几秒"。
      ; 位置: 放在托盘导航**之前** —— 窗口已开则直接激活并返回, 整趟托盘往返都省掉。
      ; 边界: 仅在标题匹配失败之后执行, 不改变"标题优先"的原语义。
      if (activateWindow("ahk_exe " processName, isHide))
        return

      ; 进程在但窗口未出现：极短等待窗口出现。托盘驻留时窗口不会自行出现，空等无意义；
      ; 启动中的窗口由 TryTrayRestoreByNav 的轮询验证（v10）与下方最终兜底捕获（WinWait 超时单位是秒）
      if (WinWait(winTitle, , 0.3)) {
        WinActivate(winTitle)
        return
      }
      ; 窗口仍不出现：通过托盘图标自动唤出（tray_nav.ps1 v10 轮询验证，窗口出现即 DONE 退出），
      ; 等效用户手动点击托盘图标，不会触发重复启动。
      ; 🔴 成功即**立即返回** (2026-09-21): 脚本内部已按 MainWindowHandle != 0 轮询验证窗口
      ;   确实出现, 且 UIA Invoke 已把窗口激活 ⇒ 此处再按 winTitle 空等 1.5s 不仅无意义,
      ;   还会在"标题与 winTitle 不一致"时被判为失败 ⇒ 白等 + 误报「程序在后台运行」。
      if (TryTrayRestoreByNav(processName, winTitle))
        return
      ; 最终兜底：程序可能仍在启动中（托盘图标尚未注册导致唤起失败），再等窗口出现。
      ; 🔴 判据用**进程名**而非 winTitle: 同上的标题不一致问题会让 winTitle 在此处恒判失败,
      ;   把"其实已经开出来了"误报成"唤不出窗口"(实测现象)。
      if (WinWait("ahk_exe " processName, , 2)) {
        WinActivate("ahk_exe " processName)
        return
      }
      ; 程序在后台运行但唤不出窗口，提示用户手动唤出，绝不重启新实例
      Tip(Translation().app_running_in_background, -2500)
      return
    }
  }

  ; 程序没有运行，运行程序
  if not target {
    return
  }
  workingDir := workingDir ? workingDir : A_WorkingDir
  RunPrograms(target, args, workingDir, admin, runInBackground)
}

/**
 * 轮换程序窗口
 * @param winTitle AHK中的WinTitle
 * @param hwnds 活动窗口的句柄数组
 * @returns {void|number}
 */
LoopRelatedWindows(winTitle?, hwnds?) {
  ; 如果没有传句柄数组则获取当前窗口的
  if not (IsSet(hwnds)) {
    predicate := (hwnd) => WinGetTitle(hwnd) != ""
    if (GetProcessName() == "explorer.exe") {
      predicate := (hwnd) => WinGetClass(hwnd) = "CabinetWClass"
    }
    hwnds := FindWindows("ahk_exe " WinGetProcessName("A"), predicate)
  }

  ; 只有一个窗口显示出来就行
  if (hwnds.Length = 1) {
    WinActivate(hwnds.Get(1))
    return
  }

  ; 没有传winTitle时，则获取当前程序的名称
  if not (IsSet(winTitle)) {
    class := WinGetClass("A")
    if (class == "ApplicationFrameWindow") {
      winTitle := WinGetTitle("A") "  ahk_class ApplicationFrameWindow"
    } else {
      winTitle := "ahk_exe " GetProcessName()
    }
  }
  winTitle := Trim(winTitle)

  static winGroup, lastWinTitle := "", lastHwnd := "", gi := 0
  if (winTitle != lastWinTitle || lastHwnd != WinExist("A")) {
    lastWinTitle := winTitle
    winGroup := "AutoName" gi++
  }

  ; 将所有的hwnd都添加到组里
  for hwnd in hwnds {
    GroupAdd(winGroup, "ahk_id" hwnd)
  }

  ; 切换
  lastHwnd := GroupActivate(winGroup, "R")
  return lastHwnd
}

/**
 * 一次打开多个链接或程序
 * @param urls 链接或程序
 */
LaunchMultiple(urls*) {
  for index, url in urls {
    ShellRun(url)
  }
}

/**
 * 进程存在时用热键激活、否则启动程序
 */
ProcessExistSendKeyOrRun(pname, key, target) {
  if ProcessExist(pname) {
    Send(key)
  } else {
    RunPrograms(target)
  }
}
