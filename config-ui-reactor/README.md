# config-ui-reactor

KeyFlux 设置面板（Rust + **windows-reactor** / WinUI）。迁移自 `config-ui-avalonia`。

## 构建

```powershell
cd D:\PortableApps\KeyFlux-main\config-ui-reactor
. .\env.ps1          # 必须：补齐 MSVC + Windows SDK 环境（原因见 env.ps1 顶部注释）
cargo build          # 首次会由 windows-reactor-setup 下载 Windows App Runtime
.\target\debug\KeyFlux.Settings.exe
```

闸门：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -- -D warnings
```

## 部署契约（引擎侧硬编码，不可改）

`bin/Functions.ahk` 硬编码了设置面板的位置与进程名：

```
Functions.ahk:103  Run('"' A_ScriptDir '\ui\KeyFlux.Settings.exe"', A_ScriptDir)
Functions.ahk:136  winTitle := "Setting ahk_exe KeyFlux.Settings.exe"
Functions.ahk:140/146/147/149  ProcessExist/ProcessClose "KeyFlux.Settings.exe"
```

⇒ 两条**不可破坏**的契约：

1. **部署产物必须命名为 `KeyFlux.Settings.exe`**，放在 `<部署树>\bin\ui\`。
   （Cargo bin 名不允许点号，故本工程 bin 名为 `keyflux-settings`，
   由部署步骤重命名——这正是 Phase 4 要落进 Makefile 的动作。）
2. **窗口标题必须包含 `Setting`**（`WindowSpec::default().title = "KeyFlux Settings"`）。

## 边界（不可动）

* Go 后端 `config-server/`、AHK 引擎 `bin/**`、部署树 `data/config.json` **零改动**；
* HTTP 协议串 `KEYFLUX_PORT` / `KEYFLUX_GUI_READY` / `KEYFLUX_BACKEND_EXITED` 语义不变；
* DTO 字段名与 Go/C# 契约**逐字一致**。

## ⚠️ API 依据纪律（重要）

`windows-reactor` 的 **crates.io 0.100.0 与仓库 master 文档不一致**。0.100.0 实测：

| master 文档写法 | 0.100.0 实际 |
|---|---|
| `ViewContext::window_frame(..)` | 不存在（只有 `window_title` + `window_visuals`） |
| `NavigationView::menu_items(..)` | 不存在，用 `SlotsControl::collection_slot(NavigationViewSlot::MenuItems, ..)` |
| `NavigationViewItem::content(..)` / `ToggleSwitch::header(..)` | 不存在，用 `slot(<XxxSlot>::Content/Header, ..)` |
| `ComponentContext::run_window(..)` / `ViewContext::set_timeout(..)` | 不存在（master 特性） |

⇒ **一律以本机 `%CARGO_HOME%\registry\src\...\windows-reactor-0.100.0` 源码为准。**
链式规则：`content(..)` / `children(..)` / `slot(..)` / `slots(..)` 是**收尾方法**（返回 `View`），
其它属性 setter 必须写在它们之前。

## 已验证的关键能力（Phase 1 / 1.5）

| 能力 | 结论 |
|---|---|
| 材质 Mica/Acrylic | ✅ `WindowVisuals::backdrop(WindowBackdrop::{Mica,Acrylic})` |
| 强制浅色 | ✅ `WindowVisuals::theme(WindowTheme::Light)` |
| 自绘标题栏 | ✅ `TitleBar` + `TitleBarSlot::{Content,RightHeader}` |
| 弹窗 | ❌ `ContentDialog` 会崩溃 ⇒ 改用 `ComponentContext::open_window` 独立窗口（✅ 已验证） |
| chips 换行 | ✅ `VariableSizedWrapGrid`（无 `WrapPanel`） |
| 阴影 | ❌ 无阴影 API ⇒ 暂用 1px 描边 |

详见 `D:\PortableApps\KeyFlux-WinUI-migration\`（01 规范 / 02 方案 / 07 对照清单 / 08 Phase 1 / 09 Phase 1.5）。
