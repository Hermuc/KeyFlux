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
; 反向敏感度 (2026-09-30 实测; 两组对照都跑在临时副本上, 未提交) —— 本探针不是恒绿:
;   * 守卫链整段拆回旧行为 => 27 项里 **16 项变红** (2a-2c / 3a-3c / 4c / 4d / 5a / 5b / 6 /
;     7 / 9b / 9c / 9d / 9e)。其中
;       2b/3b/6 = 「空/失效路径照样 Run」—— 就是 `explorer.exe ""` 打开「文档」目录的直接来源;
;       5a      = 同一会话内 400ms 内的重复通知第二次照样打开 —— 即「成批窗口」的第二道闸;
;       4c/4d   = 相对路径原样喂 explorer / 尾反斜杠把命令行引号吃掉;
;       9c/9d   = 失败路径还会把浮层和输入钩子一起收掉 (提示看不见 = 用户视角「回车没反应」)。
;   * 只把 Enter 分支退回「无条件 Close + ih.Stop」(其余保持新代码) => 恰好 9c/9d 两项变红
;     ⇒ 第 9 组精确锁住「失败可见」这一条, 不会靠别的断言兜住。
;   ⚠ 断言 1 在旧行为下同样是绿的: OnKey 入口**早已**有 `closed` 守卫, 重复通知根本到不了
;   Enter 分支 ⇒ 1 是「别把这道既有守卫改坏」的回归护栏; 真正判别「重复通知去抖」的是 5a。
;
; 断言 (覆盖修复要求 1-4 + 收尾两项, 另加 4 组补充):
;   1) 连走 3 次 Enter 分支 => Launch 只被调用 1 次 (关闭守卫生效), 会话关闭, 后续通知被拒;
;   2) path 为空 => 返回 false 且 Launch 调用数不增加;
;   3) path 指向不存在的路径 => 返回 false 且 Launch 调用数不增加 (并出一行提示);
;   4) 文件夹/文件两形态分别产生 `explorer.exe "<绝对路径>"` 与 `explorer.exe /select,"<绝对路径>"`;
;   5) 去抖窗口本身; 6) 全空白 path; 7) 会话关闭后无副作用; 8) 缝的默认实现仍走 Run (不过修);
;   9) 失败不静默: 路径失效时浮层不 Hide / ih.Stop 不调 / 会话仍 active (9a-9e), 成功时各 1 次 (9f);
;  10) 新键 err_item_missing 中英双分支都有真文案 (无回落键名);
;  11) 鼠标点选 (OnPick) 与 Enter 走同一条守卫链 (2026-10-01 扩): 成功才收浮层 (11a),
;      失效路径不启动 explorer + 出提示 (11b) 且浮层/会话保留 (11c), 未匹配 path 无副作用 (11d)。
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
; 引擎依赖 stub —— 2026-10-03 端口化后改为 **EverythingHost.Impl 整缝替换**:
;   引擎依赖 (CommandInputHooks/CommandDisplay/CommandImeGuard/SelectionContext/
;   SysLangIsChinese) 一律经真源 src/EverythingHost.ahk, 探针把 Impl 换成
;   HostRecorder (见下方) 即可观测全部引擎交互 —— 不再逐个 stub 引擎全局类。
; 插件内部**非被测**真源仍用 stub (各自真源未 #Include):
;   EverythingDropdown / EverythingSettings / EverythingSearch / EverythingProviders。
; EverythingMessages 用真源 (见下方 #Include —— 断言 3c 要对真文案)。
; ============================================================

class EverythingDropdown {
    static Hints := []          ; 记录 ShowHint 收到的文案
    static HideCount := 0       ; 记录 Hide 次数 (浮层是否被收起)
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
        EverythingDropdown.HideCount += 1
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
    static RunCount := 0
    static Run(query, limit) {
        EverythingSearch.RunCount += 1
        return {ok: false, error: 0, items: []}
    }
}

class EverythingProviders {
    static Reset() {
    }
}

/** Session.__New 会复位 GUI 降级额度 (真源 Providers 未 #Include -> 最小 stub)。 */
class EverythingGuiProvider {
    static AllowLaunch := true
}

/**
 * 查询输入面 stub (真源 src/EverythingQueryEdit.ahk 未 #Include): 记录 Show/Hide/Focus
 * 次数; SimText 模拟「用户在 Edit 里的输入」(含 IME 上屏), 供轮询同步断言。
 */
class EverythingQueryEdit {
    static ShowCount := 0
    static HideCount := 0
    static FocusCount := 0
    static SimText := ""
    static Show(rect, initial := "") {
        EverythingQueryEdit.ShowCount += 1
        EverythingQueryEdit.SimText := initial
    }
    static Hide() {
        EverythingQueryEdit.HideCount += 1
    }
    static Focus() {
        EverythingQueryEdit.FocusCount += 1
    }
    static SetText(t) {
        EverythingQueryEdit.SimText := t
    }
    static GetText() {
        return EverythingQueryEdit.SimText
    }
}

; 错误码常量 (真源 src/EverythingProviders.ahk 顶部; 探针未包含该文件,
; 但断言 12 的 OnChar→Refresh→_ErrorKey 路径会比较它们 —— 补齐避免未定义全局)
global ES_ERR_NOT_FOUND := "es-not-found"
global ES_ERR_NOT_RUNNING := "everything-down"
global ES_ERR_EXPORT := "export-failed"
global ES_ERR_QUERY := "query-failed"
global ES_ERR_NO_ES_LAUNCHED := "gui-launched"
global ES_ERR_LAUNCH_FAILED := "launch-failed"
global ES_ERR_EMPTY := "empty-query"
global ES_ERR_NO_PATH := "no-everything-path"

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
#Include ../src/EverythingHost.ahk
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
; EverythingHost 端口的记录实现 (引擎依赖整缝替换, 见顶部 stub 策略说明)
; ============================================================

class HostRecorder {
    static Calls := []
    static Reset() {
        HostRecorder.Calls := []
    }
    /** 按方法名统计调用次数。 */
    static Count(name) {
        n := 0
        for c in HostRecorder.Calls
            if (c[1] = name)
                n += 1
        return n
    }
    static RegisterCommandHook(ctrl) {
        HostRecorder.Calls.Push(["RegisterCommandHook"])
        return true
    }
    static ActivateBackend() {
        HostRecorder.Calls.Push(["ActivateBackend"])
    }
    static EchoChar(ih, c) {
        HostRecorder.Calls.Push(["EchoChar", c])
    }
    static EchoBackspace(ih, vk, sc) {
        HostRecorder.Calls.Push(["EchoBackspace"])
    }
    static ActivateCommandWindow() {
        HostRecorder.Calls.Push(["ActivateCommandWindow"])
    }
    static UnlockForSearch(ih) {
        HostRecorder.Calls.Push(["UnlockForSearch"])
    }
    static GetSelection(wait) {
        HostRecorder.Calls.Push(["GetSelection"])
        return {type: "text", content: ""}
    }
    static IsChinese() {
        HostRecorder.Calls.Push(["IsChinese"])
        return true
    }
    static HideCommandBox() {
        HostRecorder.Calls.Push(["HideCommandBox"])
    }
    static ResetAnchorCache() {
        HostRecorder.Calls.Push(["ResetAnchorCache"])
    }
    static CommandBoxAnchor() {
        HostRecorder.Calls.Push(["CommandBoxAnchor"])
        return {x: 100, y: 100, w: 841, h: 96, bottom: 196}   ; 假锚点: 让输入面/浮层几何走真实代码
    }
}

EverythingHost.Impl := HostRecorder

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
    HostRecorder.Reset()
    EverythingDropdown.Hints := []
    EverythingDropdown.HideCount := 0
    StubInputHook.StopCount := 0
    EverythingSearch.RunCount := 0
    EverythingQueryEdit.SimText := ""
    EverythingQueryEdit.ShowCount := 0
    EverythingQueryEdit.HideCount := 0
    EverythingQueryEdit.FocusCount := 0
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
Verify(EverythingDropdown.Hints.Length = 1 && EverythingDropdown.Hints[1] = EverythingMessages.T("err_item_missing"),
    "2c path 为空: 出一行提示文案 (err_item_missing)",
    "实际 " ((EverythingDropdown.Hints.Length = 1) ? "'" EverythingDropdown.Hints[1] "'" : "提示数 " EverythingDropdown.Hints.Length))

; --- 断言 3: path 不存在 => 不启动 explorer, 返回 false ---
ResetObservers()
s3 := NewSession([ItemFile(PATH_MISSING)], 1)
ok3 := s3.OpenSelected()
Verify(ok3 = false, "3a 路径不存在: OpenSelected 返回 false", "实际 " ok3)
Verify(ExplorerRecorder.Calls.Length = 0,
    "3b 路径不存在: explorer 未被启动",
    "实际 " ExplorerRecorder.Calls.Length " 次")
Verify(EverythingDropdown.Hints.Length = 1 && EverythingDropdown.Hints[1] = EverythingMessages.T("err_item_missing"),
    "3c 路径不存在: 出一行提示文案 (err_item_missing)",
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

; --- 断言 9: 失败不再静默 (Enter 分支改成「仅成功才 Close + ih.Stop」) ---
;     9a-9e: 路径失效 -> explorer 不启动、浮层不 Hide、输入钩子不 Stop (= 提示留在屏上)、
;            会话仍 active (可继续改检索词); 9f: 成功 -> 两者各恰好 1 次。
ResetObservers()
s11 := NewSession([ItemFile(PATH_MISSING)], 1)
r11 := PressEnter(s11)
Verify(r11 = true, "9a 路径失效的 Enter 仍被消费 (OnKey 返回 true)", "实际 " r11)
Verify(ExplorerRecorder.Calls.Length = 0,
    "9b 路径失效: explorer 未被启动", "实际 " ExplorerRecorder.Calls.Length " 次")
Verify(EverythingDropdown.HideCount = 0 && StubInputHook.StopCount = 0,
    "9c 路径失效: 浮层未 Hide、输入钩子未 Stop (= 提示留在屏上, 用户看得见)",
    "实际 Hide=" EverythingDropdown.HideCount " Stop=" StubInputHook.StopCount)
Verify(s11.closed = false && s11.active = true,
    "9d 路径失效: 会话保持打开, 可继续改检索词或按 Esc 退出",
    "实际 closed=" s11.closed " active=" s11.active)
Verify(EverythingDropdown.Hints.Length = 1 && EverythingDropdown.Hints[1] = EverythingMessages.T("err_item_missing"),
    "9e 路径失效: 提示文案 = err_item_missing",
    "实际提示数 " EverythingDropdown.Hints.Length)
ResetObservers()
s12 := NewSession([ItemFolder(DIR_EXISTS)], 1)
r12 := PressEnter(s12)
Verify(r12 = true && s12.closed = true && EverythingDropdown.HideCount = 1 && StubInputHook.StopCount = 1,
    "9f 成功: 会话关闭, Hide 与 ih.Stop 各调用 1 次",
    "实际 Hide=" EverythingDropdown.HideCount " Stop=" StubInputHook.StopCount " closed=" s12.closed)

; --- 断言 10: 新键 err_item_missing 在中英两条分支都是真文案 (不是回落成键名) ---
keyZh := EverythingMessages.T("err_item_missing")
EverythingMessages.En := true     ; 强行切英文分支 (静态字段可写; _ready 已 true, 不会再探测系统语言)
keyEn := EverythingMessages.T("err_item_missing")
EverythingMessages.En := false    ; 复原中文分支
Verify(keyZh != "err_item_missing" && keyEn != "err_item_missing" && keyZh != keyEn,
    "10 err_item_missing 中英双分支都有真文案 (无回落键名)",
    "zh='" keyZh "' en='" keyEn "'")

; --- 断言 11: 鼠标点选 (OnPick) 走同一守卫链 (2026-10-01 扩) ---
;     OnPick 旧实现无条件 OpenSelected + Close —— 失效路径的提示被紧随的 Hide 立刻收起,
;     与 2026-09-30 Enter 分支修的是同一症状。新实现「仅成功才 Close」。
ResetObservers()
s13 := NewSession([ItemFolder(DIR_EXISTS)], 1)
s13.OnPick(DIR_EXISTS)                               ; 点选一个有效文件夹
Verify(ExplorerRecorder.Calls.Length = 1 && s13.closed = true && EverythingDropdown.HideCount = 1,
    "11a 点选有效项: Launch 1 次, 会话关闭, 浮层 Hide 1 次",
    "实际 Launch=" ExplorerRecorder.Calls.Length " closed=" s13.closed " Hide=" EverythingDropdown.HideCount)

ResetObservers()
s14 := NewSession([ItemFile(PATH_MISSING)], 1)
s14.OnPick(PATH_MISSING)                             ; 点选一个已失效的结果
Verify(ExplorerRecorder.Calls.Length = 0 && EverythingDropdown.Hints.Length = 1
    && EverythingDropdown.Hints[1] = EverythingMessages.T("err_item_missing"),
    "11b 点选失效项: explorer 未启动, 出 err_item_missing 提示",
    "实际 Launch=" ExplorerRecorder.Calls.Length " 提示数 " EverythingDropdown.Hints.Length)
Verify(EverythingDropdown.HideCount = 0 && StubInputHook.StopCount = 0 && s14.closed = false && s14.active = true,
    "11c 点选失效项: 浮层未收、会话保留 (提示留在屏上, 可继续检索)",
    "实际 Hide=" EverythingDropdown.HideCount " closed=" s14.closed " active=" s14.active)

ResetObservers()
s15 := NewSession([ItemFolder(DIR_EXISTS)], 1)
s15.OnPick(A_ScriptDir "\__kf_not_in_list__.txt")    ; 点选不在结果里的 path
Verify(ExplorerRecorder.Calls.Length = 0 && EverythingDropdown.HideCount = 0 && s15.closed = false,
    "11d 点选未匹配 path: 循环不命中, 无任何副作用",
    "实际 Launch=" ExplorerRecorder.Calls.Length " Hide=" EverythingDropdown.HideCount " closed=" s15.closed)

; --- 断言 12: 引擎依赖全部经 EverythingHost 端口 (2026-10-03 端口化) ---
;     触发键进入搜索模式 => GetSelection (经 SeedFromSelection) 1 次 (锁英, 见断言 14);
;     激活后的普通字符 => EchoChar 1 次且检索词追加; 退格 => EchoBackspace 1 次。
;     这组断言同时是「别把直连引擎全局的旧写法改回来」的反向护栏 (直连时 HostRecorder
;     全零, 12a-12c 必红)。
ResetObservers()
s16 := EverythingSession(0)
r13 := s16.OnChar(StubInputHook(), " ", "probe")     ; 前置位置的触发键
Verify(r13 = true && HostRecorder.Count("GetSelection") = 1 && HostRecorder.Count("UnlockForSearch") = 1,
    "12a 触发键: GetSelection 与 UnlockForSearch 各走 1 次 (透传给查询输入面)",
    "实际 r=" r13 " Unlock=" HostRecorder.Count("UnlockForSearch") " GetSel=" HostRecorder.Count("GetSelection"))
r14 := s16.OnChar(StubInputHook(), "a", "probe")     ; 激活后的普通字符
Verify(r14 = true && s16.query = "" && HostRecorder.Count("EchoChar") = 0,
    "12b 激活后字符: 只消费 (文本由查询输入面原生持有, 经轮询同步)",
    "实际 r=" r14 " query=" s16.query " EchoChar=" HostRecorder.Count("EchoChar"))
r15 := s16.OnKey(StubInputHook(), EverythingSession.VK_BACK, 0, "probe")
Verify(r15 = true && HostRecorder.Count("EchoBackspace") = 0 && s16.query = "",
    "12c 退格: 只消费 (Edit 原生删字符, 轮询同步检索词)",
    "实际 r=" r15 " EchoBackspace=" HostRecorder.Count("EchoBackspace") " query=" s16.query)

; --- 断言 13: 空检索词 = 初始态, 不出任何浮层 (2026-10-03 需求) ---
;     触发后取不到选中文字 => 检索词空 => Refresh 应只 Hide (不出提示行)。
;     此前会弹「没有选中文字 — 继续输入检索词」提示框 —— 「框下另挂独立框」观感来源之一。
ResetObservers()
s17 := EverythingSession(0)
r16 := s17.OnChar(StubInputHook(), " ", "probe")     ; 触发键; GetSelection(记录器) 返回空
Verify(r16 = true && EverythingDropdown.HideCount = 1 && EverythingDropdown.Hints.Length = 0,
    "13a 空检索词: 不出提示浮层, 浮层 Hide 恰 1 次 (命令框保持初始态)",
    "实际 r=" r16 " Hide=" EverythingDropdown.HideCount " Hints=" EverythingDropdown.Hints.Length)
ResetObservers()
r17 := s17.OnKey(StubInputHook(), EverythingSession.VK_BACK, 0, "probe")
Verify(r17 = true && s17.query = "" && HostRecorder.Count("EchoBackspace") = 0,
    "13b 空检索词下退格: 只消费 (Edit 原生处理), 检索词保持空",
    "实际 r=" r17 " query=" s17.query " EchoBackspace=" HostRecorder.Count("EchoBackspace"))

; --- 断言 14: 查询输入面 (2026-10-03 Flow 式定版) ---
;     14a 触发 => 隐藏命令框 + 显示输入面 + 焦点 (IME 随焦点附着);
;     14b 用户在 Edit 里输入 (模拟 IME 上屏 "临时") => 轮询并入检索词并重查;
;     14c OnChar 不再触碰检索词 (Edit 原生持有, 防双份处理);
;     14d Close => 输入面 Hide 恰 1 次。
ResetObservers()
s18 := EverythingSession(0)
r18 := s18.OnChar(StubInputHook(), " ", "probe")     ; 触发
Verify(r18 = true && EverythingQueryEdit.ShowCount = 1 && HostRecorder.Count("HideCommandBox") = 1
    && HostRecorder.Count("UnlockForSearch") = 1 && EverythingQueryEdit.FocusCount >= 1,
    "14a 触发: 命令框隐藏 + 输入面显示/聚焦 + 透传放开",
    "实际 r=" r18 " Show=" EverythingQueryEdit.ShowCount " HideBox=" HostRecorder.Count("HideCommandBox")
    . " Unlock=" HostRecorder.Count("UnlockForSearch") " Focus=" EverythingQueryEdit.FocusCount)
EverythingQueryEdit.SimText := "临时"                  ; 模拟用户 IME 上屏
s18._SyncQuery()                                      ; 生产由 SetTimer 驱动
Verify(s18.query = "临时" && EverythingSearch.RunCount = 1,
    "14b 输入面文本轮询并入检索词并重查",
    "实际 query=" s18.query " Run=" EverythingSearch.RunCount)
r19 := s18.OnChar(StubInputHook(), "x", "probe")      ; 按键事件 (物理键实际进 Edit)
Verify(r19 = true && s18.query = "临时" && EverythingSearch.RunCount = 1,
    "14c OnChar 只消费不触碰检索词 (Edit 原生持有文本)",
    "实际 r=" r19 " query=" s18.query " Run=" EverythingSearch.RunCount)
s18.Close()
Verify(EverythingQueryEdit.HideCount = 1,
    "14d 会话 Close: 输入面 Hide 恰 1 次",
    "实际 Hide=" EverythingQueryEdit.HideCount)

; ============================================================
; 汇总
; ============================================================
total := N_PASS + N_FAIL
Emit("")
Emit("RESULT: " N_PASS "/" total (N_FAIL > 0 ? "  FAIL" : "  PASS"))
Try FileAppend("`nRESULT: " N_PASS "/" total (N_FAIL > 0 ? "  FAIL" : "  PASS") "`n", "*")

ExitApp(N_FAIL > 0 ? 1 : 0)
