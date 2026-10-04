#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, Off
; ============================================================
; open_guard_probe —— 「连点/按住 Enter 后成批弹出『文档 - 文件资源管理器』窗口」修复的回归探针。
;
; 被测对象 = 真源 ../src/EverythingSession.ahk + ../src/EverythingResults.ahk (直接 #Include,
;   不复制不改写): 探针只 stub **引擎全局**与 explorer 启动这一处副作用, 会话状态机、守卫链
;   与「结果 → 命令框」的端口调用链走的都是生产代码。
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
;       9c/9d   = 失败路径还会把结果列表和输入钩子一起收掉 (提示看不见 = 用户视角「回车没反应」)。
;   * 只把 Enter 分支退回「无条件 Close + ih.Stop」(其余保持新代码) => 恰好 9c/9d 两项变红
;     ⇒ 第 9 组精确锁住「失败可见」这一条, 不会靠别的断言兜住。
;   ⚠ 断言 1 在旧行为下同样是绿的: OnKey 入口**早已**有 `closed` 守卫, 重复通知根本到不了
;   Enter 分支 ⇒ 1 是「别把这道既有守卫改坏」的回归护栏; 真正判别「重复通知去抖」的是 5a。
;
; 断言 (覆盖修复要求 1-4 + 收尾两项, 另加 5 组补充):
;   1) 连走 3 次 Enter 分支 => Launch 只被调用 1 次 (关闭守卫生效), 会话关闭, 后续通知被拒;
;   2) path 为空 => 返回 false 且 Launch 调用数不增加;
;   3) path 指向不存在的路径 => 返回 false 且 Launch 调用数不增加 (并出一行提示);
;   4) 文件夹/文件两形态分别产生 `explorer.exe "<绝对路径>"` 与 `explorer.exe /select,"<绝对路径>"`;
;   5) 去抖窗口本身; 6) 全空白 path; 7) 会话关闭后无副作用; 8) 缝的默认实现仍走 Run (不过修);
;   9) 失败不静默: 路径失效时列表不收 / ih.Stop 不调 / 会话仍 active (9a-9e), 成功时各 1 次 (9f);
;  10) 新键 err_item_missing 中英双分支都有真文案 (无回落键名);
;  11) 命令框回推 (0x409 → OnBoxNotify) 与 Enter 走同一条守卫链 (2026-10-04 改写):
;      点选有效项成功才收列表 (11a); 失效路径不启动 explorer + 出提示 (11b) 且列表/会话保留 (11c);
;      越界行号无副作用 (11d); 悬停 (kind=2) 只同步索引, 不打开不收列表 (11e);
;  12) 引擎依赖全部经 EverythingHost 端口; 13) 空检索词不出列表;
;  14) 搜索模式 = 命令框本体输入 (0x404/读回/组合态);
;  15) 结果推送经 EverythingResults → 端口, 且**渲染责任零残留**: 显示文本 = 完整路径
;      (路径空回落 name), 单行提示 index=0, 选中移动走 0x407 (2026-10-04 新增)。
;
; 用法 (MSYS_NO_PATHCONV 必须有: Git Bash 会把 /ErrorStdOut 当路径改写):
;   MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut data/plugins/everything_search/tests/open_guard_probe.ahk
; 退出码: 0 = 全绿; 1 = 有断言失败。
; 输出:  控制台 + %TEMP%\kf_open_guard_probe.txt (UTF-8, 权威记录)
; 副作用: 无 —— 不真启动 explorer、不建真窗口、不发按键、不连真实命令框进程
;   (EverythingHost.Impl 已整缝替换, 端口调用只被记录)。
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
; 2026-10-04 起结果列表也走同一端口 (0x406/0x407/0x408): EverythingHost 新增的
;   ShowResults / SelectResult / ClearResults 在 HostRecorder 里记录参数 ⇒
;   「推了什么行、index 是多少」可逐项断言**而无需命令框进程**。
; 插件内部**非被测**真源仍用 stub (各自真源未 #Include):
;   EverythingSettings / EverythingSearch / EverythingProviders。
; 被测真源 = EverythingHost / EverythingMessages / EverythingResults / EverythingSession。
; ============================================================

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
 * 命令框文本模拟 (历史名 EverythingQueryEdit: 真源查询输入面已随 840ffe6 架构反转删除,
 * 本类剩余角色 = 「命令框缓冲里的文本」)。SimText 模拟用户在框内的输入 (含 IME 上屏),
 * 由 HostRecorder.BoxGetText 读回 / SetSimText 写入 —— 供轮询同步断言。
 * (旧版还有 Show/Hide/Focus 计数器与 SetText/GetText, 随输入面退役于 2026-10-04 移除。)
 */
class EverythingQueryEdit {
    static SimText := ""
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
#Include ../src/EverythingResults.ahk
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
    static BoxActivateForSearch() {
        HostRecorder.Calls.Push(["BoxActivateForSearch"])
    }
    static BoxForeground() {
        HostRecorder.Calls.Push(["BoxForeground"])
    }
    static BoxGetText() {
        HostRecorder.Calls.Push(["BoxGetText"])
        return EverythingQueryEdit.SimText
    }
    static BoxSendText(text) {
        HostRecorder.Calls.Push(["BoxSendText", text])
    }
    static SetSimText(t) {
        EverythingQueryEdit.SimText := t
    }
    static BoxQueryComposing() {
        HostRecorder.Calls.Push(["BoxQueryComposing"])
        return false
    }
    ; ---- 结果列表面板端口 (0x406/0x407/0x408) ----
    ; 只记录参数: 真实实现在 EverythingHost 里构造 COPYDATASTRUCT 并经
    ; SendMessageTimeoutW 发往命令框窗口 —— 探针不建窗口, 故在端口处截断,
    ; 这样「推了哪些行 / index 是多少 / 是整表还是单行提示」全部可断言。
    static ShowResults(lines, index) {
        HostRecorder.Calls.Push(["ShowResults", lines, index])
        return true
    }
    static SelectResult(index) {
        HostRecorder.Calls.Push(["SelectResult", index])
        return true
    }
    static ClearResults() {
        HostRecorder.Calls.Push(["ClearResults"])
        return true
    }
    ; ---- 搜索徽标端口 (0x40A/0x40B) ----
    ; 只记录调用: 真实实现发 SendMessageTimeoutW 到命令框窗口; 探针不建窗口。
    ; 记录用于「搜索模式开/关与徽标状态同步」断言 (组 16)。
    static ShowBadge() {
        HostRecorder.Calls.Push(["ShowBadge"])
        return true
    }
    static HideBadge() {
        HostRecorder.Calls.Push(["HideBadge"])
        return true
    }
    /** 最近一次指定端口调用的记录数组 (无 = 空数组; 元素 [0]=名字 [1..]=参数)。 */
    static LastArgs(name) {
        i := HostRecorder.Calls.Length
        while (i >= 1) {
            if (HostRecorder.Calls[i][1] = name)
                return HostRecorder.Calls[i]
            i -= 1
        }
        return []
    }
}

EverythingHost.Impl := HostRecorder

/**
 * 0x409 回推的接收器记录实现 —— EverythingResults.InstallNotify 的目标。
 * 用**实例** (与生产一致: InstallNotify 收到的是 EverythingController 实例;
 * 且 OnBoxNotify 是实例方法, 传类对象会改变 `this` 语义)。
 */
class NotifyRecorder {
    static Rows := []
    static Kinds := []
    static Reset() {
        NotifyRecorder.Rows := []
        NotifyRecorder.Kinds := []
    }
    OnBoxNotify(row, kind) {
        NotifyRecorder.Rows.Push(row)
        NotifyRecorder.Kinds.Push(kind)
    }
}

; ============================================================
; 结果推送给命令框的观测助手 —— 全部经 EverythingResults → EverythingHost 端口,
; 断言只读 HostRecorder 的记录 (探针不建命令框窗口)。布局:
;   ["ShowResults", entries, index] / ["SelectResult", index] / ["ClearResults"]
;   (2026-10-04 二版: entries = [{t: 标题/文件名, s: 副标题/路径}], 载荷 'KFR2')
; ============================================================

/** 列表被要求收起的次数 (EverythingResults.Hide → ClearResults)。 */
ResultsHideCount() {
    return HostRecorder.Count("ClearResults")
}

/** 完整数据推送 (Show / ShowHint 都算) 的次数。 */
ResultsPushCount() {
    return HostRecorder.Count("ShowResults")
}

/** 单行提示 (index = 0 且恰 1 行) 的次数。 */
HintCount() {
    n := 0
    for c in HostRecorder.Calls
        if (c[1] = "ShowResults" && c.Length >= 3 && c[3] = 0 && c[2].Length = 1)
            n += 1
    return n
}

/** 最近一次推给命令框的行文本数组 (无 = [])。 */
LastLines() {
    c := HostRecorder.LastArgs("ShowResults")
    return (c.Length = 0) ? [] : c[2]
}

/** 最近一次推送的高亮行 (1 基; 0 = 无高亮; -999 = 从未推送)。 */
LastIndex() {
    c := HostRecorder.LastArgs("ShowResults")
    return (c.Length = 0) ? -999 : c[3]
}

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
    StubInputHook.StopCount := 0
    EverythingSearch.RunCount := 0
    EverythingQueryEdit.SimText := ""
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
hint2 := LastLines()
Verify(HintCount() = 1 && hint2.Length = 1 && hint2[1].t = EverythingMessages.T("err_item_missing"),
    "2c path 为空: 出一行提示文案 (err_item_missing)",
    "实际 " ((HintCount() = 1) ? "'" hint2[1].t "'" : "提示数 " HintCount()))
Verify(LastIndex() = 0,
    "2d 提示行: 推送的高亮行 = 0 (无高亮 —— 提示不可被回车打开)",
    "实际 index=" LastIndex())

; --- 断言 3: path 不存在 => 不启动 explorer, 返回 false ---
ResetObservers()
s3 := NewSession([ItemFile(PATH_MISSING)], 1)
ok3 := s3.OpenSelected()
Verify(ok3 = false, "3a 路径不存在: OpenSelected 返回 false", "实际 " ok3)
Verify(ExplorerRecorder.Calls.Length = 0,
    "3b 路径不存在: explorer 未被启动",
    "实际 " ExplorerRecorder.Calls.Length " 次")
hint3 := LastLines()
Verify(HintCount() = 1 && hint3.Length = 1 && hint3[1].t = EverythingMessages.T("err_item_missing"),
    "3c 路径不存在: 出一行提示文案 (err_item_missing)",
    "实际 " ((HintCount() = 1) ? "'" hint3[1].t "'" : "提示数 " HintCount()))

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
;     9a-9e: 路径失效 -> explorer 不启动、列表不收、输入钩子不 Stop (= 提示留在屏上)、
;            会话仍 active (可继续改检索词); 9f: 成功 -> 两者各恰好 1 次。
ResetObservers()
s11 := NewSession([ItemFile(PATH_MISSING)], 1)
r11 := PressEnter(s11)
Verify(r11 = true, "9a 路径失效的 Enter 仍被消费 (OnKey 返回 true)", "实际 " r11)
Verify(ExplorerRecorder.Calls.Length = 0,
    "9b 路径失效: explorer 未被启动", "实际 " ExplorerRecorder.Calls.Length " 次")
Verify(ResultsHideCount() = 0 && StubInputHook.StopCount = 0,
    "9c 路径失效: 列表未收起、输入钩子未 Stop (= 提示留在屏上, 用户看得见)",
    "实际 Hide=" ResultsHideCount() " Stop=" StubInputHook.StopCount)
Verify(s11.closed = false && s11.active = true,
    "9d 路径失效: 会话保持打开, 可继续改检索词或按 Esc 退出",
    "实际 closed=" s11.closed " active=" s11.active)
hint9 := LastLines()
Verify(HintCount() = 1 && hint9.Length = 1 && hint9[1].t = EverythingMessages.T("err_item_missing"),
    "9e 路径失效: 提示文案 = err_item_missing",
    "实际提示数 " HintCount())
ResetObservers()
s12 := NewSession([ItemFolder(DIR_EXISTS)], 1)
r12 := PressEnter(s12)
Verify(r12 = true && s12.closed = true && ResultsHideCount() = 1 && StubInputHook.StopCount = 1,
    "9f 成功: 会话关闭, 收起列表与 ih.Stop 各调用 1 次",
    "实际 Hide=" ResultsHideCount() " Stop=" StubInputHook.StopCount " closed=" s12.closed)

; --- 断言 10: 新键 err_item_missing 在中英两条分支都是真文案 (不是回落成键名) ---
keyZh := EverythingMessages.T("err_item_missing")
EverythingMessages.En := true     ; 强行切英文分支 (静态字段可写; _ready 已 true, 不会再探测系统语言)
keyEn := EverythingMessages.T("err_item_missing")
EverythingMessages.En := false    ; 复原中文分支
Verify(keyZh != "err_item_missing" && keyEn != "err_item_missing" && keyZh != keyEn,
    "10 err_item_missing 中英双分支都有真文案 (无回落键名)",
    "zh='" keyZh "' en='" keyEn "'")

; --- 断言 11: 命令框回推 (0x409) 的行交互与 Enter 走同一守卫链 (2026-10-04 改写) ---
;     旧内部回调 OnPick(path) 靠 path 反查行号; 新协议直接携带行号 (命令框才是知道行几何
;     的一方)。语义不变: 点选 (kind=1) 仍「仅成功才收列表」, 失败保留列表让提示可见。
ResetObservers()
s13 := NewSession([ItemFolder(DIR_EXISTS)], 1)
s13.OnBoxNotify(1, 1)                                ; 点选第 1 行 (有效文件夹)
Verify(ExplorerRecorder.Calls.Length = 1 && s13.closed = true && ResultsHideCount() = 1,
    "11a 点选有效项: Launch 1 次, 会话关闭, 列表收起 1 次",
    "实际 Launch=" ExplorerRecorder.Calls.Length " closed=" s13.closed " Hide=" ResultsHideCount())

ResetObservers()
s14 := NewSession([ItemFile(PATH_MISSING)], 1)
s14.OnBoxNotify(1, 1)                                ; 点选一个已失效的结果
hint14 := LastLines()
Verify(ExplorerRecorder.Calls.Length = 0 && HintCount() = 1 && hint14.Length = 1
    && hint14[1].t = EverythingMessages.T("err_item_missing"),
    "11b 点选失效项: explorer 未启动, 出 err_item_missing 提示",
    "实际 Launch=" ExplorerRecorder.Calls.Length " 提示数 " HintCount())
Verify(ResultsHideCount() = 0 && StubInputHook.StopCount = 0 && s14.closed = false && s14.active = true,
    "11c 点选失效项: 列表未收、会话保留 (提示留在屏上, 可继续检索)",
    "实际 Hide=" ResultsHideCount() " closed=" s14.closed " active=" s14.active)

ResetObservers()
s15 := NewSession([ItemFolder(DIR_EXISTS)], 1)
s15.OnBoxNotify(9, 1)                                ; 越界行号 (列表只有 1 行)
Verify(ExplorerRecorder.Calls.Length = 0 && ResultsHideCount() = 0 && s15.closed = false && s15.index = 1,
    "11d 越界行号: 直接忽略, 无任何副作用 (索引不被改写)",
    "实际 Launch=" ExplorerRecorder.Calls.Length " Hide=" ResultsHideCount() " closed=" s15.closed " index=" s15.index)

ResetObservers()
s19 := NewSession([ItemFolder(DIR_EXISTS), ItemFile(FILE_EXISTS)], 1)
s19.OnBoxNotify(2, 2)                                ; 悬停到第 2 行 (高亮变化)
Verify(s19.index = 2 && ExplorerRecorder.Calls.Length = 0 && ResultsHideCount() = 0 && s19.closed = false
    && HostRecorder.Count("SelectResult") = 0,
    "11e 悬停 (kind=2): 只同步索引 (不回推 0x407 = 无回声环), 不打开不收列表",
    "实际 index=" s19.index " Launch=" ExplorerRecorder.Calls.Length " Hide=" ResultsHideCount() " Select=" HostRecorder.Count("SelectResult"))

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

; --- 断言 13: 空检索词 = 初始态, 不出任何列表 (2026-10-03 需求) ---
;     触发后取不到选中文字 => 检索词空 => Refresh 应只收列表 (不出提示行)。
;     此前会弹「没有选中文字 — 继续输入检索词」提示框 —— 「框下另挂独立框」观感来源之一。
ResetObservers()
s17 := EverythingSession(0)
r16 := s17.OnChar(StubInputHook(), " ", "probe")     ; 触发键; GetSelection(记录器) 返回空
Verify(r16 = true && ResultsHideCount() >= 1 && ResultsPushCount() = 0,
    "13a 空检索词: 不出列表也不出提示, 列表收起 ≥1 次 (命令框保持初始态)",
    "实际 r=" r16 " Hide=" ResultsHideCount() " Push=" ResultsPushCount())
ResetObservers()
r17 := s17.OnKey(StubInputHook(), EverythingSession.VK_BACK, 0, "probe")
Verify(r17 = true && s17.query = "" && HostRecorder.Count("EchoBackspace") = 0,
    "13b 空检索词下退格: 只消费 (Edit 原生处理), 检索词保持空",
    "实际 r=" r17 " query=" s17.query " EchoBackspace=" HostRecorder.Count("EchoBackspace"))

; --- 断言 14: 搜索模式 = 命令框本体输入 (2026-10-04 定版) ---
;     14a 触发 => UnlockForSearch + BoxActivateForSearch + BoxSendText(种子) 各 1 次;
;         HideCommandBox 零调用 (命令框保持可见 —— 「不换框」核心契约);
;     14b 轮询读回: BoxGetText 模拟 IME 上屏 ("临时") => query 更新 + 重查 1 次;
;     14c OnChar 只消费 (字母不重复处理, 框内原生持有);
;     14d Close => BoxQueryComposing 不再触发 (定时器停), Dropdown Hide 1 次。
ResetObservers()
s18 := EverythingSession(0)
r18 := s18.OnChar(StubInputHook(), " ", "probe")     ; 触发 (种子来自选择, 记录器返回空)
Verify(r18 = true && HostRecorder.Count("UnlockForSearch") = 1
    && HostRecorder.Count("BoxActivateForSearch") >= 1 && HostRecorder.Count("HideCommandBox") = 0,
    "14a 触发: 透传放开 + 0x404 激活(≥1) + 命令框保持可见 (HideCommandBox 零调用)",
    "实际 r=" r18 " Unlock=" HostRecorder.Count("UnlockForSearch") " Act=" HostRecorder.Count("BoxActivateForSearch") " HideBox=" HostRecorder.Count("HideCommandBox"))
r19 := s18.OnChar(StubInputHook(), "x", "probe")     ; 激活后的普通字符
Verify(r19 = true && s18.query = "" && HostRecorder.Count("EchoChar") = 0,
    "14b 激活后字符: 只消费 (检索词以命令框 WM_GETTEXT 读回为准)",
    "实际 r=" r19 " query=" s18.query " EchoChar=" HostRecorder.Count("EchoChar"))
HostRecorder.SetSimText("临时")                       ; 模拟 IME 上屏进入框内缓冲
s18._SyncQuery()                                      ; 生产由 SetTimer 驱动
Verify(s18.query = "临时" && EverythingSearch.RunCount = 1,
    "14c WM_GETTEXT 读回: 上屏文本并入检索词并重查",
    "实际 query=" s18.query " Run=" EverythingSearch.RunCount)
r20 := s18.OnKey(StubInputHook(), EverythingSession.VK_RETURN, 0, "probe")
Verify(r20 = true && HostRecorder.Count("BoxQueryComposing") >= 1,
    "14d 回车: 组合态查询经端口 (非组合 → 打开路径)",
    "实际 r=" r20 " Composing=" HostRecorder.Count("BoxQueryComposing"))
s18.Close()
Verify(ResultsHideCount() >= 1,
    "14e 会话 Close: 列表收起 (命令框由引擎隐藏路径接管)",
    "实际 Hide=" ResultsHideCount())

; --- 断言 15: 结果 → 命令框的推送契约 (2026-10-04 新增: 渲染已移交命令框, 本层只剩数据) ---
;     15a 显示文本口径; 15b 整表推送; 15c 单行提示 index=0; 15d 高亮移动走 0x407;
;     15e 收起走 0x408; 15f 0x409 回推的转发链; 15g/h 0x406 载荷逐字节布局
;     (与 command-input/src/results.rs::encode_payload 必须一致 —— 跨语言契约, 这里是
;      唯一的字节级锁; 生产路径 (Impl 被替换时) 会跳过 1基→0基 换算, 故换算与编码
;      直接用 EverythingHost.BuildResultsPayload 断言)。
ResetObservers()
txt := EverythingResults.Entries([{path: "C:\kf\a.txt", name: "a.txt", isFolder: false}
    , {path: "", name: "仅名字", isFolder: true}])
Verify(txt.Length = 2 && txt[1].t = "a.txt" && txt[1].s = "C:\kf\a.txt"
    && txt[2].t = "仅名字" && txt[2].s = "",
    "15a 展示条目: 标题 = 文件名含后缀, 副标题 = 完整路径 (Flow 双行); 名字空回落路径",
    "实际 " txt.Length " 项: '" txt[1].t "|" txt[1].s "' / '" txt[2].t "|" txt[2].s "'")

ResetObservers()
ok15 := EverythingResults.Show([{path: DIR_EXISTS, name: "x", isFolder: true}], 1)
ll15 := LastLines()
Verify(ok15 = true && ResultsPushCount() = 1 && ll15.Length = 1
    && ll15[1].t = "x" && ll15[1].s = DIR_EXISTS && LastIndex() = 1,
    "15b Show: 整表经端口推送 (标题 = name, 副标题 = 路径), 高亮 index 1 基直传",
    "实际 ok=" ok15 " Push=" ResultsPushCount() " index=" LastIndex())

ResetObservers()
EverythingResults.ShowHint("提示语")
ll16 := LastLines()
Verify(HintCount() = 1 && ll16.Length = 1 && ll16[1].t = "提示语" && ll16[1].s = "" && LastIndex() = 0,
    "15c ShowHint: 推单条 (副标题空 = 命令框无图标整行居中) + index 0",
    "实际 提示数=" HintCount() " index=" LastIndex())

ResetObservers()
EverythingResults.Select(3)
Verify(HostRecorder.Count("SelectResult") = 1 && HostRecorder.LastArgs("SelectResult")[2] = 3,
    "15d Select: 高亮移动经 0x407 端口 (只带下标, 不重推整表)",
    "实际 Select 次数=" HostRecorder.Count("SelectResult"))

ResetObservers()
EverythingResults.Hide()
Verify(ResultsHideCount() = 1 && ResultsPushCount() = 0,
    "15e Hide: 收起经 0x408 端口 (窗口回落基准高; 不重推空表)",
    "实际 Clear=" ResultsHideCount() " Push=" ResultsPushCount())

ResetObservers()
NotifyRecorder.Reset()
recv := NotifyRecorder()                       ; 实例 (与生产的 EverythingController 实例同形)
EverythingResults.InstallNotify(recv)
r21 := EverythingResultsNotifyForward(3, 1, EverythingResults.NOTIFY_MSG, 0)
Verify(r21 = 0 && NotifyRecorder.Rows.Length = 1 && NotifyRecorder.Rows[1] = 3 && NotifyRecorder.Kinds[1] = 1,
    "15f 0x409 回推: 文件级转发把 (行号, 类型) 交给控制器 OnBoxNotify 且返回 0 (消费)",
    "实际 r=" r21 " rows=" NotifyRecorder.Rows.Length)
; 注: InstallNotify 只安装一次 (幂等), 重载插件仅换目标 —— 故本组不需要 "重复安装" 断言。

; 15g/h: 0x406 载荷 (二版 'KFR2') = 12B 头 (魔数 u32 / selected i32 / count u32)
;   + 逐项 [t_len u32][t UTF-8][s_len u32][s UTF-8] —— 全小端; selected 在此处完成
;   1 基 → 0 基 换算 (0 = 无高亮 → -1); 与 Rust 侧 decode_payload 字段序/宽度逐项相同。
bufG := EverythingHost.BuildResultsPayload([{t: "ab", s: "c"}], 2)
okG := IsObject(bufG) && bufG.Size = 23
    && NumGet(bufG, 0, "UInt") = 0x3252464B && NumGet(bufG, 4, "Int") = 1 && NumGet(bufG, 8, "UInt") = 1
    && NumGet(bufG, 12, "UInt") = 2 && NumGet(bufG, 16, "UChar") = 0x61 && NumGet(bufG, 17, "UChar") = 0x62
    && NumGet(bufG, 18, "UInt") = 1 && NumGet(bufG, 22, "UChar") = 0x63
Verify(okG,
    "15g 0x406 载荷 KFR2: 魔数 / selected(1基2→0基1) / count / [t_len][t][s_len][s] 小端布局正确",
    "实际 size=" (IsObject(bufG) ? bufG.Size : "非 Buffer"))

bufH := EverythingHost.BuildResultsPayload([{t: "中", s: ""}], 0)
okH := IsObject(bufH) && bufH.Size = 23 && NumGet(bufH, 4, "Int") = -1 && NumGet(bufH, 12, "UInt") = 3
    && NumGet(bufH, 16, "UChar") = 0xE4 && NumGet(bufH, 17, "UChar") = 0xB8 && NumGet(bufH, 18, "UChar") = 0xAD
    && NumGet(bufH, 20, "UChar") = 0 && NumGet(bufH, 21, "UChar") = 0
Verify(okH,
    "15h 0x406 载荷: 中文 UTF-8 (3 字节) + 空 s (len 0); index 0 → selected -1 (无高亮)",
    "实际 size=" (IsObject(bufH) ? bufH.Size : "非 Buffer") " selected=" (IsObject(bufH) ? NumGet(bufH, 4, "Int") : "n/a"))

; --- 断言 16: 搜索徽标与搜索模式同生命周期 (2026-10-04 新增: 0x40A/0x40B) ---
;     16a 触发 (进入搜索模式) => ShowBadge 恰 1 次 (查询区右侧放大镜 = 搜索模式可视标识);
;     16b 会话 Close => HideBadge 恰 1 次;
;     16c Close 幂等: 重复收尾不重复发 0x40B (命令框侧对 0x401/0x402/0x403 另有
;         「徽标活不过一次会话」兜底, 由 command-input/src/protocol.rs 单测锁定)。
ResetObservers()
s19 := EverythingSession(0)
r22 := s19.OnChar(StubInputHook(), " ", "probe")     ; 触发 = 进入搜索模式
Verify(r22 = true && HostRecorder.Count("ShowBadge") = 1,
    "16a 进入搜索模式: 徽标显示经端口 (0x40A) 恰 1 次",
    "实际 r=" r22 " ShowBadge=" HostRecorder.Count("ShowBadge"))
ResetObservers()
s19.Close()
Verify(HostRecorder.Count("HideBadge") = 1,
    "16b 会话收尾: 徽标隐藏经端口 (0x40B) 恰 1 次",
    "实际 HideBadge=" HostRecorder.Count("HideBadge"))
s19.Close()
Verify(HostRecorder.Count("HideBadge") = 1,
    "16c Close 幂等: 重复收尾不重复发 0x40B (closed 短路)",
    "实际 HideBadge=" HostRecorder.Count("HideBadge"))

; ============================================================
; 汇总
; ============================================================
total := N_PASS + N_FAIL
Emit("")
Emit("RESULT: " N_PASS "/" total (N_FAIL > 0 ? "  FAIL" : "  PASS"))
Try FileAppend("`nRESULT: " N_PASS "/" total (N_FAIL > 0 ? "  FAIL" : "  PASS") "`n", "*")

ExitApp(N_FAIL > 0 ? 1 : 0)
