#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, Off
; ============================================================
; max_results_probe —— 「英文检索不出列表 + 命令框有概率卡死」修复的回归探针。
;
; 根因 (2026-10-05 实测, 本机 Everything 1.5): 结果条数无上限时, 英文短词导出全部匹配
;   (查询 "ge" = 72,870 条 / 9.4MB; 中文如「是」仅 11 条, 故只英文暴露):
;     ① ParseExport 逐行 FileExist × 7 万+ ⇒ 引擎线程阻塞数秒 = 「卡死」;
;     ② 0x406 载荷 9-11MB > MAX_PAYLOAD_BYTES (4MiB, 两端同值) ⇒ BuildResultsPayload
;        构造失败被静默拒绝 ⇒ 「不出列表」。
;   修复 = EverythingSearch.MAX_RESULTS (1000) 作为条数策略, 经 Search(query, maxResults)
;   传入通道层, es.exe 以 `-n` 落实 (实测 -n 在**排序之后**截断, 不破坏 GUI 同序)。
;
; 被测对象 = 真源 ../src/Everything{Providers,Search,Settings}.ahk (直接 #Include, 不复制不改写)。
; 本探针是真集成: 用插件自带 bin/es.exe 查真实 Everything (只读; 结果导出到临时目录后
;   由通道层自删)。Everything 未运行时集成组跳过 (SKIP 不算红)。
;
; 断言:
;   1) 通道层机制: _MaxResultsArgs(0) = "" (不传 -n), _MaxResultsArgs(1000) = " -n 1000";
;   2) 基类契约: Search(query, maxResults := 0) 双参可调 (0 默认值向后兼容旧调用);
;      降级通道同签名 (传 maxResults 不炸);
;   3) 集成: Search("ge", 1000) → ok 且恰好 1000 条 (上限在通道出口生效);
;   4) 集成: Search("ge", 0) 的第 1 条 == Search("ge", 1000) 的第 1 条 (-n 排序后截断,
;      同序口径不破);
;   5) 策略常量: EverythingSearch.MAX_RESULTS == 1000;
;   6) 端到端: EverythingSearch.Run("ge") → ok 且 ≤ MAX_RESULTS 条 (生产调用链, 含 -n 传参);
;   7) 条目契约: 每项含 path/name/isFolder 三字段 (通道输出契约未被上限改动破坏)。
;
; 用法 (MSYS_NO_PATHCONV 必须有: Git Bash 会把 /ErrorStdOut 当路径改写):
;   MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut data/plugins/everything_search/tests/max_results_probe.ahk
; 退出码: 0 = 全绿 (SKIP 不计); 1 = 有断言失败。
; 输出:  控制台 + %TEMP%\kf_max_results_probe.txt (UTF-8, 权威记录)
; ============================================================

#Include ../src/EverythingProviders.ahk
#Include ../src/EverythingSettings.ahk
#Include ../src/EverythingSearch.ahk

global OUT_FILE := A_Temp "\kf_max_results_probe.txt"
try FileDelete(OUT_FILE)
SetWorkingDir(A_ScriptDir)

global N_PASS := 0
global N_FAIL := 0
global N_SKIP := 0

; 🔴 探针铁律: 运行时错误一律落文件, **绝不弹 AHK 错误对话框** (2026-10-05 教训: 探针
;   在 plain Object 上调 .Has 触发运行时错误 → v2 默认弹**模态**错误框 → 脚本停在弹窗
;   等点击, 外部视角即「探针卡死」; /ErrorStdOut 只覆盖加载期错误, 管不到运行时)。
;   OnError 返回 1 = 吞掉默认对话框; 失败计红并继续跑完剩余断言。
ProbeOnError(err, exitCode) {
    global N_FAIL
    N_FAIL += 1
    Emit("[FAIL] 运行时异常: " err.Message " @ " err.What " line " err.Line)
    return 1
}
OnError(ProbeOnError)

Emit(s) {
    global OUT_FILE
    try FileAppend(s "`n", OUT_FILE, "UTF-8")
    try FileAppend(s "`n", "*", "UTF-8")
}

Check(name, cond) {
    global N_PASS, N_FAIL
    if (cond) {
        N_PASS += 1
        Emit("[PASS] " name)
    } else {
        N_FAIL += 1
        Emit("[FAIL] " name)
    }
}

Skip(name, why) {
    global N_SKIP
    N_SKIP += 1
    Emit("[SKIP] " name " —— " why)
}

