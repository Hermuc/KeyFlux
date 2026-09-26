/**
 * ConfigProvider —— 插件设置独立存储 (docs/CONTRACTS.md §3.8)。
 * 存储文件: <生成脚本目录>\..\data\plugin-settings.json (路径按脚本位置推导,
 * 保证 GenerateAHK 验证路径与运行路径一致)。
 *
 * v1 最小实现 (契约标注 "AHK 无内置 JSON 库"): 扁平字符串键值
 *   {"<pluginId>:<key>": "<value>", ...}
 * 值仅字符串 (写入端做 JSON 转义); 需要结构化的插件自行编码 (如逗号拼接),
 * 键空间按 pluginId 隔离, 插件互相不可见。
 */
class ConfigProvider {
  static StorePath() {
    return A_ScriptDir "\..\data\plugin-settings.json"
  }

  static Get(pluginId, key) {
    raw := this._ReadAll()
    if (raw = "")
      return ""
    full := this._Key(pluginId, key)
    if !RegExMatch(raw, '"' this._RegexEscape(full) '"\s*:\s*"((?:[^"\\]|\\.)*)"', &m)
      return ""
    return this._Unescape(m[1])
  }

  static Set(pluginId, key, value) {
    raw := this._ReadAll()
    full := this._Key(pluginId, key)
    needle := '"' this._RegexEscape(full) '"\s*:\s*"(?:[^"\\]|\\.)*"'
    replacement := '"' this._JsonEscape(full) '": "' this._JsonEscape(value) '"'
    if RegExMatch(raw, needle) {
      raw := this._RegexReplaceLit(raw, needle, replacement)
    } else {
      ; 空文件或追加: 在首个 { 之后插入 (已有键对则补逗号)
      raw := (raw = "") ? "{}" : raw
      open := InStr(raw, "{")
      if (open = 0)
        return false
      rest := SubStr(raw, open + 1)
      if (Trim(rest, " `t`r`n") = "")
        raw := SubStr(raw, 1, open) "`n  " replacement "`n" SubStr(raw, open + 1)
      else
        raw := SubStr(raw, 1, open) "`n  " replacement "," SubStr(raw, open + 1)
    }
    return this._WriteAll(raw)
  }

  ; ---- 内部 ----

  static _Key(pluginId, key) {
    return pluginId ":" key
  }

  static _ReadAll() {
    path := this.StorePath()
    if !FileExist(path)
      return ""
    try {
      return FileRead(path, "UTF-8")
    } catch {
      return ""
    }
  }

  static _WriteAll(content) {
    path := this.StorePath()
    try {
      if FileExist(path)
        FileDelete(path)
    } catch {
      return false
    }
    ; 删除失败 (文件仍存在) 时不追加, 避免内容翻倍
    if FileExist(path)
      return false
    try {
      FileAppend(content, path, "UTF-8")
      return true
    } catch {
      return false
    }
  }

  ; 字面量替换: 屏蔽 RegExReplace 替换串里 $ 的子模式语义
  static _RegexReplaceLit(haystack, needle, literal) {
    safe := StrReplace(literal, "$", "$$$$")
    return RegExReplace(haystack, needle, safe)
  }

  static _JsonEscape(s) {
    s := StrReplace(s, "\", "\\")
    s := StrReplace(s, "`"", "\`"")
    s := StrReplace(s, "`n", "\n")
    s := StrReplace(s, "`r", "\r")
    s := StrReplace(s, "`t", "\t")
    return s
  }

  static _Unescape(s) {
    s := StrReplace(s, "\\", "\x01")
    s := StrReplace(s, "\n", "`n")
    s := StrReplace(s, "\r", "`r")
    s := StrReplace(s, "\t", "`t")
    s := StrReplace(s, "\`"", "`"")
    s := StrReplace(s, "\x01", "\")
    return s
  }

  static _RegexEscape(s) {
    for _, ch in ["\", ".", "^", "$", "?", "*", "+", "(", ")", "[", "]", "{", "}", "|"]
      s := StrReplace(s, ch, "\" ch)
    return s
  }
}
