#Warn All, Off
; #Warn All, Off: 本文件会被独立 /Validate、也可能被未含依赖的脚本 Include —— 那时类名
; 引用是「只读未赋值」变量, AHK v2 默认 #Warn 会弹**加载期对话框**阻塞进程
; (/ErrorStdOut 无法抑制; FuzzyStrategy.ahk 同款先例)。
/**
 * AbbrSuggest.ahk —— 命令框输入期的**候选建议层** (候选词 + 备注, 可 ↑↓/回车选择)。
 *
 * 与既有两层的关系 (三者互不越界):
 *   ① 精确层: `makeCapsHook` 的 EndKeys/MatchList + `FuzzySuffixFire` 的最长后缀查表
 *      —— 命中即执行, 恒有最高优先级, 本文件不参与。
 *   ② 建议层 (**本文件**): 前缀/包含匹配 → 列为候选**只显示, 不执行**; 用户 ↑↓ 选、
 *      回车才执行。这是"我不确定命令叫什么"的场景。
 *   ③ 容错层 (`FuzzyStrategy`, 打错字): 只在 ① 全部未命中、且本层**零候选**时才被咨询
 *      —— 契约 §3.6 语义原样保留 (唯一容错候选静默执行)。
 *   衔接点只有一处: `FuzzySuffixFire` 精确循环未命中后先问本层, 本层说"接了"才 return。
 *
 * 版式: 命令框的列表本体系双行 (标题/路径 + 图标)。候选用**单行 + 右侧备注**
 *   (`ResultList.STYLE_INLINE`) —— 候选词只有 2~3 字符, 双行会把行高的一半留给空白;
 *   且备注不是路径, 交给图标提取只会白留一块图标位。判据与实现在命令框侧 (Rust)。
 *
 * 备注数据: 生成期落盘 `bin/abbr-hints.gen.txt` (每行 scope⇥abbr⇥中文⇥English)。
 *   为什么不是运行时读 config.json: AHK 无内置 JSON, 手写解析器是长期负担; 为什么不是
 *   写进 KeyFlux.ahk 的 Register 第 4 参: 那个签名是 CONTRACTS §3.6 冻结契约, 且会让
 *   parity 基线跟着漂。侧车文件是"两边都不动"的那条路; 文件缺失 = 只显示命令名, 不报错。
 *
 * 键盘: ↑↓ 移动且**不环绕** (到头即停; Clink 默认同款, 短列表绕回去只会让人找不着北);
 *   回车执行高亮项 —— 复用既有「待收尾」机制 (写 Pending + 停钩), 执行仍由
 *   EnterCapslockAbbr 延后 FinishDelayMs 完成, 不新建任何执行通道。
 *
 * 性能: 每次输入变化 = 1 次 O(N) 注册表扫描 (实测部署配置 N=30) + 1 次同步 WM_COPYDATA。
 *   无文件 IO (备注只读一次) / 无窗口枚举 (句柄一次找到即可) / 无正则。
 */
class AbbrSuggest {
  ; ---- 可调参数 ----
  static Enabled := true
  static MaxItems := 6      ; 候选条数上限 (命令框可视上限 12, 这里按"扫一眼"取 6)
  static MinLen := 1        ; 建议层生效的最短输入 (1 字符即出候选 —— 用户要的"输入 m 就列")

  ; ---- 内部状态 ----
  static Items := []        ; [[abbr, note], ...]
  static Index := 0         ; 1 基高亮; 0 = 无
  static Active := false    ; 列表是否正在显示
  static Scope := ""
  static ih := 0            ; 当前会话的 InputHook (回车/点选时 Stop 用)

  ; ============================================================
  ; 纯计算 (无副作用, 供探针直接调用)
  ; ============================================================

