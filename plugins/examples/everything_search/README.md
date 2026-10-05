# everything_search —— KeyFlux 首个真实可用的第三方插件

在**命令框**里按下**前置触发键**（默认空格，可在设置面板点插件卡改），用本机 Everything
搜索**当前选中的文字**；结果由**命令框本体向下长高**展示 —— 同一个圆角白框里
[查询区 | 分隔线 | 结果行]，与 Flow Launcher / uTools 同形态（2026-10-04 重构，见 §1.2），
`↑` `↓` 选择、鼠标点选/悬停、`回车` 在资源管理器里打开/定位。
Everything 没在运行时按配置路径**静默拉起**（后台托盘，不弹主窗口、不抢焦点，见 §1.1）。

本文件是该插件唯一的开发文档（其余说明散落在各源文件头部注释里，按「谁负责什么」就近放置）。

---

## 1. 五个功能点的实现与衔接

| 功能点 | 实现落点 | 与其他点的衔接 |
|---|---|---|
| **触发键配置** | `plugin.json` 的 `settings[triggerKey]`（`char` 类型，默认 `" "`）→ 设置面板对话框渲染成单字符输入框；值存 `data/plugin-settings.json` | 值由 `EverythingSettings.NormKey` 归一（只接受可打印 ASCII 单字符，否则回落空格）；`EverythingSession.OnChar` 用它判定「本次会话的第一个字符是不是触发键」 |
| **选中文字获取** | `EverythingSession.SeedFromSelection` → `SelectionContext.Get(true)` | 触发键被消费**之后**才取词；取词要发 `Ctrl+C`，而活动 InputHook 看得见脚本自身 Send 的按键，故用 `capturing` 捕获锁把注入的字符全部吞掉（见 §4.1）。取到文件时只取首个文件的主名（资源管理器里选中文件时用户意图通常是「找同名/同类」） |
| **结果列表展示** | `EverythingResults`（视图端口）→ `EverythingHost.ShowResults`（0x406 载荷）→ **命令框自绘**（`command-input/src/results.rs` + `win/backend_gdi.rs`） | 列表不再是第二个窗口：命令框收到整表后**自身向下长高**并绘制，与查询区同框同轮廓（2026-10-04 重构，见 §1.2）。旧实现「自建 AHK Gui + ListView + `SetWindowRgn` 耳朵拼轮廓」已删除 —— 那正是用户报障「看起来仍是两个组件」的根源。`↑`/`↓`/`回车` 仍由命令框的 `capsHook` 以 `KeyOpt(...,"N")` 通知引擎（见 §3）；鼠标点选/悬停由命令框经 0x409 回推给会话层 |
| **未启动时静默拉起** | `EverythingSearch.EnsureRunning`（+ 兜底 `HideMainWindowIfAny`） | 先 `ProcessExist("Everything.exe")` 探活（不依赖 IPC 窗口，避开版本差异）；**未运行**时按设置里的 `everythingPath` 以 `-startup` 开关拉起并轮询等待（250ms × 最多 6s，冷启动要读 db）；**已运行**时不做任何额外处理（不重启、不动已有窗口）。`-startup` = 官方「后台运行、不显示任何搜索窗口」开关（见 §1.1）；拉不起来时区分「没配路径」与「配了但拉不起来」两种提示 |
| **路径配置** | `settings[everythingPath]` / `settings[esPath]`（`file` 类型 + `filter`）| 对话框里给「浏览」按钮（文件选择器）；`esPath` 留空时 `EverythingProviders.ResolveEs` 按优先级自动探测：显式 `esPath` → `everything.exe` 同目录 → 插件自带 `bin/es.exe` → 系统 `PATH`（用 `es.exe -version` 实测一次，避免把「不存在」拖到查询期）。全不可用时降级为 `everything.exe -search` 打开 Everything 界面，并明确告知结果不在本插件下拉里 |

**一次完整时序**：

