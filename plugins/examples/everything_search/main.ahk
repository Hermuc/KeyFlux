; ============================================================
; everything_search —— Everything 搜索插件 (入口)
;
; 功能: 在命令框 (CapsLock 命令框 / KeyFlux-CommandInput) 内按下「前置触发键」(默认空格) 时,
;   - 取当前选中文字作为检索词 (SelectionContext, 即引擎的选中文本通道);
;   - 调用 Everything 检索本机文件名/文件夹名包含该文字的项目 (官方命令行 es.exe);
;   - 结果以不激活的下拉浮层展示在命令框正下方, ↑↓ 选择、回车在资源管理器中打开;
;   - Everything 未运行时按配置路径自动拉起。
;
; 分层 (每层只依赖下一层, 便于替换与测试):
;   main.ahk                 入口: 读设置 -> 建控制器 -> 注册到命令框拦截点
;   src/EverythingMessages.ahk   文案 (中/英, 复用引擎 SysLangIsChinese)
;   src/EverythingSettings.ahk   设置读写与归一 (plugin-settings.json, 经 api.GetSetting)
;   src/EverythingProviders.ahk  查询通道抽象: es-cli (首选) / gui-launch (降级)
;   src/EverythingSearch.ahk     编排: 拉起 Everything + 通道选择 + 失败重试
;   src/EverythingDropdown.ahk   浮层: 不激活 ListView, 锚定命令框下方
;   src/EverythingSession.ahk    命令框会话状态机 + 控制器 (CommandInputHooks provider)
;
; 设置 (manifest.settings, 在设置面板点插件卡编辑):
;   triggerKey     前置触发键 (默认空格), 在命令框里按它触发搜索
;   everythingPath everything.exe 路径, 未运行时插件用它拉起
;   esPath         es.exe 路径 (可选), 留空则按 everything.exe 同目录/插件 bin/PATH 依次探测
;   limit          下拉列表条数上限 (1-100, 默认 20)
;   ↑ 四个值存 data/plugin-settings.json, **每次命令框会话开始时重读**, 故设置面板保存后
;     无需重启引擎即刻生效 (若改为只在入口读一次, 用户每次改设置都得重启 KeyFlux)。
;
; 依赖引擎侧接口: CommandInputHooks (命令框输入拦截点, bin/lib/core/CommandInputHooks.ahk),
;   SelectionContext (选中文字), APIBridge 的 selection/run/settings 三个命名空间。
; ============================================================

#Include src/EverythingMessages.ahk
#Include src/EverythingSettings.ahk
#Include src/EverythingProviders.ahk
#Include src/EverythingSearch.ahk
#Include src/EverythingDropdown.ahk
#Include src/EverythingSession.ahk

; 控制器引用 (调试/后续扩展用; 保持全局避免被 GC 回收)
global __everythingSearchController := 0

/**
 * 插件入口 (manifest entry.func)。
 * @param api 按 permissions 裁剪的 API 视图 (selection / run / settings)
 */
EverythingSearchMain(api) {
  global __everythingSearchController

  EverythingSettings.Load(api)

  ctrl := EverythingController(api)
  if (!CommandInputHooks.Register(ctrl)) {
    ; 重复加载 (重载引擎时旧实例未注销) 或拦截点缺失 —— 报错交给 PluginManager 记录并隔离
    throw Error("everything_search: 命令框拦截点注册失败")
  }
  __everythingSearchController := ctrl
}
