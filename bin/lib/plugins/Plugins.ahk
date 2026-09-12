/**
 * Plugins.ahk —— plugins/ 聚合 include 入口 (与 actions/Actions.ahk 同模式)。
 * 单文件校验 / 接入时只需 include 本文件。
 * 阶段 5 框架: PluginManager / APIBridge / ScriptHost。
 * 2026-09-12 接入运行路径: 生成端注入插件 Include 与 Register/LoadEntry 引导
 * (见 generators/plugins.go), ConfigProvider (§3.8) 随本版落地。
 */
#Include APIBridge.ahk
#Include ConfigProvider.ahk
#Include PluginManager.ahk
#Include ScriptHost.ahk
