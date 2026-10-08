#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, Off
; ============================================================
; fuzzy_strategy_test —— FuzzyStrategy 编辑距离容错匹配 + 候选提示 回归探针。
;
; 覆盖 (提案 docs/contracts-proposals/fuzzy-strategy.md §7):
;   1. EditDistance 单元 (0/1/2 边界、插入/删除/替换、空串);
;   2. Candidates 排序/截断/等值排除/长度带/分域隔离;
;   3. 精确命中不受影响: FuzzySuffixFire 精确后缀/全串命中照旧, 容错层零介入;
;   4. 容错命中: 唯一候选 -> Pending + Stop; 前缀守卫 -> 不执行不提示;
;   5. 无候选: 静默 + 提示收起; 多候选: 仅提示不执行;
;   6. 阈值边界: Threshold=0 退化为纯精确; 阈值变化改变候选集;
;   7. Resolve 未命中路径: 唯一 -> 执行+Tip / 多候选 -> 仅 Tip / 无候选 -> 静默;
;   8. provider 语义: 惰性注册、退格/会话边界收起提示、恒不消费按键。
;
; 运行: MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut tools/fuzzy_strategy_test.ahk
;   退出码 0 = 全绿, 1 = 有断言失败 (可直接挂进 make, 非零即中断)。
;
; 纪律 (与 tools/command_input_hooks_test.ahk 同款):
;   * 刻意**逐字引入真身** (CommandResolver / FuzzyStrategy / CommandDisplay /
;     CommandInputHooks / EventBus), 不另写桩 —— 桩只能验证桩自己;
;   * 提示载体注入 FakeTip (记录 Show/Hide): 探针零桌面影响 (不建真 GUI), 且提示行为可断言;
;   * Tip() 桩 (Utils.ahk 不引入): Resolve 未命中路径的提示行为可断言;
;   * 工作目录隔离到 %TEMP%: 被测 _log 用相对路径 logs\*.ahk 落盘, 不污染真实日志;
;   * #Warn All, Off: 契约 §3.6 阶段 4 坑② —— 独立探针载入引用生成端符号的库文件时,
;     AHK v2 默认 #Warn 会在加载期弹对话框阻塞进程, /ErrorStdOut 无法抑制。
; ============================================================

; ---- 工作目录隔离 ----
PROBE_DIR := A_Temp "\kf_fuzzy_probe"
g_dir_ok := true
try {
    SetWorkingDir(A_Temp)
    DirCreate(PROBE_DIR "\logs")
    SetWorkingDir(PROBE_DIR)
} catch {
    g_dir_ok := false
}

; ---- 依赖桩 ----
; CommandDisplay 收口回显消费的 Post* 系列 (真身在 bin/lib/core/AbbrInput.ahk, 本探针
; 不触发透传补投, 桩仅为签名完备); Tip 桩供 Resolve 未命中路径断言提示内容。
PostCharToCapsAbbr(ih?, char?) {
    Rec.Add("PostChar", [char])
}
PostBackspaceToCapsAbbr(ih, vk, sc) {
    Rec.Add("PostBackspace", [ih, vk, sc])
}
Tip(message, time := -1500) {
    Rec.Add("Tip", [message])
}
; 批 M/L 后，被 include 的 `CommandDisplay` / `CommandInputHooks` / `CommandResolver` /
; `FuzzyStrategy` / `EventBus` 的失败留痕统一委派引擎唯一入口 `EngineLogWarn`
; （真身定义在 `core/Functions.ahk`，本探针**不** include 它）⇒ 同 `Post*` / `Tip` 一样提供桩：
; 只记录、不落盘。签名与真身逐字一致: `EngineLogWarn(context, detail := "")`。
; 🔴 2026-10-08: 缺此桩时异常路径调用未定义函数 ⇒ AHK 错误对话框 ⇒ 探针**挂住**
;    （零输出 + 进程存活；批 M 引入、批 O 期间修复）。
EngineLogWarn(context, detail := "") {
    Rec.Add("EngineLogWarn", [context, detail])
}

