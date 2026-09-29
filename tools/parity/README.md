# tools/parity — 生成端差分对账（P0）

## 用途

为 Rust 迁移（见 `docs/plan-rust-migration.md`）提供**逐字节差分对账**：

- 把 `config.json` 送进生成器，产出 `plan.json`（`DumpPlan`）与 `KeyFlux.ahk`（`GenerateAHK`）；
- 与 `reference/` 里冻结的基线**逐字节**比对（SHA-256）。

它的作用是：把"两份实现"从**无对账的第二真源**变成**受守护的对照实现**——Rust 生成器接入后，
同一脚本即成为 Go vs Rust 的等价性闸门（**对账不通过不许切换**）。

## 用法

```bash
make parity                                      # 用 bin/settings.exe 复现基线
pwsh -File tools/parity/run_parity.ps1 -Exe <path>   # 换实现（如 Rust settings.exe）
pwsh -File tools/parity/run_parity.ps1 -Capture      # 重新录制基线
```

- 输出末行为 ASCII：`PARITY: n/n PASS [CHECK|CAPTURE]`；失败时附 `  - <item> : MISMATCH [plan,ahk]`。
- 退出码：0 = 全过，1 = 有任何不等（可直接作 CI 闸门）。

## 目录

| 路径 | 说明 |
|---|---|
| `manifest.json` | 语料清单（`config` 相对本目录；可选 `plugins` 指向仓库内插件源，运行时拷贝为工作目录同级 `plugins/`） |
| `corpus/` | **冻结**的输入配置 |
| `reference/` | **冻结**的基线产物（由 `-Capture` 从参考实现录制，入库） |
| `run_parity.ps1` | 脚本本体（**纯 ASCII**，与 `tools/oracle.ps1` 同约定：`pwsh -File` 会误读无 BOM 的 UTF-8 中文） |

## 基线依赖的输入（这些改动必须重新 `-Capture`）

| 输入 | 说明 |
|---|---|
| `config-server/templates/keyflux.tmpl` | 模板（manifest 的 `template`） |
| `bin/behaviors/**` | **内置行为包**——`LoadBehaviorCatalog` 从 **`settings.exe` 所在目录**读 `behaviors/`（故 Rust 实现也须落在同目录，否则目录不同→目录内容不同→基线不等） |
| 语料同级 `plugins/` | 由 manifest 的 `plugins` 字段在运行时拷入工作目录 |

## 语料约束（重要）

Go 渲染路径存在**已知 map 迭代非确定性**（见 `config-server/internal/script/golden_test.go`
的「确定性约束」段）：同一 keymap 内 `(TypeID, hotkey)` 并列、或 ID==1 里 `WindowGroupID` 重复时，
输出顺序取决于 map 随机序。

因此：

1. 语料配置必须**规避**上述并列（否则基线会 flake）；
2. `-Capture` 会**跑两遍**，产物不一致即拒绝录制并报 `NONDETERMINISTIC`——把 flake 挡在录制环节。

## 什么时候要 `-Capture`

仅当**有意的**生成规则变更（改了生成器/模板并接受新行为）时重新录制；录制后基线随之更新。
若是无意变更 ⇒ 说明生成行为被意外改动，应修代码而不是重录基线（与 golden 的处理方式一致）。

## CI

`.github/workflows/analyzers.yml` 的 `parity-gate` 用当前构建的 `settings.exe` 复现基线
（P0 阶段守护「基准不漂移」）。
