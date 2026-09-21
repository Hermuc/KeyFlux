# KeyFlux 架构契约文档(方案 D 定稿)

> 本文档是模块化重构的**唯一权威契约**。所有接口先在此定义并冻结,再迁移实现。
> 状态:**骨架版(阶段 0)** — 接口签名已定,实现细节随阶段推进补充。
> 分支:`dev`(原名 refactor/modularize, 重构完成后更名, 继续承载新功能开发)

---

## 0. 总原则

1. **数据进配置,代码进插件**:`config.json` 只存纯数据(热键→动作ID→参数);
   一切代码载荷(自定义函数、AHK 表达式)迁入插件文件(真 `.ahk`,编译期 Include)。
2. **快路径红线**:按键重映射 / 发送按键 / 鼠标动作只在启动期做原生注册,
   永不进入运行时查表,永不经过事件总线。
3. **同接口双挂载**:内置能力与第三方插件实现同一组契约;内置为静态挂载
   (编译期),第三方为动态挂载(运行时)。

## 1. 约束清单(7 条,不可协商)

| # | 约束 | 验证方式 |
|---|---|---|
| 1 | 零行为变更 | 基线真源 = 部署目录 `MyKeymap-2.0-beta33\data\config.json`(已同步仓库);阶段 1 生成脚本与基线字节级一致(忽略 `#Include` 行);阶段 2+ 用注册计划 Oracle diff + `docs/baseline/BEHAVIOR_CHECKLIST.md` |
| 2 | 接口先行 | 契约先写入本文档并冻结,再写实现 |
| 3 | 依赖倒置 | 上层只依赖 Registry/Bus/Provider 抽象;`fileGroupExts` 等表收敛单源 |
| 4 | 错误隔离 + 阻塞隔离 | L1 插件 try/catch + 日志 + 总线广播;回调 ≤50ms;长任务走 ScriptHost 子进程 |
| 5 | 配置兼容 | `config.json` 加 `schemaVersion`(缺省=1);插件设置独立存 `data/plugin-settings.json`;`%selected%` 永久兼容 |
| 6 | 性能红线 | 见总原则 2 |
| 7 | 数据/代码分离 | 新功能禁止向 `config.json` 写入 AHK 表达式 |

## 2. 目录布局(目标态)

```
bin/lib/
├── core/        IKeyEventBus.ahk / EventBus.ahk / ModeManager.ahk(现 KeymapManager) ——
│                  **总线已落地并接入(阶段 6)**: 进程内同步实现 + 5 处发布点,
│                  零订阅者时行为不变; IKeyEventBus 接口冻结
├── context/     SelectionContext.ahk
├── actions/     ActionRegistry.ahk / IAction.ahk / IRegistration.ahk —— **已完成(阶段 3)**:
│                  三件套按 §3.2-3.4 冻结接口落地(含 ActionContext), 冒烟测试 8 项全过;
│                  仅定义不接入运行路径, 模板与生成产物不变 (快路径仍编译期直连)
│   └── builtins/  8 类内置动作各一文件 —— **已完成(阶段 3)**:`Actions.ahk` 的 42 个函数已按
│                  TypeID 拆入 `builtins/type{1,2,3,4,6,7,8,9}_*.ahk`(函数体逐行搬运,
│                  行多重集校验通过);`Actions.ahk` 保留为聚合 include 入口,模板与生成产物不变
│                  (TypeID5 remapKey 由 `KeymapManager` 直接重映射, 不是 "内置动作" 文件,
│                   故 `builtins/` 无 type5_*.ahk; 生成侧覆盖见 `golden_test.go` 覆盖矩阵)
├── rules/       SelectionEngine.ahk(只匹配,不执行)
├── commands/    CommandResolver.ahk / FuzzyStrategy.ahk —— **CommandResolver 已完成(阶段 4)**:
│                  缩写 switch 换为运行时注册表 (闭包即待执行数据), FuzzyStrategy 留桩
├── plugins/     PluginManager.ahk / APIBridge.ahk / ScriptHost.ahk / Plugins.ahk(聚合入口) ——
│                  **框架已落地(阶段 5)**: 权限词表校验/错误隔离/权限裁剪 API 视图/子进程长任务,
│                  冒烟 23 项全过; 仅定义不接入, Everything 插件推迟 (见 §3.7 注记)
└── compat/      LegacyLoader.ahk(消费 Go 编译的遗留代码载荷)—— **占位已建(阶段 3)**:
│                  仅定义不接入; 接入点见文件头注释 (阶段 5 收口 / 阶段 6 事件广播已具备: EventBus)
data/
├── config.json / plugins/<id>/ / plugin-settings.json
config-server/internal/script/generators/   (actionMap 按类型拆分)
config-server/internal/server/              (HTTP handler + 路由注册 + DTO 层)
├── server.go      gin 引擎装配、15 条路由注册、端口监听回退、headless 端口通告、openBrowser、PanicHandler
├── handlers.go    GetConfigHandler / SaveConfigHandler / GetShortcutsHandler / ServerCommandHandler / syncStartupFromRegistry
├── selectedaction.go 选中动作单键分发 API (TestSelectedActionHandler; 旧 action-schemes CRUD 六路由随
│                  方案 D 重构移除, 存量配置经 ParseConfig 读时一次性迁移)
├── behaviors.go   行为包 REST API (列表 / 新建 / 更新 / 删除 / 应用, 5 handler)
├── plugins.go     插件 REST API (列表 / 导入 zip / 删除, 3 handler)
└── dto.go         Config 及全部嵌套结构的 DTO 类型 + 双向映射 (model→dto 供 GET, dto→model 供 PUT)
config-server/internal/proc/                (共享子进程启动工具)
└── proc.go        ExecCmd / FallbackExecCmd (CREATE_BREAKAWAY_FROM_JOB + explorer 中转降级)
config-server/cmd/settings/                 (仅保留入口与模式判断)
└── main.go        main() CLI 分发 + debug/headless 判断 + 代码雨编排 + hideMatrix + server.Run() 调用
config-ui-avalonia/Resources/i18n.json      (双语文案真源, 385 键, UTF-8 无 BOM; 构建产物请勿手改)
bin/ui/Resources/i18n.json                  (松散部署物, 由 csproj Content 项产出, 随 robocopy /MIR 同步到部署目录)
scripts/                                    (维护者脚本, 不随发布包出货, 与出货的 tools/ 区分)
├── build_tools.go     发布前 AHK 版本闸 (checkForAHKUpdate) + 回写分享链接 (updateShareLink)
└── lanzou_client.py   蓝奏云上传 (make uploadLanZou 调用)
```

**i18n.json 契约**：该文件属构建产物, 请勿手改 `bin/ui/Resources/i18n.json` (真源在 `config-ui-avalonia/Resources/i18n.json`)。
缺失/损坏时 UI 降级为回显裸键号 (不崩溃、不损坏 config.json、不影响热键运行时);
恢复 = 重跑 `make buildClientAvalonia` + `make deploy`。
Makefile 已内置 SHA256 断言: publish 后自动校验产出与源一致, 不一致即 exit 1。

## 3. 核心接口

### 3.1 IKeyEventBus — 按键/模式事件总线(core/)

```ahk
; ============================================================
; IKeyEventBus —— 按键事件总线抽象(薄观察层)。
; ⚠️ 此接口后续可用 Rust FFI 实现:
;    Rust 守护进程接管键盘捕获与事件分发,通过本地 IPC / FFI 回调
;    向 AHK 层投递事件;AHK 侧订阅方代码无需任何改动即可完成引擎替换。
;    Rust 守护进程同时充当 L2 插件通道宿主(见 3.7)。
; 红线:总线仅承载慢事件,绝不参与快路径(重映射/发键/鼠标)决策链。
; ============================================================
class IKeyEventBus {
  ; eventType 词表(冻结):
  ;   "mode_enter" / "mode_exit"     模式进栈/出栈, eventData: {name}
  ;   "abbr_submit"                  缩写命令提交,  eventData: {source: "caps"|"semi", command, matched, fuzzy: bool}
  ;   "selection_action"             选中动作触发,  eventData: {behavior, name, selected} (2026-09 方案 D 起; 旧 schemeId/ruleIndex 已废弃)
  ;   "plugin_loaded" / "plugin_error"  插件生命周期, eventData: {pluginId, message?}
  Subscribe(eventType, callback) => 0   ; 返回订阅 ID
  Unsubscribe(subId) {}
  Publish(eventType, eventData) {}
}
```

**实现约定**:当前阶段 `EventBus` 为进程内同步实现;`Publish` 内对每个订阅
回调独立 `try/catch`,单个回调异常不影响其他订阅者(约束 4)。

**已完成(阶段 6)**:`core/EventBus.ahk` 按冻结接口落地(胖箭头改写为普通方法体),
发布点接入 5 处:① `KeymapManager.Activate` 进/出栈 → `mode_enter`/`mode_exit`;
② `KeymapManager._lock`/`Unlock` 锁定切换 → 同两事件;③ `CommandResolver.Resolve`
提交即报 `abbr_submit`(命中与否都报, `source` 由 scope 映射 "caps"|"semi");
④ `SelectedAction._Execute` 规则命中后执行前 → `selection_action` (方案 D: Trigger 命中 entry 后统一经 _Execute 发布, 载荷 {behavior, name, selected});
⑤ `PluginManager` 注册成功/拒绝 → `plugin_loaded`/`plugin_error`(`ActionRegistry`
重复注册同报 `plugin_error`)。所有发布点均 `try` 包裹 + 总线自身静默兜底,
零订阅者时为空遍历, 行为不变。模板新增 2 行 include, 生成脚本仅多此 2 行(已验证),
`/Validate` 全脚本 exit=0, Oracle 回归 PASS, 冒烟 13 项全过(订阅/词表拒绝/
回调隔离/退订/各发布点载荷字段/APIBridge events 桥接与拒绝)。

### 3.2 IAction — 执行型动作契约(actions/)

```ahk
class IAction {
  Type := ""            ; 唯一标识。内置: "activateOrRun"/"systemActions"/...
                        ; 插件: "plugin:<pluginId>:<actionName>"
  CanRunInAbbr := false ; 是否允许出现在缩写命令上下文
  Validate() => ""      ; 返回错误信息, 空串 = 合法(注册/插件加载时调用)
  Execute(ctx) {}       ; ctx: ActionContext
  Preview(ctx) => ""    ; 执行预览(设置页模拟测试, 对齐现 PreviewAction 语义)
}

; ActionContext — 动作执行上下文(只读数据袋)
;   .selected   : String  选中文本/文件路径(多文件换行分隔), 未获取为 ""
;   .isFile     : Bool    selected 是否为文件(CF_HDROP)
;   .winTitle   : String  触发时的窗口条件
;   .params     : Map     动作参数(来自配置数据)
;   .source     : String  "hotkey" | "abbr" | "selection" | "plugin"
```

### 3.3 IRegistration — 声明型契约(覆盖快路径)

```ahk
class IRegistration {
  ; 向指定 Keymap 声明绑定。重映射必须声明为原生注册(由 AHK 钩子直接
  ; 处理),不得包装为回调;模式结构(子模式/锁定/单击动作)同理。
  Declare(km) {}
  ; 声明内容类别(冻结): "hotkey-action" | "remap" | "submode" | "singlePress"
}
```

### 3.4 ActionRegistry — 动作注册表

```ahk
class ActionRegistry {
  static Actions := Map()              ; Type -> IAction
  static Register(action) {}           ; 重复 Type: 记日志 + plugin_error, 不覆盖先到者
  static Unregister(type) {}           ; 插件卸载时调用
  static Get(type) => actionOrBlank
  static Execute(type, ctx) {}         ; 统一 try/catch → 日志 + Tip; 快路径动作禁止走此入口
}
```

**已完成(阶段 3)**:§3.2 `IAction`+`ActionContext`、§3.3 `IRegistration`、§3.4 `ActionRegistry`
已按冻结接口落地于 `bin/lib/actions/`(胖箭头方法改写为普通方法体, 因部署版 AHK 不支持;
接口形状不变)。冒烟测试覆盖: 注册/重复拒绝(先到者胜)/空 Type 拒绝/未知 Get 返回空串/
Execute 传 ctx/异常隔离不抛出/未知 Type 不抛出/内置类型拒绝注销——全部通过。
`compat/LegacyLoader.ahk` 占位已建。四者均未被模板或生成脚本引用, 零行为变更。

### 3.5 SelectionContext — 选中文本统一入口(context/)

```ahk
class SelectionContext {
  static Get(&isFile := false) => ""   ; 合并现 GetSelectedText + GetSelectedContent
                                       ; 能力: ^c 获取 + CF_HDROP 文件判断 + 剪贴板恢复
  static Normalize(template, selected) => ""
                                       ; 占位符归一: {selected} 为规范形;
                                       ; %selected% 解析时自动等价转换(永久兼容, 文档标废弃)
}
```

### 3.6 CommandResolver — 缩写命令解析(commands/)

```ahk
class CommandResolver {
  static Table := Map()                ; "scope:command" -> [CommandStep, ...] 运行时表, 取代生成 switch;
                                       ; scope 分域 ("capslock" | "semicolon"):
                                       ; 两表命令名可重复, 必须隔离 (阶段 4 修订)
  static Strategy := ""                ; 解析策略对象, 可插拔 (阶段 4 留桩; 空串 = 未设置,
                                       ; 部署版 AHK v2.0.19 无 null 关键字)
  static Register(scope, command, steps) {}
                                       ; 由生成脚本在 InitKeymap 内调用 (路径变量之后);
                                       ; 重复命令: 记日志, 不覆盖先到者 (约束 4)
  static Resolve(scope, command, hook := "") {}
  ; 策略契约(冻结): 精确命中 → 按 steps 顺序执行;
  ;   带窗口组守卫的 step: 守卫命中 → 执行并立即返回 (对齐旧 switch 语义);
  ;   未命中 → 子序列匹配 → 编辑距离≤1 → 候选集;
  ;   唯一候选 → 静默执行 + Tip 提示实际命令; 多候选 → 仅 Tip 列出, 不执行。
  ; 当前阶段(阶段 4)只实现精确匹配, 未命中静默无操作, FuzzyStrategy 留桩。
}

; 步骤数据: 闭包即"待执行数据"。生成端把原 switch case 体编译为闭包,
; 保留任意表达式 (含遗留代码载荷/路径变量引用), 参数在执行时求值,
; {selected} 等运行时替换语义不变。
class CommandStep {
  call := ""                           ; Funcref, 无参闭包 () => <原 case 体语句>
  winTitle := ""                       ; 窗口组守卫 (仅 WindowGroupID != 0 的动作有)
  conditionType := 0                   ; 0 = 无守卫; 1-5 语义同 matchWinTitleCondition
}
```

**已完成(阶段 4)**:`bin/lib/commands/CommandResolver.ahk` 实现上述契约(含 `DumpAbbr(outFile)`
Oracle 导出: 命令按字典序, 同配置多次调用结果一致);生成端 `generators/abbr_registry.go`
的 `AbbrRegistryCode` 与 `AbbrToCode` 逐条对齐, 模板中 `ExecCapslockAbbr`/`ExecSemicolonAbbr`
改为委托 `CommandResolver.Resolve`。验证: 新旧产物 diff 审查(35 case → 35 闭包逐字一致) +
语义冒烟 7 项 + 整脚本 `/Validate` + **Oracle diff PASS**(capslock 命令集 35/35 与步骤数相等,
分号段双空)。
阶段 4 实测坑两条(影响任何独立测试脚本): ① AHK v2 的 `>` 对两个字符串按**数字**比较
(非数字串直接报错), 字典序排序必须用 `StrCompare`; ② `type9_keyflux.ahk` 调用仅存在于
生成脚本的 `ExecCapslockAbbr`, AHK v2 默认 #Warn 在**加载期**弹警告对话框阻塞进程,
`/ErrorStdOut` 无法抑制——独立测试脚本头部须加 `#Warn All, Off`。

### 3.7 PluginManager / APIBridge / ScriptHost(plugins/)

```ahk
class PluginManager {
  static Plugins := Map()              ; pluginId -> {manifest, actions, enabled}
  static ScanAndLoad(dir) {}           ; 扫描→清单校验→权限检查→加载;
                                       ; 单插件失败: 日志 + Publish("plugin_error"), 不影响其他
  static Unload(id) {}
  static GetAPI(id) => api             ; 按 manifest.permissions 裁剪的 APIBridge 视图
}

class ScriptHost {
  static Run(scriptPath, args := "", inBackground := true) ; 子进程执行长任务
                                       ; (承接现 RunScriptWithSelected 的执行方式)
}
```

**APIBridge 暴露面清单**(L1 直接调用;L2 映射为同名 JSON-RPC method,
协议已冻结(阶段 6),宿主 = 未来 Rust 守护进程,当前不落地):

**L2 协议文本(冻结)**:
- 传输:子进程 **stdio**, 一行一条消息(`\n` 分隔, UTF-8, 无长度前缀);
- 格式:JSON-RPC 2.0;宿主 → 守护进程 = request(带 `id`),守护进程 → 宿主 = response;
- method 命名:与 APIBridge 命名空间同名(`selection.GetSelectedText` 等);
- 事件推送:守护进程 → 宿主用 notification `events.notify`,
  `params: {type: eventType, data: eventData}`(eventType 词表见 §3.1);
- 错误码:保留 JSON-RPC 标准段(-32700/-32600/-32601/-32602/-32603);
  `-32001` = 权限拒绝(对应 APIBridge 未授权命名空间);
- 鉴权:无(本机单用户进程间通道);超时:调用方自定,宿主不设默认。

| 命名空间 | 方法 | 来源 |
|---|---|---|
| `selection.*` | GetSelectedText / GetSelectedFiles | SelectionContext |
| `window.*` | ActivateWindow / SmartCloseWindow / LoopRelatedWindows / GoToLastWindow / MinimizeWindow / MaximizeWindow / CenterAndResizeWindow / ToggleWindowTopMost / MoveWindowToNextMonitor | Actions.ahk / Functions.ahk |
| `send.*` | SendText / SendKeys | 原生 Send 封装 |
| `run.*` | RunProgram / RunScript / ActivateOrRun(**默认子进程化**) | Actions.ahk |
| `ui.*` | Tip / ConfirmBox | Utils.ahk |
| `events.*` | Subscribe / Unsubscribe | 桥接 IKeyEventBus |
| `config.*` | GetSetting / SetSetting(读写 plugin-settings.json) | ConfigProvider |

**已完成(阶段 5)**:`bin/lib/plugins/` 落地 `PluginManager` / `APIBridge` / `ScriptHost`
三件套框架(含 permissions 词表校验、错误隔离、按权限裁剪的 API 视图、子进程长任务),
冒烟测试覆盖注册/重复拒绝/权限非法拒绝/未知 Get/按权限裁剪/未授权调用不抛出/卸载。
**范围调整(用户裁定)**:① AHK v2 无运行时动态加载能力, L1 插件 `main.ahk` 须经编译期
`#Include`——生成端接入(扫描 `data/plugins` 生成 Include 行 + 调用 `Register(api)`)留待后续;
② 首个官方插件 `everything-search`(含 `everythingPath` 设置项与自动启动逻辑)**推迟到全部阶段完成后**再做;
③ `ConfigProvider` 的 JSON 解析(`plugin-settings.json` 读写)随首个真实插件落地。
本阶段仅定义不接入运行路径, 模板与生成产物不变(零行为变更)。

**已完成(2026-09-12, 生成端接入 + L1 API 实现, 即"插件市场可用"里程碑)**:
`generators/plugins.go` 扫描 `<config.json 同级>/plugins` 渲染 `#Include` 行与
`PluginManager.Register(<manifest Map 字面量>)` + `LoadEntry("<id>")` 引导
(模板拼接约定 `{{ PLUGIN_INCLUDES }}` / `{{ PLUGIN_BOOTSTRAP }}` 行尾注入;
零插件时生成产物与历史字节一致)。入口文件存在性与路径逃逸(../、盘符)在
**生成期**拦截 —— AHK `#Include` 指向缺失文件会拖垮整个脚本加载。
APIBridge 七命名空间 L1 委托全部实现(selection→SelectionContext; window→type3_window
零参函数; run→ScriptHost/ActivateOrRun; send→原生 SendText/Send; ui→Tip/MsgBox;
config→ConfigProvider; events→EventBus 既有), config.* 按插件 ID 作用域隔离。
`PluginManager.LoadEntry(id)` 以动态调用拉起 `<entry.func>(api)`, 异常隔离为 plugin_error。
实机验证: `plugins/examples/sample_greeter` 经导入落盘 → 引擎重启 →
`plugin registered` / `plugin entry loaded` 全链 (logs/plugin_manager.log)。
**已完成(2026-09-12 下午, 三项遗留收口)**:
① 启停持久化闭环 —— 生成端消费 `config.options.plugins.disabled`
(generators/plugins.go: 停用插件不注入不注册, 落注释; UI 开关→SaveAsync→PUT /config
既有链路承接持久化, 配置 mtime 变化经 NeedsRegenerate 自动触发重生成);
② 声明式 contributes —— 插件目录 `behaviors/` 子目录可贡献 §3.9 同格式行为包
(behaviors.LoadCatalog 增 extraDirs 变参来源, Source 标记 plugin<N>, 排在 builtin/user
之后且同 ID 先到者胜/冲突注记去重; 插件贡献包非 user 来源 → 行为库不可编辑删除,
随插件装删);
③ IAction 动作接线 —— ValidateManifest 放行 `plugin:<pluginId>:<actionName>` 形态的
entry.action 引用 (pluginActionPattern); 运行时 SelectedAction._Execute 增 default 分发:
未内置动作委托 ActionRegistry.Execute (插件启动期注册的 IAction 由此可达; 未注册动作
日志+静默)。Oracle/DumpPlan 仍不建模插件动作 (plan 侧 plugin: 前缀原样透传, 已测试)。
实机验证: sample_greeter 升级版 (IAction 注册 + behaviors/sample_timestamp 贡献包) →
重启自动重生成 → GET /api/behaviors 可见贡献包 → 临时配置规则引用生成
`action: "plugin:sample_greeter:timestamp"` 分发行。
**仍遗留**: 插件启用/停用状态在行为库 UI 的展示细化 (插件贡献包当前显示为内置标签)、
声明式 contributes 的 UI 创作辅助。

### 3.8 ConfigProvider — 配置读取收口

```ahk
class ConfigProvider {
  static Load(path) => configMap       ; JSON 解析(AHK 无内置 JSON 库, 用随附 JSON.ahk)
  static SchemaVersion(cfg) => 1       ; 缺省视为 1; 迁移钩子按版本号链式执行
  static GetPluginSetting(pluginId, key) / SetPluginSetting(...)
                                       ; 独立存储于 data/plugin-settings.json
}
```

**已落地(2026-09-12, v1 最小实现)**:`plugins/ConfigProvider.ahk`, 扁平字符串键值
`{"<pluginId>:<key>": "<value>"}`(写入端 JSON 转义, 值仅字符串; 结构化值由插件自行
编码)。Load/SchemaVersion 及 JSON.ahk 全量解析器仍随首个需要结构化设置的插件落地。

**已落地(2026-09-18, 声明式设置 + 设置界面写入端, 随首个真实插件 everything_search)**:

写入端在 **Go 后端**, AHK 侧只读 —— 单一写入者, 无跨进程写竞争:

| 环节 | 落点 |
|---|---|
| 声明 | `plugin.json` 的 `settings[]`(key/type/label/labelEn/default/filter/hint/hintEn/min/max/maxLength), 校验在 `internal/plugins.ValidateManifest`(key 命名空间 `^[A-Za-z][A-Za-z0-9_]{0,31}$`、type 词表 `char/text/number/file`、label 非空、key 唯一、声明 settings 必须申请 `settings` 权限、默认值自身必须合法) |
| 读取 | `GET /api/plugins/:id/settings` → `{id, settings[], values{}}`; values = 声明默认值合并已存值(未存过的键回落 default) |
| 写入 | `PUT /api/plugins/:id/settings`, body `{values:{}}`; 逐键按同一份声明校验(`ValidateSettingValue`: 长度/整数/闭区间/可打印字符/NUL), **整单拒绝**不做部分写入; 空串 = 删除该键(回落默认值) |
| 存储 | `internal/plugins.SettingsStore` → `../data/plugin-settings.json`, 与 `ConfigProvider.ahk` 的 `A_ScriptDir\..\data\plugin-settings.json` **同一文件**。两个必须守住的细节: ①**关闭 HTML 转义**(`SetEscapeHTML(false)`)—— AHK 的 `_Unescape` 只认 `\\ \" \n \r \t` 五种序列, Go 默认的 `\uXXXX` 会被原样留在值里; ②**原子落盘**(临时文件 + rename)—— AHK 每次触发都全量读该文件, 非原子写会让它读到半截 JSON 而丢掉全部设置。另: 未提及的键(含其它插件与外人不按约定写的非字符串条目)原样保留 |
| 界面 | `PluginSettingsDialogWindow` 按 `settings[]` 渲染四类编辑器(char/file+浏览/number/text), 卡片可点击条件 = `IsBuiltin || settings 非空`  |
| 生效 | **免重启**: 插件在每次会话/触发开始时重读该文件(`EverythingSettings.Load` 有变化才让通道探测缓存失效), 故面板保存后无需经 `PUT /config` 重启引擎 |

⚠ 卸载插件**不清** `plugin-settings.json`: 卸载重装(或同名重写)时用户填过的路径通常还想接着用,
残留键无副作用(读不到 manifest 就没人读它)。

### 3.9 行为包 (Behavior Pack) —— 选中动作「行为库」契约 (2026-09-05 冻结)

行为 = 自描述目录包: `<id>/behavior.json` (+ 二期可选脚本)。两个来源: **内置包**随软件分发
(settings.exe 同级 `behaviors/`, 仓库 `bin/behaviors/` 入库), **用户包**在 config.json 同级
`behaviors/`(即 `../data/behaviors`), 经设置界面增删。

```json
{
  "id": "ps_edit", "name": "PS 编辑图片", "nameEn": "PS Edit", "version": "1.0.0",
  "specVersion": 1, "description": "用 Photoshop 打开选中图片",
  "appliesTo": [
    {"type": "fileExt", "exts": ["jpg", "png"]},
    {"type": "textType", "value": "url", "default": true}
  ],
  "entry": {"kind": "builtin", "action": "run", "params": {"actionValue": "Photoshop.exe \"%selected%\"", "workingDir": ""}}
}
```

- **ID 规范**: `^[a-z][a-z0-9_]{0,31}$`; 内置 11 个基础动作 ID (`open_url`…`copy`) 为保留
  命名空间; 目录名必须等于 id。
- **前提语义**: 不存分组名 (分组仅是设置界面快捷填入模板, 与规则编辑页同款结论);
  fileExt 用显式后缀集 (`"*"`=任意文件)。
- **规则引用**: `ActionRule.ActionType` 的取值语义 = 行为 ID。内置 ID 直通 (存量配置零迁移,
  golden/DumpPlan 对既有配置逐字节稳定); 用户包 builtin entry 渲染期由
  `ResolveRuleAction` 展开为基础动作 (规则 ActionValue/WorkingDir 非空优先, 空值补包默认)。
- **保存校验统一为覆盖检查**: 规则引用的行为必须存在且 appliesTo 覆盖规则前提 —— 取代旧
  `textTypeActions` 静态表, 并补上 fileExt 无后端校验缺口; 顺带修复旧校验「第一条 textType
  规则即短路」缺陷。
- **删除约束 (ValidateDelete)**: 内置包不可删; 被规则引用不可删; 删除后若存在「仍被引用
  但无任何行为覆盖」的前提值 (空前提桶) → 拒绝并列出断档值; 无人引用的前提值随包消失。
