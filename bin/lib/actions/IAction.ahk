/**
 * IAction —— 执行型动作契约 (docs/CONTRACTS.md §3.2, 接口已冻结)。
 * 模块化重构阶段 3 新增。内置动作当前仍走编译期直连 (km.Map 直接调用),
 * 不经本契约; 本契约服务于阶段 5 的插件动作 (Type 形如 "plugin:<id>:<name>")
 * 与未来内置动作的收口迁移。
 * 红线: 快路径动作 (重映射/发键/鼠标) 禁止经 IAction.Execute 分派。
 */
class IAction {
  ; 唯一标识。内置: "activateOrRun"/"systemActions"/"windowActions"/"mouseActions"/
  ; "remapKey"/"sendKeys"/"textFeatures"/"builtinFunctions"/"keyfluxActions"
  ; 插件: "plugin:<pluginId>:<actionName>"
  Type := ""

  ; 是否允许出现在缩写命令上下文
  CanRunInAbbr := false

  /**
   * 返回错误信息, 空串 = 合法。注册/插件加载时由 ActionRegistry 调用。
   */
  Validate() {
    return ""
  }

  /**
   * 执行动作。
   * @param ctx ActionContext, 只读数据袋
   */
  Execute(ctx) {
  }

  /**
   * 执行预览 (设置页模拟测试), 对齐现 PreviewAction 语义。
   */
  Preview(ctx) {
    return ""
  }
}

/**
 * ActionContext —— 动作执行上下文 (只读数据袋, 契约 §3.2)。
 *   .selected : String 选中文本/文件路径 (多文件换行分隔), 未获取为 ""
 *   .isFile   : Bool   selected 是否为文件 (CF_HDROP)
 *   .winTitle : String 触发时的窗口条件
 *   .params   : Map    动作参数 (来自配置数据)
 *   .source   : String "hotkey" | "abbr" | "selection" | "plugin"
 */
class ActionContext {
  __New(selected := "", isFile := false, winTitle := "", params := "", source := "hotkey") {
    this.selected := selected
    this.isFile := isFile
    this.winTitle := winTitle
    this.params := (params == "") ? Map() : params
    this.source := source
  }
}
