# 方案：在代码安全前提下将 Go 后端职责迁移到 Rust（终版）

## 0. 项目信息

| 项 | 值 |
|---|---|
| Language | 简体中文 |
| 文档状态 | **方案（未实施）** |
| 目标 | Rust 自包含（面板 + 同名 `settings.exe`），最终退役 Go；去掉 localhost/HTTP/常驻后端 |
| 硬前提 | **代码安全第一**：契约冻结 + 差分对账 + 可回退 + golden 随迁 |
| 依据文档 | `docs/CONTRACTS.md`、`config-server/**`、`config-ui-reactor/**`、`bin/Launcher.ahk`、`Makefile` |

### 现状

```
AHK 引擎 ──(Run)──> KeyFlux.Settings.exe（Rust/WinUI 面板）
                        │ spawn --headless
                        ▼
                    settings.exe（Go 后端，HTTP 127.0.0.1:12333）
                        │
                        ▼
                    data/config.json
```

- `settings.exe` **已是双形态**：① HTTP server（**仅服务面板**，`site/` web UI 已退役）；② **CLI**（`GenerateScripts`/`GenerateAHK`/`DumpPlan`/`ChangeVersion`/`UseOriginalAHK`），**引擎 `bin/Launcher.ahk:40`、`tools/oracle.ps1`、`误报病毒时执行这个.bat` 都在用**。
- 因此「去掉 localhost」与「把逻辑迁到 Rust」是**两件可分开做的事**。

## 1. 不变式（贯穿全程，违反即停）

1. **契约面逐字不变**（见 §3）。
2. **单一真源**：迁移期间 Go 与 Rust 由**差分对账强制等价**，属"受守护的对照实现"，**不是**无对账的第二真源。
3. **对账不通过不许切换**；每阶段**独立可回退**；旧实现保留到 P4 才删。
4. **每批提交前 baseline 全绿**：`cargo fmt --check` / `clippy -D warnings` / `cargo test`（175）/ golden / Oracle 计划 diff。

## 2. 契约面（冻结，禁改）

| 项 | 内容 |
|---|---|
| 二进制名 | `settings.exe`（Launcher / oracle / bat / 面板全按名调用） |
| 子命令 | `--headless`、`GenerateScripts`、`GenerateAHK <cfg> <tmpl> <out>`、`DumpPlan <cfg> <plan>`、`ChangeVersion`、`UseOriginalAHK` |
| stdout 串 | `KEYFLUX_PORT=<n>`（headless 首行）、`KEYFLUX_GUI_READY`、`KEYFLUX_BACKEND_EXITED`；**新增** `KEYFLUX_SAVE ok\|restartFailed` |
| 运行语义 | `cwd = bin/`（Go 依赖相对 `../data`、`./templates`）；产物 `bin/KeyFlux.ahk`（**必须带 BOM**） |
| HTTP 端点（P1 前） | `/health`、`/config`(GET/PUT)、`/shortcuts`、`/server/command/:id`、`/api/selected-action/{test,play}`、`/api/behaviors…`、`/api/plugins…` |

## 3. 目标模块结构

```
config-ui-reactor/            ← crate = lib + 2 bin（cargo 原生布局，不引入 workspace 重排）
  src/lib.rs             models · Parse/Save/Clean · 校验 · 生成器 · 契约常量   ← 单一真源
  src/main.rs            bin KeyFlux.Settings.exe（面板：仅 UI + 端口调用）
  src/bin/settings.rs    bin settings.exe（CLI：drop-in 替代 Go）
  src/services/api.rs    trait SettingsApi（端口·已有）
                           ├── HttpSettingsApi（保留至 P3）
                           └── LocalCliApi（文件 + CLI，P1 新增）
  src/platform/          Ports & Adapters 唯一落点（进程 / Job Object / 文件）
```

## 4. 阶段路线

