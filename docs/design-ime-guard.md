# 命令框 IME 强制英文锁 (CommandImeGuard) 设计说明

- 状态: 已定案 (v2 布局切换, 2026-09-19 实证后修订)
- 日期: 2026-09-19
- 关联 CHANGELOG / CONTRACTS: §3.10 (CommandInputHooks), §3.11 (CommandDisplay),
  §3.12 (ImeInputHost 透传)

## 0. 定案修订 (2026-09-19 实证后)

原 §5-A「跨进程 IMM 设英文」的核心前提**经实测证伪**:

- 探针 (PowerShell P/Invoke + AHK 双通道): `ImmGetContext` 对前台窗口, **无论是否
  先 `AttachThreadInput`, 均返回 0** (lastErr=203, 前台 hIMC=0)。`ImmGetContext`
  只能取「本线程自有窗口」的 IMC, 跨进程/跨线程一律 0。
- 命令框是独立进程 (`KeyFlux-CommandInput.exe`), 引擎主进程**无法直接读写其 HIMC**。

**✅ 且经实证可行的路径 (v2, 已实现) —— 切换键盘布局而非 INTERCEPT Shift**:

- `LoadKeyboardLayout("00000409")` 可动态加载英文(美式)布局, 返回非零 HKL (探针实测
  0x4090409);
- `PostMessage(WIN_INPUTLANGCHANGEREQUEST, lParam=HKL)` 到命令框窗口, **目标进程自己
  处理, 无需 HIMC** (探针实测前台窗口线程布局成功切到英文)。这是我们已有的 PostMessage
  WM_CHAR 同通道。

**v2 行为**: 弹框会话开始 → 命令框线程布局切英文 (真「锁英文」, Shift 无中文可切);
进搜索模式 → 切回中文布局 (解锁中文)。相比早期「拦 Shift」(仅防切换、不解决聚焦变中文),
这是本质修复。

---

## 1. 目标

修改命令框 (CapsLock) 会话期间的输入法行为:

1. **无论当前输入法是中文还是英文**, 按下 CapsLock 弹出命令框后, 输入法被强制切成
   **英文 (字母数字) 模式并锁定** —— 按 Shift 无法切换回中文, 除非退出命令框。
2. 用户按下前置触发键 (如空格) 进入搜索模式后, **解除输入法锁定**, 允许自由切换中英文,
   以便在搜索结果/检索词中输入中文。
3. 遵循工程的模块化 / 可维护性 / 可移植性原则。

## 2. 约束与既有事实 (必须对齐, 不可违背)

引用自 `docs/CONTRACTS.md §3.10/3.11/3.12`:

- 命令框是**上游预编译二进制** `bin/KeyFlux-CommandInput.exe` (无源码), 只接受
  `WM_CHAR` 单向通道, 无数据 IPC。命令框窗口带 `WS_EX_NOACTIVATE`。
- 命令框键盘捕获在主进程 `InputHook`; 透传模式 (v4, ImeInputHost 启用时) hook 恒
  `V`, 物理键透传给命令框窗口 → 英文原生显示, 中文由 **IME 在前台线程 (命令框线程)
  原生组合/上屏**。
- 🔴 **跨进程 `ImmGetContext` 曾证伪为恒返回 0** (v3 死因, kf_p1 探针, `notepad` 亦 0)。
  但文档未记载是否在调用前做过 `AttachThreadInput`。**本设计的可行性正系于此假设**,
  见 §5 风险 A。
- 🔴 TSF 在 AHK 主线程 DllCall 调用挂死/抛异常, **不可用** (kf_p2b/c/d 探针)。
- 一切 provider `On*` 回调必须 `static`, 类引用用全名 (`ImeInputHost.` 式)。
- provider 抛异常只记日志并视为未消费, 不拖垮命令框。
- 透传会话必须显式激活命令框窗口 (`CommandDisplay.ActivateCommandWindow`), 失败降级。
- 整个命令框输入发生在 `EnterCapslockAbbr()` (bin/lib/actions/builtins/type9_keyflux.ahk),
  由 `CommandInputHooks.BeginSession()` / `EndSession()` 界定生命周期。

## 3. 架构定位 (模块化 / 插件无关)

**输入法锁必须且只能由引擎核心模块承担, 不得依赖任何插件。**

```
┌──────────────────────────── 引擎 (主进程) ────────────────────────────┐
│  CommandInputHooks (provider 分发, bin/lib/core/CommandInputHooks.ahk) │
│     ├── ImeInputHost      : 透传开关 (SuppressKeycap, v4 既有)          │
│     └── CommandImeGuard   : 【新增】IME 强制英文锁 (本设计)              │
│                                 · OnSessionBegin -> 锁英文 + 起定时器    │
│                                 · OnSessionEnd   -> 解锁 + 恢复原状态    │
│                                 · UnlockForSearch() -> 搜模解锁 (静态)   │
└──────────────────────────────────────────────────────────────────────┘
                     │ 只暴露一个极薄静态 API
                     ▼
┌────────────────────────────────┐
│ 插件 (everything_search, 可增删) │
│   src/EverythingSession.ahk      │
│     触发键命中 -> CommandImeGuard.UnlockForSearch()   │
└────────────────────────────────┘
```

