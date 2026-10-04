# Phase 3b：动作编辑面板（✅ 已完成）

> 日期：2026-09-28 ｜ 证据：`evidence-phase3b-action-panel.png`

---

## 1. 交付

| 项 | 内容 | 旧实现（权威来源）|
|---|---|---|
| **两级选择** | 窗口分组（`id >= 0`）+ 动作类型（10 项，缩写语境隐藏 4/5）；**无标签**，忠实复刻 | `BuildGroupOptions` / `BuildTypeOptions` + `ActionEditorPanel.axaml:94-104` |
| **切类型语义** | 清空除 `windowGroupId`/`typeId` 外**全部字段**；类型 0 置 `isEmpty`；同类型为无副作用 no-op | `OnSelectedTypeChanged` |
| **类型 1** 启动程序/激活窗口 | 窗口标题（含 `ahk_exe` 校验与红字提示）、目标（文本框 + 快捷方式下拉）、参数、工作目录、备注、三开关、窗口侦探 | `ActivateOrRunEditorVm` |
| **类型 2/3/4/7/9** 枚举单选 | 目录逐项复刻（System/Window/Mouse/Text/KeyFlux），**两两一行**，缩写过滤，选中即写备注为文案键 | `RadioCatalog` + `RadioGroupEditorVm` |
| **类型 5** 重映射 | 目标文本框 + 55 项候选下拉；`singlePress` 禁用并提示改用「输入文本或按键」（`954`）| `RemapEditorVm` |
| **类型 6** 输入按键或文本 | 多行文本框 + 备注；`isEmpty = !keysToSend` | `SendKeysEditorVm` |
| **类型 8** 自定义函数 | 多行代码框 + 2 条示例下拉（选中即填入）+ 两条 Tips（`955/956`）| `AhkCodeEditorVm` |
| **动作惰性初始化** | `ensure_action`：键/分组不存在时按旧规则补建（新建 keymap 的 `singlePress` 填入 `{blind}{触发键}`）| `_getAction` |
| **缩写联动** | 类型 9 与取值 5/6 交叉变化时重算 Command/Abbreviation 启用态并**重建导航且保持选中** | `MaybeRefreshAbbrEnable` + `MainViewModel.OnNavInvalidated` |

**闸门**：`clippy --all-targets` **0 告警**；`cargo test` **100/100**（新增 17 项）；`fmt --check` **exit 0**。

## 2. 真机验证（UIA + 截图）

选中「F 模式」→ 点键 `Q` 后的实测：

| 判据 | 结果 |
|---|---|
| 下拉数量 | **2**（窗口分组 / 动作类型）|
| 分组下拉文案 | `🌎 Global` = 真实 config 的 `windowGroups[id=0]` ✅ |
| 类型下拉文案 | `📚 文字编辑相关` = i18n `207`（类型 7 Text）✅ |
| 单选按钮数 | **33** —— 与 Text 目录（8+8+9+9）经缩写过滤后的数量一致 ✅ |
| 当前值勾选 | `右键菜单` 已选中 ✅（该键在 config 里的 `valueID`）|
| 文本编辑框 | 0 个 —— 该动作是枚举类，**符合预期**（类型 1/6/8 才出现文本框）|
| 两两一行 | 截图可见每行 2 组、组间等距 ✅ |
| Job Object | 强杀面板后 `settings = 0` ✅ |

## 3. 新掌握的 0.100.0 事实（续 §13 的 11 条）

| # | 事实 | 影响 |
|---|---|---|
| 12 | 🔴 **keyed diff 的 key 只接受 `usize`/`String`**（`i32` 不满足 `Key: From<i32>`）| `keyed_children` 的 key 必须显式用 `usize` 或 `String` |
| 13 | `TextBox::on_text_changed(IntoPayloadCallback<String>)`、`ComboBox::on_selection_changed(IntoPayloadCallback<Option<usize>>)`、`ToggleSwitch::on_toggled(IntoPayloadCallback<bool>)`、`RadioButton::on_checked(IntoPayloadCallback<bool>)` | 双向编辑链路齐备 |
| 14 | `ComboBox`：`items_source<I,S>`（S: Into\<String\>）+ `selected_index(Option<usize>)` + `is_editable` | 下拉可用；`selected_index` 是普通属性（可 diff）|
| 15 | `Button::content`/`RadioButton::content`/`Border::content`/`StackPanel::children`/`keyed_children` **均返回 `View`** | 这些位置再加 `.into()` 会被 clippy 判为无用转换 |
| 16 | `View` **不**实现 `LayoutControl`（再次确认）| 已构建的 `View` 要放 Grid 定位，必须先用 `Border` 包裹再 `grid_row/grid_column` |

## 4. 与旧设计的有意差异（用户已授权 Fluent 化）

| 项 | 旧 | 新 |
|---|---|---|
| 面板高度 | 固定高 + 内部滚动 + Viewbox 缩放 | **自然高度 + 卡片内边距**，由左列 `Grid` 的 `*` 行承担滚动 |
| 顶部下拉 | 无标签（5:7 分栏） | **无标签**（保持）|
| 左列结构 | `Viewbox` 整体缩放（文字一起缩）| 三段式 Grid：页头 / 键盘网格（有界可滚）/ 动作面板 |

## 5. 下一步

1. 缩写页（id 2/3）chips 视图（`WrapPanel` 缺失 ⇒ 需先裁定换行方案，候选：`VariableSizedWrapGrid`）
2. 选中动作页（文本/文件双聚合卡 + 匹配类型选择 + 行为库）
3. 插件页 + 匹配类型弹窗 + 其余弹窗
4. 移植 `NavBadge.EffectiveHotkey`（当前 name 为空时退化用 `hotkey`，已在代码留 TODO）
