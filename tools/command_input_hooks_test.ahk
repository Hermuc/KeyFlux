#Requires AutoHotkey v2.0
#SingleInstance Off
; ============================================================
; command_input_hooks_test —— CommandInputHooks provider 分发契约回归探针。
;
; 为什么需要它 (2026-09-19 实测缺陷):
;   _Call 曾写成 `fn := p.%name%` + `fn.Call(args*)`。AHK v2 里 `obj.Method` 取到的是
;   **未绑定 this** 的函数对象 (this 只是普通首参, 取值前无值) ⇒ 首个实参被顶成 this、
;   末位实参缺失 ⇒ 每次回调抛 `Missing a required parameter.`, 被 DispatchChar/DispatchKey
;   的 try/catch 吞掉并「视为未消费」。后果: provider 从未真正执行 —— 命令框按空格触发键
;   完全无反应, 且引擎日志只在被吞掉的 catch 里留下噪声, 症状与「插件没挂上」无法区分。
;   本探针即该缺陷的守门人: 在旧实现下必须红。
;
; 运行: bin/AutoHotkey64.exe /ErrorStdOut tools/command_input_hooks_test.ahk
;   退出码 0 = 全绿, 1 = 有断言失败 (可直接挂进 make, 非零即中断)。
;
; 刻意**逐字引入真身** (`#Include ..\bin\lib\core\CommandInputHooks.ahk`), 不另写桩实现 ——
; 否则探针只能验证自己的桩, 回归价值归零。
; ============================================================

; ---- 工作目录隔离 ----
; 被测实现的 _log 用**相对路径** `logs\command_input_hooks.log` 落盘。不隔离会把探针
; 故意制造的异常写进真实日志 (制造噪声 + 污染部署取证基线)。
PROBE_DIR := A_Temp "\kf_cih_probe"
g_dir_ok := true
try {
    SetWorkingDir(A_Temp)
    DirCreate(PROBE_DIR "\logs")
    SetWorkingDir(PROBE_DIR)
} catch {
    g_dir_ok := false
}

; ---- 依赖桩 ----
; 被测文件底部的两个入口函数 (CommandInputOnChar / CommandInputOnKeyDown) 会调用引擎的这三个
; 函数 (真身定义在 bin/lib/core/AbbrInput.ahk 与 bin/lib/commands/CommandResolver.ahk)。
; 第 11/12 组会经 CommandDisplay.EchoChar / CommandInputOnChar 触发到它们 —— 桩只做
; Rec 记录不跑真逻辑, 借此断言「抑制态不投递 / Fuzzy 旁路 / 历史形态照常」。
; 被测的 _Call / DispatchChar / DispatchKey / ShouldEcho 仍是逐字引入的真身, 同源性未受影响。
PostCharToCaspAbbr(ih, char) {
    Rec.Add("PostChar", [ih, char])
}
FuzzySuffixFire(ih, char, scope) {
    Rec.Add("FuzzySuffixFire", [ih, char, scope])
}
PostBackspaceToCaspAbbr(ih, vk, sc) {
    Rec.Add("PostBackspace", [ih, vk, sc])
}

; 被测文件的入口函数经 CommandDisplay 收口回显 (反八角 keycap 策略, 见 core/CommandDisplay.ahk),
; 故必须**逐字引入该真身**而非另写桩 —— 与引入 CommandInputHooks 同一纪律: 桩只能验证桩自己。
; CommandDisplay 消费的上面三个桩函数即为它调用的 Post* 系列。
#Include ..\bin\lib\core\CommandDisplay.ahk
#Include ..\bin\lib\core\CommandInputHooks.ahk

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

; 逐位置比较实参: 对象按 .tag 比 (避免依赖 AHK 的对象等于语义), 标量按值比。
ArgEq(got, want) {
    if (got.Length != want.Length)
        return false
    for i, w in want {
        g := got[i]
        if (IsObject(w)) {
            if (!IsObject(g) || g.tag != w.tag)
                return false
        } else if (g != w) {
            return false
        }
    }
    return true
}