| 阶段 | 交付物 | 验收门 | 回退 |
|---|---|---|---|
| **P0 护栏** | ✅ `tools/parity/`：3 条语料（`factory` / `factory-plugins` / `synthetic`）× 3 类产物（`plan` / `ahk` / `skin`）共 **9 份基线**，SHA-256 逐字节比对；`Makefile` 加 `parity`；CI 加 `parity-gate` | harness 能抓假等价（反证：篡改任一基线 ⇒ `MISMATCH [...]` + exit 1）；`-Capture` 跑两遍拒录非确定语料 | 纯新增 |
| **P1 面板去 HTTP** | ✅ 已就绪（**默认值保持 `http`，且据 §4 ROI 复核决定不再翻**）：Go 进程内桥 `server.Call` + `CliSettingsApi`（18 方法全覆盖）+ `new_settings_api` 统一工厂 + `--api=cli`／`KEYFLUX_API` 阀 + 静态资源直读（§5 #12）+ **探测失败自动回退 HTTP**。未做（需真机且收益不足）：CLI 下「保存→重启引擎」走 Go 的 breakaway 降级分支 | 两传输同源（**共用同一套 gin handler**）；cargo 189 全绿；Go `Call` 冒烟实测 | `--api=http` / `KEYFLUX_API=http`（免重建） |
| **P2 Rust 接管外围** | ✅ 仅保留**零变换**项：「使用指南文档 + 快捷方式列表」改**本机直读**（`services/local_fs.rs`，读不到回退后端）。❌ `GET /config` **不可**直读替代（§5 #13）。❌ plugins / behaviors **主动不做** —— 按 §4「ROI 复核」，复刻 965 行解析逻辑换 150 ms/次不值得 | `config_doc.md` 7522B 与后端逐字节相同；`shortcuts` 146 项与后端完全相同；cargo 189 全绿 | 直读返回 `None` 即自动回退后端（等于改动前行为） |
| **P3 Rust 重写生成器 + drop-in** | 🚧 进行中（**已跑通首个端到端对账**）：Rust `settings.exe` 已实现 `DumpPlan`，对 3 条语料的 `plan.json` **逐字节等于 Go 基线**（`-Kinds plan` → 3/3 PASS）。已迁：文本层 / 配置模型（`model`）/ 行为目录最薄加载 / `ParseConfig`+`Preprocess` / `plan`。⏳ 待迁：两个模板改写为 Rust 生成 + `type1..type9` 渲染器 ⇒ `GenerateAHK`；随后才是 drop-in 切换 | **全语料 parity 100%**（当前 plan 段 100%）+ golden + Oracle diff | 换回 Go 二进制（开关） |
| **P4 Go 退役** | 消费方全指向 Rust；golden/CONTRACTS/Oracle 随迁；CI 由 `go test` → cargo + parity | 三闸门 + parity 全绿 | git 历史 |

**顺序理由**：先去掉「连接」（P1），再换掉「实现」（P3）——两类风险分开，出问题能立刻归类。

**实测数据（2026-09-29，决定"默认值能不能翻"的输入）**

| 传输 | 每次调用成本 | 开面板（3 次调用） |
|---|---|---|
| HTTP | 一条复用的 loopback 连接（~1 ms） | 拉子进程 + 端口通告 + `/health` 轮询（数百 ms，冷启动可到 ~2 s） |
| CLI | **每次 spawn 一个 `settings.exe`：实测 130–180 ms**（`Call GET /health` 5 次 128–160 ms；`Call GET /config` 5 次 162–180 ms） | P2 第一片已去掉 2 次 ⇒ 剩 1 次 `get_config` ≈ 150 ms |

⇒ CLI 换来「无 loopback、无常驻后端」，代价是**按调用计费**。因此 **P2 的"把纯文件查询下移到 Rust"直接决定 CLI 能否当默认值**：把读路径都变成 0 次 spawn，CLI 才不吃亏。

**ROI 复核（2026-09-29，据上表）**：P2 剩余两项的收益**远低于**其成本 ——

| 项 | 改成直读的收益 | 成本 / 风险 |
|---|---|---|
| `GET /api/behaviors` | CLI 模式少 1 次 spawn（~150 ms） | Go `internal/behaviors` **965 行**（`loadDir` / `sortPacks` / 错误隔离 / plugin 目录同 ID 跳过 / `appliesTo` 校验）搬进 Rust ⇒ 大块第二真源，且 Go 侧已有一整套测试 |
| `GET /api/plugins` | 同上 | 同类（DTO + 目录扫描语义） |
| `GET /config` | 少 **启动时唯一**的 1 次 spawn | **不可做**（§5 #13：DTO 是超集，需复刻 5 个派生字段） |

