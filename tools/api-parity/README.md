# tools/api-parity — API 级差分对账基线

为 Rust 版面板后端的**逐字节验收**冻结 Go `settings.exe` 的 HTTP 响应基线。路由表面由
`config-server/internal/server/bridge_test.go:37-72` 钉死（19 条）；本工具只覆盖**只读端点**，
副作用端点本阶段仅记录请求样例与 Go 行为描述（见下文）。

## 工具说明

`run_api_parity.ps1`（PowerShell 7 语法，保持 PS 5.1 可解析；脚本本体 ASCII-only，
避免 `pwsh -File` 误读无 BOM 的 UTF-8，与 `tools/parity/run_parity.ps1` 同规）。

两种模式：

| 模式 | 命令 | 行为 |
|---|---|---|
| `-Capture` | `pwsh -File tools/api-parity/run_api_parity.ps1 -Capture [-Exe <path>]` | 在 `%TEMP%` 建**两个独立沙箱**各跑一遍，两遍逐字节一致才写 reference（防非确定输出）；不一致则 FAIL 且不落盘 |
| `-Check` | `pwsh -File tools/api-parity/run_api_parity.ps1 -Check -Exe <path>` | 建**一个**沙箱，重放同一批请求，与基线逐字节比对。候选 exe 未实现的端点 → `MISSING_ENDPOINT`（计入清单但**不算工具失败**，exit 0）；字节不一致 → `MISMATCH`（exit 1） |

`-Exe` 缺省为仓库 `bin/settings.exe`（Go）。末行输出 ASCII 摘要
`API-PARITY: <pass>/<total> PASS [MODE]`。

### 沙箱部署树（每次运行独立，位于 `%TEMP%\kfapiparity-<guid>`，用后删除）

```
<sandbox>/bin/settings.exe      # 被测 exe（Capture 从 -Exe 复制；Check 同）
<sandbox>/bin/behaviors/        # 内置行为包（来自仓库 bin/behaviors/）
<sandbox>/bin/templates/        # 模板（来自仓库 bin/templates/）
<sandbox>/data/config.json      # 出厂样例（来自仓库 data/config.json）
<sandbox>/data/plugins/         # 出厂插件（everything_search）
```

**cwd 恒为 `<sandbox>/bin/`**（Go 侧依赖相对 `../data`、`./behaviors`、`./templates`，
见 `config-server/internal/server/bridge.go:16-17`）。传输统一走 `Call` 子命令
（进程内 gin，不开 socket，免端口管理）：`Call <METHOD> <PATH> <out-file> [--body f]`，
stdout 末行契约 `KEYFLUX_CALL status=<n>`，响应原始字节写 `<out-file>`。
`%TEMP%` 路径含空格也可跑（`ProcessStartInfo` 逐参数加引号，不经过 shell）。

### 基线产物

- `reference/go/<METHOD>_<path转义>.json` — 每项含 `method` / `path` / `corpus` /
  `callLine`（`KEYFLUX_CALL status=` 行原文）/ `status` / `bodyBase64`（响应原始字节）。
  手写 JSON 序列化（不经 ConvertTo-Json），PS 5.1 / 7 再生成时字节稳定；UTF-8 无 BOM、LF。
- `reference/manifest.json` — 采集索引（含源 exe 的 SHA-256；内容确定性，无时间戳）。
- `corpus/*.json` — POST 请求体样本（UTF-8 无 BOM；**不能带 BOM**，Go 的 JSON 解码
  会因 BOM 报错）。

## 采集范围（9 项只读）

