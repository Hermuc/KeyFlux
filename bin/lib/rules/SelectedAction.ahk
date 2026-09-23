; ============================================================
; 选中动作系统 (Selected Action) —— 方案 D「单键分发」
; 参考 RunAny 的「选中内容 + 快捷键触发预设行为」能力
;
; 生成端契约 (config-server selectedActionCode 渲染, 任务 #29 冻结):
;   SelectedActionData := Array(
;     {matchType: "textType", matchValue: "url", key: 1, behavior: "open_url",
;      action: "open_url", actionValue: "", workingDir: "", name: "open_url"},
;     ...)
;   SelectedActionInit(">^p", SelectedActionData)
;
; 8 列含义: matchType (textType=内置文本特征值, 见 TextFeatureSpecs; fileExt=逗号分隔后缀) /
;   matchValue (条件值) / key (菜单序号 1-9, 同一 mapping 内从 1 递增) /
;   behavior (行为库 ID) / action (ResolveRuleAction 展开后基础动作) /
;   actionValue (展开后模板) / workingDir (工作目录) / name (显示名)。
; 数组顺序 = mappings 配置顺序 = 匹配优先级; 连续同 (matchType, matchValue) 的行
; 构成同一匹配组 (同一 mapping 的展开), key 在组内即菜单序号。
;
; 执行模型 (定稿): 选中文本/文件 -> 按主快捷键 -> 按数组行序找到首个类型匹配的组:
;   - 组内仅 1 条 entry: 直接执行;
;   - 组内多条: 弹无焦点菜单 (InputTipWindow 同款小窗), 按数字 1-9 立即执行对应项,
;     Esc 或重复按主键取消, 5s 超时自动取消, 淡入淡出不抢焦点;
;   - 无任何组匹配: Tip 气泡提示「未识别的类型」。
;
; 决策记录 (2026-09-05): 本版引擎不做 confirm/copyToClipboard/clearSelection 选项 ——
; 方案 D 菜单语义下这三个选项的行为未定义, 生成端 (任务 #29) 也未把 entry.options
; 写进数据数组; 执行模型定稿只有: 直接执行 / 菜单 / Esc / 超时。
;
; 阶段说明: 本文件重写入口与分发层 (SelectedActionInit / SelectedAction 类);
; 匹配原语 (MatchTextType/MatchFileExt) 与执行辅助 (OpenSelectedPaths/OpenSelectedFolder/
; DownloadMagnet/OpenRegistryKey/RunReplaced/RunScriptWithSelected 等) 原样保留;
; 选中内容获取统一在 SelectionContext, 执行层未来委托 ActionRegistry。
; 变量约定: {selected} 为规范形, %selected% 为兼容形 (由 context/SelectionContext.ahk
; 归一处理); 多文件用换行分隔, 与资源管理器复制文件到剪贴板的格式一致。
; ============================================================

/**
 * 初始化选中动作: 为「单键分发」注册主快捷键 (生成端在 InitKeymap 中调用)
 * @param hotkey 主快捷键 (如 ">^p"); 禁用/空热键时生成端输出空串不调用本函数, 此处兜底跳过
 * @param entries 数据数组 (契约见文件头), 每项 8 列
 */
SelectedActionInit(hotkeyName, entries) {
  if (hotkeyName == "" || !IsObject(entries) || entries.Length == 0) {
    return
  }
  ; 彩蛋 (▶ 真实执行): 留存 entries 副本 + 注册 250ms 轮询 (文件不存在时零开销返回)
  SelectedAction.Data := entries
  if not (SelectedAction.PlayTimerStarted) {
    SelectedAction.PlayTimerStarted := true
    SetTimer(SelectedAction.WatchPlayRequest, 250)
  }
  ; N 键链式热键 (物理键数 ≥3): AHK 原生自定义组合只支持两键, 拆为「头部热键 + 尾部 InputHook 顺序匹配」。
  ; 头部两种形态 (UI 生成端 HotkeyCaptureCore.CommitStaged 对应):
  ;   纯键链   "j & k & l"   -> 头部注册自定义组合 "j & k", 尾部 ["l"]
  ;   修饰链   "<^j & k & l" -> 头部注册普通修饰热键 "<^j", 尾部 ["k","l"]
  ; 修饰链不能用 "LCtrl & j" 自定义组合形式: 组合不能混修饰键, 且那样会把 LCtrl
  ; 全局注册为前缀键, 改变系统里单独按 LCtrl 的行为 (副作用), 故头段走普通热键。
  parts := StrSplit(hotkeyName, "&", " ")
  head := Trim(parts[1])
  headIsMod := parts.Length >= 2 && HotkeyHeadHasModifier(head)
  if (parts.Length >= 3 || headIsMod) {
    if (headIsMod) {
      remaining := []
      Loop parts.Length - 1 {
        remaining.Push(Trim(parts[A_Index + 1]))
      }
    } else {
      head := head " & " Trim(parts[2])
      remaining := []
      Loop parts.Length - 2 {
        remaining.Push(Trim(parts[A_Index + 2]))
      }
    }
    chain(thisHotkey) {
      SelectedAction._ChainWait(head, remaining, hotkeyName, entries)
    }
    try {
      KeymapManager.GlobalKeymap.Map(head, chain, , , , "S")
    } catch {
      return
    }
    return
  }
  trigger(thisHotkey) {
    SelectedAction.Trigger(hotkeyName, entries)
  }
  ; 无效热键(如反引号)注册失败时跳过该方案, 避免单个方案拖垮整个脚本 (与旧 InitActionScheme 同策略)
  try {
    KeymapManager.GlobalKeymap.Map(hotkeyName, trigger, , , , "S")
  } catch {
    return
  }
}

/**
 * 头段是否为「带修饰键的普通热键」(如 "<^j"): 含 AHK 修饰符字符即视为是。
 * 链式 UI 生成端只产生侧别前缀 (<^ >^ <! >! <+ >+ <# >#) 形式, 纯键名不含这些字符。
 */
HotkeyHeadHasModifier(part) {
  return InStr(part, "^") || InStr(part, "!") || InStr(part, "+") || InStr(part, "#")
}

class SelectedAction {
  ; 菜单运行状态 (当前为单主热键设计, 重入即视为取消)
  static MenuActive := false
  static ChainCompleted := false  ; N 键链式等待的完成标记
  static MenuIH := ""       ; 菜单 InputHook (打开期间非空, 供取消方跨线程 Stop)
  static MenuWindow := ""   ; InputTipWindow 实例 (淡出结束前持引用防 GC 拆窗)
  static MenuSeq := 0       ; 菜单代际号: 每次开/关菜单自增, 让过期的淡入淡出定时器自杀

  ; 彩蛋 (▶ 真实执行) 状态: SelectedActionInit 留存 entries 副本供复用
  static Data := ""              ; Init 时留存的 entries 数组 (静态副本)
  static PlayLastSeq := 0        ; 上次执行的请求 seq (去重)
  static PlayTimerStarted := false  ; 轮询定时器是否已注册 (只注册一次)

  /**
   * 主热键入口: 取选中内容 -> 行序匹配 -> 单条直执 / 多条弹菜单 / 未命中气泡
   * @param hotkeyName 主快捷键串 (如 ">^p"), 用于菜单期间主键取消判定
   * @param entries 数据数组
   */
  static Trigger(hotkeyName, entries) {
    if (this.MenuActive) {
      this._CancelMenu()
      return
    }
    selected := SelectionContext.Get()
    if not (selected.content) {
      Tip(Translation().no_items_selected, -700)
      return
    }
    group := this._FirstMatch(entries, selected)
    if (group.Length == 0) {
      Tip(Translation().no_matching_type, -2000)
      return
    }
    if (group.Length == 1) {
      this._Execute(group[1], selected)
      return
    }
    this._RunMenu(hotkeyName, group, selected)
  }

  /**
   * 按数组行序扫描, 把「连续同 (matchType, matchValue)」的行视为同一匹配组
   * (即同一 mapping 的展开, key 在组内即菜单序号), 返回首个匹配的组。
   * 行序即优先级: 首个匹配组命中后不再看后续组 (旧 RunActionScheme 同语义)。
   * 无匹配时返回空数组。
   */
  static _FirstMatch(entries, selected) {
    n := entries.Length
    i := 1
    while (i <= n) {
      mt := entries[i].matchType
      mv := entries[i].matchValue
      j := i
      while (j < n && entries[j+1].matchType == mt && entries[j+1].matchValue == mv) {
        j++
      }
      if (this._MatchCondition(mt, mv, selected)) {
        group := Array()
        Loop j - i + 1 {
          group.Push(entries[i + A_Index - 1])
        }
        return group
      }
      i := j + 1
    }
    return Array()
  }

  /**
   * 匹配类型判定, 语义同旧 MatchActionRule:
   * fileExt 只认文件选中, textType 只认文本选中 (多选文件匹配语义在 MatchFileExt 内)。
   * matchValue 为 "type:<id>" 引用时改走自定义类型表 (方案 C7): 未命中 / kind 不符一律不命中
   * (与 Go 端 matchActionRule 的引用分支同口径); 非引用值走原分支, 一行不改。
   */
  static _MatchCondition(matchType, matchValue, selected) {
    switch matchType {
      case "fileExt":
        if (selected.type != "file") {
          return false
        }
        if (IsCustomMatchRef(matchValue)) {
          mt := ResolveMatchValue(matchValue)
          return IsObject(mt) && mt.kind == "fileExt" && MatchFileExtList(mt.exts, selected.content)
        }
        return MatchFileExt(matchValue, selected.content)
      case "textType":
        if (selected.type != "text") {
          return false
        }
        if (IsCustomMatchRef(matchValue)) {
          mt := ResolveMatchValue(matchValue)
          return IsObject(mt) && mt.kind == "text" && MatchCustomRules(mt.rules, selected.content)
        }
        return MatchTextType(matchValue, selected.content)
    }
    return false
  }

  /**
   * 多 entry 无焦点菜单 (InputTipWindow + InputHook):
   *   - 数字 1-9 列入 EndKeys: 按下立即终止输入并执行对应项 (EndKeys 会被吞掉, 不漏键);
   *   - Esc 取消; T5 5s 超时取消;
   *   - 重复按主键取消: 菜单期间先停用主热键 —— 热键线程正阻塞在 Wait,
   *     MaxThreadsPerHotkey=1 下重复按压不会重入热键回调, 按键只会落入 InputHook,
   *     故对主键 KeyOpt S+N (吞键+通知), OnKeyDown 里由 _IsMainKey 判定后取消;
   *   - Suspend 包裹沿用 AbbrInput.StartInputHook 模式, 但尊重进入前的挂起状态
   *     (已挂起时不再 Suspend(true)/Suspend(false), 避免把「暂停 KeyFlux」误恢复);
   *   - 淡入淡出经 SetTimer 逐级透明度实现, 窗口 +Disabled + NoActivate 不抢焦点。
   */
  static _RunMenu(hotkeyName, entries, selected) {
    byKey := Map()
    menuText := ""
    for e in entries {
      k := e.key
      if (k < 1 || k > 9) {
        continue
      }
      byKey[String(k)] := e
      menuText .= k ". " e.name "`n"
    }
    if (byKey.Count == 0) {
      return
    }

    waitKey := ExtractWaitKey(hotkeyName)
    ; 菜单期间停用主热键 (结束后恢复), 让重复主键走 InputHook 通知路径
    KeymapManager.GlobalKeymap.DisableHotkey(waitKey)

    wasSuspended := A_IsSuspended
    if not (wasSuspended) {
      Suspend(true)
    }

    ; 评审 M2: 窗口创建/InputHook 等易抛语句纳入 try/finally ——
    ; 任何异常路径 (窗口创建失败/Wait 异常等) 都保证全局状态还原:
    ; MenuActive/MenuIH 复位、Suspend 按 wasSuspended 还原、主热键 EnableHotkey,
    ; 不再泄漏「热键停用+全局挂起」状态拖垮后续按键响应。_CancelMenu/代际号逻辑不变。
    this.MenuSeq++
    seq := this.MenuSeq
    ih := ""
    endReason := ""
    try {
      win := InputTipWindow(RTrim(menuText, "`n"), 12, 6, 4, 12, 8)
      this.MenuWindow := win
      hwnd := win.gui.Hwnd
      try WinSetTransparent(0, "ahk_id " hwnd)   ; 先置全透明再 Show, 淡入从 0 开始无闪现
      win.Show()
      this._Fade(hwnd, 0, 255, seq)

      ih := InputHook("T5", "{Esc}123456789")
      this.MenuIH := ih
      this.MenuActive := true
      if (waitKey != "") {
        try ih.KeyOpt("{" waitKey "}", "SN")
        ih.OnKeyDown := (i, vk, sc) => (SelectedAction._IsMainKey(vk, hotkeyName) ? SelectedAction._CancelMenu() : "")
      }

      ih.Start()   ; InputHook 必须显式 Start, 否则 Wait 立即以 Stopped 返回 (未开始即视为已终止)
      endReason := ih.Wait()
      ih.Stop()
    } finally {
      ; 还原段 (含异常路径), 与上方 DisableHotkey/Suspend 对称
      this.MenuActive := false
      this.MenuIH := ""
      if not (wasSuspended) {
        Suspend(false)
      }
      if (waitKey != "") {
        KeymapManager.GlobalKeymap.EnableHotkey(waitKey)
      }
    }

    chosen := ""
    if (endReason == "EndKey" && byKey.Has(ih.EndKey)) {
      chosen := byKey[ih.EndKey]
    }
    ; EndReason: EndKey(数字)=执行 / Esc / Timeout / Stopped (重复主键) 均为取消
    this._CloseMenu()
    if (chosen != "") {
      this._Execute(chosen, selected)
    }
  }

  /**
   * N 键链式等待 (物理键数 ≥3): 头部热键 (自定义组合 "j & k" 或修饰热键 "<^j") 触发后,
   * 经 InputHook 等待剩余键序列。
   *   - 有序匹配: 按下正确键推进序列, 全部命中后触发; 按错任何键 / Esc / 5s 超时 → 取消;
   *   - 组合前缀键 (k1) 按住期间的自动重复 keydown 忽略, 防误取消;
   *   - 剩余键以 S (Suppress) 抑制, 不泄漏到前台应用;
   *   - 状态管理 (Suspend/热键停用/还原) 与 _RunMenu 同款。
   */
  static _ChainWait(combo, remaining, hotkeyName, entries) {
    waitKey := ExtractWaitKey(combo)
    KeymapManager.GlobalKeymap.DisableHotkey(waitKey)

    wasSuspended := A_IsSuspended
    if not (wasSuspended) {
      Suspend(true)
    }

    this.MenuSeq++
    seq := this.MenuSeq
    this.ChainCompleted := false
    ih := ""
    try {
      ih := InputHook("T5", "{Esc}")
      this.MenuIH := ih
      this.MenuActive := true
      for _, keyName in remaining {
        try ih.KeyOpt("{" keyName "}", "S")  ; 抑制剩余键, 不泄漏到前台
      }
      prefixKey := StrLower(Trim(StrSplit(combo, "&")[1]))
      if HotkeyHeadHasModifier(prefixKey) {
        prefixKey := StrLower(ExtractWaitKey(prefixKey))  ; 修饰链头段 "<^j" -> 主键 "j"
      }
      ih.OnKeyDown := (i, vk, sc) => SelectedAction._ChainOnKey(i, vk, seq, prefixKey, remaining, hotkeyName, entries)

      ih.Start()
      ih.Wait()
      ih.Stop()
    } finally {
      this.MenuActive := false
      this.MenuIH := ""
      if not (wasSuspended) {
        Suspend(false)
      }
      if (waitKey != "") {
        KeymapManager.GlobalKeymap.EnableHotkey(waitKey)
      }
    }

    ; 剩余序列全部命中 (InputHook 被 _ChainOnKey Stop) → 触发; Esc/超时/按错 → 取消
    if (this.ChainCompleted) {
      this.Trigger(hotkeyName, entries)
    }
  }

  /**
   * 链式等待的单键回调: 匹配 remaining 首元素推进序列;
   * 全部命中 → 标记 completed 并 Stop InputHook; 按错 → 取消本次链。
   */
  static _ChainOnKey(ih, vk, seq, prefixKey, remaining, hotkeyName, entries) {
    if (this.MenuSeq != seq) {
      return
    }
    keyName := StrLower(GetKeyName(Format("vk{:X}", vk)))
    if (keyName == prefixKey) {
      return  ; 组合首键按住期间的自动重复, 忽略
    }
    if (keyName != StrLower(remaining[1])) {
      this._CancelMenu()  ; 按错: 取消本次链
      return
    }
    remaining.RemoveAt(1)
    if (remaining.Length == 0) {
      this.ChainCompleted := true  ; 完成标记 (实例属性, 供 _ChainWait 读取)
      this._CancelMenu()
    }
  }

  /**
   * 判定 OnKeyDown 捕获的按键是否为「重复按下的主键」:
   * 终止键与主键同名 (vk 相同) 且主键要求的修饰符均处于按下状态
   * (左右 Ctrl/Win 不严格区分, 方向性只由原热键定义约束)
   */
  static _IsMainKey(vk, hotkeyStr) {
    try {
      waitKey := ExtractWaitKey(hotkeyStr)
      if (waitKey == "" || GetKeyVK(waitKey) != vk) {
        return false
      }
      if (InStr(hotkeyStr, "^") && !GetKeyState("Control")) {
        return false
      }
      if (InStr(hotkeyStr, "+") && !GetKeyState("Shift")) {
        return false
      }
      if (InStr(hotkeyStr, "!") && !GetKeyState("Alt")) {
        return false
      }
      if (InStr(hotkeyStr, "#") && !GetKeyState("LWin") && !GetKeyState("RWin")) {
        return false
      }
      return true
    }
    catch {
      return false
    }
  }

  /**
   * 取消正在显示的菜单 (重复主键/重入触发时由其他线程调用):
   * Stop 后菜单线程 ih.Wait 返回 "Stopped", 由其走取消分支统一清理
   */
  static _CancelMenu() {
    ih := this.MenuIH
    if (ih != "") {
      ih.Stop()
    }
  }

  /**
   * 关闭菜单窗口: 淡出 -> 复位透明属性 -> 隐藏并释放引用。
   * 代际号自增, 让尚未完成的淡入定时器自杀, 避免两个渐变定时器互相拉扯。
   */
  static _CloseMenu() {
    win := this.MenuWindow
    this.MenuWindow := ""
    if (win == "") {
      return
    }
    hwnd := win.gui.Hwnd
    this.MenuSeq++
    seq := this.MenuSeq
    done() {
      try WinSetTransparent("Off", "ahk_id " hwnd)
      win.Hide()
    }
    this._Fade(hwnd, 255, 0, seq, done)
  }

  /**
   * 窗口透明度渐变 (淡入 0->255 / 淡出 255->0), SetTimer 逐步执行不阻塞线程。
   * seq 与当前代际号不符或窗口已不存在时自动停止; onDone 为完成回调 (可选)。
   * 评审 M3: 淡出被新代际顶替时 (重复主键连按两次触发两次 _CloseMenu 等),
   * 顶替方已清空 MenuWindow 引用且不会再 Hide —— 若被顶替的淡出不补执行 onDone
   * (win.Hide()), 窗口将保持可见成为幽灵窗。故 seq 失配且本次为淡出 (alphaTo==0)
   * 时先补执行 onDone 再自杀; 淡入被顶替维持自杀 (窗口由其关闭方负责)。
   */
  static _Fade(hwnd, alphaFrom, alphaTo, seq, onDone := "") {
    alpha := alphaFrom
    delta := (alphaTo > alphaFrom) ? 32 : -48
    step() {
      if (SelectedAction.MenuSeq != seq || !WinExist("ahk_id " hwnd)) {
        SetTimer(step, 0)
        ; 被顶替时淡出的 onDone (win.Hide()) 必须补执行, 否则留幽灵窗;
        ; try 包裹防窗口已被销毁时 Hide 抛错
        if (onDone != "" && alphaTo == 0) {
          try onDone()
        }
        return
      }
      alpha += delta
      reached := (delta > 0) ? (alpha >= alphaTo) : (alpha <= alphaTo)
      if (reached) {
        SetTimer(step, 0)
        try WinSetTransparent(alphaTo, "ahk_id " hwnd)
        if (onDone != "") {
          onDone()
        }
        return
      }
      try WinSetTransparent(alpha, "ahk_id " hwnd)
    }
    SetTimer(step, 16)
  }

  /**
   * 执行一条 entry (8 列契约, action 为生成端展开后的基础动作)
   * textType 特征的专用行为 (open_url 等) 直接作用于选中内容, 不接受命令模板;
   * 特征与行为的合法组合见 config-server/internal/script/actionscheme.go 的 textTypeActions。
   * 执行前广播 selection_action 慢事件 (薄观察层, 隔离兜底, 不影响动作执行;
   * 方案 D 后事件字段由 schemeId/ruleIndex 调整为 behavior/name/selected)。
   */
  static _Execute(entry, selected) {
    try EventBus.Publish("selection_action", Map("behavior", entry.behavior, "name", entry.name, "selected", selected.content))
    content := selected.content
    switch entry.action {
      case "open_url":
        ; 默认浏览器打开选中网址 (AHK Run 对 http(s)/ftp URL 自动调用系统默认浏览器)
        Run(Trim(content))
      case "open_path":
        ; 注册表路径 (HKEY_*/HKxx 开头) 自动转入注册表编辑器定位 (2026-09-10 open_registry 并入)
        if RegExMatch(Trim(StrSplit(content, "`n")[1]),
            "i)^(HKEY_CLASSES_ROOT|HKEY_CURRENT_USER|HKEY_LOCAL_MACHINE|HKEY_USERS|HKEY_CURRENT_CONFIG|HKCR|HKCU|HKLM|HKU|HKCC)(\\|$)") {
          OpenRegistryKey(content)
        } else {
          OpenSelectedPaths(content)
        }
      case "open_folder":
        OpenSelectedFolder(content)
      case "magnet_download":
        DownloadMagnet(content)
      case "open":
        RunReplaced(entry.actionValue, content, entry.workingDir)
      case "run":
        RunReplaced(entry.actionValue, content, entry.workingDir)
      case "search":
        url := SelectionContext.NormalizeForSearch(entry.actionValue, content)
        Run(url)
      case "send_keys":
        Send(SelectionContext.Normalize(entry.actionValue, content))
      case "script":
        RunScriptWithSelected(entry.actionValue, content)
      case "copy":
        A_Clipboard := SelectionContext.Normalize(entry.actionValue, content)
      default:
        ; 插件/未内置动作 (IAction, 契约 §3.2/§3.4): 兜底委托 ActionRegistry 统一执行。
        ; 插件启动期经 ActionRegistry.Register 注册 Type="plugin:<id>:<name>" 后即达;
        ; 未注册的未知动作由 Execute 内部拒绝 (日志), 静默返回。
        if (ActionRegistry.Get(entry.action) != "") {
          ctx := {selected: selected.content, isFile: (selected.type = "file"), winTitle: "", params: Map(), source: "plugin"}
          ActionRegistry.Execute(entry.action, ctx)
        } else {
          Tip("未知动作: " entry.action, -1200)
        }
    }
  }

  /**
   * 彩蛋 (▶ 真实执行): 按 typeId 用该类型真实配置的行为直接执行预设样例。
   * typeId 来自设置界面 ▶ 经后端白名单校验后写入的请求文件 (见 WatchPlayRequest):
   *   - 内置文本特征 (url/path/magnet/bilibili/plain): 命中 textType 组, 样例为文本;
   *   - "type:<id>" (自定义类型): 查 CustomMatchTypes 决定 text/file, 命中对应引用组;
   *   - 文件后缀组 (已折叠为规范化后缀串, 如 "jpg,png"): 命中 fileExt 组, 样例为文件 (A_Desktop)。
   * 样例内容硬编码 (不进配置), 与 _Execute 真实执行同一条入口; 未命中/未配置用现有翻译文案提示
   * (不新增 i18n 键)。多行为时执行组内首条 (与菜单序号 1 等价); 不改菜单逻辑。
   * @param typeId 解析后的条件值: 内置特征值 / "type:<id>" / 规范化后缀串 (group 已由后端折叠)
   */
  static PlaySample(typeId) {
    if not (IsObject(this.Data) && this.Data.Length > 0) {
      Tip(Translation().no_matching_type, -1500)   ; 未配置选中动作
      return
    }
    ; 决定样例形态: 文本 → type:"text"; 其余 (文件后缀/自定义 fileExt) → type:"file"
    isText := false
    if (typeId == "url" || typeId == "path" || typeId == "magnet" || typeId == "bilibili" || typeId == "plain") {
      isText := true
    } else if (SubStr(typeId, 1, 5) == "type:") {
      mt := ResolveMatchValue(typeId)
      isText := !(IsObject(mt) && mt.kind == "fileExt")
    }
    ; 文件后缀组 (group 折叠后的后缀串) 与 type:<id>(fileExt) → 文件样例; 其余 → 文本样例
    if (isText) {
      selected := {type: "text", content: this._SampleText(typeId)}
    } else {
      selected := {type: "file", content: A_Desktop}
    }
    group := this._FindGroup(typeId)
    if (group.Length == 0) {
      Tip(Translation().no_matching_type, -1500)   ; 未命中对应类型
      return
    }
    this._Execute(group[1], selected)   ; 组内首条 (等价菜单序号 1)
  }

  /**
   * 文本特征彩蛋样例内容 (硬编码, 不进配置)。
   * path 用 A_Desktop (真实存在的目录), open_path 会直接打开它; 其余为可识别的样例文本。
   */
  static _SampleText(typeId) {
    switch typeId {
      case "url": return "https://github.com/Hermuc/KeyFlux"
      case "path": return A_Desktop
      case "magnet": return "magnet:?xt=urn:btih:0000000000000000000000000000000000000000"
      case "bilibili": return "BV1xx411c7mD"
      default: return "示例文本 sample text"
    }
  }

  /**
   * 按 matchValue 精确查找匹配组 (复用 _FirstMatch 的连续同值分组逻辑, 但按条件值而非选中内容匹配):
   * 返回首个 matchValue 等于 typeId 的组 (数组); 无匹配返回空数组。
   * 内置文本特征 (matchValue=特征值) / type:<id> (matchValue=引用串) / 后缀组 (matchValue=后缀串)
   * 均按条件值唯一命中, 不依赖选中内容, 故文件后缀组即使用目录样例也能稳定定位。
   */
  static _FindGroup(matchValue) {
    data := this.Data
    n := data.Length
    i := 1
    while (i <= n) {
      mt := data[i].matchType
      mv := data[i].matchValue
      j := i
      while (j < n && data[j + 1].matchType == mt && data[j + 1].matchValue == mv) {
        j++
      }
      if (mv == matchValue) {
        group := Array()
        Loop j - i + 1 {
          group.Push(data[i + A_Index - 1])
        }
        return group
      }
      i := j + 1
    }
    return Array()
  }

  /**
   * 轮询设置界面下发的彩蛋请求文件 (%TEMP%\kf_play_request.json):
   *   - 文件不存在 → 零开销返回 (250ms 一次的常规态);
   *   - 读出 typeId + seq, 校验格式, 执行后**删除文件** (无论成败, 幂等);
   *   - seq 去重: 记录上次执行的 seq, 重复 seq 跳过 (防御文件删除与重读的竞态);
   * 请求文件内容由后端白名单校验后写入, 不含任何命令/路径参数, 无注入面。
   */
  static WatchPlayRequest() {
    path := A_Temp "\kf_play_request.json"
    if not (FileExist(path)) {
      return
    }
    try content := FileRead(path)
    catch {
      return
    }
    ; 解析 typeId 与 seq (引擎无内置 JSON 库, 用受控格式的正则提取)
    if not (RegExMatch(content, '"typeId"\s*:\s*"([^"]*)"', &m)) {
      FileDelete(path)   ; 格式异常 → 删文件避免反复触发
      return
    }
    typeId := m[1]
    seq := 0
    if (RegExMatch(content, '"seq"\s*:\s*(\d+)', &sm)) {
      seq := Integer(sm[1])
    }
    if (seq == this.PlayLastSeq) {
      return   ; 已执行过该 seq (防御竞态; 常规下 seq 单调递增不会重复)
    }
    this.PlayLastSeq := seq
    FileDelete(path)   ; 执行与否都删, 保证幂等、不残留
    this.PlaySample(typeId)
  }
}

/**
 * 获取当前选中内容 (实现已迁移到 context/SelectionContext.ahk, 保留函数签名兼容存量调用)
 * @deprecated 新代码请直接用 SelectionContext.Get(); 本壳保留是为兼容用户自定义代码
 *             (data/custom_functions.ahk) 与旧配置中的遗留引用, 勿删除。
 * @returns {{type: string, content: string}} type: file / text / ""
 */
GetSelectedContent() {
  return SelectionContext.Get()
}

; ============================================================
; 自定义匹配类型 (方案 C7): 用户可在设置界面新增"文本特征"(如 网盘链接) 与"文件后缀分组",
; 映射的条件值写 "type:<id>" 引用它们, 生成端把"可用匹配类型"渲染为全局表 CustomMatchTypes
; (config-server/internal/script/model/methods.go 的 CustomMatchTypes(); 挂载点
;  config-server/templates/keyflux.tmpl:74)。表结构:
;   {id: {kind: "text", rules: [{op, value}, ...]} | {kind: "fileExt", exts: [...]}}
;   - kind=text    -> 4 个封闭算子 OR 求值 (MatchCustomRules)
;   - kind=fileExt -> 后缀集匹配 (MatchFileExtList) —— 自定义文件类型与文件分组共用该形态
; 无自定义类型且无文件分组时生成端不输出该表 (下方 IsSet 守卫兜底)。
;
; 挂载点位于 InitKeymap() 函数体内, 故生成片段自带 global 前缀; 真机实测该 global 赋值对本类
; 的 static 方法可见 (2026-09-15 spike)。契约: 数据数组仍 8 列, 仅 matchValue 值域新增
; "type:<id>" 一类形态 (加法扩展)。
;
; 与 Go 端一致性: 算子语义由双端一致性向量
; (config-server/internal/script/testdata/match_ops.json) 守护; 大小写折叠一律走 AsciiLower
; (仅折 A-Z), 禁止 StrLower —— 后者对非 ASCII 做区域相关变换, 会造成两端语义分歧。
; ============================================================

/**
 * 是否为自定义匹配类型引用 ("type:<id>")。
 * `:` 是 Windows 文件名非法字符, 故该前缀不可能与任何真实文件后缀或内置特征值碰撞。
 */
IsCustomMatchRef(matchValue) {
  return SubStr(matchValue, 1, 5) == "type:"
}

/**
 * 把引用解析为类型定义对象; 非引用、无表、或未命中一律返回 ""。
 * 调用方据此区分"内置分支"与"引用不命中" (后者一律不匹配, 与 Go 端 c==nil / ok=false 同口径)。
 * @returns {object|string} 表项 (含 kind/rules|exts), 或 ""
 */
ResolveMatchValue(matchValue) {
  global CustomMatchTypes
  if not (IsCustomMatchRef(matchValue)) {
    return ""
  }
  if not (IsSet(CustomMatchTypes)) {
    return ""
  }
  id := SubStr(matchValue, 6)
  if not (CustomMatchTypes.Has(id)) {
    return ""
  }
  return CustomMatchTypes[id]
}

/**
 * 仅折叠 ASCII 大写字母 A-Z (+32), 其余字符原样。与 Go 端 asciiFold 逐字对齐。
 * 刻意不用 StrLower: 它会对非 ASCII (CJK/全角) 做区域相关变换, 造成双端分歧。
 */
AsciiLower(s) {
  out := ""
  Loop Parse, s {
    o := Ord(A_LoopField)
    if (o >= 65 && o <= 90) {
      out .= Chr(o + 32)
    } else {
      out .= A_LoopField
    }
  }
  return out
}

/**
 * 自定义文本类型匹配: rules 之间 OR, 任一命中即命中 (与 Go 端 matchCustomRules 同语义)。
 * 作用域约定 (方案 C7 核心风控, 两端逐字一致):
 *   - equals / prefix: 作用于 Trim(content) 的首个非空行;
 *   - suffix / contains: 作用于整个 Trim(content);
 * 空 value 的 prefix/suffix/contains 恒真 (镜像 Go 的 HasPrefix/HasSuffix/Contains 对空串返回 true),
 * 该形态由保存校验拒绝, 此处仅为手改配置的一致性兜底。
 * @param rules [{op, value}, ...]
 * @param content 选中文本
 * @returns {boolean}
 */
MatchCustomRules(rules, content) {
  if not (IsObject(rules)) {
    return false
  }
  m := Trim(content, " `t`r`n`v`f")
  first := ""
  for line in StrSplit(m, "`n") {
    line := Trim(line, " `t`r`v`f")
    if (line != "") {
      first := line
      break
    }
  }
  for r in rules {
    op := r.op
    v := AsciiLower(r.value)
    hay := (op == "equals" || op == "prefix") ? AsciiLower(first) : AsciiLower(m)
    switch op {
      case "equals":
        if (hay == v) {
          return true
        }
      case "prefix":
        if (SubStr(hay, 1, StrLen(v)) == v) {
          return true
        }
      case "suffix":
        if (v == "" || (StrLen(hay) >= StrLen(v) && SubStr(hay, -StrLen(v)) == v)) {
          return true
        }
      case "contains":
        if (v == "" || InStr(hay, v)) {
          return true
        }
    }
  }
  return false
}

/**
 * 数组形文件后缀匹配: 语义逐字对齐 MatchFileExt (SplitPath 取末段扩展名 / 去点 /
 * 忽略大小写 / "*" 匹配任意文件 / 无扩展名跳过), 供 type: 文件引用复用。
 * MatchFileExt 本体冻结不动 (注释互指, 语义唯一真源见其文档注释)。
 * @param exts 后缀数组 (不含点)
 * @param content 文件路径列表 (换行分隔)
 * @returns {boolean}
 */
MatchFileExtList(exts, content) {
  if not (IsObject(exts)) {
    return false
  }
  for line in StrSplit(content, "`n") {
    SplitPath(line, , , &ext)
    if not (ext) {
      continue
    }
    for v in exts {
      v := LTrim(Trim(v), ".")
      if v == "*" || AsciiLower(v) == AsciiLower(ext) {
        return true
      }
    }
  }
  return false
}

/**
 * 文件后缀匹配, 条件值支持逗号分隔多个后缀, "*" 匹配任意文件
 * @param matchValue 如 ".txt" / "txt,md" / "*"
 * @param content 文件路径列表 (换行分隔)
 * @returns {boolean}
 */
MatchFileExt(matchValue, content) {
  if matchValue == "*" {
    return true
  }
  exts := StrSplit(matchValue, ",")
  for line in StrSplit(content, "`n") {
    SplitPath(line, , , &ext)
    if not (ext) {
      continue
    }
    for v in exts {
      v := LTrim(Trim(v), ".")
      ; 扩展名比较忽略大小写 (与 Go 端 EqualFold 一致): Windows 上 .JPG/.PNG 等大写扩展名也必须能匹配
      if v == "*" || StrLower(v) == StrLower(ext) {
        return true
      }
    }
  }
  return false
}

/**
 * 内置文本特征注册表 —— AHK 侧唯一真源。
 *
 * 组织方式 (与 Go 端 config-server/internal/behaviors/textfeatures.go **同构** 且同序):
 *   - 表的顺序 = 界面顺序 (「添加映射」类型下拉 / 映射行特征 Toggle), **兜底特征恒居末位**;
 *   - named=true  具名特征: 各持一条**锚定**正则, 命中即"属于该特征";
 *   - named=false 兜底特征 (目前仅 plain): **不持正则**, 命中条件由具名集**派生** ——
 *     "其余全部具名特征都不命中"。故新增具名特征时 plain 的排除集自动扩大, 无需手工同步
 *     (2026-09-17 之前这里是硬编码的 `not (isURL or isPath or isMagnet or isBilibili)`,
 *      加第 5 个特征时靠人肉改 —— 正是本次重构要消灭的失败模式);
 *   - ignoreCase 与 pattern 分离: 正则源串与 Go 端**逐字相同**, 大小写开关运行时施加
 *     (AHK 加 "i)" 前缀 / Go 编译期加 (?i)) ⇒ 可工具化比对 (tools/texttype_conformance.py)。
 *
 * 为什么 plain 必须排除全部具名特征: 映射按数组行序取**首个**命中, 而「添加映射」恒追加到末尾
 * ⇒ 若 plain 也命中某具名特征的样例, 先建的「纯文本」映射会恒遮蔽后建的具名映射 (配了却不生效)。
 *
 * 不要用 $ 收尾: PCRE2 的 $ 还会认末尾换行前的位置, 而 Go 的 $ 只认文本末尾 ⇒ 多行选中时两端分歧;
 * \z 在两侧都表示"文本绝对末尾", 是方言交集。
 *
 * @returns {array} 特征表 (static, 只求值一次)
 */
TextFeatureSpecs() {
  static specs := [
    {value: "url", named: true, ignoreCase: true, pattern: "^(https?|ftp)://"},
    {value: "path", named: true, ignoreCase: false, pattern: "^(\\\\[^\\]+\\[^\\]+|[a-zA-Z]:\\)"},
    {value: "magnet", named: true, ignoreCase: true, pattern: "^magnet:"},
    {value: "bilibili", named: true, ignoreCase: true, pattern: "^(av[0-9]+|bv[0-9a-z]{10})\z"},
    {value: "plain", named: false, ignoreCase: false, pattern: ""},
  ]
  return specs
}

/**
 * 单个特征的命中判定 (fallback 特征的排除集从注册表**派生**, 非硬编码)。
 * @param spec TextFeatureSpecs() 的表项
 * @param content 选中文本 (不 Trim —— 具名特征都是 ^ 锚定, 且该值可能被原样拼进 URL)
 * @returns {boolean}
 */
TextFeatureHit(spec, content) {
  if (spec.named) {
    return RegExMatch(content, (spec.ignoreCase ? "i)" : "") . spec.pattern) > 0
  }
  for other in TextFeatureSpecs() {
    if (other.named and RegExMatch(content, (other.ignoreCase ? "i)" : "") . other.pattern) > 0) {
      return false
    }
  }
  return true
}

/**
 * 文本特征匹配入口 —— 与 Go 端 behaviors.MatchTextFeature 逐字对齐。
 * 特征名归一化: 去首尾空白 (大小写敏感比较, 与旧 switch 同口径 —— AHK 的 switch/`=` 对字符串
 * 是大小写不敏感而 `==` 敏感, 这里刻意用 `==` 保持"配置值必须小写"的既有严格性);
 * content 一律不 Trim (锚定口径)。未知特征名返回 false (与 Go 端同口径)。
 * @param t 特征类型
 * @param content 选中文本
 * @returns {boolean}
 */
MatchTextType(t, content) {
  tv := Trim(t)
  for spec in TextFeatureSpecs() {
    if (spec.value == tv) {
      return TextFeatureHit(spec, content)
    }
  }
  return false
}

/**
 * 打开选中路径 (逐行), 按系统关联程序打开, 等同资源管理器双击
 * @param content 路径列表 (换行分隔)
 */
OpenSelectedPaths(content) {
  for line in StrSplit(content, "`n") {
    line := Trim(line)
    if (line) {
      Run(QuoteIfSpace(line))
    }
  }
}

/**
 * 打开选中路径所在文件夹: 选中本身是目录时直接打开, 是文件时打开其父目录
 * 多选时只处理第一行 (行为语义: 打开第一个路径所在文件夹)
 * @param content 选中内容
 */
OpenSelectedFolder(content) {
  line := Trim(StrSplit(content, "`n")[1])
  if not (line) {
    return
  }
  ; FileExist 返回属性串, 含 "D" 表示目录
  if InStr(FileExist(line), "D") {
    Run(QuoteIfSpace(line))
    return
  }
  SplitPath(line, , &dir)
  if (dir) {
    Run(QuoteIfSpace(dir))
  }
}

/**
 * 用默认 BT 下载工具下载磁力链接 (走 magnet: 协议关联, 不硬编码具体下载软件)
 * 系统未注册默认处理器时给出中文提示而非静默失败
 * @param content 选中内容
 */
DownloadMagnet(content) {
  line := Trim(StrSplit(content, "`n")[1])
  if not (line) {
    return
  }
  if not (CheckMagnetHandler()) {
    Tip(Translation().magnet_no_handler, -2500)
    return
  }
  Run(line)
}

/**
 * 检测系统是否注册了 magnet: 协议默认处理器
 * HKCR 为 HKLM/HKCU 类注册的合并视图, 普通用户权限可读
 * @returns {boolean}
 */
CheckMagnetHandler() {
  try {
    cmd := RegRead("HKCR\magnet\shell\open\command")
    return cmd != ""
  }
  catch {
    return false
  }
}

/**
 * 打开注册表编辑器并定位到选中键路径
 * 原理: regedit 启动时读取 LastKey 值自动定位 (系统内置行为, 无需第三方工具与管理员权限)
 * regedit 已在运行时先结束再重开 (regedit 无未保存数据, 杀进程安全)
 * @param content 选中内容
 */
OpenRegistryKey(content) {
  path := Trim(StrSplit(content, "`n")[1])
  if not (path) {
    return
  }
  try RegWrite(path, "REG_SZ", "HKCU\Software\Microsoft\Windows\CurrentVersion\Applets\Regedit", "LastKey")
  catch {
    Tip(Translation().registry_open_failed, -2500)
    return
  }
  if ProcessExist("regedit.exe") {
    ProcessClose("regedit.exe")
    ProcessWaitClose("regedit.exe", 2)
  }
  Run("regedit.exe")
}

/**
 * 把 %selected% 替换为选中内容后执行命令, 多文件时逐行执行
 * 参考 RunAny: 占位符未带引号且内容含空格时自动包上双引号
 * @param command 命令模板
 * @param content 选中内容
 * @param workingDir 工作目录 (可选)
 */
RunReplaced(command, content, workingDir := "") {
  lines := StrSplit(content, "`n")
  if lines.Length == 1 {
    Run(SelectionContext.Normalize(command, QuoteIfSpace(lines[1])), workingDir)
    return
  }
  for line in lines {
    Run(SelectionContext.Normalize(command, QuoteIfSpace(line)), workingDir)
  }
}

/**
 * 内容含空格且未加引号时自动包上双引号 (参考 RunAny)
 * @param text 文本
 * @returns {string}
 */
QuoteIfSpace(text) {
  q := Chr(34)  ; 字面双引号: AHK v2 中 """" 会解析为两个空字符串, 必须用 Chr(34)
  if InStr(text, " ") and not (SubStr(text, 1, 1) == q) {
    return q text q
  }
  return text
}

/**
 * 执行 AHK 脚本片段: 把 %selected% 替换为字符串字面量, 写入临时脚本用 AutoHotkey 执行
 * @param code 脚本模板
 * @param content 选中内容
 */
RunScriptWithSelected(code, content) {
  file := A_WorkingDir "\data\selected_action_cache.ahk"
  script := StrReplace(code, "%selected%", ToAHKString(content))
  f := FileOpen(file, "w", "UTF-8")
  f.Write(script)
  f.Close()
  Run('"' A_WorkingDir '\bin\AutoHotkey64.exe" "' file '"', A_WorkingDir "\data")
}

/**
 * 转义为 AHK 双引号字符串字面量
 * 注意: 反引号+引号 (`` `" ``) 在字符串内是合法的转义, 但为避免歧义这里用 Chr(34) 构造双引号
 * @param text 文本
 * @returns {string}
 */
ToAHKString(text) {
  ; 反引号 → `` (双反引号)
  text := StrReplace(text, "``", "````")
  ; 双引号 → `" (反引号 + 双引号)
  text := StrReplace(text, Chr(34), "``" Chr(34))
  return Chr(34) text Chr(34)
}