Dump(arr) {
    s := "(" arr.Length ") "
    for i, v in arr
        s .= (i > 1 ? " | " : "") (IsObject(v) ? "<" v.tag ">" : String(v))
    return s
}

; ---- 记录器 ----

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

    static Last(name) {
        i := this.Calls.Length
        while (i >= 1) {
            if (this.Calls[i].name = name)
                return this.Calls[i]
            i -= 1
        }
        return 0
    }
}

; ---- 假 provider ----
; 方法名与签名必须与 CommandInputHooks 文档化的 provider 契约逐字一致 (见被测文件头部注释)。
; `this` 由引擎分发层负责传入 —— 这正是本探针要守住的东西。
class FakeProvider {
    __New(tag, kind := "ok", consume := true) {
        this.tag := tag
        this.kind := kind
        this.consume := consume
    }

    OnSessionBegin() {
        Rec.Add("OnSessionBegin", [this.tag])
    }

    OnSessionEnd() {
        Rec.Add("OnSessionEnd", [this.tag])
    }

    OnChar(ih, char, scope) {
        Rec.Add("OnChar", [this.tag, ih, char, scope])
        if (this.kind = "throw")
            throw Error("fake provider 故意抛错")
        return this.consume
    }

    OnKey(ih, vk, sc, scope) {
        Rec.Add("OnKey", [this.tag, ih, vk, sc, scope])
        if (this.kind = "throw")
            throw Error("fake provider 故意抛错")
        return this.consume
    }
}

; 只实现 OnSessionBegin 的最小 provider —— 用于验证「未实现的方法走 HasProp 守卫」。
class MinimalProvider {
    __New(tag) {
        this.tag := tag
    }

    OnSessionBegin() {
        Rec.Add("OnSessionBegin", [this.tag])
    }
}

; ============================================================
; 断言
; ============================================================

Try FileAppend("=== command_input_hooks_test ===`n", "*")
Check(g_dir_ok, "探针工作目录已隔离到 %TEMP% (logs 不污染真实运行时)")

ih := { tag: "ihStub" }

; --- 1) Register 幂等 ---
pA := FakeProvider("A")
Check(CommandInputHooks.Register(pA) = true, "Register 首次返回 true")
Check(CommandInputHooks.Register(pA) = false, "Register 二次返回 false (幂等)")

; --- 2) BeginSession -> 零参 OnSessionBegin (this 必须已绑定) ---
Rec.Reset()
CommandInputHooks.BeginSession()
n := Rec.Count("OnSessionBegin")
Check(n = 1, "BeginSession 触发零参 OnSessionBegin", "记录 " n " 次 (旧实现下为 0 —— this 未绑定) ")
last := Rec.Last("OnSessionBegin")
Check(last != 0 && last.args[1] = "A", "OnSessionBegin 方法体内 this 指向 provider 自身",
    last = 0 ? "未执行" : "this.tag=" String(last.args[1]))
Check(CommandInputHooks.SessionActive = true, "BeginSession 置 SessionActive=true")

; --- 3) DispatchChar: 位置精确 + 返回值透传 ---
Rec.Reset()
r := CommandInputHooks.DispatchChar(ih, " ", "capslock")
Check(r = true, "DispatchChar 透传 provider 的 true (= 已消费)")
Check(Rec.Count("OnChar") = 1, "DispatchChar 触达 OnChar", "记录 " Rec.Count("OnChar") " 次")
last := Rec.Last("OnChar")
Check(last != 0 && ArgEq(last.args, ["A", ih, " ", "capslock"]),
    "OnChar 收到精确 (ih, char, scope) 三个位置",
    last = 0 ? "未执行" : "实际 " Dump(last.args))