| # | 请求 | corpus | 基线要点 |
|---|---|---|---|
| 1 | `GET /health` | — | 200，`ok`（2 字节） |
| 2 | `GET /config` | — | 200，29184 字节，见下节 |
| 3 | `GET /shortcuts` | — | 200，**`null`**（4 字节） |
| 4 | `GET /api/behaviors` | — | 200，`{"builtin":[…],"user":[],"errors":[]}` |
| 5 | `GET /api/plugins` | — | 200，`{"plugins":[…],"errors":[]}` |
| 6 | `GET /api/plugins/everything_search/settings` | — | 200，声明式 schema + 默认值合并 |
| 7 | `POST /api/selected-action/test` | `test_selected-action_url_hit.json` | 200，命中 textType url |
| 8 | `POST /api/selected-action/test` | `test_selected-action_textfeature_hit.json` | 200，命中自定义 type: 特征（plain 继承覆盖） |
| 9 | `POST /api/selected-action/test` | `test_selected-action_nomatch.json` | 200，`{"matched":false}` |

## 如何复现

```powershell
# 1) 重新采集（覆盖 reference/，内部双跑确定性门禁）
pwsh -File tools/api-parity/run_api_parity.ps1 -Capture
# 2) Go 自比对（应 100% PASS）
pwsh -File tools/api-parity/run_api_parity.ps1 -Check
# 3) Rust 就绪后
pwsh -File tools/api-parity/run_api_parity.ps1 -Check -Exe <rust>/bin/settings.exe
```

## 行尾 / 编码注意

- `tools/api-parity/.gitattributes` 将 `reference/**` 与 `corpus/**` 标 `-text`
  （参照根 `.gitattributes` 对 `tools/parity` 的做法）：仓库 `core.autocrlf=true`，
  不加该行则 checkout 会把 LF 换成 CRLF，导致逐字节比对假红。
- 基线与 corpus 均为 UTF-8 无 BOM；corpus 是请求体原样字节，加 BOM 会让 Go 的
  `ShouldBindJSON` 失败（500）。
- 响应字节含中文时为**原始 UTF-8 多字节序列**（gin 只转义 HTML 敏感字符，不转中文）。

## GET /config 基线要点

- 顶层 JSON 键（gin.H / DTO map 键按字典序输出）：
  `fileGroups`、`keymaps`、`matchTypes`、`options`、`selectedAction`。
- 出厂样例 `data/config.json` 顶层只有 `keymaps/options/selectedAction`，但 DTO
  读取时补齐缺段 —— **空集合形态是 `[]`**：
  `"fileGroups":[],"matchTypes":[]`、`"selectedAction":{"hotkey":"","enable":false,"mappings":[]}`。
  （对照：`/shortcuts` 无目录时是 **`null`** —— Go nil slice 的 JSON 形态，与 `[]` 不同，
  Rust 侧必须区分。）

## HTML 转义证据

gin `c.JSON` 默认 HTML 转义（`<`→`\u003c`、`>`→`\u003e`、`&`→`\u0026`），中文不转义：

- `GET /config` 中 `"comment":"符号 \u003c"`（原文 `符号 <`，`符号` 保持 UTF-8 原文）。
- `POST /api/selected-action/test`（url 样本，content 含 `<a>&tag=x`）：
  `"preview":"用默认浏览器打开: https://example.com/搜索?k=KeyFlux\u003ca\u003e\u0026tag=x"`。

## Go 行为备忘（如实记录，不改 Go）

1. **`options.startup` 是环境依赖字段**：`GET /config` 会 spawn
   `schtasks /query /tn KeyFlux` 回填开机自启显示态（`handlers.go:19-29`）。
   本机存在 KeyFlux 计划任务 → 基线中 `"startup":true`。若采集/验收机器上任务
   状态不同，该字节会不同 —— 对账时需保证两台机器任务状态一致，或单独豁免该字段。
2. **`/shortcuts` 无目录时返回 `null` 而非 `[]`**：`GetShortcutsHandler` 的
   `var data []shortcut` 在零命中时保持 nil，`c.JSON` 序列化为 `null`
   （`handlers.go:93-115`）。路径解析自 `os.Executable()` 的祖父目录，故必须在
   沙箱内部署 exe 采集（工具已强制，勿直接跑仓库 exe，否则会读到仓库 `shortcuts/`）。
