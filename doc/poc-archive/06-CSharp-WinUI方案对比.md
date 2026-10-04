# C# WinUI 3 vs Rust + windows-reactor：复杂度对比与决策

> 前置说明（用户 2026-09-28 裁定）：**主题/皮肤将重新设计，不再作为选型约束。**
> 这会**消除 Rust 路线的最大短板**（reactor 无 ResourceDictionary/Style/Setter、无阴影 API），因此选型的决定性因素**只剩一条：windows-rs 是否必须与 UI 同进程**。

---

## 1. 直接回答

| 前提 | 哪个更简单 |
|---|---|
| 「windows-rs 必须与 WinUI 同进程、作为 UI 的核心实现」 | **Rust + windows-reactor 更简单**（C# 路线根本无法同进程用 windows-rs） |
| 「只要求产品中用到 windows-rs / 或 UI 用 WinUI 3 即可」 | **C# WinUI 3 明显更简单** |
| 「既要 WinUI 3 的成熟度，又要用到 windows-rs」 | **混合（C# WinUI 3 + Rust cdylib）复杂度居中**，是务实最优 |

一句话：**在「必须是 WinUI 3」的前提下，C# 方案在除「同进程 window-rs」以外的每一个维度都更简单。**

## 2. 逐维度对比（基于 KeyFlux 代码结构）

| 维度 | C# WinUI 3（Windows App SDK） | Rust + windows-reactor | 优势方 |
|---|---|---|---|
| 语言 | 项目现有语言 | 全新语言栈 | **C#** |
| **现有代码复用** | **高**：26 个 ViewModel / Models / 大部分 Services 可复用（`CommunityToolkit.Mvvm` 与 UI 框架无关，WinUI 3 + MVVM Toolkit 是官方推荐组合） | **零**：全部重写为 Rust | **C#** |
| 视图层 | WinUI XAML（与 Avalonia XAML 概念同源，改写量中等） | 无 XAML，改 builder API 式 `Component/View` | **C#** |
| 框架成熟度 | 成熟：海量文档、样例、WinUI Gallery、设计器、热重载 | 0.100，2026-04 首发，约 1.2k 下载，官方样例仅 counter / theme-transition | **C#** |
| 主题/样式 | **强**：`ResourceDictionary`/`ThemeResource`/`ThemeShadow`/`Style` 全支持 | 仅 `ThemeBrush`(8) + `WindowTheme` + `WindowBackdrop`；无 Style/Setter/阴影 | **C#**（但本次主题重设计 ⇒ 权重降低） |
| 材质 Mica/Acrylic | 原生 `Window.SystemBackdrop` + `SystemBackdropElement` | `WindowBackdrop::{Mica,MicaAlt,Acrylic}` ✅ | 相当 |
| 控件面 | 全部 WinUI 3 控件 + Community Toolkit（含 `WrapPanel`/`UniformGrid`） | 核心控件覆盖广；`WrapPanel`/`ItemsWrapGrid`/`MenuFlyout` = 0 | **C#** |
| 测试体系 | 纯逻辑测试可复用；UI 可走 WinAppDriver/Appium 或截帧 | 全部重写；**未核实 headless 框架** | **C#** |
| **构建链** | **复用现有**：dotnet SDK 10 + Makefile + CI（`release.yml`/`analyzers.yml`） | 新增 Rust 工具链 + 重写 Makefile/CI | **C#** |
| 部署（便携/免安装） | 支持 unpackaged + self-contained，**xcopy 可部署**（见 §3） | 需 `windows-reactor-setup` 部署 WASDK Runtime | **C#** |
| 交付体积 | 较大（含 WASDK Runtime） | 较小（原生 exe） | Rust |
| **windows-rs 集成** | ❌ **不能同进程**；须 Rust cdylib(P/Invoke) 或侧车进程 | ✅ **同进程原生** | **Rust** |
| 启动/体积调优既有工作 | 复用（裁剪/R2R 仍可用） | 全部重做 | **C#** |

