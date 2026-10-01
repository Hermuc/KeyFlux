#Warn All, Off
/**
 * FuzzyStrategy —— 缩写命令模糊策略: 编辑距离容错匹配 + 候选提示。
 * 提案与完整设计: docs/contracts-proposals/fuzzy-strategy.md (接口先行, 约束 #2)。
 * 契约: docs/CONTRACTS.md §3.6 CommandResolver.Strategy 留桩的落地实现 ——
 *   未命中 → 子序列匹配 → 编辑距离 ≤ Threshold → 候选集;
 *   唯一候选 → 静默执行 + Tip 提示实际命令; 多候选 → 仅 Tip 列出, 不执行。
 *
 * 接入方式 (零侵入默认):
 *   CommandResolver.Strategy 默认 "" (= 未设置, 部署版 AHK v2.0.19 无 null 关键字),
 *   引擎未载入本文件时 CommandResolver 的所有委托点都有 `!= ""` 守卫 —— 行为与
 *   纯精确匹配完全一致。本文件末尾「载入即挂接」一行把自己赋给 CommandResolver.Strategy,
 *   前提是 CommandResolver.ahk 先于本文件被 #Include (模板现有顺序满足)。
 *
 * 逐字符容错 (OnInputChanged, 由 FuzzySuffixFire 在全部精确后缀未命中后委托):
 *   衔接关系 = 精确后缀 (最长优先) 恒有最高优先级, 容错层只在精确层零命中时被咨询;
 *   容错命中四条件 (缺一不可):
 *     ① Enabled 且输入长度 ≥ MinInputLen (1 字符不做容错, 首键误触面太大);
 *     ② 精确层零命中 (由调用点保证);
 *     ③ 前缀守卫 —— 本 scope 不存在以输入为真前缀的已注册命令 (输入仍可能长成
 *        精确命令时容错层完全静默, 防止打 "swap" 的过程在 "sw" 处被 "se" 抢先执行);
 *     ④ 候选集恰为 1 (多候选歧义只提示不执行)。
 *   命中动作复用后缀模糊命中的「待收尾」机制 (Pending + ih.Stop), 不新建执行通道;
 *   终止字符补投 EchoTerminalChar 仅透传模式 (与 FuzzySuffixFire 命中分支同款)。
 *
 * 候选提示 (多候选时): InputTipWindow 单例 (core 既有机制, 本批零改动), 候选一行一个;
 *   精确命中 / 容错命中 / 退格 / 会话边界 / 无候选时收起; 2s 无后续输入自动隐藏。
 *   provider 惰性注册到 CommandInputHooks (全 static, 恒不消费按键 —— 只观察退格与会话边界)。
 *
 * 性能预算 (约束 #6): 每次输入变化 = 2 次 O(N) 注册表扫描 (候选 + 前缀守卫) + 长度带
 *   预筛后的少量小规模 DP (阈值 1 时仅 |长度差| ≤ 1 的命令进 DP); 无文件 IO (日志仅
 *   异常分支) / 无窗口枚举 / 无 DllCall / 无正则。插件消费字符后 DispatchChar 提前
 *   return, 本层完全不进入。快路径 (重映射/发键/鼠标) 不经过本模块任何代码 (§0 红线)。
 *
 * 分号域未接 (semiHook.OnChar 不经 CommandInputOnChar → FuzzySuffixFire 链路, 接线
 *   需改模板): 本策略当前只在 capslock 域生效, 分号域行为不变。
 *
 * #Warn All, Off (文件首行, quickswitch 库文件同款先例): 本文件独立 /Validate 或被
 *   未含 CommandResolver 的脚本 Include 时, 类名引用是「只读未赋值」变量, AHK v2 默认
 *   #Warn 会弹加载期对话框阻塞进程 (/ErrorStdOut 无法抑制, 契约 §3.6 阶段 4 坑②)。
 */
