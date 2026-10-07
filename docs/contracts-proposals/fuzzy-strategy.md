# 提案: FuzzyStrategy —— 缩写命令编辑距离容错匹配 + 候选提示

> 状态: **已落地（`bin/lib/commands/FuzzyStrategy.ahk` + `CommandResolver.Strategy` 挂接, 2026-10-01; 本文件保留为决策记录）**
> 关联契约: `docs/CONTRACTS.md` §3.6 `CommandResolver.Strategy` 留桩、§0 快路径红线、约束 #4/#6/#7
> 落地文件: `bin/lib/commands/FuzzyStrategy.ahk` (新增) + `bin/lib/commands/CommandResolver.ahk` (2 处最小接入)
> 分支: `agent/fuzzy-strategy`

---

## 1. 背景与范围

`readme.md` 功能进展表: 逐字符实时后缀校验 (`FuzzySuffixFire`, 最长后缀优先) 已实现;
「编辑距离容错与候选提示仍在规划」。CONTRACTS §3.6 在 `CommandResolver` 上预留了可插拔
策略桩 (`static Strategy := ""`, 未命中时委托), 但 `FuzzyStrategy.ahk` 文件实际不存在。

本提案把该桩落地为一个**可整体旁路**的策略对象, 补齐两件事:

1. **容错命中**: 输入无任何精确命中、且已不可能长成任何精确命令时, 若恰有一个编辑距离
   ≤ 阈值的已注册命令, 静默执行它 (对齐冻结契约「唯一候选 → 静默执行」);
2. **候选提示**: 容错层有多个候选时**只提示不执行** (对齐「多候选 → 仅 Tip 列出, 不执行」)。

**明确不在范围内**:

- 分号域: `semiHook.OnChar` 直连 `semiHookAbbrWindow.Show` (keyflux.tmpl), 不经过
  `CommandInputOnChar` → `FuzzySuffixFire` 链路; 接线需改模板, 本批不做 (见 §8 边界)。
