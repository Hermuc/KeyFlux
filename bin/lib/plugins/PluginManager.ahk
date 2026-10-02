/**
 * PluginManager —— L1 插件生命周期管理 (docs/CONTRACTS.md §3.7)。
 * 阶段 5 落地框架: 元数据管理 / permissions 词表校验 / 错误隔离 / 卸载。
 *
 * 2026-09-12 生成端接入完成: 生成器扫描 data/plugins 渲染 #Include 行与
 * Register(<manifest 字面量>) / LoadEntry("<id>") 引导 (见 generators/plugins.go),
 * 本类新增 LoadEntry 按清单入口函数拉起插件并注入按权限裁剪的 API 视图。
 */
class PluginManager {
  static Plugins := Map()        ; pluginId -> Map{manifest, enabled}
  static Errors := []            ; 加载期错误 [{pluginId, message}], 供调试 / Oracle
  ; permissions 词表 (冻结, 契约 §4)
  static PERMISSIONS := ["selection", "run", "clipboard", "window", "settings", "events"]

  ; ---- 插件动作注册表 (命令注册表范式; 2026-10-01 插件化 P2 落地) ----
  ; 核心对插件实现**零静态引用**: 生成端只产出 PluginAction("<pluginId>",
  ; "<actionId>") (P7b), 插件在入口内经 APIView.RegisterAction("<actionId>", <fn>) 注册实现。
  ; 🔴 为什么必须经表间接寻址而不能直调: AHK v2 直调未定义函数是**加载期致命错误**
  ;   (2026-10-01 探针实测, 见插件化提案 §0.5) —— 若核心薄壳直调插件符号,
  ;   插件被删除/停用后整个引擎无法启动。
  ;   缺席 => 记日志返回 false, 绝不抛错 (可删除性保证)。
  static Actions := Map()        ; "<pluginId>:<actionId>" -> Func

  /**
   * 插件入口内注册动作实现 (经 APIView.RegisterAction 调用)。
   * @param fn 函数引用 (Func 对象 / 闭包), 无参调用
   * @return true = 注册成功; false = 插件未注册 (拒绝孤儿注册)
   */
  static RegisterAction(pluginId, actionId, fn) {
    if (!this.Plugins.Has(pluginId)) {
      this._recordError(pluginId, "RegisterAction rejected: plugin not registered")
      return false
    }
    this.Actions[pluginId ":" actionId] := fn
    this._log("action registered: " pluginId ":" actionId)
    return true
  }

  /**
   * 调用插件动作 (核心侧唯一入口; 生成端 callMap 与薄壳走这里)。
   * 插件未加载/未注册该动作 => 日志 + false (静默, 不打断调用方)。
   * 动作异常 => 错误隔离 (recordError), 不冒泡 (约束 4)。
   * @return true = 动作已执行 (无论内部结果); false = 动作不可用或执行失败
   */
  static InvokeAction(pluginId, actionId) {
    key := pluginId ":" actionId
    if (!this.Actions.Has(key)) {
      this._log("action not available: " key " (插件未加载或未注册该动作)")
      return false
    }
    fn := this.Actions[key]
    try {
      fn.Call()
    } catch as err {
      this._recordError(pluginId, "action '" actionId "' failed: " err.Message " @ " err.What " line " err.Line)
      return false
    }
    return true
  }

  /**
   * 注册插件。manifest 为生成端渲染的 AHK Map (无需 AHK 侧 JSON 解析)。
   * 必需字段: id, entry。permissions 须在词表内。
   * 重复 id: 记日志, 不覆盖先到者 (约束 4)。
   * @return true = 注册成功; false = 拒绝
   */
  static Register(manifest) {
    if (!IsObject(manifest)) {
      this._recordError("", "Register rejected: manifest not an object")
      return false
    }
    id := manifest.Has("id") ? manifest["id"] : ""
    if (id == "") {
      this._recordError("", "Register rejected: missing 'id'")
      return false
    }
    if (!manifest.Has("entry") || manifest["entry"] == "") {
      this._recordError(id, "Register rejected: missing 'entry'")
      return false
    }
    if (this.Plugins.Has(id)) {
      this._recordError(id, "Register rejected: duplicate plugin (first wins)")
      return false
    }
    perms := manifest.Has("permissions") ? manifest["permissions"] : []
    err := this._validatePermissions(perms)
    if (err != "") {
      this._recordError(id, "Register rejected: " err)
      return false
    }
    this.Plugins[id] := Map("manifest", manifest, "enabled", true)
    this._log("plugin registered: " id)
    ; 阶段 6: 插件生命周期事件 (隔离兜底, 不影响注册结果)
    try EventBus.Publish("plugin_loaded", Map("pluginId", id))
    return true
  }

