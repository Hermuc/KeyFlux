#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, Off
; ============================================================
; open_guard_probe —— 「连点/按住 Enter 后成批弹出『文档 - 文件资源管理器』窗口」修复的回归探针。
;
; 被测对象 = 真源 ../src/EverythingSession.ahk (直接 #Include, 不复制不改写): 探针只 stub
;   **引擎全局**与 explorer 启动这一处副作用, 会话状态机与守卫链走的都是生产代码。
;
; 为什么要抽调用缝 (修复要求 4): AHK 无法 monkey-patch 内置 Run。生产代码把「启动资源管理器」
;   收敛成 EverythingExplorerRunner.Launch; 探针把 Impl 换成记录器, 于是能对「到底启动了几次
;   explorer / 命令串长什么样」做硬断言。
;
; 反向敏感度 (2026-09-30 实测, 守卫链整段拆回旧行为后再跑本探针): 20 项里 13 项变红 ——
;   2a-2c / 3a-3c / 4c / 4d / 5a / 5b / 6 / 7 / 8。其中
;   * 2b/3b/6 = 「空/失效路径也照样 Run」—— 就是 `explorer.exe ""` 打开「文档」目录的直接来源;
;   * 5a    = 同一会话内 400ms 内的重复通知第二次照样打开 —— 即「成批窗口」的第二道闸;
;   * 4c/4d = 相对路径原样喂 explorer / 尾反斜杠把命令行引号吃掉。
;   ⚠ 断言 1 在旧行为下同样是绿的: OnKey 入口**早已**有 `closed` 守卫, 重复通知根本到不了
;   Enter 分支 ⇒ 1 是「别把这道既有守卫改坏」的回归护栏; 真正判别「重复通知去抖」的是 5a。
;
; 断言 (覆盖修复要求 1-4, 另加 2 组补充):
;   1) 连走 3 次 Enter 分支 => Launch 只被调用 1 次 (关闭守卫生效), 会话关闭, 后续通知被拒;
;   2) path 为空 => 返回 false 且 Launch 调用数不增加;
;   3) path 指向不存在的路径 => 返回 false 且 Launch 调用数不增加 (并出一行既有提示);
;   4) 文件夹/文件两形态分别产生 `explorer.exe "<绝对路径>"` 与 `explorer.exe /select,"<绝对路径>"`;
;   5) 去抖窗口本身 (要求 2); 6) 全空白 path; 7) 会话关闭后无副作用; 8) 缝的默认实现仍走 Run (不过修)。
;
; 用法 (MSYS_NO_PATHCONV 必须有: Git Bash 会把 /ErrorStdOut 当路径改写):
;   MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut plugins/examples/everything_search/tests/open_guard_probe.ahk
; 退出码: 0 = 全绿; 1 = 有断言失败。
; 输出:  控制台 + %TEMP%\kf_open_guard_probe.txt (UTF-8, 权威记录)
; 副作用: 无 —— 不真启动 explorer、不建真窗口、不发按键 (断言 8 只用非法目标做冒烟)。
; ============================================================

global OUT_FILE := A_Temp "\kf_open_guard_probe.txt"
try FileDelete(OUT_FILE)
SetWorkingDir(A_ScriptDir)

global N_PASS := 0
global N_FAIL := 0

Emit(s) {
    global OUT_FILE
    try FileAppend(s "`n", OUT_FILE, "UTF-8")
    Try FileAppend(s "`n", "*")
}

Verify(cond, label, detail := "") {
    global N_PASS, N_FAIL
    if (cond) {
        N_PASS += 1
        Emit("[OK]   " label)
    } else {
        N_FAIL += 1
        Emit("[FAIL] " label (detail != "" ? "   -> " detail : ""))
    }
}

