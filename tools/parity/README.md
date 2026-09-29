# tools/parity — 生成端差分对账（P0）

## 用途

为 Rust 迁移（见 `docs/plan-rust-migration.md`）提供**逐字节差分对账**：

- 把 `config.json` 送进生成器，按 manifest 声明的 `artifacts` 产出多份产物；
- 与 `reference/` 里冻结的基线**逐字节**比对（SHA-256）。

产物与模板的对应（与运行时 `script.GenerateScripts` 一致 —— 它一次生成**两份**产物，
只覆盖其一等于有一半产物无人守）：

| artifact | 命令 | 模板 | 基线文件 |
|---|---|---|---|
| `plan` | `DumpPlan <cfg> <out>` | —（`generators.WritePlan`） | `<name>.plan.json` |
| `ahk` | `GenerateAHK <cfg> keyflux.tmpl <out>` | `config-server/templates/keyflux.tmpl` | `<name>.keyflux.ahk` |
| `skin` | `GenerateAHK <cfg> CommandInputSkin.tmpl <out>` | `config-server/templates/CommandInputSkin.tmpl` | `<name>.skin.txt` |

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
| `manifest.json` | 语料清单（`config` 相对本目录；`artifacts` 选 `plan`/`ahk`/`skin`；可选 `plugins` 指向仓库内插件源，运行时拷贝为工作目录同级 `plugins/`） |
| `corpus/` | **冻结**的输入配置 |
| `reference/` | **冻结**的基线产物（由 `-Capture` 从参考实现录制，入库） |
| `run_parity.ps1` | 脚本本体（**纯 ASCII**，与 `tools/oracle.ps1` 同约定：`pwsh -File` 会误读无 BOM 的 UTF-8 中文） |

## 语料（覆盖了什么）

| item | 来源 | 覆盖面 |
|---|---|---|
| `factory` | 出厂 `data/config.json` 的冻结副本 | 真实样例（键位/缩写/选中动作/窗口组/QuickSwitch…） |
| `factory-plugins` | 同上 + `plugins/examples` 拷为同级 `plugins/` | 插件注入路径（ahk 22766 → 23415 字节证明注入生效） |
| `synthetic` | `config-server/internal/script/golden_test.go` 的 `syntheticConfig()` 导出的 JSON | **覆盖矩阵全集**：9 个 TypeID 各分支、缩写注册表（含 ct5 去单引号）、`hotifHeader` conditionType 0–5、`.KeyMapping` 重映射、windowGroups 单行/多行、QuickSwitch `excludedPrefixes`… |

`synthetic` 的再生成（改了 `syntheticConfig()` 后必须重跑这两步）：

```bash
cd config-server && UPDATE_PARITY_CORPUS=1 go test ./internal/script/ -run TestExportParityCorpus
pwsh tools/parity/run_parity.ps1 -Capture     # 导出后必须重录基线
```

⚠️ **`synthetic` 不是 golden 的输入**：`ParseConfig` 在加载时会把为空的 entry `name`
补成行为目录里的本地化名（`open_url` → `默认浏览器打开网址`），因此 JSON 往返**无法**
与 Go 字面量逐字节等价 ⇒ 不能拿 `testdata/golden.keyflux.ahk` 直接对账。它是一份**不同的、
但同样合法**的输入；价值在于让 Rust 侧也走到全矩阵的每条渲染分支（已用 21 个矩阵锚点核对）。

## 基线依赖的输入（这些改动必须重新 `-Capture`）

| 输入 | 说明 |
|---|---|
| `config-server/templates/keyflux.tmpl` | 模板（manifest 的 `template`） |
| `config-server/templates/CommandInputSkin.tmpl` | 模板（manifest 的 `skinTemplate`） |
| `bin/behaviors/**` | **内置行为包**——`LoadBehaviorCatalog` 从 **`settings.exe` 所在目录**读 `behaviors/`（故 Rust 实现也须落在同目录，否则目录不同→目录内容不同→基线不等） |
| 语料同级 `plugins/` | 由 manifest 的 `plugins` 字段在运行时拷入工作目录 |
| `syntheticConfig()` | 改了它就要重导出 `corpus/synthetic/config.json`（见上） |

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

## 行尾与 git（重要）

`reference/` 与 `corpus/` 在根 `.gitattributes` 里标记为 **`-text`**（原字节存取，禁 EOL 转换）：
本闸门是**字节级**的，而 `core.autocrlf=true` 会在 checkout 时把存储的 LF 变成 CRLF
⇒ 生成的 `plan.json`（LF）与取出的 reference（CRLF）不等，**只有 CI 会红**（本地工作区不重取，不复现）。

- **不要**改成 `text eol=lf`——生成的 `KeyFlux.ahk` 本就是 CRLF，强制 LF 会改坏该产物。
- 新增 reference 文件后若 `git add` 未按新属性重新哈希，需 `git rm --cached` 再 `git add` 强制重存。

## CI

`.github/workflows/analyzers.yml` 的 `parity-gate` 用当前构建的 `settings.exe` 复现基线
（P0 阶段守护「基准不漂移」）。
