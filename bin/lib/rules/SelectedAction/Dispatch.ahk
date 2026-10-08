; ============================================================
; SelectedAction —— 触发与分发（class SelectedAction 第一段）
;
; 本文件是 class SelectedAction 的**开头**：静态状态 + 主入口 Trigger + 行序匹配。
; class 的其余两段在 Menu.ahk / Execute.ahk / Sample.ahk —— 由本文件末尾的 #Include
; 在 **class 体内部**展开（AHK v2 允许 #Include 出现在 class 体内，列 0；
; 2026-10-08 实测确认）。文件末尾的 `}` 收口整个 class。
; ============================================================

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


#Include Menu.ahk
#Include Execute.ahk
#Include Sample.ahk
}
