# everything_search —— KeyFlux 首个真实可用的第三方插件

在**命令框**里按下**前置触发键**（默认空格，可在设置面板点插件卡改），用本机 Everything
搜索**当前选中的文字**；结果以**不激活的下拉浮层**列在命令框正下方，`↑` `↓` 选择、`回车`
在资源管理器里打开/定位。Everything 没在运行时按配置路径**静默拉起**（后台托盘，不弹主窗口、不抢焦点，见 §1.1）。

本文件是该插件唯一的开发文档（其余说明散落在各源文件头部注释里，按「谁负责什么」就近放置）。

---

## 1. 五个功能点的实现与衔接

| 功能点 | 实现落点 | 与其他点的衔接 |
|---|---|---|
| **触发键配置** | `plugin.json` 的 `settings[triggerKey]`（`char` 类型，默认 `" "`）→ 设置面板对话框渲染成单字符输入框；值存 `data/plugin-settings.json` | 值由 `EverythingSettings.NormKey` 归一（只接受可打印 ASCII 单字符，否则回落空格）；`EverythingSession.OnChar` 用它判定「本次会话的第一个字符是不是触发键」 |
| **选中文字获取** | `EverythingSession.SeedFromSelection` → `SelectionContext.Get(true)` | 触发键被消费**之后**才取词；取词要发 `Ctrl+C`，而活动 InputHook 看得见脚本自身 Send 的按键，故用 `capturing` 捕获锁把注入的字符全部吞掉（见 §4.1）。取到文件时只取首个文件的主名（资源管理器里选中文件时用户意图通常是「找同名/同类」） |
| **结果下拉展示** | `EverythingDropdown`（自建 AHK Gui + ListView，`-Caption +ToolWindow +E0x08000000` 且 `Show("NA")`） | 锚点取命令框窗口（`ahk_class MyKeymap_Command_Input`）的实时几何，贴合其正下方；**为什么不能直接塞进命令框**：命令框本体是上游预编译二进制 `bin/KeyFlux-CommandInput.exe`（无源码、无 ListView 资源、与引擎只有单向 `PostMessage WM_CHAR` 通道），没有任何「投递候选列表」接口 —— 视觉上仍是「command 下方的下拉列表」。`↑`/`↓`/`回车` 由命令框的 `capsHook` 以 `KeyOpt(...,"N")` 通知给引擎侧（见 §3） |
| **未启动时静默拉起** | `EverythingSearch.EnsureRunning`（+ 兜底 `HideMainWindowIfAny`） | 先 `ProcessExist("Everything.exe")` 探活（不依赖 IPC 窗口，避开版本差异）；**未运行**时按设置里的 `everythingPath` 以 `-startup` 开关拉起并轮询等待（250ms × 最多 6s，冷启动要读 db）；**已运行**时不做任何额外处理（不重启、不动已有窗口）。`-startup` = 官方「后台运行、不显示任何搜索窗口」开关（见 §1.1）；拉不起来时区分「没配路径」与「配了但拉不起来」两种提示 |
| **路径配置** | `settings[everythingPath]` / `settings[esPath]`（`file` 类型 + `filter`）| 对话框里给「浏览」按钮（文件选择器）；`esPath` 留空时 `EverythingProviders.ResolveEs` 按优先级自动探测：显式 `esPath` → `everything.exe` 同目录 → 插件自带 `bin/es.exe` → 系统 `PATH`（用 `es.exe -version` 实测一次，避免把「不存在」拖到查询期）。全不可用时降级为 `everything.exe -search` 打开 Everything 界面，并明确告知结果不在本插件下拉里 |

**一次完整时序**：

```
引擎 EnterCapslockAbbr
  → CommandInputHooks.BeginSession()      记前台窗口 + 通知控制器开新会话
  → StartInputHook(Suspend true + InputHook.Wait)
       OnChar(' ')   → 是触发键? → 消费 → 取选中文字 → 查 Everything → 显示浮层
       OnChar('x')   → 追加检索词 → 重查 → 同步投递字符给命令框做视觉回显
       OnKey(↓/↑)    → 移动高亮
       OnKey(回车)   → 打开当前项 → 收浮层 → ih.Stop()（引擎走 HIDE 分支隐藏命令框）
  → CommandInputHooks.EndSession()        收浮层（Esc 等未走回车的退出路径）
```

---

## 1.1 未启动时的静默拉起（2026-09-20）

### 问题

原实现是 `Run('"' exe '"')` —— 裸跑 `everything.exe`。Everything 默认启动即**弹出主窗口**，
并由 `bring_into_view` / `maximized` 等配置决定是否抢前台焦点。后果：用户在命令框里
按触发键去搜文件，Everything 主窗突然盖上来并夺走键盘焦点，**当前输入被直接打断**。

### 修法