#Include ..\bin\lib\core\IKeyEventBus.ahk
#Include ..\bin\lib\core\CommandDisplay.ahk
#Include ..\bin\lib\core\CommandInputHooks.ahk
#Include ..\bin\lib\core\InputTipWindow.ahk
#Include ..\bin\lib\core\EventBus.ahk
#Include ..\bin\lib\commands\CommandResolver.ahk
#Include ..\bin\lib\commands\FuzzyStrategy.ahk

; ---- 断言基建 ----

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
}

ArrEq(a, b) {
    if (!IsObject(a) || !IsObject(b))
        return false
    if (a.Length != b.Length)
        return false
    for i, v in a {
        if (IsObject(v)) {
            if (!ArrEq(v, b[i]))
                return false
        } else if (v != b[i]) {
            return false
        }
    }
    return true
}

; 数组转可读文本 (断言失败时的 detail)
Dump2(a) {
    if (!IsObject(a))
        return String(a)
    s := "(" a.Length ") "
    for i, v in a
        s .= (i > 1 ? "," : "") (IsObject(v) ? "<obj>" : String(v))
    return s
}

; ---- 记录器 (Tip / Post* 调用) ----

class Rec {
    static Calls := []

    static Reset() {
        this.Calls := []
    }

    static Add(name, args) {
        this.Calls.Push({ name: name, args: args })
    }

    static Count(name) {
        n := 0
        for c in this.Calls
            if (c.name = name)
                n += 1
        return n
    }

    static LastMsg(name) {
        n := this.Calls.Length
        Loop n {
            c := this.Calls[n - A_Index + 1]
            if (c.name = name)
                return c.args[1]
        }
        return ""
    }
}

; ---- 测试替身 ----

; 提示载体桩: 与 InputTipWindow 的 Show/Hide 签名一致, 只记录不建 GUI (零桌面影响)。
class FakeTip {
    static Shows := 0
    static Hides := 0
    static LastText := ""

    Show(text := "", addition := false) {
        FakeTip.Shows += 1
        if (text != "")
            FakeTip.LastText := text
    }

    Hide() {
        FakeTip.Hides += 1
    }

    static Reset() {
        FakeTip.Shows := 0
        FakeTip.Hides := 0
        FakeTip.LastText := ""
    }
}

; InputHook 桩: FuzzySuffixFire / OnInputChanged 只消费 .Input 与 .Stop()。
class FakeIH {
    Input := ""
    Stopped := false

    Stop() {
        this.Stopped := true
    }
}

; ---- 测试脚手架 ----

g_exec := []   ; 命令步骤执行记录 (元素 = "scope:cmd")

; 就地清空执行记录 —— 必须 Length := 0 而非重新赋值: RegCmd 的步骤闭包捕获的是
; 数组**对象**, 重新赋值会让断言读到新数组而闭包仍写旧数组 (假绿)。
ClearExec() {
    global g_exec
    g_exec.Length := 0
}

; 清空待收尾状态 —— 前一子测试的容错命中会留下 Pending, 其后的「不应命中」子测试
; 必须先清空, 否则断言读到残留值 (假红)。
ClearPending() {
    CommandInputHooks.PendingScope := ""
    CommandInputHooks.PendingAbbr := ""
}

ResetAll() {
    global g_exec
    CommandResolver.Table.Clear()
    CommandResolver.Strategy := FuzzyStrategy
    CommandInputHooks.PendingScope := ""
    CommandInputHooks.PendingAbbr := ""
    g_exec.Length := 0
    FakeTip.Reset()
    Rec.Reset()
    FuzzyStrategy.Enabled := true
    FuzzyStrategy.Threshold := 1
    FuzzyStrategy.MinInputLen := 2
    FuzzyStrategy.MaxCandidates := 5
    FuzzyStrategy.HintShowMs := 3600000   ; 自动隐藏定时器在探针内不触发 (计时断言不进探针)
    FuzzyStrategy.TipWindow := FakeTip()
    FuzzyStrategy._providerWired := false
    CommandInputHooks.Providers := []
}