- **引擎侧零插件依赖**: 移除/停用 everything_search, `CommandImeGuard` 照常工作
  (命令框锁英文、退出恢复)。
- **插件侧零引擎改动**: 插件只需在某一行调用 `UnlockForSearch()`, 插件缺失时该调用
  不存在, 对 `CommandImeGuard` 本体零影响。
- **新文件**: `bin/lib/core/CommandImeGuard.ahk` (全 static, 自包含, 不新增全局碰撞)。
- **挂载点**: `config-server/templates/keyflux.tmpl` 在 `CommandInputHooks.Register(ImeInputHost)`
  同区新增一行 `CommandInputHooks.Register(CommandImeGuard)`。

## 4. 具体行为时序 (v2 布局切换)

| 阶段 | 动作 | 落点 |
|---|---|---|
| 会话开始 (CapsLock 按下) | `OnSessionBegin`: 加载中/英布局 HKL → 向命令框 PostMessage INPUTLANGCHANGEREQUEST 切英文 | `CommandImeGuard.OnSessionBegin` |
| 会话中 (未触发搜索) | 命令框线程布局为英文 → IME 无中文 → Shift 无论如何切不出中文 | 布局驱动, 无需拦键 |
| 触发前置键 (空格) | 插件调 `CommandImeGuard.UnlockForSearch()` → 切回中文布局 | `EverythingSession.ahk` |
| 搜索模式 | 命令框线程布局中文 → 可拼音输入中文检索词 | `CommandImeGuard` |
| 会话结束 (Esc/回车/匹配) | `OnSessionEnd`: 复位状态 (不做恢复——布局随命令框窗口线程, 隐藏即失效, 无泄漏) | `EndSession` 触发 |

## 5. 技术要点 (v2)

- 布局来源: `user32\LoadKeyboardLayout("00000409")` 动态加载英文布局 (无需语言预装), 返回
  稳定 HKL 句柄, 一次性持有; 中文同理 `"00000804"`。
- 切换: `user32\PostMessage(命令框hwnd, 0x0050, 0, HKL)` —— 消息投递, 目标进程自己处理,
  无需 HIMC (实证可行)。
- 时序: `OnSessionBegin` 必须先于 command 框获得焦点时切换? 否 —— provider 在
  `BeginSession()` (SHOW 之前) 触发, 此刻 PostMessage 到已 show 的命令框即可。若命令框
  尚未出现, `_Switch` 找不到窗口则静默跳过 (锁定降级, 不报错)。
- 状态机: `Locked` (会话期 true) / `SearchMode` (前置键后 true 并切中文)。`OnSessionEnd`
  复位; 插件缺失时 `UnlockForSearch` 无人调, 但 `Locked` 仍复位, 无泄漏。

## 6. 明确不可行 / 不再考虑的路径 (备案, 勿重蹈)

均经实测证伪或已废弃, 不作实现:
- 跨进程 `ImmGetContext` / `ImmSetConversionStatus` (HIMC 跨线程恒 0)。
- TSF 主线程 DllCall (挂死/抛异常, 见 CONTRACTS §3.12)。
- ~~拦截 Shift (v1)~~: 只防会话中途切换, 不解决「聚焦命令框即变中文」的根因, 已废弃为 v2 替换。

## 7. 实现清单

1. `bin/lib/core/CommandImeGuard.ahk` (全 static provider, 布局切换状态机)。
2. `config-server/templates/keyflux.tmpl` 注册 `CommandImeGuard` (与 ImeInputHost 并排)。
3. `plugins/examples/everything_search/src/EverythingSession.ahk` 触发键命中处调
   `CommandImeGuard.UnlockForSearch()` (极薄, 插件可增删)。
4. 同步 CHANGELOG / CONTRACTS 补一条新节。

## 8. 验收标准

- [ ] 英文态按 CapsLock: 命令框内输入法为英文, Shift 无法切中文 (真「锁英文」)。
- [ ] 中文态按 CapsLock: 命令框线程布局被切英文, 同样锁英文 (修复聚集即变中文的根因)。
- [ ] 按空格进入搜索模式后: 可切中文 (如打中文检索词)。
- [ ] 退出命令框 (Esc/回车): 不影响后续窗口输入 (布局随命令框窗口线程)。
- [ ] 停用/移除 everything_search 插件: `CommandImeGuard` 仍工作, 无报错。