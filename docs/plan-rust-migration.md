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
| **P0 护栏** | `tools/parity/` 差分 harness：Go=reference，固定语料（golden + 真实 config + 插件变体 + 异常输入）两版产物**逐字节**比对；`Makefile` 加 `parity`；CI 加一步 | harness 能抓假等价（先造一处"故意不等"验证会红） | 纯新增 |
| **P1 面板去 HTTP** | `LocalCliApi` 直读 `config.json`；保存/辅助走 **Go CLI**；`--api=http\|local` 开关 | 直读 == `GET /config`；175 测试 + parity 全绿 | 切回 HTTP 适配器 |
| **P2 Rust 接管外围** | config 解析/默认值、shortcuts、plugins、behaviors、指南（文件与模型层） | 与 Go 输出逐字段对账相等 | 同上 |
| **P3 Rust 重写生成器 + drop-in** | 生成器 **与校验**（同源）迁 Rust；产出同名 `settings.exe`；Go 退为 reference（不进部署包） | **全语料 parity 100%** + golden + Oracle diff | 换回 Go 二进制（开关） |
| **P4 Go 退役** | 消费方全指向 Rust；golden/CONTRACTS/Oracle 随迁；CI 由 `go test` → cargo + parity | 三闸门 + parity 全绿 | git 历史 |

**顺序理由**：先去掉「连接」（P1），再换掉「实现」（P3）——两类风险分开，出问题能立刻归类。

## 5. 迁移特有的隐藏 bug（"代码安全"的考点）

| # | 隐藏 bug | 对策 |
|---|---|---|
| 1 | **BOM 丢失**：Rust 写文件默认不写 BOM，而 `keyflux.tmpl` 带 BOM 是铁律 | 写 `KeyFlux.ahk` **显式写 BOM**；parity 会抓到 |
| 2 | **Go `json.Marshal` 默认 HTML 转义**（`<`/`>`/`&` → `\u003c`），serde_json 不转 | parity 逐字节比对暴露；按需对齐编码 |
| 3 | 键序 / 字段序：Go 结构体序 vs serde 声明序 / `BTreeMap` 排序 | 显式固定序列化顺序 |
| 4 | 数字格式化：Go `%v` vs Rust `Display`（浮点尾数、科学计数） | parity 覆盖超长/浮点样本 |
| 5 | 行尾 CRLF vs LF | 统一按 Go 现状，parity 校验 |
| 6 | **CLI 中文错误乱码**（输出经管道走控制台码页） | CLI 输出强制 UTF-8；结果行走 ASCII 标记 |
| 7 | **保存失败被静默吞**（`proc.ExecCmd` 是 fire-and-forget，不查退出码） | CLI 适配器**必等退出码 + 解析 `KEYFLUX_SAVE`** |
| 8 | **引擎不重生成**：`Launcher.ahk` 的 `NeedsRegenerate` 用 **mtime** 判定 | 保存后保证 mtime 更新，或显式触发重生成 |
| 9 | 保存副作用遗漏：缓存失效 / 外观变更 kill CommandInput / `restartFailed` 提示 | CLI 子命令逐条等价，用例 + parity 覆盖 |
| 10 | `options.startup` 直读失真（现靠后端查 `schtasks` 回填） | 面板自查计划任务 |
| 11 | 并发写 / 半写文件（面板被杀留 `*.tmp`） | `*.tmp` + 原子 rename；读侧容忍半写 |

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
- [ ] 建 `tools/parity/run_parity.ps1`：跑两版 → 逐字节 diff → 输出 `PARITY: n/n PASS`（纯 ASCII 供 CI）。
- [ ] 语料：复用 `config-server/internal/script/testdata/` + `data/config.json` + 构造的插件/异常样本。
- [ ] `Makefile` 加 `parity` 目标；`.github/workflows/analyzers.yml` 加一步。
- [ ] **反证**：临时制造一处不等，确认 harness 变红（证明不是恒真断言）。

## 8. 附：不推荐的路径（备忘）

- **直接删除 Go 并让 Rust 重写**（无对账）：正是「第二真源」的制造方式——两份无对账实现必然漂移，症状是"设置改了引擎不生效"这类**静默 bug**。
- **只删原真源**：不解决分叉，只是换了幸存者；若忘记迁移消费方（`Launcher.ahk` 等）会直接破坏核心功能。
- 正确姿态：**收敛到一份**，而非"删掉其中一份"。本方案即"先对账、再替换、后淘汰"。
