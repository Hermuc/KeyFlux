# Phase 4/5 — 构建部署集成与收尾

> 状态：**已完成，Avalonia 已退役**（2026-09-28，用户确认后执行，见 §7）。

## 1. 交付总览

| 层 | 内容 | 验证 |
|---|---|---|
| 页面 | 导航外壳 / 使用指南 / 选中动作 / 插件 / CapsLock+F 模式 / Command / 缩写 / 选项 | 真机逐页截图 |
| 弹窗 | QuickSwitch 配置 / 插件市场 / 插件声明式设置 / Win32 文件选择器 | 真机逐个截图 |
| 服务 | api(13+5 端点) / backend / store / markdown / keymap / abbr / plugins / market / action_editor / selected_action | cargo test 157/157 |
| 进程 | Job Object（强杀面板连带回收后端，BREAKAWAY_OK 保托盘存活） | 实测前后对照 |
| 构建 | make buildClientReactor（三闸门 + 自包含发布）/ build-reactor / deploy-reactor | 实跑 + dry-run |
| CI | analyzers.yml 新增 reactor-gates job（fmt/clippy/test） | YAML 复核 |

## 2. 闸门（最终状态）

| 项 | 结果 |
|---|---|
| cargo fmt --all --check | exit 0 |
| cargo clippy --all-targets -- -D warnings | 0 告警 |
| cargo test | 157/157 通过 |

三层测试替代（对应旧版 ModelUnitTests / XAML 手测 / CI analyzers）：

1. 单测：models 契约（键名/回退/往返）+ 各服务纯逻辑（含并行写入者的等价测试合并）；
2. 真机集成：每页/每弹窗启动真实后端（KeyFlux-1.0-beta1 部署树）操作并截图，
   含「真实 62KB config.json 反序列化」「真实插件目录」「真实声明式设置表单」；
3. CI 闸门：reactor-gates（与 Makefile 严格一致）+ 既有 .NET analyzers 并行保留。

## 3. Phase 4 细节

### 3.1 Makefile 新目标

```
make buildClientReactor   # 三闸门 -> cargo build --release -> bin/ui（自包含）
make build-reactor        # buildServer + buildClientReactor + copyFiles + 打包
make deploy-reactor       # check + buildClientReactor + sync-out + 重启实例
```

* 闸门与构建统一经 config-ui-reactor/env.ps1 注入工具链（本机 PATH 陷阱），
  每步显式判定 $LASTEXITCODE。
