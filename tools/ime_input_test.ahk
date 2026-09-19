#Requires AutoHotkey v2.0
#SingleInstance Off
; ============================================================
; ime_input_test —— 「无 V 的 InputHook 收不到 IME 中文」前提回归探针。
;
; 为什么需要它 (2026-09-19 定位结论):
;   用户报「命令框里切中文输入法打不出中文」。定位过程中**证伪**了两个前提:
;     ① 「原软件对输入法有硬性限制」—— 代码里不存在 (DisableIME 全仓 0 调用点);
;     ② 「皮肤 keyColor/keyOpacity 能只去八角框留字」—— 不可行 (描边与字符同一支画刷)。
;   真正根因 (v3 实证修正): capsHook 以 InputHook("",…) 创建, 无 V ⇒ 默认不可见, **吞掉
;   文本键** —— 物理键到不了焦点窗口的 IME 上下文, 组合无从发生 (官方文档的 "does not
;   support IME" 即指此: 钩子在按键抵达 IME 之前就用 ToUnicodeEx/ToAsciiEx 翻译成 ASCII)。
;   本探针把该结论固化为**可自动重跑**的断言 —— 一旦上游 AHK 版本改变了这一行为,
;   本探针会红, 提示更新 §3.12 的断言前提 (而非退役模块)。
;
; 现行解法 (2026-09-19 v3): IME 开启的会话由 MakeCapsHook 建可见 hook (V) 透传物理键,
;   IME 原生组合/上屏; IME 关闭的会话维持历史形态。本探针断言的是「无 V 时收不到中文」
;   这一理论基础, 与 V 方案不冲突 (探针自身 hook 无 V, 证的是默认形态)。
;
; 自动化手法 (无需人工打字):
;   * InputHook 的钩子能看到 **SendEvent** 注入的按键, 但看不到 SendInput 的
;     (SendInput 期间 AHK 临时卸载自己的钩子) —— 故必须 SendMode("Event");
;   * 用 ImmSetOpenStatus 程序化开/关当前输入法, 并用 ImmGetOpenStatus 回读确认,
;     不依赖用户手动切输入法 (否则断言不可重跑)。
;
; ⚠ 前置条件: KeyFlux 必须已退出 —— 它的 #UseHook + High 优先级独占键盘钩子,
;   本探针将收不到任何按键 (症状 = 所有用例 0 字符)。
;
; 用法: bin/AutoHotkey64.exe /ErrorStdOut tools/ime_input_test.ahk
;   退出码 0 = 全绿, 1 = 有断言失败。
;
; 输出: %TEMP%\kf_ime_input_test.txt  (详细)  与  %TEMP%\kf_ime_input_keys.txt (按键明细)
; ============================================================

global LOGFILE := A_Temp "\kf_ime_input_test.txt"
global KEYFILE := A_Temp "\kf_ime_input_keys.txt"
try FileDelete(LOGFILE)
try FileDelete(KEYFILE)

WriteLog(s) {
    global LOGFILE
    try FileAppend(s "`n", LOGFILE, "UTF-8")
}
WriteKey(s) {
    global KEYFILE
    try FileAppend(FormatTime(, "HH:mm:ss.fff") "  " s "`n", KEYFILE, "UTF-8")
}

global g_pass := 0
global g_fail := 0

Check(cond, label, detail := "") {
    global g_pass, g_fail
    if (cond) {
        g_pass += 1
        Try FileAppend("[OK]   " label "`n", "*")
    } else {
        g_fail += 1
        Try FileAppend("[FAIL] " label (detail != "" ? "   -> " detail : "") "`n", "*")
    }
    WriteLog((cond ? "[OK]   " : "[FAIL] ") label (detail != "" ? "   -> " detail : ""))
}

WriteLog("=== ime_input_test ===")
WriteLog("AHK = " A_AhkVersion "  " (A_PtrSize * 8) "bit")

; ---- 接收窗口 (IME 需要附着对象, 否则 ImmSetOpenStatus 无效) ----
g := Gui("+AlwaysOnTop +ToolWindow", "kf_ime_sink")
g.MarginX := 0
g.MarginY := 0
g.Add("Edit", "w420 h60 ReadOnly vSink")
g.Show("x120 y120 w420 h60")
global SINK := g.Hwnd

ImeOpen(open) {
    global SINK
    hIMC := 0
    try hIMC := DllCall("imm32\ImmGetContext", "ptr", SINK, "ptr")
    if (!hIMC)
        return false
    try DllCall("imm32\ImmSetOpenStatus", "ptr", hIMC, "int", open ? 1 : 0)
    try DllCall("imm32\ImmReleaseContext", "ptr", SINK, "ptr", hIMC)
    return true
}
ImeIsOpen() {
    global SINK
    hIMC := 0
    try hIMC := DllCall("imm32\ImmGetContext", "ptr", SINK, "ptr")
    if (!hIMC)
        return -1
    v := -1
    try v := DllCall("imm32\ImmGetOpenStatus", "ptr", hIMC)
    try DllCall("imm32\ImmReleaseContext", "ptr", SINK, "ptr", hIMC)
    return v
}

WriteLog("sink hwnd = " SINK)

; ---- 前提 0: 机器必须有可用输入法 ----
; 断言「本机能程序化切换输入法」是本探针其余断言成立的前提。
hIMC0 := 0
try hIMC0 := DllCall("imm32\ImmGetContext", "ptr", SINK, "ptr")
Check(hIMC0 != 0, "接收窗口可取得 IME 上下文 (ImmGetContext)", "hIMC=" hIMC0)
if (hIMC0)
    try DllCall("imm32\ImmReleaseContext", "ptr", SINK, "ptr", hIMC0)

tid := 0
try tid := DllCall("GetWindowThreadProcessId", "ptr", SINK, "ptr", 0)
hkl := 0
try hkl := DllCall("GetKeyboardLayout", "uint", tid, "ptr")
WriteLog("HKL = " Format("{:08X}", hkl))
Check(hkl != 0, "可读取键盘布局 (GetKeyboardLayout)", "HKL=" Format("{:08X}", hkl))

; ---- InputHook ----
global g_chars := []
global g_keys := []

ih := InputHook("L0", "{Esc}")
ih.KeyOpt("{Backspace}", "N")
ih.KeyOpt("{Enter}", "N")
ih.KeyOpt("{Space}", "N")
ih.OnChar := (h, c) => g_chars.Push(c)
ih.OnKeyDown := (h, vk, sc) => g_keys.Push(vk)
ih.Start()
Sleep 150

/**
 * 注入一串按键并收集回调。
 * @returns {{n:Integer, cjk:Boolean, text:String}}
 */
Inject(wantOpen, keys, submit) {
    global g_chars, g_keys, SINK
    ImeOpen(wantOpen)
    Sleep 220
    openNow := ImeIsOpen()

    g_chars := []
    g_keys := []

    try WinActivate("ahk_id " SINK)
    Sleep 90

    SendMode("Event")
    SetKeyDelay(12, 8)
    for k in keys
        SendEvent("{" k "}")
    Sleep 70
    if (submit != "")
        SendEvent("{" submit "}")
    Sleep 220

    txt := ""
    cjk := false
    for c in g_chars {
        txt .= c
        if (Ord(c) > 127)
            cjk := true
    }
    return {n: g_chars.Length, cjk: cjk, text: txt, open: openNow}
}

; ============================================================
; 断言组
; ============================================================

; --- A) SendEvent 注入的按键能被 InputHook 看到 (本探针一切结论的前提) ---
a := Inject(false, ["a", "s", "d", "f"], "")
Check(a.n = 4, "前提: SendEvent 注入按键可被 InputHook 观察到", "收到 " a.n " / 期望 4")
Check(a.text = "asdf", "前提: 注入的 ASCII 原样回流", "收到 '" a.text "'")
Check(a.cjk = false, "对照: 关 IME 时无中文码点 (正常)")

