; ============================================================
; SelectedAction —— 菜单 UI 与链式按键（class SelectedAction 第二段）
;
; 由 Dispatch.ahk 在 class 体内 #Include，故本文件的成员缩进必须是 2 空格。
; 覆盖: 打开菜单 / 序号选择 / 二级链式等待与逐键推进 / 主键取消 / 关闭与淡出。
; ============================================================

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
    catch as err {
      ; 修饰键状态读取失败 (会话切换期 / 钩子异常) ⇒ 按「非主键重复」处理, 不打断热键链;
      ; 留痕: 否则表现为「某热键偶发不触发」, 日志里无任何线索。
      EngineLogWarn("SelectedAction._IsMainKey: 修饰键探测失败", "hotkey=" hotkeyStr " err=" err.Message)
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

