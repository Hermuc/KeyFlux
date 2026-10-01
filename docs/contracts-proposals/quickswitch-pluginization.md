# 提案: QuickSwitch 插件化 —— 把内建模块降级为可启停 / 可删除的随包插件

> 状态: 待评审 (本文件为 CONTRACTS 约束 #2「接口先行」的提案载体; 实现随本提案分批提交)
> 关联契约: `docs/CONTRACTS.md` §0 总原则 1/2/3、§1 约束 #1/#4/#5/#6/#7、§3.4 ActionRegistry、
> §3.7 PluginManager/APIBridge/ScriptHost、§3.8 ConfigProvider、§4 插件清单格式、§5 生成端契约
> 落地文件: `plugins/bundled/quick_switch/`(真源, 新增) · `data/plugins/quick_switch/`(部署, 新增) ·
> `config-server/templates/keyflux.tmpl` · `config-ui-reactor/src/generator/{template,plugins,actions}.rs` ·
> `config-ui-reactor/src/{services,server,ui}/**` · `bin/lib/quickswitch/**`(移出) ·
> `bin/lib/actions/ActionRegistry.ahk`
> 分支: `agent/quickswitch-pluginization`

---

## 0. 结论先说

1. **这不是"新增插件化"，而是"补齐一个项目自己已经声明过的架构"。**
   `CONTRACTS.md` §0 总原则 3 原文：「**同接口双挂载**：内置能力与第三方插件实现同一组契约;
   内置为静态挂载(编译期), 第三方为动态挂载(运行时)」。QuickSwitch 现在**只挂了静态那一半**，
   动态那一半的框架（`PluginManager` / `ActionRegistry` / `APIBridge` / `ConfigProvider`）
   早已落地并通过冒烟测试。本提案做的是**把 QuickSwitch 从"静态挂载"迁到"动态挂载"**。
2. **真正的阻塞点只有一个**：键位编译器 `generator/actions.rs:411` 对 `type9 / valueID 9`
   **直接吐函数调用 `QuickSwitchGoto()`**。插件一旦删除，这个符号不存在 ⇒ 引擎加载/热键执行失败。
   其余 7 个耦合点都只是"没有走插件通道"，本身可安全拆除。
3. **删除的安全性 ≠ 完全解耦，两者要分两期拿**：
   - **安全删除**（Phase 3，改动可控）：动作分发改为"经注册表 + 优雅降级"，删插件后键位降级为
     "该动作不可用"，引擎不崩。此时核心**仍知道**存在一个叫"快速切换"的动作位。
   - **完全解耦**（Phase 6，需改协议）：动作列表由插件 `provides.actions` 动态生成，配置存
     字符串动作 ID，核心对 QuickSwitch **零知识**。代价是 protocol 变更 + 基线全量重录。
4. **推荐路线**：按 Strangler Fig 分 7 期渐进，每期独立可发布、可回滚，绝不 big-bang 重写。

---

## 0.5 P0 实测结果（2026-10-01 23:30，已完成）

**问题**：AHK v2 里"调用一个不存在的函数"，是**加载期**报错还是**运行期**报错？这决定 C4 的事故等级。

**方法**：用 `bin/AutoHotkey64.exe` 跑三组最小脚本，以**退出码**为信号（避开文件写入在沙箱下的干扰）：

| 脚本 | 内容 | 观测 | 结论 |
|---|---|---|---|
| `ExitApp(5)` | 仅一行 | `EXIT=5` | 调用方式本身正常 |
| `#Requires AutoHotkey v2.0` + `ExitApp(13)` | 验证指令不干扰 | `EXIT=13` | `#Requires` 不是混淆变量 |
| **`ExitApp(11)` → `NotAFunction_XYZ()` → `ExitApp(12)`** | 调用在 `ExitApp(11)` **之后** | **未退出（超时）** | **`ExitApp(11)` 根本没被执行 ⇒ 加载期即失败** |

**辅助证据**：早期两个探针（顶层直接调用 / **藏进 `Map` 的 lambda**）都以 `EXIT=2` 结束，
且**第一句 `FileAppend` 的输出文件从未生成**（= 一条语句都没执行）。

**结论**：
1. 🔴 **未定义函数调用是 AHK v2 的加载期致命错误** —— 不是"那个热键失效"，是**整个引擎脚本无法加载**。
2. **lambda 里也一样** ⇒ `km.Map("hotkey", _ => QuickSwitchGoto())` 这个真实形态同样致命。
3. ⇒ **C4 的风险等级从"高"上调为"最高"**，且证明 P4 是**删除功能的前置条件**，不能后置。
4. ⇒ 任何"删掉 QuickSwitch 但保留 type9 键位绑定"的中间状态，都会让引擎**完全起不来**
   （不是降级）。这使 P4 必须先于 P6 从"建议"升级为**硬约束**。

**附：初始化时序（P0 第二项，已测绘）** —— 生成产物实际调用顺序：

```
L43  SetWorkingDir("../")
L44  PluginManager.Register(...)          ← {{ PLUGIN_BOOTSTRAP }} 注入点
L45  PluginManager.LoadEntry(...)
L46  OnError(EngineOnError)               ← 引擎级异常兜底在这里才装上
L48  InitTrayMenu()
L49-58  CommandInputHooks.Register(ImeInputHost) / ImeInputHost.Enable() / CommandImeGuard
L59  InitKeymap()
L60  InitQuickSwitch({...})               ← 硬编码，位于 InitKeymap **之后**
L61  OnExit(KeyFluxExit)
```

🔴 **两个必须处理的事实**：
- 插件引导点在 **L44**，而 `InitQuickSwitch` 在 **L60** ⇒ **迁到插件会显著提前调用时机**。
- 若要保序，**不能**直接复用 `{{ PLUGIN_BOOTSTRAP }}`，需要引入一个**插件无关的"晚初始化"扩展点**
  （默认不产出任何字节 ⇒ 现有 12 份基线零变化）。
- 且 **L60 在 `OnError(EngineOnError)`（L46）之后**，而 L44 在**之前** ——
  初始化提前会让 QuickSwitch 的异常**逃出引擎兜底**。这是 P3 之前必须定案的设计点。

---

## 1. 背景与范围

### 1.1 诉求

让 QuickSwitch 与主程序解耦，用户可以**自由启用 / 禁用 / 删除**它，且**不影响系统其他部分**。

### 1.2 现状规模

| 项 | 值 |
|---|---|
| 插件代码 | `bin/lib/quickswitch/` 6 模块 **1403 行**（QuickSwitch 408 / DialogInspector 304 / QuickSwitchUI 258 / HistoryStore 216 / FolderRanker 152 / FolderHistory 65） |
| 位置 | 引擎核心目录 `bin/lib/`，与 `core/` `actions/` 平级 ⇒ **不是**插件目录 |
| 生成产物 | `bin/KeyFlux.ahk` 第 27-28 行 `#Include`，第 65 行 `InitQuickSwitch(...)` |
| 配置 | `options.quickSwitch` **9 字段**（4 bool + 4 int + 1 字符串数组） |
| 动作 | 内建 TypeID 9 的**子项 9**（"快速切换"） |
| 数据 | `data/quickswitch/history.tsv` |
| i18n | `bin/lib/core/translation.ahk` 4 处；面板 `i18n.json` 若干 |

### 1.3 范围

**含**：注入通道迁移、初始化迁移、动作分发改造、配置段迁移、UI 卡片统一、删除语义与 tombstone、
生命周期清理、基线重录、CONTRACTS 修订。
**不含**：QuickSwitch 的功能变更 / UI 重设计 / 历史数据格式变更。

---

## 2. 业界调研

调研覆盖 4 类真实产品形态 + 3 组工程范式。**核心结论：主流产品的做法高度一致 ——
"功能随包分发 + 走与第三方完全相同的插件契约"，差别只在"可删"还是"仅可禁用"。**

### 2.1 四种产品形态

| 产品 | 内建功能如何组织 | 启停 | 删除 | 对本项目的启示 |
|---|---|---|---|---|
| **Obsidian** | 「核心插件」**本身就是插件**，与社区插件走**同一套 Plugin API**；大多是**默认关闭** | ✅ 图形开关 | ❌ 不可删（随应用包） | **最贴近目标态**：核心功能=插件，只是分发渠道不同 |
| **Neovim** | 自带插件放在 `pack/dist/opt/`（如 `matchit`），属 **opt 而非 start** | ✅ `:packadd` 按需加载 | ✅ 目录可删 | **"随包但可选"的标准做法**：`opt`/`start` 二分 |
| **VS Code** | 内建扩展**硬编码在二进制**（`builtInExtensions.js`） | ⚠️ 可禁用，但**只阻断 `activate()`，不撤销 contribution 注册** | ❌ 明确报错 "is built-in and cannot be uninstalled" | **反例**：正是本项目现在踩的坑（"看着有开关，其实是半个"） |
| **Vite DevTools / Koishi** | 插件用 `ctx.commands.register()` 注册命令并返回 `handle.unregister()` | ✅ | ✅ | **命令注册表 = 唯一真源**，卸载即命令消失 |

### 2.2 三组工程范式

**① 微内核 / 插件扩展架构（arc42 · Richards《Software Architecture Patterns》）**
> 核心只留最小内核 + 稳定扩展点；功能全部以插件形式到达；**"Microkernel: the limiting case,
> where even built-in features are plugins"**。
机制：核心声明扩展点 → 插件按契约实现 → 通过**目录扫描 / 服务注册表 / 运行时热插拔**被发现 →
宿主只调扩展点、不知道具体类型。隔离是"旋钮"：进程内最快，子进程/沙箱最安全。

**② 命令注册表（"The Command Palette Is an Architecture, Not a Widget"）**
> 把「命令」当**数据**而非 UI 元素：`{id, title, run, isEnabled, shortcut}`；
> **一个 Registry 做唯一真源**，所有消费面（面板/菜单/快捷键）都从它读。
> **依赖方向**：功能依赖 Registry（稳定共享），面板依赖 Registry，**两者互不依赖**。
> ⇒ "**Remove the feature, its cleanup runs, the commands vanish.**"
这正是本项目 `ActionRegistry` 已有的形状（`Type -> IAction`），只需把 QuickSwitch 从
"编译期直连"改成"注册表成员"。

**③ 生命周期与资源回收（Obsidian Lifecycle / Koishi Reversibility / AuditBuffet ab-002086）**
> - **可逆性保证**：任意次 load/unload 后状态一致、无内存泄漏、**不对其他插件留残留**
> - **对称清理**：`pluginDidLoad` 里分配的每一项，必须在 `pluginWillUnload` 里镜像释放
>   （全局快捷键 / 窗口 / 监听器 / 定时器 / 缓存）
> - **宿主侧安全网**：宿主按插件维度记录 disposables，卸载时**即使插件作者漏写**也强制回收
>   （防"禁用后定时器还在跑，只有重启才真正移除"）

### 2.3 迁移范式：Strangler Fig + Branch by Abstraction

> **永不 big-bang 重写正在使用的系统**（Netscape 教训）。做法：先插**门面（facade）**并让它
> 100% 走旧路径（一次 no-op 发布，证明门面可用）→ 按**业务价值高、耦合低**的顺序逐个抽取 →
> 用 **feature toggle** 做切换闸门与 kill switch → 最后一个端点翻完、旧码零流量后再删旧码，
> 并**举行"葬礼"以防复活**。
配套：**Branch by Abstraction**（同一 codebase 内换实现）与 **Parallel Run**（新旧并行比对，
差异率归零才切）。

**本项目对应**：`{{ PLUGIN_INCLUDES }}` 占位符 = 已存在的门面；`factory-plugins` parity 语料 =
现成的并行比对；enable/disable 开关 = 现成的 feature toggle。

---

## 3. 现状测绘：8 个耦合点

> 全部经现场取证（文件:行号）。

| # | 耦合点 | 位置 | 性质 | 拆除难度 |
|---|---|---|---|---|
| **C1** | 模板硬编码 6 行 `#Include` | `keyflux.tmpl:23-28`；镜像于 `generator/template.rs:384-389`（编译期常量） | 不走插件通道 | 低 |
| **C2** | 模板硬编码 `InitQuickSwitch(...)` 调用 | `keyflux.tmpl:62`；由 `generator/template.rs:80-107` 逐段 `push_str` 拼出 | 不走插件通道 | 低 |
| **C3** | 专用配置段 `options.quickSwitch`（9 字段） | `generator/model.rs:204-223`；`models/config.rs:350+`；默认值契约在三处维护（`generator/config.rs:51-68` `default_quick_switch_option` / `is_quick_switch_zero`） | 未走 `plugin-settings.json` | 中 |
| **C4** | 🔴 **键位编译期直连 `QuickSwitchGoto()`** | `generator/actions.rs:411`（`keyflux_actions9`，`value_id 9 => "QuickSwitchGoto()"`） | **删除即致命** | **高** |
| **C5** | `ActionRegistry.BuiltinTypes` 含 `keyfluxActions`，`Unregister` 拒绝注销内置 | `bin/lib/actions/ActionRegistry.ahk`（`BuiltinTypes` 数组；`Unregister()` 早退分支） | 阻止动态化 | 中 |
| **C6** | UI 内置卡片**硬编码合成** | `services/plugins.rs:69 is_builtin()`、`:82-83`（写死 `can_delete:false`）、`:104/:112-114`（`builtin => can_delete=false`）、`:133-134 apply_enabled` 内置分支只写 `collectEnabled` | 卡片不是扫描来的 | 中 |
| **C7** | 删除后端**硬拒绝** | `server/handlers_plugins.rs:528`（`if BUILTIN_PLUGIN_IDS.contains(&id) { return Err(...) }`） | 直接拦截删除 | 低 |
| **C8** | 内置 ID 保留集 + 卡上开关语义错位 | `generator/plugins.rs:29 BUILTIN_PLUGIN_IDS`、`services/plugins.rs:19`；UI 开关只写 `collectEnabled` ⇒ **关了采集，浮层 `autoShow` 与 type9 动作仍可用** | 语义不实 | 低 |

**两处已具备的有利条件（不需要新建）**

- **插件注入通道已可用且有字节级回归门**：`render_plugin_blocks()`（`generator/plugins.rs:436`）
  扫描 `<config.json 同级>/plugins`（即 `data/plugins`），产出
  `#Include ../data/plugins/<id>/<file>` + `PluginManager.Register(<manifest>)` +
  `PluginManager.LoadEntry("<id>")`；**入口文件存在性在生成期校验**（缺失即跳过并留注释，
  这正是"删插件不拖垮引擎"的现成保险）。parity 语料 **`factory-plugins`** 已专门覆盖该路径
  （`tools/parity/README.md:56`：ahk 22766 → 23415 字节，证注入生效）。
- **插件运行时有完整 API 面**：`APIBridge` 七命名空间（selection / window / send / run / ui /
  config / events），`PluginManager.LoadEntry` 以动态调用拉起 `<entry.func>(api)` 并做
  `plugin_error` 异常隔离；`CONTRACTS §3.7` 已冻结 `Unload(id)`；设置独立存
  `data/plugin-settings.json`（§1 约束 #5）。

---

## 4. 目标架构

```
┌─ 微内核（bin/lib/，不含任何 QuickSwitch 代码）──────────────────────────┐
│  core/      EventBus · KeymapManager · Utils · CommandDisplay …        │
│  actions/   ActionRegistry(唯一动作真源) · IAction · IRegistration     │
│  plugins/   PluginManager · APIBridge · ConfigProvider · ScriptHost    │
│  quickswitch/  ← 🗑 整体移除                                            │
└───────────────────────────────────────────────────────────────────────┘
        ▲ 编译期组合（生成器扫描目录 → #Include + Register + LoadEntry）      ▲
        │                                                                  │ 运行时契约
        │                                                                  │
┌─ 随包插件（plugins/bundled/quick_switch/ → 部署到 data/plugins/）────────┐ │
│  plugin.json   id=quick_switch · entry.func=QuickSwitchMain            │ │
│                settings[]  ← 原 options.quickSwitch 9 字段              │ │
│                provides.actions[]  ← 声明贡献的动作（Phase 6）          │ │
│  main.ahk      QuickSwitchMain(api) → 注册动作 + 订阅 EventBus          │ │
│  src/          FolderRanker · HistoryStore · FolderHistory             │ │
│                DialogInspector · QuickSwitchUI · QuickSwitch           │ │
└────────────────────────────────────────────────────────────────────────┘
        ▲
┌─ 配置 ─────────────────────────────────────────────────────────────────┐
│  data/config.json         options.plugins.{disabled[], removed[]}       │
│  data/plugin-settings.json  { "quick_switch": { …9 字段… } }             │
│  data/quickswitch/history.tsv   （插件私有数据，随插件删除而清理）        │
└────────────────────────────────────────────────────────────────────────┘
```

**与第三方插件完全同构**：QuickSwitch 与 `everything_search` 使用**同一个 manifest 格式、
同一条注入通道、同一组运行时 API、同一张设置存储、同一套启停/删除流程**。唯一区别是它
**随包分发**（默认在 `data/plugins/` 里，而非从市场安装）。

---

## 5. 四个关键设计决策

### D1. 动作分发：从"编译期直连"改为"注册表 + 优雅降级"（解 C4/C5）

**问题**：`generator/actions.rs:411` 对 `value_id 9` 直接吐 `QuickSwitchGoto()`。
插件删除后该符号不存在；AHK v2 对**直接函数调用**做加载期校验，
`Call to nonexistent function` 会中止脚本加载（与本项目已记录的
「`#Include` 指向缺失文件会拖垮整个脚本加载」同量级事故）。

**Phase 3 方案（安全删除）**：把直连换成**核心通用门** `PluginAction(pluginId, actionId)`：

```ahk
; 生成产物：不再直连插件函数
km.Map("CapsLock & q", _ => PluginAction("quick_switch", "goto"))
```

```ahk
; core 侧新增（对 QuickSwitch 零知识，只知道"有个插件动作位"）
PluginAction(pluginId, actionId) {
  action := ActionRegistry.Get(pluginId "." actionId)
  if (action == "") {                      ; ← 插件缺失 / 未注册
    ActionRegistry._log("plugin action unavailable: " pluginId "." actionId)
    Tip("该动作不可用（插件未安装或已禁用）")
    return
  }
  ActionRegistry.Execute(pluginId "." actionId, {})
}
```

效果：删插件 ⇒ 键位**降级为提示**，引擎不崩、其余功能不受影响。
`ActionRegistry.BuiltinTypes` 中的 `keyfluxActions` 保留（type1-8 的快路径直连红线不动），
但**子项 9 不再直连插件**。

⚠️ **快路径红线（§0 总原则 2 / 约束 #6）不受影响**：红线只约束"重映射 / 发键 / 鼠标"三类
编译期原生注册动作；QuickSwitch 是"开 GUI + 轮询"的重动作，本就走运行时路径。

**Phase 6 方案（完全解耦，终态）**：manifest 增加 `provides.actions[]`，动作列表由注册表
动态生成（下拉选项不再写死"快速切换"），配置存**字符串动作 ID** 而非 `type/valueID` 数字
⇒ 核心对 QuickSwitch **零知识**，跨插件动作命名空间天然隔离（对照 Obsidian `addCommand`
的 `id` + Vite DevTools 的 `my-plugin:action` 命名约定）。

### D2. 配置迁移：`options.quickSwitch` → `plugin-settings.json`（解 C3）

**协议缺口**：`SETTING_TYPES = ["char","text","number","file"]`（`generator/plugins.rs:41`，
两端同步词表）。9 个字段里 `number` 有 4 个（✅ 可直迁）、**`bool` 缺 4 个**、
**字符串数组 `excludedPrefixes` 缺 1 个**。

**决策**：**扩展协议加 `bool`**（必需 —— 4 个开关，且插件卡片本身就需要布尔语义），
在 `SETTING_TYPES` 加第 5 项并同步两端校验 + `plugins_view` 渲染 + `ConfigProvider` 读取；
`excludedPrefixes` **先复用 `text` 多行**（换行分隔）以**避免一次改动过大**，
`list` 类型留作后续独立提案（Phase 5 再评估是否需要专用编辑控件）。

**一次性迁移**：首次启动检测 `options.quickSwitch` 非零（复用现成的 `is_quick_switch_zero()`
签名判定）→ 写入 `plugin-settings.json` 的 `quick_switch` 段 → 标记
`options.quickSwitch` 为 `deprecated`。
**必须保留旧字段的读取兼容至少一个版本**（约束 #5 配置兼容），否则回滚即丢配置。

### D3. 删除语义：tombstone 防复活（解 C7/C8）

**难点**：删除必须是"真删除"，但 `make sync-plugins` / `deploy` 用 robocopy
（`Makefile:232` / `:244`）会把随包插件再拷回来 ⇒ **删除的持久性是假的**（结构性复活）。

**方案**：
1. 删除 = `remove_dir_all(data/plugins/quick_switch)` **+** 写墓碑
   `options.plugins.removed += ["quick_switch"]`。
2. 部署/升级脚本读 config 的 `removed` 表，**跳过** tombstone 中的 ID。
3. UI 提供"恢复随包插件"入口（清墓碑 + 重新落盘）。
4. `options.plugins.disabled` 与 `removed` **语义严格分开**：`disabled` = 不注入不加载但保留文件；
   `removed` = 文件已删（`LoadCatalog` 的"缺目录"路径已天然正确，`plugins.rs:339`
   `NotFound` 不记错误）。

### D4. 生命周期与资源回收（防 C8 语义错位 + 卸载残留）

QuickSwitch 持有 **定时器（轮询）+ GUI 浮层 + 事件订阅 + 历史文件句柄**，
必须在 `pluginWillUnload` 里**对称释放**（业界"可逆性保证"）：

| 分配点 | 回收点 | 现状 |
|---|---|---|
| `SetTimer` 轮询（`pollIntervalMs`） | `SetTimer(..., 0)` | 需补 |
| 浮层 GUI 对象 | `Destroy()` | 需补 |
| `EventBus.Subscribe` 订阅 | `Unsubscribe` | 需补 |
| `history.tsv` 句柄 / 缓存 | 释放 + 落盘 | 需补 |
| 动作注册 | `ActionRegistry.Unregister` | 需补（并同步解除 C5 的早期直连） |

同时按业界"宿主侧安全网"做法，在 `PluginManager.Unload(id)` 内**按插件维度强制回收**
（即使插件作者漏写），并加**"连续 load/unload 100 次内存与句柄数持平"**的验收项。

### D5（附带）. C8 的语义错位是本提案的"顺带修复项"

现在 UI 内置卡的开关写 `collectEnabled`，但 `autoShow`（浮层自动弹出）与 type9 动作
**都不检查**该位 ⇒ 用户"关掉插件"后它仍在弹。插件化后开关语义天然正确
（`disabled` ⇒ 不注入 ⇒ 无定时器、无浮层、无动作），**这是插件化的直接收益**。

---

## 6. 分阶段实施（Strangler Fig）

> 每期都以**独立可发布 + 可回滚**为前提；每期结束必须 `make check` 全绿。

| 期 | 目标 | 关键动作 | 基线影响 | 回滚 |
|---|---|---|---|---|
| **P0 探针** | 钉住事实 | ① 验证 AHK v2 对未定义函数是**加载期**还是**运行期**报错（决定 C4 事故等级）；② 写一个"删插件 + 保留 type9 键位"的失败复现；③ 确认 `PluginManager.Unload` 现有实现程度 | 无 | 无（只读 + 探针） |
| **P1 协议扩容** | 加 `bool` 设置类型 | 两端 `SETTING_TYPES` 同步 + 校验 + UI 渲染 + `ConfigProvider`；补契约测试 | 无（未引用） | revert |
| **P2 代码搬家（双轨）** | 插件成为可加载单元 | `bin/lib/quickswitch/**` → `plugins/bundled/quick_switch/`（真源）+ `data/plugins/quick_switch/`；写 `plugin.json` + `main.ahk`；**模板暂时仍硬编码 include 指向新位置** ⇒ 产物应字节等价 | 比对（应零差异） | revert |
| **P3 切注入通道** | 走 `{{PLUGIN_INCLUDES}}` | 删 `keyflux.tmpl:23-28` 与 `template.rs:384-389`；初始化从"模板注入参数字面量"改为"插件经 `ConfigProvider` 自取" | **重录 12 份基线** | revert + 重录回滚 |
| **P4 切动作分发** | 安全删除达成 | `actions.rs:411` 直连 → `PluginAction(...)`；插件侧注册 `quick_switch.goto`；加"动作不可用"降级路径；补 P0 的失败复现测试转绿 | plan 产物变 | revert |
| **P5 切配置** | 配置迁到 `plugin-settings.json` | 加一次性迁移 + 旧字段读兼容；`options.quickSwitch` 标 deprecated | **GET /config 变 ⇒ api-parity 需同步** | revert（保留旧字段） |
| **P6 切 UI / 开删除** | 卡片统一 + 可删 | 删 `services/plugins.rs` 内置卡合成；`handlers_plugins.rs:528` 放开 + tombstone；`sync-plugins` 尊重墓碑；加"恢复随包插件" | 无 | revert |
| **P7 完全解耦（终态）** | 核心零知识 | manifest `provides.actions[]`；动作列表动态化；配置存字符串动作 ID；**删 `BUILTIN_PLUGIN_IDS`** | **协议变更 + 全量重录** | 单独分支，独立评审 |

**关键顺序理由**：P3 之前先有 P2 的双轨等价证明（门面 no-op 发布）；
P4（安全删除）必须先于 P6（开启删除按钮）—— **先让删除安全，再允许用户点删除**。

---

## 7. 影响面与闸门

| 面 | 影响 | 处置 |
|---|---|---|
| **parity（12 份基线）** | P3/P4 必改生成产物 | 按 `make parity` 重录；**`reference/` 是 Go 冻结产物**，需同步更新口径说明；`factory-plugins` 语料是本次的主回归门 |
| **api-parity（23 步）** | P5 改 `GET /config` | 同步重录；注意 `KEYFLUX_VERSION` / `KEYFLUX_API_PARITY` 夹具开关 |
| **i18n** | 动作标签 / 卡片文案 / 设置项 label 迁移 | `translation.ahk` 4 处 + 面板 `i18n.json`；保持键位不删只迁 |
| **CONTRACTS.md** | §0.3、§3.4、§3.7、§4、§5 需修订 | 本提案通过后先改契约再写实现（约束 #2） |
| **文档口径冲突** | `CONTRACTS.md §4` 的 manifest 示例（`runtime` / `entry:"main.ahk"` / `provides` / `settings` 为 map）**与已实现格式不符**（实为 `entry.{kind,file,func}` + `settings[]` 数组） | **本提案一并订正 §4**，避免后续照错文档实现 |
| **三端默认值守卫** | `default_quick_switch_option` / `is_quick_switch_zero` 的跨端一致性测试 | 迁移后改为"迁移正确性"测试 |
| **部署链** | `sync-plugins` 需读墓碑；`NeedsRegenerate` 靠 `data/plugins` mtime（`bin/Launcher.ahk:71-85`）天然感知增删 | P6 一并处理 |

---

## 8. 风险与对策

| 风险 | 等级 | 对策 |
|---|---|---|
| **初始化时序变化**：`InitQuickSwitch` 现在位于 `INITKEYMAP_HEAD` **之前**（L62），插件 bootstrap 在 L43 —— 迁到插件后调用时机会变 | 🔴 高 | P0 探针必须先画清 L44-L68 的初始化依赖图；必要时给插件引导加"时机钩子"（`afterEngineInit`） |
| **C4 的失败等级判定错误**（加载期 vs 运行期） | 🔴 高 | P0 实测确认；按更严重的等级设计 |
| **墓碑被绕过**：用户手工拷回 / robocopy 覆盖 | 中 | 墓碑在**部署脚本**而非运行时生效；UI 显式提示"该插件已被移除，可恢复" |
| **配置迁移丢数据** | 🔴 高 | 迁移前备份 `config.json`；旧字段读兼容保留 ≥1 版本；迁移幂等 |
| **parity 基线漂移被误读为回归** | 中 | 重录必须带夹具开关；diff 逐条人工确认后再 `UPDATE_GOLDEN` |
| **AHK 无运行时动态加载**（`CONTRACTS §3.7` 已记录） | 中 | 禁用 = 生成期不注入（现有机制）；**"删除"后必须重新生成**才生效 —— 这是设计内行为，需在 UI 文案说明 |
| **回滚后配置不认**（新→旧） | 中 | 迁移**只写不改**（新写 `plugin-settings.json`，保留旧段），保证旧版本可回退 |

---

## 9. 验收标准

**功能**
1. 插件页出现 QuickSwitch 卡片，**可禁用**：禁用后不注入、不注册、无定时器、无浮层、type9 动作不可用，重启引擎无报错。
2. 插件页 QuickSwitch 卡片**可删除**：删除后目录消失、键位降级为"该动作不可用（插件未安装）"提示，**引擎正常启动、其余键位与插件不受影响**。
3. 删除后**升级不复活**（`make deploy` 不拷回），且提供"恢复随包插件"。
4. 设置项（9 字段）在插件卡片中可编辑，与旧 `options.quickSwitch` 行为**逐项一致**。

**工程**
5. 连续 load/unload ×100：内存与句柄数持平（可逆性保证）。
6. `make check` / `cargo-gates` 全绿；新增契约测试覆盖：动作降级、墓碑、迁移幂等。
7. parity / api-parity 在 P3/P5 后重录并通过。
8. `grep -rn "quickswitch" bin/lib/` 在 P7 后**零命中**（核心零知识的机械证明）。

---

## 10. 决策（2026-10-01 23:35，用户授权"一切由你自行决策，依据代码的模块化和可移植性"）

> 决策原则：**模块化** = 每单位改动移除的耦合量 / 引入的回头路；**可移植性** = 换机、换版本、
> 回滚时是否仍然成立。二者与"改动量"冲突时，**优先前者**；与前向兼容冲突时，**优先可回滚**。

### D1 推进范围 = **P0–P4 本轮闭环；P5+P6 次轮；P7 另起提案**

- **模块化**：模块化收益的绝大部分集中在三件事 —— 注入通道解耦（P3）、动作解耦（P4）、
  可安全删除（P4+P6）。P5（配置搬家）**不增加任何模块化程度**，它只是把一个仍属"专用段"的
  数据结构换个存储位置。
- **可移植性**：P5 是全案**回滚代价最高**的一步（配置迁移一旦写盘，回退版本就读不懂新值），
  必须在结构稳定、回归充分之后再做。
- **纠正一处原计划的顺序错误**：P6（开启删除）**依赖 P5**，因为 QuickSwitch 现在的配置入口是
  一个**专用对话框**（4 开关 + 历史 + 排除表 + 清空历史）；卡片一旦按普通插件重建，
  专用对话框就没了，必须先用声明式设置（P5）把它接住。⇒ 原表"P6 不依赖 P5"是错的，已修正。
- P7 触及**配置文件格式**（动作 ID 字符串化），写入后旧版本读不懂 ⇒ 前向不兼容，
  必须独立评审、独立回滚窗口。

### D2 `excludedPrefixes` = **换行分隔的 `text` + 新增可选 `multiline` 标志**

- **不引入 `list` 类型**：那需要一整套"重复项编辑器"控件族（增行/删行/排序/去重），
  为一个字段引入整族控件是本末倒置。
- **也不做"暂留 config.json"**：那会让插件同时从两处读配置，是最坏的可移植性。
- 折中：给 `text` 加一个**布尔标志 `multiline`**（复用现成 TextBox，只多设一个属性），
  值以换行分隔、由插件自行切分。既不加控件族，又保持单一配置来源。

### D3 墓碑与禁用：**内部两字段，对外一个概念**

- 内部必须分开：`disabled` = 不注入但**文件还在**；`removed` = **文件已删**。
  换机/重装时二者含义不同 —— 合并存储就无法区分"用户不想要"与"文件缺失"，
  而"恢复随包插件"这个动作**只能对 removed 做**。
- 对外只暴露一个「移除 / 恢复」动作：两个概念的差别对用户不可见，摆两个只会让人困惑。

### D4 P7 **另起提案**，不与前 6 期混批（理由见 D1 末条）。

### D5 **先改 `CONTRACTS.md` 再动代码**（约束 #2 接口先行）——但 P1 属"协议词表外延"，
按下列口径处理：**提案（本文档）= 接口先行载体，`CONTRACTS.md` 的正式修订与 P1 同批提交**，
避免契约文档出现"写了但还没落地"的空头条款。

---

## 11. 本轮执行记录

### ✅ P0（探针）—— 已完成，结果见 **§0.5**

两项产出：① 用退出码实验**证明**未定义函数是 AHK v2 的**加载期致命错误**（C4 等级由"高"上调为"最高"）；
② 测绘出引擎初始化时序，发现插件引导点（L44）与 `InitQuickSwitch`（L60）**不同位**，
且 L60 在 `OnError(EngineOnError)`（L46）**之后** ⇒ 迁到插件会让异常逃出引擎兜底。

### ✅ P1（协议加 `bool`）—— 已完成

| 文件 | 改动 |
|---|---|
| `config-ui-reactor/src/generator/plugins.rs` | `SETTING_TYPES` 4 → 5 项加 `bool`；`value_limit` → 5；`validate_setting_value` 加 `bool` 分支（严格 `true`/`false`）；错误文案同步；**新增 4 个契约测试** |
| `config-server/internal/plugins/plugins.go` | `SettingTypeBool` 常量 + 词表同步 + `ValueLimit` → 5 + `ValidateSettingValue` 分支 + 错误文案（两端机械同步） |
| `config-ui-reactor/src/services/plugins.rs` | `SETTING_BOOL` / `is_bool` / `bool_value`；`max_length` 补 `bool → 5` |
| `config-ui-reactor/src/ui/plugins_view.rs` | `setting_row` 加泛型参数 `T: IntoPayloadCallback<bool>`；`bool` 渲染为 `ui::compact_switch`（**唯一 ToggleSwitch 构造点**） |
| `config-ui-reactor/src/app/dialogs.rs` | 开关复用**同一条** `PsValue` 字符串通道（`"true"`/`"false"`）⇒ 零新消息、零后端新分支 |
| `bin/lib/plugins/ConfigProvider.ahk` | **零改动** —— 其存储本就是扁平字符串，`bool` 天然承载 |

**零基线影响**：`bool` 尚未被任何 manifest 引用 ⇒ 生成产物、plan、skin、`GET /config` 全部逐字节不变。
**AHK 侧零改动**是这次协议设计的关键收益：值域选字符串 `"true"`/`"false"` 而非 JSON 布尔，
使新类型完全落在既有存储模型内。

### ⏭ 下一轮（P2+P3）

搬家 + 切注入通道（`bin/lib/quickswitch/**` → `plugins/bundled/quick_switch/` →
`data/plugins/quick_switch/`），并**重录 12 份 parity 基线**；同期定案 §0.5 提出的
「插件无关的晚初始化扩展点」。P4 紧随其后（C4 的通用门 + 优雅降级）。

### ⚠️ 本轮已知未完项

- `CONTRACTS.md` §4「插件清单格式(冻结)」的**示例与实现不符**（见 §7），本轮**未改**——
  已列入 P2 同批（届时 manifest 会真实改写，正好一并订正）。
- `services/plugins.rs` 的 `build_cards()` **无条件前置合成 QuickSwitch 卡**，
  插件化后会与扫描到的真卡**重复** ⇒ P2 必须加去重/停用合成的守卫（本轮已定位，未改）。

### ✅ P2 执行记录（2026-10-01 23:xx，本轮完成）

> 用户授权「一切由你自行决策，依据代码的模块化和可移植性决策」。P2 范围经现场再测绘后
> **收敛为「运行时解耦」**：AHK 代码搬入插件包 + 晚初始化扩展点 + 动作注册表门 +
> 目录加载放行。**配置段（C3）迁移到 plugin-settings.json 与 UI 卡切换（C6/C7）留给
> P4/P5** —— 配置段留在 config.json 是「无害孤段」（插件删掉后没人读它），不是
> 可删除性的必要条件；先拿可删除闭环，避免一次性爆炸半径过大（Strangler Fig）。

**落地清单**（14 处）：

| # | 文件 | 改动 |
|---|---|---|
| 1 | `bin/lib/quickswitch/*` → `plugins/examples/quick_switch/src/*` | `git mv` 搬迁 6 模块 + README（真源随包） |
| 2 | `plugins/examples/quick_switch/plugin.json` | 新建 manifest（无 settings —— 本期值仍由生成期渲染，避免 UI 出现改不生效的控件） |
| 3 | `plugins/examples/quick_switch/main.ahk` | 新入口 `QuickSwitchMain(api)`：`api.RegisterAction("goto", QuickSwitchRun)` |
| 4 | `bin/lib/plugins/PluginManager.ahk` | 动作注册表 `Actions` + `RegisterAction` + `InvokeAction`（缺席静默 false，错误隔离） |
| 5 | `bin/lib/plugins/APIBridge.ahk` | `APIView.RegisterAction`（不门控：动作自描述是基础能力） |
| 6 | `bin/lib/actions/builtins/type9_keyflux.ahk` | `QuickSwitchGoto()` 直调 → `PluginManager.InvokeAction("quick_switch","goto")`（**callMap[9] 文本不变**，golden 覆盖点零改动） |
| 7 | `config-server/templates/keyflux.tmpl` | 删 6 行硬编码 include；`InitQuickSwitch(...)` → `{{- PLUGIN_LATE_INIT }}`（`{{-` 吃前导换行 ⇒ 空块零字节） |
| 8 | `generators/plugins.go` | 三元组（inc/boot/**late**）+ `PluginLateInit` + `renderQuickSwitchLateInit`（字节形态与迁移前逐字节一致，单测钉死） |
| 9 | `generators/generators.go` | 函数表注册 `PLUGIN_LATE_INIT` |
| 10 | `internal/plugins/plugins.go` | `ValidateManifest` 拆分：**导入拒绝内置 ID**（防冒名）/ **目录加载放行**（`validateManifestBody`）；`InstallFromZip` 补严格校验 |
| 11 | `generator/plugins.rs`（Rust） | 同构：`validate_manifest`/`validate_manifest_body` 拆分 + `render_late_init` + `render_quick_switch_late_init` |
| 12 | `generator/template.rs`（Rust） | HEAD_INCLUDES 删 6 行；InitQuickSwitch 段 → `render_late_init` 条件拼接 |
| 13 | `services/plugins.rs` | `build_cards()` 去重守卫（跳过扫描到的 quick_switch，首卡合成保留） |
| 14 | 测试 | Go：`TestPluginLateInit_QuickSwitch`（字节形态）/`_Absent`（禁用+缺失不产出）/`TestBuiltinID_CatalogAllowsInstallRejects`；golden 管线挂 quick_switch 桩包（基线从此覆盖「插件注入+晚初始化」双路径） |

**关键设计落点**：

- **晚初始化扩展点**（§0.5 遗留问题的解）：`{{ PLUGIN_LATE_INIT }}` 位于 `InitKeymap()`
  与 `OnExit` 之间 = **原 InitQuickSwitch 硬编码位置** ⇒ 调用时机与参数来源（生成期渲染
  `options.quickSwitch`）与迁移前**逐字节一致**，P0 发现的「初始化时序错位 + OnError 兜底
  逃逸」风险整体消除（根本没挪位置）。存在性/禁用判定与 `render_plugin_blocks` 完全同构
  ⇒ 插件被删/停用时初始化行同步消失（可删除性闭环）。
- **动作注册表门**（C4 解）：核心薄壳不再直调 `QuickSwitchRun()`；生成端 `callMap[9]`
  文本不变 ⇒ 动作元数据 / golden 覆盖点 / UI 标签零改动。P7（manifest `provides.actions[]`）
  届时再把 `callMap[9]` 泛化成动作 ID 寻址。
- **BUILTIN_PLUGIN_IDS 语义细化**：从「目录加载也拒绝」改为「**导入冒名拒绝**、目录加载
  放行」—— 随包内置插件以标准插件形态分发的闸门。
- **位置裁决**：真源放 `plugins/examples/quick_switch/`（仓库自称该目录为「随软件分发的
  官方插件单一真源」，`sync-plugins` 现成通道零新增机制），而非新建 `plugins/bundled/`。

**验证状态**：Go 全仓测试全绿（含 golden 重录 + 新增 4 用例）；Rust 侧
`cargo-gates`（fmt/clippy/test）跑闸中；**parity 12 份基线与 `make check` 尚未跑**
——产物字节会有预期漂移（6 行 include → 1 行插件 include + Register/LoadEntry 引导行 +
晚初始化行），重录前先做新旧产物定向 diff 审计。

**P4/P5 待办**（依 §6 路线顺延）：UI 卡切目录驱动 + 删除放行 + 墓碑（P4）；
settings 声明 + plugin-settings.json 迁移 + `options.quickSwitch` 段退役（P5）；
`CONTRACTS §4` 订正随 P5 manifest 真实扩容一并做。

---

## 附: 一句话本质

> **KeyFlux 早就为 QuickSwitch 预留了插件位（`BUILTIN_PLUGIN_IDS`、`PluginManager`、
> `ActionRegistry`），只是 QuickSwitch 自己没走进去。**
> 本提案 = 让它走进去，并把"走进去之后还能安全地走出来"这件事补完。
