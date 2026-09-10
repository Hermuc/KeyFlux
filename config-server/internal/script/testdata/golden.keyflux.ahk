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
InitTrayMenu()
InitKeymap()
OnExit(KeyFluxExit)
#include ../data/custom_functions.ahk

InitKeymap()
{
  taskSwitch := TaskSwitchKeymap("e", "d", "s", "f", "c", "space")
  mouseTip := InputTipWindow("🐶",,,, 20, 16)
  slow := MouseKeymap("slow mouse", false, mouseTip, 30, 150, "T200", "T600", 3, "T200", "T600")
  fast := MouseKeymap("fast mouse", false, mouseTip, 6, 60, "T200", "T600", 3, "T200", "T600", slow)
  slow.Map("*space", slow.LButtonUp())

  capsHook := InputHook("", "{CapsLock}{Esc}", "edit,expr,jk,multi,web")
  capsHook.KeyOpt("{CapsLock}", "S")
  capsHook.KeyOpt("{Backspace}", "N")
  capsHook.OnChar := (ih, char) => (PostCharToCaspAbbr(ih, char), FuzzySuffixFire(ih, char, "capslock"))
  capsHook.OnKeyDown := PostBackspaceToCaspAbbr
  Run("bin\KeyFlux-CommandInput.exe")

  semiHook := InputHook("", "{CapsLock}{Esc}{;}", ",,,sys")
  semiHook.KeyOpt("{CapsLock}", "S")
  semiHook.KeyOpt("{Backspace}", "N")
  semiHook.OnChar := (ih, char) => semiHookAbbrWindow.Show(char, true)
  semiHook.OnKeyDown := (ih, vk, sc) => semiHookAbbrWindow.Backspace()
  semiHookAbbrWindow := InputTipWindow()


  ; 路径变量
  editor := "D:\tools\edit.exe"
  desktop := A_Desktop

  ; 缩写命令注册表 (阶段 4: 取代 ExecCapslockAbbr 内的 switch)
  CommandResolver.Register("capslock", "edit", [CommandStep(() => Send("{blind}code"), "ahk_exe code.exe", 1)])
  CommandResolver.Register("capslock", "expr", [CommandStep(() => Run("calc.exe"), WinActive("A") && GetKeyState("Shift"), 5)])
  CommandResolver.Register("capslock", "jk", [CommandStep(() => Send("{blind}jk"))])
  CommandResolver.Register("capslock", "multi", [CommandStep(() => SystemLockScreen()), CommandStep(() => Send("{enter}"))])
  CommandResolver.Register("capslock", "web", [CommandStep(() => ActivateOrRun("", "chrome.exe"))])
  CommandResolver.Register("semicolon", ",", [CommandStep(() => Send("{enter}"))])
  CommandResolver.Register("semicolon", "sys", [CommandStep(() => SystemLockScreen(), "ahk_group MY_WINDOW_GROUP_2", 2)])
  ; 窗口组
  GroupAdd("MY_WINDOW_GROUP_2", "ahk_exe chrome.exe")
  GroupAdd("MY_WINDOW_GROUP_2", "ahk_exe firefox.exe")
  GroupAdd("GROUP_DISABLE_KEYMAP_6", "ahk_exe game1.exe")
  GroupAdd("GROUP_DISABLE_KEYMAP_6", "ahk_exe game2.exe")

  KeymapManager.GlobalKeymap.DisabledAt := ""

  ; Custom Hotkeys
  km1 := KeymapManager.NewKeymap("customHotkeys", "Custom Hotkeys", "", "")
  km := km1
  km.RemapInHotIf("a", "b")
  km.RemapInHotIf("c", "d", "ahk_exe code.exe", 1)
  km.RemapInHotIf("e", "f", "ahk_group MY_WINDOW_GROUP_2", 2)
  km.RemapInHotIf("g", "h", "ahk_class CabinetWClass", 3)
  km.RemapInHotIf("i", "j", "ahk_exe photoshop.exe", 4)
  km.RemapInHotIf("k", "l", 'WinActive("A") && GetKeyState("Shift")', 5)
  km.Map("!f17", _ => KeyFluxReload(), , , , "S")

  ; CapsLock
  km5 := KeymapManager.NewKeymap("*CapsLock", "CapsLock", "", "ahk_exe steam.exe")
  km := km5
  km.Map("*1", _ => ActivateOrRun("", "notepad.exe"))
  km.Map("*2", _ => SystemLockScreen())
  km.Map("*3", _ => SmartCloseWindow())
  km.Map("*4", _ => Send("^!{tab}"), taskSwitch)
  km.Map("*5", fast.MoveMouseUp, slow), slow.Map("*5", slow.MoveMouseUp)
  km.Map("*6", fast.ScrollWheelUp), slow.Map("*6", slow.ScrollWheelUp)
  km.RemapKey("t", "x")
  km.Map("*7", _ => (Send("hello"), Send("{text}world")))
  km.RemapKey("8", "up")
  km.Map("*9", _ => (Send("{blind}^{left}")))
  km.Map("*0", _ => MsgBox("hello"))
  km.Map("*e", _ => KeyFluxToggleSuspend(), , , , "S")
  km.Map("*q", _ => EnterCapslockAbbr(capsHook))
  km.Map("*r", km.ToggleLock)
  km.Map("*w", _ => EnterSemicolonAbbr(semiHook, semiHookAbbrWindow))

  ; 媒体控制
  km6 := KeymapManager.NewKeymap("*F13", "媒体控制", "", "ahk_group GROUP_DISABLE_KEYMAP_6")
  km := km6
  km.Map("m1", _ => ActivateOrRun("ahk_exe calc.exe", "calc.exe", "/auto", "D:\tools", true, false, false), , "ahk_exe code.exe", 1)
  km.Map("m2", _ => SoundControl(), , "ahk_group MY_WINDOW_GROUP_2", 2)
  km.Map("m5", fast.MoveMouseDown, slow, "ahk_exe code.exe", 1), slow.Map("m5", slow.MoveMouseDown, , "ahk_exe code.exe", 1)
  km.Map("m3", _ => (ToolTip("hi"), Sleep(500), Send("{enter}")))
  km.Map("m4", _ => HoldDownModifierKey("LShift"), , 'WinActive("A") && GetKeyState("Shift")', 5)
  km.Map("m6", _ => KeyFluxOpenSettings())
  km.Map("m7", _ => ToggleCapslock())

  ; ===== 选中动作 (单键分发) =====
  ; SelectedActionData 每项字段: matchType (匹配类型, 输出顺序 = 匹配优先级) / matchValue (条件值) / key (菜单序号 1-9) /
  ;   behavior (行为库 ID) / action (展开后基础动作) / actionValue (展开后模板) / workingDir (工作目录) / name (显示名)
  SelectedActionData := Array(
    {matchType: "textType", matchValue: "url", key: 1, behavior: "open_url", action: "open_url", actionValue: "", workingDir: "", name: "open_url"},
    {matchType: "textType", matchValue: "url", key: 2, behavior: "search", action: "search", actionValue: "https://www.bing.com/search?q=%selected%", workingDir: "", name: "search"},
    {matchType: "fileExt", matchValue: "jpg,png", key: 1, behavior: "open", action: "open", actionValue: "%selected%", workingDir: "", name: "open"},
  )
  SelectedActionInit(">^p", SelectedActionData)


  KeymapManager.GlobalKeymap.Enable()
}

ExecCapslockAbbr(command) {
  CommandResolver.Resolve("capslock", command)
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
a::b

#HotIf WinActive("ahk_exe code.exe")
c::d

#HotIf WinExist("ahk_group MY_WINDOW_GROUP_2")
e::f

#HotIf !WinActive("ahk_class CabinetWClass")
g::h

#HotIf !WinExist("ahk_exe photoshop.exe")
i::j

#HotIf 'WinActive("A") && GetKeyState("Shift")' 
k::l

#HotIf