# Phase 1（环境 + PoC）结论与方案修订建议

> 日期：2026-09-28 ｜ 状态：**PoC 通过（8 项中 7 项通过，1 项发现阻塞）**
> 产物：`poc/kf-reactor-poc/`（可构建、可运行，含截图与构建日志证据）

---

## 1. 环境落定（实测）

| 组件 | 状态 | 位置 |
|---|---|---|
| Rust | ✅ 1.98.1（≥ reactor MSRV 1.95） | `D:\PortableApps\rust\rustup` / `...\cargo`（C 盘只读规避） |
| rustfmt / clippy | ✅ 1.9.0 / 0.1.98 | 同上 |
| MSVC | ✅ 14.44.35207（VS Community 2026） | `C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.44.35207` |
| Windows SDK | ✅ 10.0.28000.0 | `C:\Program Files (x86)\Windows Kits\10` |
| reactor 依赖 | ✅ 14 包全部锁定并缓存（`Cargo.lock` 已生成） | `D:\PortableApps\rust\cargo` |

**🔴 关键环境坑（已在 PoC README 固化配方）**：`vswhere` 查不到 VS 2026 的 VC 组件 ⇒ rustc 自动探测失败。必须手工把 MSVC `bin\HostX64\x64` 与 SDK `bin\<ver>\x64`（含 `mt.exe`）加入 `PATH`，并设 `LIB` / `INCLUDE`。否则依次报 `link.exe not found` → `LNK1158: 无法运行 mt.exe`。

## 2. PoC 结论（8 项）

| 项 | 结果 |
|---|---|
| 材质 Acrylic/Mica（`WindowBackdrop`） | ✅ |
| 主题表达力（Color/Brush/ThemeBrush/CornerRadius/Thickness/WindowTheme::Light） | ✅ |
| 自绘网格（Grid + Border 卡片） | ✅ |
| chips 换行（`VariableSizedWrapGrid`） | ✅ |
| 交互与状态回流（受控 TextBox / ToggleSwitch / NavigationView） | ✅ |
| 强制浅色（`WindowTheme::Light`） | ✅ |
| 自包含部署（`windows-reactor-setup`） | ✅ |
| **ContentDialog 弹窗** | ❌ **启动崩溃 0xC000027B** |

证据：`evidence-frame.png`（深色，暴露问题）→ `evidence-frame-light.png`（最终验收图）；`build-attempt1..4.log`（linker/mt.exe 排错链）。

## 3. 对原计划的修订建议

| # | 原方案假设 | 实测事实 | 修订建议 |
|---|---|---|---|
| R1 | 可用 `window_frame` 做「集成标题栏 + 材质覆盖整窗」 | **0.100.0 无 `window_frame`** | 改为：`window_visuals` + `TitleBar` 控件自绘标题栏，或 `windows` 原生互操作（`ExtendsContentIntoTitleBar`）。**需追加一轮专项 PoC** |
| R2 | `ContentDialog` 作为弹窗方案 | **进 view 树即崩溃** | 改用 `open_window`（独立窗口）或 `run_window` + 原生 MessageBox；**需追加一轮专项 PoC** |
| R3 | 以 master 的 `public-api.txt` 作为 API 依据 | **master ≠ 0.100.0**（`menu_items`/`content`/`header`/`window_frame` 均不存在） | **一律以 `cargo registry/src/<pkg>` 本机源码为准**；已更新技能 `windows-reactor` |
| R4 | 强调色可像旧版那样覆盖 `SystemAccentColor` | reactor 未见覆盖系统 Accent 的入口（ToggleSwitch 仍为默认色） | 暖色强调色需自绘控件或原生互操作；**列为主题设计的待解项** |
| R5 | 阴影用 `ThemeShadow` 补齐 | reactor 无阴影 API | 先按 1px 描边方案（PoC 已验证可行）；`ThemeShadow` 走原生互操作作为增强项 |
| R6 | 打包体积可控 | `target\debug` 被拷入 ~90 个语言目录 | 正式构建需白名单过滤（对齐旧版 `SatelliteResourceLanguages`） |

## 4. 对 Phase 2 的输入

- **构建环境固化**：把 §1 的 PATH/LIB/INCLUDE 配方写入 `.cargo/config.toml` + `env.ps1`，避免每台机器手工设置。
- **隔离层加强**：`window_frame`/`ContentDialog` 的缺失说明「UI 原语层」必须有适配位 —— 建议在 `platform/` 暴露 `WindowHost`（标题栏/材质）与 `ModalHost`（弹窗）两个 trait，实现可换（reactor 原生 or `windows` 原生）。
- **API 依据纪律**：所有控件签名从本机 `cargo registry` 源码提取；技能 `windows-reactor` 已补「0.100.0 真实 slot API」章节。
- **对照清单**：见 `07-迁移对照清单.md`（18 视图 / 26 VM / 26 Service / 模型契约 / 测试分类）。

## 5. 建议的下一步（需你确认）

Phase 1 是计划中设定的**门禁**：按原计划「PoC 通过才进入规模化开发」。当前 7/8 通过，但 **R1（标题栏）与 R2（弹窗）两项是正式开发的强依赖**，建议：

- **选项 A（推荐）**：先追加 **Phase 1.5 专项 PoC**（1~2 天量级）验证 `TitleBar` 自绘标题栏 + `open_window`/`run_window` 弹窗替代，再进入 Phase 2 骨架。
- **选项 B**：直接进入 Phase 2（骨架 + 契约层，与 UI 原语无关部分），把 R1/R2 的验证并行推进。
