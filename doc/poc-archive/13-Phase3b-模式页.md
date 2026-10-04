# Phase 3b：模式页（按键矩阵）（✅ 已完成）

> 日期：2026-09-28 ｜ 证据：`evidence-phase3b-keymap.png`、`evidence-phase3b-keymap-selected.png`

---

## 1. 交付

| 项 | 内容 | 旧实现 |
|---|---|---|
| **键盘网格** | 布局解析 → 行/键格；单字符 43×43 正方、多字符最小 58、间距 4、圆角 6、字号 20/15.7 | `KeymapPageView.axaml` + `KeyCellVm` |
| **键格四态** | 选中=白底陶土字/描边 · 禁用=奶油底不可点 · 已绑定=柔和暖绿底 · 空键=象牙底 | `KeyCellVm.RefreshCells` |
| **禁用键集合** | 每个已启用 keymap 的触发键（含 `*` 变体、小写）在其**自身模式与父模式**中禁用 | `KeymapEditorCore.DisabledKeymaps` |
| **绑定判定** | 按**当前窗口分组**判定是否有非空动作（只读不改模型） | `IsBound` |
| **备注汇总** | 各非空动作按 `「分组名: 」+ 译文` 逐行拼接，空备注剔除，右侧独立滚动 | `BuildCommentEntries` + `ActionCommentTable` |
| **页头** | keymap 名称（空则触发键）+ 子模式上层信息（label:503） | `HeaderTitle` / `ParentInfo` |
| **点选** | 键格点击 → 选中态；禁用键的点击被**语义层**忽略（与旧 `Core.SelectKey` 同源） | `Key.SelectKey` |

**闸门**：`cargo clippy --all-targets` **0 告警**；`cargo test` **83/83**（新增 9 项）；`cargo fmt --check` **exit 0**。

## 2. 真机验证（UIA + 像素双证据）

| 步骤 | 证据 |
|---|---|
| 导航切换 | UIA `SelectionItemPattern.Select('F 模式')` 成功 |
| 网格渲染 | 截图：`1-9 0 / QWERTYUIOP / ASDFGHJKL; / ZXCVBNM,. / Space Enter Backspace - [ ' …` |
| 状态着色 | `F`、`Backspace` 显示暖绿（已绑定），其余象牙（空键）|
| 备注列表 | 真实配置备注：`B→Backspace`、`P→Ctrl + Tab`、`N→光标 - 上一单词`、`S→Shift 键`… |
| **可访问性** | UIA 暴露完整按钮树：`Minimize｜Maximize｜Close｜返回｜关闭导航｜保存配置（CTRL+S）｜1..0｜Q…P｜A…L｜Z…` |
| **选中交互（像素判据）** | 陶土像素 `133 → 201`（**Δ+68**）；截图中 `W` 为白底+陶土描边+陶土字 |

## 3. 🔴 本轮最重要的 API 陷阱（已定位根因并修复）

**现象**：点击键格后，选中色不出现（陶土像素增量 = 0）。

**根因定位过程**（不靠猜）：
1. 先做**临时诊断**：选中时给标签追加 `•` → 用 UIA 读按钮名。结果 `W•` 出现 ⇒ **点击已到达、重渲染已发生**，排除了事件与 diff 两大嫌疑。
2. 故问题收敛到：**`resource_overrides` 只在元素创建时应用，重渲染不会重新应用**。

**修复**：把**状态纳入键格 key**（`format!("{hotkey}|{state:?}")`）⇒ 状态变化时 reactor **重建该键格元素**，覆盖随之生效。
代价：状态变化时重建该键格（keyed diff 只重建受影响项）。

**复验**：同一像素判据 `133 → 201`（Δ+68）✅。

> 📌 这条应记入通用经验：**reactor 中「一次性应用型」API（资源覆盖等）若要随状态变化，必须把状态纳入 key**。

## 4. 新掌握的 0.100.0 事实（续 §12 的 6 条）

| # | 事实 | 影响 |
|---|---|---|
| 7 | **只有 `Border` 拥有 `background`/`padding`/`corner_radius`/`border_brush`/`border_thickness`**（Grid/TextBox 另有 background） | 控件级外观只能靠 `resource_overrides` 或 Border 包裹 |
| 8 | **`Button::resource_overrides(ResourceOverrides)` + `style(ButtonStyle)`**；`ResourceOverrides::new().set(key, value)`，`value ∈ {Color, Thickness, CornerRadius}` | 这是 reactor 里唯一的**换肤通道**（等价旧版覆盖 Fluent 资源键）；需 `Style`/`ResourceDictionary` 的旧方案在此有解 |
| 9 | 🔴 **`resource_overrides` 不参与重渲染 diff** | 动态态必须靠 key 重建（见 §3）|
| 10 | `Border::on_pointer_pressed/released/entered/exited/moved` 存在 | 有样式的自绘交互元素可行 |
| 11 | `Button` **无** `padding`/`background`/`foreground` | 无法直接设按钮内边距/底色 |

WinUI 轻量样式资源键（经**官方 MCP 文档**核实，非记忆）：
`ButtonBackground` / `ButtonForeground` / `ButtonBorderBrush`，状态后缀 `PointerOver` / `Pressed` / `Disabled`。
官方原话：*"Modifying these resources is preferred to setting properties such as Background and Foreground."*

## 5. 与旧设计的有意差异（用户已授权 Fluent 化）

| 项 | 旧 | 新 |
|---|---|---|
| 缩放 | 左列 `Viewbox Stretch=Uniform` 整体等比缩放（文字一起缩）| **自然尺寸 + 滚动**（不缩放文字）|
| 键格悬停 | 选中键悬停转陶土、按下转 Coral（自绘样式）| **由 WinUI 模板处理**（同一套资源覆盖，已含 PointerOver/Pressed）|
| 键格内边距 | 单字符 0 / 多字符 10,0 | 略（`Button` 无 padding 接口）|

## 6. 下一步（Phase 3b 续）

1. **动作编辑面板**（`ActionEditorPanel` + `MappingRowVm` + `AddMappingVm`，本页当前为占位卡）——这是模式页最大的剩余件
2. 缩写页（id 2/3）的 chips 视图（`WrapPanel` 缺失，需先定换行方案：`VariableSizedWrapGrid` 在 API 内，待验证）
3. 选中动作页（文本/文件双聚合卡 + 匹配类型选择）
4. 移植 `NavBadge.EffectiveHotkey`（name 为空时的标签推导，当前留 TODO）