class FuzzyStrategy {
  ; ---- 可调参数 ----
  static Enabled := true       ; 总开关; false = 容错与提示全旁路 (等价纯精确匹配)
  static Threshold := 1        ; 编辑距离阈值 (Levenshtein; 冻结契约口径「编辑距离≤1」)
  static MaxCandidates := 5    ; 候选提示最多条数 (按 (距离, 字典序) 升序截断, 最近者保留)
  static MinInputLen := 2      ; 容错层生效的最短输入长度
  static HintShowMs := 2000    ; 提示窗无后续输入时的自动隐藏时长 (ms)
  ; ---- 内部状态 ----
  static TipWindow := ""       ; 显示载体实例 (InputTipWindow); 测试可注入桩, "" = 首次懒建真身
  static _hideTimerFn := ""    ; 自动隐藏定时器 (同一 BoundFunc 重臂, 防泄漏)
  static _providerWired := false  ; provider 惰性注册标志 (首次显示提示时注册一次)

  ; ============================================================
  ; 纯计算 (无副作用, 供本层与测试复用)
  ; ============================================================

  /**
   * Levenshtein 编辑距离。比较用 AHK `=` (大小写不敏感), 与 InputHook MatchList 的
   * 不敏感语义一致。单行滚动 DP: O(la*lb) 时间 / O(lb) 空间; 空串边界 = 对方长度。
   */
  static EditDistance(a, b) {
    la := StrLen(a)
    lb := StrLen(b)
    if (la = 0)
      return lb
    if (lb = 0)
      return la
    prev := []
    Loop lb + 1
      prev.Push(A_Index - 1)   ; prev[j+1] = dp[0][j] = j
    Loop la {
      i := A_Index
      curr := [i]              ; curr[1] = dp[i][0] = i
      ca := SubStr(a, i, 1)
      Loop lb {
        j := A_Index
        d := prev[j] + ((ca = SubStr(b, j, 1)) ? 0 : 1)   ; 替换/匹配 (prev[j] = dp[i-1][j-1])
        if (prev[j + 1] + 1 < d)                           ; 删除 (prev[j+1] = dp[i-1][j])
          d := prev[j + 1] + 1
        if (curr[j] + 1 < d)                               ; 插入 (curr[j] = dp[i][j-1])
          d := curr[j] + 1
        curr.Push(d)
      }
      prev := curr
    }
    return prev[lb + 1]
  }

  /**
   * 子序列判定: short 的全部字符能否按原序在 long 中找到 (双指针)。
   * 仅 CommandResolver.Resolve 未命中路径使用 (冻结契约 §3.6: 未命中 → 子序列匹配)。
   * 空输入恒 false —— 空输入不应产生任何候选。
   */
  static IsSubsequence(short, long) {
    ls := StrLen(short)
    if (ls = 0)
      return false
    ll := StrLen(long)
    pos := 1
    Loop ls {
      c := SubStr(short, A_Index, 1)
      found := false
      while (pos <= ll) {
        if (SubStr(long, pos, 1) = c) {
          pos += 1
          found := true
          break
        }
        pos += 1
      }
      if (!found)
        return false
    }
    return true
  }

  /**
   * 候选集: 本 scope 注册表中「编辑距离 ∈ [1, Threshold]」的命令 (includeSubsequence 时
   * 并入子序列匹配者), 按 (距离, 字典序) 升序, 截断至 MaxCandidates。
   * 与 input 精确等值的命令恒排除 —— 等值属于精确层语义, 容错层不得重新引入
   * (逐字符路径下等值输入必然已被精确层处理)。
   * 长度带预筛: 编辑距离 ≥ 长度差, |长度差| > Threshold 且非子序列匹配者跳过 DP (纯提速,
   * 结果与全量 DP 一致)。
   */
  static Candidates(scope, input, includeSubsequence := false) {
    out := []
    if (input = "")
      return out
    prefix := scope ":"
    plen := StrLen(prefix)
    li := StrLen(input)
    for key in CommandResolver.Table {
      if (SubStr(key, 1, plen) != prefix)
        continue
      cmd := SubStr(key, plen + 1)
      if (cmd = input)
        continue
      isSub := includeSubsequence && this.IsSubsequence(input, cmd)
      if (!isSub && Abs(StrLen(cmd) - li) > this.Threshold)
        continue
      d := this.EditDistance(input, cmd)
      if ((d >= 1 && d <= this.Threshold) || isSub)
        out := this._InsertSorted(out, cmd, d)
    }
    if (out.Length > this.MaxCandidates)
      out.RemoveAt(this.MaxCandidates + 1, out.Length - this.MaxCandidates)
    names := []
    for i, e in out
      names.Push(e["cmd"])
    return names
  }