; 插件自带 es.exe (与 ResolveEs 的落点 3 同源; 探针直连, 不走探测缓存)
global ES_EXE := A_ScriptDir "\..\bin\es.exe"
; 🔴 ResolveEs 的落点 3 = A_ScriptDir 基点 (生产 = 引擎 bin/, 探针 = 插件 tests/):
; 端到端组 (6) 必须先把 SelfEsPath 对齐到探针环境, 否则解析落空走降级通道 —— 那是
; 探针基点差异, 不是产品缺陷 (与 open_guard_probe 的「stub 引擎全局」同款口径)。
EverythingSettings.SelfEsPath := ES_EXE

; ---- 1) 通道层机制 ----
p := EverythingEsProvider(ES_EXE)
Check("1a _MaxResultsArgs(0) = 空串 (0 = 不限制)", (p._MaxResultsArgs(0) = ""))
Check("1b _MaxResultsArgs(1000) = ' -n 1000'", (p._MaxResultsArgs(1000) = " -n 1000"))

; ---- 2) 接口契约 (签名可扩展, 旧调用不破) ----
base := EverythingProvider()
r := base.Search("x")
Check("2a 基类 Search 单参可调 (默认 maxResults=0)", IsObject(r) && (r.ok = false))
r2 := base.Search("x", 5)
Check("2b 基类 Search 双参可调", IsObject(r2) && (r2.ok = false))
g := EverythingGuiProvider("")
r3 := g.Search("x", 5)
Check("2c 降级通道同签名, 传 maxResults 不炸", IsObject(r3) && (r3.ok = false) && (r3.error = ES_ERR_NOT_FOUND))

; ---- 集成前置: es.exe 与 Everything 就绪性 ----
haveEs := FileExist(ES_EXE)
haveEverything := EverythingSearch.IsRunning()
if (!haveEs) {
    Skip("3-7 集成组", "插件 bin/es.exe 不存在")
} else if (!haveEverything) {
    Skip("3-7 集成组", "Everything 未运行 (探针不代拉, 保持只读)")
}

if (haveEs && haveEverything) {
    ; ---- 3) 上限在通道出口生效 ----
    res := p.Search("ge", EverythingSearch.MAX_RESULTS)
    Check("3a Search('ge', 1000) ok=true", (res.ok = true))
    Check("3b 条数恰好 1000 (got " res.items.Length ")", (res.items.Length = 1000))

    ; ---- 4) -n 排序后截断 (同序口径) ----
    full := p.Search("ge", 0)
    if (full.ok && full.items.Length > 0 && res.items.Length > 0) {
        Check("4a Search('ge', 0) 也 ok (不传 -n 路径可用)", (full.ok = true))
        Check("4b 首条一致 (-n 截断保序): " full.items[1].path,
            (full.items[1].path = res.items[1].path))
        Check("4c 全量条数 > 上限 (got " full.items.Length ", 前提成立才有截断意义)",
            (full.items.Length > EverythingSearch.MAX_RESULTS))
    } else {
        Check("4 集成对照可跑 (full.ok=" full.ok " full.n=" full.items.Length ")", false)
    }

    ; ---- 5) 策略常量 ----
    Check("5 EverythingSearch.MAX_RESULTS = 1000", (EverythingSearch.MAX_RESULTS = 1000))

    ; ---- 6) 端到端 (生产调用链: Run → EnsureRunning → Create → Search(query, MAX)) ----
    runRes := EverythingSearch.Run("ge")
    Check("6a EverythingSearch.Run('ge') ok=true", (runRes.ok = true))
    Check("6b Run 条数 ≤ MAX_RESULTS (got " runRes.items.Length ")",
        (runRes.items.Length <= EverythingSearch.MAX_RESULTS))

    ; ---- 7) 条目契约 ----
    ; 🔴 用 HasOwnProp 而非 Has: AHK v2 的 plain Object 没有 Has 方法 (那是 Map 的);
    ;    在 plain Object 上调 .Has = 运行时错误 (2026-10-05 探针卡死弹窗的根因)。
    okFields := true
    for i, it in runRes.items {
        if (!it.HasOwnProp("path") || !it.HasOwnProp("name") || !it.HasOwnProp("isFolder")) {
            okFields := false
            Emit("       第 " i " 条缺字段")
            break
        }
    }
    Check("7 全部条目含 path/name/isFolder", okFields)
}

Emit("------")
Emit("pass=" N_PASS " fail=" N_FAIL " skip=" N_SKIP)
if (N_FAIL > 0)
    ExitApp(1)
ExitApp(0)
