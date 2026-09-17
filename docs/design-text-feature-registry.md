# 设计：内置文本特征注册表（plain 排除集的派生化）

日期：2026-09-17　|　关联提交：`2f7c4e3`（B 站特征）之后的一次重构

## 1. 问题

`plain（纯文本）`的语义是「其余文本特征都不命中」。在引入第 5 个特征（bilibili）之前，
它的排除集是**硬编码布尔式**：

```
Go : !reURL && !rePath && !reMagnet && !reBilibili
AHK: not (isURL or isPath or isMagnet or isBilibili)
```

失败模式：每新增一个具名特征，都要**人肉**在两端补上 `!新特征`，漏一处就是
"配了却不生效"（映射按数组行序取首个命中，先建的 plain 映射会恒遮蔽后建的具名映射）。
且两端 + 三处词表（Go / AHK / C#）之间只有注释约定，没有强制力——
`SelectedAction.ahk` 曾声称由 `testdata/match_ops.json` 守护，但该向量**只有 Go 侧消费**。

## 2. 方案：声明式注册表 + 派生兜底

### 2.1 组织方式（三端同构）

| 端 | 位置 | 角色 |
|---|---|---|
| Go（真源） | `config-server/internal/behaviors/textfeatures.go` | 值 / 中文名 / 正则 / 大小写开关 / 具名与兜底；`init()` 自检 |
| AHK（运行时） | `bin/lib/rules/SelectedAction.ahk` 的 `TextFeatureSpecs()` | 运行时命中判定（`MatchTextType` + `TextFeatureHit`） |
| C#（界面镜像） | `config-ui-avalonia/Services/ActionSchemeCatalog.cs` 的 `TextTypes` | 顺序 + i18n 标签；无匹配逻辑 |
| 共享契约 | `config-server/internal/script/testdata/text_types.json` | `types`（顺序）+ 63 条用例的 `expectTypes` **全集** |

每条具名特征四个字段：

```
Value（配置值，小写稳定 id）  Label（中文名，错误文案用）
IgnoreCase（大小写开关）      Pattern（正则源串，两端逐字相同）
```

设计要点：

- **IgnoreCase 与 Pattern 分离**：Go 编译期加 `(?i)`、AHK 运行期加 `"i)"` 前缀，
  正则源串因此可以在两端**逐字比对**（工具化校验的前提）。新增特征别把 `(?i)` 写进 Pattern。
- **兜底特征（plain）不持正则**：`Fallback=true`，命中条件由具名集**派生**——
  遍历全部 `Named=true` 项，任一命中即 `false`。新增特征 ⇒ 排除集自动扩大。
- **顺序即界面顺序**：添加映射下拉、映射行 Toggle 均按注册表顺序渲染，兜底恒居末位。
  该不变量由注册表 `init()`（Go）+ 一致性测试双向钉死。
- **`\z` 而非 `$` 收尾**：Go 的 `$` 只认文本末尾，PCRE2 的 `$` 还认末尾换行前的位置，
  多行选中时两端会分歧；`\z` 是方言交集。

### 2.2 新增一个内置文本特征的接入点（4 处）

| # | 位置 | 内容 | 漏改的后果 | 谁会拦住 |
|---|---|---|---|---|
| ① | `behaviors/textfeatures.go` 注册表加一行 | 值/名/正则/大小写 | — | — |
| ② | AHK `TextFeatureSpecs()` 加同一行 | 同上（无 Label） | 运行时不命中该特征 | `make check-texttypes`（静态对账 + 运行时对账） |
| ③ | `bin/behaviors/<id>/behavior.json` | 该前提下的可用行为 | 「添加映射」弹窗空列表死胡同 | `TestBuiltinCatalog_CoversEveryTextFeature`（注册表驱动，自动覆盖新特征） |
| ④ | i18n 新键 + `TextTypes` 加一行 + `MappingRowVm` bool + XAML Toggle | 界面文案与互斥 Toggle | Toggle 数量不齐 / 文案缺失 | `TextTypeToggleExclusiveTests`（数量由 `TextTypes.Length` 派生）+ i18n 键数守卫 |
| ⑤ | `testdata/text_types.json` 追加用例 | 该特征的边界样例 | 契约覆盖不足 | 用例数下限断言（≥40） |