  ; 候选按 (距离, 字典序) 升序插入 (N ≤ 数十, 插入排序足够)
  static _InsertSorted(arr, cmd, d) {
    n := arr.Length
    pos := n + 1
    Loop n {
      if (d < arr[A_Index]["d"] || (d = arr[A_Index]["d"] && StrCompare(cmd, arr[A_Index]["cmd"]) < 0)) {
        pos := A_Index
        break
      }
    }
    arr.InsertAt(pos, Map("cmd", cmd, "d", d))
    return arr
  }

  /**
   * 前缀守卫判定: 本 scope 是否存在「以 input 为真前缀」的已注册命令。
   * true = 输入仍可能长成精确命令 → 容错层完全静默 (不执行也不提示)。
   */
  static HasPrefixCommand(scope, input) {
    if (input = "")
      return false
    prefix := scope ":"
    plen := StrLen(prefix)
    li := StrLen(input)
    for key in CommandResolver.Table {
      if (SubStr(key, 1, plen) != prefix)
        continue
      cmd := SubStr(key, plen + 1)
      if (StrLen(cmd) > li && SubStr(cmd, 1, li) = input)
        return true
    }
    return false
  }

  ; ============================================================
  ; 运行时入口 (CommandResolver 委托)
  ; ============================================================

  /**
   * 逐字符容错判定 + 候选提示。由 FuzzySuffixFire 在「全部精确后缀未命中」后调用
   * (每次命令框输入变化一次)。规则见类头注释; 命中 → _FireTolerant, 多候选 → ShowHints。
   * @param ih     InputHook 对象 (命中时 ih.Stop())
   * @param scope  "capslock" | "semicolon"
   * @param input  当前输入缓冲 (ih.Input)
   * @param char   本次键入的字符 (命中时作为终止字符补投)
   */
  static OnInputChanged(ih, scope, input, char) {
    if (!this.Enabled)
      return
    if (StrLen(input) < this.MinInputLen) {
      this.HideHints()
      return
    }
    cands := this.Candidates(scope, input, false)
    if (cands.Length = 0) {
      this.HideHints()
      return
    }
    if (this.HasPrefixCommand(scope, input)) {
      this.HideHints()      ; 前缀守卫: 仍可能长成精确命令 → 完全静默
      return
    }
    if (cands.Length = 1) {
      this._FireTolerant(ih, scope, cands[1], char)
      return
    }
    this.ShowHints(cands)   ; 多候选: 仅提示, 不执行 (冻结契约语义)
  }

  /**
   * 容错命中: 复用后缀模糊命中的既有「待收尾」机制 —— 终止字符补投 (仅透传模式, 同
   * FuzzySuffixFire 命中分支) + 写 Pending + 停钩; 执行由 EnterCapslockAbbr 的
   * TakePending 分支延后 FinishDelayMs 完成, 事件 abbr_submit 带 fuzzy=true。
   * 本函数不直接执行命令体, 不新建任何执行通道。
   */
  static _FireTolerant(ih, scope, cmd, char) {
    this.HideHints()
    if (CommandDisplay.SuppressKeycap) {
      try CommandDisplay.EchoTerminalChar(char)
      catch as e
        CommandResolver._log("Fuzzy 容错命中终止字符补投异常: " e.Message)
    }
    CommandInputHooks.PendingScope := scope
    CommandInputHooks.PendingAbbr := cmd
    CommandResolver._log("Fuzzy 容错命中: '" cmd "' (scope=" scope ")")
    ih.Stop()
  }

  /** 精确命中瞬间调用 (CommandResolver 委托): 收起提示 —— 命令即将执行, 提示不得残留。 */
  static OnExactHit() {
    this.HideHints()
  }

