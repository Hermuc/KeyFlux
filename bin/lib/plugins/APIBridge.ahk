/**
 * APIBridge —— 插件 API 视图工厂 (docs/CONTRACTS.md §3.7 暴露面清单)。
 * 阶段 5 权限门控骨架; 2026-09-12 生成端接入真实插件, L1 委托实现补全。
 *
 * 权限 → 命名空间映射 (契约 §4 词表 6 项 -> §3.7 命名空间 7 组):
 *   selection -> selection.*          (GetSelectedText / GetSelectedFiles)
 *   clipboard -> selection.*          (文件清单经剪贴板 CF_HDROP, 同族归并)
 *   window    -> window.*
 *   run       -> run.* + send.*       (自动化发送为运行类动作的配套能力)
 *   settings  -> config.*
 *   events    -> events.*
 *   ui.* (Tip / ConfirmBox) 为基础反馈, 不门控, 所有插件可用。
 *
 * 委托来源: selection -> context/SelectionContext; window -> actions/builtins/type3_window
 * (零参函数, 作用于当前活动窗口); run -> ScriptHost / actions type1 ActivateOrRun;
 * ui -> core/Utils Tip + 原生 MsgBox; config -> plugins/ConfigProvider (§3.8)。
 * 未授权命名空间调用 → 记日志 + 返回空值, 不抛出 (错误隔离, 约束 4)。
 */
class APIBridge {
  ; 权限 -> 命名空间列表
  static PermNamespace := Map(
    "selection", ["selection"],
    "clipboard", ["selection"],
    "window", ["window"],
    "run", ["run", "send"],
    "settings", ["config"],
    "events", ["events"]
  )

  /**
   * 给定权限数组与插件 ID (config.* 命名空间按插件作用域隔离), 返回 APIView。
   */
  static Create(permissions, pluginId := "") {
    return APIView(permissions, pluginId)
  }
}

class APIView {
  permissions := []
  _granted := Map()              ; namespace -> true
  _pluginId := ""                ; config.* 设置隔离的作用域

  __New(permissions, pluginId := "") {
    this.permissions := permissions
    this._pluginId := pluginId
    if (IsObject(permissions)) {
      for p in permissions {
        if (APIBridge.PermNamespace.Has(p)) {
          for ns in APIBridge.PermNamespace[p]
            this._granted[ns] := true
        }
      }
    }
  }

  _has(ns) {
    return this._granted.Has(ns)
  }

  _deny(ns) {
    PluginManager._log("APIBridge denied namespace '" ns "' (plugin: " this._pluginId ")")
    return ""
  }

  ; ---- selection.* (委托 context/SelectionContext, 契约 §3.5) ----
  GetSelectedText() {
    if (!this._has("selection"))
      return this._deny("selection")
    sel := SelectionContext.Get(false)
    return (sel.type != "") ? sel.content : ""
  }

  GetSelectedFiles() {
    if (!this._has("selection"))
      return this._deny("selection")
    sel := SelectionContext.Get(false)
    return (sel.type = "file") ? sel.content : ""
  }

  ; ---- window.* (委托 actions/builtins/type3_window; 零参函数作用于当前活动窗口) ----
  ActivateWindow(winTitle) {
    if (!this._has("window"))
      return this._deny("window")
    try {
      WinActivate(winTitle)
      return true
    } catch {
      return false
    }
  }

  SmartCloseWindow() {
    if (!this._has("window"))
      return this._deny("window")
    SmartCloseWindow()
    return true
  }

  LoopRelatedWindows() {
    if (!this._has("window"))
      return this._deny("window")
    LoopRelatedWindows()
    return true
  }

  GoToLastWindow() {
    if (!this._has("window"))
      return this._deny("window")
    GoToLastWindow()
    return true
  }

  MinimizeWindow() {
    if (!this._has("window"))
      return this._deny("window")
    MinimizeWindow()
    return true
  }

  MaximizeWindow() {
    if (!this._has("window"))
      return this._deny("window")
    MaximizeWindow()
    return true
  }

  CenterAndResizeWindow(width, height) {
    if (!this._has("window"))
      return this._deny("window")
    CenterAndResizeWindow(width, height)
    return true
  }

  ToggleWindowTopMost() {
    if (!this._has("window"))
      return this._deny("window")
    ToggleWindowTopMost()
    return true
  }

  MoveWindowToNextMonitor() {
    if (!this._has("window"))
      return this._deny("window")
    MoveWindowToNextMonitor()
    return true
  }

  ; ---- send.* (委托 AHK v2 原生 SendText / Send) ----
  SendText(text) {
    if (!this._has("send"))
      return this._deny("send")
    SendText(text)
    return true
  }

  SendKeys(keys) {
    if (!this._has("send"))
      return this._deny("send")
    Send(keys)
    return true
  }

  ; ---- run.* (委托 ScriptHost / actions type1 ActivateOrRun) ----
  RunProgram(target, args := "", workingDir := "") {
    if (!this._has("run"))
      return this._deny("run")
    try {
      Run('"' target '"' (args != "" ? " " args : ""), workingDir)
      return true
    } catch {
      return false
    }
  }

  RunScript(scriptPath, args := "") {
    if (!this._has("run"))
      return this._deny("run")
    return ScriptHost.Run(scriptPath, args, true)
  }

  ActivateOrRun(winTitle := "", target := "", args := "", workingDir := "") {
    if (!this._has("run"))
      return this._deny("run")
    try {
      ActivateOrRun(winTitle, target, args, workingDir)
      return true
    } catch {
      return false
    }
  }

  ; ---- ui.* (基础反馈, 不门控) ----
  Tip(msg) {
    Tip(msg)
    return true
  }

  ConfirmBox(msg) {
    return MsgBox(msg, "KeyFlux 插件", "OKCancel Icon?") = "OK"
  }

  ; ---- config.* (委托 ConfigProvider, 契约 §3.8; 按插件 ID 作用域隔离) ----
  GetSetting(key) {
    if (!this._has("config"))
      return this._deny("config")
    return (this._pluginId != "") ? ConfigProvider.Get(this._pluginId, key) : ""
  }

  SetSetting(key, value) {
    if (!this._has("config"))
      return this._deny("config")
    return (this._pluginId != "") ? ConfigProvider.Set(this._pluginId, key, value) : false
  }

  ; ---- events.* (阶段 6 已桥接 EventBus) ----
  Subscribe(eventType, callback) {
    if (!this._has("events"))
      return this._deny("events")
    return EventBus.Subscribe(eventType, callback)
  }

  Unsubscribe(subId) {
    if (!this._has("events"))
      return this._deny("events")
    EventBus.Unsubscribe(subId)
    return true
  }
}
