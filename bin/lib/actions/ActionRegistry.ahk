/**
 * ActionRegistry —— 动作注册表 (docs/CONTRACTS.md §3.4, 接口已冻结)。
 * 模块化重构阶段 3 新增。
 * 红线: 快路径动作 (重映射/发键/鼠标) 禁止经 Execute 入口分派,
 *       它们由 Go 生成端编译为 km.Map 直连调用 (性能红线, 约束 6)。
 * 当前内置动作仍走编译期直连, 本表服务于阶段 5 插件动作与未来的收口迁移。
 */
class ActionRegistry {
  ; Type -> IAction
  static Actions := Map()

  ; 内置动作 Type 词表 (与 Go 生成端 generators/type{1..9}_*.go 一一对应)
  static BuiltinTypes := [
    "activateOrRun",      ; TypeID 1
    "systemActions",      ; TypeID 2
    "windowActions",      ; TypeID 3
    "mouseActions",       ; TypeID 4
    "remapKey",           ; TypeID 5
    "sendKeys",           ; TypeID 6
    "textFeatures",       ; TypeID 7
    "builtinFunctions",   ; TypeID 8
    "keyfluxActions"     ; TypeID 9
  ]

  /**
   * 注册动作。重复 Type: 记日志 + plugin_error 事件, 不覆盖先到者 (约束 4)。
   * @param action IAction 实例
   */
  static Register(action) {
    type := action.Type
    if (type == "") {
      ActionRegistry._log("Register rejected: action with empty Type")
      return false
    }
    if (this.Actions.Has(type)) {
      ActionRegistry._log("Register rejected: duplicate Type '" type "' (first registration wins)")
      ; 阶段 6: 重复注册广播 plugin_error (隔离兜底, 不影响注册结果)
      try EventBus.Publish("plugin_error", Map("pluginId", type, "message", "duplicate action Type (first registration wins)"))
      return false
    }
    if (err := action.Validate()) {
      ActionRegistry._log("Register rejected: Type '" type "' invalid: " err)
      return false
    }
    this.Actions[type] := action
    return true
  }

  /**
   * 插件卸载时调用。内置动作不允许注销。
   */
  static Unregister(type) {
    if !this.Actions.Has(type) {
      return
    }
    for builtin in this.BuiltinTypes {
      if (type == builtin) {
        ActionRegistry._log("Unregister rejected: builtin Type '" type "' cannot be unregistered")
        return
      }
    }
    this.Actions.Delete(type)
  }

  /**
   * @returns IAction 实例, 未注册返回空串
   */
  static Get(type) {
    return this.Actions.Has(type) ? this.Actions[type] : ""
  }

  /**
   * 统一执行入口: try/catch → 日志 + Tip (错误隔离, 约束 4)。
   * 快路径动作禁止走此入口。
   */
  static Execute(type, ctx) {
    action := this.Get(type)
    if (action == "") {
      ActionRegistry._log("Execute rejected: unknown action Type '" type "'")
      return
    }
    try {
      action.Execute(ctx)
    } catch as e {
      ActionRegistry._log("Execute error: Type '" type "' - " e.Message)
      Tip("动作执行出错: " type)
    }
  }

  /**
   * 错误日志 (约束 4)。追加写文件, 失败静默 —— 日志不能反过来破坏运行。
   */
  static _log(msg) {
    try {
      FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") " " msg "`n", "logs\action_registry.log")
    }
  }
}
