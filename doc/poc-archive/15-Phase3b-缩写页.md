# Phase 3b：缩写页（Command / Abbreviation）（✅ 已完成）

> 日期：2026-09-28 ｜ 证据：`evidence-phase3b-abbr.png`、`evidence-phase3b-abbr-selected.png`

---

## 1. 交付

| 项 | 内容 | 旧实现（权威来源）|
|---|---|---|
| **chips 网格** | `keymap.hotkeys` 每键一格；**自动换行**；点击选中；高度 44 | `AbbrPageView.axaml` `WrapPanel` + `KeyCellVm` |
| **chip 三态** | 选中=Sand 底深字 · 禁用=奶油底不可点 · 其余=象牙底；**无绑定态** | `AbbrPageViewModel.RefreshChips` |
| **尾空格可见** | 尾部空格逐个显示为 `□`（U+25A1，**非**旧版 `◻️`，理由见 §4）| `FormatSpace` |
| **命令框** | placeholder = i18n `406`；**回车执行**；`del <缩写>` 删除 / `rn <新名>` 重命名当前选中 / 其余视为选中 | `RunCmd` + `OnCmdKeyDown` |
| **换键语义** | 目标已存在时：当前键未配置（首动作 `typeId=0`）则丢弃当前键；否则覆盖目标键 | `KeymapEditorCore.ChangeHotkey` |
| **备注汇总** | 键显示用 `format_space` 口径（**原样** + 尾空格可见），不走 `key_text` 的大写化 | `RefreshComments` |
| **动作编辑面板** | 与模式页完全同源（`action_editor_panel`，缩写语境隐藏类型 4/5） | `ActionEditorPanel` |

**闸门**：`cargo clippy --all-targets` **0 告警**；`cargo test` **108/108**（新增 8 项）；`cargo fmt --check` **exit 0**。

## 2. 真机验证（UIA + 截图，指向部署树真实后端）

启动配方（同 Phase 3a）：`keyflux-settings.exe --settings-exe <部署树>\bin\settings.exe --backend-dir <部署树>\bin`。

| 判据 | 结果 |
|---|---|
| 导航选中 | UIA `SelectionItemPattern.Select('Command')` 成功 |
| chips 数量 | **45 条**（真实 config 的 keymap id=2 全部条目），8 列自动换行 ✅ |
| chip 点击 | 「请先在左侧键盘网格中选中一个键」消失，编辑面板出现 `mm` 的类型 1 字段（`重启 KeyFlux` 等）✅ |
| **`del se` + Enter** | 条目数 50→**49**，`se` 消失 ✅ |
| **选中后 `rn abc` + Enter** | `abc` 出现、`mm` 消失 ✅（含冲突分支语义）|
| **`se` + Enter**（选中已存在键） | 面板切换到该键编辑器 ✅ |
| Job Object | 强杀面板后 `settings = 0` ✅ |

## 3. 🔴 本轮 API 结论（续 §14 的 #16）

| # | 事实 | 影响 |
|---|---|---|
| 17 | **`VariableSizedWrapGrid` 可用**：`item_width/item_height/orientation` + `ChildrenControl`（`children`/`keyed_children`）；但本库**未暴露** `ColumnSpan`/`RowSpan` 附加属性 ⇒ **统一格宽**布局 | chips 换行用它可以，代价是所有 chip 等宽（旧版是自然宽）；格宽按最长标签估算（`chip_item_width`，夹 53..160）|
| 18 | **`ItemsRepeater` 在 0.100.0 不可用**：native 层直接 `UnsupportedKind` | 不要尝试用 `ItemsRepeater` + 布局做换行 |
| 19 | **`TextBox` 无键盘事件回调**（只有 `on_text_changed`），且 0.100.0 **没有** `routed_callback`（那是 master 文档的 API）| 回车执行走 `KeyAccelerator`：`Grid::key_accelerators(KeyAccelerators::new([KeyAccelerator::new(AcceleratorKey::Enter, AcceleratorModifiers::None, cb)]))`——**真机已验证生效** |
| 20 | `key_accelerators` 接受 `impl Into<Option<KeyAccelerators>>`，**数组不满足** `From` ⇒ 必须显式 `KeyAccelerators::new([...])` | 编译期已踩 |
| 21 | `IntoUnitCallback` 只为 `Callback<()>` 实现 ⇒ 闭包 `|_: String| ()` 不满足；且**单测环境无法构造 `Callback`**（需 `ComponentContext`）| UI 构建器无法单测（本模块注释里已固化这条边界）|

## 4. 与旧设计的有意差异（用户已授权 Fluent 化）

| 项 | 旧 | 新 |
|---|---|---|
| 排序 | C# `Dictionary` 插入序 | `BTreeMap` 键序（Phase 2 既有取舍，逐字节可复现）|
| chip 宽度 | 自然宽（MinWidth 53）| **统一格宽**（VariableSizedWrapGrid 限制；估算函数夹取）|
| 尾空格符 | `◻️`（U+25FD+VS16，会走 emoji 回退渲染成彩色方块）| `□`（U+25A1 单色）；⚠️ 真实配置无尾空格条目，**字形未真机取样** |
| 左列缩放 | `Viewbox Stretch=Uniform` | 自然尺寸 + 编辑面板滚动（延续模式页口径）|
| 空态提示 | 无 | 无条目时显示 i18n `406` 说明文字 |

**未真机取样的分支**（低风险，如实登记）：chip 禁用态（该 keymap 无与触发键同名的条目）、`□` 字形、超长缩写触发 160px 上限。

## 5. 验证工具层的坑（与产品代码无关，但会坑下一次验证）

1. **UIA `ValuePattern.SetValue` 不改变焦点** ⇒ 随后 `SendKeys` 的 Enter 发给旧焦点元素（如刚点击的 chip 按钮），命令框收不到。**必须先 `$edit.SetFocus()`**（第一轮误判「Enter 无效」即此因）。
2. **`SendKeys.SendWait('zzqq')` 实测键入 `z'z'q'q`**（字母间被插入 `'`）⇒ 键入式验证不可靠，判据改用 `SetValue`（焦点已就位时）+ 读 `ValuePattern.Current.Value` 回显。
3. **「添加缩写」没有即时条目**：`Select` 分支只设选中，条目在编辑动作时才惰性创建（旧版同语义）⇒ 自动化判据必须用 `del`/`rn` 的**条目增删**，不能数「新增条目」。
4. PowerShell 5.1 按 ANSI 解码无 BOM 的 UTF-8 脚本 ⇒ 验证脚本一律**纯 ASCII**。

## 6. 下一步（Phase 3b 续）

1. 选中动作页（文本/文件双聚合卡 + 匹配类型选择 + 行为库）
2. 插件页 + 匹配类型弹窗 + 其余弹窗（`ContentDialog` 会崩 ⇒ 一律 `open_window`）
3. 设置页（快捷键方案 / 外观材质 / 语言 / 路径变量 / 其他设置）
4. 移植 `NavBadge.EffectiveHotkey`（name 为空时的标签推导，代码留 TODO）
