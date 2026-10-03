; ============================================================
; quick_switch —— 快速切换插件 (入口)
;
; 功能: 文件对话框 (打开/另存/文件夹选择) 出现时自动收集最近使用的文件夹,
;   浮层展示候选, 支持一键跳转; 另有「跳转到推荐候选」的手动动作入口。
;
; 分层 (每层只依赖下一层, 便于替换与测试):
;   main.ahk                     入口: 注册动作到 PluginManager.Actions
;   src/HistoryStore.ahk         历史落盘 (data/quickswitch-history.txt, 异步 SetTimer)
;   src/FolderRanker.ahk         候选排序 (最近使用 + 频次加权)
;   src/FolderHistory.ahk        会话内最近目录记忆
;   src/DialogInspector.ahk      对话框分类严判 (打开/另存/文件夹选择/其它)
;   src/QuickSwitchUI.ahk        浮层 GUI (锚定对话框, 不抢焦点)
;   src/QuickSwitch.ahk          编排: 轮询 + 收集 + 自动跳转 + InitQuickSwitch(配置)
;
; 与引擎的接线 (2026-10-01 插件化 P2, 提案 docs/contracts-proposals/quickswitch-pluginization.md):
;   ① 动作通道: 本入口把 QuickSwitchRun 注册为动作 "goto" (P7b 另注册
;      "clear_history" = 清空切换历史, 回归 P6 移除的清空入口); 生成端 callMap[9]
;      (valueID=9 子类型 + actionId) 直接产出 PluginAction("quick_switch", "<actionId>")
;      (PluginManager.ahk 全局薄壳) 经动作注册表间接寻址 ——
;      核心对本插件零静态引用, 本插件被删除/停用时该调用点静默返回 false。
;   ② 配置通道 (2026-10-02 P5): 晚初始化扩展点 ({{ PLUGIN_LATE_INIT }}) 注入无参
;      InitQuickSwitch() 调用 (保持原调用时机, InitKeymap 之后), 配置由插件运行时
;      经 ConfigProvider (data/plugin-settings.json, 契约 §3.8) 自取, 键缺失回落
;      manifest.settings[].default 同源的内置默认值; 本插件被删除/停用时生成端
;      不再注入该行。options.quickSwitch 专用段已 deprecated (仅保留读取兼容,
;      迁移由后端启动时一次性写入 plugin-settings.json)。
;
; 依赖引擎侧接口: DialogInspector 所用的窗口枚举、InputTipWindow (core),
;   CommandInputHooks 不涉及 (本插件不经命令框触发, 触发为对话框轮询)。
; ============================================================

#Include src/HistoryStore.ahk
#Include src/FolderRanker.ahk
#Include src/FolderHistory.ahk
#Include src/DialogInspector.ahk
#Include src/QuickSwitchUI.ahk
#Include src/QuickSwitch.ahk

/**
 * 插件入口 (manifest entry.func)。
 * @param api 按 permissions 裁剪的 API 视图 (window)
 */
QuickSwitchMain(api) {
  ; 动作自描述: 核心薄壳经注册表间接寻址, 本插件缺席时调用点静默降级。
  if (!api.RegisterAction("goto", QuickSwitchRun)) {
    throw Error("quick_switch: 动作注册失败 (goto)")
  }
  ; 清空切换历史 (P7b; manifest provides.actions 声明, 面板动作下拉可见)。
  if (!api.RegisterAction("clear_history", QuickSwitchClearHistory)) {
    throw Error("quick_switch: 动作注册失败 (clear_history)")
  }
}