  /**
   * 建议候选: 本 scope 下「以 input 为前缀」的优先, 其后是「包含 input」的;
   * 同级按 (命令长度, 字典序) 升序 —— 短命令在前 (2 字母命令是绝大多数人的习惯)。
   * 完全等值的命令排除 (那是精确层的语义, 建议层不重复介绍)。
   * @returns {Array} [[abbr, note], ...], 已截断到 MaxItems
   */
  static Match(scope, input) {
    scored := []
    if (input = "")
      return scored
    prefix := scope ":"
    plen := StrLen(prefix)
    li := StrLen(input)
    for key in CommandResolver.Table {
      if (SubStr(key, 1, plen) != prefix)
        continue
      cmd := SubStr(key, plen + 1)
      if (cmd = input || StrLen(cmd) < li)
        continue
      if (SubStr(cmd, 1, li) = input)
        tier := 1
      else if (InStr(cmd, input))
        tier := 2
      else
        continue
      scored := this._InsertSorted(scored, cmd, tier)
    }
    res := []
    for i, e in scored {
      if (i > this.MaxItems)
        break
      res.Push([e["cmd"], AbbrHints.Note(scope, e["cmd"])])
    }
    return res
  }

  ; 按 (tier, 长度, 字典序) 插入 (N ≤ 数十, 插入排序足够)。就地改动并返回同一数组。
  static _InsertSorted(arr, cmd, tier) {
    n := arr.Length
    ln := StrLen(cmd)
    pos := n + 1
    Loop n {
      e := arr[A_Index]
      if (tier < e["tier"]
          || (tier = e["tier"] && ln < e["len"])
          || (tier = e["tier"] && ln = e["len"] && StrCompare(cmd, e["cmd"]) < 0)) {
        pos := A_Index
        break
      }
    }
    arr.InsertAt(pos, Map("cmd", cmd, "tier", tier, "len", ln))
    return arr
  }

  ; ============================================================
  ; 运行时入口 (FuzzySuffixFire 精确层未命中后调用)
  ; ============================================================

  /**
   * 计算并显示候选。
   * @returns {boolean} true = 本层已接管本次输入变化 (调用方不要再进容错层)
   */
  static OnInput(ih, scope, input) {
    if (!this.Enabled)
      return false
    ; 🔴 回车在 OnChar 里以换行 (Ord=10) 出现 (everything_search 实测记录同款坑):
    ;   不剥掉的话 "cw`n" 一条都匹配不上, 列表会在回车那一下莫名消失。
    input := Trim(input, "`r`n`t")
    if (StrLen(input) < this.MinLen) {
      this.Hide()
      return false
    }
    items := this.Match(scope, input)
    if (items.Length = 0) {
      this.Hide()
      return false
    }
    this.Scope := scope
    this.ih := ih
    this.Items := items          ; 执行用: 原始小写命令 (查表键)
    this.Index := 1
    if (!ResultList.Show(this._ViewOf(items), 1, ResultList.STYLE_INLINE)) {
      ; 命令框收不进去 (旧版 exe 只认 KFR2 / 窗口不在) ⇒ 退回纯文本提示,
      ; 行为不比改动前差; 仍算"接管", 免得容错层又冒一个 Tip 出来打架。
      this.Active := false
      this._FallbackTip(items)
      return true
    }
    this.Active := true
    return true
  }

  /**
   * 显示用的行集: 候选词**大写**。
   *
   * 为什么只做一份"显示副本"而不改数据: 命令框的既有惯例就是大写 (查询区那行由
   * `CharUpperW` 渲染, 见 backend_gdi 的 query 文本绘制), 列表跟着它才对得上;
   * 但执行必须拿**原始小写命令**去查 `CommandResolver.Table` (表键是小写),
   * 所以 `this.Items` 保持原样, 只有推给命令框的那份转大写。
   */
  static _ViewOf(items) {
    view := []
    for it in items
      view.Push([StrUpper(it[1]), it[2]])
    return view
  }

  /** 收起列表并清状态 (幂等)。 */
  static Hide() {
    if (!this.Active)
      return
    this.Active := false
    this.Index := 0
    this.Items := []
    ResultList.Hide()
  }

  ; ============================================================
  ; CommandInputHooks provider
  ; ============================================================