3. **自定义 textType 的 plain 继承覆盖**：映射值 `type:<id>` 由 plain 行为包
   （copy/search/run/script/send_keys）自动覆盖（两段式覆盖，`behaviors.go:279-286`），
   corpus 样本 8 依赖此行为。
4. **`Call` 的 stdout 契约行**：`KEYFLUX_CALL status=<n>` 为 stdout **末行**，
   与 out-file 分离；进程退出码 0 表示调用本身成功，业务状态看契约行。
5. **非 debug 模式无 CORS**：`NewRouter(nil, nil, false)`，`Call` 与生产 HTTP
   路径同 handler 链；响应不含 `Access-Control-*` 头（本工具未采集响应头，如需
   头级对账须另行扩展 recorder 输出）。

## 副作用端点（本阶段只记录，不执行）

需引擎 / 文件系统副作用，Rust 就绪后再加执行；执行前须为每类准备状态复位
（沙箱重建即可复位，但会拖慢 -Check，故暂缓）。

| 端点 | 请求样例 | Go 行为描述 |
|---|---|---|
| `PUT /config` | body = `ConfigDTO` 全量 JSON（Content-Type `application/json`） | 校验 selectedAction/fileGroups 组合合法性，非法 → 400 `{"message":"保存失败: …"}`；合法 → 覆盖写 `../data/config.json`、失效 startup 缓存、外观变更时杀 `KeyFlux-CommandInput.exe`、`proc.ExecCmd("./KeyFlux.exe")` 重启引擎 → 200 `{"message":"ok","restartFailed":<bool>}`（`handlers.go:142-191`）。**副作用：落盘 + 拉起引擎进程** |
| `POST /server/command/:id` | 路径参数 id ∈ {2,3,4}（白名单） | id=2 拉起 `./KeyFlux.exe /script bin/WindowSpy.ahk`；id=3/4 切换开机自启（MiscTools.ahk RunAtStartup On/Off）；非白名单 id 静默无操作。恒 200 `{}`（`handlers.go:117-140`）。**副作用：执行 AHK 脚本** |
| `POST /api/behaviors` | body = 行为包 JSON（`entry.kind` 不得为 `script`） | 写用户包到 `../data/behaviors/<id>/`；script entry → 400；非法/重复 → 400 `{"message":…}`（`behaviors.go:88-101`）。**副作用：建目录写文件** |
| `POST /api/behaviors/apply` | 无 body | 显式重启引擎使行为包变更生效。**副作用：进程操作** |
| `PUT /api/behaviors/:id` / `DELETE /api/behaviors/:id` | body = 行为包 JSON / 无 | 更新 / 删除用户包（删除有 RuleRef 引用校验）。**副作用：写/删目录** |
| `POST /api/plugins/import` | multipart 字段 `file`（插件 zip） | `InstallFromZip` 安装到 `data/plugins/<id>/`，成功返回 manifest；缺文件/非法 → 400（`plugins.go:60-79`）。**副作用：解包写盘** |
| `DELETE /api/plugins/:id` | 无 | 删 `data/plugins/<id>/`（内置 ID/非法 ID/不存在均 400），**不**清理 plugin-settings.json（`plugins.go:87-94`）。**副作用：删目录** |
| `PUT /api/plugins/:id/settings` | `{"values":{"key":"…"}}` | 合并写 `../data/plugin-settings.json`，引擎侧 hot reload，无重启。**副作用：写盘** |

## 文件清单

```
tools/api-parity/
├── .gitattributes                  # reference/** 与 corpus/** 标 -text
├── README.md                       # 本文件
├── run_api_parity.ps1              # Capture / Check 双模式脚本（ASCII-only）
├── corpus/
│   ├── test_selected-action_url_hit.json
│   ├── test_selected-action_textfeature_hit.json
│   └── test_selected-action_nomatch.json
└── reference/
    ├── manifest.json
    └── go/*.json                   # 9 条基线（status + bodyBase64 + callLine）
```
