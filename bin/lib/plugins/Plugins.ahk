/**
 * Plugins.ahk —— plugins/ 聚合 include 入口 (与 actions/Actions.ahk 同模式)。
 * 单文件校验 / 接入时只需 include 本文件。
 * 阶段 5 框架: PluginManager / APIBridge / ScriptHost。
 * 2026-09-12 接入运行路径: 生成端注入插件 Include 与 Register/LoadEntry 引导
 * (见 generators/plugins.go), ConfigProvider (§3.8) 随本版落地。
 * 本聚合同时引入 ActionRegistry + IAction 契约类: 插件 main.ahk 在启动期
 * 经 ActionRegistry.Register 注册 IAction (契约 §3.2/§3.4), SelectedAction
 * 分发兜底派发到本注册表 (规则引用 "plugin:<id>:<name>" 动作时可达)。
 */
#Include ..\actions\ActionRegistry.ahk
#Include ..\actions\IAction.ahk
#Include APIBridge.ahk
#Include ConfigProvider.ahk
#Include PluginManager.ahk
#Include ScriptHost.ahk