- 生成端 (`abbr_registry.go`) 与 config.json: 零改动 (约束 #7, 不向 config.json 写任何载荷)。

## 2. 公开接口 (bin/lib/commands/FuzzyStrategy.ahk)

```ahk
class FuzzyStrategy {                       ; 全 static (可作 CommandInputHooks provider 注册)
  ; ---- 可调参数 ----
  static Enabled := true        ; 总开关; false = 容错与提示全旁路 (等价纯精确匹配)
  static Threshold := 1         ; 编辑距离阈值 (Levenshtein; 冻结契约口径 "编辑距离≤1")
  static MaxCandidates := 5     ; 候选提示最多条数 (超出部分截断, 最近者保留)
  static MinInputLen := 2       ; 容错层生效的最短输入; 1 字符不做容错 (首键误触面太大)
  static HintShowMs := 2000     ; 提示窗无后续输入时的自动隐藏时长

  ; ---- 纯计算 (无副作用, 供测试与未来 UI 复用) ----
  static EditDistance(a, b)                       ; Levenshtein, 滚动单行 DP, O(la*lb); `=` 大小写不敏感
  static IsSubsequence(short, long)               ; 双指针子序列判定 (仅 Resolve 未命中路径用)
  static Candidates(scope, input, includeSubsequence := false)
                                                  ; 候选命令名数组, 按 (距离, 字典序) 升序, 截 MaxCandidates;
                                                  ; 排除与 input 精确等值的命令 (等值属于精确层, 不属容错)
  static HasPrefixCommand(scope, input)           ; 本 scope 是否存在以 input 为真前缀的已注册命令

  ; ---- 运行时入口 (CommandResolver 委托, 见 §3) ----
  static OnInputChanged(ih, scope, input, char)   ; 逐字符容错判定 + 候选提示 (每次输入变化调用)
  static OnExactHit()                             ; 精确命中瞬间调用: 收起提示 (命令即将执行)
  static Resolve(scope, command, hook := "")      ; CommandResolver.Resolve 未命中 (会话已结束) 时委托

  ; ---- CommandInputHooks provider 钩子 (全 static, 惰性注册, 永不消费按键) ----
  static OnKey(ih, vk, sc, scope) => false        ; 退格 (vk=0x08): 收起提示; 其余忽略
  static OnSessionBegin() / OnSessionEnd()        ; 收起提示 (跨会话不泄漏)

  ; ---- 提示载体 ----
  static ShowHints(cands)                         ; InputTipWindow 单例 (多行候选); 2s 自动隐藏定时
  static HideHints()                              ; 收起提示 + 撤销定时
  static TipWindow := ""                          ; 显示载体实例; 测试可注入桩, "" = 懒建真身
}
```

文件末尾一行**载入即挂接**: `CommandResolver.Strategy := FuzzyStrategy`
(类静态属性默认 `""` = 未设置; 该行运行的前提是 `CommandResolver.ahk` 先于本文件被
Include —— 模板现有顺序 `lib/commands/CommandResolver.ahk` 在前, 追加一行即可满足)。

## 3. 与 `CommandResolver.ahk` 的接入 (最小侵入, 共 2 处)

### 3.1 逐字符路径 — `FuzzySuffixFire` (CommandResolver.ahk:160)

```
原有: 最长后缀优先扫描, 首个精确命中 → 记 Pending + ih.Stop()   [一行不动]
新增① 精确命中分支内: Strategy != "" 时调用 OnExactHit() 收起提示
新增② 扫描全部未命中后: Strategy != "" 时调用 OnInputChanged(ih, scope, input, char)
```

全部引用经 `CommandResolver.Strategy` 转发并有 `!= ""` 守卫 —— **引擎未载入
FuzzyStrategy.ahk 时零符号引用、零行为变更** (不存在「未定义类」加载期 #Warn 风险)。
前缀守卫与容错命中在策略层内部判定, `FuzzySuffixFire` 不感知细节。

**衔接关系 (优先级冻结)**: 每次输入变化 = `精确后缀 (最长优先) → 容错层`。
精确机制永远先走且有最高优先级; 容错层只在「所有后缀都未精确命中」时被咨询,
因此 `dfc→fc`、全串 MatchList 命中两条既有通道**字节级不变**。

### 3.2 未命中路径 — `CommandResolver.Resolve` (CommandResolver.ahk:61)

```
原: this.Strategy.Resolve(command, hook)
改: this.Strategy.Resolve(scope, command, hook)     ; 补传 scope, 策略需分域查表
```

该路径的现实触发面 = 「MatchList 全串命中但注册表查无此命令」(生成端 `AbbrRegistryCode`
对整命令无可用 TypeID 时跳过注册, 而 `CapslockAbbrKeys` 仍含该键; 正常配置两者一致,
此路径是**兜底**而非主通道)。策略按冻结契约 §3.6 语义处理:

- 候选集 = {子序列匹配} ∪ {1 ≤ 编辑距离 ≤ Threshold}; 空 → 静默无操作 (与无策略时一致);
- **唯一候选** → `Tip` 提示实际命令 + 经 `CommandResolver.Resolve(scope, 候选)` 真身执行
  (命中分支: 步骤守卫语义、`abbr_submit` 事件 matched=true 全部继承; 事件流先 false 后
  true 两条, 属预期: 一次「尝试未中」+ 一次「容错执行」);
- **多候选** → 仅 `Tip` 列出 (≤MaxCandidates), 不执行。

## 4. 容错命中规则 (逐字符路径, `OnInputChanged`)

四条件**全部满足**才静默执行, 否则退化为提示或静默:

| # | 条件 | 为什么 |
|---|---|---|
| 1 | `Enabled` 且输入长度 ≥ `MinInputLen`(2) | 总开关; 1 字符容错会把大量「刚开始打字」误判成 typo |
| 2 | 精确层零命中 (由调用点保证) | 精确永远优先, 衔接关系见 §3.1 |
| 3 | **前缀守卫**: 本 scope 不存在以当前输入为真前缀的已注册命令 | 输入仍可能长成精确命令时容错层**完全静默** (不打断也不提示) —— 防止输入 `swap` 的过程在 `sw` 处被 `se`(距离 1) 抢先执行 |
| 4 | 候选集恰为 1 个 (距离 ∈ [1, Threshold]) | 冻结契约「唯一候选 → 静默执行」; 多候选歧义只提示 |

命中动作 = 复用后缀模糊命中的既有「待收尾」机制 (终止字符补投 `EchoTerminalChar` 仅
透传模式, 写 `CommandInputHooks.PendingScope/PendingAbbr`, `ih.Stop()`) → 由
`EnterCapslockAbbr` 的 `TakePending` 分支延后 `FinishDelayMs` 执行, 事件
`abbr_submit` 带 `fuzzy=true`。**不新建执行通道**, §3.12 硬约束 9 的绘制时序语义全部继承。

**前缀守卫同样约束提示**: 输入是某命令前缀时, 即使存在距离 1 的候选也不提示
(用户在打 `settings` 的过程中不该被 `se` 的提示打扰)。提示只在「已不可能长成精确命令
且候选 ≥ 2」时出现。

**阈值边界语义**: 距离 0 (输入与某命令等值) 不属于容错 —— 逐字符路径下等值输入必然已
被精确层处理; 候选过滤显式排除等值, 防止容错层重新引入精确语义。长度差 > Threshold 的
命令在进 DP 前被长度带预筛跳过 (结果与全量 DP 一致, 纯提速)。

## 5. 候选提示 (行为与显示载体)

- **载体**: `InputTipWindow` 单例 (`bin/lib/core/InputTipWindow.ahk` **零改动**) —— 与
  分号域 `semiHookAbbrWindow`、选中动作无焦点菜单同一既有机制; 无边框置顶工具窗、
  跟随鼠标、`NoActivate`, 不抢命令框焦点。内容 = 候选命令名一行一个, ≤ `MaxCandidates` 行。
- **显示时机**: 每次输入变化重算; ≥2 候选且通过前缀守卫 → Show (替换文本);
  0 候选 / 前缀守卫 / 唯一候选(执行) / 精确命中 / 退格 / 会话结束 → Hide。
- **自动隐藏**: 每次 Show 重臂 `HintShowMs`(2s) 单发定时器 (同一 BoundFunc 重臂, 不泄漏);
  Esc/EndKey 结束会话等无钩子路径由该兜底收敛, provider 的 `OnSessionEnd` 即时收敛。
- **provider 惰性注册**: 首次显示提示时才 `CommandInputHooks.Register(FuzzyStrategy)`
  (幂等, Register 自带去重); 注册前零钩子介入。provider 全 static (§3.12 硬约束 0 同款),
  所有 `On*` 恒返回 false / 无返回 —— **永不消费按键**, 不会改变 EchoChar /
  FuzzySuffixFire 的既有派发顺序。
- **不干扰插件搜索**: everything_search 类 provider 消费字符后 `DispatchChar` 提前
  return, `FuzzySuffixFire` 与容错层都不进入 (check-hooks 第 12 组既有双保险不变)。

## 6. 性能预算 (约束 #6, 按键钩子内轻量)

单次输入变化的最坏成本 (注册表 N 条命令, 输入长 L ≤ 数十):

| 步骤 | 成本 | 说明 |
|---|---|---|
| 精确后缀扫描 | 原有, 不变 | 最长 L 次 Map.Has |
| 注册表扫描 ×2 (候选 + 前缀守卫) | O(N) 次字符串前缀比较 | N ≈ 数十; 纯内存 Map 遍历 |
| 编辑距离 DP | 每命令 O(L²), 但长度带预筛后仅 |长度差| ≤ 1 的命令进 DP | 阈值 1 时实际进 DP 的通常 0~几条 |
| 提示窗 Show/Hide | 已建窗口后仅文本替换 + Show | 与 `semiHookAbbrWindow` 每字符 Show 同级 |

**不做的事** (红线自证): 无文件 IO (日志仅在异常分支)、无窗口枚举、无 DllCall、无
正则、无网络; 插件消费路径 (DispatchChar 提前 return) 完全零成本。快路径 (重映射/发键/
鼠标) 不经过本模块任何代码 (CONTRACTS §0 红线不适用性同 `CommandResolver` 既有说明)。

## 7. 测试与门禁

`tools/fuzzy_strategy_test.ahk` (风格对齐 `tools/command_input_hooks_test.ahk`:
AHK 运行时断言 + 0/1 退出码 + 工作目录隔离到 %TEMP% + 逐字 Include 真身 + 提示载体注入
FakeTip 避免 GUI 弹窗), 覆盖:

1. `EditDistance` 单元 (含阈值边界: 0/1/2、插入/删除/替换、空串);
2. `Candidates` 排序/截断/等值排除/长度带/分域隔离 (caps 查不到 semi 的命令);
3. 精确命中不受影响: `FuzzySuffixFire` 精确后缀/全串命中照旧, 且容错层零介入;
4. 容错命中: 唯一候选 → Pending + Stop; 前缀守卫 → 不执行不提示;
5. 无候选: 静默 + 提示收起; 多候选: 仅提示不执行 (FakeTip 记录);
6. 阈值边界: `Threshold := 0` 退化为纯精确; 调大后原先不命中的候选出现;
7. Resolve 未命中路径: 唯一 → 执行+Tip、多候选 → 仅 Tip、无候选 → 静默;
8. provider 语义: 退格/会话边界收起提示、恒不消费按键。

自检门禁 (提交前全绿): `lint_ident.py` 四文件 → 新测试 → `command_input_hooks_test.ahk`
既有回归 → `/Validate FuzzyStrategy.ahk`。

## 8. 已知边界与部署接线 (需维护者知悉)

1. **引擎接线**: 模板 `config-server/templates/keyflux.tmpl` 追加一行
   `#Include lib/commands/FuzzyStrategy.ahk` (置于 `lib/commands/CommandResolver.ahk`
   之后)。该文件不在本批文件所有权内, 故本批提交后引擎仍为纯精确行为 —— 接线前
   **零行为变更**是刻意设计; 接线后如需临时关闭, 置 `FuzzyStrategy.Enabled := false`
   即可 (无需摘 Include)。
2. **分号域未接**: 需改模板 semiHook 的 OnChar 接线 (模板不在所有权内), 后续单独处理。
3. **大小写**: 编辑距离与候选比较用 AHK `=` (大小写不敏感), 与 InputHook MatchList
   的不敏感语义一致; 但注册表 `Map.Has` 是大小写敏感的 (既有事实), 大写输入本就
   无法精确命中 —— 容错层经前缀守卫对「等值不同大小写」同样静默, 不放大该缺口。
4. **容错命中的提前执行面**: 与「打完即执行」的产品哲学一致 (后缀机制本就在输入中途
   命中即执行); 前缀守卫把「正在输入精确命令」的用户完全隔离在外, 剩余暴露面 =
   「输入已不可能成为任何精确命令」时的唯一近邻, 属特性语义而非缺陷。
5. **多候选提示窗的陈旧窗口期**: 退格不触发 OnChar, 提示可能陈旧至多 `HintShowMs`
   (2s); provider `OnKey`(退格) 与 2s 自动隐藏定时器双保险收敛。