启动命令加官方开关 **`-startup`**（`EverythingSearch.StartupSwitch`）：

```
everything.exe -startup
```

> 官方文档（Everything.exe 命令行选项 · General）：
> `-startup` — *"Run Everything in the background without showing any search windows."*

即：进程照常启动并加载索引、常驻托盘，但**不创建/不显示任何搜索窗口**。

**为什么不用 `-minimize`**：`-minimize` 只是把窗口最小化，窗口仍然存在且仍是前台候选，
任务栏/Alt+Tab 仍会出现，焦点抢夺问题没有真正解决；`-startup` 是从根本上不建窗口。

### 兜底（降级方案）

`-startup` 作用于**启动瞬间的窗口策略**，若 `Everything.ini` 里有强制显示类配置
（如 `maximized=1` + `bring_into_view=1`）或用户装的是会弹窗的魔改版，主窗口仍可能出现。
故 `EnsureRunning` 在探活成功后调一次 `HideMainWindowIfAny()`：在 1.2s 窗口期内
（250ms 步长）枚举 Everything 进程的**可见**顶层窗口，主动 `WinHide`。

- 筛选规则：以**类名黑名单**排除 IME 辅助窗（`MSCTFIME UI` / `IME` / `Default IME`），
  其余可见顶层窗一律视为主窗口。**不以「标题非空」为主筛** —— 实测 `-startup` 下
  Everything 创建的窗口标题可能为空，用标题筛选会漏掉真正的主窗口。
- **不影响用户手动开窗**：本方法只在「刚拉起后的 1.2s 窗口期」调用一次；
  此后用户自己点托盘图标开主窗时，`EnsureRunning` 早已因「已在运行」直接返回，
  不会再走到隐藏逻辑。

### 实测证据（2026-09-20，本机 Everything 1.5.0.1418）

| 观测项 | 裸启动 | `-startup` |
|---|---|---|
| 可见顶层窗口 | **主窗口弹出 + 抢焦点** | **0 个**（仅 2 个 `visible=0` 的 IME 辅助窗） |
| 进程存活（40s 采样） | 稳定 2 进程 | 稳定 2 进程 |
| `es.exe -get-result-count` 退出码 | 0 | **0（全程）** |
| 前台焦点变化 | 被抢 | **不变**（`PiliPlus` → `PiliPlus`） |

对照探针 `es_compare_test.ahk` 跑 `bare` / `startup` 两模式各 40 秒，逐秒采样
进程数与 IPC 退出码 —— 两者**完全等价**，唯一差异就是窗口是否出现。
另有 `es_silent_launch_test.ahk` 双场景断言 9/9 通过（未运行 → 静默拉起且不抢焦点；
已运行 → 不重启且 PID 不变）。

---

## 2. 分层（每层只依赖下一层）

| 文件 | 职责 |
|---|---|
| `main.ahk` | 入口：读设置 → 建控制器 → 注册到命令框拦截点 |
| `src/EverythingMessages.ahk` | 中英文案（复用引擎 `SysLangIsChinese`） |
| `src/EverythingSettings.ahk` | 设置读取与归一（不做 IO） |
| `src/EverythingProviders.ahk` | 查询通道抽象：`es-cli`（首选）/ `gui-launch`（降级） |
| `src/EverythingSearch.ahk` | 编排：拉起 Everything + 选通道 + 失败重试 |
| `src/EverythingDropdown.ahk` | 浮层渲染（不自查、不读配置、不发键） |
| `src/EverythingSession.ahk` | 命令框会话状态机 + 控制器（`CommandInputHooks` provider） |

依赖引擎侧接口：`CommandInputHooks`（`bin/lib/core/CommandInputHooks.ahk`）、
`SelectionContext`、`APIBridge` 的 `selection` / `run` / `settings` 三个命名空间。

---

## 3. 引擎侧的接入点（跨仓改动清单）

插件不是自包含的 —— 命令框的输入钩子在引擎里，插件只能通过新开的一个拦截点接进去：

1. **`bin/lib/core/CommandInputHooks.ahk`（新增）**：provider 列表 + 分发入口
   `CommandInputOnChar(ih, char, scope)` / `CommandInputOnKeyDown(ih, vk, sc, scope)`。
   顺序调用 provider，任一返回 `true` 即消费该次按键（不再走引擎原有语义）。
2. **`config-server/templates/keyflux.tmpl`**：`capsHook.OnChar/OnKeyDown` 改绑上述分发入口；
   并为 `{Up}` `{Down}` `{Enter}` 加 `KeyOpt(..., "N")`。

   为什么必须加 `KeyOpt(...,"N")`：`N` 只**通知**不投递，不消费时行为与历史完全一致
   （方向键/回车本就不进命令框的字符缓冲）。实测 `KeyOpt("{Up}","N")` 对**非 EndKey** 也
   确实产生 `OnKeyDown` 通知，故导航键不必塞进 `EndKeys`。
   半角分号钩子 `semiHook` **不受影响** ——它是缩写提示窗（`InputTipWindow`），与命令框无关。

