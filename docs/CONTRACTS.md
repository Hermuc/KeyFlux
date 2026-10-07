# KeyFlux 架构契约文档(方案 D 定稿)

> 🔴 **2026-10-06 Go 后端退役**: `config-server/` 已整体删除。文中提及的 Go 实现真源
> （`internal/script/generators`、`internal/server`、`internal/proc`、`behaviors/`、
> `cmd/settings` 等）由 **`config-ui-reactor`**（Rust: 生成器 `src/generator/`、
> 后端 `src/server/`、CLI `src/bin/settings.rs`）接管；历史小节保留原文作为契约演化记录。

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
scripts/                                    (维护者脚本, 不随发布包出货, 与出货的 tools/ 区分;
                                             构建/运维 CLI 已 Rust 化: build-tools.go 于 2026-10-07
                                             迁入 config-ui-reactor/src/bin/build_tools.rs, 此处只剩 lanzou_client.py)
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
(注: 该示例插件已于 2026-09-24 按用户要求从仓库移除 —— 本节为**历史验证记录**,
链路与契约本身未变, 复现时需自备任一示例插件。)

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
(同上: 示例插件已于 2026-09-24 移除, 本节为历史记录。)

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
| 声明 | `plugin.json` 的 `settings[]`(key/type/label/labelEn/default/filter/hint/hintEn/min/max/maxLength), 校验在 `internal/plugins.ValidateManifest`(key 命名空间 `^[A-Za-z][A-Za-z0-9_]{0,31}$`、type 词表 `char/text/number/file/bool`、label 非空、key 唯一、声明 settings 必须申请 `settings` 权限、默认值自身必须合法) |
| 读取 | `GET /api/plugins/:id/settings` → `{id, settings[], values{}}`; values = 声明默认值合并已存值(未存过的键回落 default) |
| 写入 | `PUT /api/plugins/:id/settings`, body `{values:{}}`; 逐键按同一份声明校验(`ValidateSettingValue`: 长度/整数/闭区间/可打印字符/NUL), **整单拒绝**不做部分写入; 空串 = 删除该键(回落默认值) |
| 存储 | `internal/plugins.SettingsStore` → `../data/plugin-settings.json`, 与 `ConfigProvider.ahk` 的 `A_ScriptDir\..\data\plugin-settings.json` **同一文件**。两个必须守住的细节: ①**关闭 HTML 转义**(`SetEscapeHTML(false)`)—— AHK 的 `_Unescape` 只认 `\\ \" \n \r \t` 五种序列, Go 默认的 `\uXXXX` 会被原样留在值里; ②**原子落盘**(临时文件 + rename)—— AHK 每次触发都全量读该文件, 非原子写会让它读到半截 JSON 而丢掉全部设置。另: 未提及的键(含其它插件与外人不按约定写的非字符串条目)原样保留 |
| 界面 | `PluginSettingsDialogWindow` 按 `settings[]` 渲染五类编辑器(char/file+浏览/number/text/**bool 开关**), 卡片可点击条件 = `IsBuiltin || settings 非空`  |
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

> 🔴 **2026-10-02 订正**: 本节原样例与实现严重不符(旧文含 `runtime`/`provides` 字段、
> `entry` 为字符串、`settings` 为对象表、示例 id 用连字符) —— 均系**设计期草稿**, 从未实现。
> 以下为**实现真源**(`config-server/internal/plugins/plugins.go` 的 `Manifest` + 校验,
> 与 `config-ui-reactor/src/generator/plugins.rs` 同构; 双端单测守护)。实现先于本订正,
> 本节自 quick_switch 插件化 (P2-P5) 起即按此运作。

`data/plugins/<id>/plugin.json`(真源 `plugins/examples/`, 部署到 `data/plugins/`):

```json
{
  "id": "quick_switch",
  "name": "快速切换",
  "nameEn": "Quick Switch",
  "version": "1.0.0",
  "specVersion": 1,
  "description": "……",
  "author": "KeyFlux",
  "entry": { "kind": "script", "file": "main.ahk", "func": "QuickSwitchMain" },
  "permissions": ["window", "settings"],
  "settings": [
    { "key": "excludedPrefixes", "type": "text", "label": "排除目录前缀",
      "labelEn": "Excluded prefixes", "default": "",
      "hint": "……", "hintEn": "……", "multiline": true }
  ]
}
```

- 🔴 **id 词表(冻结)**: `^[a-z][a-z0-9_]{0,31}$` —— **下划线**分隔(如 `quick_switch`),
  不是连字符。随包内置 ID 集 = `BUILTIN_PLUGIN_IDS`(现仅 `quick_switch`; 导入 API 拒绝冒名,
  目录加载放行内置 ID —— 校验拆分见 2026-10-01 变更行)。
- `specVersion` 必须为 `1`(不等即拒绝); `name` 必填; `nameEn`/`version`/`description`/
  `author`/`permissions`/`settings` 可省略。
- `entry` 是**对象**: `kind` 当前仅支持 `"script"`(`{file, func}` 必填) —— 加载器对其他
  kind 产 `[插件错误]` 并跳过。`late`(2026-10-02 P7a, 可选): 晚初始化函数名
  (`^[A-Za-z_][A-Za-z0-9_]{0,63}$`), 生成端渲染进 `PLUGIN_LATE_INIT` 扩展点为**无参**调用
  (配置由插件运行时自取, P5 定式); 空省略 = 无晚初始化, 空块零字节。
  消费切换(P7b)后 quick_switch 的晚初始化特判将改走本声明。
- `permissions` 词表(冻结): `selection` / `window` / `run` / `settings` / `events`
  (`clipboard` 经 APIBridge 归并到 selection 命名空间)。**声明 `settings` 必须同时申请
  `settings` 权限**(校验器拒绝自相矛盾声明)。APIBridge 命名空间映射见
  `bin/lib/plugins/APIBridge.ahk`(`settings -> config.*`)。
- `settings` 是**数组**(每项一个 `Setting`), 不是对象表; 类型词表(冻结):
  `char` / `text` / `number` / `file` / `bool`(2026-10-01 P1; 值域严格 `"true"`/`"false"`)。
  每项: `key`(^[A-Za-z][A-Za-z0-9_]{0,31}$, 插件内唯一) / `type` / `label`(必填) /
  `labelEn` / `default` / `filter`(仅 file) / `hint` / `hintEn` / `min`+`max`(仅 number,
  整数闭区间) / `maxLength`(仅 text, 0=1024) / `multiline`(2026-10-02 P5, 仅 text:
  多行编辑器 + 换行分隔值)。
- **存储**: 设置值独立存 `data/plugin-settings.json`(扁平字符串 KV, 键 `<pluginId>:<key>`;
  唯一写入者 = 后端 GET/PUT `/api/plugins/:id/settings`; 引擎侧 `ConfigProvider.ahk`
  只读同一文件, 契约 §3.8)。空串 = 未设置(回落 `default`)。
- `provides`(2026-10-02 P7a, 可选): 能力提供块 `{actions: [...]}` —— 声明插件对外提供的
  **一等动作**(出现在主界面动作下拉)。每项: `id`(^[A-Za-z][A-Za-z0-9_]{0,31}$, 插件内唯一;
  全局动作 ID = `<pluginId>.<actionId>`, 与 ActionRegistry 键空间一致) / `label`(必填) /
  `labelEn` / `kind`(词表当前仅 `"plugin"`)。上限 32/插件; 声明空 `actions` 即拒绝。
  **P7b (同日) 消费切换**: 面板动作编辑器类型 9 分区**由目录 `provides.actions[]` 动态聚合**
  (核心零硬编码名单); 生成端 `callMap[9]` 改产出全局薄壳
  `PluginAction("<pluginId>", "<actionId>")`(bin/lib/plugins/PluginManager.ahk)经动作注册表
  间接寻址 —— 核心层对具体插件/动作名零知识(机械验收: grep 零命中, 残余仅注释/测试/兼容层)。
- `bundled`(2026-10-02 P7b, 可选 bool): **随包分发标记** —— 「内置/用户」判定的唯一真源
  (替代 P4 硬编码保留 ID 名单, 名单已删)。随包插件 manifest 携带; **导入 API
  (`ValidateManifest`) 见 `bundled:true` 即拒绝**(随包渠道专属, 第三方包不得冒用);
  同名目录冲突由 `InstallFromZip` 已存在检查兜底。面板 `is_builtin` = manifest.bundled。
- **动作绑定 (P7b 双字段过渡)**: typeID 9 的 `actionValueID` 语义收窄为**子类型标记**
  (值恒 9 = 「插件动作」, 不再是可选菜单项; 静态目录已移除该项), 具体动作由新字段
  `actionId`(json `actionId,omitempty`, 值 `<pluginId>.<actionId>`)承载 —— **新保存双写**
  (`actionValueID: 9` + `actionId`), 渲染时 `actionId` 优先; 旧式 `actionValueID:9` 无
  `actionId` 的绑定渲染为空(2026-09-23 起出厂配置已无此绑定, 无迁移需求)。数字字段
  退役另立 compat 期批次。
- L1 插件入口约定: `entry.func`(如 `QuickSwitchMain(api)`)由 PluginManager 在引导点调用,
  `api` 为按 `permissions` 裁剪的 APIView; 「必须晚于 InitKeymap」的初始化走
  `{{ PLUGIN_LATE_INIT }}` 晚初始化扩展点(空块零字节; **P7b 起仅由 `entry.late` 声明驱动**,
  生成器不再有任何插件特判)。

## 5. 生成端契约（Go → 2026-10-06 起由 config-ui-reactor 接管）

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

## 5.1 settings.exe `--headless` 模式契约(2026-08 冻结; 实现已由 Rust `server::run_headless` 接管)

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
| `POST /api/selected-action/play` | **直接序列化 model** (未经 DTO) | 彩蛋 (▶ 真实执行): 白名单校验 typeId → 原子写请求文件 `%TEMP%\kf_play_request.json`, 由 AHK 引擎轮询消费 (见 §5.2.1) |
| `GET /shortcuts` | 内联结构体, 无 model 依赖 | 无需 DTO |

**改 json tag 须同时改两处**: `internal/script/model/types.go` (存储模型) 与 `internal/server/dto.go` (传输 DTO);
漏改任一侧会导致 GET/PUT wire 不一致或落盘字段丢失。

**三处同步清单 (实测教训)**: 新增 `options` 子段时须**同时**改动 ① C# `config-ui-avalonia/Models/ConfigModels.cs`
的 `Options` ② Go `model/types.go` 的 `Options` ③ Go `dto.go` 的
`OptionsDTO` + `optionsToDTO()` + `dtoToOptions()` **五个落点** (DTO 结构体 + 两个转换函数各一处)。
漏 `optionsToDTO` 会让 GET 少字段, 漏 `dtoToOptions` 会让 PUT 静默丢字段 (G1 教训)。
新增段**必须**走全五处 (例: `options.commandFont`, 2026-09-20)。

守护测试: C# 侧 `ModelUnitTests.Options_And_SubStructs_JsonNames_MatchGoTags` 锁键名集合;
Go 侧 `server/dto_test.go` 的 `Test<X>RoundTrip` 锁 PUT→model→GET 往返与空段恒对象契约。

**RestartFailed 双份定义 (残留耦合, 登记为后续项)**:
`model.ActionScheme.RestartFailed` 与 `ActionSchemeDTO.RestartFailed` 同时存在。
action-scheme 端点直接在 model 上设置该字段后序列化返回, 未经 DTO 转换;
移除 model 侧定义需先将 action-scheme 端点 DTO 化, 属后续清理范围。

### 5.2.1 彩蛋通道: `POST /api/selected-action/play` → 请求文件 → AHK 轮询 (2026-09-22 冻结)

「选中动作」页聚合卡头部的 ▶ 按钮 (XAML 绑定 `Detail.PlaySampleCommand`, Tooltip 用 i18n `990`)
改造为**彩蛋**: 用该行类型真实配置的行为, 经本通道让 AHK 引擎**直接执行预设样例** (用户选定
「走引擎真实执行」这一最忠实方案, 而非前端模拟)。

**数据流 (前端 ▶ → Go 端点 → 请求文件 → AHK 轮询 → `_Execute`)**:

1. **前端 → 后端**: `MappingRowVm.PlaySampleCommand` → `SelectedActionPageViewModel.PlaySampleAsync(typeId)`
   → `SettingsApiClient.PlaySelectedActionAsync(typeId)` (`POST /api/selected-action/play`, body `{"typeId":"..."}`)。
   `typeId` 取该行的类型标识 (`MappingRowVm._typeId`): 内置文本特征 / `group:<name>` / `type:<id>`。
2. **后端路由/处理器** (`internal/server/selectedaction_play.go`): 白名单校验 → 折叠 → 原子写 → 返回 `{"ok":true}`。
   - **白名单校验** (否则 400): 仅接受 ① 内置文本特征值 (`url`/`path`/`magnet`/`bilibili`/`plain`)
     ② 已配置的 `group:<name>` (在 `config.FileGroups` 中存在) ③ 已配置的 `type:<id>` (在 `config.MatchTypes` 中存在)。
   - **group 折叠**: `group:<name>` 由后端解析为规范化后缀串 `strings.Join(g.Exts, ",")`, 写入请求文件
     (引擎侧按 `matchValue` 精确命中组, 无全局 groups 表)。
   - **原子写**: 先写同目录临时文件 (`os.CreateTemp`) 再 `os.Rename` 到最终名, 避免引擎读到半截内容;
     文件落点 `%TEMP%\kf_play_request.json` (`os.TempDir()`), 内容 `{"typeId":"...","seq":<自增>}`。
   - `seq` 为进程内 `atomic` 自增序号, 供引擎去重。
3. **引擎侧轮询** (`bin/lib/rules/SelectedAction.ahk`):
   - `SelectedActionInit` 留存 entries 副本 (`static Data := entries`) 并注册 `SetTimer(SelectedAction.WatchPlayRequest, 250)`。
   - `WatchPlayRequest` 每 250ms: `FileExist` 不存在即零开销返回; 存在则读文件 → 正则提取 `typeId`/`seq`
     (引擎无内置 JSON 库, 用受控格式正则) → `seq` 去重 (`static PlayLastSeq`) → **执行后 `FileDelete` 删除文件
     (无论成败, 幂等)** → 调 `PlaySample(typeId)`。
   - `PlaySample(typeId)`: 按 typeId 构造硬编码样例 `selected` (文本特征→文本样例; 文件后缀/`type:<id>` fileExt→
     `{type:"file", content: A_Desktop}`), 经 `_FindGroup(typeId)` 按 `matchValue` 精确命中组, 取**组内首条**
     entry 走 `_Execute` 真实执行 (与菜单序号 1 等价, 不改菜单逻辑); 未命中/未配置用现有翻译文案 Tip 提示 (不新增 i18n 键)。
   - **样例内容 (硬编码, 不进配置)**: `url`→`https://github.com/Hermuc/KeyFlux`; `path`→`A_Desktop`;
     `bilibili`→`BV1xx411c7mD`; `magnet`→`magnet:?xt=urn:btih:0000000000000000000000000000000000000000`;
     `plain`→示例文本; `group:*`/`type:*`(fileExt)→`{type:"file", content: A_Desktop}`; 自定义 text 类型→按 text 处理。

**安全边界** (与引擎对称, 缺一不可):

- **仅白名单 typeId**: 后端 `resolvePlayTypeId` 拒绝一切非白名单值 (空串 / 未知串 / 不存在的 group/type 一律 400),
  引擎端 `PlaySample` 不接收任何来自请求文件之外的参数。
- **样例硬编码**: 执行内容全程硬编码在 AHK 端, 请求文件只含 `typeId` + `seq`, **不含任何命令/路径参数** ⇒ 无注入面。
- **执行后删文件**: `WatchPlayRequest` 无论成败都 `FileDelete`, 保证幂等、不残留、不被重复触发; 文件不存在时零开销返回。

**部署提示**: 改 `bin/lib/rules/SelectedAction.ahk` ⇒ 定向 cp 到部署树 `…\KeyFlux-1.0-beta1\bin\lib\rules\SelectedAction.ahk`,
且引擎需**重载** (托盘 Reload / `Alt+'`) 后彩蛋才生效; 后端改 Go ⇒ `make buildServer` + `settings.exe` 三处同步。

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

> 完整变更流水已拆分为独立文件：见 [CHANGELOG.md](./CHANGELOG.md)（2026-10-07 起独立维护，
> 该段逐字节搬移）。契约正文在本文件，后续变更流水不再追加于此。