; --- 4) DispatchKey: 位置精确 (四个位置, 最易被 this 顶掉末位) ---
Rec.Reset()
r := CommandInputHooks.DispatchKey(ih, 0x26, 0, "capslock")
Check(r = true, "DispatchKey 透传 provider 的 true")
last := Rec.Last("OnKey")
Check(last != 0 && ArgEq(last.args, ["A", ih, 0x26, 0, "capslock"]),
    "OnKey 收到精确 (ih, vk, sc, scope) 四个位置",
    last = 0 ? "未执行" : "实际 " Dump(last.args))

; --- 5) 未实现的方法: HasProp 守卫, 不抛不消费 ---
CommandInputHooks.Unregister(pA)
pMin := MinimalProvider("M")
CommandInputHooks.Register(pMin)
r := 0
threw := false
try r := CommandInputHooks.DispatchChar(ih, "x", "capslock")
catch
    threw := true
Check(!threw, "provider 未实现 OnChar 时不抛异常")
Check(r = false, "provider 未实现 OnChar 时视为未消费 (返回 false)")

; --- 6) 异常隔离 + 后续 provider 仍被询问 ---
CommandInputHooks.Unregister(pMin)
pThrow := FakeProvider("T", "throw")
pAfter := FakeProvider("B", "ok", false)
CommandInputHooks.Register(pThrow)
CommandInputHooks.Register(pAfter)
Rec.Reset()
r := 0
threw := false
try r := CommandInputHooks.DispatchChar(ih, "y", "capslock")
catch
    threw := true
Check(!threw, "provider 抛异常被隔离, 不外泄到调用方")
Check(r = false, "抛异常的 provider 视为未消费 (返回 false)")
Check(Rec.Count("OnChar") = 2, "抛异常后仍继续询问后续 provider",
    "记录 " Rec.Count("OnChar") " 次 (期望 2: 抛错者 + 后续者)")

; --- 7) 短路: 前序消费后不再问后续 ---
CommandInputHooks.Unregister(pThrow)
CommandInputHooks.Unregister(pAfter)
CommandInputHooks.Register(pA)
pSkipped := FakeProvider("S")
CommandInputHooks.Register(pSkipped)
Rec.Reset()
r := CommandInputHooks.DispatchChar(ih, "z", "capslock")
Check(r = true, "短路: 首个消费即返回 true")
Check(Rec.Count("OnChar") = 1, "短路: 后续 provider 未被调用",
    "记录 " Rec.Count("OnChar") " 次 (期望 1)")

; --- 8) EndSession -> OnSessionEnd ---
Rec.Reset()
CommandInputHooks.EndSession()
Check(Rec.Count("OnSessionEnd") >= 1, "EndSession 触发 OnSessionEnd",
    "记录 " Rec.Count("OnSessionEnd") " 次")
Check(CommandInputHooks.SessionActive = false, "EndSession 置 SessionActive=false")

; --- 9) Unregister 生效 ---
; 注意: 此处仍有 pSkipped 在册 (consume=true), 故分发会被它消费并留下 1 条记录 ——
; 断言的是「不再触达已注销的 A」, 而不是「一条记录都没有」。
CommandInputHooks.Unregister(pA)
Rec.Reset()
r := CommandInputHooks.DispatchChar(ih, "q", "capslock")
last := Rec.Last("OnChar")
Check(Rec.Count("OnChar") = 1 && last != 0 && last.args[1] = "S",
    "Unregister 后分发不再触达该 provider (仅剩余 provider 被调用)",
    "记录 " Rec.Count("OnChar") " 次, tag=" (last = 0 ? "无" : String(last.args[1])))
Check(r = true, "剩余 provider (consume) 仍生效")

; --- 10) ActivateBackend 无后台窗口时不抛 ---
threw := false
try CommandInputHooks.ActivateBackend()
catch
    threw := true
Check(!threw, "ActivateBackend 在无可用后台窗口时不抛异常")

