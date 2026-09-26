/**
 * CommandResolver —— 缩写命令运行时解析表 (docs/CONTRACTS.md §3.6, 接口已冻结)。
 * 模块化重构阶段 4 新增: 取代生成脚本中的缩写 switch。
 * 生成端在 InitKeymap 中调用 Register 注册闭包步骤, 触发时经 Resolve 精确查表执行。
 * 快路径红线不适用说明: 缩写命令本就是一次输入后查表执行, 不在重映射/发键快路径上。
 */

; 步骤数据: 闭包即"待执行数据"。
; call 为无参闭包 () => <原 switch case 体语句>, 参数在执行时求值,
; 与旧 switch 语义一致 (含 {selected} 运行时替换、路径变量引用、遗留代码载荷)。
class CommandStep {
  call := ""
  winTitle := ""
  conditionType := 0 ; 0 = 无守卫; 1-5 语义同 matchWinTitleCondition

  __New(call, winTitle := "", conditionType := 0) {
    this.call := call
    this.winTitle := winTitle
    this.conditionType := conditionType
  }
}

class CommandResolver {
  ; "scope:command" -> [CommandStep, ...]; scope 分域: "capslock" | "semicolon"
  static Table := Map()

  ; 解析策略对象, 可插拔。阶段 4 留桩: 未命中静默无操作。
  ; 空串 = 未设置 (部署版 AHK v2.0.19 无 null 关键字)
  static Strategy := ""

  /**
   * 注册缩写命令。重复命令: 记日志, 不覆盖先到者 (约束 4)。
   * @param scope "capslock" | "semicolon"
   * @param command 缩写命令文本
   * @param steps [CommandStep, ...]
   */
  static Register(scope, command, steps) {
    key := scope ":" command
    if (this.Table.Has(key)) {
      CommandResolver._log("Register rejected: duplicate command '" key "' (first registration wins)")
      return false
    }
    this.Table[key] := steps
    return true
  }

  /**
   * 精确命中 → 按 steps 顺序执行 (对齐旧 switch 语义):
   *   无守卫步骤: 执行后继续下一步骤;
   *   带守卫步骤: matchWinTitleCondition 命中 → 执行并立即返回, 未命中 → 跳过。
   * 未命中 → 委托 Strategy; 阶段 4 无策略, 静默无操作。
   * 阶段 6: 提交即广播 abbr_submit (命中与否都报, matched 见契约 §3.1)。
   * @param fuzzy true = 后缀模糊命中 (FuzzySuffixFire 路径), 仅影响事件上报, 执行语义不变。
   */
  static Resolve(scope, command, hook := "", fuzzy := false) {
    key := scope ":" command
    ; scope -> source 词表 (契约 §3.1: "caps" | "semi")
    source := (scope == "capslock") ? "caps" : "semi"
    if (!this.Table.Has(key)) {
      try EventBus.Publish("abbr_submit", Map("source", source, "command", command, "matched", false, "fuzzy", fuzzy))
      if (this.Strategy != "") {
        this.Strategy.Resolve(command, hook)
      }
      return
    }
    try EventBus.Publish("abbr_submit", Map("source", source, "command", command, "matched", true, "fuzzy", fuzzy))
    for step in this.Table[key] {
      if (step.conditionType != 0) {
        if (matchWinTitleCondition(step.winTitle, step.conditionType)) {
          step.call.Call()
          return
        }
        continue
      }
      step.call.Call()
    }
  }

  /**
   * Oracle 导出 (对接 settings.exe DumpPlan 的 abbr 段, 见 docs/CONTRACTS.md §5):
   * 运行时实际注册的命令集 + 每命令步骤数, 与生成端计划 diff。
   * 阶段 4 维度: 命令集相等 + 步骤数相等; 动作参数级对比留待阶段 5 加载器。
   * 命令按字典序输出, 同配置多次调用结果一致 (确定性)。
   * @param outFile 输出文件路径, 空串 = 返回文本不写盘 (供测试用)
   */
  static DumpAbbr(outFile := "") {
    caps := []
    semi := []
    keys := []
    for k in this.Table
      keys.Push(k)
    ; 字典序排序 (Bubble, 条目少无所谓性能; 注意 > 对字符串按数字比较, 须用 StrCompare)
    n := keys.Length
    Loop n - 1 {
      Loop n - A_Index {
        if (StrCompare(keys[A_Index], keys[A_Index + 1]) > 0) {
          t := keys[A_Index]
          keys[A_Index] := keys[A_Index + 1]
          keys[A_Index + 1] := t
        }
      }
    }
    for k in keys {
      e := Map("command", SubStr(k, InStr(k, ":") + 1), "steps", this.Table[k].Length)
      ; scope 取冒号前首段 (与 Register 的 scope 参数同源, 不硬编码前缀字符串)
      if (SubStr(k, 1, InStr(k, ":") - 1) == "capslock")
        caps.Push(e)
      else
        semi.Push(e)
    }
    json := "{`n"
    json .= "  `"capslock`": [" CommandResolver._dumpEntries(caps) "],`n"
    json .= "  `"semicolon`": [" CommandResolver._dumpEntries(semi) "]`n"
    json .= "}`n"
    if (outFile != "")
      FileAppend(json, outFile)
    return json
  }

