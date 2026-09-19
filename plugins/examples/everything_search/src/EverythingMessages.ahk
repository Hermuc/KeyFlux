; ============================================================
; EverythingMessages —— 插件自带文案 (中/英)。
; 刻意不塞进引擎的 translation.ahk: 引擎文案是「引擎 UI」的词表, 插件文案属于插件自身,
; 混入会让引擎词表被第三方插件污染。语言判定复用引擎已公开的 SysLangIsChinese()。
;
; 词表纪律: **只保留有实调用点的键** —— 每个键都必须能在本插件内 grep 到
; `EverythingMessages.T("<键>")` (或经 `_ErrorKey` 的返回值间接命中)。2026-09-19 清理过
; 一批零引用键 (hint_continue / title_key_space / err_empty), 新增键请先确认调用点。
; ============================================================

class EverythingMessages {
  ; 语言判定只做一次 (与引擎 Translation() 同款「首调固定」语义)
  static En := 0
  static _ready := false

  static _Init() {
    if (this._ready)
      return
    this._ready := true
    try
      this.En := !SysLangIsChinese()
    catch
      this.En := false
  }

  /** 取文案: 参数为键名。英文缺失时回落中文。 */
  static T(key) {
    this._Init()
    return this.En ? this._en(key) : this._zh(key)
  }

  static _zh(key) {
    switch key {
      case "hint_no_selection": return "没有选中文字 — 继续输入检索词"
      case "hint_empty": return "没有匹配的文件或文件夹"
      case "err_es_not_found": return "未找到 es.exe（命令行通道不可用）"
      case "err_not_running": return "Everything 未运行，且未能自动拉起（请检查 Everything 路径）"
      case "err_no_path": return "未配置 everything.exe 路径（点插件卡片填写）"
      case "err_query": return "Everything 查询失败"
      case "err_launched_gui": return "未找到 es.exe — 已用 Everything 打开搜索结果"
      case "title": return "Everything"
    }
    return key
  }

  static _en(key) {
    switch key {
      case "hint_no_selection": return "Nothing selected — type your query"
      case "hint_empty": return "No matching files or folders"
      case "err_es_not_found": return "es.exe not found (CLI channel unavailable)"
      case "err_not_running": return "Everything is not running and could not be launched (check the path setting)"
      case "err_no_path": return "everything.exe path is not configured (click the plugin card)"
      case "err_query": return "Everything query failed"
      case "err_launched_gui": return "es.exe not found — opened the results in Everything"
      case "title": return "Everything"
    }
    return key
  }
}