```
引擎 EnterCapslockAbbr
  → CommandInputHooks.BeginSession()      记前台窗口 + 通知控制器开新会话
  → StartInputHook(Suspend true + InputHook.Wait)
       OnChar(' ')   → 是触发键? → 消费 → 取选中文字 → 0x404 激活命令框 → 查 Everything
                        → 0x406 把结果推给命令框 (后者向下长高并自绘列表)
       OnChar('x')   → 只消费 (文本由命令框原生持有, 经 WM_GETTEXT 轮询同步) → 重查 → 重推
       OnKey(↓/↑)    → 移动高亮 → 0x407 只推下标
       OnKey(回车)   → 打开当前项 → 0x408 收列表 → ih.Stop()（引擎走 HIDE 分支隐藏命令框）
       0x409 回推    → 命令框的鼠标点选/悬停 → OnBoxNotify(行号, 类型)
  → CommandInputHooks.EndSession()        收列表（Esc 等未走回车的退出路径）
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
已运行 → 不重启且 PID 不变）。（两个探针均为开发期临时脚本，**未入库** ——
（`open_guard_probe.ahk` 与 `max_results_probe.ahk` 为入库的回归探针；其余证据见 §4.3 的开发期约定。）

---

## 1.2 结果面板：命令框本体向下延伸（2026-10-04 重构）

### 问题

旧实现把结果画在一个**独立的** AHK `Gui + ListView` 上（`-Caption +ToolWindow`），只把命令框
当锚点贴在它下面，再用 `SetWindowRgn` 的「两只方角耳朵 + 中段豁口」去拼出一条看起来连贯的
轮廓。用户报障：**视觉上仍然分裂** —— 两个窗口各自的圆角、阴影、透明度、字体渲染不共享，
接缝永远存在（2026-10-03 的「连体重构」只是把接缝藏进弧口，没有消除它）。

### 修法：让命令框自己长高、自己画

命令框已从上游预编译二进制换成**本仓库的 Rust 实现**（`command-input/`），具备自绘能力 ⇒
列表交给它渲染，插件退化为纯数据源：

- 命令框在收到结果时 `SetWindowPos` 向下长高 `list_extra_px(行数)`，同一个圆角白框里
  `[ 查询区 (网格 + 大字) | 分隔线 | 结果行 × N | 底部留白 ]` 一体绘制；
- 收起时窗口回落基准几何（`200px @125%`，R11 定案值），**无列表时绘制输出与旧版逐像素一致**
  （查询区口径 `query_bottom = base_h − band`，网格范围随之收敛）；
- 折叠/滚轮/悬停的命中判定在命令框侧（只有它知道行几何），命中后经回推消息告诉会话层。

### 消息契约（与 0x401-0x405 同构：只加不改）

| 消息 | 方向 | 载荷 |
|---|---|---|
| `0x406` = `WM_COPYDATA` | 引擎 → 命令框 | `dwData` = 魔数 `'KFR2'`（`0x3252464B`，二版 **Flow Launcher 双行版式**，2026-10-04；一版 `'KFR1'` 单字符串/项已废弃 —— 魔数即版本号，旧版载荷被新版拒收）；`lParam` = `COPYDATASTRUCT`；`wParam` = 引擎脚本窗口（命令框记为回推目标）。数据 = `[魔数 u32][selected i32][count u32]` + 逐项 `[t_len u32][t UTF-8][s_len u32][s UTF-8]`，全小端；`t` = 标题（文件名含后缀），`s` = 副标题（完整路径，兼作命令框的**系统图标提取键** —— 命令框按路径经 `SHGetFileInfoW` 自提图标，插件零图标职责） |
| `0x407` | 引擎 → 命令框 | `wParam` = 0 基下标（`-1` = 无高亮）。单向，命令框**不回推**（防回声环） |
| `0x408` | 引擎 → 命令框 | 收起列表（窗口回落基准高） |
| `0x409` | 命令框 → 引擎 | `wParam` = 1 基行号；`lParam` = `1` 点选（打开） / `2` 高亮变化（悬停/滚轮） |
| `0x40A` | 引擎 → 命令框 | **显示搜索徽标**：`wParam` = 字形编号（`1` = 放大镜，与 `command-input/src/badge.rs` 注册表同值）。查询区右侧固定图标，位置锚定查询区（列表展开不影响）。搜索模式激活时发送 |
| `0x40B` | 引擎 → 命令框 | **隐藏搜索徽标**。会话收尾（`Close`）时发送；命令框侧对 0x401/0x402/0x403 另有「徽标活不过一次会话」兜底清除 |

- 载荷编解码的**单一真源** = `command-input/src/results.rs::encode_payload` / `decode_payload`
  （Rust 单测锁定格式）；AHK 侧镜像 = `EverythingHost.BuildResultsPayload`，探针第 15 组
  把两端**逐字节**对齐（魔数 / `selected` 的 1基→0基换算 / count / `[len][UTF-8]` / 中文 3 字节/字）。
- 防御口径：魔数不符、`cbData` 超上限（4 MiB）、结构截断一律**忽略该消息**（对端错误不得带崩命令框）；
  非法 UTF-8 用替换字符兜底（宁可视错，不可整表拒收）。
- 会话生命周期：列表**不跨会话存活** —— 0x401（显示）/0x402（淡出）/0x403（取消）都会清列表，
  插件不需要发收尾消息。

### 交互与显示口径

- 显示文本 = **完整路径**（`EverythingResults.Lines`，路径空时回落文件名）；命令框用
  `DT_PATH_ELLIPSIS` 保留首尾（比旧 ListView 「滚到文件名端」更可读），且**不做大写字形变换**
  （文件名大小写有语义；大写化只属于查询区）；
- 选中行 = 底色 + 左侧强调条；配色**由既有 18 键皮肤派生**（`skin::list_*`，不新增皮肤键 ⇒
  生成端/parity 零改动），派生色与背景对比不足时回落文字色中性灰；
- 行高 30 DIP / 字号 17 DIP / 分隔线 1 DIP / 底部留白 8 DIP（@125% ⇒ 38/21/1/10 px）；
  可见行数上限默认 12 行，且按屏幕高度收敛（展开后不越屏）；
- 提示行（路径失效等）走 `ShowHint` = 单行 + `selected = -1`：看得出、**选不中**（回车打不开它）。

---

## 2. 分层（每层只依赖下一层）

| 文件 | 职责 |
|---|---|
| `main.ahk` | 入口：读设置 → 建控制器 → 注册到命令框拦截点 |
| `src/EverythingMessages.ahk` | 中英文案（复用引擎 `SysLangIsChinese`） |
| `src/EverythingSettings.ahk` | 设置读取与归一（不做 IO） |
| `src/EverythingProviders.ahk` | 查询通道抽象：`es-cli`（首选）/ `gui-launch`（降级） |
| `src/EverythingSearch.ahk` | 编排：拉起 Everything + 选通道 + 失败重试 |
| `src/EverythingHost.ahk` | 引擎依赖的**唯一端口**（引擎 API 演进只改此文件；结果推送 0x406/0x407/0x408 与搜索徽标 0x40A/0x40B 也在这里） |
| `src/EverythingResults.ahk` | 结果列表**视图端口**：只把行文本推给命令框（渲染/几何在命令框内），并接收 0x409 回推 |
| `src/EverythingSession.ahk` | 命令框会话状态机 + 控制器（`CommandInputHooks` provider） |
| `tests/open_guard_probe.ahk` | 守卫链 + 结果推送契约回归探针（51 项；stub 引擎端口，不建窗口） |
| `tests/max_results_probe.ahk` | 结果条数上限回归探针（14 项；真集成：直连插件 es.exe 查真实 Everything，上限生效/保序/条目契约） |

依赖引擎侧接口：`CommandInputHooks`（`bin/lib/core/CommandInputHooks.ahk`）、
`CommandDisplay`（回显收口：`EchoChar` / `EchoBackspace` / `ActivateCommandWindow`，
契约见 `docs/CONTRACTS.md` §3.11）、`CommandImeGuard`（触发搜索时 `UnlockForSearch` 放开中文输入）、
`SelectionContext`；APIBridge 侧实际只消费 **`settings`** 一个命名空间（`api.GetSetting`）——
selection / run 两种能力分别走引擎全局 `SelectionContext` 与原生 `Run`/`RunWait`
（L1 插件与引擎同进程编译期包含，不需要绕 APIBridge；manifest 仍如实声明三项 permissions）。

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
（后端是唯一写入者，AHK 侧只读）。三个键：

| key | 类型 | 默认 | 说明 |
|---|---|---|---|
| `triggerKey` | char | `" "` | 前置触发键，单个可打印字符 |
| `everythingPath` | file | 空 | `everything.exe` 完整路径，未运行时用它**静默**拉起 |
| `esPath` | file | 空 | `es.exe` 路径（可选，留空走 §1 的探测链） |

结果条数上限 **1000**（`EverythingSearch.MAX_RESULTS`，通道层经 `es.exe -n` 落实；实测
`-n` 在**排序之后**截断，不破坏 GUI 同序）。

> 🔴 2026-10-05 修订 2026-10-04 的「不设上限」：英文短词会同时引爆三条管线约束 ——
> 实测本机查询 `ge` 返回 **72,870 条 / 导出 9.4MB**（中文如「是」仅 11 条，故当时未暴露）：
> ① 逐行 `FileExist` 复核 7 万+ 次 ⇒ 引擎线程阻塞数秒（用户视角即「命令框卡死」）；
> ② 0x406 载荷 9-11MB 超过两侧同值的 `MAX_PAYLOAD_BYTES`（4 MiB）⇒ 构造失败被
> `BuildResultsPayload` **静默拒绝** ⇒ 不出列表；③ 数万条目的跨进程编组本身也是无谓开销。
> 1000 条载荷 ≈ 150KB，通道导出与解析均在几十毫秒级；可视区不足仍由命令框侧滚动跟随。

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
- 列表不是独立窗口，而是命令框窗口**向下长高的同一块白框**（2026-10-04 起）：因此它天然跟随
  命令框的 DPI / 皮肤 / 圆角 / 阴影，折叠时回落基准几何（不残留、不「留下一个空盒子」）。
- 列表**可见行数上限 12 行**（`command-input/src/config.rs` 的 `LIST_MAX_ROWS`，并按屏幕高度收敛）；
  超出部分靠 `↑`/`↓`、鼠标悬停或滚轮移动高亮**自动滚动跟随**（滚轮步长 3 行），右侧画细滚动条。
  结果条数按 `EverythingSearch.MAX_RESULTS`（1000）封顶（2026-10-05 修订，理由见 §设置）
  ⇒ 命中很多时列表仍完整可滚，超出可见 12 行的部分靠滚动跟随 —— 高亮行始终保持在可视
  窗口内；可见行数只决定窗口高度，不改变结果条数。
- 鼠标交互（点选 / 悬停 / 滚轮）在命令框侧命中判定后经 0x409 回推；**点选**与回车走同一条守卫链
  （失败时列表不收、提示留在屏上，见 §1 与探针第 11 组）。


---

## 9. 状态自检（2026-10-01，示例 → 插件本体打磨）

审计方法：逐功能点对照本文 §1 的「文档声称」与源码现实（逐文件读码对账 + 引擎侧依赖只读核对），
动态验证以三道门禁为准（全部在 worktree 根目录执行，命令与结果如下）：

- `python tools/lint_ident.py <main + src 七文件>` → `TOTAL_FINDINGS=0`（exit 0）
- `MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut /Validate plugins/examples/everything_search/main.ahk` → exit 0
- `MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut plugins/examples/everything_search/tests/open_guard_probe.ahk` → **31/31 PASS**（exit 0；
  本次扩第 11 组后由 27 项增至 31 项，反向敏感度已实测：把 OnPick 退回旧的无条件 Close
  时**恰好 11c 一项变红**（Hide=1 / closed=1），其余 30 项不受影响 ⇒ 新断言精确锁住「点选失败提示可见」这一条）

| 功能点 | 落实情况 |
|---|---|
| **触发键配置** | ✅ 代码与文档一致：`plugin.json` 声明与 `EverythingSettings.NormKey`（可打印 ASCII 单字符，否则回落空格）与 `EverythingSession.OnChar` 的前置键判定逐条对账通过 |
| **选中文字获取** | ✅ 一致：`SeedFromSelection` → `SelectionContext.Get(true)`（引擎真身返回 `{type, content}`，已只读核对 `bin/lib/context/SelectionContext.ahk`）、`capturing` 捕获锁、文件取首文件主名（去扩展名）均与 §1/§4.1 描述相符 |
| **结果下拉展示** | ✅ 一致（本次修一处缺口）：`-Caption +ToolWindow +E0x08000000` + `Show("NA")`、命令框锚定（可见白框对齐 + 隐藏窗口二次探测）与文档相符；**发现并修复**：鼠标点选路径 `OnPick` 在 `OpenSelected` 失败时仍无条件 `Close()`，失效提示被立刻收起 —— 与 2026-09-30 Enter 分支修的是同一症状，已对齐为「仅成功才收浮层」并补探针第 11 组；`ED_ROWS` 文档漂移（10 → 30）已订正 |
| **未启动时静默拉起** | ✅ 代码与文档一致：`ProcessExist` 探活 → `-startup` 拉起 → 250ms × 6s 轮询 → `HideMainWindowIfAny` 兜底（类名黑名单）；「没配路径 vs 配了拉不起来」双提示在 `EverythingSearch.Run` + `_ErrorKey` 落实 |
| **路径配置** | ✅ 一致：三键声明（`Settings []Setting` 数组形，已对 `internal/plugins/plugins.go:126` 核对）、`ResolveEs` 四级探测链与 `es.exe -version` 实测、GUI 降级通道及 `err_launched_gui` 提示均在；§2 依赖清单已补 `CommandDisplay` / `CommandImeGuard`（引擎侧文件均已只读核对存在）并订正 APIBridge 实际消费面（仅 `settings`） |

### 遗留项

1. **端到端真机验证未跑**：触发键 → 取词 → 浮层 → ↑↓/回车全链路需要运行中的 KeyFlux 引擎 +
   真实 Everything 实例 + 按键注入，本工作区不满足条件（部署目录禁触、`make check-ime` 在禁令清单）。
   门禁覆盖的是静态检查 + 会话状态机/守卫链回归。
2. `_SortArgs` 读 `Everything.ini` 的排序跟随（§5）依赖真实 ini 与 GUI 对照，本次未实测（解析整体
   try/catch 兜底，异常退回 es 默认排序，不影响查询）。
3. ListView 鼠标滚轮滚动行为仍未专门验证（§8 如实标注）；↑↓ 高亮的自动滚入可视区有
   `LVM_ENSUREVISIBLE` 代码落实但无自动化断言（GUI 控件行为，探针桩无法覆盖）。
4. 设置热重载的**写入端**（设置面板 → `PUT /api/plugins/:id/settings` → 后端原子落盘）属跨端链路，
   本次只验证了 AHK 侧读端（`EverythingSettings.Load` 每会话重读 + 值变更才失效探测缓存）。
5. `plugins/marketplace.json` **有意未**加入 everything_search 条目：插件随软件分发
   （`make sync-plugins`），市场目录是发布侧下载清单（条目需真实 zip 地址），且内置插件集合
   `BUILTIN_PLUGIN_IDS` 在 `config-ui-reactor/`（本次禁改范围）——是否上架属产品决策，非缺口。

---

## 10. 状态自检（2026-10-04，结果面板并入命令框）

本次改动 = §1.2 的重构：**删除** `src/EverythingDropdown.ahk`（自建浮层）与
`EverythingHost` 的命令框锚点段（`CommandBoxAnchor` / `_AnchorCache` / `ResetAnchorCache`，
只有浮层用得到），**新增** `src/EverythingResults.ahk` 与四组消息（0x406/0x407/0x408/0x409）。

### 门禁（全部本地实跑，命令与结果如下）

| 门禁 | 命令 | 结果 |
|---|---|---|
| Rust 单测（含 `results.rs` 12 项） | `cargo test --manifest-path command-input/Cargo.toml` | **63 passed / 0 failed** |
| 插件回归探针（守卫链 + 结果推送契约，15 组） | `MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut data/plugins/everything_search/tests/open_guard_probe.ahk` | **51/51 PASS**（exit 0） |
| 插件语法 | `… /Validate data/plugins/everything_search/main.ahk` | exit 0 |
| 活体几何 + 像素（私有非提权实例，不碰生产实例） | `%TEMP%\kf_list_smoke\probe_results.py` | **9/9 PASS**：基准高 200 → 推 5 行 → **401**（顶边不动）→ 0x408 回落到 200 → 推 1 行 → 249；截图 `results_expanded.png` |

活体冒烟的做法（与 `cmdinput-re/smoke.ps1` 同一安全契约）：把候选 exe 连同 skin/font/sound
复制到 `%TEMP%` 下的 staging 目录另起一个**非提权私有实例**，只对 `PID == 我方进程` 的窗口
发消息（按 pid 过滤 `EnumWindows`），因此永远不会误伤用户正在使用的实例 ——
`0x406` 的跨进程载荷由系统编组（`SendMessageW` + `COPYDATASTRUCT`），与生产路径同一条。

### 反向敏感度（探针不是恒绿）

- 第 9 组（失败不静默）在工作区历史上有实测：把 Enter 分支退回「无条件 `Close` + `ih.Stop`」
  ⇒ 恰好 9c/9d 两项变红；
- 第 11 组本次改写为 0x409 回推：把 `OnBoxNotify` 的 `kind = 1` 分支退回「无条件 `OpenSelected` + `Close`」
  ⇒ 11b/11c 变红（提示被立刻收起）；
- 第 15 组是**跨语言字节锁**：改 `EverythingHost.BuildResultsPayload` 的字段序/宽度/单位
  （或改 Rust `encode_payload`）⇒ 15g/15h 必红。

### 遗留项

1. **端到端真机全链路仍靠人工**：触发键 → 取词 → 命令框内输入 → 实时重推 → 鼠标点选，
   需要真实 Everything 实例 + 按键/IME 注入；本次覆盖到「载荷 → 长高 → 自绘 → 收起」这一段
   （即本次改动的全部新增路径）。
2. 鼠标滚轮步长（3 行）与悬停高亮的观感未做像素级断言（行为断言在 Rust 单测与探针里）。
3. `plugins/examples/everything_search/` 是 parity 语料（`tools/parity/manifest.json` 的
   `factory-plugins`）的取源，本次**有意未同步** —— 同步会改变生成物字节、需要重录 parity 基线；
   `data/plugins/everything_search/`（`.gitignore` 特例放行的 bundled 插件真源）才是本次改动落点。