* 自包含：build.rs 的 windows_reactor_setup::as_self_contained() 把 pinned Windows App
  Runtime stage 进 target/release（exe + 28 DLL + 4 PRI + 87 语言资源目录）。
  拷贝用 robocopy 排除 cargo 中间产物（.fingerprint/build/deps/examples/incremental、
  *.pdb/*.d/*.cargo-*lock）⇒ 实测 207 文件 / 65MB（与 Avalonia 的 65M 同体积）。
* 与 Avalonia 的差异：i18n 用 include_str! 编译期内嵌（唯一真源 resources/i18n.json），
  故无「散资源 + SHA256」断言；键数与语义一致性由 i18n 单测守护。
* 保留 buildClientAvalonia / build / deploy 原样（退役前双轨）。

### 3.2 Runtime 装载（实测）

bin/ui/keyflux-settings.exe 直启真实后端：面板存活、窗口 KeyFlux Settings，
证明自包含 Runtime 在发布位置（而非构建目录）可用。

### 3.3 CI（.github/workflows/analyzers.yml）

新增 reactor-gates job：windows-latest + dtolnay/rust-toolchain@stable
（版本由 rust-toolchain.toml 钉住）+ cargo 缓存 + 三闸门。
与 .NET analyzers 同哲学：只给红灯不拦发布（release.yml 未动）。

## 4. 真机证据（config-ui-reactor/evidence-*.png）

| 文件 | 内容 |
|---|---|
| evidence-phase3a-shell.png | 外壳 + 真实 config 解析（keymap 16 / 布局 5 行 / 热键 261 条） |
| evidence-phase3b-*.png | 指南 / 键位图 / 动作编辑面板 / 缩写页 / 选中动作页 |
| evidence-phase3c-plugins.png | 插件页（内置卡 + 真实 Everything 插件 v1.0.1） |
| evidence-phase3c-quickswitch-dialog.png | QuickSwitch 配置对话框（4 开关 + NumberBox + 排除目录） |
| evidence-phase3c-market.png | 插件市场对话框（错误态 + 重试，本机 TLS 拦截所致） |
| evidence-phase3c-plugin-settings.png | 声明式设置表单（char/text/file/number 四种编辑器 + 当前值回填） |
| evidence-phase4-binui-launch.png | bin/ui 发布位置直启 |

## 5. 已知差异与遗留（有意为之）

| 项 | 说明 |
|---|---|
| Avalonia 退役 | 需用户确认后才删除 config-ui-avalonia/ 并把 build/deploy/release.yml 切到 reactor |
| 对话框深色 | reactor 0.100.0 无 requested_theme API，ContentDialog 跟随系统深色主题 |
| 插件市场网络 | 目录走 GitHub raw（外部网络）；本机代理拦 TLS 属环境限制，错误/重试路径已验证 |
| 长键位图性能 | 0.100.0 无虚拟化容器（旧版 ItemsRepeater 虚拟化）；实测 261 热键页可接受 |
| make 需 bash | 与既有目标同约束（rm/cp/mkdir 为 POSIX 语义） |

## 6. 双写者合并记录（本轮）

另一位并行写入者与我在 services/keymap.rs / abbr 上撞车，处理原则
（与《多 Agent 冲突对账》文档一致）：

* 我为主线（services/abbr.rs 唯一实现），但吸收其两处更优设计：
  1. SPACE_MARK 用 U+25A1 单色方块替代旧版 U+25FD+VS16 —— 规避 emoji 回退渲染成彩色方块；
  2. chip_item_width（按最长标签估算统一格宽）取代写死 76px。
* 其 abbr_comment_entries / AbbrChip / abbr_chips / AbbrCommand 等旧实现与测试
  已删除（等价覆盖并入 abbr.rs），app.rs 调用点全部改走 abbr::*。
* services/api.rs 中其遗留的本地 PluginSettingsRequest（HashMap 版）删除，
  统一用 models::PluginSettingsRequest（BTreeMap，序列化逐字节可复现）。

## 7. Avalonia 退役（2026-09-28，用户确认后执行）

**删除**：`config-ui-avalonia/`（.NET 10 / Avalonia 11 客户端）与 `KeyFlux.Settings.Tests/`
（其 C# 单测；契约覆盖已等价迁移为 `cargo test` 157 项）。

**切换**（全部完成后实测复核）：

| 处 | 变更 |
|---|---|
| `Makefile` | 删 `buildClientAvalonia` / `check-cs` / 临时 `build-reactor`、`deploy-reactor`；`build` / `out` / `deploy` 全部改依赖 `buildClientReactor`；`analyzers` 改为 Rust 风格闸门（fmt + clippy） |
| `Makefile`（关键） | 发布拷贝时把 `keyflux-settings.exe` **改名为 `KeyFlux.Settings.exe`** —— 引擎按该名拉起/关闭面板（`bin/lib/core/Functions.ahk:103/136-149`）；robocopy 排除原名 + `Copy-Item` 落新名 + `test -f` 断言 |
| `.github/workflows/analyzers.yml` | 删 .NET `analyzers` job（含 setup-dotnet），仅留 `reactor-gates`；头部留档历史 |
| `.github/workflows/release.yml` | 删「C# unit tests」+ trx 上报 + 「Publish Avalonia UI」+「Assert i18n loose resource hash」；新增「Build reactor UI」（三闸门）与「Deploy reactor UI to bin/ui」（robocopy + 改名 + 断言） |
| `readme.md` / `readme.en.md` | 客户端描述与构建说明改为 `config-ui-reactor` |
| `.gitignore` | 清 Avalonia/Tests 产物条目，补 `config-ui-reactor/target/` |

**退役后实测**：三闸门全绿（fmt=0 / clippy 0 / 157 测试）→ 发布集 207 文件 / 65MB →
`bin/ui/KeyFlux.Settings.exe` 直启真实后端成功（窗口 `KeyFlux Settings`，
证据 `evidence-phase5-retired-launch.png`）。仓库根已无 `.NET` 工程。

**历史文档保留**：`doc/与原版的差异.md` 等变更记录中提及 `config-ui-avalonia` 的条目
均为历史事实（当时修改的文件路径），不改写。



