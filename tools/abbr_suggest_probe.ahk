#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, Off
/**
 * abbr_suggest_probe.ahk —— 候选建议层 (AbbrSuggest) 与列表端口 (ResultList) 的离线契约探针。
 *
 * 覆盖两件容易在两端漂移的事:
 *   ① 0x406 载荷字节: 与 command-input/src/results.rs 的 `encode_payload` **逐字节**一致;
 *   ② 候选排序: 前缀 > 包含, 同级按 (长度, 字典序), 截断到 MaxItems, 等值命令排除。
 *
 * 纯离线: 不建窗口、不注入按键、不碰命令框 (Hwnd 未用到)。可随时 `make check-abbr`。
 *
 * 已卸载的依赖说明: `CommandInputHooks.Register` 在 AbbrSuggest 底部执行, 故这里必须
 * 先 Include CommandInputHooks (它本身只需 Functions 的 EngineLogWarn 兜底日志)。
 */

; 无人值守诊断: 任何未捕获异常先落盘再退出 —— 否则 AHK 会弹**模态错误框**把探针挂住
; (在 CI / 后台跑时表现为"无输出且超时", 排查成本极高)。落 %TEMP%, 与 g_log 同目录
; (不写死盘符: 换机/换盘符后固定路径失效, 反而丢失唯一堆栈)。
OnError(ProbeOnError)
ProbeOnError(err, mode) {
  FileAppend("ERROR mode=" mode "`n" err.Message "`n" (err.HasProp("Stack") ? err.Stack : "") "`n"
      , A_Temp "\kf-abbr-probe-error.txt", "UTF-8")
  ExitApp 3
  return true
}
#Include ../bin/lib/core/translation.ahk
#Include ../bin/lib/core/Functions.ahk
#Include ../bin/lib/core/AbbrInput.ahk
#Include ../bin/lib/core/CommandInputHooks.ahk
#Include ../bin/lib/commands/CommandResolver.ahk
#Include ../bin/lib/core/ResultList.ahk
#Include ../bin/lib/core/AbbrHints.ahk
#Include ../bin/lib/commands/AbbrSuggest.ahk

global g_pass := 0
global g_fail := 0
; 输出落文件而不是 stdout: AHK 在 /ErrorStdOut 下写 `*` 的流向随宿主而变 (WorkBuddy 的
; 捕获层实测拿不到), 落盘再读是唯一稳的证据链 —— 与本仓库其它探针同一口径。
global g_log := A_Temp "\kf-abbr-suggest-probe.txt"
try FileDelete(g_log)

Log(line) {
  global g_log
  FileAppend(line "`n", g_log, "UTF-8")
}

Check(name, cond, extra := "") {
  global g_pass, g_fail
  if (cond) {
    g_pass += 1
  } else {
    g_fail += 1
    Log("FAIL: " name (extra != "" ? "  [" extra "]" : ""))
  }
}

HexOf(buf, size) {
  out := ""
  Loop size
    out .= Format("{:02X}", NumGet(buf, A_Index - 1, "UChar"))
  return out
}

; ============================================================
; ⓪ 挂接契约: 建议层经**插槽**接入, CommandResolver 不认识具体类
; ============================================================
Check("0a Suggest 插槽已挂接 AbbrSuggest", CommandResolver.Suggest = AbbrSuggest)
Check("0b Strategy 插槽未被本层抢占 (仍为空)", CommandResolver.Strategy = "")

; ============================================================
; ① 载荷字节 (金标来自 results.rs 的单测口径; 改动任一侧这里必红)
; ============================================================
items := [["cw", "关闭窗口"], ["ca", ""]]
pkg := ResultList.BuildPayload(items, 2, ResultList.STYLE_INLINE)
Check("1a 返回对象", IsObject(pkg))
Check("1b size = 48 (16 头 + 8+'cw' + 12 + 8+2+'ca')", pkg.size = 48, pkg.size)
Check(
  "1c 字节逐位一致",
  HexOf(pkg.buf, pkg.size)
    = "4B4652330100000002000000010000000200000063770C000000E585B3E997ADE7AA97E58FA302000000636100000000",
  HexOf(pkg.buf, pkg.size)
)
; 头字段独立断言 (上面的长串一改就全红, 这里定位到具体偏移)
Check("1d magic = 'KFR3'", NumGet(pkg.buf, 0, "UInt") = 0x3352464B)
Check("1e selected 0 基 (传 2 ⇒ 1)", NumGet(pkg.buf, 4, "Int") = 1)
Check("1f count = 2", NumGet(pkg.buf, 8, "UInt") = 2)
Check("1g style = 1 (inline)", NumGet(pkg.buf, 12, "UInt") = 1)