- **API**: `GET /api/behaviors` (builtin+user+errors) / `POST`(创建) / `PUT :id`(更新) /
  `DELETE :id`(校验后删除) / `POST /api/behaviors/apply`(显式重启引擎生效 —— 行为变更
  不自动重启, 与方案 CRUD 的「保存即重启」刻意区分, 避免连续增删的重启风暴)。
- **一期边界**: 仅 builtin entry; script entry (编译期 Include + BehaviorRegistry,
  `BehaviorMain(ctx)` 对齐 §3.2 ActionContext) 与 zip 导入导出为二期, 格式预留。
- 实现: `internal/behaviors/` (加载/覆盖/校验), `internal/server/behaviors.go` (API),
  `internal/script/generators` 注入 `BehaviorCatalog` (渲染 + plan 镜像同步展开)。

### 3.10 CommandInputHooks —— 命令框输入期拦截点(provider 契约)

命令框本体是上游预编译二进制(只做按键镜像), 真正的键盘捕获在主进程的 InputHook。任何
「输入期间的新交互」(插件按下前置键唤起下拉列表等)都只能挂在 InputHook 的 OnChar/OnKeyDown 上,
本类即那一层稳定扩展点(2026-09-18 引入, 随 everything_search 首个消费者落地)。

```ahk
class CommandInputHooks {
  static Register(provider) => bool     ; 幂等: 已注册过返回 false
  static Unregister(provider) => bool
  static BeginSession() / EndSession()  ; 命令框显示前 / 输入结束后
  static DispatchChar(ih, char, scope) => bool   ; true = 已消费
  static DispatchKey(ih, vk, sc, scope) => bool  ; true = 已消费
  static ActivateBackend() => bool      ; 把前台切回会话开始时的窗口(取选中文字用)
}
```

provider 为实现以下方法的对象(类实例), **全部可选, 缺失即视为不处理**(`HasProp` 守卫, 不抛不记):

| 方法 | 何时被调 |
|---|---|
| `OnSessionBegin()` | `BeginSession()`, 命令框显示前(设置热重载的挂点) |
| `OnSessionEnd()` | `EndSession()`, 输入结束(含命中/Esc/超时) |
| `OnChar(ih, char, scope) -> bool` | 每个输入字符, 返回 true = 消费(引擎不再投递字符、不做缩写模糊匹配) |
| `OnKey(ih, vk, sc, scope) -> bool` | 每个被 `KeyOpt(..., "N")` 通知的按键(退格/↑/↓/回车) |

**两条硬约束(违反即静默失效, `/Validate` 与 lint 都查不出)**:

1. 🔴 **分发时 `this` 必须由引擎显式传入**。AHK v2 的 `obj.Method` 取到的是**未绑定**的函数对象
   (`this` 只是普通首参, 取值前无值 —— 与 Python/JS 的 bound method 相反), 故 `fn := p.%name%`
   + `fn.Call(args*)` 会把首个实参顶成 `this` 并令末位实参缺失 → 每次回调在调用边界抛
   `Missing a required parameter.`。**必须**写 `p.%name%(args*)` 或 `ObjBindMethod(p, name).Call(args*)`。
   (2026-09-19 实测缺陷: 旧写法令 provider 一次都没执行, 症状 = 命令框按触发键毫无反应,
   而日志只有一行被 catch 吞掉的噪声, 与「插件没注册」无法区分。)
2. provider 抛异常**只记日志并视为未消费**(错误隔离, 与 `PluginManager` 同策略), 不拖垮命令框,
   且**继续询问后续 provider**; 前序 provider 返回 true 则短路。

回归守门人: `tools/command_input_hooks_test.ahk` + `make check-hooks`(已挂入 `make check`)。
探针逐字 `#Include` 实现真身而非另写桩, 并把工作目录隔离到 `%TEMP%`(被测 `_log` 写相对路径
`logs\command_input_hooks.log`)。旧写法下 13 项红, 新写法下 23 项全绿; 2026-09-19 扩入
`CommandDisplay` 白名单断言后为 49 项全绿。

### 3.11 CommandDisplay —— 命令框回显收口 / 八角 keycap 抑制 (2026-09-19 冻结)

**背景: 命令框 exe 的绘制约束 (反汇编 + 实测结论, 不可绕过的物理事实)**

命令框本体是上游预编译二进制(`bin/KeyFlux-CommandInput.exe`, 无源码)。经 PE 解析与
`.pdata` 函数表定位 `SampleWindow::DrawKeys`, 确认:

1. **只有 `a-zA-Z0-9` 会套八角 keycap** —— 白名单为 exe 内 UTF-16 字面量(VA `0x1daa0`);
   中文/全角/其他符号**不套**, 以普通字形直绘。
   **字体来源 = `bin/font/font.ttf` 私有字体集合**(非系统回退): exe 用相对路径
   `font\font.ttf` 建 `IDWriteFontCollection`, 再按该 ttf 的 `name` 表族名
   `CreateTextFormat(..., DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STYLE_NORMAL,
   DWRITE_FONT_STRETCH_NORMAL, 44.0f, L"", ...)` —— 权重/字号/斜体**硬编码**,
   族名/字形**由字体文件决定**。详见 §3.11.1。
2. **描边与字符由同一支画刷绘制** —— RTTI 签名 `SampleWindow::DrawKeys(ComPtr<ID2D1DeviceContext>&,
   D2D_RECT_F&, ComPtr<ID2D1SolidColorBrush>& brush, D2D1::Matrix3x2F&)` 只收**一支**
   `ID2D1SolidColorBrush`; exe 内**不存在**独立的边框绘制函数(仅 `DrawKeys` + `DrawInputVisual`)。
   皮肤键 `keyColor`/`keyOpacity` 驱动的就是这支画刷。
3. 实测反证(三条, 全部已在真机验证):
   - `keyOpacity = 0.0` ⇒ **字符与描边一起消失**(该透明度是**图层级**, 不是描边级);
   - `keyColor = #FFFFFF` ⇒ 描边与字符**同为白色**;
   - `keyColor = #FF0000` ⇒ 描边与字符**同为红色**(八角内部透明)。
4. **patch 二进制跳过 `DrawGeometry` 会破坏窗口初始化** (2026-09-19 试过并已回滚):
   `0x7258` 处的 `call [rax+0x78]` 之后紧跟 `test eax,eax / jne`, 其返回值不是纯错误码 ——
   跳过它令命令框**完全不弹出**。⇒ 该路径不可作为「去框」手段。
5. **命令框的显示 = 纯投递的 WM_CHAR (0x0102)**: capsHook 以 `InputHook("", ...)` 创建
   (无 V ⇒ 默认不可见, 吞掉文本键), 物理键到不了命令框窗口。实测铁证: 抑制投递后
   字母全部消失 (2026-09-19)。
6. **✅ 已落地: 数据 patch (2026-09-19)** —— 白名单是 `.rdata` 中的**绘制判定数据**
   (62 字符 UTF-16LE, 文件偏移 `0x1cca0` = RVA `0x1daa0`, 前后紧邻 `Draw()`/`dwriteF…`),
   已替换为不可匹配字符 U+0001×62 (保留长度): 字母/数字走普通字形路径 ⇒ **命令框内
   直接显示, 无八角框**。与已证伪的代码 patch (第 4 条) 性质完全不同: 改常量数据
   不动控制流。patch 前后 SHA 与还原路径: 部署树 `KeyFlux-CommandInput.orig.exe`
   (SHA256 `f14bba71…468ab3`) 为原始备份, 覆盖回去即还原。

**结论**: 「只去八角框、保留文字」的可行路径 = **数据 patch 白名单** (已落地, 只改常量
数据); 代码 patch (跳调用) 与皮肤调参 (单画刷同源) 均已证伪, 勿再尝试。

**契约**

```ahk
class CommandDisplay {
  static SuppressKeycap := false        ; 「IME 会话中」代理: true = 投递/缩写匹配/providers 全旁路
  static IsKeycapChar(c) => bool        ; 单字符; a-zA-Z0-9 ⇒ true (判定保留, 与 exe 已脱钩)
  static ShouldEcho(c) => bool          ; 抑制开启且命中白名单 ⇒ false
  static EchoChar(ih, c) => bool        ; true = 确实投递; false = 被抑制跳过
  static EchoBackspace(ih, vk?, sc?)    ; IME 会话中不投递 (物理退格已透传给 IME, 防二次删除)
  static Reset()                        ; 复位 (引擎退出)
}
```

**硬约束**:

1. 🔴 **所有命令框回显必须经本模块收口** —— 不得再直接调 `PostCharToCaspAbbr` /
   `PostBackspaceToCaspAbbr`(插件侧亦然)。散落直调会让「抑制」出现漏洞: 有的字符被拦、
   有的漏过去, 症状是**部分字母仍带八角框**, 且静态检查查不出。
   当前调用点: `CommandInputHooks.CommandInputOnChar/OnKeyDown`、
   `type9_keyflux.EnterCapslockAbbr`(Match 分支)、`everything_search` 的 `EverythingSession`;
   替换为 `AbbrInput` 的底层函数仅限这四处。
2. `IsKeycapChar` 的白名单语义保留原值 (仅 `0x30-0x39` / `0x41-0x5A` / `0x61-0x7A`)。
   ⚠ exe 内烧录值已由数据 patch 改为 U+0001 (见背景第 6 条) —— **AHK 侧与 exe 脱钩是
   有意为之, 勿"同步修复"**。本判定现在只服务于 ShouldEcho 的语义完整性 (IME 会话中
   「本来会画框的字符」不投递) 与排查对照。
3. `SuppressKeycap` 默认 **false**(零行为变更), 语义为「**透传模式总开关**」(§3.12 v4.2):
   由 `ImeInputHost.OnSessionBegin` 置位 (启用时**恒 true**, 不再查 IME 状态 —— 查询
   已整体证伪)、`OnSessionEnd` 复位。true = 投递通道整体关闭 (EchoChar/EchoBackspace
   恒 no-op —— v3 只停白名单字符是错的, 中文 U+4E00+ 会漏网双显); **providers 派发与
   缩写匹配保留** (v4.2 起: 词表恢复 + FuzzySuffixFire 恒跑 —— 用户裁决缩写命令全为
   英文字母, 中文输入仅发生在前置键之后, 那时字符已被插件消费、到不了匹配层)。
4. ⚠ **sync-out 会用仓库侧 (未 patch) exe 覆盖部署树** —— patch 后重跑 `make sync-out`
   会把八角框带回来。**2026-09-21 实测复现**: 一次 `make deploy` 后部署树 exe SHA 退回
   `f14bba71…` (= 官方原版), 用户看到「字母周围又有八角框」。**已自动化收口**:
   patch 脚本固化为 `tools/patch_command_input.py` (幂等, 前置哨兵校验 + 回读复核),
   并挂为 Makefile **`patch-commandinput`** 目标, 由 `sync-out` 配方**末尾自动调用**
   (先 `Stop-Process KeyFlux-CommandInput` 解锁 —— 运行中的 exe 自锁不可写; 该进程懒加载,
   下次唤起命令框时引擎重建)。**不再需要人工记忆这一步**。
   诊断: `make check-commandinput-patch` (只读, 报告部署树 exe 的 patch 状态)。
   patch 后 exe SHA 恒为 **`2aed3232…5fbe`**, 与原版差异**恰好 62 字节** (`0x1cca0–0x1cd1a`)。

回归守门人: `tools/command_input_hooks_test.ahk` 第 11/12 组(白名单边界 12 项 + 抑制语义 9 项)。

### 3.11.1 命令框字体 (2026-09-20 冻结, 同日数次改定)

**当前字体 = `Sthginkra` 加粗版 (轮廓膨胀 r=12)** —— SHA `b6f98778…289c`, 21,174,484 B,
33072 字形。由源 `D:\UserData\Downloads\Sthginkra.otf` (CFF/OTF) 经 **`otf2ttf` 转为 glyf
真 TrueType** + **元数据改造对齐 BOLD(700)** + **几何加粗 (轮廓膨胀)** 三步生成,
详见下方硬约束 2/3/4。生成脚本已固化: `tools/font_otf2ttf.py`、`tools/font_embolden.py`。

> ⚠ **本节描述的"当前字体"是仓库基线**。用户在自己的部署树里可通过设置页选择任意
> 字体 + 5 档字重 (见下方「配置写入端」与硬约束 7), 此时部署树 `bin/font/font.ttf`
> 与本节基线**必然不同** —— 属预期行为, 不是漂移。

历代字体重录 (均为当日决策, 供追溯):

| 次序 | 字体 | 来源 | SHA256 | 结果 |
|---|---|---|---|---|
| — | Iosevka Bold 2.3.3 | 上游仓库自带 | `deebc76e…` | 无中文字形 (靠回退) |
| 1 | 得意黑 Smiley Sans Oblique | 用户下载 | `2e4ce734…` | 斜体设计; 需改元数据 |
| 2 | MiSans-Bold | 仓库 `Assets/Fonts/` | `250fb5c8…` | 原生精确匹配, 零改造 |
| 3 | Sthginkra (CFF→glyf) | 用户下载 `.otf` | `d758c7b4…` | 笔画偏细 (竖干 80 单位) |
| 4 | **Sthginkra 膨胀 r=12 (当前)** | 由 3 加粗 | `b6f98778…` | 竖干 80→104; 2.01x 体积 |

选择理由 (历代): ①与软件 UI 视觉统一 (配置界面全局 `AppUiFont` = MiSans);
②正体非斜体; ③字符覆盖; ④优先原生 `usWeightClass=700` ⇒ 免改造。
**判据优先级: 原生 700 精确匹配 > 字符覆盖 > 笔画粗细合适 > 视觉风格**。

**机制 (PE 静态解析结论, 无源码事实)**: 命令框文本字体**不来自系统字体, 也不由配置决定**。

- exe 内 UTF-16 字面量 `font\font.ttf` (RVA `0x1ddc8`) 是**唯一**字体来源: 相对 exe 自身
  目录解析 ⇒ 实际文件 = `bin/font/font.ttf`。
- exe 用 `GetModuleFileNameW` 定位自身目录后拼该相对路径, 建 `IDWriteFontCollection`
  (`IDWriteFont3`/DWrite.dll 是唯一被导入的字体相关 DLL, 且**只导入 `DWriteCreateFactory`**
  —— 其余接口全走 COM 虚表)。
- 创建文本格式的实参由 exe 内断言串直接给出:
  `dwriteFactory->CreateTextFormat( fontName.c_str(), fontCollection.Get(),
  DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
  44.0f, L"", textFormat.GetAddressOf() )` (RVA `0x1ddf0`)。
- `fontName` 是**局部变量, 取自该 ttf 的 `name` 表** —— `.rdata` 全量字符串里
  **不存在任何字体族名**(`Iosevka` 的 ASCII 与 UTF-16 形态均 0 命中), 也**不存在**
  `fontName` 配置键。
- 渲染栈 = **DirectWrite + Direct2D + D3D11 + DComp** (导入表: `DWrite.dll` / `d2d1.dll` /
  `d3d11.dll` / `dcomp.dll`), **非 GDI** ⇒ `WM_SETFONT` / `AddFontResource` 类注入无效。

**⇒ 换字体 = 替换 `bin/font/font.ttf`**(无需重编译 / 装系统字体 / 改配置)。

**皮肤配置 `bin/CommandInputSkin.txt` 的边界**: KeyFlux 新增的外置皮肤配置, **恰好 18 键**
(2026-09-21 订正: 原文写「19 键」系笔误 —— 键名清单、`DefaultCommandInputSkin()`、
`OptionsDTO.CommandInputSkin`、`CommandInputSkin.tmpl` 四处均为 **18**; 有单测
`skin_defaults_test.go` 硬断言字段数),
与 exe `.rdata` 中的配置键**一一对应**(已全量比对; exe 内键名以 ASCII NUL 填充存放,
文件名字面量 `CommandInputSkin.txt` 为 UTF-16LE, 位于 exe 偏移 `0x1c060`):

```
backgroundColor  backgroundOpacity  borderWidth     borderColor      borderOpacity
borderRadius     cornerColor        cornerOpacity   gridlineColor    gridlineOpacity
keyColor         keyOpacity         hideAnimationDuration            windowYPos
windowWidth      windowShadowColor  windowShadowOpacity              windowShadowSize
```

⚠ **18 键全是颜色/透明度/圆角/尺寸/动画时长, 无任何 font 键** ⇒ **字体的样式参数**不经
`CommandInputSkin.txt` 调整。

🔴 **读取时机 = 仅进程启动时读一次 (2026-09-21 实证, 与 `font.ttf` 完全相同)**:
用 Windows 托管的**最后访问时间**(`fsutil behavior query disablelastaccess` = `2`)做无侵入
实验 —— 单独启动 `KeyFlux-CommandInput.exe` (cwd = 部署树 `bin/`) 后观察该文件
`LastAccessTime`: **启动后 0.1s 前进**, 之后 10s 观察窗内**不再变化**, 进程全程存活
(证明是**常驻进程**而非每次唤起重建)。
⇒ **改皮肤与改字体共用同一结论: 必须重启命令框进程才可见**。由此产生的两处设计:
1. **设置页**: 「命令框字体」小节**并入「命令框皮肤」卡** (不再独立成卡), 共用 `ShowSkin`
   分区开关, 用细分隔线分组 —— 两者生效条件一致, 分卡会让用户误以为生效时机不同
   (`MotionSmokeTests` 的 `sectionBody` 计数随之由 9 回到 **8**)。
2. **保存端**: 两段**合并判定**是否变化 (见硬约束 7)。

⚠ **`sync-out` 的 robocopy 会用仓库侧的旧默认覆盖部署树的该文件** —— 隔离目录实测复现:
源比目标旧时 robocopy 把源标为「**较旧的**」**却仍然复制** (复制 1 / 跳过 0)。
但影响是**暂时性**的: `CommandInputSkin.txt` 是**派生产物** —— 引擎每次启动都由
`GenerateScripts` 从 config.json 重新渲染, 故下次引擎重启即自愈
(与 exe patch 的永久覆盖不同, 那个没有自愈路径)。

**但字体文件本身自 2026-09-20 起可由配置指定** (新增 `options.commandFont` 段):

| 键 | 类型 | 语义 |
|---|---|---|
| `sourcePath` | string | 用户经设置页「选项 → 命令框皮肤」卡内字体小节选择的字体文件**绝对路径** (空 = 未自定义, 沿用现有 `font.ttf`) |
| `weight` | string | 字重档位 `thin`/`light`/`regular`/`semibold`/`bold`。**自 2026-09-21 起真实生效** —— 见下「字重档位机制」 |

**字重档位机制 (2026-09-21 起真实生效)**:

exe 硬编码请求 `BOLD(700)` 且不可改, 故"调字重"只能在**字形**上做 —— 即按档位对
轮廓做**几何变换**: 变粗用**膨胀** (半径 r, 笔画宽度精确 +2r 字体单位), 变细用**腐蚀**
(对偶运算, 宽度精确 −2r)。生成端是 Go 程序, 没有也无法引入轮廓布尔运算库 (离线环境取不到
依赖), 调外部 Python 又会把部署树绑死在「目标机器须装有 Python+fontTools+skia-pathops」上
⇒ **破坏可移植性**。

故采用 **构建期预烘焙 + 运行期选文件**:

| 档位 | 运算 | 半径 (upem 1000) | 变体文件 | 落地来源 |
|---|---|---|---|---|
| `thin` | 腐蚀 | −20 | `<源去扩展名>.thin.ttf` | 变体 |
| `light` | 腐蚀 | −10 | `<源去扩展名>.light.ttf` | 变体 |
| `regular` | — | 0 | (无) | **源字体本体** (不落冗余副本) |
| `semibold` | 膨胀 | 14 | `<源去扩展名>.semibold.ttf` | 变体 |
| `bold` | 膨胀 | 28 | `<源去扩展名>.bold.ttf` | 变体 |

- 🔴 **为什么需要 `thin`/`light` (腐蚀)**: 命令框 exe 请求 `BOLD(700)`, 用户常直接选一份
  **已是粗体**的字体作源 (如 inpin 鸿蒙体 w700)。对这类源, "不做膨胀"只是**回到源字体**,
  无法得到比源字体更细的效果 ⇒ 必须靠**腐蚀**主动减细。
- 🔴 **档位间距按"实测墨迹"定, 不按半径等分**: 膨胀的墨迹增长是**次线性**的 (r=6→10 只
  +3.0%, 而 0→6 有 +7.3%), 因为膨胀同时填窄字腔; 腐蚀则近线性 (−6.7%/6 单位)。
  旧 4 档 (0/6/10/14) 实测首尾总差仅 +15.8%、中→半粗只有 +3.0% (低于 ~3.5% 肉眼可辨阈值),
  故用户报障"看不出区别"。新 5 档实测相邻增幅 9.7%~15.8%、首尾 +27.1%, 每档可辨。
- **选文件逻辑** (`script.FontWeightVariants` / `VariantPath` / `InstallCommandFont`):
  按 `weight` 推出变体路径, 变体**存在且自身通过格式闸门**才采用, 否则**回落源字体本体**。
  回落而非报错 —— 用户可能直接从别处拷来字体而未烘焙变体, "显示原样字体"远好于"整条链路失败"。
- 🔴 **未知/空字重一律回落 `regular`** (中性档)。**不可猜更粗的档位** —— 猜粗会在部分字体上
  把 CJK 字腔 (封闭白区) 永久填死 (不可逆)。已移除的旧档名 (`medium`) 同样按未知处理。
- 🔴 **变体命名必须与生成端约定一致** (`<源去扩展名>.<档位><扩展名>`, 与源字体**同目录**)。
  名字对不上 ⇒ 静默回落源字体 ⇒ 表现为"选了字重没变化"。
- **烘焙工具**: `tools/font_weight_prebake.py <src.ttf>` (默认写入源字体同目录)。
  依赖 fontTools + skia-pathops + Pillow (Pillow 仅用于安全校验), 复用
  `tools/font_embolden.py` 的真布尔并集/差集原语 (单一算法真源)。
- 🔴 **退化字形必须"失败保原样"而不是中止整档**: skia-pathops 的布尔并集对**退化轮廓**
  (零面积自交 / 重复点堆叠) 会抛 `PathOpsError`。实测 Sarasa 系统字体在 r=+28 时**仅
  `uni57F3` 一个字形**触发 (r=+14 全通过)。若因此中止, 整档 `bold` 都拿不到 ——
  代价与收益完全不成比例。`bake_one` 现逐个字形捕获异常并**保留原轮廓** (该字形只是
  不加粗, 与邻居差 1 档以内), 同时计数回报; 占比超过 `UNSAFE_RATIO_MAX = 1%` 才判定
  该档不可用 (防"大面积退化却仍落盘")。
- **字体集合抽取 + 裁剪工具 (2026-09-21 新增)**:
  - `tools/font_ttc_extract.py --list <x.ttc>` 列出全部 face; `--family "Sarasa Gothic SC"`
    或 `--index N` 抽取单个 face 为独立 `.ttf`。**用途**: 命令框 exe 只用 `.ttc` 的
    **第一个 face**, 而 Sarasa 这类字体把 6 风格 × 8 语言区共 48 个 face 打进一个文件
    (~80 MB/字重), face[0] 常是 CL (古典拉丁) 而非用户想要的 SC ⇒ 必须显式抽出。
  - `tools/font_subset.py <src> <dst>` 按 Unicode 区段裁剪。默认保留 Basic Latin→
    Fullwidth Forms + CJK 统一汉字基本区 (U+4E00-9FFF), 排除 Ext-A/Ext-B+/CJK 兼容区。
    **用途**: Sarasa SC 单 face 44.9 MB, 超出 `FontMaxBytes` (32 MiB) ⇒ 裁剪至
    **6.9 MB** (24362 字形) 才可用; 这是让"选大字体"从不可用变为可用的关键一步。
    ⚠ 裁剪会丢弃 Ext-A/B 等**生僻字**字形 —— 只保简体常用区, 生僻字回退系统字体。
- 🔴 **可复现构建 (2026-09-21 修)**: fontTools 的 `head.compile` 内有
  `if ttFont.recalcTimestamp: self.modified = timestampNow()` —— 默认 `True` 会用**保存时刻**
  覆盖 `head.modified`, 使**同一输入产出不同 SHA** (实测仅差 6 字节, 全在 `head` 表):
  变体无法用 SHA 断言/门禁校验, 部署也不可复现。三个字体工具现已统一: 置
  `f.recalcTimestamp = False` + 用**源字体的 `modified` 原值**填回 ⇒ 输出纯粹是输入的函数。
  实测: 两次独立烘焙 4 档变体 **SHA 全同**; 与旧产物的差异**逐表比对仅 `head` 一处**
  (其余 13 表字节全同) ⇒ 修复不改字形, 只归一化时间戳。
  ⚠ `head.checkSumAdjustment` 由 `save()` 末尾按新内容自动重算, 无需手工维护。
- 🔴 **两个方向的半径上限不同, 且都不可逆**:
  - **膨胀上限 = CJK 字腔**。膨胀会蚕食封闭白区, 过大即糊成实心块。实测 inpin 鸿蒙体
    最大字腔面积保留率 r=14→0.89 / r=24→0.80 / r=28→0.75 / r=32→0.72 (**无突变拐点**),
    故取 r=28 (保留 3/4) 留足余量。
  - **腐蚀上限 = 该字体最细笔画的半宽**。过大则细笔画先锯齿、后断裂 (整段消失)。
  - 换字体后必须**重新出预览确认**, 不要盲目沿用。
  - ⚠ **半径值不可跨字体照抄**: 同一组 r 在**粗细不同的源**上会得到**相反**的档位顺序。
    实测 (Sarasa XLight 源): r=−10 只剩 46.9% 墨迹 (细于 r=−20 的 68.0%) —— 腐蚀过猛
    已毁笔画。故从**常规粗细 (wght=400)** 的源出发时, 默认半径 (−20/−10/14/28) 才成立:
    实测 44px 下 `thin 0.45x / light 0.71x / regular 1.00x / semibold 1.43x / bold 1.85x`,
    **严格单调递增**, 相邻增幅 29~58%, 总跨度 +312%。
- 🔴 **安全判据必须用"字腔面积", 不可用"字腔个数"**: 膨胀会在相邻笔画间**新生成**小的
  封闭白口袋, 使个数**先升后降** (实测本字体 r=14 时 15→16, r=20 时 18, 之后才回落)。
  按个数设阈值会误报 —— 本实现初版即因此把一个其实可用的档位判成失败。
- **烘焙自带安全校验**: 膨胀档查**最大字腔面积**保留率 (下限 0.55); 腐蚀档查**最细笔画**
  墨迹保留率 (下限 0.30) 且不得断成多段。**不合规即删除该变体并跳过该档** ——
  生成端因此回落源字体, 不会拿到坏文件。
- **可移植性收益**: 部署树零外部依赖 (纯数据文件), 运行时零计算, 无超时/无失败面,
  不执行任何外部脚本。

- **写入端**: 设置页「选项 → 命令框皮肤」卡内的**字体小节** (2026-09-21 由独立卡片并入;
  系统文件弹窗 → `StorageProvider.OpenFilePickerAsync`, 过滤器 `*.ttf;*.otf;*.ttc`)。
