# Phase 1.5 专项 PoC 结论（标题栏 + 弹窗替代）

> 日期：2026-09-28 ｜ 状态：**两项均通过 → Phase 2 门禁解除**
> 产物：`poc/kf-reactor-poc/src/bin/phase15.rs`（可运行）
> 证据：`evidence-phase15.png`（自绘标题栏）、`evidence-phase15-openwindow.png`（双窗口）、`build-phase15.log`

---

## 1. 验证结果

| # | 验证项 | 结果 | 证据 |
|---|---|---|---|
| ① | **自绘标题栏** `TitleBar` 控件 | ✅ **通过** | 截图：标题「KeyFlux 设置面板」+ 副标题 + `RightHeader` 槽位内容均渲染在**窗口标题区**，无多余系统标题栏，最小化/最大化/关闭按钮保留 |
| ② | **弹窗替代** `ComponentContext::open_window` | ✅ **通过** | UI Automation 自动点击：**点击前 1 个顶层窗口 → 点击后 2 个**，新窗口名 `独立窗口（弹窗替代）` |
| ③ | `run_window` + 原生 MessageBox | ❌ **不可用** | `run_window` 在 **0.100.0 不存在**（master 特性） |

## 2. 已核实的真实签名（本机 0.100.0 源码）

```rust
// 自绘标题栏（推荐方案）
TitleBar::new()
    .title("…").subtitle("…")
    .preferred_height(WindowTitleBarHeight::{Standard, Tall})
    // 槽位：TitleBarSlot::{ Content, RightHeader }
    .slot(TitleBarSlot::RightHeader, /* impl Into<View> */)

// 弹窗替代：在 Component::update 内调用（注意：在 ComponentContext 上，不在 ViewContext）
fn update(&mut self, msg: Msg, context: &ComponentContext<Self>) {
    let accepted: bool = context.open_window(View::component::<DialogBody>(()));
}
```

- 每个独立窗口有**独立 Pump**（`ComponentContext::open_window` → `WindowRef::request_open`）。
- ⚠️ `#[must_use]`：返回 `false` 表示当时无活跃发布，非致命错误。

## 3. 新增发现

| # | 发现 | 影响 | 处置 |
|---|---|---|---|
| N1 | **窗口状态不继承**：`DialogBody` 未声明 `window_visuals` 时，新窗口是系统深色主题、无 Acrylic、带系统标题栏 | 每个独立窗口都需**各自**声明 `window_title` / `window_visuals(theme/backdrop)` | 隔离层提供 `WindowSpec`，统一生成窗口声明 |
| N2 | `WindowTitleBarHeight::{Standard, Tall}` 控制标题栏高度；`Tall` 时标题栏参与内容布局 | 设计系统需据此定标题栏高度规范 | 记入主题模块 |
| N3 | `TitleBar::slots()` 是**收尾方法**（返回 `View`）⇒ 无法再对标题栏设 `grid_row` | 标题栏+内容布局需用 `StackPanel`（垂直）或把标题栏作为 Grid 的**独立行**并在收尾前设好 | 已用 `StackPanel::children((title_bar, body))` 验证可行 |
| N4 | 0.100.0 无 `set_timeout`（master 特性），仅有 `spawn_background` / `use_effect` | 定时/延迟逻辑需用 `spawn_background` 或原生计时器 | 记入实现约束 |

## 4. 对正式方案的影响（针对 08 文档 R1/R2 的结论）

- **R1（标题栏）→ 已解决**：改用 `TitleBar` 控件即可获得原生集成自绘标题栏；**不再需要** `windows` 原生互操作去设 `ExtendsContentIntoTitleBar`。
- **R2（弹窗）→ 已解决（方案变更）**：`ContentDialog` 弃用；**模态语义用「独立窗口（`open_window`）+ 父窗口禁用」实现**（父窗口禁用需后续用 `windows` 原生 `EnableWindow` 或 reactor 的未来特性）。
  - 对旧版 8 个弹窗（ActionEditor / BehaviorLibrary / MatchTypes / PluginMarket / PluginSettings / QuickSwitch / WindowGroup / OverviewEdit）⇒ 全部改为独立窗口 + `TitleBar`。
- **R3/R4/R5/R6 维持 08 文档结论**。

## 5. Phase 2 可直接开工

门禁已解除，Phase 2 的任务（骨架 + 隔离层 + DTO/i18n 契约）可直接推进，且隔离层设计据此更新为：

```rust
// platform/ 层需暴露：
pub trait WindowHost { fn open(&self, spec: WindowSpec, root: View) -> bool; }
pub struct WindowSpec { pub title: String, pub theme: WindowTheme, pub backdrop: WindowBackdrop, pub size: (f64, f64) }
pub trait ModalHost { /* 独立窗口式模态：打开 + 父窗禁用/恢复 */ }
```