  /**
   * 按键。仅在高亮存在时消费 ↑↓/回车; 退格**不消费** (引擎要照常投递删除),
   * 只顺手收起陈旧列表 —— 退格不经 OnChar, 不会有 OnInput 来刷新它。
   */
  static OnKey(ih, vk, sc, scope) {
    if (vk = 0x08) {
      this.Hide()
      return false
    }
    if (!this.Active)
      return false
    if (vk = 0x26) {
      this.Move(-1)
      return true
    }
    if (vk = 0x28) {
      this.Move(1)
      return true
    }
    if (vk = 0x0D) {
      ; 重入闸: 长按/连击回车会产生多次 OnKeyDown (KeyOpt "N" 只通知不吞),
      ; 第一次执行后 Active 已置 false, 后续通知在这里就被挡住 (同 Everything 的守卫链)。
      if (this.Index >= 1 && this._Fire())
        return true
      return false
    }
    return false
  }

  /** 会话开始: 清掉上一会话的残留。 */
  static OnSessionBegin() {
    this.Hide()
  }

  /** 会话结束 (命中/Esc/超时): 收起列表。 */
  static OnSessionEnd() {
    this.Hide()
  }

  /** 命令框鼠标回推 (0x409)。kind: 1 点选 / 2 悬停。 */
  static OnRow(row1, kind) {
    if (!this.Active || row1 < 1 || row1 > this.Items.Length)
      return
    this.Index := row1
    if (kind = 1)
      this._Fire()
    ; 悬停: 命令框自己已把高亮画到该行, 这里只记行号供回车用
  }

  ; ============================================================
  ; 内部
  ; ============================================================

  /** 高亮移动 (不环绕)。 */
  static Move(delta) {
    n := this.Items.Length
    if (n = 0)
      return
    idx := (this.Index <= 0) ? 1 : this.Index + delta
    idx := (idx < 1) ? 1 : ((idx > n) ? n : idx)
    if (idx = this.Index)
      return
    this.Index := idx
    ResultList.Select(idx)
  }

  /**
   * 执行高亮项: 复用既有「待收尾」通道 (写 Pending + 停钩) —— 执行由
   * EnterCapslockAbbr 延后 FinishDelayMs 完成, 本函数不碰命令体、不新建执行路径。
   */
  static _Fire() {
    if (this.Index < 1 || this.Index > this.Items.Length)
      return false
    cmd := this.Items[this.Index][1]
    scope := this.Scope
    ih := this.ih
    this.Active := false
    this.Index := 0
    this.Items := []
    ResultList.Hide()
    if (IsObject(ih)) {
      try ih.Stop()
      catch as e
        EngineLogWarn("AbbrSuggest._Fire", e.Message)   ; 停钩失败不阻断执行 (Pending 已写)
    }
    CommandInputHooks.PendingScope := scope
    CommandInputHooks.PendingAbbr := cmd
    return true
  }

  /** 兜底显示 (命令框收不下时): 老式纯文本小窗, 一行一条。 */
  static _FallbackTip(items) {
    text := ""
    for i, it in items {
      note := it[2]
      text .= (i > 1 ? "`n" : "") StrUpper(it[1]) (note != "" ? "    " note : "")
    }
    try Tip(text, -2000)
    catch as e
      EngineLogWarn("AbbrSuggest._FallbackTip", e.Message)
  }
}

; 载入即挂接。三处依赖全部是**插槽/端口**, 不反向引用具体实现:
;   * 按键/会话边界 → CommandInputHooks provider (与插件同一套扩展点);
;   * 精确层未命中的委托 → CommandResolver.Suggest (与 Strategy 同款的策略插槽);
;   * 列表显示 → ResultList 协议端口; 备注 → AbbrHints 数据端口。
CommandInputHooks.Register(AbbrSuggest)
CommandResolver.Suggest := AbbrSuggest
ResultList.Handler := ObjBindMethod(AbbrSuggest, "OnRow")
ResultList.InstallNotify()