---

## 4. 四条实测得出的硬约束（踩过的坑）

### 4.1 活动 InputHook 看得见脚本自身 Send 的按键

取选中文字要发 `Ctrl+C`（`SelectionContext.Get`）。而 `InputHook` 处于活动状态时，
**脚本自己 Send 出去的 `c` 也会进 `OnChar`**，不处理就会污染检索词、还会把它投递到命令框。
→ 加 `capturing` 捕获锁：取词期间 `OnChar`/`OnKey` 一律返回 `true`（吞掉），取完即释放。

### 4.2 回车以换行字符（`Ord = 10`）进入 OnChar

命令框里的回车不是「回车键事件」而是字符 `\n`。不把它当控制字符过滤掉，检索词末尾会被塞进换行。
→ `OnChar` 里 `Ord(c) < 32 && c != " "` 一律不消费。真正的回车由 `OnKey`（VK 0x0D）处理。

### 4.3 `$TEMP` 在 Git Bash 下的字面量陷阱（仅影响开发期探针）

探针脚本里若用 `$TEMP` 拼路径再传给原生 exe，Bash 只在内建解析时把它当 `C:` 临时目录，
传出去会变成字面量 `/tmp` → 被解释成 `D:\tmp`，于是「Script file not found」。
→ 开发期一律写显式 `C:\Users\<user>\AppData\Local\Temp\...`。

### 4.4 🔴 AHK v2 的 `obj.Method` **不绑定 `this`**（引擎侧踩的；症状最像「插件根本没挂上」）

`CommandInputHooks` 分发 provider 回调时若写成：

```ahk
fn := p.%name%
return fn.Call(args*)          ; ❌ 每一次都抛 Missing a required parameter.
```

就会**全盘静默失效**。AHK v2 里 `this` 只是函数的普通首参（与 Python/JS 的 bound method 语义**相反**）
—— 官方作者 lexikos 原话：*"`this` is just a parameter of the function, and doesn't have a value
until you provide one when you call or bind the function"*。故 `obj.Method` 取到的是**未绑定**的
函数对象，`fn.Call(ih, char, scope)` 会把 `ih` 顶替成 `this`、末位实参缺失 → 每次回调在**调用边界**
就抛 `Missing a required parameter.`；异常随即被分发层的 try/catch 吞掉并「视为未消费」
⇒ **provider 一次都没执行**，日志里只留一行被吞掉的噪声。

→ 动态派发必须显式给出接收者：`p.%name%(args*)`（本仓库采用，先例 `bin/lib/Monitor.ahk:363`）
或 `ObjBindMethod(p, name).Call(args*)`。

**为什么这条很难自己发现**：`/Validate`（语法）与 `lint`（标识符遮蔽）都是**静态**检查，查不出纯运行时
语义；而症状「按触发键毫无反应」与「插件压根没注册」在外部表现上完全一致 —— 第一反应必然是去查插件。

回归守门人 = `tools/command_input_hooks_test.ahk`（`make check-hooks`，已挂入 `make check`）：
旧写法下 **13 项红 / 10 项绿**，新写法下 **23 项全绿**。探针**逐字 `#Include` 引擎真身**而非另写桩
实现（否则只会验证自己的桩，回归价值归零），并把工作目录隔离到 `%TEMP%` 以免污染部署日志。

---

## 5. 通道选型（为什么不直接用 IPC / SDK DLL）

Everything 有四条可编程通道，可用性与可移植性差异很大：

| 通道 | 结论 | 依据 |
|---|---|---|
| **`es.exe` 官方 CLI** | **首选** | 官方命令行工具，自带 `-ipc1/-ipc2/-ipc3` 自适应 Everything 1.4/1.5；结果可 `-export-txt` 落文件读取（AHK 读不了子进程 stdout，这是最稳的一条）；退出码有语义（`8` = 找不到 IPC 窗口 ⇒ Everything 未运行；`5` = 无法创建导出文件） |
| WM_COPYDATA IPC | **弃用** | 实测 Everything **1.5.0.1418** 上回复不稳定（同一个查询一次有回复、一次空回复）；回复数据在对端进程地址空间（要 `ReadProcessMemory`）；发送线程阻塞时无法处理回复而 AHK 是单线程模型 |
| `Everything64.dll`（SDK2） | **弃用** | 需随插件分发 dll；且官方文档明确 1.5 alpha 分支不支持 —— Flow Launcher 走的就是这条路线，其文档标注「仅支持 1.4.x」，与本机实测相互印证 |
| `everything.exe -search` | **降级** | 只能把 Everything 界面打开到搜索结果，结果**不在**本插件下拉列表里 —— 仅在 es 缺失且前两条不可用时使用，并明确告知用户 |