; ============================================================
; 11) CommandDisplay —— 回显收口 / 八角 keycap 抑制
;
; 背景: 命令框 exe 对 a-zA-Z0-9 会画八角 keycap, 且描边与字符同用一支画刷 (RTTI 证据),
; 无法只去框留字。故「移除八角框」的实现方式是**不下发这些字符** ——
; 本组断言守住的就是那条判据 (白名单边界必须与 exe 逐字一致)。
; ============================================================

Check(CommandDisplay.SuppressKeycap = false, "CommandDisplay 默认不抑制 (零行为变更)")

; 白名单边界: 恰好 a-zA-Z0-9, 一个不多一个不少
Check(CommandDisplay.IsKeycapChar("a") = true, "白名单: 'a' 命中")
Check(CommandDisplay.IsKeycapChar("Z") = true, "白名单: 'Z' 命中")
Check(CommandDisplay.IsKeycapChar("0") = true, "白名单: '0' 命中")
Check(CommandDisplay.IsKeycapChar("9") = true, "白名单: '9' 命中")
Check(CommandDisplay.IsKeycapChar("/") = false, "白名单外: '/' 不命中")
Check(CommandDisplay.IsKeycapChar(":") = false, "白名单外: ':' 不命中")
Check(CommandDisplay.IsKeycapChar("@") = false, "白名单外: '@' 不命中")
Check(CommandDisplay.IsKeycapChar(" ") = false, "白名单外: 空格不命中")
Check(CommandDisplay.IsKeycapChar("中") = false, "白名单外: 中文不命中 (exe 也不给中文画框)")
Check(CommandDisplay.IsKeycapChar("（") = false, "白名单外: 全角括号不命中")
Check(CommandDisplay.IsKeycapChar("") = false, "空串不命中 (边界不崩)")
Check(CommandDisplay.IsKeycapChar("ab") = false, "多字符串不命中 (只接受单字符)")

; 抑制关闭时: 所有字符照常下发 (历史行为)
Check(CommandDisplay.ShouldEcho("a") = true, "未抑制: 'a' 下发")
Check(CommandDisplay.ShouldEcho("中") = true, "未抑制: '中' 下发")
Check(CommandDisplay.ShouldEcho(" ") = true, "未抑制: 空格下发")

; 抑制开启时: 全部停投 (v4 §3.12: 透传已原生显示, 任何投递都是二次显示 —— 不再区分白名单)
CommandDisplay.SuppressKeycap := true
Check(CommandDisplay.ShouldEcho("a") = false, "已抑制: 'a' 不下发 (透传已原生显示)")
Check(CommandDisplay.ShouldEcho("9") = false, "已抑制: '9' 不下发")
Check(CommandDisplay.ShouldEcho("中") = false, "已抑制: '中' 不下发 (v4 全停, 防中文双显)")
Check(CommandDisplay.ShouldEcho(" ") = false, "已抑制: 空格不下发")
Check(CommandDisplay.ShouldEcho("（") = false, "已抑制: 全角符号不下发")

; EchoChar 返回值语义: true = 确实投递, false = 被抑制
Check(CommandDisplay.EchoChar(ih, "a") = false, "EchoChar 被抑制时返回 false")
Check(CommandDisplay.EchoChar(ih, "中") = false, "EchoChar 中文同样被抑制 (v4 全停)")

; Reset 复位
CommandDisplay.Reset()
Check(CommandDisplay.SuppressKeycap = false, "Reset 后抑制关闭 (会话间不泄漏状态)")
Check(CommandDisplay.EchoChar(ih, "中") = true, "未抑制: EchoChar 正常投递")
Check(Rec.Count("PostChar") = 1, "未抑制投递确实到达 PostChar 桩 (镜像显示通道)")

