# Phase 2 进度：骨架 + 契约层（✅ 已完成）

> 日期：2026-09-28 ｜ 状态：**Phase 2 全部完成**（骨架 + DTO + 全部服务层）
> 工程：`D:\PortableApps\KeyFlux-main\config-ui-reactor\`

---

## 1. 交付与验证（全部实测）

| 模块 | 文件 | 覆盖的旧实现 | 验证 |
|---|---|---|---|
| 工程骨架 | `Cargo.toml` / `build.rs` / `rust-toolchain.toml` / `rustfmt.toml` / `env.ps1` / `README.md` / `.gitignore` | — | `cargo build` ✅ 实机运行 ✅ |
| 根组件与导航 | `src/app.rs` | `MainWindow.axaml` + `MainViewModel`（骨架） | 实机截图 ✅ |
| 隔离层 | `src/platform/mod.rs`（`WindowSpec`） | `Services/Win32/DialogChrome`+`DialogPlacer`+`DialogReveal`（抽象位） | 编译 ✅ |
| 主题令牌 | `src/theme.rs` | `Styles/Skins/Claude.axaml` + `ClaudePalette.cs`（**重建**） | 编译 ✅ |
| **DTO 契约** | `src/models/config.rs`（20 结构） | `Models/ConfigModels.cs` 全 561 行 | **9 单测** ✅ |
| **i18n** | `src/services/i18n.rs` + `resources/i18n.json` | `Services/I18n.cs` | **8 单测** ✅ |
| **HTTP 客户端** | `src/services/api.rs` | `Services/SettingsApiClient.cs`（13 端点） | **5 单测** ✅ |
| **后端会话** | `src/services/backend.rs` | `Services/BackendSession.cs`（通告/健康/参数） | **8 单测** ✅ |
| **保存清洗** | `src/services/store.rs` | `Services/ConfigSaver.cs` 全 172 行（含 `ConfigActions`） | **14 单测** ✅ |
| **Markdown** | `src/services/markdown.rs` | `Services/MarkdownParser.cs` 全 169 行 | **14 单测** ✅ |

**闸门结果**：`cargo test` → **58/58 通过**；`cargo clippy --all-targets` → **0 告警**；`cargo fmt --all -- --check` → **exit 0**。

## 2. 依赖（版本均由 `cargo add` 实测锁定，非推测）

| crate | 版本 | 用途 |
|---|---|---|
| `windows-reactor` | 0.100 | WinUI 声明式 UI |
| `windows-reactor-setup` | 0.100 | 自包含部署（build-dependency） |
| `serde` / `serde_json` | 1.0.229 / 1.0.151 | DTO 序列化 |
| `ureq` | 3.4.2 | 阻塞式 HTTP（在 `spawn_background` 中调用）；`http_status_as_error(false)` 以取非 2xx 的 `message` |
| `regex` | 1.13.1 | Markdown 解析（忠实复刻 C# 正则语义） |

## 3. 已锁死的契约（单测守卫）

- **DTO**：顶层键集合、`Options` 14 键、`CommandInputSkin` 18 字段全为 string、字面 `*ID` 键（`parentID`/`windowGroupID`/`actionTypeID`/`actionValueID`）、`SelectedEntry.actionValue`/`workingDir` 空串省略键、`plugins`/`commandFont` 容忍 `null`、`is_new`/`is_empty` 不序列化。
- **i18n**：键数守卫 **398**（实测，旧记忆 393 已过时）+ 6 个非数字键 + `label:` 前缀 + 语言回退 + 占位符不抛异常。
- **协议串**：`KEYFLUX_PORT=` / `KEYFLUX_GUI_READY` / `KEYFLUX_BACKEND_EXITED` / `--headless` / `settings.exe` 五条字面量冻结测试。
- **后端**：20s 端口通告 / 15s 健康轮询 / 候选路径顺序（`bin\ui\settings.exe` → `bin\settings.exe`）/ 参数解析语义。
- **保存链路**：空动作剔除（下标对齐）、自定义 keymap（`id > 4`）按键集合过滤（**大小写敏感**）、`change_abbr_enable` 的「倒数第 3/2」语义、`normalize_key_name` 大小写不敏感 + `*` 变体。
- **Markdown**：`#{1,4}` 后必须空白（`#####` 非标题）、列表 2 空格缩进最多 3 层、段落合并与中断条件、行内链接/代码按索引配对。

## 4. 有意为之的差异（都已在代码注释中写明理由）

| 项 | 旧（C#） | 新（Rust） | 理由 |
|---|---|---|---|
| i18n 资源 | 松散文件 + 运行期路径探测 | `include_str!` 编译期内嵌 | 免探测；守卫测试可直接对账 |
| i18n 格式化 | `string.Format` + `catch(FormatException)` | 手动替换 `{i}` | 语义等价，天然无异常 |
| `Keymap.hotkeys` | `Dictionary` | `BTreeMap` | PUT 载荷**逐字节可复现**；Go 侧迭代顺序本就随机 |
| 后端生命周期 | Job Object（`BREAKAWAY_OK`） | `Drop` 终止子进程 | ⚠️ **已知缺口**：强杀 GUI 会留孤儿；补齐需 `windows` crate |
| Markdown | 手写/正则混合 | `regex` crate | 忠实复刻正则语义，降低手写解析器的偏差风险 |

## 5. ⚠️ 契约数字更正

| 项 | 旧记忆 | **实测（2026-09-28）** |
|---|---|---|
| i18n 键数 | 393 | **398** |
| 非数字键 | 6 个 | 6 个（一致）|

`i18n.json` SHA256：`301BC8ECE80BDAAD18B158700CAA8C28342E5C72C48EA79853194AE357D6C81A`

## 6. 下一步：Phase 3（逐页迁移）

按计划顺序：主窗骨架 → 设置页 → 使用指南（Markdown 渲染）→ 模式页（键盘网格）→ 缩写页 → 选中动作页 → 匹配类型 → 插件页 → 其余弹窗。

Phase 3 需先补齐两个**平台能力**（Phase 1.5 已验证方向）：
1. `TitleBar` 自绘标题栏 + `window_visuals` 材质（已验）；
2. 弹窗替代 `ComponentContext::open_window`（已验）+ 父窗假模态；
3. Job Object（补 §4 缺口）；
4. 视觉验收工具：`Windows.Graphics.Capture` 截帧 + 像素/WCAG 回读（L3 测试层）。
