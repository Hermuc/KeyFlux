#Requires AutoHotkey v2.0
#SingleInstance Force
#UseHook true

#include lib/core/translation.ahk
#Include lib/core/IKeyEventBus.ahk
#Include lib/core/EventBus.ahk
#Include lib/core/Functions.ahk
#Include lib/core/Programs.ahk
#Include lib/core/WindowUtils.ahk
#Include lib/core/AbbrInput.ahk
#Include lib/core/CommandDisplay.ahk
#Include lib/core/ImeInputHost.ahk
#Include lib/core/CommandImeGuard.ahk
#Include lib/core/CommandInputHooks.ahk
#Include lib/actions/Actions.ahk
#Include lib/core/KeymapManager.ahk
#Include lib/core/InputTipWindow.ahk
#Include lib/core/Utils.ahk
#Include lib/context/SelectionContext.ahk
#Include lib/rules/SelectedAction.ahk
#Include lib/commands/CommandResolver.ahk
#Include lib/quickswitch/FolderRanker.ahk
#Include lib/quickswitch/HistoryStore.ahk
#Include lib/quickswitch/FolderHistory.ahk
#Include lib/quickswitch/DialogInspector.ahk
#Include lib/quickswitch/QuickSwitchUI.ahk
#Include lib/quickswitch/QuickSwitch.ahk
#Include lib/plugins/Plugins.ahk

; #WinActivateForce   ; 先关了遇到相关问题再打开试试
; InstallKeybdHook    ; 这个可以重装 keyboard hook, 提高自己的 hook 优先级, 以后可能会用到
; ListLines False     ; 也许能提升一点点性能 ( 别抱期待 ), 当有这个需求时再打开试试
; #Warn All, Off      ; 也许能提升一点点性能 ( 别抱期待 ), 当有这个需求时再打开试试

try DllCall("SetThreadDpiAwarenessContext", "ptr", -3, "ptr") ; 多显示器不同缩放比例会导致问题: https://www.autohotkey.com/boards/viewtopic.php?f=14&t=13810
SetMouseDelay 0                                           ; SendInput 可能会降级为 SendEvent, 此时会有 10ms 的默认 delay
SetWinDelay 0                                             ; 默认会在 activate, maximize, move 等窗口操作后睡眠 100ms
A_MaxHotkeysPerInterval := 256                            ; 默认 70 可能有点低, 即使没有热键死循环也触发警告
SendMode "Event"                                          ; 执行 SendInput 的期间会短暂卸载 Hook, 这时候松开引导键会丢失 up 事件, 所以 Event 模式更适合 KeyFlux
SetKeyDelay 0                                             ; 默认 10 太慢了, https://www.reddit.com/r/AutoHotkey/comments/gd3z4o/possible_unreliable_detection_of_the_keyup_event/
ProcessSetPriority "High"
SetWorkingDir("../")
; 引擎级未捕获异常兜底: 替代「错误弹窗 + 线程死亡 + Suspend 残留 (热键全灭)」,
; 记录全文到 logs\engine_error.log (见 Functions.ahk 的 EngineOnError 注释)。
OnError(EngineOnError)
InitTrayMenu()
; 命令框中文输入 + 八角框移除 (CONTRACTS §3.11/§3.12 v4):
; 八角框由 exe 数据 patch 移除 (keycap 白名单串 -> U+0001, 字母走普通字形路径);
; 中文输入由透传 hook 承担: hook 恒 V, 物理键透传 -> 英文原生显示 / IME 原生组合上屏;
; 投递通道整体关闭 (SuppressKeycap), providers 照常派发。
; 本注册即透传模式的启闭开关 (不查任何 IME 状态 —— 跨进程查询已整体证伪, 见 §3.12)。
; 若要回到「命令框历史行为」(吞键 + 投递显示), 注释掉下面两行即可 (零其它改动)。
CommandInputHooks.Register(ImeInputHost)
ImeInputHost.Enable()
; 命令框会话内「锁英文」(CommandImeGuard): 在 OnSessionBegin 强制历史形态(吞键+投递英文,
; 不使用输入法), 从而英文态弹框不会变中文, Shift 也无中文可切。注册顺序必须在
; ImeInputHost 之后(后生效, 覆盖其透传标志)。插件无关, 可注释此行整体关闭。
; 见 docs/design-ime-guard.md。
CommandInputHooks.Register(CommandImeGuard)
InitKeymap()
InitQuickSwitch({collectEnabled: true, autoShow: true, autoJumpOpen: true, autoJumpSave: false, pollIntervalMs: 800, maxHistory: 200, overlayRows: 8, overlayRowsCompact: 4, excludedPrefixes: []})
OnExit(KeyFluxExit)
#include ../data/custom_functions.ahk