  /**
   * 按清单入口拉起插件 (生成端在引擎引导期对每个插件各调一次)。
   * 入口函数签名: <func>(api), api 为按权限裁剪的 APIView。
   * 入口异常: plugin_error + 不影响其他插件 (约束 4)。
   * @return true = 入口执行成功; false = 未注册/执行失败
   */
  static LoadEntry(id) {
    plugin := this.Get(id)
    if (plugin == "") {
      this._recordError(id, "LoadEntry rejected: plugin not registered")
      return false
    }
    m := plugin["manifest"]
    e := m.Has("entry") ? m["entry"] : ""
    fnName := (IsObject(e) && e.Has("func") && e["func"] != "") ? e["func"] : "Register"
    api := this.GetAPI(id)
    if (api == "") {
      this._recordError(id, "LoadEntry rejected: api view unavailable")
      return false
    }
    try {
      %fnName%(api)
    } catch as err {
      this._recordError(id, "entry '" fnName "' failed: " err.Message " @ " err.What " line " err.Line)
      return false
    }
    this._log("plugin entry loaded: " id " (" fnName ")")
    return true
  }

  static Unregister(id) {
    if (this.Plugins.Has(id)) {
      this.Plugins.Delete(id)
      return true
    }
    return false
  }

  static Get(id) {
    return this.Plugins.Has(id) ? this.Plugins[id] : ""
  }

  /**
   * 按 manifest.permissions 裁剪的 API 视图 (契约 §3.7)。
   * 未知插件返回空串。config.* 命名空间按插件 ID 作用域隔离。
   */
  static GetAPI(id) {
    plugin := this.Get(id)
    if (plugin == "")
      return ""
    perms := plugin["manifest"].Has("permissions") ? plugin["manifest"]["permissions"] : []
    return APIBridge.Create(perms, id)
  }

  static _validatePermissions(perms) {
    if (!IsObject(perms))
      return "permissions not an array"
    for p in perms {
      if (!this._inList(p, PluginManager.PERMISSIONS))
        return "unknown permission '" p "'"
    }
    return ""
  }

  static _inList(needle, arr) {
    for x in arr
      if (x == needle)
        return true
    return false
  }

  static _recordError(pluginId, msg) {
    this.Errors.Push(Map("pluginId", pluginId, "message", msg))
    this._log(msg)
    ; 阶段 6: 插件错误事件 (隔离兜底)
    try EventBus.Publish("plugin_error", Map("pluginId", pluginId, "message", msg))
  }

  ; 日志 (与 ActionRegistry._log 同策略: 追加写, 失败静默; 目录自建, cwd=部署根)
  static _log(msg) {
    try {
      DirCreate("logs")
      FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") " " msg "`n", "logs\plugin_manager.log")
    }
  }
}

/**
 * 插件动作统一入口 (2026-10-02 P7b 全局薄壳): 生成端 keymap 行直接调用本函数,
 * 转发到动作注册表。核心不知道任何具体插件/动作名 —— 寻址完全由配置 actionId 驱动。
 * 为什么是全局函数而非 PluginManager.PluginAction: 生成端产出的是无限定名调用,
 * 全局命名空间与 AHK v2 函数解析规则一致 (类静态调用亦可, 但全局名最短最稳)。
 * @return true = 动作已执行; false = 动作不可用 (静默, 不打断调用方)
 */
PluginAction(pluginId, actionId) {
  return PluginManager.InvokeAction(pluginId, actionId)
}