Go 侧的错误文案（`可选：链接 / 路径 / … / 纯文本`）、`reservedTextTypeNames`、
`KnownTextTypes` 均已改为注册表**派生**，不再属于接入点。

### 2.3 自定义类型（`type:<id>`）不在注册表里

方案 C7 的自定义文本特征（equals/prefix/suffix/contains 四算子 OR）走
`Config.MatchTypes` + `CustomMatchTypes` 全局表，与内置注册表是**两个命名空间**
（`type:` 前缀是 Windows 文件名非法字符，天然不冲突）。关键边界：

- **自定义类型不参与 plain 的排除集**（只排除内置具名特征）——保持 2026-09-17 之前的口径：
  plain 的语义是"不属于任何**内置**具名特征"，自定义类型由用户显式建映射，不抢 plain 的判定。
- 自定义类型 id 不得与内置值重名：`reservedTextTypeNames`（派生自注册表）在保存期拒绝。

## 3. 边界情况（向量已覆盖，共 63 条）

| 边界 | 期望 | 为什么 |
|---|---|---|
| 内容首尾空白（`" av170001"`、`"av170001 "`） | 不命中具名，归 plain | 锚定口径；该值会被原样拼进 URL，容忍空白生成非法链接 |
| 尾换行 `"av170001\n"` | plain | `\z` 钉死：若改用 `$`，PCRE2 会命中而 RE2 不会 ⇒ 两端分歧 |
| 前导换行 `"\nav170001"` | plain | `^` 只认文本起始 |
| 多行选中 `"https://a\nBV1xx411c7mD"` | 仅 url | 首行前缀命中即归该特征 |
| 交叉污染 `"C:\av170001"` / `"magnet:?…av170001"` | 仅 path / magnet | 具名特征两两互斥，先命中者独占 |
| `"av170001\nBV1xx411c7mE"` | plain | 整串锚定，不做逐行扫描 |
| 空串 / 纯空白 | plain | 具名全不命中 ⇒ 兜底 |
| 全角数字 `"av17000１"` | plain | `[0-9]` 不含全角，不放宽 |
| `"https://"`（仅协议头） | url | 前缀锚定，不要求后续内容 |
| `"C:"`（盘符无反斜杠）/ `"\\server"`（UNC 单段） | plain | 正则要求两段结构 |

## 4. 验证要点（可执行判据）

1. **共享向量 = 双端契约**：`text_types.json` 的 `expectTypes` 记录"全部命中特征"（非单点），
   天然同时断言具名互斥性与 plain 排除集。
2. **Go**：`texttype_vector_test.go` —— 注册表顺序 == `types`、兜底唯一居末、逐用例比对。
3. **AHK 运行时对账**：`tools/texttype_conformance.py` 从源文件**逐字提取**函数体生成探针
   （不手抄，杜绝探针与产品代码漂移），跑 63 用例 × 5 特征 = 315 次求值比对。
4. **静态对账**：同一工具解析两侧注册表源码，逐项比对 value / named / ignoreCase / pattern 与顺序。
5. **重构等价性**：重构前后各跑一次探针，`diff` 逐位一致（本次 63×5 全同）。
6. **回归**：`go test ./...`、`dotnet test`（264/264）、analyzers、`lint_ident`、
   GenerateAHK + `/Validate`。
7. **一键**：`make check-texttypes` 已挂进 `make check`。

## 5. 已知既有差异（保持不动，非本次引入）

- 特征**名**的大小写口径：Go `strings.ToLower(TrimSpace(t))`（宽松，`URL` 也认）；
  AHK `Trim(t)` + `==`（严格，只认小写）。实际取值经保存期白名单校验恒为小写，
  故不可达；重构刻意不"顺手"改齐——行为等价性优先，且 `==`（大小写敏感）与旧 `switch`
  实测一致（AHK v2 的 `switch` 对字符串是大小写不敏感的，`==` 才敏感，已用探针实测确认）。
- `path` 的 `strings.EqualFold`（Go）vs `StrLower`（AHK）：两者对非 ASCII 的折叠口径
  本就有差异，属历史遗留（见 `doc/与原版的差异.md`），与文本特征注册表无关。