; 注册一条单步命令, 步骤体把 "scope:cmd" 推进 g_exec (执行断言用)
RegCmd(scope, cmd) {
    global g_exec
    execlog := g_exec   ; 不叫 log: 避免遮蔽同名内置 (lint_ident WARN)
    CommandResolver.Register(scope, cmd, [CommandStep(() => execlog.Push(scope ":" cmd))])
}

; 事件收集 (abbr_submit)
g_events := []
OnAbbrEvent(ev) {
    global g_events
    g_events.Push(ev)
}

; 订阅一次 (EventBus.Subs 跨 ResetAll 存续; OnAbbrEvent 每次读全局 g_events, 不捕获旧数组)
EventBus.Subscribe("abbr_submit", OnAbbrEvent)

; ============================================================
; 分组 1: EditDistance 单元 (阈值边界的算术基础)
; ============================================================

Check(FuzzyStrategy.EditDistance("se", "se") = 0, "ED: 等值距离 0")
Check(FuzzyStrategy.EditDistance("sw", "se") = 1, "ED: 替换 = 1")
Check(FuzzyStrategy.EditDistance("s", "se") = 1, "ED: 插入 = 1 (输入短于命令)")
Check(FuzzyStrategy.EditDistance("sew", "se") = 1, "ED: 删除 = 1 (输入长于命令)")
Check(FuzzyStrategy.EditDistance("", "se") = 2, "ED: 空串边界 = 对方长度")
Check(FuzzyStrategy.EditDistance("fc", "") = 2, "ED: 空串边界对称")
Check(FuzzyStrategy.EditDistance("abc", "xyz") = 3, "ED: 全不同 = 3")
Check(FuzzyStrategy.EditDistance("settang", "settings") = 2, "ED: 双错距离 2 (阈值 1 必须排除)")
Check(FuzzyStrategy.EditDistance("SE", "se") = 0, "ED: 大小写不敏感 (= 语义, 与 MatchList 一致)")

; ============================================================
; 分组 2: Candidates (排序 / 等值排除 / 长度带 / 分域隔离 / 截断)
; ============================================================

ResetAll()
RegCmd("capslock", "se")
RegCmd("capslock", "sc")
RegCmd("capslock", "fc")
RegCmd("capslock", "settings")
RegCmd("semicolon", "cd")

Check(ArrEq(FuzzyStrategy.Candidates("capslock", "sw"), ["sc", "se"]), "Cand: 距离同为 1 按字典序", Dump2(FuzzyStrategy.Candidates("capslock", "sw")))
Check(ArrEq(FuzzyStrategy.Candidates("capslock", "fw"), ["fc"]), "Cand: 唯一候选")
Check(FuzzyStrategy.Candidates("capslock", "zzz").Length = 0, "Cand: 无候选返回空数组")
Check(ArrEq(FuzzyStrategy.Candidates("capslock", "se"), ["sc"]), "Cand: 与输入等值的命令被排除 (等值属精确层)", Dump2(FuzzyStrategy.Candidates("capslock", "se")))
Check(ArrEq(FuzzyStrategy.Candidates("capslock", "cx"), []), "Cand: 分域隔离 —— caps 查不到 semicolon 的 cd", Dump2(FuzzyStrategy.Candidates("capslock", "cx")))

; 长度带: settings(len 8) 与输入 sw(len 2) 长度差 6 > 1, 不进候选 (与全量 DP 结论一致)
Check(ArrEq(FuzzyStrategy.Candidates("capslock", "sw"), ["sc", "se"]), "Cand: 长度带预筛不误杀近距离候选")
Check(FuzzyStrategy.Candidates("capslock", "settinx").Length = 0, "Cand: 距离 2 被 Threshold=1 排除")

; 截断: 3 个距离 1 候选, MaxCandidates=2 保留 (距离, 字典序) 最前两名
RegCmd("capslock", "sa")
RegCmd("capslock", "sb")
FuzzyStrategy.MaxCandidates := 2
Check(ArrEq(FuzzyStrategy.Candidates("capslock", "sd"), ["sa", "sb"]), "Cand: 超出 MaxCandidates 截断且按序保留最近者", Dump2(FuzzyStrategy.Candidates("capslock", "sd")))
FuzzyStrategy.MaxCandidates := 5

