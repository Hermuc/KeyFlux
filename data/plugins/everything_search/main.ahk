; ============================================================
; everything_search —— Everything 搜索插件 (入口)
;
; 功能: 在命令框 (CapsLock 命令框 / KeyFlux-CommandInput) 内按下「前置触发键」(默认空格) 时,
;   - 取当前选中文字作为检索词 (SelectionContext, 即引擎的选中文本通道);
;   - 调用 Everything 检索本机文件名/文件夹名包含该文字的项目 (官方命令行 es.exe);
;   - 结果由**命令框本体**向下长高展示 (同一个圆角白框: 查询区 + 分隔线 + 结果行),
;     ↑↓ 选择、回车在资源管理器中打开, 鼠标点选/悬停同样可用;
;   - Everything 未运行时按配置路径**静默**拉起 (`-startup`, 后台托盘, 不弹主窗口、不抢焦点)。
;
; 分层 (每层只依赖下一层, 便于替换与测试):
;   main.ahk                 入口: 读设置 -> 建控制器 -> 经 EverythingHost 注册到命令框拦截点
;   src/EverythingHost.ahk       引擎依赖的唯一端口 (CommandInputHooks/CommandDisplay/
;                                CommandImeGuard/SelectionContext/SysLangIsChinese 全在这,
;                                引擎 API 演进只改此文件; 探针经 Impl 整缝替换)
;   src/EverythingMessages.ahk   文案 (中/英, 语言判定经 EverythingHost)
;   src/EverythingSettings.ahk   设置读写与归一 (plugin-settings.json, 经 api.GetSetting)
;   src/EverythingProviders.ahk  查询通道抽象: es-cli (首选) / gui-launch (降级)
;   src/EverythingSearch.ahk     编排: 拉起 Everything + 通道选择 + 失败重试
;   src/EverythingResults.ahk    结果列表视图端口: 只把数据推给命令框 (渲染/几何在命令框内)
;   src/EverythingSession.ahk    命令框会话状态机 + 控制器 (CommandInputHooks provider)
;
;   🔴 中文检索口径 (2026-10-04 定版): 搜索模式**就在命令框本体输入** —— 0x404 激活
;      (框摘 NOACTIVATE 自取前台+焦点), IME 组合窗跟随命令框, 上屏中文经 WM_CHAR 进
;      框内缓冲; 引擎轮询 WM_GETTEXT 读回 = 检索词 (显示与检索词天然一致)。
;      (历史: 闭源原版无读回通道 → 覆盖式 QueryEdit 曾是过渡方案, 随命令框 Rust 化退役;
;       透传+IMM 捕获路线被 24H2 实测否决。Everything pinyin=1 保留为拼音检索增强。)
;
; 设置 (manifest.settings, 在设置面板点插件卡编辑):
;   triggerKey     前置触发键 (默认空格), 在命令框里按它触发搜索
;   everythingPath everything.exe 路径, 未运行时插件用它静默拉起 (-startup)
;   esPath         es.exe 路径 (可选), 留空则按 everything.exe 同目录/插件 bin/PATH 依次探测
;   ↑ 三个值存 data/plugin-settings.json, **每次命令框会话开始时重读**, 故设置面板保存后
;     无需重启引擎即刻生效 (若改为只在入口读一次, 用户每次改设置都得重启 KeyFlux)。
;   结果条数上限 1000 (EverythingSearch.MAX_RESULTS, 经 es.exe -n 落实): 2026-10-05 修订
;     2026-10-04 的「不设上限」—— 英文短词会导出数万条 (实测 "ge" = 72,870 条/9.4MB),
;     逐行 FileExist 拖死引擎线程 (卡死) 且 0x406 载荷超 4MiB 被静默拒绝 (不出列表)。
;
; 依赖引擎侧接口: 经 src/EverythingHost.ahk 端口访问 —— CommandInputHooks (命令框输入
;   拦截点, bin/lib/core/CommandInputHooks.ahk), CommandDisplay / CommandImeGuard /
;   SelectionContext / SysLangIsChinese; APIBridge 的 selection/run/settings 三个命名空间。
; ============================================================

#Include src/EverythingHost.ahk
#Include src/EverythingMessages.ahk
#Include src/EverythingSettings.ahk
#Include src/EverythingProviders.ahk
#Include src/EverythingSearch.ahk
#Include src/EverythingResults.ahk
#Include src/EverythingSession.ahk

/**
 * 插件入口 (manifest entry.func)。
 * @param api 按 permissions 裁剪的 API 视图 (selection / run / settings)
 */
EverythingSearchMain(api) {
  EverythingSettings.Load(api)

  ctrl := EverythingController(api)
  if (!EverythingHost.RegisterCommandHook(ctrl)) {
    ; 重复加载 (重载引擎时旧实例未注销) 或拦截点缺失 —— 报错交给 PluginManager 记录并隔离
    throw Error("everything_search: 命令框拦截点注册失败")
  }
  ; 命令框鼠标交互 (点选/悬停) 回推通道: 只在安装时注册一次, 重载插件仅换目标 (见 InstallNotify)
  EverythingResults.InstallNotify(ctrl)
  ; 存活性: CommandInputHooks.Providers (静态数组) 与 EverythingResults._target (静态类变量)
  ; 已持有 ctrl 引用, 无需额外全局 (曾有的 __everythingSearchController 只写不读, 2026-10-04 移除)
}