; ============================================================
; 引擎全局 stub: 只实现 EverythingSession 引用到的成员
; (EverythingMessages 用真源, 见下方 #Include —— 断言 3c 要对真文案)
; ============================================================

class EverythingDropdown {
    static Hints := []          ; 记录 ShowHint 收到的文案
    static Callback := 0
    static SetCallback(cb) {
        EverythingDropdown.Callback := cb
    }
    static Show(items, index) {
    }
    static ShowHint(text) {
        EverythingDropdown.Hints.Push(text)
    }
    static Select(index) {
    }
    static Hide() {
    }
}

class CommandDisplay {
    static SuppressKeycap := true
    static EchoChar(ih, c) {
    }
    static EchoBackspace(ih, vk, sc) {
    }
    static ActivateCommandWindow() {
    }
}

class CommandImeGuard {
    static UnlockForSearch(ih) {
    }
}

class CommandInputHooks {
    static ActivateBackend() {
    }
}

class SelectionContext {
    static Get(flag) {
        return {type: "text", content: ""}
    }
}

class EverythingSettings {
    static TriggerKey := " "
    static Limit := 20
    static Load(api) {
        return false
    }
}

class EverythingSearch {
    static Run(query, limit) {
        return {ok: false, error: 0, items: []}
    }
}

class EverythingProviders {
    static Reset() {
    }
}

/** 引擎侧函数 (EverythingMessages 依赖), 独立进程里没有 -> 最小 stub (中文口径)。 */
SysLangIsChinese() {
    return true
}

/** 会话结束输入钩子的最小替身 (Enter 分支会 try ih.Stop())。 */
class StubInputHook {
    static StopCount := 0
    Stop() {
        StubInputHook.StopCount += 1
        return true
    }
}

; ============================================================
; 被测真源
; ============================================================
#Include ../src/EverythingMessages.ahk
#Include ../src/EverythingSession.ahk

; ============================================================
; explorer 调用缝的记录实现
; ============================================================

class ExplorerRecorder {
    static Calls := []
    static Reset() {
        ExplorerRecorder.Calls := []
    }
    static Last() {
        return (ExplorerRecorder.Calls.Length = 0) ? "<无>" : ExplorerRecorder.Calls[ExplorerRecorder.Calls.Length]
    }
}

/** 缝的替换实现: 只记录命令串, 绝不真启动 explorer。 */
RecorderLaunch(cmd) {
    ExplorerRecorder.Calls.Push(cmd)
    return 4242
}

EverythingExplorerRunner.Impl := RecorderLaunch

; ============================================================
; 用例脚手架
; ============================================================

DIR_EXISTS := A_ScriptDir                          ; 必然存在的文件夹 (探针自身所在目录)
FILE_EXISTS := A_ScriptFullPath                    ; 必然存在的文件 (探针自身)
PATH_MISSING := A_ScriptDir "\__kf_open_guard_no_such__.txt"   ; 必然不存在

ItemFolder(path) {
    return {path: path, name: "", isFolder: true}
}

ItemFile(path) {
    return {path: path, name: "", isFolder: false}
}

/** 造一个「已在检索中」的会话 (跳过引擎握手与检索, 只测打开路径的守卫链)。 */
NewSession(items, index) {
    s := EverythingSession(0)
    s.chars := 1
    s.active := true
    s.closed := false
    s.capturing := false
    s.query := "probe"
    s.items := items
    s.index := index
    return s
}

/** 等价于「引擎又来一条 Enter 的 OnKeyDown 通知」。 */
PressEnter(s) {
    return s.OnKey(StubInputHook(), EverythingSession.VK_RETURN, 0x1C, "capsule")
}

ResetObservers() {
    ExplorerRecorder.Reset()
    EverythingDropdown.Hints := []
}

; ============================================================
; 断言
; ============================================================

Emit("=== open_guard_probe ===")
Emit("AHK = " A_AhkVersion "   " (A_PtrSize * 8) "bit")
Emit("被测真源 = " A_ScriptDir "\..\src\EverythingSession.ahk (经 #Include)")
Emit("存在项: 文件夹=" DIR_EXISTS "  文件=" FILE_EXISTS)
Emit("不存在项: " PATH_MISSING)
Emit("")

; --- 断言 1: 连按 3 次回车 => explorer 只启动 1 次 (关闭守卫) ---
ResetObservers()
StubInputHook.StopCount := 0
s1 := NewSession([ItemFolder(DIR_EXISTS), ItemFile(FILE_EXISTS), ItemFile(PATH_MISSING)], 1)
r1 := PressEnter(s1)
r2 := PressEnter(s1)
r3 := PressEnter(s1)
Verify(ExplorerRecorder.Calls.Length = 1,
    "1a 连按 3 次回车: Launch 只被调用 1 次 (关闭守卫)",
    "实际 " ExplorerRecorder.Calls.Length " 次; 命令串=" ExplorerRecorder.Last())
Verify(r1 = true, "1b 第 1 次回车被消费 (OnKey 返回 true)", "实际 " r1)
Verify(r2 = false && r3 = false, "1c 同一批重复通知被拒 (OnKey 返回 false)", "实际 " r2 "/" r3)
Verify(s1.closed = true && s1.active = false, "1d 会话已关闭且 active 已撤销")
Verify(StubInputHook.StopCount = 1,
    "1e 重复通知在 OnKey 入口即被拒, 未再走到 Enter 分支 (ih.Stop 只调 1 次)",
    "实际 " StubInputHook.StopCount " 次")

; --- 断言 2: path 为空 => 不启动 explorer, 返回 false ---
ResetObservers()
s2 := NewSession([ItemFile("")], 1)
ok2 := s2.OpenSelected()
Verify(ok2 = false, "2a path 为空: OpenSelected 返回 false", "实际 " ok2)
Verify(ExplorerRecorder.Calls.Length = 0,
    '2b path 为空: explorer 未被启动 (旧实现 explorer.exe "" 会打开「文档」)',
    "实际 " ExplorerRecorder.Calls.Length " 次")
Verify(EverythingDropdown.Hints.Length = 1 && EverythingDropdown.Hints[1] = EverythingMessages.T("hint_empty"),
    "2c path 为空: 出一行既有提示文案 (hint_empty)",
    "实际 " ((EverythingDropdown.Hints.Length = 1) ? "'" EverythingDropdown.Hints[1] "'" : "提示数 " EverythingDropdown.Hints.Length))

; --- 断言 3: path 不存在 => 不启动 explorer, 返回 false ---
ResetObservers()
s3 := NewSession([ItemFile(PATH_MISSING)], 1)
ok3 := s3.OpenSelected()
Verify(ok3 = false, "3a 路径不存在: OpenSelected 返回 false", "实际 " ok3)
Verify(ExplorerRecorder.Calls.Length = 0,
    "3b 路径不存在: explorer 未被启动",
    "实际 " ExplorerRecorder.Calls.Length " 次")
Verify(EverythingDropdown.Hints.Length = 1 && EverythingDropdown.Hints[1] = EverythingMessages.T("hint_empty"),
    "3c 路径不存在: 出一行既有提示文案 (hint_empty)",
    "实际 " ((EverythingDropdown.Hints.Length = 1) ? "'" EverythingDropdown.Hints[1] "'" : "提示数 " EverythingDropdown.Hints.Length))

; --- 断言 4: 两种形态的命令串 + 绝对路径规范化 ---
ResetObservers()
s4 := NewSession([ItemFolder(DIR_EXISTS)], 1)
ok4 := s4.OpenSelected()
Verify(ok4 && ExplorerRecorder.Calls.Length = 1 && ExplorerRecorder.Calls[1] = 'explorer.exe "' DIR_EXISTS '"',
    '4a 文件夹: explorer.exe "<绝对路径>"',
    "实际 " ExplorerRecorder.Last())

ResetObservers()
s5 := NewSession([ItemFile(FILE_EXISTS)], 1)
ok5 := s5.OpenSelected()
Verify(ok5 && ExplorerRecorder.Calls.Length = 1 && ExplorerRecorder.Calls[1] = 'explorer.exe /select,"' FILE_EXISTS '"',
    '4b 文件: explorer.exe /select,"<绝对路径>"',
    "实际 " ExplorerRecorder.Last())

; 4c: 相对路径必须补成绝对路径 (A_WorkingDir 已 SetWorkingDir 到探针目录)
ResetObservers()
s6 := NewSession([ItemFile("open_guard_probe.ahk")], 1)
ok6 := s6.OpenSelected()
Verify(ok6 && ExplorerRecorder.Calls.Length = 1 && ExplorerRecorder.Calls[1] = 'explorer.exe /select,"' A_ScriptFullPath '"',
    "4c 相对路径: 规范化成绝对路径后才喂 explorer",
    "实际 " ExplorerRecorder.Last())

; 4d: 尾部反斜杠必须去掉 (否则命令行里 `\` + `"` 会被解析成转义引号, 参数截断)
ResetObservers()
s7 := NewSession([ItemFolder(DIR_EXISTS "\")], 1)
ok7 := s7.OpenSelected()
Verify(ok7 && ExplorerRecorder.Calls.Length = 1 && ExplorerRecorder.Calls[1] = 'explorer.exe "' DIR_EXISTS '"',
    '4d 尾部反斜杠: 规范化后命令串不含 \ + " 的坏引号',
    "实际 " ExplorerRecorder.Last())

; --- 补充断言 5: 去抖窗口本身 (要求 2) ---
;     断言 1 由「关闭守卫」兜住; 这里把会话强行重新激活, 隔离出 lastOpenTick 去抖。
ResetObservers()
s8 := NewSession([ItemFolder(DIR_EXISTS)], 1)
PressEnter(s8)                                   ; 第 1 次: 成功打开并记下 lastOpenTick
s8.closed := false
s8.active := true                                ; 模拟「同一会话又来一条通知」
ok8 := s8.OpenSelected()
Verify(ExplorerRecorder.Calls.Length = 1 && ok8 = false,
    "5a 成功打开后 400ms 内的重复调用被去抖拒绝",
    "实际调用 " ExplorerRecorder.Calls.Length " 次, 返回 " ok8)
Sleep 450
ok9 := s8.OpenSelected()
Verify(ok9 = true && ExplorerRecorder.Calls.Length = 2,
    "5b 超过去抖窗口 (400ms) 后恢复正常打开",
    "实际调用 " ExplorerRecorder.Calls.Length " 次, 返回 " ok9)

; --- 补充断言 6: 全空白 path 同样被拦 (es.exe 可能回传空白行) ---
ResetObservers()
s9 := NewSession([ItemFile("   ")], 1)
ok10 := s9.OpenSelected()
Verify(ok10 = false && ExplorerRecorder.Calls.Length = 0,
    "6 全空白 path: 与空 path 同等被拦 (Trim 后为空)",
    "实际调用 " ExplorerRecorder.Calls.Length " 次, 返回 " ok10)

; --- 收尾: 会话结束后再来通知必须无副作用 (回归护栏) ---
ResetObservers()
s10 := NewSession([ItemFolder(DIR_EXISTS)], 1)
s10.Close()
ok11 := s10.OpenSelected()
Verify(ok11 = false && ExplorerRecorder.Calls.Length = 0,
    "7 会话关闭后直接调 OpenSelected: 返回 false 且无副作用",
    "实际调用 " ExplorerRecorder.Calls.Length " 次, 返回 " ok11)

; --- 断言 8: 缝的**默认**实现仍是生产路径 (未替换 -> 内置 Run) ---
;     过修方向的护栏: 守卫链再严, 也不能把「正常打开」一起挡掉。
;     用非法目标做无窗口冒烟: 若默认路径真的调了内置 Run, 目标不存在必然抛错;
;     不抛错 => 说明 Launch 其实没走到 Run (例如被重构改成空实现), 这里必须红。
EverythingExplorerRunner.Impl := 0
threw := false
try
    EverythingExplorerRunner.Launch('::kf_probe_definitely_not_a_target::')
catch
    threw := true
Verify(threw,
    "8 未替换 Impl 时走内置 Run (生产路径): 非法目标抛错",
    "实际 threw=" threw)
EverythingExplorerRunner.Impl := RecorderLaunch          ; 复原缝

; ============================================================
; 汇总
; ============================================================
total := N_PASS + N_FAIL
Emit("")
Emit("RESULT: " N_PASS "/" total (N_FAIL > 0 ? "  FAIL" : "  PASS"))
Try FileAppend("`nRESULT: " N_PASS "/" total (N_FAIL > 0 ? "  FAIL" : "  PASS") "`n", "*")

ExitApp(N_FAIL > 0 ? 1 : 0)