; selected = 0 ⇒ 无高亮哨兵 -1
pkg0 := ResultList.BuildPayload(items, 0, ResultList.STYLE_STACKED)
Check("1h selected 0 ⇒ -1", NumGet(pkg0.buf, 4, "Int") = -1)
Check("1i style 透传", NumGet(pkg0.buf, 12, "UInt") = 0)

; ============================================================
; ② 候选排序 (命令表用桩填充; 与真实注册表的遍历方式一致)
; ============================================================
CommandResolver.Table := Map(
  "capslock:cw", [],
  "capslock:ca", [],
  "capslock:cc", [],
  "capslock:cmd", [],
  "capslock:wcw", [],
  "semicolon:cw", []        ; 另一 scope, 必须被过滤掉
)
CommandResolver.Table["capslock:zz"] := []
AbbrHints.Inject(Map())   ; 备注走内存注入: 离线, 不依赖部署树里的侧车文件

m := AbbrSuggest.Match("capslock", "c")
Check("2a 'c' 命中 5 条 (cw/ca/cc/cmd/wcw)", m.Length = 5, m.Length)
Check("2b 前缀优先 + 短者在前: ca", m.Length >= 1 && m[1][1] = "ca", m.Length >= 1 ? m[1][1] : "-")
Check("2c 其次 cc", m.Length >= 2 && m[2][1] = "cc")
Check("2d 再次 cw", m.Length >= 3 && m[3][1] = "cw")
Check("2e 前缀里长的靠后: cmd", m.Length >= 4 && m[4][1] = "cmd")
Check("2f 包含匹配排最后: wcw", m.Length >= 5 && m[5][1] = "wcw")
Check("2g scope 隔离 (semicolon 不出现)", !InStr(StrJoinCmds(m), "semicolon"))

m2 := AbbrSuggest.Match("capslock", "w")
; "w" 是 wcw 的前缀、只是 cw 的包含 ⇒ 前缀优先, wcw 在前 (这正是排序口径的考点)
Check("2h 前缀优先于包含: wcw 在 cw 前", m2.Length = 2 && m2[1][1] = "wcw" && m2[2][1] = "cw", StrJoinCmds(m2))

m3 := AbbrSuggest.Match("capslock", "cw")
Check("2i 等值命令自己不入列 (但 wcw 仍因包含而入选)", m3.Length = 1 && m3[1][1] = "wcw", StrJoinCmds(m3))

Check("2j 空输入零候选", AbbrSuggest.Match("capslock", "").Length = 0)
Check("2k 无匹配零候选", AbbrSuggest.Match("capslock", "q").Length = 0)

; 截断
Loop 9
  CommandResolver.Table["capslock:x" A_Index] := []
m4 := AbbrSuggest.Match("capslock", "x")
Check("2l 截断到 MaxItems = 6", m4.Length = 6, m4.Length)

; 备注取用 (中/英双列, 按系统语言取)
AbbrHints.Inject(Map("capslock:cw", ["关闭窗口", "Close"]))
note := AbbrHints.Note("capslock", "cw")
if SysLangIsChinese()
  Check("2m 中文环境取中文列", note = "关闭窗口", note)
else
  Check("2m 英文环境取英文列", note = "Close", note)
Check("2n 无备注 ⇒ 空串", AbbrHints.Note("capslock", "ca") = "")

StrJoinCmds(list) {
  s := ""
  for it in list
    s .= it[1] " "
  return s
}

; ============================================================
; ③ 显示副本: 候选词大写, 备注原样 (执行仍用小写 Items)
; ============================================================
v := AbbrSuggest._ViewOf([["me", "打开 Microsoft Edge"], ["mm", ""]])
Check("3a 显示副本首列大写", v.Length = 2 && v[1][1] = "ME" && v[2][1] = "MM", v.Length >= 1 ? v[1][1] : "-")
Check("3b 备注原样保留", v[1][2] = "打开 Microsoft Edge")
Check("3c 空备注仍是空串", v[2][2] = "")

; ============================================================
; 汇总
; ============================================================
total := g_pass + g_fail
; 汇总也走日志文件 (stdout 在部分宿主下拿不到, 见文件头的落盘理由)
Log("RESULT: " g_pass "/" total " passed")
if (g_fail = 0)
  ExitApp 0
ExitApp 1