; --- B) 核心结论: 开 IME 后拼音串仍以 ASCII 回流, 「中」不出现 ---
;
; 这是本探针存在的意义: 把「无 V 收不到 IME 中文」变成可自动重跑的断言。若某天本组断言
; 变红 (收到中文码点), 说明 AHK 默认形态已透传 IME —— 应更新 §3.12 的断言前提, 而非退役。
b1 := Inject(true, ["z", "h", "o", "n", "g"], "Space")
Check(b1.open = 1, "IME 确实处于开启态 (ImmGetOpenStatus=1)", "实测 open=" b1.open)
Check(b1.n > 0, "开 IME 时仍能收到按键 (钩子未被 IME 吞掉)", "收到 " b1.n)
Check(b1.cjk = false,
    "核心: 开 IME 打 zhong+空格, 无 V 的 InputHook 收不到中文码点 (默认形态吞文本键)",
    "收到 '" b1.text "' —— 若含中文, 说明 AHK 默认形态已透传 IME, 需更新 §3.12 断言前提")
Check(InStr(b1.text, "zhong") > 0,
    "核心: 收到的是拼音 ASCII 原文 (证明钩子在 IME 之前翻译了按键)",
    "收到 '" b1.text "'")

b2 := Inject(true, ["z", "h", "o", "n", "g"], "Enter")
Check(b2.cjk = false, "核心: 开 IME 打 zhong+回车同样收不到中文", "收到 '" b2.text "'")

