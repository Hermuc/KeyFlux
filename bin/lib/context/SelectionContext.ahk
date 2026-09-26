; ============================================================
; SelectionContext —— 选中文本/文件的统一获取与占位符归一 (契约见 docs/CONTRACTS.md 3.5)
; 合并自原 core/Functions.ahk 的 GetSelectedText 与 rules/SelectedAction.ahk 的 GetSelectedContent:
;   - 剪贴板保存采用 ClipboardAll 完整快照 (GetSelectedContent 语义, 避免丢失文件类剪贴板内容)
;   - 返回对象 {type, content}: type = "file" (CF_HDROP) / "text" / "" (未获取到)
; 占位符约定: {selected} 为规范形; %selected% 解析时自动等价转换 (永久兼容, 文档标注废弃)
; ============================================================

class SelectionContext {
  /**
   * 获取当前选中内容
   * 优先尝试资源管理器选中文件 (剪贴板 CF_HDROP), 否则获取选中文本
   * @param silent 为 true 时获取失败不弹提示 (选中动作路径自带提示, 避免重复)
   * @returns {{type: string, content: string}} type: file / text / ""
   */
  static Get(silent := true) {
    saved := ClipboardAll()
    ; 清空剪贴板
    A_Clipboard := ""

    Send("^c")
    if not (ClipWait(0.4)) {
      A_Clipboard := saved
      if not (silent) {
        Tip(Translation().no_items_selected, -700)
      }
      return {type: "", content: ""}
    }
    ; CF_HDROP: 剪贴板中存在文件列表 (资源管理器复制/剪切文件)
    isFile := DllCall("IsClipboardFormatAvailable", "UInt", 15)
    content := A_Clipboard

    A_Clipboard := saved
    return {type: isFile ? "file" : "text", content: RTrim(content, "`r`n")}
  }

  /**
   * 占位符归一替换: 同时支持 {selected} (规范) 与 %selected% (兼容)
   * @param template 含占位符的模板字符串
   * @param selected 选中内容
   * @returns {string}
   */
  static Normalize(template, selected) {
    return StrReplace(StrReplace(template, "{selected}", selected), "%selected%", selected)
  }

  /**
   * 占位符归一替换 (搜索场景: 先做 URL 编码再替换)
   * @param template 含占位符的 URL 模板
   * @param selected 选中内容
   * @returns {string}
   */
  static NormalizeForSearch(template, selected) {
    return this.Normalize(template, URIEncode(selected))
  }
}