InitKeymap()
{
  taskSwitch := TaskSwitchKeymap("e", "d", "s", "f", "c", "space")
  mouseTip := false
  slow := MouseKeymap("slow mouse", false, mouseTip, 10, 13, "T0.13", "T0.01", 1, "T0.2", "T0.03")
  fast := MouseKeymap("fast mouse", false, mouseTip, 110, 70, "T0.13", "T0.01", 1, "T0.2", "T0.03", slow)
  slow.Map("*space", slow.LButtonUp())

  ; hook 每会话经 MakeCapsHook() 动态创建 (见其函数注释): 透传模式 -> V, 否则历史形态
  Run("bin\KeyFlux-CommandInput.exe")

  semiHook := InputHook("", "{CapsLock}{Esc}{;}", ",,,.,/,dk,dq,fz,gg,gt,i love nia,jt,kg,rq,sj,sk,xf,xk,zh,zk")
  semiHook.KeyOpt("{CapsLock}", "S")
  semiHook.KeyOpt("{Backspace}", "N")
  semiHook.OnChar := (ih, char) => semiHookAbbrWindow.Show(char, true)
  semiHook.OnKeyDown := (ih, vk, sc) => semiHookAbbrWindow.Backspace()
  semiHookAbbrWindow := InputTipWindow()


  ; 路径变量
  programs := "C:\ProgramData\Microsoft\Windows\Start Menu\Programs\"

  ; 缩写命令注册表 (阶段 4: 取代 ExecCapslockAbbr 内的 switch)
  CommandResolver.Register("capslock", "bb", [CommandStep(() => ActivateOrRun("Bing 词典", "msedge.exe", "--app=https://www.bing.com/dict/search?q={selected}", "", false, false, false))])
  CommandResolver.Register("capslock", "ca", [CommandStep(() => ActivateOrRun("计算器", "calc.exe"))])
  CommandResolver.Register("capslock", "cc", [CommandStep(() => ActivateOrRun("", "shortcuts\Visual Studio Code.lnk", "-n `"{selected}`"", "", false, false, false))])
  CommandResolver.Register("capslock", "cmd", [CommandStep(() => ActivateOrRun("ahk_exe cmd.exe", "cmd.exe", "/k cd /d %userprofile%", "", false, false, false))])
  CommandResolver.Register("capslock", "dd", [CommandStep(() => ActivateOrRun("", "shell:downloads"))])
  CommandResolver.Register("capslock", "dm", [CommandStep(() => ActivateOrRun("", A_WorkingDir))])
  CommandResolver.Register("capslock", "ex", [CommandStep(() => KeyFluxExit())])
  CommandResolver.Register("capslock", "ga", [CommandStep(() => ActivateOrRun("Game ahk_exe explorer.exe", A_Desktop "\Game"))])
  CommandResolver.Register("capslock", "gg", [CommandStep(() => ActivateOrRun("", "https://bing.com/search?q={selected}"))])
  CommandResolver.Register("capslock", "gj", [CommandStep(() => SystemShutdown())])
  CommandResolver.Register("capslock", "kp", [CommandStep(() => CloseWindowProcesses())])
  CommandResolver.Register("capslock", "ld", [CommandStep(() => BrightnessControl())])
  CommandResolver.Register("capslock", "lj", [CommandStep(() => ActivateOrRun("", "shell:RecycleBinFolder"))])
  CommandResolver.Register("capslock", "ly", [CommandStep(() => ActivateOrRun("", "ms-settings:bluetooth"))])
  CommandResolver.Register("capslock", "mm", [CommandStep(() => ActivateOrRun("KeyFlux2 - Visual Studio Code", "shortcuts\Visual Studio Code.lnk", "D:\MyFiles\KeyFlux2", "", false, false, false))])
  CommandResolver.Register("capslock", "ms", [CommandStep(() => ActivateOrRun("my_site - Visual Studio Code", "shortcuts\Visual Studio Code.lnk", "D:\project\my_site", "", false, false, false))])
  CommandResolver.Register("capslock", "mu", [CommandStep(() => MuteActiveApp())])
  CommandResolver.Register("capslock", "no", [CommandStep(() => ActivateOrRun("记事本", "notepad.exe"))])
  CommandResolver.Register("capslock", "pd", [CommandStep(() => ShowActiveProcessInFolder())])
  CommandResolver.Register("capslock", "rb", [CommandStep(() => SystemReboot())])
  CommandResolver.Register("capslock", "rex", [CommandStep(() => SystemRestartExplorer())])
  CommandResolver.Register("capslock", "se", [CommandStep(() => KeyFluxOpenSettings())])
  CommandResolver.Register("capslock", "sl", [CommandStep(() => SystemSleep())])
  CommandResolver.Register("capslock", "sp", [CommandStep(() => ActivateOrRun("Spotify", "https://open.spotify.com/"))])
  CommandResolver.Register("capslock", "ss", [CommandStep(() => ActivateOrRun("ahk_exe Spotify.exe", "shortcuts\Spotify.lnk"))])
  CommandResolver.Register("capslock", "tm", [CommandStep(() => Send("^+{esc}"))])
  CommandResolver.Register("capslock", "vm", [CommandStep(() => ActivateOrRun("", "ms-settings:apps-volume"))])
  CommandResolver.Register("capslock", "we", [CommandStep(() => ActivateOrRun("网易云音乐", "shortcuts\网易云音乐.lnk"))])
  CommandResolver.Register("capslock", "wf", [CommandStep(() => ActivateOrRun("", "ms-availablenetworks:"))])
  CommandResolver.Register("capslock", "wt", [CommandStep(() => ActivateOrRun("", "wt.exe", "-d `"{selected}`"", "", false, false, false))])
  CommandResolver.Register("semicolon", ",", [CommandStep(() => Send("，"))])
  CommandResolver.Register("semicolon", ".", [CommandStep(() => Send("。"))])
  CommandResolver.Register("semicolon", "/", [CommandStep(() => Send("、"))])
  CommandResolver.Register("semicolon", "dk", [CommandStep(() => Send("{text}{}"), Send("{left}"))])
  CommandResolver.Register("semicolon", "dq", [CommandStep(() => ActivateOrRun("", "bin\AutoHotkey64.exe", "bin\AlignComment.ahk", "", false, false, true))])
  CommandResolver.Register("semicolon", "fz", [CommandStep(() => ActivateOrRun("连续复制后 ahk_class AutoHotkeyGUI", "bin\AutoHotkey64.exe", "bin\CollectText.ahk", "", false, false, false))])
  CommandResolver.Register("semicolon", "gg", [CommandStep(() => Send("{text}git add -A; git commit -a -m `"`"; git push origin (git branch --show-current);"), Send("{left 47}"))])
  CommandResolver.Register("semicolon", "gt", [CommandStep(() => Send("🐶"))])
  CommandResolver.Register("semicolon", "i love nia", [CommandStep(() => Send("{text}我爱尼娅! "), Send("{text}( 还 有 大 家 )"))])
  CommandResolver.Register("semicolon", "jt", [CommandStep(() => Send("{text}➤ "))])
  CommandResolver.Register("semicolon", "kg", [CommandStep(() => InsertSpaceBetweenZHAndEn())])
  CommandResolver.Register("semicolon", "rq", [CommandStep(() => Send(Format("{}-{}-{}", A_YYYY, A_MM, A_DD)))])
  CommandResolver.Register("semicolon", "sj", [CommandStep(() => Send(Format("{}年{}月{}日 {}:{}", A_YYYY, A_MM, A_DD, A_Hour, A_Min)))])
  CommandResolver.Register("semicolon", "sk", [CommandStep(() => Send("「  」"), Send("{left 2}"))])
  CommandResolver.Register("semicolon", "xf", [CommandStep(() => Send("();{left 2}"))])
  CommandResolver.Register("semicolon", "xk", [CommandStep(() => Send("(){left}"))])
  CommandResolver.Register("semicolon", "zh", [CommandStep(() => Send("{text} site:zhihu.com inurl:question"))])
  CommandResolver.Register("semicolon", "zk", [CommandStep(() => Send("[]{left}"))])
  ; 窗口组
  GroupAdd("MY_WINDOW_GROUP__1", "Stardew Valley ahk_class SDL_app")
  GroupAdd("MY_WINDOW_GROUP__1", "ahk_exe Rune Factory 3 Special.exe")
  GroupAdd("MY_WINDOW_GROUP_1", "ahk_exe chrome.exe")
  GroupAdd("MY_WINDOW_GROUP_1", "ahk_exe msedge.exe")
  GroupAdd("MY_WINDOW_GROUP_1", "ahk_exe firefox.exe")

  KeymapManager.GlobalKeymap.DisabledAt := "ahk_group MY_WINDOW_GROUP__1"

  ; CapsLock
  km5 := KeymapManager.NewKeymap("*CapsLock", "CapsLock", "", "")
  km := km5
  km.Map("*c", _ => SoundControl())
  km.Map("*z", _ => CopySelectedAsPlainText())
  km.Map("*.", _ => MakeWindowDraggable())
  km.Map("*a", _ => CenterAndResizeWindow(1370, 930))
  km.Map("*b", _ => MinimizeWindow())
  km.Map("*e", _ => Send("^!{tab}"), taskSwitch)
  km.Map("*g", _ => ToggleWindowTopMost())
  km.Map("*p", _ => GoToNextVirtualDesktop())
  km.Map("*q", _ => MaximizeWindow())
  km.Map("*r", _ => LoopRelatedWindows())
  km.Map("*s", _ => CenterAndResizeWindow(1200, 800))
  km.Map("*t", BindWindow())
  km.Map("*v", _ => MoveWindowToNextMonitor())
  km.Map("*w", _ => GoToLastWindow())
  km.Map("*x", _ => SmartCloseWindow())
  km.Map("*y", _ => GoToPreviousVirtualDesktop())
  km.Map("*,", fast.LButtonDown()), slow.Map("*,", slow.LButtonDown())
  km.Map("*/", _ => MoveMouseToCaret()), slow.Map("*/", _ => MoveMouseToCaret())
  km.Map("*;", fast.ScrollWheelRight), slow.Map("*;", slow.ScrollWheelRight)
  km.Map("*h", fast.ScrollWheelLeft), slow.Map("*h", slow.ScrollWheelLeft)
  km.Map("*i", fast.MoveMouseUp, slow), slow.Map("*i", slow.MoveMouseUp)
  km.Map("*j", fast.MoveMouseLeft, slow), slow.Map("*j", slow.MoveMouseLeft)
  km.Map("*k", fast.MoveMouseDown, slow), slow.Map("*k", slow.MoveMouseDown)
  km.Map("*l", fast.MoveMouseRight, slow), slow.Map("*l", slow.MoveMouseRight)
  km.Map("*m", fast.RButton()), slow.Map("*m", slow.RButton())
  km.Map("*n", fast.LButton()), slow.Map("*n", slow.LButton())
  km.Map("*o", fast.ScrollWheelDown), slow.Map("*o", slow.ScrollWheelDown)
  km.Map("*u", fast.ScrollWheelUp), slow.Map("*u", slow.ScrollWheelUp)
  km.Map("*0", _ => (Send("{home}+{end}{backspace}"), Send("{text}i love homura and hikari"), Sleep(1000), Send("{enter}yes{enter}")))
  km.Map("*d", _ => CenterAndResizeWindow(1740, 1000))
  km.Map("singlePress", _ => EnterCapslockAbbr())

  ; CapsLock + F
  km6 := KeymapManager.AddSubKeymap(km5, "*f", "CapsLock + F", "")
  km := km6
  km.Map("*a", _ => ActivateOrRun("ahk_exe WindowsTerminal.exe", "shortcuts\终端预览.lnk"))
  km.Map("*d", _ => ActivateOrRun("ahk_exe msedge.exe", "shortcuts\Microsoft Edge.lnk"))
  km.Map("*e", _ => ActivateOrRun("ahk_class CabinetWClass ahk_exe Explorer.EXE", "shortcuts\File Explorer.lnk"))
  km.Map("*h", _ => ActivateOrRun("- Microsoft Visual Studio", "shortcuts\Visual Studio 2019.lnk"))
  km.Map("*i", _ => ActivateOrRun("ahk_exe Typora.exe", "shortcuts\Typora.lnk"))
  km.Map("*j", _ => ActivateOrRun("ahk_exe idea64.exe", "shortcuts\IntelliJ IDEA Ultimate.lnk"))
  km.Map("*k", _ => ActivateOrRun("ahk_class PotPlayer64", "shortcuts\PotPlayer 64 bit.lnk"))
  km.Map("*l", _ => ActivateOrRun("ahk_exe EXCEL.EXE", "shortcuts\Excel.lnk"))
  km.Map("*n", _ => ActivateOrRun("ahk_exe goland64.exe", "shortcuts\GoLand.lnk"))
  km.Map("*o", _ => ActivateOrRun("ahk_exe ONENOTE.EXE", "shortcuts\OneNote.lnk"))
  km.Map("*p", _ => ActivateOrRun("ahk_exe POWERPNT.EXE", "shortcuts\PowerPoint.lnk"))
  km.Map("*q", _ => ActivateOrRun("ahk_class EVERYTHING", "shortcuts\Everything.lnk"))
  km.Map("*r", _ => ActivateOrRun("ahk_exe FoxitReader.exe", "D:\install\Foxit Reader\FoxitReader.exe"))
  km.Map("*s", _ => ActivateOrRun("ahk_exe Code.exe", "shortcuts\Visual Studio Code.lnk"))
  km.Map("*w", _ => ActivateOrRun("ahk_exe chrome.exe", "shortcuts\Google Chrome.lnk"))
  km.Map("singlePress", _ => (Send("{blind}{f}")))
  km.Map("*m", _ => ProcessExistSendKeyOrRun("QQ.exe", "^!z", "shortcuts\QQ.lnk"))

  ; CapsLock + Space
  km7 := KeymapManager.AddSubKeymap(km5, "*Space", "CapsLock + Space", "")
  km := km7
  km.Map("*d", _ => ActivateOrRun("ahk_exe datagrip64.exe", "shortcuts\DataGrip.lnk"))
  km.Map("singlePress", _ => (Send("{blind}{space}")))
  km.Map("*w", _ => ProcessExistSendKeyOrRun("Weixin.exe", "^!w", "shortcuts\微信.lnk"))

  ; J 模式
  km8 := KeymapManager.NewKeymap("*j", "J 模式", "", "")
  km := km8
  km.Map("*i", _ => (Send("{blind}ji")))
  km.Map("singlePress", _ => (Send("{blind}{j}")))
  km.RemapKey(",", "delete")
  km.RemapKey(".", "insert")
  km.Map("*2", _ => (Send("^+{tab}")))
  km.Map("*3", _ => (Send("^{tab}")))
  km.RemapKey("a", "home")
  km.Map("*b", _ => (Send("^{backspace}")))
  km.RemapKey("c", "backspace")
  km.RemapKey("d", "down")
  km.RemapKey("e", "up")
  km.RemapKey("f", "right")
  km.RemapKey("g", "end")
  km.Map("*k", _ => HoldDownModifierKey("LShift"))
  km.RemapKey("q", "appskey")
  km.RemapKey("r", "tab")
  km.RemapKey("s", "left")
  km.Map("*t", _ => (Send("{home}+{end}{backspace}")))
  km.Map("*v", _ => (Send("{blind}^{right}")))
  km.Map("*w", _ => (Send("{blind}+{tab}")))
  km.RemapKey("x", "esc")
  km.Map("*z", _ => (Send("{blind}^{left}")))
  km.Map("*space", _ => (Send("{blind}{enter}")))

  ; 3 模式
  km10 := KeymapManager.NewKeymap("*3", "3 模式", "", "")
  km := km10
  km.RemapKey("0", "F10")
  km.RemapKey("2", "F2")
  km.RemapKey("4", "F4")
  km.RemapKey("5", "F5")
  km.RemapKey("9", "F9")
  km.RemapKey("b", "7")
  km.RemapKey("e", "F11")
  km.RemapKey("h", "0")
  km.RemapKey("i", "5")
  km.RemapKey("j", "1")
  km.RemapKey("k", "2")
  km.RemapKey("l", "3")
  km.RemapKey("m", "9")
  km.RemapKey("n", "8")
  km.RemapKey("o", "6")
  km.RemapKey("r", "F12")
  km.RemapKey("t", "Volume_Up")
  km.RemapKey("u", "4")
  km.RemapKey("w", "Volume_Down")
  km.RemapKey("space", "F1")
  km.Map("singlePress", _ => (Send("{blind}{3}")))
  km.Map("*/", km.ToggleLock)

  ; 分号模式( ; )
  km13 := KeymapManager.NewKeymap("*;", "分号模式( `; )", "", "")
  km := km13
  km.Map("*a", _ => (Send("{blind}*")))
  km.Map("*b", _ => (Send("{blind}%")))
  km.Map("*c", _ => (Send("{blind}.")))
  km.Map("*d", _ => (Send("{blind}=")))
  km.Map("*e", _ => (Send("{blind}{^}")))
  km.Map("*f", _ => (Send("{blind}>")))
  km.Map("*g", _ => (Send("{blind}{!}")))
  km.Map("*h", _ => (Send("{blind}{+}")))
  km.Map("*i", _ => (Send("{blind}:")))
  km.Map("*j", _ => (Send("{blind};")))
  km.Map("*k", _ => (Send("{blind}``")))
  km.Map("*m", _ => (Send("{blind}-")))
  km.Map("*n", _ => (Send("{blind}/")))
  km.Map("*r", _ => (Send("{blind}&")))
  km.Map("*s", _ => (Send("{blind}<")))
  km.Map("*t", _ => (Send("{blind}~")))
  km.Map("*u", _ => (Send("{blind}$")))
  km.Map("*v", _ => (Send("{blind}|")))
  km.Map("*w", _ => (Send("{blind}{#}")))
  km.Map("*x", _ => (Send("{blind}_")))
  km.Map("*y", _ => (Send("{blind}@")))
  km.Map("*z", _ => (Send("{blind}\")))
  km.Map("singlePress", _ => EnterSemicolonAbbr(semiHook, semiHookAbbrWindow))

  ; 句号模式( . )
  km14 := KeymapManager.NewKeymap("*.", "句号模式( . )", "", "")
  km := km14
  km.Map("singlePress", _ => (Send("{blind}{.}")))
  km.Map("*,", _ => HoldDownModifierKey("LShift"))
  km.Map("*2", _ => (Send("^+{tab}")))
  km.Map("*3", _ => (Send("^{tab}")))
  km.RemapKey("a", "home")
  km.Map("*b", _ => (Send("^{backspace}")))
  km.RemapKey("c", "backspace")
  km.RemapKey("d", "down")
  km.RemapKey("e", "up")
  km.RemapKey("f", "right")
  km.RemapKey("g", "end")
  km.RemapKey("q", "appskey")
  km.RemapKey("r", "tab")
  km.RemapKey("s", "left")
  km.Map("*v", _ => (Send("{blind}^{right}")))
  km.Map("*w", _ => (Send("{blind}+{tab}")))
  km.RemapKey("x", "esc")
  km.Map("*z", _ => (Send("{blind}^{left}")))
  km.Map("*space", _ => (Send("{blind}{enter}")))

  ; 鼠标右键
  km16 := KeymapManager.NewKeymap("RButton", "鼠标右键", "", "")
  km := km16
  km.Map("*XButton1", _ => GoToNextVirtualDesktop())
  km.Map("*XButton2", _ => GoToPreviousVirtualDesktop())
  km.Map("singlePress", fast.RButton()), slow.Map("singlePress", slow.RButton())
  km.Map("*LButton", _ => (Send("^!{tab}")))
  km.Map("*MButton", _ => (Send("#{tab}")))
  km.RemapKey("c", "backspace")
  km.RemapKey("d", "delete")
  km.RemapKey("x", "esc")
  km.Map("*space", _ => (Send("{blind}{enter}")))
  km.Map("*WheelUp", _ => (Send("^+{tab}")))
  km.Map("*WheelDown", _ => (Send("^{tab}")))

  ; Custom Hotkeys
  km1 := KeymapManager.NewKeymap("customHotkeys", "Custom Hotkeys", "", "")
  km := km1
  km.RemapInHotIf("RAlt", "LControl")
  km.Map("!'", _ => KeyFluxReload(), , , , "S")
  km.Map("!+'", _ => KeyFluxToggleSuspend(), , , , "S")
  km.Map("!f17", _ => KeyFluxReload(), , , , "S")
  km.Map("!CapsLock", _ => ToggleCapslock())



  KeymapManager.GlobalKeymap.Enable()
}

ExecCapslockAbbr(command) {
  CommandResolver.Resolve("capslock", command)
}

/**
 * 动态创建 CapsLock 命令框 InputHook (每会话一次, 在 EnterCapslockAbbr 内调用)。
 *
 * 🔴 为什么动态: InputHook 对象一次性, 每会话新建。可见性按透传开关二态:
 *   透传模式 (SuppressKeycap=true, ImeInputHost 启用时恒如此) => InputHook("V"):
 *   物理键透传到命令框窗口 —— 英文字母原生显示; 上屏中文以 WM_CHAR 直达 (非白名单,
 *   无框)。历史形态 (false) => InputHook(""): 吞文本键, 显示靠投递 (数据 patch 后无八角框)。
 *
 * 词表 (v4.2 恢复, 两形态共用): 设置面板的命令全部由英文字母组成, 且命令框内需要
 * 输入中文的唯一场景是前置键 (如空格) 触发插件之后 —— 那时字符已被插件 OnChar
 * 消费 (DispatchChar 提前 return), 到不了匹配层; MatchList 为**全串匹配**, 搜索期
 * 的 Input 形如 " se" (带空格前缀) ≠ "se", 永不误触发。故 v4 的「透传词表置空」
 * 废除, 两形态同词表: 全串命中走 Match 分支 (ExecCapslockAbbr), 带前缀后缀命中走
 * FuzzySuffixFire —— 双通道行为与历史形态完全一致。
 * 时序: EnterCapslockAbbr 先 BeginSession() (OnSessionBegin 已置 SuppressKeycap),
 * 再调本函数, 此刻读该标志即拿到正确形态。
 */
MakeCapsHook() {
  ih := InputHook(CommandDisplay.SuppressKeycap ? "V" : "", "{CapsLock}{Esc}"
                  , "bb,ca,cc,cmd,dd,dm,ex,ga,gg,gj,kp,ld,lj,ly,mm,ms,mu,no,pd,rb,rex,se,sl,sp,ss,tm,vm,we,wf,wt")
  ih.KeyOpt("{CapsLock}", "S")
  ih.KeyOpt("{Esc}", "S")
  ; S = V 模式下抑制透传 (EndKey 默认透传): CapsLock 防切大小写状态, Esc 防触发 exe
  ; 原生行为; EndKey 检测不受 S 影响, 会话照常结束。历史形态下 S 无副作用 (本来就吞)。
  ih.KeyOpt("{Backspace}", "N")
  ; 🔴 Up/Down/Enter/Backspace 在 V 模式下必须保持透传 (只有 N 无 S): IME 组合期它们是
  ; 选候选/翻页/确认拼音原文/删组合串的原生操作, 吞掉即毁 IME 交互。
  ; N 仅保留 OnKeyDown 通知 (供插件下拉列表导航, 由 CommandInputHooks 分发);
  ; exe 是纯镜像显示窗口 (只处理 WM_CHAR), 对非字符键无原生行为, 透传噪声可忽略。
  ih.KeyOpt("{Up}", "N")
  ih.KeyOpt("{Down}", "N")
  ih.KeyOpt("{Enter}", "N")
  ih.OnChar := (ih2, char) => CommandInputOnChar(ih2, char, "capslock")
  ih.OnKeyDown := (ih2, vk, sc) => CommandInputOnKeyDown(ih2, vk, sc, "capslock")
  return ih
}

ExecSemicolonAbbr(command) {
  CommandResolver.Resolve("semicolon", command)
}

InitTrayMenu() {
  A_TrayMenu.Delete()
  A_TrayMenu.Add(Translation().menu_pause, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_exit, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_reload, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_settings, TrayMenuHandler)
  A_TrayMenu.Add(Translation().menu_window_spy, TrayMenuHandler)
  A_TrayMenu.Default := Translation().menu_pause
  A_TrayMenu.ClickCount := 1

  A_IconTip := "KeyFlux"
  TraySetIcon("./bin/icons/logo.ico", , true)
}


#HotIf
RAlt::LControl

#HotIf