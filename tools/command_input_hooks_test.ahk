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
; 本探针只直接调 DispatchChar / DispatchKey, 故这三个桩**永不执行** —— 仅为满足加载期的
; 符号解析。被测的 _Call / DispatchChar / DispatchKey 仍是逐字引入的真身, 同源性未受影响。
PostCharToCaspAbbr(ih, char) {
}
FuzzySuffixFire(ih, char, scope) {
}
PostBackspaceToCaspAbbr(ih, vk, sc) {
}

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