- 🔴 **选择时的即时校验提示 (2026-09-21 新增, 修"选了没反应"缺陷)**: 生成端的失败
  **一律静默跳过** (字体是表现层资源, 不阻断脚本生成) —— 这让用户选中 83 MB 的 `.ttc`
  或 CFF 的 `.otf` 后**界面毫无反馈**, 命令框字体也没变, 表现为"选了跟没选一样",
  且无从得知原因。故新增 **`Models/CommandFontValidator.cs`** 把同一套判据前移到 UI
  **选择时刻**: 体积 (>32 MiB) / sfnt 签名 (`0x00010000`·`true` 通过; `OTTO` 拒绝;
  `ttcf` 看首 face) / 文件可读性 ⇒ 结论 + 原因**当场显示**在字体小节下方的提示条
  (`HasCommandFontNotice` / `CommandFontNoticeIsError`, AXAML `Border.noticeBanner`
  两态样式: `.err` 暖红 = 不可用, 默认米色 = 可用但有注意点)。
  ⚠ **判据必须与 Go 侧同口径** —— 漂移会双向出错: UI 说可用而生成端跳过 (用户又被坑),
  或 UI 拦住而生成端其实接受 (合法字体用不了)。守门人 `CommandFontValidatorTests` 逐字节
  固定签名判定, 并把 32 MiB 限值与 Go 常量绑成一条断言。
  📌 **启动时不做复检** —— 配置里的路径很可能指向已删除/超限的文件 (正是上次踩的坑),
  若载入即弹错误提示, 用户每次打开设置都看到一条无法消除的告警 (噪声)。只在**用户主动
  选择**时给结论, 那才是需要解释"为什么没变化"的时刻。
  ⚠ 本校验**只做能当场断言**的检查; 判定"可用"**不等于已生效** —— 仍需重启命令框进程。
- **`.ttc` 只用第一个 face (用户最易误解点)**: 命令框 exe 建 `IDWriteFontCollection` 时
  只取首个 face, 而 Sarasa 等字体把多语言版本打进同一文件 ⇒ 用户以为选了"简体中文版",
  实际落地的是集合里的**第一个** face (常为拉丁版)。`CollectionFirstFaceOnly` 专门给出
  这条提醒 (中性色, 非错误态 —— 否则用户会以为自己选错了)。
  正确做法: 先用 `tools/font_ttc_extract.py --family "<想要的族名>"` 抽出目标 face。
- **消费端**: 生成端 `InstallCommandFont()` (`internal/script/font.go`), 在 `GenerateScripts`
  (运行时) 与 `GenerateAHK` (CLI/校验) 中调用, 把 `sourcePath` 指向的文件**复制到**
  `bin/font/font.ttf`。CLI 路径下复制目标 = **输出文件所在目录**的 `font/font.ttf`
  (即部署树的 `bin/font/`), 而非 cwd。
- **失败一律静默跳过** (不阻断生成): 路径为空 / 源不存在 / 超 32 MiB / **轮廓非 glyf
  (只接受 `0x00010000`·`true`, 以及首 face 为 glyf 的 `ttcf`; `OTTO`/CFF **一律拒绝**)**
  / 源即目标 (避免自复制截断)。跳过时沿用上次成功落地的
  `font.ttf`。
- **链路三处同步**: C# `ConfigModels.Options.CommandFont` ⇄ Go `model.Options.CommandFont`
  ⇄ Go `OptionsDTO.CommandFont` (CONTRACTS §5.2 双轨 DTO, 漏改任一处会静默抹字段)。
- 🔴 **UI 文案键必须查证为空闲 — 不可凭"看起来像空洞"就用** (2026-09-21 踩坑):
  字重 5 档的标签键曾误用 `2517`/`2518`, 而这两个键**早已被
  `SelectedActionPageView.axaml` 占用** (自定义文本类型的留桩提示条文案 / 「创建专属行为」按钮)。
  因 JSON **后定义覆盖前定义**, 那两处 UI 文案被**静默**改成了「极细」/「细」, 且键数虚增
  (405 个定义但解析后仅 403 个有效键)。现已迁到经查证的空洞 `2526`/`2549`。
  **加键前必做两查**: ① 在 `Resources/i18n.json` 里确认**该键不存在**(并 grep 全仓引用);
  ② 用「行内正则计数 vs JSON 解析计数」比对是否相等 —— **不等即有重复键**。
  `I18nResourceTests.Json_Is_Loaded_Completely_Without_Duplicate_Keys` 已硬断言
  `去重键数 == ExpectedKeyCount`, 是这道工序的守门人。
- 🔴 **本条不推翻「换字体 = 替换 `bin/font/font.ttf`」**: 配置只是让**生成端**替用户做那次替换。
  字体族**依旧无法通过配置改变** —— 命令框 exe 的 `CreateTextFormat` 实参是局部变量, 取自
  该 ttf 的 `name` 表 (见下「机制」第 4 条)。

**硬约束 (替换 font.ttf 时)**:

1. 🔴 **文件名必须是 `font.ttf`** —— 路径是 exe 内烧录的字面量, 改不了。
2. 🔴 **元数据须与请求匹配, 否则 DirectWrite 合成加粗** —— exe 请求
   `WEIGHT_BOLD(700)` + `STYLE_NORMAL`。**优先选原生 `usWeightClass=700` 的 Bold 静态字体**
   (如 `MiSans-Bold.ttf`), 此时**零改造**。若目标字体原生非 700 (得意黑 `400+ITALIC`、
   Sthginkra `400`), 私人字体集合中只有这一个 face 时, DirectWrite 会施加
   **合成加粗 (BOLDSIM, 笔画横向撑宽)** ⇒ 中文密集笔画糊成一团 (WPF 实测复现)。
   修法 = 改造元数据使请求成为**精确匹配**:
   - `OS/2.usWeightClass`: `400 → 700`
   - `OS/2.fsSelection`: 清 `ITALIC(0x0001)`/`REGULAR(0x0040)`, 置 `BOLD(0x0020)`
     (其余位如 `WWS(0x0100)`/`USE_TYPO_METRICS(0x0080)` **保留**)
   - `OS/2.panose.bWeight`: `→ 8`
   - `head.macStyle`: 清 `italic(0x0002)`, 置 `bold(0x0001)`
   - `post.italicAngle`: 归 0
   - **`name` 表不动**(保持族名, 因 `fontName` 取自文件名表, 改名有查不到的风险)
   - ⚠ 注意: 清元数据标记**改不掉斜体字形本身** —— 得意黑的倾斜画在 glyf 轮廓里,
     `italicAngle=-8.0` 时字形实测倾角 ≈8.7°, 故清标记后**渲染依旧斜**。
     要正体只能换原生正体字体。
   - ⚠ 改完必须用 **fontTools `save()` 重编译**重算校验和 —— 手工改字节会让
     `OS/2`/`head`/`post` 三表校验和失配 (fontTools 会报 `bad checksum`)。
3. 🔴 **轮廓格式须为 glyf (TrueType), 不接受 CFF/OTF** —— 已知可用的历代字体
   (Iosevka / MiSans-Bold / 得意黑 TTF / Sthginkra) **全是 glyf**; exe 对 CFF 的加载路径
   **无验证先例** (其自定义字体集合若硬编码 `DWRITE_FONT_FACE_TYPE_TRUETYPE`, CFF 会直接加载失败)。
   拿到 `.otf` (CFF) 源**必须先转 glyf**。
   **🔴 该约束已由代码强制 (2026-09-21)**: `InstallCommandFont` 的格式闸门
   (`classifyFontKinds`) **只接受 glyf** —— `0x00010000` / `'true'` / `'ttcf'` 且首 face 为
   glyf; **`OTTO` 一律拒绝**并返回可读原因。回归背景: 原实现把 `OTTO` 当作合法 sfnt 签名
   放行 ⇒ 用户在设置面板选 `.otf` 后, 文件被**原样复制**为 `bin/font/font.ttf`
   (实测 `sfntVersion=4f54544f`, 8,688,388 B) ⇒ 下游加载失败且**全程静默**
   (错误被刻意忽略), 用户只看到「选了没效果」。守门人:
   `font_test.go::TestInstallCommandFont_FormatGate` (CFF 拒绝 / glyf 集合接受 /
   CFF 集合拒绝 / `'true'` 接受, 4 例)。
   转换要点 (缺一不可, 均为实测踩坑):
   - 逐字形 `Cu2QuPen(TTGlyphPen(), max_err=1.0, reverse_direction=True)` 把三次贝塞尔转二次;
   - **`sfntVersion` 必须从 `OTTO` 改为 TrueType 签名** (否则解析器仍按 CFF 找表 ⇒
     FreeType 报 `SFNT font table missing`), 即 `\x00\x01\x00\x00`;
   - 必须**显式创建 `loca` 表** (`newTable("loca")`), fontTools **不会**随 `glyf` 自动创建
     (否则 `loca table missing`);
   - `maxp` 需 `tableVersion=0x00010000` + `recalc()`, 且**补齐 glyf 专有字段**
     (`maxZones=1`/`maxTwilightPoints`/`maxStorage`/`maxFunctionDefs`/`maxInstructionDefs`/
     `maxStackElements`/`maxSizeOfInstructions` 全 0) —— CFF 源是 v0.5, 没有这些键,
     不补会 `KeyError: 'maxZones'`;
   - `recalc()` 前须先对每个非空字形调 `glyph.recalcBounds(glyf)`, 否则报
     `'Glyph' object has no attribute 'xMin'`;
   - 删 `CFF `/`VORG`, 丢弃失效的 `DSIG` (改造后签名必然失效), `post.formatType=3.0`;
   - **`name`/`cmap`/`hmtx`/`GPOS`/`GSUB` 原样保留** ⇒ 族名、度量、字符映射不变。
   - **保真度实测**: OTF vs 转换 TTF 二值掩膜 IoU 随字号收敛 `0.79 (56px) → 0.91 (200px)
     → 0.9896 (400px)`, 400px 下墨迹像素仅差 0.05% ⇒ **差异纯为小字号渲染量化 (CFF 提示丢失),
     形状无损**。
