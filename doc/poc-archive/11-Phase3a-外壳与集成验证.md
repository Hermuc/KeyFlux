# Phase 3a：外壳迁移 + 真机集成验证（✅ 已完成）

> 日期：2026-09-28 ｜ 工程：`config-ui-reactor` ｜ 证据：`evidence-phase3a-shell.png`

---

## 1. 本阶段交付

| 项 | 内容 | 对应旧实现 |
|---|---|---|
| 自绘标题栏 | `TitleBar` 控件（`WindowTitleBarHeight::Tall`，系统三键原生） | `MainWindow.axaml:230-238` 自绘三键 + 拖动区 |
| 左侧导航 | `NavigationView::MenuItems` 槽（4 项，文案走 i18n 913/914/2418/2581） | `MainWindow.axaml:147-162` 的 `ListBox` + `ItemTemplate` |
| 侧栏页脚 | `NavigationView::PaneFooter`（分隔线 + 保存提示 + 保存按钮 `label:507`） | `MainWindow.axaml:134-144` |
| 内容区三态 | `Content` 槽按 加载中 / 错误 / 页面 互斥切换 | `TransitioningContentControl` + 两层叠加遮罩 |
| 异步连接后端 | `create` 里 `spawn_background`（直连或 `--headless` 子进程 + 健康轮询） | `MainViewModel.InitializeAsync` + `BackendSession` |
| 保存链路 | `clean_for_save` 清洗 → `PUT /config` → `restartFailed` 提示 | `MainViewModel.SaveAsync` + `ConfigSaver` |
| 主题令牌 | Claude 调色板全集（含 12 色断言）+ 圆角 Sm/Md/Lg/Xl + 排版/尺寸令牌 | `Styles/Skins/Claude.axaml` + `ClaudePalette.cs` |

**闸门**：`cargo clippy --all-targets` **0 告警**；`cargo test` **61/61 通过**；`cargo fmt --check` **exit 0**。

## 2. 🔑 真机集成验证（本阶段最高价值证据）

启动命令（指向部署树的真实后端）：

```
target\debug\keyflux-settings.exe ^
  --settings-exe D:\PortableApps\KeyFlux-1.0-beta1\bin\settings.exe ^
  --backend-dir  D:\PortableApps\KeyFlux-1.0-beta1\bin
```

实测结果（见截图）：

| 证据 | 值 |
|---|---|
| 窗口标题 | `KeyFlux Settings`（**满足 AHK 契约**：`Functions.ahk:136` 按标题含 `Setting` + 进程名匹配）|
| 后端子进程 | 面板自行 `--headless` 拉起 1 个 `settings.exe` |
| **真实配置解析** | **`后端已连接 · keymap 16 个 · 布局 5 行 · 热键 261 条`** ⇒ Rust DTO 成功反序列化部署树 **62,220 字节的真实 `config.json`** |
| 导航 / 页脚 / 材质 | 全部渲染正常（浅色主题 + Mica）|

> 这是**契约层的最强验证**：真实生产配置能被我们的 DTO 完整读出，证明字段名/类型与 Go 契约一致。

## 3. 本阶段暴露并已确证的问题

### ✅ 3.1 Job Object 缺口（已复现 → 已修复 → 已验证）

**修复前（实测复现）**

| 步骤 | `keyflux-settings` | `settings`（后端） |
|---|---|---|
| 面板运行中 | 1 | 1 |
| **强杀面板后** | 0 | **1（孤儿）** ← 缺陷 |

**修复后（实测验证）** —— 新增 `src/platform/job.rs`（`windows-sys 0.61.2`）

| 步骤 | `settings`（后端） |
|---|---|
| 基线 | 0 |
| 面板运行中 | 1 |
| **强杀面板后** | **0** ✅ OS 连带回收 |

实现要点：
* `CreateJobObjectW` + `SetInformationJobObject`，`LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK`
  （`BREAKAWAY_OK` 是旧版明确要求保留的：让 `settings.exe` 保存时以 `CREATE_BREAKAWAY_FROM_JOB`
  重启的 KeyFlux **脱离**本 Job ⇒ 关闭设置面板不会连带杀掉托盘程序）。
* Job 句柄在 `BackendSession` **最后**析构（先 kill 子进程，再关 Job 句柄）。
* `windows-sys` 的 `HANDLE` 是裸指针（非 `Send`）⇒ 用 `SendHandle` 显式 `unsafe impl Send + Sync`
  （内核句柄本身跨线程可传递，封装面只有「创建/赋值/关闭」三个线程安全操作）。
* Job 创建失败时**降级**为无 Job 运行（只损失强杀保护，不影响功能）。
* 需要 `Win32_Security` feature（`CreateJobObjectW` 依赖 `SECURITY_ATTRIBUTES` 参数）。

### 🟠 3.2 0.100.0 API 事实（三个都是 master 文档不成立的）

1. **`View` 不实现 `LayoutControl`** ⇒ `grid_row`/`grid_column` 只能设在**未收尾的 builder** 上（`TitleBar`/`NavigationView` 等实现了 `LayoutControl`，但收尾方法返回的 `View` 不能再定位）。master 的 trait 文档示例 `View::fragment(..).grid_column(0)` 在 0.100.0 **不成立**。
2. **窗口无 `window_frame`**（集成标题栏需改用 `TitleBar` 控件 + `window_visuals`，Phase 1.5 已验证）。
3. **无 `run_window` / `set_timeout`**（弹窗改用 `open_window`，Phase 1.5 已验证）。

### 🟠 3.3 样式受限

`NavigationView` 自带背景色，Claude 暖底（Parchment）无法直接透出（无 `ResourceDictionary`/`Style` 可覆盖控件内部画刷）。**待决**：自绘侧栏（`SplitView` + 自建列表）以完全掌控视觉，或接受 WinUI 原生观感。

### 🟢 3.4 环境修复：crates.io 镜像（顺带解决反复超时）

本阶段多次遇到 `spurious network error: transfer too slow ... transferred 0 bytes`，
`windows-sys` 直接下载失败。实测对照：

| 源 | HTTP | 耗时 |
|---|---|---|
| `static.crates.io` | 200 | **12.0s**（且多次超时失败）|
| `rsproxy.cn` | 200 | **1.0s** |

已在 `D:\PortableApps\rust\cargo\config.toml` 配置稀疏索引镜像（`replace-with = "rsproxy-sparse"`）。
**回退方法**：删除该文件即恢复官方源（不改动任何工程文件）。

## 4. 下一步（Phase 3b）

1. **优先补 Job Object**（数据/进程安全，属「特别重要」项）；
2. 使用指南页：消费 `services::markdown` 的块模型渲染（含内联链接点击）；
3. 模式页：键盘矩阵网格（`Grid` 自绘 + 布局解析）；
4. 侧栏视觉决策（§3.3）。