; --- 12) 入口函数 CommandInputOnChar 的透传守卫语义 (§3.12 v4.2) ---
; 抑制态: DispatchChar 照跑 (providers 派发保留), EchoChar 被兑停 (ShouldEcho 全停),
;         FuzzySuffixFire 恒跑 (v4.2 恢复: 缩写全英文字母, 中文意图仅在前置键之后,
;         那时字符已被插件消费, 到不了匹配层 —— v4 的旁路已撤, 与历史形态对齐)。
Rec.Reset()
CommandInputHooks.Unregister(pSkipped)
CommandDisplay.SuppressKeycap := true

threw := false
try
    CommandInputOnChar(ih, "a", "capslock")
catch
    threw := true
Check(!threw, "CommandInputOnChar 在透传模式下不抛异常")
Check(CommandDisplay.ShouldEcho("a") = false, "透传模式下 'a' 确实被拦下")
Check(Rec.Count("PostChar") = 0, "透传模式下字符未投递 (EchoChar no-op)")
Check(Rec.Count("FuzzySuffixFire") = 1, "透传模式下 FuzzySuffixFire 恒跑 (v4.2 恢复, 即输即执行)")

; 透传模式下 providers 派发保留, 且**消费型** provider 会短路: DispatchChar 提前
; return ⇒ EchoChar 与 FuzzySuffixFire 都不执行 (搜索期不误触发缩写的双保险 #2)
pWatch := FakeProvider("W", "ok", true)
CommandInputHooks.Register(pWatch)
Rec.Reset()
CommandInputOnChar(ih, "b", "capslock")
Check(Rec.Count("OnChar") = 1, "透传模式下 providers 派发保留 (v4 语义)")
Check(Rec.Count("PostChar") = 0, "透传模式下仍不投递")
Check(Rec.Count("FuzzySuffixFire") = 0, "插件消费字符后 FuzzySuffixFire 不跑 (搜索期不误触发, 双保险)")
CommandInputHooks.Unregister(pWatch)

; 历史形态 (未抑制): EchoChar 照常投递, FuzzySuffixFire 照常
CommandDisplay.Reset()
Rec.Reset()
CommandInputOnChar(ih, "a", "capslock")
Check(Rec.Count("PostChar") = 1, "历史形态下字符照常投递")
Check(Rec.Count("FuzzySuffixFire") = 1, "历史形态下缩写模糊匹配照常")
CommandDisplay.Reset()

; --- 13) 焦点激活 ActivateCommandWindow 的降级语义 (§3.12 v4.1, 2026-09-19) ---
; 测试环境无命令框窗口: WinWait 0.5s 超时 -> 必须返回 false (触发调用方降级),
; 且函数自身不得触碰 SuppressKeycap (降级决策归编排层, 单一职责)。
threw := false
skBefore := CommandDisplay.SuppressKeycap
retActivate := true
try
    retActivate := CommandDisplay.ActivateCommandWindow()
catch
    threw := true
Check(!threw, "ActivateCommandWindow 在无命令框窗口时不抛异常")
Check(retActivate = false, "ActivateCommandWindow 无窗口返回 false (调用方降级依据)")
Check(CommandDisplay.SuppressKeycap = skBefore, "ActivateCommandWindow 不触碰 SuppressKeycap (单一职责)")

; ============================================================
; 收尾
; ============================================================

; 先把工作目录移出待删目录 (Windows 下删除 CWD 会失败)
try SetWorkingDir(A_Temp)
try DirDelete(PROBE_DIR, true)

total := g_pass + g_fail
; 结论行刻意保持**纯 ASCII**: 经 make 管道时 AHK stdout 走控制台码页 (非 UTF-8),
; 中文标签可能乱码, 而这一行是人与 CI 唯一必须读准的.
Try FileAppend("`nRESULT: " g_pass "/" total (g_fail > 0 ? "  FAIL" : "  PASS") "`n", "*")

if (g_fail > 0)
    ExitApp(1)
ExitApp(0)