⇒ **结论：不翻默认值、也不继续 P2 的"复刻式直读"**。CLI 传输**保持 opt-in**（`--api=cli` / `KEYFLUX_API=cli`），要"彻底不连 loopback"应由 **P3（Rust 生成器 + 同名 `settings.exe` drop-in）** 实现 —— 那才是真正减少 Go 的部分，而不是把 Go 的逻辑抄一份。P2 只保留**零变换**的 `config_doc.md` / `shortcuts`（已完成）。

## 5. 迁移特有的隐藏 bug（"代码安全"的考点）

| # | 隐藏 bug | 对策 |
|---|---|---|
| 1 | **BOM 丢失**：Rust 写文件默认不写 BOM，而 `keyflux.tmpl` 带 BOM 是铁律 | 写 `KeyFlux.ahk` **显式写 BOM**；parity 会抓到 |
| 2 | **Go `json.Marshal` 默认 HTML 转义**（`<`/`>`/`&` → `\u003c`），serde_json 不转 | parity 逐字节比对暴露；按需对齐编码 |
| 3 | 键序 / 字段序：Go 结构体序 vs serde 声明序 / `BTreeMap` 排序 | 显式固定序列化顺序 |
| 4 | 数字格式化：Go `%v` vs Rust `Display`（浮点尾数、科学计数） | parity 覆盖超长/浮点样本 |
| 5 | 行尾 CRLF vs LF | 统一按 Go 现状，parity 逐字节校验。⚠️ 实测坑：`core.autocrlf=true` 会在 checkout 时改 reference 行尾 ⇒ 仅 CI 假红；`tools/parity/{reference,corpus}` 已在根 `.gitattributes` 标 `-text` |
| 6 | **CLI 中文错误乱码**（输出经管道走控制台码页） | CLI 输出强制 UTF-8；结果行走 ASCII 标记 |
| 7 | **保存失败被静默吞**（`proc.ExecCmd` 是 fire-and-forget，不查退出码） | CLI 适配器**必等退出码 + 解析 `KEYFLUX_SAVE`** |
| 8 | **引擎不重生成**：`Launcher.ahk` 的 `NeedsRegenerate` 用 **mtime** 判定 | 保存后保证 mtime 更新，或显式触发重生成 |
| 9 | 保存副作用遗漏：缓存失效 / 外观变更 kill CommandInput / `restartFailed` 提示 | CLI 子命令逐条等价，用例 + parity 覆盖 |
| 10 | `options.startup` 直读失真（现靠后端查 `schtasks` 回填） | 面板自查计划任务 |
| 11 | 并发写 / 半写文件（面板被杀留 `*.tmp`） | `*.tmp` + 原子 rename；读侧容忍半写 |
| 12 | ~~静态资源（指南 4 图 + 内部链接）依赖 `127.0.0.1:<port>`~~ | ✅ **已解**：vendor reactor **内建** `Image::source_data(EncodedImage)`（WinRT `SetSourceAsync` **流式**加载，不依赖 URI 方案）与 `source_file`。CLI 模式登记 `<deploy>/bin/site` 后：图片直读 + `source_data`、内部链接走 `file:///`（`ui/doc_assets.rs`）。**未登记时（HTTP 模式）行为逐字不变** |
| 13 | **`GET /config` 的响应是 `config.json` 的严格超集** ⇒ 面板直读 `config.json` 会**静默丢字段**。2026-09-29 实测（出厂 config）：DTO 多出顶层 `fileGroups` / `matchTypes`、`options.commandFont` / `options.plugins`（`ConfigToDTO` 补），且 `options.startup` 被 `schtasks` 回填（`cfg.startup=false` 而 `dto.startup=true`） | **config 一律走后端**；若将来要直读，必须先让 `ConfigToDTO` 的派生集合成为**受测契约**（多一个派生字段即测试变红），并在 Rust 侧复刻 5 个派生项 —— 代价高，暂不做 |
| 14 | **纯目录 glob 的两个隐性口径**（`GET /shortcuts`）：① 分隔符 —— Go 的 `filepath.Glob` 产出 `<root>\shortcuts\X.lnk` 后按前缀截取 ⇒ 结果是**反斜杠**，Rust 手写 `shortcuts/{name}` 会与后端不等（已实测复现）；② `filepath.Glob` **只按名字匹配、不 stat** ⇒ **目录条目也算命中**，加 `is_file()` 过滤即行为变更 | 复制 Go 口径（`Path::join` 生成反斜杠；不过滤目录），并以单测 + 与后端的逐项对账锁定（`services/local_fs.rs`） |