; 子序列判定 (Resolve 未命中路径的候选来源之一)
Check(FuzzyStrategy.IsSubsequence("stg", "settings") = true, "Sub: stg ⊆ settings (原序)")
Check(FuzzyStrategy.IsSubsequence("sgt", "settings") = false, "Sub: 乱序不是子序列")
Check(FuzzyStrategy.IsSubsequence("", "settings") = false, "Sub: 空输入不产生候选")
Check(FuzzyStrategy.IsSubsequence("settingss", "settings") = false, "Sub: 长于目标必 false")

; ============================================================
; 分组 3: 精确命中不受影响 (FuzzySuffixFire 真身, 策略已挂接)
; ============================================================

ResetAll()
RegCmd("capslock", "fc")
RegCmd("capslock", "se")
RegCmd("capslock", "settings")

ih := FakeIH()
ih.Input := "dfc"
FuzzySuffixFire(ih, "c", "capslock")
Check(ih.Stopped, "精确后缀命中: 停钩 (既有机制不变)")
Check(CommandInputHooks.PendingScope = "capslock" && CommandInputHooks.PendingAbbr = "fc", "精确后缀命中: dfc->fc 记 Pending (最长后缀优先)")
Check(g_exec.Length = 0, "精确命中不直接执行 (待收尾层负责)")
Check(FakeTip.Shows = 0, "精确命中路径零候选提示")

ih := FakeIH()
ih.Input := "se"
FuzzySuffixFire(ih, "e", "capslock")
Check(CommandInputHooks.PendingAbbr = "se", "全串精确命中: 整串即命令 (后缀含全串)")

; 精确命中时此前展示的候选提示必须被收起 (OnExactHit)
FuzzyStrategy.ShowHints(["sc", "sb"])
hidesBefore := FakeTip.Hides
ih := FakeIH()
ih.Input := "fc"
FuzzySuffixFire(ih, "c", "capslock")
Check(CommandInputHooks.PendingAbbr = "fc" && FakeTip.Hides > hidesBefore, "精确命中: OnExactHit 收起候选提示")

; ============================================================
; 分组 4: 容错命中 (唯一候选) 与前缀守卫
; ============================================================

ResetAll()
RegCmd("capslock", "fc")
RegCmd("capslock", "se")
RegCmd("capslock", "sc")
RegCmd("capslock", "settings")

ih := FakeIH()
ih.Input := "fw"
FuzzySuffixFire(ih, "w", "capslock")
Check(ih.Stopped, "容错命中: fw 无精确命中 -> 策略层停钩")
Check(CommandInputHooks.PendingScope = "capslock" && CommandInputHooks.PendingAbbr = "fc", "容错命中: 唯一候选 fc 记 Pending (经 TakePending 延后执行, 事件带 fuzzy=true)")
Check(FakeTip.Shows = 0, "容错命中: 不提示 (直接执行)")
Check(FakeTip.Hides >= 1, "容错命中: 命中前收起旧提示")

; 事件通路自证: 模糊命中经 Resolve(scope, abbr, , true) 收尾时事件带 fuzzy=true
g_events := []
CommandResolver.Resolve("capslock", "fc", , true)
Check(g_events.Length = 1 && g_events[1]["matched"] = true && g_events[1]["fuzzy"] = true && g_events[1]["source"] = "caps", "事件: 模糊路径 abbr_submit {matched:true, fuzzy:true, source:caps}")

; 前缀守卫: 输入 set 仍是 settings 的前缀 -> 完全静默 (防 swap 在 sw 处被 se 抢执行)
ClearPending()
ih := FakeIH()
ih.Input := "set"
FuzzySuffixFire(ih, "t", "capslock")
Check(!ih.Stopped && CommandInputHooks.PendingAbbr = "", "前缀守卫: set (settings 前缀) 不执行")
Check(FakeTip.Shows = 0, "前缀守卫: 不提示 (不干扰正在输入精确命令的用户)")