## 3. 部署事实核实（C# WinUI 3，官方文档）

对 KeyFlux 这类**免安装便携应用**，C# WinUI 3 是可行的：

- **未打包（unpackaged）应用**需 Windows App SDK Runtime，两种方式：
  1. 随包附带 **runtime 安装器**（用户装一次）；或
  2. **`<WindowsAppSDKSelfContained>true</WindowsAppSDKSelfContained>` + `<WindowsPackageType>None</WindowsPackageType>`** ⇒ 运行时**直接拷到 exe 旁边**，**可 xcopy 部署**（与现有 `bin/ui` + robocopy 模型天然契合）。
- 初始化的自动机制：设 `<WindowsPackageType>None</WindowsPackageType>` 即启用 **auto-initializer（bootstrapper）**。
- `PublishSingleFile`：**仅**「unpackaged + self-contained」组合支持（WASDK 1.5+），且首启会解压到临时目录（非零解压）。
- 代价：self-contained 输出体积显著增大。
- 来源：`learn.microsoft.com/windows/apps/package-and-deploy/unpackage-winui-app`、`.../self-contained-deploy/deploy-self-contained-apps`（经 microsoft-learn MCP 检索）。

## 4. C# 路线的复用度评估（决定"简单多少"）

**可直接复用/微改**：
- `Models/`（`ConfigModels`/`BehaviorModels`/`PluginModels`/`ConfigReadDefaults`）——纯数据，UI 无关；
- `Services/`：`MarkdownParser`（UI 无关）、`HotkeyLogic`、`ConfigSaver`、`SettingsApiClient`（HTTP）、`I18n`、`WindowMatch`、`LinkOpener`；
- `ViewModels/`（26 个）：`CommunityToolkit.Mvvm` 跨框架，主要改 `Avalonia.*` 类型引用（如 `IBrush`、`Dispatcher` → `DispatcherQueue`）；
- `Resources/i18n.json` 契约不变。

**必须重写**：`Views/*.axaml`（18）、`Styles/*`（皮肤）、`Controls/*`、Avalonia 专属转换器、基于 headless/Skia 的像素测试。

> 对比 Rust 路线：**上述 C# 资产复用率为 0**。

## 5. 三条路线与建议

| 路线 | 组成 | 复杂度 | 适用 |
|---|---|---|---|
| **R1 纯 Rust** | Rust + windows-reactor | 高（全量重写 + 新工具链 + 库极新） | windows-rs 必须同进程 |
| **R2 纯 C#** | C# WinUI 3 | **低**（复用 C# 资产 + 现有 CI） | 不强制 windows-rs |
| **R3 混合（推荐折中）** | **C# WinUI 3 做 UI** + **Rust cdylib 承载 windows-rs**（P/Invoke，暴露窄 C ABI） | 中 | 既要成熟 WinUI，又要 windows-rs |

**R3 说明**：Rust 侧编译为 `cdylib`，导出少量 `extern "C"` 接口（如窗口探测、系统信息、特定 Win32/WinRT 能力），C# 侧 `DllImport` 调用。既满足「集成 windows-rs」，又保留 C# 的复用度与 WinUI 成熟度。代价 = 多一个构建目标 + 一层 FFI ABI 需稳定化与测试。

## 6. 决策问题（请裁定）

1. **windows-rs 是否必须与 UI 同进程、作为 UI 的核心实现？**
   - 是 ⇒ 只能 **R1**。
2. **若允许 windows-rs 以「独立 Rust 库/进程」形式存在** ⇒ 建议 **R3**（务实最优），或 **R2**（最简）。

> 若选 R2/R3，现有 `01–05` 文档中的 Rust 专用部分（规范、脚手架、reactor 技能）需相应改写；`02`/`03` 的方案与计划将按新路线重编。
