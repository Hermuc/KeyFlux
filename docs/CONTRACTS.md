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
│   └── builtins/  9 类内置动作各一文件 —— **已完成(阶段 3)**:`Actions.ahk` 的 42 个函数已按
│                  TypeID 拆入 `builtins/type{1,2,3,4,6,7,8,9}_*.ahk`(函数体逐行搬运,
│                  行多重集校验通过);`Actions.ahk` 保留为聚合 include 入口,模板与生成产物不变
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
├── server.go      gin 引擎装配、11 条路由注册、端口监听回退、headless 端口通告、openBrowser、PanicHandler
├── handlers.go    GetConfigHandler / SaveConfigHandler / GetShortcutsHandler / ServerCommandHandler / syncStartupFromRegistry
├── actionscheme.go  选中动作方案 REST API (6 handler + loadActionSchemes / saveActionSchemes / parseSchemeID)
└── dto.go         Config 及全部嵌套结构的 DTO 类型 + 双向映射 (model→dto 供 GET, dto→model 供 PUT)
config-server/internal/proc/                (共享子进程启动工具)
└── proc.go        ExecCmd / FallbackExecCmd (CREATE_BREAKAWAY_FROM_JOB + explorer 中转降级)
config-server/cmd/settings/                 (仅保留入口与模式判断)
└── main.go        main() CLI 分发 + debug/headless 判断 + 代码雨编排 + hideMatrix + server.Run() 调用
config-ui-avalonia/Resources/i18n.json      (双语文案真源, 308 键, UTF-8 无 BOM; 构建产物请勿手改)
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

### 3.8 ConfigProvider — 配置读取收口

```ahk
class ConfigProvider {
  static Load(path) => configMap       ; JSON 解析(AHK 无内置 JSON 库, 用随附 JSON.ahk)
  static SchemaVersion(cfg) => 1       ; 缺省视为 1; 迁移钩子按版本号链式执行
  static GetPluginSetting(pluginId, key) / SetPluginSetting(...)
                                       ; 独立存储于 data/plugin-settings.json
}
```


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