4. **笔画粗细可调, 用几何加粗 (轮廓膨胀) 而非 DirectWrite 合成加粗** —— exe 请求的
   `BOLD(700)` 只是**元数据权重**, 若字体设计本身偏细, 元数据匹配 700 后
   **DirectWrite 不会再加粗** (精确匹配 ⇒ 无 BOLDSIM) ⇒ 想变粗只能在字形上做。
   🔴 **推论 (2026-09-20 补)**: 故 `options.commandFont.weight` 这个字重档位**无法改变渲染
   粗细** —— 无论选 `regular` 还是 `bold`, exe 恒定请求 700 且已精确匹配, 最终笔画宽度
   100% 由上述字形轮廓决定。该字段当前**仅作显式记录 + 未来扩展落点** (例如将来做"按字重
   查表找对应 ttf"); 设置页在该控件旁不承诺"能调粗细", 避免误导。
   两条路:
   - ✅ **几何加粗 (推荐, 可控)** = 轮廓膨胀: `dilate(path, r) = path ∪ stroke(path, 2r)`,
     即与半径 r 的圆盘做 Minkowski 和。**笔画宽度精确增加 2r 单位**;
     `upem=1000` 时 44px 下增加约 `2r × 0.044` px。工具: `tools/font_embolden.py`。
     - 实测基准: 本字体竖干 80 单位 (44px 下 3.52px)。
       `r=8→96 (4.22px)`, `r=12→104 (4.58px)`, `r=16→112 (4.93px)`, `r=20→120 (5.28px)`。
     - 🔴 **半径上限 = CJK 字腔**: 本字体设计紧凑, **`r ≥ 16` 起封闭白区 (字腔) 开始被
       填死** —— 「保存 加载 重置」会糊成实心块且不可逆 (原文轮廓信息丢失)。
       故 **推荐 r=12**; 需更粗时应换更粗的字体, 而非继续加大半径。
     - 🔴 **必须用 `pathops.op(..., PathOp.UNION)` 做真布尔并集**: **不可用**
       `pathops.union([a, b], pen)` —— 后者只是把轮廓丢进同一 Path 再 `simplify()`,
       当原字形 (TrueType 顺时针外轮廓) 与 stroke 产出的环 (绕向不同) 混合时会误判,
       **把重叠区当空洞 ⇒ 笔画渲染成空心轮廓**。实测 (`加`, r=16): 真 UNION 面积
       `280700 → 403497` (3 段轮廓); concat+simplify 只得 `285886` (碎成 12 段)。
     - ⚠ 副作用: **文件体积翻倍** (10.5MB → 21.2MB, 圆角 join 引入大量曲线点);
       `maxp` 的 `maxPoints`/`maxContours` 必须 `recalc()` (337→722 / 40→34)。
     - ⚠ 复合字形 (`numberOfContours < 0`) 须跳过, 否则子字形会被重复加粗。
   - ⚠ **备选: DirectWrite 合成加粗 (BOLDSIM)** —— 把 `usWeightClass` 压回 `400`
     (即让请求 700 变成"不匹配") 让 DirectWrite 自动合成加粗。**零体积增长**,
     但**加粗量不可控**且质量低于几何加粗。仅在不想改字形时使用。
5. **字符覆盖须自足** —— 命令框要显示字母/数字/键名/中文, 且 exe 传 `L""` (**空 locale**)
   ⇒ 字体回退链不确定。原 Iosevka **不含中文字形**(204 字符抽样缺 109), 中文靠回退;
   Sthginkra 自带 **33072 字形 / 33288 cmap 项**(191 字符抽样 **0 缺失**; CJK 常用区
   20976/20992 = 99.9%) ⇒ 不依赖回退。
6. ⚠ **`sync-out` 不含 `*.ttf`** —— Makefile `sync-out` 的 robocopy 白名单是
   `'*.ahk' '*.exe' '*.ps1' '*.txt' '*.dll'`, **`*.ttf` 不在其中** ⇒ `make out`/`deploy`
   **不会**把仓库字体同步到部署树, 也**不会**删除部署树字体。换字体后须**手动同步两处**
   (仓库 `bin/font/` + 部署树 `bin/font/`), 否则两侧不一致。
7. ⚠ **字体与皮肤都只在进程启动时读取一次** —— 实测 `KeyFlux-CommandInput.exe` 运行期间
   字体文件**未被锁定**(可写), 但换字体 / 改皮肤后**必须重启命令框进程**才会生效
   (DirectWrite 私有字体集合与皮肤参数在启动时构建并常驻; 皮肤的读取时机已用最后访问时间
   实证, 见上文「皮肤配置边界」)。进程**懒加载**: 杀掉后引擎不自动拉起,
   待下次唤起命令框时重建。
   **✅ 已自动化 (2026-09-21)**: 设置面板「保存」即让新外观生效, 无需用户手动重启。
   `server.SaveConfigHandler` 在落盘**之前**用
   `script.CommandBoxAppearanceFromConfigFile(script.ConfigRelPath)` 记下旧的
   `options.commandFont` **与** `options.commandInputSkin`, 落盘后若**任一段**与新的不相等,
   则调 `proc.StopProcessByName("KeyFlux-CommandInput.exe")` 结束旧命令框
   (引擎随后本就重启并重新生成脚本 ⇒ 新字体已落到 `bin/font/font.ttf`、皮肤已重渲染到
   `bin/CommandInputSkin.txt`; 命令框在用户下次唤起时用新外观重建)。
   **只判「外观是否变化」而非每次保存都杀** —— 避免无谓地让用户付出重建 DirectWrite
   字体集合的代价。
   - 两段**合并为一次文件读取** (`CommandBoxAppearance` 快照): 保证两段来自同一文件快照,
     不会撕裂; 二者生效条件本就一致 (设置页也已并卡, 见上文)。
   - `proc.StopProcessByName` 对「进程不存在」(taskkill 退出码 128) 视作**成功**(幂等):
     用户可能从未唤起过命令框。
   - `CommandBoxAppearanceFromConfigFile` 任何读取/解析失败一律返回**零值**; 零值 ≠ 用户的
     实际选择 ⇒ "读不到旧配置"被判为"外观变了", 走保守分支。**宁可多重建一次, 不可漏生效**。
   - `script.ConfigRelPath` 是配置落点的**单一真源** (`SaveConfigFile` 与读取端共用),
     两处各自硬编码一旦分叉会导致「读错文件 ⇒ 恒判未变 ⇒ 新外观永不生效」的静默缺陷。
   - 守门人: `font_test.go::TestCommandBoxAppearanceFromConfigFile` **6 例** (正常读出两段 /
     **皮肤改动可被检出** / 文件缺失退化 / JSON 非法退化 / 缺 options 段退化 /
     零值≠用户选择) + `proc_test.go::TestStopProcessByName_MissingIsIdempotent` (1 例)。
8. **回滚**: 历史字体各存一份, **均在部署树** `bin/font/`:
   - `font.ttf.bak-iosevka` —— 原始 Iosevka Bold 2.3.3 (`deebc76e…`, 539,832 B)
   - `font.ttf.bak-smiley` —— 得意黑 (`2e4ce734…`, 2,629,764 B)
   - `font.ttf.bak-misans` —— MiSans-Bold (`250fb5c8…`, 7,804,780 B)
   - `font.ttf.bak-sthginkra-thin` —— Sthginkra 细体 (`d758c7b4…`, 10,511,648 B)
   另有 `git checkout HEAD~ -- bin/font/font.ttf` 回到上一个已提交版本。

**当前状态**: `bin/font/font.ttf` = **Sthginkra 加粗版 (膨胀 r=12)**,
SHA256 `b6f98778…289c`, 21,174,484 B, 33072 字形, 族名 `Sthginkra`,
`usWeightClass=700` / `fsSelection=0x01A0` / `macStyle=0x01` / `italicAngle=0`,
`sfntVersion=0x00010000` (真 TrueType), `maxp maxPoints=722 maxContours=34`。
三方一致 (仓库 = 部署树 = 构建产物)。
**注意**: 该 SHA 与源文件 `D:\UserData\Downloads\Sthginkra.otf` **不同** —— 差异即
CFF→glyf 转换 + 元数据改造 + 轮廓膨胀三步。
**注意 (2)**: 若用户在设置页为 `options.commandFont.sourcePath` 选了字体, 生成端会把它复制
覆盖到该路径 —— 此时"三方一致"仅指**仓库与部署树在未被配置覆盖时**相同; 用户配置生效后
部署树字体为**用户所选文件的字节**, 与仓库副本可能不同 (这是预期行为)。

### 3.12 ImeInputHost —— 命令框透传模式开关 / 恒可见 hook (2026-09-19 v4.2 冻结, 焦点修复 + 缩写执行恢复)

**根因 (实证修正, 覆盖 v3 表述)**: capsHook 以 `InputHook("", ...)` 创建, 无 V 选项 ⇒
**默认不可见 = 吞掉文本键**, 物理键到不了命令框窗口, IME 上下文永远收不到键, 组合无从
发生。官方文档那句 "does not support IME" 的实际含义: 钩子在按键抵达 IME **之前**就把
它翻译成 ASCII —— 吞键模式下中文确实不可能; **但 V (可见) 模式下物理键会透传到焦点窗口
的 IME 上下文, 组合/候选/上屏全部由输入法原生完成** (上屏中文以 WM_CHAR 直达命令框,
不经 AHK 回调)。

❗ 定位过程中证伪并已废弃的前提 (记录以免重走; 第 5/6 条为 v3→v4 的死因链):
- 「原软件对输入法有硬性限制」—— 不存在 (`DisableIME()` 全仓 0 调用点, exe 未导入
  imm32, 无子控件)。
- 「皮肤 keyColor/keyOpacity 可只去框留字」—— 不可行, 见 §3.11。
- 「ImmGetCompositionStringW 能在引擎进程读到组合串做浮层回显」—— **证伪**: ImmGetContext
  只能取本线程上下文, 引擎进程永远看不到前台 IME 的组合 (浮层恒空)。浮层方案 (v2)
  已整体删除 (用户否决: 要求字母直接显示在命令框内)。
- 🔴 **「跨进程读 IME 开关状态做条件透传 (v3 设计)」—— 证伪, 即 v3 的死因**: HIMC 是
  进程本地句柄, 跨进程 `ImmGetContext` 恒返回 0 (kf_p1_ime_crossproc 探针实测
  notepad 同样 hIMC=0) ⇒ `_QueryImeOpen` 恒 -1 ⇒ `SuppressKeycap` 恒 false ⇒ hook 恒
  吞键 ⇒ 中文打不出 (2026-09-19 用户真机确认; v3 的透传分支从未生效过)。
- 🔴 **「TSF 全局 compartment 读 IME 中英模式」—— 证伪**: AHK 内 DllCall 消费 TSF
  四连败: CLSID_TF_ThreadMgr REGDB_E_CLASSNOTREG → `TF_CreateThreadMgr` 免 COM 路径
  成功, 但 `ITfThreadMgr::Activate` 在 AHK 主线程 DllCall 直接挂死 (须跳过),
  `GetGlobalCompartment` (vtable idx14, IDL 序) 调用即抛异常 (kf_p2b/p2c/p2d 探针)。
  结论: **放弃一切 IME 状态预查, 透传恒开**。

**现行方案 (v4.2)**: hook 恒 V (本模块启用时) —— `MakeCapsHook` 建 `InputHook("V", …)`
+ **原词表** (v4.2 恢复, 两形态共用); `ImeInputHost.OnSessionBegin` **恒置**
`CommandDisplay.SuppressKeycap := true`。物理键透传接管一切显示: 英文字母原生直显;
拼音进 IME 原生组合/候选/上屏, 上屏中文以 WM_CHAR 直达命令框 (非白名单, 无框)。
投递通道整体关闭 (§3.11 硬约束 3); **缩写匹配两形态恒开** (词表 MatchList 全串 +
FuzzySuffixFire 后缀, 与历史形态完全一致 —— 用户裁决: 缩写全英文字母, 中文意图仅在
前置键之后, 那时字符已被插件消费)。providers 派发保留。回退 = 注释掉模板的
`Register(ImeInputHost)`/`Enable()` 两行 ⇒ hook 回历史形态 (无 V + 词表 + 投递显示),
自洽。

**焦点契约 (v4.1, 2026-09-19 用户真机反馈后新增)**: 透传模式下物理键按「焦点窗口」路由,
而命令框窗口带 **WS_EX_NOACTIVATE**(kf_focus_probe v5 探针实测 exStyle=0x8200008,
NOACTIVATE=1 + TOPMOST) —— SHOW 消息只改可见性、从不带来键盘焦点 ⇒ 透传的按键会全部
打进会话开始时的原文本框(用户实测: 英文无法输入 + 焦点滞留)。故 `EnterCapslockAbbr`
在 SHOW 之后、建 hook 之前必须调用 `CommandDisplay.ActivateCommandWindow()` 显式激活:
等窗口可见 → WinActivate → 循环回读 WinActive → 失败兜底 AttachThreadInput + SetFocus;
**激活失败则置 `SuppressKeycap := false` 降级历史形态**(吞键 + 投递显示, 英文仍可用;
OnSessionEnd 复位标志, 降级自洽无泄漏)。会话结束 (非 Match 分支) 经
`CommandInputHooks.ActivateBackend` 把前台还给会话开始时的窗口 —— 该函数的
WinActive 检查保证历史形态会话零行为变更 (命令框本就不在前台)。Match 分支不恢复
(命令体自会接管前台)。

```ahk
class ImeInputHost {                    ; 注册为 CommandInputHooks provider (全 static)
  static Enabled / InSession            ; 闸门 / 会话标志 (PrevOpen 已随查询退役)
  static OnSessionBegin()               ; 恒置 CommandDisplay.SuppressKeycap := true (不查任何状态)
  static OnSessionEnd() / Disable()     ; 复位 false (hook 回历史形态)
  static OnChar/OnKey => false          ; 防御性透明旁路
}
```

**硬约束**:

0. 🔴 **四个 `On*` 回调必须声明为 `static`** —— `CommandInputHooks.Register(ImeInputHost)`
   注册的是**类对象本身**, 而 AHK v2 类对象上**实例方法的 `HasProp` 为 `false`**(实测:
   `Foo.HasProp("Inst")=0`), 非静态会让 `_Call` 的 `HasProp` 守卫**静默跳过** ⇒ provider
   从不运行且零日志。全静态、全类名引用。(对照: EverythingController 注册的是实例。)
1. 🔴 **永远不要试图查询 IME 状态来驱动可见性** —— 跨进程 IMM (HIMC 进程本地) 与 TSF
   (AHK 主线程 DllCall 不可行) 均已证伪 (见上方证伪链第 5/6 条)。可见性一律恒 V
   (启用时), 中/英文差异由物理键透传天然完成。**勿重蹈 v3 覆辙。**
2. 🔴 **词表两形态恒用 (v4.2 恢复, 不得再清空)** —— v4 曾为防「拼音误触发缩写」清空
   词表, 结果透传会话里缩写命令全部失效 (用户实测 `se` 不再打开设置面板)。用户裁决:
   **缩写命令全部由英文字母组成, 唯一需要输入中文的场景是前置键 (如空格) 之后** ——
   那时字符已被插件 (providers) 消费, DispatchChar 提前 return, 根本到不了匹配层,
   「拼音误触发缩写」在实践中不存在。残余理论边界见硬约束 7 (用户接受)。
3. 🔴 **`FuzzySuffixFire` 恒跑 (v4.2 恢复, 不得再旁路)** —— 它查 `CommandResolver`
   注册表 (与 hook 词表无关), 逐字符后缀命中即 `ih.Stop()`+执行命令, 是「打完即执行」
   的兜底通道, 必须与历史形态行为一致 (v4 旁路导致缩写失效)。搜索期不误触发的双保险:
   ① 词表 MatchList 是**全串精确**匹配, 检索词 Input 形如 `" se"` (带空格前缀) 永不
   等于 `"se"`; ② 插件消费字符后 DispatchChar 提前 return, Fuzzy 根本不进 (回归守门:
   check-hooks 第 12 组两条断言)。
4. **providers 派发在透传模式下保留** —— 插件 OnChar/OnKey 消费路径不受透传影响
   (内置插件均经 OnKey 触发, 无 OnChar 消费者)。v3 的「OnChar/OnKeyDown 顶部整体
   旁路」已废弃: 它会连 everything_search 的空格触发一起杀掉。
5. **Up/Down/Enter/Backspace 在 V 模式下必须保持透传 (KeyOpt 仅 "N" 无 "S")** —— IME
   组合期它们是选候选/翻页/确认拼音原文/删组合串的原生操作, 吞掉即毁 IME 交互;
   CapsLock 与 Esc 为 EndKey + "S" (抑制透传防切大小写/防触发 exe 原生行为, EndKey
   检测不受 S 影响)。
6. 🔴 **透传会话必须显式激活命令框窗口, 激活失败必须降级** —— 窗口 NOACTIVATE, SHOW 不
   带键盘焦点; 不激活则物理键全部漏进原窗口 (2026-09-19 用户真机实测: 英文无法输入)。
   落点: `EnterCapslockAbbr` 在 SHOW 与 MakeCapsHook 之间调用
   `CommandDisplay.ActivateCommandWindow()`, 返回 false ⇒ `SuppressKeycap := false`
   (降级历史形态)。`ActivateCommandWindow` 自身不得触碰 SuppressKeycap (单一职责,
   check-hooks 第 13 组)。
7. **已知局限 (语义正确, 非缺陷)**: IME 开启且**未按前置键**直接打拼音时, 拼音全串
   (MatchList) 或其任一后缀 (FuzzySuffixFire) 若恰好等于某缩写会触发执行 —— 用户裁决
   接受: 缩写全为英文字母, 中文输入只发生在前置键之后 (那时字符已被插件消费, 不进
   匹配层), 该场景概率极低; everything_search 的空格前置触发与 IME 选字键共用物理
   空格 —— 中文组合期按空格既选字也可能触发插件下拉 (接受, 边缘场景, Esc 可收起)。
8. **check-ime 判据**: `tools/ime_input_test.ahk` 断言**无 V 时**收不到中文码点 ——
   该事实仍成立且是本方案的理论基础 (故保留为回归闸门)。「断言变红 ⇒ 模块退役」的
   旧判据作废: 变红说明 AHK 默认形态已透传 IME, 应改断言而非退役。
9. 🔴 **命中的终止字符必须经 `EchoTerminalChar` 强制投递** (2026-09-20 用户实测缺陷)
   —— 命中这一击的字符**不会被原生显示** (会话就在这一击结束), 它是该字符唯一的显示来源;
   而透传模式下 `ShouldEcho` 恒 false ⇒ 若走 `EchoChar` 就等于不投递 ⇒ 用户看到「最后一个
   字母不显示」。故 `CommandDisplay.EchoTerminalChar(c)` = **唯一允许绕过 ShouldEcho 的回显
   通道** (由本模块自带, 是第 1 条「回显唯一收口」的受控例外; 调用方仍不得直调底层 `Post*`)。
   两条命中路径都经它: 全串命中 (`EnterCapslockAbbr` Match 分支, **无条件**投) 与模糊命中
   (`FuzzySuffixFire`, **仅透传模式**投 —— 历史形态那边已由 OnChar 的 EchoChar 投过, 再补会双显)。
   ⚠ **`EchoChar(ih, c)` 两个实参都必须传**: 首参是历史遗留参数 (只转发给
   `PostCharToCaspAbbr`, 后者并不消费), 曾因按历史写法省成 `EchoChar(, char)` 而每次命中都抛
   `Missing a required parameter.` 并被 catch 吞掉 (铁证 = 部署树 `logs\command_input_hooks.log`
   连发 `EchoChar(Match) 异常`) —— 这是本缺陷的**第一层**成因 (第二层 = 透传模式把它兑停)。
   ⚠ 命中后的执行/隐藏**延后** `CommandInputHooks.FinishDelayMs`(现 30ms ≈ 2 帧) —— 让刚投递的
   字符先被绘制 (投完立刻执行+隐藏会把它吃掉)。首版取 150ms 被用户判为「太慢」, 已下调到
   感知上等同上屏瞬间执行; **勿改回当场执行, 也勿设 0** (≤1 帧有丢字符风险)。
   回归守门: check-hooks 第 15 组 (终止字符两形态投递 + 与 EchoChar 的对照) 与第 14 组
   (`TakePending` 一次性消费 / `BeginSession` 复位 / 延迟区间)。

回归守门人: `tools/command_input_hooks_test.ahk` (75 项, 含 v4.2 恒跑/消费即停守卫 +
v4.1 焦点降级语义 + 延后收尾状态 + 终止字符强制投递语义) + `tools/ime_input_test.ahk` +
`make check-ime` (手动, 需退出 KeyFlux)。
⚠ 透传端到端 (V → IME 组合 → 上屏 WM_CHAR) 无法在 AHK 探针内完整自动化, 由用户真机验证。

### 3.13 EngineOnError —— 引擎级未捕获异常兑底 (2026-09-19 冻结)

**背景**: 无 OnError 时, 热键/Timer 线程的未捕获异常 = 错误弹窗 + 线程死亡; 若发生在
命令框会话中 (`StartInputHook` 已 `Suspend(true)` 而永远走不到恢复), **全部热键随线程
死亡而失效** —— 用户视角即「报错弹窗 + 命令框卡死, 无法关闭也无法输入」(2026-09-19 实测)。
实测依据: `PostMessage` 到已消失的命令框窗口会抛 `TargetError`(KF_diag2 探针);
`EchoChar`/`FuzzySuffixFire`/命令体执行原先都在热键线程**裸奄**, 任何一步抛出即触发上述现象。

**双层修复**:
1. **源头包裹** (CommandInputHooks.ahk / type9_keyflux.ahk): `CommandInputOnChar` 里的
   `EchoChar`、`FuzzySuffixFire`, `CommandInputOnKeyDown` 里的 `EchoBackspace`, 以及
   `EnterCapslockAbbr` Match 分支的 `EchoChar` 与 `ExecCapslockAbbr`(命令体), 全部 try 包裹
   → 异常记入 `logs\command_input_hooks.log`, 命令体失败另给 Tip 提示。
2. **兑底网络** (Functions.ahk `EngineOnError`, 模板在 auto-exec 早期 `OnError` 注册):
   记录 `Type/Message/File/Line/Stack` 全文到 `logs\engine_error.log` (UTF-8) +
   `Suspend(false)` 复位残留暂停态 + Tip 提示 + 返回 1 压制弹窗。实测语义: 热键/Timer
   线程异常 ⇒ 线程终止、无弹窗、脚本其余部分继续; auto-exec 线程 ⇒ 退出但日志已落盘。

**契约**: 任何新加的热键/Timer/输入回调代码, 不允许存在会向外抛异常且无 try 包裹的外部调用
(文件 IO、PostMessage、Run、用户命令体); 新 provider 的注册对象若是**类**而非实例,
其 `On*` 方法必须 `static`(见 §3.12 硬约束 0)。排查命令框故障时先看 `logs\engine_error.log`。

## 4. 插件清单格式(冻结)

`data/plugins/<id>/plugin.json`:

```json
{
  "id": "everything-search",
  "name": "Everything 本地搜索",
  "version": "1.0.0",
  "runtime": "ahk",
  "entry": "main.ahk",
  "provides": { "actions": ["everythingSearch"], "abbrCommands": ["fs"] },
  "permissions": ["selection", "run", "settings"],
  "settings": {
    "everythingPath": { "type": "path", "label": "Everything.exe 路径" }
  }
}
```

- `runtime`:`"ahk"` = L1 进程内(当前支持);`"process"` = L2(预留,清单可声明,加载器报"暂不支持")
- `permissions` 词表(冻结):`selection` / `run` / `clipboard` / `window` / `settings` / `events`
- L1 插件入口约定:`main.ahk` 必须导出 `Register(api)` 函数,由 PluginManager 调用

## 5. Go 生成端契约

- 阶段 3 起,`config-server/internal/script/action.go` 的 `actionMap` 拆入
  `generators/` 目录,每个 TypeID 一文件;
  **已完成(阶段 3)**:数据模型在 `model/` 包,9 个 TypeID 渲染函数各一文件,
  原文件已删除,外部调用经 `script` 包类型别名兼容,生成产物逐字节回归通过;
- 生成器新增义务:可输出 **注册计划 JSON**(`settings.exe DumpPlan <config> <out>`),
  与 AHK 运行时加载器的注册计划 diff(Oracle 机制);
  **Go 侧已实现(阶段 3)**:`generators/plan.go` 的 `BuildPlan` 与渲染路径同源推导,
  输出确定性已验证(两次运行逐字节一致);
  **AHK 侧已对接(阶段 4)**:`CommandResolver.DumpAbbr` 导出运行时实际注册表,
  与 DumpPlan 的 abbr 段对比命令集 + 步骤数, 首次运行即 PASS(动作参数级对比留待阶段 5);
- ~~`preprocess()` 的 `!f17` 注入逻辑缺陷~~(已证伪):`Preprocess` 遍历逻辑本身正确;
  真实缺陷是 `GenerateAHK` 验证命令不调用 `Preprocess`(仅运行时 `GenerateScripts` 调用),
  导致验证产物缺 `!f17`,此前"`!f17` 为手工行"的认知作废。
  阶段 3 已修复:`Preprocess` 导出,`GenerateAHK` 对齐运行时路径,
  修复后生成产物与部署运行产物**逐行完全一致**(已验证);
- 遗留代码载荷(`ahkCode` / `ahk-expression:` / `ahk:` 行 / conditionType 5 表达式)
  继续由 Go 编译,输出到 `compat/` 消费格式,直至用户迁移为插件。

## 5.1 settings.exe `--headless` 模式契约(2026-08 冻结)

供 Avalonia 原生设置壳(`config-ui-avalonia/`)以子进程方式拉起后端,与既有模式共存:

- **启动方式**:`settings.exe --headless`(工作目录约定与既有启动方式一致: 部署根目录, 与无参模式相同);
- **端口通告**:stdout **首行**输出 `KEYFLUX_PORT=<数字>`(实际监听端口),无任何其他装饰输出;
  GUI 壳逐行读取并匹配 `KEYFLUX_PORT=` 前缀获取端口;
- **端口回退**:默认尝试 12333,被占用时回退随机可用端口并**如实通告**(通告行永远反映真实监听端口);
- **行为差异(仅此三项)**:跳过代码雨动画、跳过自动打开浏览器、不打印 "KeyFlux config server is running..." 装饰行;
- **不变项**:全部 HTTP 路由与响应格式、配置写盘(仍由 Go 后端唯一负责, 写盘格式不变)、
  无参 / `debug` / CLI 子命令(如 `DumpPlan`/`GenerateAHK`)行为完全不受影响。
- **进程生命周期**:由 GUI 壳管理(命名 Mutex 单实例、Job Object 强杀兜底),后端自身无感知。
- 实现:`config-server/cmd/settings/main.go`(模式判定/代码雨跳过) + `config-server/internal/server/server.go`(端口回退与 `KEYFLUX_PORT=` 通告);
  入口:`bin/lib/core/Functions.ahk` 启动 `bin\ui\KeyFlux.Settings.exe`;
  构建:`make buildClientAvalonia` → `bin/ui/`(`.gitignore` 忽略)。
- 部署实测(2026-08, beta33):GUI 打开/关闭/进程回收正常, 配置哈希不变。

## 5.2 Go HTTP 层双轨 DTO 边界 (2026-09-03 登记)

| 端点 | 序列化路径 | 备注 |
|---|---|---|
| `GET /config` | model → `ConfigToDTO` → DTO → gin JSON | DTO 排除 `json:"-"` 计算态字段 |
| `PUT /config` | gin JSON → DTO → `DTOToConfig` → model → 校验 → 落盘 | 落盘仍走 model, 生成器输入不变 |
| `POST /api/selected-action/test` | **直接序列化 model** (未经 DTO) | 方案 D 后仅剩模拟测试端点 (旧 action-schemes CRUD 六路由已移除, 存量配置读时一次性迁移为顶层 selectedAction) |
| `GET /shortcuts` | 内联结构体, 无 model 依赖 | 无需 DTO |

**改 json tag 须同时改两处**: `internal/script/model/types.go` (存储模型) 与 `internal/server/dto.go` (传输 DTO);
漏改任一侧会导致 GET/PUT wire 不一致或落盘字段丢失。

**三处同步清单 (实测教训)**: 新增 `options` 子段时须**同时**改动 ① C# `config-ui-avalonia/Models/ConfigModels.cs`
的 `Options` ② Go `model/types.go` 的 `Options` ③ Go `dto.go` 的
`OptionsDTO` + `optionsToDTO()` + `dtoToOptions()` **五个落点** (DTO 结构体 + 两个转换函数各一处)。
漏 `optionsToDTO` 会让 GET 少字段, 漏 `dtoToOptions` 会让 PUT 静默丢字段 (G1 教训)。
⚠ 历史残留: `options.acrylic` **只在 model 与 C# 侧存在, dto.go 里没有对应分支** —— 属已知的历史缺口
(该段纯 UI 呈现、引擎不消费, 故未暴露为故障); 新增段**不应**照抄这个缺口, 必须走全五处。
现成参照实现: `options.commandFont` (2026-09-20)。

守护测试: C# 侧 `ModelUnitTests.Options_And_SubStructs_JsonNames_MatchGoTags` 锁键名集合;
Go 侧 `server/dto_test.go` 的 `Test<X>RoundTrip` 锁 PUT→model→GET 往返与空段恒对象契约。

**RestartFailed 双份定义 (残留耦合, 登记为后续项)**:
`model.ActionScheme.RestartFailed` 与 `ActionSchemeDTO.RestartFailed` 同时存在。
action-scheme 端点直接在 model 上设置该字段后序列化返回, 未经 DTO 转换;
移除 model 侧定义需先将 action-scheme 端点 DTO 化, 属后续清理范围。

## 5.3 契约测试前置二进制契约 (2026-09-03 冻结)

`KeyFlux.Settings.Tests/Infrastructure/SettingsTestServer.cs` 拉起 headless settings.exe 子进程,
其前置二进制路径为:

```
%TEMP%\mk_settings_headless\settings.exe
```

- **产出命令**: 等价于 `make buildServer` (go build -tags=nomsgpack -ldflags "-s -w -X settings/internal/script.KeyfluxVersion=$(version)" -o ../bin/settings.exe ./cmd/settings), 然后复制到上述路径;
- **陈旧度要求**: 该二进制的 SHA256 必须与当前 `bin/settings.exe` 一致 (即本轮构建产物);
  若不一致, 契约测试拿旧后端跑出假绿, 等于没测;
- **覆盖时机**: 每次 `make buildServer` 后、运行 `make check-cs` 前, 须手动或自动覆盖;
- **C 盘只读例外**: 此路径位于 `%TEMP%`, 是 C 盘只读约束的唯一既定例外。

## 6. 未来功能落点(不在本次实现)

| 功能 | 落点 |
|---|---|
| 1. 命令模糊输入 | **已实现(2026-08-22)**: 逐字符实时后缀校验 `FuzzySuffixFire`(接在 `InputHook.OnChar`,最长后缀优先);命中即停钩执行,`abbr_submit.fuzzy=true`。`Strategy` 桩保留给未来的编辑距离≤1/候选提示类策略 |
| 2. Everything 搜索 | 首个官方插件 `everything-search`(含 `everythingPath` 设置项与自动启动逻辑)——**阶段 5 裁定推迟到全部阶段完成后**;框架已就位(§3.7) |
| 3. 外接脚本/函数 | L1 插件 = `custom_functions.ahk` 正式化;长任务走 ScriptHost |
| 4. 开放 API/插件市场 | 双层插件体系 + 权限化 APIBridge;内置同接口倒逼 API 完备 |
| 5. Rust 重写 | IKeyEventBus 抽象已就位;Rust 守护进程 = 总线实现 + L2 宿主 |

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-22 | 骨架版建立(方案 D 定稿),全部接口签名冻结 |
| 2026-08-22 | 阶段 3/4/5 完成注记; 阶段 5 裁定 Everything 插件推迟 |
| 2026-08-22 | 阶段 6: EventBus 落地并接入 5 处发布点; L2 JSON-RPC 协议文本冻结; 模板补记阶段 4 格式 |
| 2026-08-22 | §6 第 1 条落地: 命令模糊输入(逐字符后缀校验, 最长优先, CapsLock 域); `Resolve` 增 `fuzzy` 可选参数(默认不变); `config-server/templates` 副本补齐阶段 6 include |
| 2026-08-23 | 修复存量缺陷: `ActivateWindow` 谓词排除桌面壳窗口 (Progman) —— 此前 `ahk_exe explorer.exe` 类匹配把常驻桌面窗口当成"已打开的窗口"提前返回, 导致 fe 等命令在无真实窗口时永远不启动目标程序 (冷启动失效); 实证与模糊输入功能无关 |
| 2026-08-23 | 死代码/过时代码审计与清理 (零行为变更): ① 删 `Functions.ahk` 孤立工具 `MapFindKey`/`Join` (零调用); ② 删 Go 端 `AbbrToCode` 生成器及其 FuncMap 注册 (阶段 4 已被 `AbbrRegistryCode` 取代, 模板零引用) + `command.go` panic 后不可达 return (go vet); ③ 删 config-ui 脚手架残留 `HelloWorld.vue` (零 import); ④ 部署目录 `bin/lib/` 平铺旧库 7 文件 (重构前布局残留, 仅基线文档引用) 备份后删除, 同步 `ActionRegistry.ahk` 部署副本至 phase6 版 (留桩, 不在运行链) |
| 2026-08-23 | 设置入口启动延迟优化 (零行为变更): ① `openBrowser` 删除端口就绪后固定 600ms 延迟 (net.Listen 成功后端口已可接受连接, 实测 Go 业务初始化仅 ~20ms); ② `KeyFluxOpenSettings.launchSettings` 改为 wt 直接运行 `settings.exe` (绝对路径) 绕过 pwsh -NoExit 层 (实测启动链热 wt 660ms→~200ms, 窗口行为不变); 实测基线: 热场景全链 ~1465ms→~405ms, 冷场景 ~2480ms→~1150ms; GenerateScripts 产物逐字节一致 |
| 2026-08-23 | 开机自启机制迁移 (零行为变更): ① `MiscTools.ahk` `RunAtStartup` 由启动文件夹快捷方式改为 HKCU\Run 注册表键 (值 `KeyFlux` = 带引号的 exe 全路径, RegWrite/RegDelete 读写); ② 旧 Startup\KeyFlux.lnk 已删除且不再重建; ③ 新增全中文 `卸载软件.bat` (二次 Y 确认; taskkill 相关进程容错; 删注册表自启值; 删旧 lnk; 删目录前检测主程序防误删; 目录删除后由 %TEMP% tail 副本输出总结, tail 以 exit 终止避免读已删文件报错); 开关往返实测无残留; GenerateScripts 产物逐字节一致 |
| 2026-08-24 | 文件分组并入文件后缀 (fileGroups 配置化): ① 删除「文件分组」匹配类型 —— Go `matchFileGroup`/`fileGroupExts`、AHK `MatchFileGroup`/内置表、前端 MATCH_TYPES 项/RuleEditor 控件/RuleList 分支、类型注释全清, 运行时引擎只认 fileExt 后缀列表; ② 分组表收敛为配置数据 `config.json` 新增 `fileGroups` 段 (name/label/exts, 默认 6 组, 用户可自行增改), Go `Config`/`model.FileGroup`/前端 `FileGroup` 类型同步, `SaveConfigHandler` 新增 `ValidateFileGroups` 结构校验 (名称/显示名/后缀非空, 非法拒绝保存); ③ 前端「文件后缀」条件值下方新增「常用分组快捷填入」下拉 (数据源=配置 fileGroups, 选择分组展开为逗号分隔后缀列表, 可继续手改; 配置缺失时不显示, 降级为纯手输); 存量配置零 fileGroup 规则故无迁移; 生成链路 (actionSchemesCode 原样透传规则字段) 与 Oracle plan 输出不受影响 |
| 2026-08-23 | 修复存量缺陷: 选中动作热键触发报 Too many parameters (RunActionScheme) —— 热键回调 `handler(thisHotkey)` 经 `RunActionScheme.Bind(scheme)` 调用时, BoundFunc 把绑定参数前置并追加调用参数, 实际以 2 参数调用只定义 1 参数的 `RunActionScheme(scheme)`; 修复为签名加默认参数 `RunActionScheme(scheme, trigger := "")` 吸收追加参数 (闭包捕获 for 变量有指向最后一方案陷阱, 故不用箭头函数改注册) |
| 2026-08-23 | 选中动作功能改造: ① 移除「文本正则」匹配类型 (Go 匹配分支 / AHK case / 前端选项 / 类型注释全清); ② 「文本特征→行为类型」动态联动 (textTypeActions 单一真源: url→[open_url,search], path→[open_path,open_folder], magnet→[magnet_download], plain→[open_registry,search,run,send_keys,script,copy]; Go 端保存/建/改/测四入口校验非法组合拒绝并中文提示, 前端动态渲染+自动纠正+导入校验); ③ 新增 5 类文本特征行为 open_url/open_path/open_folder/magnet_download/open_registry (全走系统默认关联: Run() ShellExecute / magnet: 协议处理检测失败给中文提示 / regedit LastKey 定位, 零第三方依赖零提权); ④ 修复 CheckMagnetHandler try 无 catch 导致 magnet 未注册时异常弹窗; GenerateScripts 产物逐字节一致, /Validate exit=0, Oracle 35/35 PASS |
| 2026-08 | 新增 §5.1 `settings.exe --headless` 模式契约并冻结: 供 Avalonia 原生设置壳子进程拉起, stdout 首行 `KEYFLUX_PORT=<端口>` 通告 (12333 占用回退随机端口并如实通告), 跳过代码雨/浏览器/装饰行, 路由与配置写盘与无参模式完全一致 |
| 2026-08 | 设置界面原生窗口化 (零行为变更, 旧浏览器版保留): ① 新增 `config-ui-avalonia/` (Avalonia 11 / .NET 10 / CommunityToolkit.Mvvm, 完整移植键位图/自定义热键/缩写/选中动作/设置/主页全部页面) 与仓库根目录 `KeyFlux.Settings.Tests/` (74 个单元测试全绿); ② Go 后端新增 `--headless` 模式 (§5.1); ③ Makefile 新增 `buildClientAvalonia` (dotnet publish 自包含 win-x64 + ReadyToRun → `bin/ui/`, 已入 `build` 依赖链), `.gitignore` 追加 `bin/ui/`; ④ AHK 设置入口 (`bin/lib/core/Functions.ahk`) 改启动 `bin\ui\KeyFlux.Settings.exe`; ⑤ 配置写盘权不变: GUI 仅经 localhost HTTP 调用, Go 后端保持 config.json 唯一写盘权; 部署实测: GUI 打开/关闭/进程回收正常, 配置哈希不变 |
| 2026-09-03 | Functions.ahk 按职责拆分 (零行为变更): 35 个函数逐字节搬运至 core/{Programs,WindowUtils,AbbrInput}.ahk, Functions.ahk 仅保留托盘生命周期/选中文本/编码杂项 (671→205 行); 模板 include +3; oracle.ps1 harness 同步新 include 并自包含 settings.exe 拷贝; Makefile 新增 check/check-cs/deploy 目标 (一键回归: Go 单测+GenerateAHK+/Validate+Oracle; 一键部署: 同步部署目录并重启实例) |
| 2026-09-03 | 仓库卫生 + Makefile 部署源修正 (零行为变更): ① 维护者发布脚本 git mv 归位 `scripts/` (build_tools.go / lanzou_client.py, 不放 tools/ 因其随发布包出货), Makefile uploadLanZou 三处引用同步并顺带修 python3→python (本机无 python3); 脚本内部路径全部 cwd 相对, 移动后 0 改动; ② 修 Makefile:103 deploy 的 UI robocopy 源 bug: `config-ui-avalonia/bin/ui` 是旧自包含发布残留, 正牌输出为 buildClientAvalonia 写入的 `bin/ui`, 原写法会把部署目录 UI 用 /MIR 降级为旧构建 → 改为 `bin/ui`; ③ `bin/lib/Monitor.ahk` 头部加 KeyFlux 侧注明块 (仅 ; 注释, 代码零改动): 唯一消费者 ChangeBrightness.ahk、跨进程拉起链 type2_system.ahk BrightnessControl→TypeID2、实际使用面 Monitor()/GetBrightness/SetBrightness 及 13 方法传递闭包、未使用面约 580 行为保持与上游 tigerlily-dev v2.4.1 可 diff 而刻意保留 |
| 2026-09-03 | I18n 字典外置 (零行为变更): `config-ui-avalonia/Services/I18n.cs` 的 440 行内联字典外置为 `Resources/i18n.json` (308 键, UTF-8 无 BOM), I18n.cs 降到 184 行只留加载器与 `T()`; csproj 新增 `Content` 项 (`CopyToOutputDirectory` + `CopyToPublishDirectory` 双元数据), 这是本项目首个松散部署物 (此前 AvaloniaResource 全打进 dll), publish 后落在 `bin\ui\Resources\i18n.json`, 随 Makefile:103 的 `robocopy bin/ui … /MIR` 自动同步到部署目录; 新增 7 项守卫测试 (键数 308、axaml/cs 键覆盖对账、null 与空串语义、占位符与转义保真、双语回退、Language 归一化、无 BOM), C# 测试总数 114→121 |
| 2026-09-03 | cmd 瘦身 + API DTO 分离 (零 wire 变更): ① 任务1——`cmd/settings/main.go` 的 HTTP 层 (server() 函数体、11 条路由、全部 handler、PanicHandler、syncStartupFromRegistry) 与 `actionscheme.go` 整体迁入新建 `internal/server/` (server.go / handlers.go / actionscheme.go), `execCmd`/`fallbackExecCmd` 下沉为独立 `internal/proc/` 包 (main 与 server 共同引用, 避免循环依赖), cmd/settings/main.go 从 355 行瘦身到 61 行只留入口与模式判断; ② 任务2——新建 `internal/server/dto.go` 定义 Config 及全部嵌套结构的 DTO + 双向映射 (model→dto 供 GET, dto→model 供 PUT), GET/PUT /config handler 改为只与 DTO 打交道, 排除 json:"-" 计算态字段 (KeyMapping / RemapInHotIf) 进入 wire; RestartFailed 保守保留在 model (action-scheme 端点仍直接序列化 model, 残留耦合登记为后续项); 验证: wire 三端点逐字节一致、GenerateAHK 产物 SHA256 不变、/Validate exit=0、Oracle PASS、C# 契约测试 121 全绿 |
| 2026-09-03 | 生成器回归网 (golden test): 新增 `internal/script/golden_test.go` + `testdata/golden.keyflux.ahk`; 落点选 `internal/script/` 而非 `generators/` 因 golden test 调用 `SaveAHK`/`Preprocess` (属 script 包导出), 放 generators 会产生 script↔generators 导入环; 刷新方式: `UPDATE_GOLDEN=1 go test ./internal/script/...`; 合成配置规避 map 迭代序非确定性 (约束 1: sortHotkeys 非稳定排序; 约束 2: handleKeyRemapping SliceStable 保留随机序) |
| 2026-09-03 | 三维评审非阻断修复批次 (Go/Makefile/AHK/文档, 零行为变更): ① `bin/lib/Monitor.ahk` 注明块裸行号全部改为符号锚点描述 (对上游 diff 更 robust, 消除 +25 行偏移导致的 ~15 处行号失效); ② `internal/server/dto.go` keymapToDTO/dtoToKeymap 内层 Hotkeys value slice 补 nil 守卫 (修复 null→[] 往返非恒等), 新增 `dto_test.go` 表驱动测试; ③ `internal/script/golden_test.go` BOM 断言从 normalizeAHK 归一化中拆出为独立 bytes.HasPrefix 检查 (消除产物 BOM 丢失不可观测盲区); ④ Makefile buildClientAvalonia 后新增 i18n.json SHA256 断言 (publish 产出与源不一致即 exit 1); ⑤ CONTRACTS.md 补全: §2 目录树补 i18n.json 双路径 + 契约条目、§5.1 实现指针更新、新增 §5.2 双轨 DTO 边界 + §5.3 契约测试前置二进制契约; 并行 C# 侧修复 (归属任务 #65): I18nResourceTests 物理存在断言、SettingsTestServer 陈旧度断言、I18n.cs 头部注释补回 |
| 2026-09-04 | 移除选中动作「默认 (兜底)」匹配类型: ① Go `matchActionRule` 删 `case "default": return true` (仅剩 fileExt/textType, 落空 return false), AHK `MatchActionRule` 同步删 case; ② 前端 MATCH_TYPES 词条、default→\* 条件值分支、RuleList default→「任意内容」展示分支、兜底规则不在末尾警示 (DefaultRuleNotLast/RefreshDefaultWarning + axaml 警示 Border) 全清; ③ i18n 删 4 键 (979/998/1033/1036) 键数守卫 309→305; ④ 存量 default 规则将不再命中 (历史规则在编辑器中回退显示为第一项, 用户重新选择即完成迁移); ⑤ golden 夹具删两条 default 规则并重刷基线 (仅 matchType: "default" 渲染行消失); readme/readme.en 匹配类型改两类表述 |
| 2026-09-05 | 行为包体系一期落地 (§3.9 冻结): 11 个内置行为打包为只读内置包 (bin/behaviors 入库), 规则 ActionType 语义升级为行为 ID (内置 ID 直通=存量零迁移), internal/behaviors 包 (加载/覆盖/删除约束), 保存校验统一为覆盖检查 (取代 textTypeActions 静态表+修复首条短路缺陷), API 5 端点 (GET/POST/PUT/DELETE /api/behaviors + apply 显式重启), 生成期展开 (渲染+plan 镜像); 行为库前端窗口与 C# 目录服务为二期提交 |
| 2026-09-05 | 行为库前端落地 (一期收口): C# BehaviorCatalog (GET /api/behaviors 快照+覆盖/默认/显示名推导), 旧五张静态词表 (ActionTypes/TextTypeActions/TextTypeDefaultAction/TextActions/FileGroupActions/FileGroupDefaultAction/FileActions/DefaultSearchUrl) 全部退役, 行为库窗口+编辑表单窗口 (新建/编辑/删除/立即生效), 编辑页「管理行为…」入口; i18n +24 键 (守卫 305→329); 行为目录测试夹具+7 个 BehaviorCatalogTests, 测试服务器补 behaviors staging; dotnet 154/154, 部署 beta33 (后端+UI+内置包) 并 API 冒烟通过 |
| 2026-09-06 | 方案 D 重构落地 (选中动作单键分发): ① **§3.1 `selection_action` 事件载荷变更 (对插件作者破坏性)**: `{schemeId, ruleIndex, selected}` → `{behavior, name, selected}` (schemeId/ruleIndex 随多方案模型退役, behavior=行为库 ID, name=显示名), 订阅该事件的插件须改读新字段; 发布点迁至 `SelectedAction._Execute`; ② §5.2 旧 action-schemes CRUD 六路由移除, 新增 `POST /api/selected-action/test`; 存量配置读时一次性迁移为顶层 `selectedAction` (actionSchemes 键不再输出); ③ 生成端渲染函数 `actionSchemesCode`→`selectedActionCode` (generators/actionscheme.go), AHK 端 `InitActionScheme`→`SelectedActionInit` / `MatchActionScheme`→`MatchSelectedAction` (bin/lib/rules/SelectedAction.ahk 重写入口与分发层, 匹配原语与执行辅助原样保留) |

| 2026-09-10 | 删除「注册表定位」独立行为并集成进「打开文件/程序 (系统关联)」: AHK ExecuteActionRule 的 open_path 分支前置注册表路径检测 (HKEY_* 全称或 HKCR/HKCU/HKLM/HKU/HKCC 缩写) 命中走 OpenRegistryKey (LastKey 定位, 实现不变); 删 bin/behaviors/open_registry/ 行为包; 前端 BaseActionNoValue/颜色映射、Go BuiltinActionIDs/PreviewAction 清理; plain 覆盖集 6→5; C# 208/0, Go go test -count=1 全过, /Validate 冒烟 exit=0 |
| 2026-09-17 | 新增内置文本特征「B 站」(`bilibili`): AV 号 (`av`+数字) 或 BV 号 (`bv`/`BV`+10 位 `[0-9A-Za-z]`), 必须**整串**命中 (正则用 `\z` 收尾而非 `$` —— Go 的 `$` 只认文本末尾而 PCRE2 的 `$` 还认末尾换行前的位置, 两端会分歧; 首尾不 Trim 因该值会被原样拼进视频 URL); **plain 语义变更 (需周知)**: 由「非 url/path/magnet」扩为「非 url/path/magnet/bilibili」⇒ 形如 av/BV 号的选中文本不再命中 plain 映射 (既有规则不失效, 但该类文本需新建 B 站映射才会命中; 不改此语义则数组行序会让先建的「纯文本」映射恒遮蔽后建的「B 站」映射, 因「添加映射」恒追加到末尾); 三端同批: Go (`behaviors.KnownTextTypes` / `script.matchTextType` / `script.reservedTextTypeNames` / 两处错误文案)、AHK (`bin/lib/rules/SelectedAction.ahk` 的 `MatchTextType`)、C# (`ActionSchemeCatalog.TextTypes` 单一静态词表 + `MappingRowVm.IsBilibili` + 映射行 Toggle 5 选 1); 新增内置行为包 `bin/behaviors/open_bilibili` (entry builtin=`open`, 模板 `https://www.bilibili.com/video/%selected%`, 走系统默认 https 关联, 不硬编码浏览器); i18n 新增 2580 (`B 站`) 守卫 385→386; 新增 Go 测试 `match_texttype_test.go` (特征语义表 + 读**真实** `bin/behaviors` 的覆盖/默认守卫, 防"特征新增却漏建行为包"导致添加映射弹窗空列表死胡同); `TextTypeToggleExclusiveTests` 4→5 |
| 2026-09-17 | 内置文本特征改为**声明式注册表** (组织方式/接入点/边界详见 `docs/design-text-feature-registry.md`): 真源 = 新增 `config-server/internal/behaviors/textfeatures.go` (`TextFeature{Value,Label,Named,Fallback,IgnoreCase,Pattern}` 有序表, `init()` 自检编译), **plain 的排除集由具名集派生** (兜底项不持正则, 命中条件 = 其余具名特征全不命中 ⇒ 新增特征自动扩大排除集, 消灭"人肉补 `!新特征`"失败模式); `script.matchTextType` 退化为分派壳, `reservedTextTypeNames` / `behaviors.KnownTextTypes` / 两处错误文案 (`TextFeatureHint`) 全部改派生; **API 变更**: `behaviors.KnownTextTypes` (map 变量) → `behaviors.IsKnownTextType(v)` / `FindTextFeature` / `TextFeatureValues` / `TextFeatureLabels` / `TextFeatureHint` / `MatchTextFeature`; AHK 端同构: 新增 `TextFeatureSpecs()` 表 + `TextFeatureHit()`, `MatchTextType` 改表驱动 (重构前后 63 用例 × 5 特征 = 315 次求值**逐位一致**, diff 空); C# 端 `ActionSchemeCatalog.TextTypes` 注释重写 + 新增 `FallbackTextType` 常量, `TextTypeToggleExclusiveTests` 的 Toggle 数量断言改由 `TextTypes.Length` **派生** (漏加 Toggle 即红); **双端契约升级**: 新增共享一致性向量 `config-server/internal/script/testdata/text_types.json` (types 顺序 + 63 条用例 expectTypes **全集**), Go 侧 `texttype_vector_test.go` 消费 + 覆盖守卫 `TestBuiltinCatalog_CoversEveryTextFeature` 改注册表驱动, AHK 侧由新增 `tools/texttype_conformance.py` 消费 (静态对账两侧注册表 + **逐字提取**真实函数体跑运行时对账 —— 修正 `SelectedAction.ahk` 声称由 match_ops.json 守护但该向量只有 Go 侧消费的无强制契约), 挂入 `make check-texttypes` → `make check`; 界面侧新增 `TextFeatureRegistryConsistencyTests` 把 C# 镜像钉进同一向量; Go 全过, C# 264/264 (261→264), analyzers/lint_ident/`/Validate` 全过 |
| 2026-09-17 | 清除「选中动作 / 插件 / Settings」三页组件框的**悬停灰描边** (纯视觉, 零 API/DB/route/protocol 变更): 三页卡片悬停原都会出 1px `#d1cfc5` 灰线 —— SelectedAction 走 `BorderBrush`→`ClaudeRingWarmBrush` (`Border.actionCard:pointerover` + `Border.row-card:pointerover`), 插件 / Settings 走 `BoxShadow`→`ClaudeShadowCardHover` (该令牌内含 `0 0 0 1 #d1cfc5` 环层, 会与实体描边叠成双线); 现统一改为**不动描边色、只加深投影** —— 三页 `:pointerover` 一律改用皮肤里既有的**去环版** `ClaudeShadowCardDeep` (`0 6 20 0 #1a000000`, 无零扩散环层 ⇒ 不可能绘制描边), 灰线消失, 仍保留轻微抬起感; SelectedAction 映射行卡元素同时带 `row-card`+`actionCard` 两类, 故 `actionCard:pointerover` 已覆盖行卡, 原行卡专用灰边样式删除; `ClaudeShadowCardHover` 令牌自此无接线消费方 (保留于皮肤契约, 注释已标注); 测试: `ActionPageCardStyleTests` 灰边悬停项**反转**为「悬停描边色不变 + 投影换 Deep」, `SettingsCardEffectTests` 悬停断言换 Deep 并新增跨页 (插件 + Settings) 无灰环守护 (折叠分区内 `Bounds=0` 的隐藏卡不参与悬停); C# 265/265 (264→265), analyzers×2 exit 0 |
| 2026-09-17 | 三页组件框**悬停投影加深幅度增强** (承接同日「清除悬停灰描边」, 纯视觉, 零 API/DB/route/protocol 变更): 用户反馈悬停的深浅变化**不够明显**。算术根因: `ClaudeShadowCardDeep` 原为**单层** `0 6 20 0 #1a000000` (α10%, y6/blur20), 而静止档 `ClaudeShadowCard` 是**双层** `0 2 4 0 #1c000000, 0 6 18 0 #14000000` (α11%+8%) —— 悬停的墨量与下坠都不占优, 故"有反应却看不出反应"。现 Deep 改为与静止档同族的**双层**配方: `0 4 12 0 #25000000, 0 14 30 0 #33000000` (贴边层 α14.5% 让下沿立即可辨 + 抬升层 α20%), 下坠 6→14、弥散 18→30 ⇒ 卡片下发卡阴影明显"离地"; 下沿叠加处墨量 静止≈18% → 悬停≈32% (≈1.7×), **实测渲染像素 Δ亮度 0.0085 → 0.1588 (≈18.7×)**。三页共用同一令牌 (SelectedAction `actionCard`、Plugins `pluginCard`、Settings `settingsCard`/`leftPanel`) ⇒ 自动同步、风格仍统一; 仍**不含** `0 0 0 N` 零扩散环层 ⇒ 不会复活悬停灰线 (`SkinContractTests` 有 "DoesNotContain blur==0" 守护)。新增两道专门闸门, 防"令牌被改弱但断言仍绿"的盲区 (原测试只比对 `BoxShadow.ToString()` 与令牌相等, 无法发现令牌本身比静止还轻): ① `SkinContractTests.Skin_Contract_Hover_Shadow_Is_Visibly_Stronger_Than_Rest` —— 无环层 + 墨量 Σα ≥1.4× 静止 + 下坠 maxOffsetY ≥1.4×; ② `SettingsCardEffectTests.Hover_Shadow_Darkens_Rendered_Pixels_Below_Card` —— Skia 截帧回读卡片下沿外侧 2..7 行像素, 要求悬停帧 Δ亮度 ≥0.015, 并把整改前的旧值作对照基线打印 (仅记录不断言); C# 267/267 (265→267), analyzers×2 exit 0 |
| 2026-09-17 | **取消三页组件框的悬停投影加深效果, 保留基础阴影** (承接同日「清除悬停灰描边」→「加深增强」→「降重」三轮, 纯视觉, 零 API/DB/route/protocol 变更): 用户对悬停加深幅度的反馈历经 不够明显 → 过于明显、边缘生硬 → 降重后仍不满意, 最终裁定**直接取消加深**。落地 = **删除三页 `:pointerover` 投影接线**: SelectedAction `Border.actionCard:pointerover` (原先已覆盖同时带 row-card 类的映射行卡)、Plugins `Border.pluginCard:pointerover`、Settings `Border.settingsCard:pointerover` + `Border.leftPanel:pointerover` ⇒ 悬停一律保持静止档 `ClaudeShadowCard`; `ClaudeShadowCardDeep` 与 `ClaudeShadowCardHover` 两令牌自此**均无消费方** (都保留在皮肤契约里作恢复路径, 令牌注释已标注; 皮肤顶栏清单相应改写)。聚焦环 `ClaudeShadowFocusRing` 与 `.matched` 陶土边不受影响。测试**反向调整**: ① `ActionPageCardStyleTests.All_Action_Cards_Hover_Deepens_Shadow_Without_Gray_Border` → `..._Hover_Keeps_Base_Shadow_Without_Gray_Border` (悬停断言由 "== Deep" 改为 "== ClaudeShadowCard"); ② `SettingsCardEffectTests.Hover_Deepens_Shadow_Without_Gray_Ring_On_Both_Pages` → `Hover_Keeps_Base_Shadow_...`; ③ 像素用例由「悬停必须变暗 Δ≥0.015」**反转为「悬停帧与静止帧在卡下 2..7 行采样带逐字节一致」** (更名 `Hover_Keeps_Base_Shadow_Rendered_Pixels_Unchanged`, 实测两侧亮度均 0.9575 / 字节一致 True); ④ `SkinContractTests` 的「悬停档必须重于静止档」数值契约 (墨量比 ∈[1.25,1.75] + 下坠 ≥1.4× + 弥散 ≥1×) 随加深取消而**作废**, 替换为 `Skin_Contract_Card_Shadow_Tokens_Contain_No_Ring_Layer` (`ClaudeShadowCard` 与 `ClaudeShadowCardDeep` 不得含 `0 0 0 N` 零扩散环层 ⇒ 结构上不可能画出描边; 对 `ClaudeShadowCardHover` 反向断言其仍带环, 以免两个悬停档语义混淆)。C# 267/267, analyzers×2 exit 0 |
| 2026-09-17 | 三页组件框新增**悬停光圈** (承接同日「清除灰描边」→「加深」→「降重」→「取消加深」四轮, 纯视觉, 零 API/DB/route/protocol 变更): 用户要求「悬停时组件框周围显示一圈**光圈**, 注意**不能是纯线条**」, 颜色优先橙色。落地 = 皮肤新增**光圈令牌族 4 档** (`ClaudeShadowCardHalo` / `HaloHover` / `HaloPressed` / `HaloFocus`), 在卡片原有两层投影之上**追加第 3 层无偏移大模糊层** (`0 0 16 2 <color>`) ⇒ 四周等量外扩的**弥散光晕**, 而非 `0 0 0 N` 零扩散描边环 (后者正是当日灰线事故的成因, 已明令排除); Spread 2 把光圈从卡沿推出后由 Blur 16 化开 ⇒ 有"圈"的形、无"线"的硬。颜色取 **Terracotta `#c96442`** (品牌主 CTA 色): Coral `#d97757` 是焦点环专用色且更亮, 浅米底上弥散对比不足并与焦点态语义打架; 悬停档源 alpha **0x4d (30%)** —— 首版 0x2e(18%) 经 Skia 截帧实测卡下 2..7 行 Δ亮度仅 0.0134, 逼近历史"看不出变化"档 (0.0085) 故上调, 复测 **Δ亮度 0.0358 / 暖度 Δ(R−B)=10.0** (量级参照: 0.0085=用户判"没有变化", 0.074 起=用户判"过于明显")。**四档层数强制相等 (各 3 层)**: Avalonia `BoxShadowsAnimator` 在 progress<1 时按 `oldValue.Count` 输出层数 (源码 `int cnt = progress >= 1d ? newValue.Count : oldValue.Count;`), 层数不等会让光圈在动画**末帧**突然出现/消失 (回弹时表现为投影两段跳) ⇒ 静止档用 alpha=0 同形占位层对齐, 聚焦档把既有 Coral 2px 实环前移到第 3 层 (前两层透明化, 视觉与 `ClaudeShadowFocusRing` 一致)。接线: SelectedAction `Border.actionCard` (含同时带 row-card 的映射行卡) / Plugins `Border.pluginCard` / Settings `Border.settingsCard` + `Border.leftPanel`, 均 `:pointerover` → `HaloHover` 且挂 `BoxShadowsTransition` (`ClaudeMotion.Micro` 120ms) ⇒ 光圈淡入淡出; 悬停**不动描边色、不动投影前两层** ⇒ 既不复活灰线, 也不等于"偷偷加深"。测试: ① `SkinContractTests.Skin_Contract_Hover_Halo_Is_Soft_Warm_And_Not_A_Line` (光圈层 Blur>0 / 偏移 0 / Spread∈[0, Blur/4] / 暖色 R>B / α∈(0,0x60] / 四档层数相等 / 静止档 α=0); ② `Skin_Contract_Card_Shadow_Tokens_Contain_No_Ring_Layer` 扩到 Halo 静止+悬停两档; ③ `Skin_Contract_All_Token_Keys_Resolve` 影档 8→12; ④ `SettingsCardEffectTests` 三态断言改 Halo 族 + 新增 `Hover_Halo_Transition_Is_Wired_On_All_Three_Pages` (过渡接线: 断言 `Transitions` 含 `BoxShadowsTransition` 且时长 == `ClaudeMotion.Micro`; headless 不推动画时钟故只锁接线, 同 `MotionSmokeTests` 约定) + 像素用例更名 `Hover_Halo_Lights_Warm_Pixels_Below_Card` (**实测**: 卡下 2..7 行亮度下降且 R−B 上升 ⇒ 证明橙色弥散真的渲染出来, 断言区间 (0.004, 0.12) 同时防"看不见"与"过重"); ⑤ `ActionPageCardStyleTests` 悬停断言改 `HaloHover` + 逐卡过渡接线断言。⚠ headless 读"终点态"前必须先摘过渡 (局部值优先级高于样式 Setter, 已封装 `DetachShadowTransition`), 否则读到的是插值中间值, 结果随真实耗时抖动。C# 267→269/269, analyzers×2 exit 0; **另修本批新增用例的 headless 抖动**: `All_Action_Cards_Hover_Adds_Halo_Without_Gray_Border` 在静止档偶发读到第 3 层 alpha=0x0a 的插值中间值 (期望 0x00), 表现为「单跑绿、全量红」—— 根因是过渡动画由**真实时钟**驱动而 `Dispatcher.UIThread.RunJobs()` 不推进该时钟, 且摘除 `Transitions` **不会**中止已在飞的动画, 故原来「先断言静止档、后摘过渡」的顺序留下了窗口 (悬停某卡可连带点亮与之重叠/嵌套的卡); 改为**遍历前先把全部卡片的过渡一律摘掉**, 静止档断言前把指针停到非卡片角落复位, 并新增 `SettleShadow` 用 `AvaloniaHeadlessPlatform.ForceRenderTimerTick` 逐帧结算到期望终点态 (过渡 120ms ≈ 8 帧, 上限 40 帧) 作兜底与诊断。修后**连跑 3 次全量均 269/269 全绿** |
| 2026-09-18 | 设置面板**页面改名** (纯文案层变更, 零数据/API/DB/route/protocol 变更): ① 原「总览」(导航 i18n `913`) 与页内 H1 (`939`) 统一改为**「使用指南」/ Guide** —— 该页实际渲染 `config_doc.md` 使用文档 + 底部自定义编辑区 + 回退引导, 「总览」名不副实, 且页内 H1 原本就叫「文档」; ② 原导航项「Settings」改为**「选项」/ Options** —— 整窗即「KeyFlux 设置面板」(窗口标题原 `Setting`), 页面再叫「Settings」导致同一层级「设置」指代两个范围, 「选项」不含「设置」二字且与之形成 *设置面板 > 选项* 的清晰层级 (Windows/Firefox 中文惯例)。**关键实现**: 导航 keymap `id=4` 的标题**改由 i18n 常量提供** (`MainViewModel.BuildNav` 特判 `km.Id == 4` 取 `I18n.T("2581")`), **不再读 config 的 `name`** —— 该字段会被 `config-server/internal/script/generators/generators.go:145` 写进生成的 AHK `NewKeymap(...)`, 改动将连带 golden/oracle 基线与用户 live config 迁移, 且 `id=4` 不在设置页「快捷键方案」列表内 (只列 `Id > 4`) 故无法在 UI 改名; 走 i18n 还顺带获得中英双语标题。窗口标题 `MainWindow.axaml` 由 `Setting` 改为 `KeyFlux Settings` (`OverviewEditWindow.axaml` 的 XAML 占位同步为 `Edit Guide`, 其运行时标题本就由 `I18n.T("2406")` 覆盖; 另 4 个对话框窗口仍为静态 `Title="Setting"`, 未纳入本次范围)。关联文案同步: i18n `931`/`936`/`2406`/`2407` 去「总览」化, `937` 的「设置」页引用改「选项」页; `site-assets/config_doc.md` 与 `config_doc.html`「点开 Settings 页」→「点开「选项」页」; 代码注释与 `readme.md` 全量去「总览」(31 处)。新增 i18n `2581` (选项/Options) ⇒ `I18nResourceTests.ExpectedKeyCount` 386→387 (**新增键自 2582 起**)。⚠ **踩坑**: 仅改 Go 侧注释 (`config-server/internal/script/model/types.go`) 即触发 `SettingsTestServer.AssertBackendNotStale` 前置守卫 (比较 exe 与 Go 源 mtime), 22 个端点测试全红 —— 必须 `make buildServer` 并把 `bin/settings.exe` 复制到 `%TEMP%\mk_settings_headless\` 才恢复。校验: C# 269/269, analyzers×2 exit 0, `make check` lint CLEAN / texttypes 315 次求值一致 / ORACLE PASS |
| 2026-09-18 | 首个**真实可用**的第三方插件 `everything_search` + **声明式插件设置**落地 (纯增量, 既有 API/route/protocol 零改动): ① **命令框插件拦截点** `bin/lib/core/CommandInputHooks.ahk` (新增) —— 可插拔 provider 列表, 命令框输入期的 OnChar/OnKeyDown 先过 provider, 返回 true 即消费; `keyflux.tmpl` 的 capsHook 改绑 `CommandInputOnChar/OnKeyDown` 并为 `{Up}/{Down}/{Enter}` 加 `KeyOpt(...,"N")`(无 provider 消费时不投递字符, 与历史行为一致; 半角分号钩子不受影响 —— 它是缩写提示窗, 与命令框无关)。② **Everything 插件** `plugins/examples/everything_search/`(manifest + main.ahk + src/ 六层: Messages/Settings/Providers/Search/Dropdown/Session): 命令框内按下**前置触发键**(默认空格, 由设置面板配置)时取当前选中文字, 经 **es.exe 官方 CLI** 检索, 结果以不激活的浮层下拉列在命令框正下方, ↑↓ 选择 / 回车在资源管理器打开/定位; Everything 未运行时按配置的 everything.exe 路径自动拉起(轮询 6s); 通道选型见该目录 `EverythingProviders.ahk` 文件头(WM_COPYDATA IPC 在 Everything 1.5.0.1418 上回复不稳定, SDK DLL 路线官方只支持 1.4.x —— 与 Flow Launcher 文档一致, 故取 ES CLI; `everything.exe -search` 仅作降级)。③ **声明式设置**: `plugin.json` 新增 `settings[]`(key/type/label/labelEn/default/filter/hint/hintEn/min/max/maxLength), Go `ValidateManifest` 全量校验(key 命名空间 / type 词表 char|text|number|file / label 非空 / key 唯一 / 声明 settings 必须申请 settings 权限 / 默认值自身合法); 新增 `internal/plugins.SettingsStore`(与 AHK `ConfigProvider` 同一文件同一扁平格式, 关闭 HTML 转义 + 临时文件 rename 原子落盘) 与两个端点 `GET/PUT /api/plugins/:id/settings`(PUT 逐键按同一份声明校验、**整单拒绝**、空串=删除回落默认值); C# 侧 `PluginSetting`/`PluginSettingsResponse` DTO + `ISettingsApi` 两方法 + `PluginSettingsDialogWindow`(按 settings[] 渲染 char/file+浏览/number/text 四类编辑器) + `PluginsPageViewModel.CanConfigure` 放开到「有 settings 声明的用户插件」。④ **免重启生效**: 插件在每次会话开始时重读 plugin-settings.json (`EverythingSettings.Load` 仅在值真变时返回 true ⇒ 只在必要时让通道探测缓存失效, 避免每次会话白起一次 `es.exe -version` 子进程)。⑤ **部署链**: Makefile 新增 `sync-plugins`(robocopy plugins/examples → `$(OUT_DIR)/data/plugins`, 刻意不带 /MIR ⇒ 用户自装插件不被清掉), 并挂为 `check` 与 `sync-out` 的前置 —— 生成端只扫 `<config.json 同级>/plugins`, 插件不到位则 check 会在空目录上假绿; `make check` 现覆盖真实插件注入路径。i18n 新增 6 键(2582 浏览/2583 选择文件/2584 设置加载失败/2585 保存失败/2586 无可配置项/2587 当前为空格) ⇒ `ExpectedKeyCount` 387→393(**新增键自 2588 起**); 另改**值** 2421「运行时支持开发中」→「第三方插件」与 2425 运行时说明(插件运行时已随 2026-09-12 里程碑落地, 原文案失真; 仅改值不改键)。**闸门**: Go `go test ./...` 全过(新增 manifest 设置校验 15 例 + ValidateSettingValue 19 例 + SettingsStore 8 例 + 端点 5 例); golden 快照随模板的 Include/KeyOpt 变更刷新(`UPDATE_GOLDEN=1`, diff 仅该 6 行); C# 288/288(277→288, 新增 `PluginSettingsDialogTests` 11 例, 含**直读仓库真实 plugin.json** 的契约夹具 —— 该用例当场抓出 limit 未声明 min/max 与 hint 文案不一致); analyzers×2 exit 0; `make check` lint CLEAN / texttypes 315 求值一致 / `/Validate` exit 0 / ORACLE PASS; AHK 端到端探针 ALL PASS(热重载 changed 语义、新触发键即刻生效、零配置 SelfEsPath 自探测走 es-cli 并真实返回结果、会话状态机回归)。实机部署: es.exe(官方 CLI, voidtools 签名 Valid, v1.1.0.37, SHA256 3BE71857...) 置于部署树 `data/plugins/everything_search/bin/` (插件 `SelfEsPath` 的设计落点, **不入库** —— 第三方二进制不含在仓库) |
| 2026-09-19 | 修复缺陷: **CommandInputHooks 的 provider 分发从未真正执行** ⇒ 命令框按前置触发键(空格)毫无反应 (拦截点引入于 2026-09-18, 自始未生效)。**根因**: `_Call` 写成 `fn := p.%name%` + `fn.Call(args*)`, 而 **AHK v2 的 `obj.Method` 取到的是未绑定 `this` 的函数对象**(`this` 只是普通首参, 取值前无值 —— 与 Python/JS 的 bound method 语义相反, 官方作者 lexikos 明示), 于是首个实参被顶替成 `this`、末位实参缺失, 每次回调在**调用边界**抛 `Missing a required parameter.`; 该异常被 `DispatchChar/DispatchKey/_Notify` 的 try/catch 吞掉并「视为未消费」⇒ provider 一次都没执行, 日志只剩一行被吞掉的噪声。**判据**: 插件 `OnSessionBegin()` 声明**零参**, 零实参 `.Call()` 仍报缺参 ⇒ 缺的只能是隐式 `this`。**修复**: 改用动态名直接调用 `p.%name%(args*)`(同仓先例 `bin/lib/Monitor.ahk:363`), 等价备选 `ObjBindMethod(p, name).Call(args*)`。**为何此前无闸门可拦**: `/Validate`(语法)与 `lint`(标识符遮蔽)均为静态检查, 查不出该纯运行时语义; 而外部症状「按键毫无反应」与「插件没注册」完全一致, 极易误判到插件侧。**新增守门人**: `tools/command_input_hooks_test.ahk` + `make check-hooks`(已挂入 `make check`), 23 项断言覆盖 this 绑定 / 实参位置 / 返回值透传 / HasProp 守卫 / 异常隔离且继续分发 / 短路 / 注册幂等 / 注销生效 / 无后台窗口; 探针**逐字 `#Include` 实现真身**而非另写桩(否则只验证自己的桩, 回归价值归零), 并把工作目录隔离到 `%TEMP%`(被测 `_log` 写相对路径, 不隔离会污染部署 `logs/`); **反转验证**: 旧写法 13 项红 / 新写法 23 项全绿。同批清理插件侧 5 处零引用死代码(`hint_continue` / `title_key_space` / `err_empty` / `EverythingMessages.F()` / `EverythingSettings.TriggerName()`), 插件文案表加「只保留有实调用点的键」纪律。**契约**: 新增 §3.10 冻结 `CommandInputHooks` provider 契约(方法签名 / 返回值语义 / `this` 绑定要求) |

| 2026-09-19 | **命令框中文输入 + 移除文字外围八角边框** (两项用户需求, 零 API/DB/route/protocol 变更; 未提交 —— 本机验证中): ① **中文输入**: 用户前提「原软件对输入法有硬性限制」经查**不成立** —— `DisableIME()`(`bin/lib/core/Utils.ahk`, 基于 `ImmAssociateContext(ctrl,0)`)全仓 **0 调用点**(上游 MyKeymap 亦只定义不调用), exe 未导入 `imm32.dll`, 命令框无子控件(离线实测 `controls = []`)。**真正根因**是 AHK 官方文档明示的架构限制: *"AutoHotkey does not support Input Method Editors (IME). The keyboard hook intercepts keyboard events and translates them to text by using `ToUnicodeEx` or `ToAsciiEx`."* —— 钩子在按键抵达 IME **之前**就翻译成 ASCII。**自动化实证**(新增 `tools/ime_input_test.ahk`, 用 `SendEvent` 注入 + `ImmSetOpenStatus` 程序化开关输入法, 无需人工打字): IME 实测 `openStatus=1` 下注入 `zhong`+空格, `OnChar` 收到 `7A 68 6F 6E 67 20`(拼音原文+空格), 「中」从未出现; 6 用例(zhong/zhongkong/ascii × 空格/回车)全部零中文码点。**手段侧实证**: AHK 能把任意 Unicode 逐码点注入目标窗口 `WM_CHAR`(含 emoji 走 UTF-16 代理对两条, 与 `PostCharToCaspAbbr(0x0102, Ord(c))` 语义兼容)。**落地**: 新增 `bin/lib/core/ImeInputHost.ahk`(注册为 CommandInputHooks provider) + `bin/lib/core/ime_messages.ahk`(模块自带中英文案, 不污染引擎词表) —— 会话期建**不激活**临时窗口同步 IME 打开标志, 用 `ImmGetCompositionStringW(GCS_COMPSTR)` 读组合串并在自绘浮层回显; `OnChar`/`OnKey` **始终返回 false**(透明旁路, 绝不消费 —— 中文通道与 `bb`/`ca` 缩写匹配是正交需求, 消费即让全部缩写失效, 与 everything_search 的「前置触发键」消费语义形成对照); 注入一律 `SendEvent`/`SendText`, **禁用 `SendInput`**(AHK 会临时卸载自身钩子致注入按键被自己漏掉, 这同时是探针能自动化的前提); `Injecting` 捕获锁吞掉自身注入的按键回流。默认由 `keyflux.tmpl` 的 `CommandInputHooks.Register(ImeInputHost)` + `ImeInputHost.Enable()` **启用**(注释掉两行即回退历史行为)。② **移除八角框**: 反汇编 + RTTI + 三条真机实测确认**皮肤路线无解** —— exe 内 `SampleWindow::DrawKeys(..., ComPtr<ID2D1SolidColorBrush>& brush, ...)` 只收**一支**画刷(描边与字符同源), 不存在独立边框绘制函数; `keyOpacity=0.0` ⇒ 字符与描边一起消失(图层级), `keyColor=#FFFFFF`/`#FF0000` ⇒ 描边与字符同色(八角内部透明)。**patch 二进制路线亦被证伪并已回滚**: 跳过 `DrawGeometry`(`RVA 0x7258` 的 `call [rax+0x78]`) 令命令框**完全不弹出** —— 其返回值不是纯错误码, 紧随的 `test eax,eax / jne` 之后是绘制必经路径(部署树已还原, 原文件备份为 `KeyFlux-CommandInput.orig.exe`, SHA256 `f14bba71…`)。**唯一可行路径 = 让 exe 不画那些字符**: 新增 `bin/lib/core/CommandDisplay.ahk` 作为**回显唯一收口**(`IsKeycapChar` 白名单与 exe 内 UTF-16 字面量 `a-zA-Z0-9`(VA `0x1daa0`)逐字一致; `SuppressKeycap` 默认 false ⇒ 零行为变更; `Enable()` 时置位), 四处调用点(`CommandInputHooks.CommandInputOnChar/OnKeyDown`、`type9_keyflux.EnterCapslockAbbr` Match 分支、`everything_search.EverythingSession`)全部改为经该模块。**闸门**: `tools/command_input_hooks_test.ahk` 23→**49 项**(新增 CommandDisplay 白名单边界 12 项 + 抑制语义 9 项 + 收口断言 2 项 + 引入 CommandDisplay 真身; 该 include 缺失时探针**连 `/Validate` 都会挂起**, 已在探针注释记录); 新增 `tools/ime_input_test.ahk` **14 项** + `make check-ime`(**刻意不并入 `make check`** —— 需按键注入、短暂占用键盘钩子、依赖系统已装中文输入法, 不属「随时可跑」闸门; 运行前 KeyFlux 必须退出, 因其 `#UseHook` + High 优先级独占钩子会让全用例收 0 字符); `make check` lint CLEAN / texttypes 315 求值一致 / `/Validate` exit 0 / ORACLE PASS; golden 快照与部署树产物同步刷新; `check-cs`/`analyzers` 未跑(**本批零 C# 改动**, 且本机 PATH 无 .NET SDK)。**契约**: 新增 §3.11 `CommandDisplay`(含 exe 绘制约束的四条物理事实与「回显必须收口」红线)与 §3.12 `ImeInputHost`(含四条硬约束与「探针变红即可退役」判据) |
| 2026-09-19 | **修复「打开命令框报错弹窗 + 卡死」** (用户实测: AHK 错误对话框 + 命令框无法关闭无法输入; 未提交)。**三层定位**: ① 隔离复现探针实测 **AHK v2 类对象上实例方法的 `HasProp` 为 `false`**(`Foo.HasProp("Inst")=0` / 静态方法 `=1`, 方法调用抛 `MethodError`) ⇒ `CommandInputHooks.Register(ImeInputHost)` 注册的是**类对象**, 而其四个 `On*` 回调是**实例方法** ⇒ `_Call` 的 `HasProp` 守卫**静默跳过** ⇒ 中文承载 provider 自部署起从未运行(与 13:35 会话 `command_input_hooks.log` 零新增条目吻合)。修复: 四个 `On*` 改 `static`(§3.12 硬约束 0)。② 隔离探针实测 **`PostMessage` 到已消失窗口抛 `TargetError`** + 热键线程未捕获异常 = 错误弹窗 + 线程死亡 + `StartInputHook` 已执行的 `Suspend(true)` 永远无法恢复 ⇒ **全部热键失效**(用户视角「卡死」的机制); `EchoChar`/`FuzzySuffixFire`/命令体执行原在热键线程**裸奔**, 任何一步抛出即触发。修复: `CommandInputOnChar` 的 `EchoChar`+`FuzzySuffixFire`、`CommandInputOnKeyDown` 的 `EchoBackspace`、`EnterCapslockAbbr` Match 分支的 `EchoChar`+`ExecCapslockAbbr`(命令体) 全部 try 包裹并记入 hook 日志(命令体失败另 Tip 提示)。③ 新增**引擎级兜底** `EngineOnError`(`bin/lib/core/Functions.ahk`, 模板 auto-exec 早期 `OnError` 注册): 全文记录 `Type/Message/File/Line/Stack` 到 `logs\engine_error.log`(UTF-8) + `Suspend(false)` 复位残留暂停态 + Tip 提示 + 返回 1 压制弹窗; 实测语义(kf_diag2): 热键/Timer 线程异常 ⇒ 线程终止、无弹窗、脚本继续(40/40 循环完成); auto-exec ⇒ 退出但日志落盘。另: `ImeInputHost._ShowLayer` 补 `DetectHiddenWindows(1)`(会话开始时命令框仍是隐藏窗口, 否则锚点取不到、浮层跑到屏幕中央兜底位)。**门禁**: `make check` 全绿(lint 仅 6 项既有 WARN / hooks 49/49 / texttypes / Validate exit 0 / ORACLE PASS), golden +12 行(3 include + OnError 注册 + provider 注册段), 部署树逐文件 SHA 全 MATCH + Validate exit 0。**契约**: §3.12 硬约束 0(static 注册) + 新增 §3.13 `EngineOnError`(回调代码不得存在无包裹的外部抛出点; 排查命令框故障先看 `engine_error.log`) |
| 2026-09-19 | **命令框「字母无八角框 + 中文输入」v3 定稿: 数据 patch + 动态可见性 hook** (未提交 — 本机验证中; 用户否决 v2 浮层回显, 要求字母直接显示在命令框内): ① **数据 patch 移除八角框 (§3.11 背景第 6 条)** — keycap 白名单 = `.rdata` 绘制判定数据 (62 字符 UTF-16LE, 文件偏移 `0x1cca0` = RVA `0x1daa0`, 前邻 `Draw()`/后邻 `dwriteF…`), 替换为不可匹配字符 U+0001×62 (保留长度): 字母/数字走普通字形路径 ⇒ **命令框内直接显示、无八角框**; 与已证伪的代码 patch (NOP 跳调用破坏窗口初始化) 性质完全不同 — 改常量数据不动控制流; patch 脚本内置前置校验 + 回读复核; 原文件备份 `KeyFlux-CommandInput.orig.exe` (SHA `f14bba71…468ab3`)。② **中文打不出的真根因 (实证修正)**: capsHook 以 `InputHook("", …)` 创建 (无 V ⇒ 默认不可见, **吞掉文本键**), 物理键到不了命令框窗口的 IME 上下文, 组合无从发生 (铁证: 显示=纯投递 WM_CHAR, 抑制投递后字母全部消失); ⇒ **IME 开启的会话 hook 必须 V (可见)** — 物理键透传给 IME 原生组合/候选/上屏, 上屏中文以 WM_CHAR 直达命令框 (不在白名单, 无框)。③ **落地**: `EnterCapslockAbbr()` 无参化 + 模板顶层 `MakeCapsHook()` (每会话动态创建: IME 开 ⇒ `InputHook("V",…)`+空词表; 关 ⇒ 历史形态+原词表; **必须在 BeginSession 之后调用**, 否则读到陈旧 IME 值); `ImeInputHost` 瘦身为「IME 状态同步器」(浮层/ImmGetCompositionStringW/ImmSetOpenStatus 全套删除, 只读 `_QueryImeOpen` → `SuppressKeycap := (PrevOpen=1)`, -1 按「关」处理宁多显示不吞输入); `CommandInputOnChar/OnKeyDown` 顶部新增 **IME 会话守卫** (SuppressKeycap=true ⇒ 整体旁路: 投递/缩写匹配/providers 派发全跳过 — 防拼音与 IME 组合 UI 重复显示、防拼音中途触发命令执行毁掉会话、防下拉列表误触发; 退格投递同样跳过防二次删除); `CommandDisplay` 删 Buf/OnBufferChanged/ClearBuf (v2 过渡产物, 消费者已无); `ime_messages.ahk` 退役 (模板 include 已删, **文件本体删除待用户确认** — D 盘删除需审批)。④ **链式变更**: `type9_keyflux.go` callMap[6] → `EnterCapslockAbbr()` (golden_test.go 断言同步); golden 重刷 (`UPDATE_GOLDEN=1`)。⑤ **门禁**: check-hooks 49/49, `/Validate` exit 0, ORACLE PASS (capslock 38 条), lint CLEAN; 首轮 check 曾抓到部署树同步缺口 (部署树 lib 为旧定义 → `Too few parameters passed to function: EnterCapslockAbbr`) — `make sync-out` 修复; ⚠ sync-out 的 `robocopy bin '*.exe'` 会用仓库侧**未 patch** exe 覆盖部署树 ⇒ 纪律: patch 后重跑 sync-out 必须重新 patch (§3.11 硬约束 4)。**契约**: §3.11 重写 (数据 patch 事实 + AHK/exe 白名单脱钩系有意 + sync-out 覆盖警告), §3.12 重写 (动态可见性方案 + 6 条硬约束 + check-ime 判据更新), Makefile check-ime 注释同步。⚠ IME 开启会话的端到端 (V 透传→组合→上屏) 无法被 AHK 探针完整自动化, 待用户真机验证 |
| 2026-09-19 | **命令框中文输入 v4: 恒透传方案** (承接同日 v3; 用户实测反馈「英文可输、无八角框, 中文打不出」; 未提交)。**v3 死因 (两层证伪)**: ① 跨进程 `ImmGetContext` 恒 hIMC=0 (HIMC 进程本地句柄, kf_p1 探针 notepad 同样为 0) ⇒ `_QueryImeOpen` 恒 -1 ⇒ `SuppressKeycap` 恒 false ⇒ hook 恒吞键 ⇒ v3 的透传分支从未生效; ② 兜底的 TSF 路线四连败 (CLSID REGDB_E_CLASSNOTREG → `TF_CreateThreadMgr` 虽成功但 `Activate` 在 AHK 主线程挂死 + `GetGlobalCompartment` idx14 抛异常, kf_p2b/c/d 探针)。**v4 决策: 放弃一切 IME 状态预查, hook 恒 V + 透传接管** — `MakeCapsHook` 建为 `InputHook("V",…)`+空词表 (词表必须空: MatchList 先于 OnChar, 拼音后缀命中即误执行), `ImeInputHost.OnSessionBegin` 恒置 `SuppressKeycap := true` (查询函数全删, 类退化为启闭开关); `CommandDisplay.ShouldEcho` **全停投递** (v3 只停白名单字符是错的 —— 中文 U+4E00+ 漏网双显); `FuzzySuffixFire` 单点旁路 (查注册表非词表, 拼音后缀命中缩写词即 ih.Stop()+执行命令毁会话); **providers 派发保留** (v3 整体旁路会连 everything_search 空格触发一起杀掉); Up/Down/Enter/Backspace 维持 KeyOpt "N" 透传 (IME 组合期选候选/翻页/确认拼音原文/删组合串的原生操作, 吞掉即毁 IME 交互), Esc 补 "S" (EndKey 检测不受影响); EchoBackspace 自身守卫兜停投递 (OnKeyDown 顶部守卫删除)。**门禁**: check-hooks 49→**57/57** (新增: 抑制态全停×5 + EchoChar 中文抑制 + Fuzzy 旁路 + providers 保留 + 历史形态照常; Post*/Fuzzy 桩改 Rec 记录可观测), golden 重刷, `/Validate` exit 0, ORACLE PASS 38 条, lint CLEAN; 部署树 bin/lib + KeyFlux.ahk 手动同步 (robocopy /E 避开 sync-out 的 exe 覆盖 — §3.11 硬约束 4), 四核心文件 SHA 全 MATCH。**契约**: §3.11 硬约束 3 改「透传模式总开关」语义, §3.12 重写 (恒透传, 硬约束 0-7, 新增「勿再查 IME 状态」「词表必须空」两条), ImeInputHost/CommandDisplay/CommandInputHooks/keyflux.tmpl/type9_keyflux 注释全量同步。⚠ 端到端待用户真机验证 (重启 KeyFlux 后: 英文直显 / IME 开打中文上屏 / 缩写触发 / 退格删除) |
| 2026-09-19 | **命令框 v4.1: 透传焦点修复** (承接同日 v4; 用户实测反馈「英文也没法输入了, 且焦点在其他文本框时打开命令框后仍在原文本框打字」; 未提交)。**根因 (kf_focus_probe v5 探针实证)**: 命令框窗口 exStyle=`0x8200008` (WS_EX_**NOACTIVATE**=1 + TOPMOST=1) —— SHOW 消息只改可见性、从不带来键盘焦点; v4 透传后物理键按「焦点窗口」路由, 而焦点滞留在会话开始时的原文本框 ⇒ 按键全部漏进原窗口、命令框一个都收不到 (两个症状同源)。**修复**: ① `CommandDisplay.ActivateCommandWindow()` 新增 (等窗口可见 0.5s → WinActivate → 循环回读 WinActive 400ms → 失败兜底 AttachThreadInput+SetFocus 200ms; 探针判定 WinActive 可成功激活 NOACTIVATE+TOPMOST 命令框, focusIsCmdWindow=1; 函数自身不触碰 SuppressKeycap, 单一职责); ② `EnterCapslockAbbr` 编排: SHOW 之后 MakeCapsHook 之前调用, **激活失败 ⇒ `SuppressKeycap := false` 降级历史形态** (吞键+投递显示, 英文仍可用; OnSessionEnd 复位标志, 自洽无泄漏); ③ 会话结束 (非 Match 分支) `CommandInputHooks.ActivateBackend()` 把前台还给会话开始时的窗口 (该函数的 WinActive 检查保证历史形态会话零行为变更); Match 分支不恢复 (命令体自会接管)。**探针沙箱四坑 (本轮新发现, 记入技能)**: ① AHK 内置 `PostMessage` 对投递失败抛 TargetError 无 try 即弹错误框挂死探针 —— 统一 `DllCall("PostMessageW",...)` + ret 检查; ② `WinGet`/`IsFunc`/`WinGetPIDs` 在 KeyFlux 分发的 AHK 解释器 (v2.0.19) 中**不存在** —— 调用即加载期 #Warn 弹框挂死 (「This global variable appears to never be assigned a value」); 已实证存在: WinExist/WinActivate/WinActive/WinWait/WinGetExStyle/WinGetList/WinGetPID/WinGetTitle/WinGetText/WinGetControls/DetectHiddenWindows/DllCall/PostMessageW; 取 ExStyle 用 `WinGetExStyle`; ③ 挂死探针的错误对话框在沙箱里不可见 —— 诊断法: 另起 AHK 探针用 `WinGetList("ahk_exe AutoHotkey64.exe")` + `WinGetText` 读挂死进程的对话框文本; ④ 多行 OUT 表达式续行用 `. ` 前缀 (行尾 `)` 不是开括号, 不构成续行)。**门禁**: check-hooks 57→**60/60** (第 13 组: 无窗口不抛异常 / 返回 false / 不触碰标志), 其余随 make check 全量复跑; 部署树 robocopy /E 同步 (避开 exe)。**契约**: §3.12 标题 v4.1 化, 新增「焦点契约」段 + 硬约束 6 (原 6/7 顺延为 7/8), 回归守门人 60 项, 变更记录本行 |
| 2026-09-19 | **命令框 v4.2: 缩写执行恢复 (打完即执行)** (承接同日 v4/v4.1; 用户实测反馈「输入 `se` 不再打开设置面板」; 未提交)。**根因**: v4 为防「拼音误触发缩写」清空 hook 词表 + 单点旁路 `FuzzySuffixFire` ⇒ 透传会话里缩写匹配双通道全灭 (`se` 等全部缩写不再执行)。**用户裁决**: 设置面板命令全部由英文字母组成, 唯一需要输入中文的场景是前置键 (如空格) 之后 —— 那时字符已被插件消费、到不了匹配层 ⇒ 「拼音误触发」场景不存在 ⇒ 恢复「打完即执行」。**落地**: ① `CommandInputOnChar` 撤旁路, `FuzzySuffixFire` 恒跑 (与历史形态一致); ② 两份模板 `keyflux.tmpl` 词表恢复 (`InputHook(SuppressKeycap ? "V" : "", …, CapslockAbbrKeys)`, bin 与 config-server 副本 SHA 一致); ③ `CommandDisplay`/`ImeInputHost` 注释 v4.2 化; ④ check-hooks 第 12 组断言反转 (「透传恒跑」=1) + 新增「插件消费后 Fuzzy 不跑」=0 (双保险)。**搜索期不误触发双保险**: 词表 MatchList 全串精确匹配被空格前缀挡住 (检索词 `" se"` ≠ `"se"`); 插件消费字符后 DispatchChar 提前 return。**门禁**: check-hooks 60→**61/61** + make check 全量复跑 + 部署树 robocopy /E 同步 (避开 exe) + SHA 校验。**契约**: §3.11 硬约束 3 与 §3.12 现行方案/硬约束 2/3/7 重写为恢复语义, 回归守门人 61 项, 变更记录本行 |
| 2026-09-20 | **修复缺陷: 命令框「最后一个字母不显示 + 命令被立即执行」** (v4.2 回归; 用户实测: 逐字输入 `se`, 键入最后一个 `e` 时该字母不显示、命令立刻执行)。**真实根因 = 两层叠加 (引擎日志坐实)**: ① `EchoChar(ih, c)` 两参必填, 而 Match 分支按历史写法省成 `EchoChar(, char)` ⇒ **每次命中都在调用边界抛 `Missing a required parameter.`**, 被紧随的 try/catch 吞掉 (铁证 = 部署树 `logs\command_input_hooks.log` 连发 `EchoChar(Match) 异常: Missing a required parameter.`: 09-19 23:19 与 09-20 09:29 共 5 次, 与用户每次复测一一对应); ② 即便参数写对, 透传模式下 `ShouldEcho` 恒 false ⇒ `EchoChar` 仍是 no-op —— 而**该字符不会被原生显示** (命中这一击就结束了会话) ⇒ 它彻底失去显示来源。v4.0/v4.1 未暴露的原因: 那时词表空、缩写不命中, 该行永不执行。**修复**: 新增 `CommandDisplay.EchoTerminalChar(c)` = **唯一允许绕过 ShouldEcho 的回显通道**, 两条命中路径都改走它 (Match 分支**无条件**投; `FuzzySuffixFire` **仅透传模式**投, 防历史形态双显); `EchoChar` 恢复严格双参签名并在注释记下该陷阱; 命中后的执行/隐藏**延后** `CommandInputHooks.FinishDelayMs` (150ms) (新增 `FinishCapslockAbbr` + `Pending*`/`TakePending`, `BeginSession` 复位) 让刚投递的字符先被绘制, 旧行为同线程「立即执行 + 隐藏」会把它吃掉。**语义不变**: 命令仍无需用户确认即执行。**排查教训 (重要)**: 第一轮判定 (「探针实测该键 WM_KEYDOWN 已送达 ⇒ 缺的只是绘制时间」) **是错的** —— 探针在 KeyFlux 运行期间**仪器无效** (连「无钩」对照例都收不到 WM_CHAR: 注入键被外部键盘钩子吃掉翻译步骤, 却仍送达 WM_KEYDOWN ⇒ KEYDOWN ≠ CHAR); 决定性证据在**引擎自己的日志**里 (教训已入技能 `ahk-v2-probe-harness` §14)。另修掉探针自身两处不忠实: `PostCharToCaspAbbr` 桩曾写成两参必填 (与真身 `(ih?, char?)` 不符, 会把「省略首参」误报成产品缺陷)、`EchoChar(ih?, c)` 曾触发 AHK v2「可选参数之后必须全部可选」的 `Parameter default required. Specifically: c` 载入错。**门禁**: check-hooks 61→**75/75** (第 14 组待收尾状态 6 项 + 第 15 组终止字符强制投递 8 项), lint CLEAN, GenerateAHK + `/Validate` + ORACLE DIFF PASS, 部署树 SHA 校验。**契约**: §3.12 硬约束 9 重写 (终止字符强制投递 + EchoChar 双参纪律 + 延后收尾), 守门人 75 项, 变更记录本行 |
| 2026-09-20 | **调优: 命中收尾延迟 150ms→30ms** (承接同日「终止字符未被投递」修复; 用户反馈「输入命令后执行速度太慢, 要在看到最后一个字母的一瞬间执行」)。该延迟的唯一作用是让 `EchoTerminalChar` 投出的终止字符有 1~2 个绘制周期上屏 (60Hz 下 1 帧 ≈16.7ms), 首版 150ms 属过失保守。现值 `CommandInputHooks.FinishDelayMs := 30` ≈ 2 帧 —— 感知上等同上屏瞬间即执行, 同时保住字符可见性。**勿设 0 / ≤1 帧**: 投完立刻执行+隐藏会让该字符来不及绘制 (即用户本轮报的「最后一个字母不显示」)。回归守门: check-hooks 第 14 组延迟区间断言放宽为 5..500ms (原 50..500)。**契约**: §3.12 硬约束 9 同步, 变更记录本行 |
| 2026-09-19 | **命令框搜索插件未运行时静默拉起** (修复 Everything 自动启动抢前台焦点): 用户报告进入搜索模式时插件拉起 Everything, 后者弹出主窗口并抢焦点, 打断命令框输入。**根因**: `EverythingSearch.EnsureRunning` 用裸命令 `Run('"exe"')` 启动 ⇒ Everything 显示主窗口。**修复**: 改用官方静默开关 **`-startup`** ("Run Everything in the background without showing any search windows"); **不用 `-minimize`** (只最小化, 窗口仍在且仍是焦点候选)。**兜底**: 新增 `HideMainWindowIfAny()` + `_HideVisibleMainWindow()` —— 探活后 1.2s 窗口期内 (250ms 步长) 枚举 Everything **可见**顶层窗并 `WinHide`; 筛选走**类名黑名单** (排除 `MSCTFIME UI`/`IME`/`Default IME`), **不以「标题非空」为主筛** (实测 `-startup` 下窗口标题可能为空, 用标题筛会漏掉真主窗口); 只在刚拉起后调用一次, 不影响用户此后手动开窗。**实测** (Everything 1.5.0.1418): 对照探针跑 bare/`-startup` 各 40s 逐秒采样 —— 存活性与 IPC 可用性完全等价 (恒 2 进程, `es -get-result-count` 恒 0 退出码); 唯一差异是 `-startup` **可见窗 0 个** (仅 2 个 `visible=0` 的 IME 辅助窗)、前台焦点不变; 双场景断言探针 9/9 PASS。**踩坑**: ① 探针 `Run()` 拉起的 Everything 在脚本退出后消失, 系 **Bash/PowerShell 工具调用结束会清理其子进程树** (非产品缺陷); ② **`plugins/examples/` 才是入库权威源**, `data/plugins/` 被 `.gitignore` 忽略 —— 改错位置会被 `make check` 的 sync-plugins (robocopy examples→部署树) 整体冲掉。**门禁**: make check 全绿 (check-hooks 75/75 / lint / check-texttypes / GenerateAHK / `/Validate` / ORACLE DIFF PASS), 三方 SHA 一致 (`EverythingSearch.ahk` `61c2073b`)。**契约**: 插件 README §1.1 记录修法, 变更记录本行 |
| 2026-09-20 | **命令框字体替换为得意黑 (Smiley Sans)** (纯资源替换, 零代码/API/DB/route/protocol 变更): 用户要求把命令框字体统一改为得意黑, 中英文数字与 placeholder 全由其渲染且不出现英文回退。**机制确认 (PE 静态解析)**: 命令框字体**不来自系统字体、也不由配置决定** —— exe 内 UTF-16 字面量 `font\font.ttf` 是唯一来源 (相对 exe 自身目录), 用它建 `IDWriteFontCollection` 后按该 ttf 的 `name` 表族名调 `CreateTextFormat`, 权重/字号硬编码 (`WEIGHT_BOLD(700)` / `44.0f`, 断言串 RVA `0x1ddf0`); `.rdata` 全量字符串**无任何字体族名** (`Iosevka` ASCII/UTF-16 均 0 命中)→ 族名只能取自文件本身; 皮肤配置 `CommandInputSkin.txt` 的 **19 键全是颜色/透明度/圆角/尺寸/动画, 无 font 键** (与 exe `.rdata` 配置键已全量比对一一对应); 渲染栈 DirectWrite/D2D/D3D11/DComp (非 GDI) ⇒ `WM_SETFONT` 类注入无效。**⇒ 换字体 = 替换 `bin/font/font.ttf`** (无需重编译 / 装系统字体 / 改配置)。**⚠ 关键改造 (否则中文糊成一团)**: exe 请求 `WEIGHT_BOLD(700)` 而得意黑原生 `usWeightClass=400` + `fsSelection ITALIC`, DirectWrite 在单 face 私有集合中会施加**合成加粗 (BOLDSIM)** —— WPF 实测复现中文笔画粘连。故用 fontTools 改造元数据使其成为**精确匹配**: `OS/2.usWeightClass 400→700`、`OS/2.fsSelection` 清 ITALIC/REGULAR 置 BOLD (`0x0001→0x0020`)、`OS/2.panose.bWeight 0→8`、`head.macStyle` 清 italic 置 bold (`0x0002→0x0001`)、`post.italicAngle -8.0→0`; **`name` 表与 `glyf` 字形不动** (族名保持 `得意黑`/`Smiley Sans Oblique`, 因族名取自文件且改名有查不到的风险; 倾斜在设计里不靠元数据)。⚠ 改完必须经 **fontTools `save()` 重编译**重算校验和 —— 手工改字节会让 `OS/2`/`head`/`post` 三表校验和失配 (fontTools 报 `bad checksum`)。**字符覆盖**: 原 Iosevka **不含中文字形** (204 字符抽样缺 109), 中文靠 DirectWrite 回退 (exe 传 `L""` 空 locale ⇒ 回退链不确定); 得意黑自带 **9497 字形** (同抽样 **0 缺失**, 含全部中文与 latin) ⇒ 换后中英文数字不再依赖回退。**落地**: `bin/font/font.ttf` 替换 (三方 SHA 一致 `2e4ce734…ba5bb`, **与上游原版 `b447d7e7…d25c4` 不同 —— 差异即上述元数据改造**), 仓库与部署树同时更新; 原 Iosevka 在 git 历史 (`HEAD:bin/font/font.ttf`, `deebc76e…`) + 部署树 `font.ttf.bak-iosevka` (同 SHA) 双备份, `git checkout` 即回滚。**⚠ 同步注意**: Makefile `sync-out` 的 robocopy 白名单为 `'*.ahk' '*.exe' '*.ps1' '*.txt' '*.dll'` —— **不含 `*.ttf`** ⇒ 换字体后必须**手动同步两处**, `make out`/`deploy` 既不复制也不删除字体。**门禁**: make check 全绿 (check-hooks 75/75 / lint / check-texttypes / GenerateAHK / `/Validate` / ORACLE DIFF PASS), 字体 SHA 三方一致。**契约**: 新增 §3.11.1「命令框字体」(机制 + 19 键皮肤边界 + 5 条硬约束), §3.11 背景第 1 条订正 (原写「DirectWrite 系统字体回退」→ 改为私有字体集合 + 指向 §3.11.1), 变更记录本行 |
| 2026-09-20 | **命令框字体改定 MiSans-Bold** (承接同日「替换为得意黑」, 纯资源替换, 零代码/API/DB/route/protocol 变更): 用户要求改为「软件使用的 MiSans」。**来源**: `config-ui-avalonia/Assets/Fonts/MiSans-Bold.ttf` —— 即配置界面全局 `AppUiFont` 的同一族字体 (族名 `MiSans`, 界面侧经 `avares://KeyFlux.Settings/Assets/Fonts/#MiSans` 引用, 见 `Styles/Skins/Claude.axaml` 与 `App.axaml`)。**选 Bold 字重的依据 (元数据实测)**: `usWeightClass=700` / `fsSelection=0x0120`(BOLD=1, ITALIC=0) / `macStyle=0x01`(bold=1, italic=0) / `italicAngle=0` / `panose.bWeight=8` —— 与 exe 硬编码请求 `WEIGHT_BOLD(700)` + `STYLE_NORMAL` **原生精确匹配** ⇒ **零元数据改造** (对比得意黑需 fontTools 改 5 处元数据且仍有校验和风险)。**附加收益**: ①**正体非斜体** (得意黑官方只有 `SmileySans-Oblique` 单字重, 倾斜画在 glyf 轮廓里, 清元数据标记也去不掉, 实测字形倾角 ≈8.7°); ②**字符覆盖更优** —— 213 字符抽样 MiSans-Bold **0 缺失**, 得意黑缺 `※■□◆◇○●` 共 7 个; ③与软件 UI 视觉统一。**落地**: `bin/font/font.ttf` 替换 (`250fb5c8…21be`, 7,804,780 B, 29093 字形), 仓库与部署树**手动同步** (因 `sync-out` 白名单不含 `*.ttf`), 三方 SHA 一致 (仓库 = 部署树 = `Assets/Fonts/MiSans-Bold.ttf`)。**备份**: 部署树新增 `bin/font/font.ttf.bak-smiley` (得意黑 `2e4ce734…ba5bb`), 与原 `font.ttf.bak-iosevka` (Iosevka `deebc76e…`) 并列作为回滚资产。**⚠ 生效条件**: 字体在 `KeyFlux-CommandInput.exe` **启动时读取一次** (运行期间文件未被锁定, 实测可写) ⇒ **必须重启引擎/命令框进程**才可见新字体。**门禁**: make check 全绿 (check-hooks 75/75 / lint / check-texttypes / GenerateAHK / `/Validate` / ORACLE DIFF PASS), 字体未被 `make check` 覆盖 (SHA 复核不变)。**契约**: §3.11.1 标题与开头改写为「当前字体 = MiSans-Bold」+ 选择理由, 硬约束第 2 条改为「优先选原生 700 的 Bold 静态字体 ⇒ 零改造」并补充「清元数据标记改不掉斜体字形」的实证警示, 硬约束第 5 条补充「字体仅启动时读取一次, 须重启进程」, 第 6 条回滚资产补充 `.bak-smiley` |
| 2026-09-20 | **命令框字体改定 Sthginkra (CFF→glyf 转换 + 元数据改造)** (承接同日 MiSans-Bold / 得意黑, 纯资源替换, 零代码/API/DB/route/protocol 变更): 用户指定 `D:\UserData\Downloads\Sthginkra.otf`。**源体检 (实测)**: `usWeightClass=400` / `fsSelection=0x01C0`(REGULAR=1) / `macStyle=0x00` / `italicAngle=0`, **CFF 轮廓** (表: `CFF `/`GDEF`/`GPOS`/`GSUB`/`vhea`/`vmtx`, 无 `glyf`/`loca`), 33072 字形 / 33288 cmap 项, upem 1000。**两项必修**: ①**非 700 ⇒ 必须改元数据**, 否则 DirectWrite 施加 BOLDSIM 合成加粗 (笔画糊); ②**CFF ⇒ 必须转 glyf** —— 已知可用历代字体 (Iosevka / MiSans-Bold / 得意黑 ttf) 全是 glyf, exe 对 CFF 加载路径**无验证先例** (自定义字体集合若硬编码 `DWRITE_FONT_FACE_TYPE_TRUETYPE` 会直接失败), 故按已知可用格式对齐。**转换 (fontTools, 14s)**: 逐字形 `Cu2QuPen(TTGlyphPen(), max_err=1.0, reverse_direction=True)`; **5 处实测踩坑**: ①`sfntVersion` 必须 `OTTO`→TrueType 签名 (否则 FreeType 报 `SFNT font table missing`); ②必须显式 `newTable("loca")` (fontTools 不随 glyf 自动建, 否则 `loca table missing`); ③`maxp` 需 `tableVersion=0x00010000`+`recalc()` 并补齐 7 个 glyf 专有字段 (CFF 源是 v0.5 无此键, 否则 `KeyError: 'maxZones'`); ④`recalc()` 前须逐字形 `recalcBounds(glyf)` (否则 `'Glyph' object has no attribute 'xMin'`); ⑤`f.getGlyphSet()` 而非 `f["CFF "].getGlyphSet()` (新版 fontTools 后者已移除)。删 `CFF `/`VORG`, 弃失效 `DSIG`, `post.formatType=3.0`; **`name`/`cmap`/`hmtx`/`GPOS`/`GSUB` 原样保留**。**元数据改造**: `usWeightClass 400→700`、`fsSelection 0x01C0→0x01A0` (清 REGULAR, 置 BOLD, 保留 WWS+USE_TYPO_METRICS)、`panose.bWeight 5→8`、`macStyle 0x00→0x01`、`italicAngle` 已 0。**保真度实测**: OTF vs 转换 TTF 二值掩膜 IoU 随字号收敛 **0.7932 (56px) → 0.9100 (200px) → 0.9896 (400px)**, 400px 墨迹像素 632588 vs 632285 (**差 0.05%**) ⇒ 差异纯为小字号渲染量化 (CFF 提示丢失), **形状无损**。**字符覆盖**: 191 字符抽样 **0 缺失** (CJK 常用区 20976/20992 = 99.9%, ASCII 95/95, 平/片假名 96.9%/100%, 拉丁扩展 336/336; 缺 `■□◆◇○●` 6 个几何符号 — 与得意黑同)。**正体性**: 逐行扫描 `I`/`口`/`1` 墨迹垂直笔画边界恒定 ⇒ 零倾斜。**落地**: `bin/font/font.ttf` 替换 (`d758c7b4…9a61`, 10,511,648 B, 33072 字形, `sfntVersion=0x00010000` 真 TrueType), 仓库与部署树**手动同步** (sync-out 白名单不含 `*.ttf`), 三方 SHA 一致。**备份**: 部署树新增 `font.ttf.bak-misans` (MiSans-Bold `250fb5c8…`), 与 `.bak-smiley`/`.bak-iosevka` 并列, **三条回滚资产齐全**。**生效条件**: 字体在 exe 启动时读取一次 ⇒ 已 `Stop-Process` 旧命令框进程 (懒加载, 引擎不自动拉起)。**门禁**: make check 全绿 (check-hooks 75/75 / lint / check-texttypes / GenerateAHK / `/Validate` / ORACLE DIFF PASS), 字体未被覆盖 (SHA 复核不变)。**契约**: §3.11.1 标题/开头改写为「当前字体 = Sthginkra」+ **历代字体重录表** (Iosevka→得意黑→MiSans-Bold→Sthginkra 含 SHA/结论); **新增硬约束 3「轮廓格式须为 glyf, 不接受 CFF/OTF」** (含 5 条转换踩坑 + 保真度实测数据), 原 3~6 顺延为 4~7; 硬约束 2 的 fsSelection 说明补「保留 WWS/USE_TYPO_METRICS 位」; 硬约束 7 回滚资产补 `.bak-misans`; 「当前状态」段同步更新 |
| 2026-09-20 | **命令框字体加粗 (几何轮廓膨胀 r=12) + 固化字体预处理工具** (承接同日 Sthginkra 转换; 纯资源替换 + 新增 dev 工具, 零代码/API/DB/route/protocol 变更): 用户反馈「字体太细了, 再粗一点」。**关键认知**: exe 请求的 `BOLD(700)` 只是**元数据权重** —— 元数据对齐 700 后 DirectWrite 认为**精确匹配** ⇒ **不会再加粗** (无 BOLDSIM) ⇒ 想变粗**只能在字形上做**。**两条路对比**: ①**几何加粗 (采用, 可控)** = 轮廓膨胀 `dilate(path, r) = path ∪ stroke(path, 2r)` (与半径 r 圆盘做 Minkowski 和, ROUND_CAP/ROUND_JOIN), 笔画宽度**精确增加 2r 单位**; ②DirectWrite 合成加粗 (把 `usWeightClass` 压回 400 制造"不匹配"触发 BOLDSIM) —— 零体积增长但**加粗量不可控**且质量低, 仅作备选。**基准与选档 (实测竖干宽度, upem=1000)**: 原始 80 单位 (44px 下 3.52px); `r=8→96 (4.22px)`、**`r=12→104 (4.58px)`**、`r=16→112 (4.93px)`、`r=20→120 (5.28px)`。对照参考: 上游原 Iosevka Bold 竖干 118 单位、MiSans-Bold 175 单位。**🔴 选 r=12 的理由**: 本字体设计紧凑, **`r ≥ 16` 起 CJK 字腔 (封闭白区) 开始被填死** —— 「保存 加载 重置」在 88px 下已糊成实心块且**不可逆** (原轮廓信息丢失); r=12 在 44px (命令框实际字号) 与 88px 下均笔画实心、字腔张开、CJK 可读。**🔴 最大踩坑: 轮廓绕向导致"空心轮廓"**: 最初用 `pathops.union([fill, stroke], pen)` —— 该函数**并非布尔并集**, 只是把轮廓丢进同一 Path 再 `simplify()`, 当原字形 (TrueType 顺时针外轮廓) 与 stroke 产出的环 (绕向不同) 混合时 `simplify` **把重叠区判成空洞** ⇒ 笔画渲染成空心轮廓。实测 (`加`, r=16): 错误做法面积 `280700→285886` (轮廓碎成 12 段); **改用 `pathops.op(fill, line, PathOp.UNION)` 真布尔并集**得 `280700→403497` (3 段)。**其它实测细节**: ①Skia `stroke()` 产出 **CONIC 段, 布尔运算不接受** ⇒ 必须先 `convertConicsToQuads()`; ②`Path.stroke()` 是**原地修改且返回 None**; ③膨胀会**跳过错字形** (`numberOfContours<0` 的复合字形会重复加粗子字形), 实测 dilated=33060 / skipped=12; ④`maxp` 的 `maxPoints`/`maxContours` 必须 `recalc()` (**337→722 / 40→34**, 圆角 join 引入大量曲线点); ⑤**文件体积翻倍** 10,511,648 → 21,174,484 B (**2.01x**); ⑥逐字形 `recalcBounds(glyf)` 后 `maxp.recalc(f)`; ⑦全量 33072 字形耗时 **87s** (不含 save)。**渲染验证**: 「IoU 随字号收敛」用在 OTF→TTF 转换上得 0.79→0.91→0.9896; 本次加粗核对 44px/88px 双档预览, 笔画实心无空心轮廓, 字符覆盖 191 抽样 **0 缺失** 不变。**落地**: 加粗版替换 `bin/font/font.ttf` (**`b6f98778…289c`**, 21,174,484 B, 33072 字形, 元数据仍 `700/0x01A0/0x01/0`), 仓库与部署树**手动同步** (sync-out 白名单不含 `*.ttf`), 三方 SHA 一致。**备份**: 部署树新增 `font.ttf.bak-sthginkra-thin` (细体 Sthginkra `d758c7b4…`), **四条回滚资产齐全**。**生效条件**: 已 `Stop-Process` 旧命令框进程 (懒加载)。**🆕 固化工具 (新增 2 个 dev 脚本, 非运行时)**: `tools/font_otf2ttf.py` (CFF→glyf 转换 + 元数据权重对齐, 含 5 处转换踩坑) 与 `tools/font_embolden.py` (轮廓膨胀加粗, 含真布尔并集踩坑与半径选择建议) —— 因命令框字体当日已更换 4 次, 把两次踩坑固化为可复用脚本。**门禁**: make check 全绿 (check-hooks 75/75 / lint / check-texttypes / GenerateAHK / `/Validate` / ORACLE DIFF PASS), 字体未被覆盖 (SHA 复核不变)。**契约**: §3.11.1 开头与历代字体重录表加第 4 行 (Sthginkra 膨胀 r=12), 判据优先级插入「笔画粗细合适」; **新增硬约束 4「笔画粗细可调, 用几何加粗」** (含两条路线对比 / 实测档位表 / CJK 字腔上限 / 真布尔并集踩坑 / 体积与 maxp 副作用), 原 4~7 顺延为 5~8; 硬约束 8 回滚资产补 `.bak-sthginkra-thin`; 「当前状态」段同步更新 |
| 2026-09-20 | **设置页「选项」新增命令框字体设置 (字体文件选择 + 字重档位) 与生成端字体落地** (用户要求: 在选项页新增组件框自定义 command 命令输入框字体; 提供字体族选择控件调系统弹窗选本地字体文件; 提供字重档位控件; 位置/标题/间距/交互风格与既有设置项一致; 实时预览 + 与既有配置持久化机制 (配置读写/i18n) 一致; 处理配置缺失或非法取值的默认值与边界)。**前置实证 (用户二次确认)**: 本轮用 C 探针从 DWrite 语义层证明 `IDWriteFactory::CreateTextFormat` 的 `familyName` 实参**不被校验、不参与选 face、仅被原样存储** —— 传不存在的族名 (`ZzzNoSuchFontXYZ`) 甚至空串一律 `S_OK`, 且 `GetFontFamilyName` 逐字读回请求值 (探针 `dwprobe4.exe`, 因 mingw `dwrite.h` 的 `IDWriteTextFormat`/`IDWriteTextLayout` 同名方法重复定义无法 include, 改为手写 vtable 索引; 踩坑: `GetFontFamilyNameLength` 是**按值返回 UINT32** 而非 out 参数, 写成 out 会踩栈 `0xC0000409`)。⇒ **字体族无法通过配置改变**, 只能换 `font.ttf` 文件本身, 故本任务落地为「UI 存路径 + 生成端复制文件」。**落地 (5 处代码 + 文档/文案/测试)**: ① **配置段** `options.commandFont { sourcePath, weight }` —— C# `ConfigModels.Options.CommandFont` ⇄ Go `model.Options.CommandFont` ⇄ Go `OptionsDTO` + `optionsToDTO` + `dtoToOptions` (**五落点全改**, 避免 G1 类静默抹字段); ② **生成端** 新增 `internal/script/font.go` 的 `InstallCommandFont(opt, baseDir)` —— 在 `GenerateScripts` (运行时, baseDir="" ⇒ 落点 `font/font.ttf` 相对 cwd=bin) 与 `GenerateAHK` (CLI/校验, baseDir=**输出文件目录** ⇒ 部署树 `bin/font/`) 中调用; **失败一律静默跳过不阻断生成**, 覆盖 7 条边界: 空路径 / 源不存在 / 超 32 MiB / 非字体签名 (仅接受 sfnt `0x00010000`·`true`·`OTTO`·`ttcf`) / **源即目标 (必须跳过, 否则自复制把文件截断为 0 字节)** / 目标目录不存在 (自动建) / 相对路径按 baseDir 解析; ③ **UI 卡片** `SettingsPageView.axaml` 右列第 12 张卡 (命令框字体, 紧随窗口毛玻璃), 复用 `Border.settingsCard` + `Button.sectionHeader` + `StackPanel.sectionBody` 既有范式; 控件 = 只读路径框 + 「浏览」(系统弹窗 `StorageProvider.OpenFilePickerAsync`, 过滤器 `*.ttf;*.otf;*.ttc`, 先例同 PluginsPageView.OnImportClick) + 「恢复默认」+ 字重 `ComboBox`; ④ **VM** `SettingsPageViewModel` 新增 `ShowCommandFont` + `ToggleSection` 的 `case "commandfont"` (wasOpen/重置/case **三处同改**) + `CommandFontPath`/`SelectedFontWeight`/`LoadCommandFont`/`CurrentCommandFont`/`ResetCommandFont`/`SetCommandFontPath`, 默认值与非法值口径照抄 `AcrylicOption` 范式; ⑤ **i18n** 新增 9 键 (2503 命令框字体 / 2504 字体文件 / 2505 选择本地字体文件 / 2506 未选择 / 2507 恢复默认 / 2508 字重 / 2509 常规 / 2510 中等 / 2511 半粗, 中英双语), `I18nResourceTests.ExpectedKeyCount` 393→**402** (回填原空洞 2503-2511)。**🔴 字重字段的语义边界 (用户质疑的澄清点)**: `weight` 档位**当前仅作记录, 完全不参与渲染**。上一轮「字体变粗成功」的实现是 `tools/font_embolden.py` **几何轮廓膨胀 r=12** (竖干 80→104 字体单位), 即**改字形本身** —— 与 `DWRITE_FONT_WEIGHT_BOLD(700)` 请求无关; 因元数据已与请求**精确匹配** ⇒ DirectWrite **不再合成加粗 (无 BOLDSIM)** ⇒ 最终笔画粗细 100% 由 ttf 字形决定, 与选哪档无关。故两次说法一致而非矛盾: 「字重参数只负责触发/不触发合成加粗; 精确匹配后粗细由字形决定」(已在 §3.11.1 硬约束 4 补该推论, 并在卡片注释与 VM 注释中明确标注, 不做「能调粗细」的误导)。**测试 (新增 15 例, 全绿)**: Go `script/font_test.go` (7 条边界: 空路径/源缺失/超限/非字体/正常复制+自动建目录/源即目标不自截断/相对路径解析) + Go `server/dto_test.go::TestCommandFontRoundTrip` (PUT→model→GET 往返 + 空段恒对象恒键) + C# `CommandFontContractTests` (json 键名对齐 Go tag / 空值仍序列化 / 缺段补默认 / 已有段不覆盖用户值 / 往返 / 字重规范化 Theory 8 例 / 白名单一致 / file URI→本地路径)。**门禁**: `go build ./...` + `go vet` clean; `dotnet build` 0 错误; `make check-cs` 中与本改动相关的用例全绿 (303 总数 / 281 通过, 余 22 条为**既有**的 endpoint 契约用例 —— 已用 git stash 基线复跑确认同样 22 条失败, 根因是 `SettingsTestServer.AssertBackendNotStale()` 要求 `%TEMP%\mk_settings_headless\settings.exe` 为最新构建产物, 与本改动无关); `make analyzers` 两项目 clean。⚠ **未跑** `make check` 全量 (需重编 settings.exe 并同步部署树, 且其 golden 用例 `TestGoldenKeyFluxAHK` 在基线即为红 —— `CommandImeGuard.ahk` include 顺序差异, 已单独 stash 复跑确认预先存在, 非本改动引入)。**⚠ 同步提示**: 生成端会写 `bin/font/font.ttf`, 而 `sync-out` 的 robocopy 白名单**不含 `*.ttf`** ⇒ `make check` (它跑 `GenerateAHK`) 会把字体刷到部署树, 但 `make out`/`deploy` 既不复制也不删除字体; 用户配置生效后部署树字体为所选文件字节, 与仓库副本可能不同 (预期行为, 已记入 §3.11.1「当前状态」)。**契约**: §3.11.1 皮肤边界段改写 (取消「字体不经配置调整」的绝对表述, 新增 `options.commandFont` 表 + 三处同步清单 + 明确「配置只让生成端替用户做替换, 字体族依旧无法由配置改变」), 硬约束 4 补字重不参与渲染的推论, 「当前状态」补配置生效后的 SHA 差异说明; §5.2 补「三处同步清单 (五落点)」+ 登记 `options.acrylic` 的既有 DTO 缺口; 变更记录本行 |
| 2026-09-21 | **修复「字母周围又有八角框」回归 + 命令框字体格式闸门 (拒绝 CFF)** (用户报障; **两个独立缺陷叠加**, 均已根治并加守门人)。**① 八角框回归 (用户所报症状)**: 根因是 `make deploy` 的 `sync-out` 里 `robocopy bin ... '*.exe'` 用仓库侧**未 patch** 的 `KeyFlux-CommandInput.exe` 覆盖了部署树 —— 实测部署树 exe SHA 退回 `f14bba71…` (= 与 `KeyFlux-CommandInput.orig.exe` **逐字节相同**) ⇒ `.rdata` 偏移 `0x1cca0` 的 keycap 白名单 (`a-zA-Z0-9` 共 62 字符 UTF-16LE) 被还原 ⇒ 字母/数字重新被套上八角框。这正是 §3.11 硬约束 4 早已警告、但此前**只靠人工纪律**执行的坑 (09-19 那次就踩过)。**根治 (自动化收口)**: 把原先的临时脚本 (`%TEMP%\kf_patch_whitelist.py`, 已随 09-20 的临时文件清理一并回收) 固化为 **`tools/patch_command_input.py`** —— 幂等 (三态判定 `patched`/`original`/`unknown`)、**前置哨兵校验** (偏移 `0x1CC80` = `'ace->EndD'`、`0x1CD1C` = `\0\0dwrite`, 一律用**显式 hex 常量**表达以免转义 NUL 数错字节; 防 exe 版本漂移)、**写盘后回读复核**、`--check`/`--revert` 子模式; 并新增 Makefile 目标 **`patch-commandinput`**, **挂在 `sync-out` 配方末尾自动执行** (先 `Stop-Process KeyFlux-CommandInput` 解锁 —— 运行中的 exe 自锁不可写; 该进程**懒加载**, 下次唤起命令框时由引擎重建, 且 `deploy` 末步本就重启实例), 另加只读诊断目标 `check-commandinput-patch`。**接线实测有效**: `make buildServer sync-out` 输出显示 robocopy 后 `状态 = original` → patch 自动重施 → `patched`。patch 后 exe SHA = **`2aed3232…5fbe`** (与工作记忆记录的历次 patch 值一致), 与原版差异**恰好 62 字节** (范围 `0x1cca0–0x1cd1a`), 文件长度不变 (580096 B)。**② 字体格式闸门 (上一轮新增功能的缺陷)**: 用户在设置面板选了 `D:\UserData\Downloads\Sthginkra.otf` (CFF), 而 `InstallCommandFont` 把 `OTTO` 当作合法 sfnt 签名**放行并原样复制** ⇒ 部署树 `bin/font/font.ttf` 变成 `sfntVersion=4f54544f` (OTTO) 的 CFF 文件 (8,688,388 B) —— 直接违背 §3.11.1 硬约束 3 (exe 只接受 glyf), 且因错误被刻意忽略而**全程静默**, 用户只看到「选了没效果」。**修法**: 把 `looksLikeFont` 换成语义更强的 `classifyFontKinds` —— 只接受 glyf (`0x00010000` / `'true'`; 遇 `'ttcf'` 则递归校验**首个 face** 的实际轮廓标签), **`OTTO` 一律拒绝**并返回可读原因 (含转换指引); 同步订正函数头边界注释第 4 条。**恢复**: 部署树 `font.ttf` 回滚为仓库已知良好版 (`b6f98778…289c`, glyf, 21,174,484 B, 两侧 SHA MATCH)。**测试**: 新增 `font_test.go::TestInstallCommandFont_FormatGate` 4 例 (CFF 拒绝且不落盘 / glyf 集合接受 / CFF face 集合拒绝 / `'true'` 接受), 连同原 7 例共 **11/11 全绿**。**门禁**: `go build ./...` 与 `go vet ./...` clean; `go test ./internal/script/... ./internal/server/...` 除 `TestGoldenKeyFluxAHK` 外全过 —— 该用例已用 `git stash` 基线复跑确认**同样失败** (`CommandImeGuard.ahk` include 顺序差异, **既有基线红**, 非本改动引入); `gofmt -l` 对全仓 54 个 Go 文件标记 10 个 (含 `dto.go`/`config.go`/`command.go` 等**与本改动无关的既有文件**), 把新增文件 LF 归一后 `FORMAT OK` ⇒ 纯 CRLF 假阳性, 非格式缺陷。产物校验: `bin/settings.exe` `a16008e3…`、`bin/font/font.ttf` `b6f98778…`、patched exe `2aed3232…` 两侧一致。**契约**: §3.11 硬约束 4 重写 (记录本次复现 + 自动化收口 + 诊断目标 + patch 后 SHA 与差异范围), §3.11.1 硬约束 3 补「该约束已由代码强制」段 (含 CFF 放行缺陷的回归背景与守门人), 变更记录本行。**⚠ 待用户验证**: 重启引擎/唤起命令框后确认 ① 字母数字**无八角框**; ② 字体为加粗 Sthginkra。**⚠ 遗留 (待决策)**: `config.json` 的 `options.commandFont.sourcePath` 仍指向那份 `.otf` (现被闸门拒绝 ⇒ 静默跳过, 字体保持现状), UI 仍显示该路径; 另「选了就立刻生效」尚未达成 —— 字体在 exe 启动时**只读一次**, 换字体后仍须重启命令框进程。 |
| 2026-09-21 | **命令框字体「保存即生效」+ 提供可直接选用的 .ttf 字体** (承接同日报障修复; 用户要求「转成 .ttf, 另一个也要做」)。**① 转换 `.otf` → `.ttf`**: 用已固化的 `tools/font_otf2ttf.py` 把 `D:\UserData\Downloads\Sthginkra.otf` (CFF) 转为 `D:\UserData\Downloads\Sthginkra.ttf` (10,511,648 B, 33072 字形, `sfntVersion` `OTTO`→`00010000`, glyf 轮廓, 元数据 `usWeightClass 400→700` / `fsSelection 0x01C0→0x01A0`, 丢弃失效 `DSIG`, 耗时 19s, 工具自报 `reload OK`); 另把仓库当前生效的加粗版复制为 `D:\UserData\Downloads\Sthginkra-Bold.ttf` (`b6f98778…`, 21,174,484 B) 供用户按喜好选用。**转换可复现性实测**: 与部署树的历史细体备份 `font.ttf.bak-sthginkra-thin` 逐表比对 —— **15 个表集合完全相同, 仅 `head` 表校验和不同** (该表含修改时间戳: `head.modified` 3872795517 vs 3872753866; `head.created` 两者相同), 文件大小一致 ⇒ **内容逐表一致, 差异纯为构建时间戳**; 故新文件 SHA (`38c723db…`) 与历史记录 (`d758c7b4…`) 不同属**预期**, 非缺陷。**② 字体「保存即生效」自动化**: 此前保存配置只重启引擎 (`proc.ExecCmd("./KeyFlux.exe")`), 而 `KeyFlux-CommandInput.exe` 是**独立进程且仅在启动时读一次 `font.ttf`** ⇒ 新字体不生效 —— 这是用户「选了没效果 / 区别不大」的根因之一。**落地**: ① 新增 `proc.StopProcessByName(name)` —— 按镜像名 `taskkill /F /IM` 结束进程, **进程不存在 (退出码 128) 视作成功** (幂等, 覆盖"用户从未唤起过命令框"这一正常情形); ② 新增 `script.CommandFontFromConfigFile(path)` —— 读已落盘 config.json 的 `options.commandFont` 段, 任何读取/解析失败一律返回**零值** (零值 ≠ 用户的非空选择 ⇒ "读不到旧配置"被判为"字体变了", 走保守分支: 宁可多重建一次, 不可漏生效); ③ 把 `script.ConfigRelPath` 抽为配置落点的**单一真源** (`SaveConfigFile` 与读取端共用 —— 两处各自硬编码一旦分叉, 会产生「读错文件 ⇒ 恒判未变 ⇒ 新字体永不生效」的静默缺陷); ④ `server.SaveConfigHandler` 在**覆盖写之前**记下旧字体段, 落盘后若与新的**不相等**则结束命令框进程 —— 引擎随后本就重启并重新生成脚本 (新字体已落到 `bin/font/font.ttf`), 命令框在用户下次唤起时用新字体重建。**关键取舍**: 只判「字体段是否变化」而**非每次保存都杀** —— 避免让用户无谓付出重建 DirectWrite 私有字体集合的代价。**测试**: 新增 `font_test.go::TestCommandFontFromConfigFile` 5 例 (正常读出 / 文件缺失退化 / JSON 非法退化 / 缺 options 段退化 / 零值≠用户选择) ⇒ 字体相关用例共 **16/16 全绿**。**门禁**: `go build ./...` 与 `go vet ./...` clean; `make check sync-out` 全绿 (check-hooks **75/75** / lint CLEAN / check-texttypes / GenerateAHK + `/Validate` / ORACLE DIFF PASS), patch 步骤在 robocopy 后自动重施 (`original` → `patched`)。**格式闸门在真实流程中生效的旁证**: 本次 `check` 用**部署树的 config.json** (其中 `commandFont.sourcePath` 仍指向那份 `.otf`) 跑 GenerateAHK ⇒ CFF 被拒绝、字体未被污染, 部署树 `font.ttf` 复核仍为 `b6f98778…` (glyf, 21,174,484 B)。产物: `bin/settings.exe` `4708a7bc…`、`bin/font/font.ttf` `b6f98778…`、patched exe `2aed3232…`, **两侧 SHA 一致**。**契约**: §3.11.1 硬约束 7 补「✅ 已自动化」段 (机制 + 幂等口径 + 保守分支依据 + 单一真源 + 守门人), 变更记录本行。 |
| 2026-09-21 | **「命令框字体」卡并入「命令框皮肤」卡 + 皮肤纳入「保存即生效」** (承接同日两条; 用户要求「按你的建议修改, 并把命令框字体选项框整合进该选项框中」)。**① 皮肤纳入保存即生效**: 把上一轮为字体加的自动重启触发条件由「仅 `options.commandFont` 变化」扩展为「`options.commandFont` **或** `options.commandInputSkin` 变化」。实现上把 `script.CommandFontFromConfigFile` 升级为 `script.CommandBoxAppearanceFromConfigFile`, 返回 `CommandBoxAppearance{Font, Skin}` **一次读取的快照** (避免两次读取撕裂, 且两段生效条件本就一致); `SaveConfigHandler` 落盘前记快照、落盘后**任一段**不等即 `proc.StopProcessByName("KeyFlux-CommandInput.exe")`。**依据 (皮肤读取时机的动态实证)**: 用 Windows 托管的最后访问时间 (`fsutil behavior query disablelastaccess` = `2`) 做无侵入实验 —— 单独启动命令框 exe 后 `CommandInputSkin.txt` 的 `LastAccessTime` **在启动后 0.1s 前进, 之后 10s 观察窗内不再变化**, 进程全程存活 ⇒ **exe 只在启动时读一次皮肤, 与 `font.ttf` 完全同源** ⇒ 改皮肤同样必须重启命令框进程 (此前保存只重启引擎, 故改皮肤看不到效果, 用户据此提问「皮肤卡是否还有用」)。**② 卡片合并**: 删除「选项」页右列原有的独立「命令框字体」卡, 把其控件 (只读路径框 + 「选择…」+「恢复默认」+ 字重 `ComboBox`) 移入「命令框皮肤」卡的 `sectionBody`, 以 1px `ClaudeBorderCreamBrush` 细分隔线分组, **共用 `ShowSkin` 分区开关**; VM 侧删除 `ShowCommandFont` 字段与 `ToggleSection` 的 `"commandfont"` 分支 (wasOpen/重置/case 三处), AXAML 删除为该卡添加的 `nth-child(12)` 入场级联档 (卡片数 11→10, 回到 `nth-child(2..11)` 十档)。**合并理由**: 两者同属「命令框外观」且**生效条件完全相同**, 分成两张卡会让用户误以为生效时机不同。**③ 顺带订正文档笔误**: §3.11.1 原写皮肤「恰好 19 键」**实为 18** (键名清单 / `DefaultCommandInputSkin()` / `OptionsDTO.CommandInputSkin` / `CommandInputSkin.tmpl` 四处一致, 且 `skin_defaults_test.go` 硬断言字段数), 已订正并写明依据。**④ 记录一个新发现的既有风险 (本次不修)**: `sync-out` 的 robocopy 会用仓库侧旧默认覆盖部署树的 `CommandInputSkin.txt` —— 隔离目录实测: 源比目标旧时 robocopy 把源标为「较旧的」**却仍然复制** (复制 1 / 跳过 0)。**但影响是暂时性的**: 该文件是**派生产物**, 引擎每次启动都由 `GenerateScripts` 从 config.json 重新渲染, 下次引擎重启即自愈 (与 exe patch 的永久覆盖不同, 那个没有自愈路径)。**测试**: Go 侧 `TestCommandBoxAppearanceFromConfigFile` 扩为 **6 例** (新增「皮肤改动可被检出」—— 若两快照相等则皮肤改动永远不会触发结束命令框进程, 正是本改动的核心守门点); `go build` / `go vet` clean; C# 侧 `MotionSmokeTests` 的 `sectionBody` 计数 **9→8** (并注明 09-20 增卡 / 09-21 并卡的历史)。**门禁**: `dotnet build` 0 错误; `dotnet test` **303 总 / 281 通过 / 22 失败** —— 22 条为既有的 endpoint 契约用例 (缺 `%TEMP%\mk_settings_headless\settings.exe` 前置产物), 与基线逐条一致, **无回归**。**契约**: §3.11.1 皮肤段补「读取时机实证 + 并卡理由 + robocopy 覆盖风险」，字体段「写入端」由「独立字体卡」改为「皮肤卡内的字体小节」, 硬约束 7 扩展为「字体与皮肤都只在进程启动时读取一次」+ 外观合并判定 + 守门人 6 例, 键数笔误订正, 变更记录本行。 |
| 2026-09-21 | **修复 i18n 键 2517/2518 被误复用 (真实 UI 文案回归) + 字重变体可复现构建** (承接同日「字重档位真实生效」; 用户报障「字重下拉显示命令框皮肤」暴露的第一类问题的**同类残留**, 本次自查发现)。**① i18n 键撞车 (静默覆盖, 已在破坏 UI 文案)**: 字重 5 档的标签键首版用了 **2517/2518**, 而这两个键**早已被 `Views/SelectedActionPageView.axaml` 占用** —— 2517 = 自定义文本类型留桩提示条文案「该类型暂无专属行为，当前可用行为来自通用文本类型」, 2518 = 其跳转按钮「创建专属行为」。因 JSON **后定义覆盖前定义**, 加入字重键后这两处 UI 文案被**静默**改成了「极细」/「细」, 且键数虚增 (文件里 405 个定义, 但 `json.load` 解析后仅 **403** 个有效键)。根因是我把"看似空闲"的键号当成了空洞 —— 它们实际位于 `2500/2501/2502` 之后、`2519/2520` 之前, 与字重段**交错**。**修法**: 字重 thin/light 迁到经查证的真实空洞 **`2526`** (极细/Thin) 与 **`2549`** (细/Light); 删掉尾部覆盖性的 `2517`/`2518` 重定义; `SettingsPageViewModel.FontWeights` 改用新键并在注释里写死"741/2517/2518 均属别的文案域, 不可复用"。**② 可复现构建 (工具缺陷)**: `tools/font_weight_prebake.py` 初版每次烘焙产出不同 SHA —— 根因是 fontTools 的 `head.compile` 内有 `if ttFont.recalcTimestamp: self.modified = timestampNow()`, 默认 `True` 会用**保存时刻**覆盖 `head.modified` (实测两次独立烘焙仅差 **6 字节**, 全在 `head` 表)。这使变体无法用 SHA 断言/**门禁校验**, 部署也不可复现。**修法 (三工具同口径)**: `font_weight_prebake.py` / `font_embolden.py` / `font_weight_meta.py` 统一置 `f.recalcTimestamp = False` + 用**源字体的 `modified` 原值**填回 (`_source_modified()` 助手, 源异常回落 0) ⇒ 输出纯粹是输入的函数。**验证**: ① 两次独立烘焙 4 档变体 **SHA 全同**; ② 新产物与旧产物**逐表比对——除 `head` 外 13 表字节全同**, 证实修复不改字形, 只归一化时间戳。**③ 变体重烘焙 (用户批准)**: 用可复现方式重新生成 Downloads 下 4 个变体 (`c0e46bfd…` / `8112f389…` / `f805eff0…` / `40f4c4ba…`), 旧文件已备份至 `%TEMP%\kf_font_bak_20260921`。**④ 端到端复验 (真实字体 5 档全过)**: 临时 Go 用例 `TestE2E_RealFontAllTiers` 调 `InstallCommandFont` 真链路, 5 档落地 SHA 逐一对应 (thin `c0e46bfd…` / light `8112f389…` / regular `598f6226…`= 源本体 / semibold `f805eff0…` / bold `40f4c4ba…`), sfnt 签名均为 `0x00010000` 真 TrueType; 用后即删。**⑤ 门禁与部署**: Go `build`/`vet` clean, `gofmt` 无输出; `make buildServer` 重建后端 (13553664 B) 并补齐 `%TEMP%\mk_settings_headless\settings.exe` 前置产物后 **C# `dotnet test` 305/305 全绿** (303→305, 新增 2 例即本轮的重复键守卫生效), `dotnet build` 0 报错, `make analyzers` 双项目 exit 0; `make check` 全绿 (check-hooks 75/75 / lint CLEAN / check-texttypes / GenerateAHK + `/Validate` / ORACLE DIFF PASS); `make buildClientAvalonia` 后 `make sync-out` 完成 (patch 自动重施 `original`→`patched`)。**四产物 SHA 两侧全 MATCH**: `settings.exe 320986aa…` / `KeyFlux.Settings.dll a053c8a3…` / `i18n.json 4b670df9…` / `KeyFlux.ahk 279715a4…`; 部署树 i18n 复核 **405 键、零重复**, 2517/2518 **已恢复原意**, 下拉 5 档标签解析为「极细/细/常规/半粗/粗体」。**字节级验证**: DLL 内搜得新键 `2526`×11 / `2549`×4 (旧 `2517`×5 / `2518`×4 亦在, 因留桩提示条仍用它们)。**契约**: §3.11.1「写入端」段新增「UI 文案键必须查证为空闲」硬警示 (含**加键前必做两查**: 键是否已存在 + 行内计数 vs JSON 解析计数比对), 「烘焙工具」段补「可复现构建」条目 (附 `recalcTimestamp` 根因与逐表比对结论); 变更记录本行。**⚠ 待用户验证**: 需手动启动部署树 `KeyFlux.exe` (UAC 限制), 确认 ① 设置页「选项 → 命令框皮肤」卡内字重下拉显示「极细/细/常规/半粗/粗体」而非「命令框皮肤」; ② 切换到不同档位并保存后, 命令框笔画粗细**真的变化**; ③ 选「自定义文本类型」时留桩提示条文案正常 (不再显示「极细」)。 |
| 2026-09-21 | **命令框字体改定 Sarasa Gothic SC (抽 face + 激进裁剪 + 5 档重烘焙) + 字体选择即时校验提示** (承接同日「字重档位真实生效」; 用户报障「我选中了其他字体，可是感觉还是没什么区别啊。而且粗细的变化也跟没有一样」)。**① 根因确认 (用户的选择被静默跳过)**: 部署树 `data/config.json` 里 `commandFont` = `{sourcePath: "D:\\UserData\\Downloads\\更纱黑体_SC-v1.0.40\\Sarasa-ExtraLight.ttc", weight: "thin"}`, 但部署树 `bin/font/font.ttf` 复核仍是上一轮烘焙的 inpin 变体 —— 原因是 `.ttc` 为 **83 MB** > `FontMaxBytes` (32 MiB), `InstallCommandFont` 返回描述性错误后**被两处调用点刻意丢弃** (`_ =`), 于是全程静默 ⇒ 用户「选了跟没选一样」。**这条链路上没有任何一个环节会告诉用户为什么** —— 是本轮要根治的**设计缺陷**。**② 抽 face (用户批准「抽出 Sarasa Gothic SC」)**: Sarasa 把 6 风格 × 8 语言区共 **48 个 face** 打进单个 ~80 MB 文件, 而命令框 exe 只用 **第一个 face** —— 实测 face[0] = `Sarasa Gothic CL` (古典拉丁, **无简体中文字形**) ⇒ 直接选 `.ttc` 即使不超限也拿不到中文字。新增 **`tools/font_ttc_extract.py`** (`--list` 列全部 face / `--family "Sarasa Gothic SC"` 或 `--index N` 抽单 face), 抽出 face[1] = `Sarasa Gothic SC` (44,954,636 B, upem 1000, 48741 字形, 20992 CJK 统一汉字, glyf 轮廓)。**③ 激进裁剪 (用户批准「只保留简体中文常用区」)**: 44.9 MB 仍超限, 新增 **`tools/font_subset.py`** (Basic Latin→Fullwidth Forms + CJK 统一汉字基本区 U+4E00-9FFF; 排除 Ext-A/Ext-B+/CJK 兼容区) ⇒ **7,211,968 B (6.9 MiB)**, 24362 字形, 关键字符抽样全命中。**④ 源字体从 XLight 改用 Regular (关键决策)**: 首次用 XLight 源烘焙 5 档后**光栅化实测定序失败** —— r=−10 只剩 **46.9%** 墨迹, 反而比 r=−20 的 68.0% **更细** (腐蚀过猛已毁笔画, `令` 字 32px vs 96px)。改用 `Sarasa-Regular.ttc` 的 face[1] (45,149,496 B → 裁剪 7,146,908 B) 并做元数据对齐 (`usWeightClass 400→700` / `fsSelection 0xc0→0xa0` / `macStyle 0x0→0x1`) ⇒ `SarasaGothicSC-Reg.w700.ttf`。**⑤ 修 `PathOpsError` (工具健壮性)**: r=+28 时 skia-pathops 布尔并集对**退化轮廓**失败, 逐字形扫描定位到**仅 `uni57F3` 一个字形** (r=+14 全通过)。若因此中止则整档 `bold` 都拿不到 —— 代价与收益完全不成比例。`bake_one` 改为逐个字形捕获异常并**保留原轮廓** (该字形不加粗, 与邻居差 1 档以内), 计数回报; 新增 `UNSAFE_RATIO_MAX = 1%` 闸门 (超限才丢弃该档, 防「大面积退化却仍落盘」)。**⑥ 5 档烘焙 + 单调性实证**: thin −20 / light −10 / regular 0 / semibold +14 / bold +28 ⇒ 44px 实测 `thin 0.45x / light 0.71x / regular 1.00x / semibold 1.43x / bold 1.85x`, **严格单调递增**, 相邻增幅 29~58%, 总跨度 **+312%** ⇒ 每档肉眼可辨 (对比旧 4 档中→半粗仅 +3.0%)。**⑦ 端到端链路复验 (真实字体走真 Go 路径)**: 临时 Go 探针逐档调 `InstallCommandFont`, 5 档落地 SHA 与源变体**逐一对应** (thin `f4b7a3a3…` / light `917e03cc…` / regular `f8215778…` / semibold `6ce7cc18…` / bold `65c9d59c…`), 未知字重回落 regular 亦验证; 用后即删。5 档元数据复核全部 `glyf` + `usWeightClass=700` + `fsSelection=0xa0` + `macStyle=0x1` + 族名 `Sarasa Gothic SC` 保留 (⇒ **无 BOLDSIM 合成加粗**, 中文不糊)。**⑧ 新增「选择即时校验提示」(根治无反馈缺陷)**: 新增 `Models/CommandFontValidator.cs` 把生成端判据**前移到 UI 选择时刻** —— 体积 (>32 MiB) / sfnt 签名 (`0x00010000`·`true` 通过, `OTTO` 拒绝, `ttcf` 看首 face) / 可读性, 结论 + 原因**当场显示**在字体小节下方提示条 (AXAML `Border.noticeBanner` 两态: `.err` 暖红 = 不可用, 默认米色 = 可用但有注意点)。新增 i18n **5 键** (2588 超 32MB / 2589 轮廓不支持 / **2590 字体集合只用首个 face (唯一带占位符 `{0}`)** / 2591 已选定重启生效 / 2592 无法读取), `I18n.T` 增 `params object[]` 重载 (格式化失败**回退模板不抛异常**, 防界面崩溃), `ExpectedKeyCount` 405→**410**。🔴 **启动时不做复检** —— 配置里的路径很可能指向已删除/超限的文件 (正是用户上次踩的坑), 载入即弹告警会让用户每次打开设置都看到无法消除的提示 (噪声); 只在**用户主动选择**时给结论。**⑨ 落地**: `bin/font/` 同步 5 文件 (`font.ttf` + `.thin/.light/.semibold/.bold`), 部署树⇄源 **SHA 全 MATCH**; 原 `font.ttf` 备份为 `font.ttf.bak-prev-sarasa`; `data/config.json` 的 `commandFont` 更新为 `{sourcePath: "…\\SarasaGothicSC-Reg.w700.ttf", weight: "regular"}`。**门禁**: Go `build`/`vet` clean, `go test ./internal/script/` 字体用例 **18/18 全绿** (唯一红为既有基线 `TestGoldenKeyFluxAHK` 的 `CommandImeGuard.ahk` include 顺序); `make buildServer` 重建后端 (13553664 B) 并补齐 `%TEMP%\mk_settings_headless\settings.exe` 后 C# `dotnet test` **324/324 全绿** (305→324, 新增 19 例 = `CommandFontValidatorTests` 逐字节固定签名判定 + 32 MiB 限值与 Go 常量绑定 + 畸形 ttc 不越界读), `dotnet build` 0 错误。**契约**: §3.11.1「字重档位机制」补 4 条硬条目 (退化字形失败保原样 + `UNSAFE_RATIO_MAX` / 两个新工具 + 用途 / **半径不可跨字体照抄** (XLight 反例) / 精确墨迹实测数据), 「写入端」补「选择即时校验提示」段 (含判据同口径警示 + 启动不复检理由) 与「`.ttc` 只用第一个 face」条; 标题补「当前字体是仓库基线」提示 (用户部署树字体必然不同属预期); 变更记录本行。**⚠ 待用户验证**: 手动启动部署树 `KeyFlux.exe` (UAC 限制) 后确认 ① 命令框中文显示 Sarasa 且字形正常; ② 切换字重档位 + 保存后笔画**真的变化**; ③ 在设置里再选一个 `.ttc`/超大文件时**能看到**提示条与原因。**⚠ 未清临时目录 (待批准)**: `D:\tmp\kf_prebake_verify` / `D:\tmp\kf_r1` / `D:\tmp\kf_r2`。 |
| 2026-09-21 | **半粗设为默认字重 + 固化进「恢复默认」按钮 + 字体卡排版调整** (承接同日「命令框字体改定 Sarasa Gothic SC」; 用户要求「把当前设置的字体设置为默认字体，粗细要确保一致」)。**① 默认值口径订正 (`ConfigReadDefaults.DefaultCommandFontWeight`)**: 由 `regular` 改为 **`semibold`**。依据 = 用户实际在用的档位就是半粗 (部署树 `data/config.json` 实测 `weight:"semibold"`), 而原默认值 `regular` 会让「配置缺段」与「显式选了半粗」落到**不同笔画** —— 用户无法预期「为什么删了配置段字体就变细了」。新口径 = **默认值必须与用户实际在用的档位一致**。该常量**一处定义、三处生效** (刻意的单真源): ① 配置缺段时的读取默认值; ② `NormalizeFontWeight` 对非法值的回落; ③ `ResetCommandFont` 的落点。**② 与 Go 侧刻意"不同口径"(非缺陷, 已在两侧注释互相点名)**: Go `script.NormalizeFontWeight` 的回落**仍为 `regular`**。职责不同 —— C# 常量是「UI 层默认值」(用户在界面上看到并保存的就是它), Go 回落是「最后兜底」(仅在配置出现**未知档位名**时生效, 正常路径 UI 不可能写出这种值 ⇒ **正常路径两侧表现一致**: UI 写 `semibold` ⇒ Go 读到已知档位直接透传)。且 Go 侧**不知道用户选了什么源字体**, 回落中性档才不会在某些字体上把 CJK 字腔填死 (实测 Sarasa r=+28 把「令」的字腔压到 32px)。**③ 排版调整 (`SettingsPageView.axaml`)**: 字重 `ComboBox` 由「字体文件」行**下方**移到**上方** —— 字重是日常调节项 (5 档可点), 而字体文件路径是低频设置项 (选完基本不动), 且默认字重既已固定为半粗, 把「最可能需要动的控件」放在最上更符合操作频次。**④ 清掉 3 处过时注释**: 卡片注释与 VM 两处仍写「字重仅作记录 / 不参与渲染」—— 该说法在「字重档位真实生效」那一轮就已失效 (5 档为**独立预烘焙变体文件**, 通过 `VariantPath` 真实替换), 留着会误导后续维护者; 统一改写为「5 档为预烘焙变体, 默认 = 半粗」。**⑤ 测试**: `CommandFontContractTests` 新增 **`DefaultWeight_IsSemibold`** 钉死用例 (同时断言常量、缺段默认值、`NormalizeFontWeight(null)` 三处均为 `semibold`) + `NormalizeFontWeight_FallsBackToDefault_OnInvalid` 的 Theory 数据由 `regular` 改 `semibold` (null / "" / "REGULAR" / "ultra-black" / "medium")。**⑥ 验证 (端到端)**: ① 5 档变体文件解析实测 —— `semibold` → `SarasaGothicSC-Reg.w700.semibold.ttf` (SHA `6ce7cc18…`), 与部署树现行 `font.ttf` **同 SHA**; thin/light/bold 各自命中, `regular`/未知/空 → 源字体本体; ② 5 档元数据复核全部 `usWeightClass=700` + `fsSelection=0xa0` (WWS 置位, italic/regular 清零) + `panose.bWeight=8` + `macStyle.bold=1` + 24362 字形 ⇒ **无 BOLDSIM 合成加粗**; ③ **真实光栅化 44px 定序** (样本「命令框字重键盘映射」): `thin 0.38x / light 0.69x / regular 1.00x / semibold 1.36x / bold 1.77x`, **严格单调递增**, 相邻步进 +80.7% / +45.0% / +36.3% / +29.9%, 总跨度 **+363.6%**。**⑦ 门禁**: Go `gofmt` 修掉一处 `font.go` 注释缩进被 gofmt 判为代码块的**真实**格式问题 (CRLF 假阳性已用 `tr -d '\r'` 到真实路径复核区分), `go test ./internal/script/` 字体用例 **18/18 全绿**; `make buildServer` 重建后端 (`73b9b505…`) 并同步 `%TEMP%\mk_settings_headless\` 与部署树**三处同 SHA** ⇒ C# `dotnet test` **325/325 全绿** (324→325, 新增本轮的默认字重钉死例), `dotnet build` 0 错误, `make analyzers` 双项目 exit 0, `make check` 全绿 (check-hooks 75/75 / lint CLEAN / check-texttypes / GenerateAHK + `/Validate` / **ORACLE DIFF PASS**)。**⑧ 部署树同步 (发现并修正一处漂移)**: 仓库 `bin/font/` 居然只有一枚 **Sep 20 的陈旧 21MB `font.ttf`**, 4 个 Sarasa 变体**只存在于部署树** —— 正是 §3.11.1 早已记录的硬约束 (`sync-out` 的 robocopy 白名单**不含 `*.ttf`** ⇒ 换字体必须**手动同步两侧**)在上一轮只做了一半的后果。本轮补齐: 把部署树 5 文件复制回仓库 `bin/font/`, 双侧 **5 产物 SHA 全 MATCH** (`font.ttf` `6ce7cc18…` / thin `f4b7a3a3…` / light `917e03cc…` / semibold `6ce7cc18…` / bold `65c9d59c…`)。**⑨ 顺带修掉一个 sync-out 卡死**: `make sync-out` 在 `bin/` 那一趟 robocopy 上**挂死 37 分钟** (RSS 冻结在 6,400 K 达 60s 不动, 非机械硬盘慢)。放弃等待后改为**逐产物定向复制** (`settings.exe` / 5 字体 / `KeyFlux.ahk`), 并重跑 `make check-commandinput-patch` 确认 keycap patch 仍在位 (`0x1cca0` = `patched`, 部署树 exe SHA `2aed3232…` 未被 robocopy 冲掉)。**四产物两侧 SHA**: `settings.exe 73b9b505…` / `KeyFlux.Settings.dll 3519a608…` / `i18n.json ac9b6359…` / `KeyFlux.ahk 279715a4…`。**⚠ 待用户验证**: 手动启动部署树 `KeyFlux.exe` (UAC 限制) 后确认 ① 设置页「命令框皮肤」卡内**字重下拉在最上**、显示「半粗」; ② 点「恢复默认」后字重**回到半粗** (而非常规); ③ 切换档位并保存后命令框笔画**真的变化**。 |