  /**
   * CommandResolver.Resolve 未命中 (会话已结束, 输入是完整意图) 时的策略委托。
   * 冻结契约 §3.6 语义: 候选集 = 子序列匹配 ∪ 编辑距离 ∈ [1, Threshold];
   *   唯一候选 → Tip 提示实际命令 + 经 CommandResolver.Resolve(scope, 候选) 真身执行
   *   (步骤守卫语义与 abbr_submit 事件全部继承; 事件流先 matched=false 后 matched=true,
   *   即「尝试未中」+「容错执行」两条, 属预期);
   *   多候选 → 仅 Tip 列出, 不执行; 无候选 → 静默无操作 (与未设策略时一致)。
   */
  static Resolve(scope, command, hook := "") {
    if (!this.Enabled || command = "")
      return
    cands := this.Candidates(scope, command, true)
    if (cands.Length = 0)
      return
    if (cands.Length = 1) {
      cmd := cands[1]
      try Tip("未命中 " command ", 已执行容错匹配: " cmd, -2000)
      catch as e
        CommandResolver._log("Fuzzy Resolve Tip 异常: " e.Message)
      CommandResolver._log("Resolve 未命中 '" command "' -> 容错执行 '" cmd "' (scope=" scope ")")
      CommandResolver.Resolve(scope, cmd, hook)
      return
    }
    list := ""
    for i, c in cands
      list .= (i > 1 ? " / " : "") c
    try Tip("未命中 " command ", 候选: " list, -2500)
    catch as e
      CommandResolver._log("Fuzzy Resolve Tip 异常: " e.Message)
  }

  ; ============================================================
  ; CommandInputHooks provider 钩子 (全 static, §3.12 硬约束 0 同款;
  ; 惰性注册于首次显示提示时; 恒不消费按键 —— 只观察, 不改变既有派发顺序)
  ; ============================================================

  /** 退格: 输入缓冲已变化, 旧候选陈旧, 立即收起。恒返回 false (不消费)。 */
  static OnKey(ih, vk, sc, scope) {
    if (vk = 0x08)
      this.HideHints()
    return false
  }

  /** 会话开始: 收起上一会话可能残留的提示。 */
  static OnSessionBegin() {
    this.HideHints()
  }

  /** 会话结束 (命中/Esc/超时): 收起提示。 */
  static OnSessionEnd() {
    this.HideHints()
  }

  ; ============================================================
  ; 提示载体 (InputTipWindow 单例 —— core 既有机制, 本批零改动)
  ; ============================================================

  /** 显示候选提示 (多行); 重臂自动隐藏定时器。TipWindow 可被测试注入桩。 */
  static ShowHints(cands) {
    if (cands.Length = 0)
      return
    text := ""
    for i, c in cands
      text .= (i > 1 ? "`n" : "") c
    if (!IsObject(this.TipWindow)) {
      try this.TipWindow := InputTipWindow("", 12, 4, 2, 12, 8)
      catch as e {
        CommandResolver._log("Fuzzy 提示窗创建异常: " e.Message)
        return
      }
    }
    try this.TipWindow.Show(text)
    catch as e {
      CommandResolver._log("Fuzzy 提示窗 Show 异常: " e.Message)
      return
    }
    if (!IsObject(this._hideTimerFn))
      this._hideTimerFn := ObjBindMethod(FuzzyStrategy, "HideHints")
    SetTimer(this._hideTimerFn, -this.HintShowMs)
    this._EnsureProvider()   ; 显示即需要观察者 (退格/会话边界收起提示), 惰性注册一次
  }

  /** 收起提示并撤销自动隐藏定时器 (幂等, 任意时机可调)。 */
  static HideHints() {
    if (IsObject(this._hideTimerFn))
      SetTimer(this._hideTimerFn, 0)   ; 撤销定时器 (不存在时为无副作用的空操作)
    if (IsObject(this.TipWindow)) {
      try this.TipWindow.Hide()
    }
  }

  ; provider 惰性注册 (幂等; Register 自带去重)。失败只记日志, 不影响输入。
  static _EnsureProvider() {
    if (this._providerWired)
      return
    this._providerWired := true
    try CommandInputHooks.Register(FuzzyStrategy)
    catch as e
      CommandResolver._log("FuzzyStrategy provider 注册异常: " e.Message)
  }
}

; 载入即挂接 (CommandResolver.Strategy 默认 "" = 未设置)。本行执行前提: CommandResolver.ahk
; 已先于本文件被 #Include (模板现有顺序: lib/commands/CommandResolver.ahk 在前, 追加一行
; 即满足)。未载入本文件时引擎停留在纯精确匹配, 零行为变更。
CommandResolver.Strategy := FuzzyStrategy