b3 := Inject(true, ["z","h","o","n","g","k","o","n","g"], "Space")
Check(b3.cjk = false, "核心: 更长拼音串同样只有 ASCII", "收到 '" b3.text "'")

; --- C) 中文输入法开启不影响 ASCII 直输 (回归护栏) ---
c1 := Inject(true, ["a", "s", "c", "i", "i"], "Space")
Check(c1.text = "ascii ", "开 IME 时 ASCII 直输仍正确", "收到 '" c1.text "'")

; --- D) 收尾: 关 IME 恢复 ---
d1 := Inject(false, ["a", "s", "d", "f"], "")
Check(d1.text = "asdf", "收尾: 关 IME 后 ASCII 输入正常", "收到 '" d1.text "'")
Check(ImeIsOpen() = 0 || ImeIsOpen() = -1, "收尾: IME 已回到关闭态")

ih.Stop()

; ---- 用例数值落盘 (取证用) ----
try {
    WriteLog("")
    WriteLog("=== 用例数值汇总 ===")
    WriteLog("A 关IME asdf         : n=" a.n " text='" a.text "' cjk=" a.cjk)
    WriteLog("B1 开IME zhong+空格  : n=" b1.n " text='" b1.text "' cjk=" b1.cjk " open=" b1.open)
    WriteLog("B2 开IME zhong+回车  : n=" b2.n " text='" b2.text "' cjk=" b2.cjk " open=" b2.open)
    WriteLog("B3 开IME zhongkong   : n=" b3.n " text='" b3.text "' cjk=" b3.cjk " open=" b3.open)
    WriteLog("C1 开IME ascii+空格  : n=" c1.n " text='" c1.text "' cjk=" c1.cjk " open=" c1.open)
    WriteLog("D1 关IME asdf        : n=" d1.n " text='" d1.text "' cjk=" d1.cjk)

    WriteLog("")
    WriteLog("=== 结论 ===")
    WriteLog("开 IME 下所有用例均无中文码点 => 无 V 的 InputHook 吞掉文本键, IME 收不到,")
    WriteLog("中文组合不可能发生 (§3.12 理论基础)。故 IME 开启的会话必须用可见 hook (V)")
    WriteLog("透传物理键, 由 MakeCapsHook 动态选择 (2026-09-19 v3 已落地)。")
}

try g.Destroy()

total := g_pass + g_fail
; 结论行保持纯 ASCII: 经 make 管道时 AHK stdout 走控制台码页 (非 UTF-8), 中文会乱码
Try FileAppend("`nRESULT: " g_pass "/" total (g_fail > 0 ? "  FAIL" : "  PASS") "`n", "*")

if (g_fail > 0)
    ExitApp(1)
ExitApp(0)