## 6. 安全机制（四点保证）

1. **契约冻结**：§2 逐字不变，加"禁改"测试。
2. **差分对账**（P0）：迁移的许可证——对账不过不许切。
3. **可回退开关**：`--api=http|local` + 保留旧二进制，直到 P4。
4. **验证资产随迁**：golden / `CONTRACTS.md` / Oracle `DumpPlan` 计划 diff 必须在 Rust 侧继续生效。

## 7. 前置动作与首个里程碑（P0 清单）

**前置**
1. 先提交工作区现有改动（导航栏宽度 264、`backend.rs` 过期文档更正）⇒ 干净基线。
2. 冻结 `docs/CONTRACTS.md` 中生成契约，作为 parity 的期望源。

**P0 开工清单（零行为风险）**
- [x] 建 `tools/parity/run_parity.ps1`：`-Capture` 录制 / 默认 CHECK；跑两遍拒绝录制非确定性语料；纯 ASCII 输出 `PARITY: n/n PASS` + 退出码。
- [x] `Makefile` 加 `parity` 目标；`.github/workflows/analyzers.yml` 加 `parity-gate`（用当前 `settings.exe` 复现基线）。
- [x] **反证**：篡改 reference ⇒ `MISMATCH [ahk]` + exit 1（证明不是恒真断言）。
- [ ] 语料扩充：当前仅 `factory`（`data/config.json` 冻结副本）；待补「含 `plugins/` 的项」（manifest 已支持 `plugins` 字段）与边界样本。

## 8. 附：不推荐的路径（备忘）

- **直接删除 Go 并让 Rust 重写**（无对账）：正是「第二真源」的制造方式——两份无对账实现必然漂移，症状是"设置改了引擎不生效"这类**静默 bug**。
- **只删原真源**：不解决分叉，只是换了幸存者；若忘记迁移消费方（`Launcher.ahk` 等）会直接破坏核心功能。
- 正确姿态：**收敛到一份**，而非"删掉其中一份"。本方案即"先对账、再替换、后淘汰"。

## 9. 附：P4 切换 runbook（2026-09-29 更新，代码侧已全部就绪）

**当前态**：Rust `settings.exe` 五个子命令（DumpPlan / GenerateAHK / GenerateScripts /
ChangeVersion / UseOriginalAHK）+ `InstallCommandFont` 均已实现；对 4 条语料的
plan/ahk/skin 共 12 份产物与 Go **逐字节一致**（双向 `PARITY: 4/4 PASS`）；
`GenerateScripts` 已在沙箱部署树做 Go vs Rust 差分冒烟（两产物 SHA256 相同）。

**消费方无需改动**：`bin/Launcher.ahk:40`、`tools/oracle.ps1`、`误报病毒时执行这个.bat`
全部按**文件名**调用 `bin/settings.exe` —— 覆盖该文件即完成切换。

**切换步骤**（需要真机验收，建议用户在场）：

1. `make parity` 必须 `4/4 PASS`；`cargo test` 全绿。
2. `make drop-in-rust`（新增目标：fmt/clippy/test/release 全过才覆盖 `bin/settings.exe`）。
3. 仓库内验证：`cd bin && ./settings.exe DumpPlan ../data/config.json %TEMP%\p.json`，
   与 `tools/parity/reference/factory.plan.json` 比对应相等。
4. 部署：`make deploy`（sync-out 会把新二进制带进部署树并重启实例）。
5. **真机验收清单**：① 引擎开机生成（`Launcher.ahk` 的 `NeedsRegenerate` 走 mtime，
   首次切换建议删 `bin/KeyFlux.ahk` 强制重生成）；② 设置保存→引擎重启生效；
   ③ **换命令框字体生效**（`font/font.ttf` 由 Rust `InstallCommandFont` 落盘）；
   ④ 命令框唤起正常（字体渲染）。
6. 回退：`make buildServer`（重建 Go 版覆盖回去）—— Go 源码与 `go test` 守卫保留至确认稳定。
7. 稳定后再做收尾：`go test` 守卫改挂 cargo + parity、从部署包移除 Go 二进制、
   `docs/CONTRACTS.md`/golden 的"实现语言"措辞随迁。
