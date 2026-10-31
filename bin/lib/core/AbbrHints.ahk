#Warn All, Off
; #Warn All, Off: 本文件会被独立 /Validate、也可能被未含依赖的脚本 Include —— 那时类名
; 引用是「只读未赋值」变量, AHK v2 默认 #Warn 会弹**加载期对话框**阻塞进程
; (/ErrorStdOut 无法抑制; FuzzyStrategy.ahk 同款先例)。
/**
 * AbbrHints.ahk —— 缩写**备注数据**的唯一读取口 (生成侧车 → 内存表)。
 *
 * 分工: 本文件只回答"某条命令的备注是什么", 不匹配、不渲染、不知道命令框存在。
 * 上层 `AbbrSuggest` 负责匹配与展示。这样换数据源 (侧车 → 将来某天读配置服务)
 * 只改这一个文件。
 *
 * 文件格式 (由 `config-ui-reactor/src/generator/hints.rs` 生成):
 *   每行 `scope⇥abbr⇥中文⇥English`, UTF-8 无 BOM, 路径 = `A_ScriptDir\abbr-hints.gen.txt`。
 *
 * 为什么两侧语言都落盘、运行时才挑: 界面语言可以随时切换, 但切语言**不会**触发
 * 重新生成 `KeyFlux.ahk`。双列 + 运行时按系统语言取, 切语言就自动跟着变。
 *
 * 缺失/损坏 ⇒ 空表。功能退化为"候选只有命令名", 不抛、不阻断输入 (这是刻意的:
 * 备注是锦上添花, 不该成为命令框能不能用的前置条件)。
 */
class AbbrHints {
  static Path := ""          ; "" = A_ScriptDir\abbr-hints.gen.txt (探针可注入临时文件)
  static Table := Map()      ; "scope:abbr" -> [zh, en]
  static Loaded := false

  /** 备注取用: 按系统语言取一列; 无数据返回空串。 */
  static Note(scope, abbr) {
    this.Ensure()
    key := scope ":" abbr
    if (!this.Table.Has(key))
      return ""
    pair := this.Table[key]
    return SysLangIsChinese() ? pair[1] : pair[2]
  }

  /** 读取 (幂等)。@returns {boolean} 是否至少加载到一条 */
  static Ensure() {
    if (this.Loaded)
      return this.Table.Count > 0
    this.Loaded := true
    path := A_ScriptDir "\abbr-hints.gen.txt"
    if (!FileExist(path))
      return false
    text := ""
    try text := FileRead(path, "UTF-8")
    catch as e {
      EngineLogWarn("AbbrHints.Ensure", e.Message)
      return false
    }
    for line in StrSplit(text, "`n", "`r") {
      if (line = "")
        continue
      f := StrSplit(line, "`t")
      if (f.Length < 4 || f[1] = "" || f[2] = "")
        continue
      this.Table[f[1] ":" f[2]] := [f[3], f[4]]
    }
    return this.Table.Count > 0
  }

  /** 探针用: 直接注入内存表 (绕过文件)。 */
  static Inject(map) {
    this.Table := map
    this.Loaded := true
  }
}