本机 Everything 版本：**1.5.0.1418**。

### 结果排序与 Everything 主界面一致（v1.0.1）

es.exe 查询会读取 Everything.ini 的 `sort=` / `sort_ascending=`（便携版在 everything.exe 旁，安装版在 `%APPDATA%` + `\Everything`），映射为 `-sort` 参数传给查询 —— 命令框下拉列表与 Everything 主界面同序（在 GUI 点列头改排序后，下一次查询即跟随，无需重启）。排序名按白名单映射（`Date Modified` → `date-modified` 等 13 种），未知排序名不传 `-sort`，走 es 默认（名称升序）。解析整体 try/catch，任何意外退回默认，不影响查询本身。

2026-09-25 用户报障背景：GUI 排序为 `Date Modified` 降序时，同一查询命令框展示的文件与 GUI 完全不同（es 默认名称升序 + `-n` 截断放大差异）。

---

---

## 6. 设置与热重载

设置存 `data/plugin-settings.json`（契约 `docs/CONTRACTS.md` §3.8），键空间按
`"<pluginId>:<key>"` 隔离，由**设置界面经后端 `PUT /api/plugins/:id/settings` 写入**
（后端是唯一写入者，AHK 侧只读）。四个键：

| key | 类型 | 默认 | 说明 |
|---|---|---|---|
| `triggerKey` | char | `" "` | 前置触发键，单个可打印字符 |
| `everythingPath` | file | 空 | `everything.exe` 完整路径，未运行时用它**静默**拉起 |
| `esPath` | file | 空 | `es.exe` 路径（可选，留空走 §1 的探测链） |
| `limit` | number | `20` | 下拉条数上限（1–100） |

**热重载**：`EverythingSettings.Load()` 在**每次命令框会话开始时**重读该文件，故设置面板保存后
**无需重启引擎**即刻生效。`Load` 只在值真的变了才返回 `true`，调用方据此决定是否让
`EverythingProviders` 的通道探测缓存失效 —— 无条件失效会让每次会话都白起一次
`es.exe -version` 探测子进程。

---

## 7. 部署与依赖

- 插件随软件分发：`make sync-plugins` 把 `plugins/examples/` 同步到
  `$(OUT_DIR)/data/plugins/`（robocopy **不带 `/MIR`** ⇒ 用户自装插件不被清掉），
  并挂为 `make check` 与 `make sync-out` 的前置 —— 生成端只扫 `<config.json 同级>/plugins`，
  插件不到位 `check` 会在空目录上**假绿**。
- **`es.exe` 不入库**（第三方二进制，不放进仓库）。期望落点是
  `data/plugins/everything_search/bin/es.exe` —— 正是 `ResolveEs` 的第 3 优先级
  `SelfEsPath`（`A_ScriptDir\..\data\plugins\everything_search\bin\es.exe`），
  放这里即可**零配置**用上 ES 通道。官方下载：<https://www.voidtools.com/downloads/>
  （"Download Everything Command-line Interface"）。放好后可用
  `es.exe -version` 自检（当前部署用的版本：`1.1.0.37`，签名主体 voidtools PTY LTD）。
- 关掉插件：设置面板插件卡开关（写 `config.options.plugins.disabled`，经 `PUT /config`
  保存并重启引擎），或删掉 `data/plugins/everything_search/` 目录后重启引擎。
  卸载**不会**清 `plugin-settings.json`（重装后设置还在）。

---

## 8. 已知边界

- 触发键只在**本次命令框会话的第一个字符**位置生效（前置键语义）；一旦输入过别的字符，
  本会话就不再触发 —— 避免与命令框自身的缩写模糊匹配抢键。
- 检索词全部走 `es.exe` 的一次性调用，**不**做增量/防抖；单次含子进程启动与全库查询，
  本机实测约 **141ms**（`config-server` 一词，含 `-timeout 4000` 与导出落盘），在命令框
  输入期**同步**执行 —— 每次追加/退格字符都会重查一次。
- 检索词里的双引号会被替换为空格、连续空白折叠为一个空格（AHK 的 `Run` 无法表达嵌套引号，
  而引号在 Everything 语法里只是短语包裹）；以 `-` 开头的词会前置 `--` 关闭开关解析。
- 下拉浮层是独立窗口，**不**跟随命令框在会话中移动（只在显示时锚定一次）。
- 浮层**可见高度上限 10 行**（`ED_ROWS`），而 `limit` 默认 20 ⇒ 默认场景下就会有结果落在
  可视区之外，需要靠 ListView 自身的滚动条访问（该滚动行为未做专门验证）。若希望
  「看到多少就配多少」，把 `limit` 设为 10。