; 继续输入到全串: 精确通道接管
ih := FakeIH()
ih.Input := "settings"
FuzzySuffixFire(ih, "s", "capslock")
Check(CommandInputHooks.PendingAbbr = "settings", "前缀守卫后全串命中: 精确层接管")

; MinInputLen: 单字符不做容错
ClearPending()
ih := FakeIH()
ih.Input := "f"
FuzzySuffixFire(ih, "f", "capslock")
Check(!ih.Stopped && CommandInputHooks.PendingAbbr = "", "MinInputLen: 单字符不触发容错")

; ============================================================
; 分组 5: 多候选 -> 仅提示不执行; 无候选 -> 静默
; ============================================================

ResetAll()
RegCmd("capslock", "se")
RegCmd("capslock", "sc")
RegCmd("capslock", "sr")

ih := FakeIH()
ih.Input := "sw"
FuzzySuffixFire(ih, "w", "capslock")
Check(!ih.Stopped && CommandInputHooks.PendingAbbr = "", "多候选: 不执行 (冻结契约: 多候选仅 Tip 列出)")
Check(FakeTip.Shows = 1, "多候选: 提示展示一次 (InputTipWindow 载体)", FakeTip.Shows)
Check(InStr(FakeTip.LastText, "sc") && InStr(FakeTip.LastText, "se") && InStr(FakeTip.LastText, "sr"), "多候选: 提示内容含全部候选 (多行)")
Check(CommandInputHooks.Providers.Length = 1, "provider 惰性注册 (首次提示时)")

; 输入继续增长 -> 候选消散 -> 提示收起
ih := FakeIH()
ih.Input := "swzq"
FuzzySuffixFire(ih, "q", "capslock")
Check(FakeTip.Hides >= 1, "无候选: 提示收起 (候选随输入消散)")

; Enabled=false: 容错与提示全旁路
ResetAll()
RegCmd("capslock", "fc")
FuzzyStrategy.Enabled := false
ih := FakeIH()
ih.Input := "fw"
FuzzySuffixFire(ih, "w", "capslock")
Check(!ih.Stopped && CommandInputHooks.PendingAbbr = "" && FakeTip.Shows = 0, "Enabled=false: 容错层全旁路 (回纯精确)")
FuzzyStrategy.Enabled := true

; ============================================================
; 分组 6: 阈值边界
; ============================================================

ResetAll()
RegCmd("capslock", "se")

FuzzyStrategy.Threshold := 0
ih := FakeIH()
ih.Input := "sw"
FuzzySuffixFire(ih, "w", "capslock")
Check(!ih.Stopped && CommandInputHooks.PendingAbbr = "" && FakeTip.Shows = 0, "阈值边界: Threshold=0 退化为纯精确 (距离 1 不算候选)")

FuzzyStrategy.Threshold := 1
ih := FakeIH()
ih.Input := "sw"
FuzzySuffixFire(ih, "w", "capslock")
Check(ih.Stopped && CommandInputHooks.PendingAbbr = "se", "阈值边界: Threshold=1 时同一输入命中唯一候选 se")

ResetAll()
RegCmd("capslock", "se")
ih := FakeIH()
ih.Input := "swa"
FuzzySuffixFire(ih, "a", "capslock")
Check(!ih.Stopped && CommandInputHooks.PendingAbbr = "", "阈值边界: 距离 2 (swa vs se) 在 Threshold=1 下不命中")

; 阈值语义 vs 长度带一致性: Candidates 与 EditDistance 结论一致
ResetAll()
RegCmd("capslock", "settings")
RegCmd("capslock", "se")
Check(ArrEq(FuzzyStrategy.Candidates("capslock", "settingt"), ["settings"]), "阈值边界: 距离 1 的近邻在长命令上同样命中 (长度带不误杀)", Dump2(FuzzyStrategy.Candidates("capslock", "settingt")))

; ============================================================
; 分组 7: Resolve 未命中路径 (冻结契约 §3.6: 子序列 ∪ 编辑距离)
; ============================================================

ResetAll()
RegCmd("capslock", "fc")
g_events := []

