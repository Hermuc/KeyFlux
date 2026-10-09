; ============================================================
; SelectedAction —— 单条执行（class SelectedAction 第三段）
;
; 由 Dispatch.ahk 在 class 体内 #Include。8 列 entry 的实际落地动作在此分派。
; ============================================================

  /**
   * 执行一条 entry (8 列契约, action 为生成端展开后的基础动作)
   * textType 特征的专用行为 (open_url 等) 直接作用于选中内容, 不接受命令模板;
   * 特征与行为的合法组合由共享向量 testdata/text_types.json +
   * devtools texttype-conformance 守护 (生成端 = config-ui-reactor 生成器)。
   * 执行前广播 selection_action 慢事件 (薄观察层, 隔离兜底, 不影响动作执行;
   * 方案 D 后事件字段由 schemeId/ruleIndex 调整为 behavior/name/selected)。
   */
  static _Execute(entry, selected) {
    try EventBus.Publish("selection_action", Map("behavior", entry.behavior, "name", entry.name, "selected", selected.content))
    catch as e
      EngineLogWarn("Execute._Execute", e.Message)
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