  static _dumpEntries(entries) {
    s := ""
    for i, e in entries {
      if (i > 1)
        s .= ", "
      s .= "{`"command`": " CommandResolver._jsonStr(e["command"]) ", `"steps`": " e["steps"] "}"
    }
    return s
  }

  ; 最小 JSON 字符串转义 (命令词表不含特殊字符, 防御性保留)
  static _jsonStr(s) {
    s := StrReplace(s, "\", "\\")
    s := StrReplace(s, "`"", "\`"")
    return "`"" s "`""
  }

  ; 错误日志 (与 ActionRegistry._log 同策略: 追加写, 失败静默)
  static _log(msg) {
    try {
      FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") " " msg "`n", "logs\command_resolver.log")
    }
  }
}

/**
 * 命令模糊输入 —— 逐字符实时后缀校验 (契约 §6 第 1 条落地, 模板接在 InputHook.OnChar)。
 * 每键入一个字符: 取当前缓冲 (ih.Input) 的尾部连续片段, 从最长后缀向最短逐个查表,
 * 首个命中即为结果 (最长优先, 保证 dfc→fc 不误取更短的 c): **立即停止输入钩子**, 并把命中
 * 记为「待收尾」交给 EnterCapslockAbbr 延后执行 (见 CommandInputHooks.FinishDelayMs)。
 * 全部未命中则忽略本次输入, 继续等待后续字符累积。
 *
 * 🔴 为什么不在本函数里直接执行 (2026-09-20): 命中这一击的字符 (终止字符) 刚随物理键抵达
 * 命令框窗口, 命令框需要一次绘制周期才把它显示出来; 当场 Resolve 会让命令的窗口抢在前头、
 * 旧实现还会紧接隐藏命令框 ⇒ 该字符永远显示不出来 (用户真机实测「最后一个字母不显示」)。
 *
 * 与原生精确匹配的关系: 全串命中由 InputHook MatchList 在 OnChar 之前先行终止输入, 故本函数
 * 只会遇到"带多余前缀"的缓冲 (精确命中走 Match 分支, 同样经延后收尾)。
 * 快路径红线: 缩写命令属输入后查表的慢路径, 与重映射/发键/鼠标无关。
 */
FuzzySuffixFire(ih, char, scope) {
  input := ih.Input
  len := StrLen(input)
  if (len < 1)
    return
  ; 后缀从长到短: start 从 1 递增, 即截取长度从 len 递减到 1 (含单字符命令)
  Loop len {
    suffix := SubStr(input, A_Index)
    if (CommandResolver.Table.Has(scope ":" suffix)) {
      ; 只记录 + 停钩 (EndReason="Stopped" -> EnterCapslockAbbr 走「待收尾」分支);
      ; 不在回调里执行命令 —— 见函数头注释 (终止字符需要绘制时间)
      ;
      ; 🔴 终止字符补投 (2026-09-20): 命中这一击就结束了会话, 该字符**不会被原生显示**
      ; (与 Match 分支同理, 那一路同样靠显式补投; 用户实测「最后一个字母不显示」)。
      ; 透传模式下 CommandInputOnChar 的 EchoChar 被 ShouldEcho 兑停, 故走唯一允许绕过
      ; 总开关的收口 CommandDisplay.EchoTerminalChar; 历史形态那边已由 EchoChar 投过,
      ; 再补会双显 —— 故只在透传模式下补。
      if (CommandDisplay.SuppressKeycap) {
        try CommandDisplay.EchoTerminalChar(char)
        catch as e
          CommandInputHooks._log("Fuzzy 终止字符补投异常: " e.Message)
      }
      CommandInputHooks.PendingScope := scope
      CommandInputHooks.PendingAbbr := suffix
      ih.Stop()
      return
    }
  }
}