CommandResolver.Resolve("capslock", "fx")
Check(g_exec.Length = 1 && g_exec[1] = "capslock:fc", "Resolve 未命中: 唯一候选静默执行 (步骤真身)")
Check(Rec.Count("Tip") = 1 && InStr(Rec.LastMsg("Tip"), "fc"), "Resolve 未命中: Tip 提示实际命令")
Check(g_events.Length = 2 && g_events[1]["matched"] = false && g_events[2]["matched"] = true, "事件: 未命中先报 matched=false, 容错执行再报 matched=true")

ResetAll()
RegCmd("capslock", "sc")
RegCmd("capslock", "se")
Rec.Reset()
ClearExec()
CommandResolver.Resolve("capslock", "sx")
Check(g_exec.Length = 0, "Resolve 未命中: 多候选不执行")
Check(Rec.Count("Tip") = 1 && InStr(Rec.LastMsg("Tip"), "sc") && InStr(Rec.LastMsg("Tip"), "se"), "Resolve 未命中: 多候选仅 Tip 列出")

Rec.Reset()
CommandResolver.Resolve("capslock", "zzz")
Check(Rec.Count("Tip") = 0, "Resolve 未命中: 无候选静默无操作")

; 子序列候选 (契约: 未命中 -> 子序列匹配 -> 编辑距离)
ResetAll()
RegCmd("capslock", "settings")
ClearExec()
CommandResolver.Resolve("capslock", "stg")
Check(g_exec.Length = 1 && g_exec[1] = "capslock:settings", "Resolve 未命中: 子序列唯一候选执行 (stg->settings)")

; 分号域隔离: caps 输入不会执行 semicolon 候选
ResetAll()
RegCmd("semicolon", "cd")
ClearExec()
Rec.Reset()
CommandResolver.Resolve("capslock", "cx")
Check(g_exec.Length = 0 && Rec.Count("Tip") = 0, "Resolve 未命中: 分域隔离 (caps 无候选, semicolon 的 cd 不越域)")

; ============================================================
; 分组 8: provider 语义 (惰性注册 / 恒不消费 / 会话边界)
; ============================================================

ResetAll()
RegCmd("capslock", "sc")
RegCmd("capslock", "se")
Check(CommandInputHooks.Providers.Length = 0, "provider 未注册 (未显示过提示)")

FuzzyStrategy.ShowHints(["sc", "se"])
Check(CommandInputHooks.Providers.Length = 1 && CommandInputHooks.Providers[1] = FuzzyStrategy, "provider 惰性注册 (首次提示后) 且注册对象 = FuzzyStrategy 类 (全 static, §3.12 硬约束 0 同款)")
; 短路合并且先判 Length: Providers 为空时 Providers[1] 会抛 IndexError (未捕获 = 错误对话框阻塞探针)

ok := CommandInputHooks.DispatchKey(FakeIH(), 0x08, 0, "capslock")
Check(ok = false, "OnKey 恒不消费退格 (EchoChar/FuzzySuffixFire 派发链不变)")
Check(FakeTip.Hides >= 1, "退格收起提示 (候选已陈旧)")

ok := CommandInputHooks.DispatchChar(FakeIH(), "x", "capslock")
Check(ok = false, "OnChar 未实现 -> 派发视为未消费 (引擎继续原有处理)")

CommandInputHooks.BeginSession()
CommandInputHooks.EndSession()
Check(FakeTip.Hides >= 1, "会话边界收起提示 (BeginSession/EndSession)")

; ============================================================
; 收尾
; ============================================================

try SetWorkingDir(A_Temp)
try DirDelete(PROBE_DIR, true)

total := g_pass + g_fail
; 结论行刻意保持纯 ASCII (经 make 管道时 stdout 走控制台码页, 中文标签可能乱码)
Try FileAppend("`nRESULT: " g_pass "/" total (g_fail > 0 ? "  FAIL" : "  PASS") "`n", "*")

if (!g_dir_ok) {
    Try FileAppend("WARN: probe dir isolation failed`n", "*")
}

if (g_fail > 0)
    ExitApp(1)
ExitApp(0)
