# tools/api-parity — API 级差分对账基线

为 Rust 版面板后端的**逐字节验收**冻结 Go `settings.exe` 的 HTTP 响应基线。路由表面由
基线录制时由 `config-server/internal/server/bridge_test.go:37-72` 钉死（19 条；该文件随 Go 后端于 2026-10-06 退役，路由面契约现由本工具的冻结基线 + `config-ui-reactor` 的 cargo 单测承载）。本工具按**固定顺序**
采集 23 步（只读端点 + 副作用端点的完整 CRUD 场景），基线内容确定性由双跑门禁保证。

## 工具说明

`run_api_parity.ps1`（PowerShell 7 语法，保持 PS 5.1 可解析；脚本本体 ASCII-only，
避免 `pwsh -File` 误读无 BOM 的 UTF-8，与 `tools/parity/run_parity.ps1` 同规）。

两种模式：

| 模式 | 命令 | 行为 |
|---|---|---|
| `-Capture` | `pwsh -File tools/api-parity/run_api_parity.ps1 -Capture [-Exe <path>]` | 在 `%TEMP%` 建**两个独立沙箱**各按固定顺序跑完 23 步，两遍逐字节一致才写 reference（防非确定输出）；不一致则 FAIL 且不落盘 |
| `-Check` | `pwsh -File tools/api-parity/run_api_parity.ps1 -Check -Exe <path>` | 建**一个**沙箱按同序重放，与基线逐字节比对。候选 exe 未实现的端点 → `MISSING_ENDPOINT`；其后依赖沙箱状态的步骤 → `SKIPPED_STATE`（两者都**不算工具失败**，exit 0）；字节不一致 → `MISMATCH`（exit 1） |

`-Exe` 缺省为仓库 `bin/settings.exe`（Go）。末行输出 ASCII 摘要
`API-PARITY: <pass>/<total> PASS [MODE]`。

### 沙箱部署树（每次运行独立，位于 `%TEMP%\kfapiparity-<guid>`，用后删除）

```
<sandbox>/bin/settings.exe      # 被测 exe（Capture 从 -Exe 复制；Check 同）
<sandbox>/bin/behaviors/        # 内置行为包（来自仓库 bin/behaviors/）
<sandbox>/bin/templates/        # 模板（来自仓库 bin/templates/）
<sandbox>/data/config.json      # 出厂样例（来自仓库 data/config.json）
<sandbox>/data/plugins/         # 出厂插件（everything_search）
<sandbox>/KeyFlux.exe           # 空操作 stub（运行期用 .NET Framework csc 现编；见下）
```

**cwd 恒为 `<sandbox>/bin/`**（Go 侧依赖相对 `../data`、`./behaviors`、`./templates`，
（Go 传输入口随 2026-10-06 Go 后端退役；Rust 端为 `config-ui-reactor/src/server/bridge.rs`）。传输统一走 `Call` 子命令
（进程内 gin，不开 socket，免端口管理）：`Call <METHOD> <PATH> <out-file> [--body f]
[--content-type ct]`，stdout 末行契约 `KEYFLUX_CALL status=<n>`，响应原始字节写
`<out-file>`。`%TEMP%` 路径含空格也可跑（`ProcessStartInfo` 逐参数加引号，不经 shell）。

**stub KeyFlux.exe 的作用**：`PUT /config` 与 `POST /api/behaviors/apply` 会
`proc.ExecCmd("./KeyFlux.exe")` 拉起引擎；沙箱无引擎时 Go 回退 `explorer.exe` 中转
（`proc.FallbackExecCmd`）——返回值相同（`restartFailed=false`）但会向真实桌面拉起
一个指向即将删除的沙箱路径的 explorer（可能弹错误对话框）。放一个立即退出的空
stub exe 让 `ExecCmd` 走 breakaway 快速路径，**响应字节两种情形完全一致**，
只是避免 QA 机器桌面噪音；csc 不可用时自动跳过 stub，不影响基线字节。

### 基线产物

- `reference/go/<METHOD>_<path转义>[.<后缀>][.<corpus词干>].json` — 每项含
  `step` / `method` / `path` / `corpus` / `callLine`（`KEYFLUX_CALL status=` 行，
  已做标准 JSON 转义）/ `status` / `bodyBase64`（响应原始字节）。手写 JSON 序列化
  （不经 ConvertTo-Json），PS 5.1 / 7 再生成时字节稳定；UTF-8 无 BOM、LF。
  同一 (METHOD, PATH) 在不同状态点的多次采集用 `suffix` 区分
  （如 `GET_api_plugins.json` 与 `GET_api_plugins.after-import.json`）。
- `reference/manifest.json`（schema v2）— **有序**步骤索引（`step` 序号、`stateful`
  标志、`bodyFrom`、`multipart`、源 exe SHA-256；内容确定性，无时间戳）。
  -Check 按同序重放：副作用步骤依赖前序状态（如 DELETE 依赖 import 已建插件），
  故任一 stateful 步骤 MISSING 后，后续 stateful 步骤跳过而非误报 MISMATCH。
- `corpus/*.json` — 请求体样本（UTF-8 无 BOM；**不能带 BOM**，Go 的 JSON 解码
  会因 BOM 报错）。`demo_plugin.manifest.json` 在运行期被打成最小插件 zip
  （plugin.json + main.ahk）再经 multipart 字段 `file` 上传。

## 采集步骤（23 步，固定顺序）

只读（1-9）：

| step | 请求 | corpus | 基线要点 |
|---|---|---|---|
| 1 | `GET /health` | — | 200，`ok` |
| 2 | `GET /config` | — | 200，29184 字节，见下节 |
| 3 | `GET /shortcuts` | — | 200，**`null`**（4 字节） |
| 4 | `GET /api/behaviors` | — | 200，`{"builtin":[…],"user":[],"errors":[]}` |
| 5 | `GET /api/plugins` | — | 200，仅 everything_search |
| 6 | `GET /api/plugins/everything_search/settings` | — | 200，声明 + 默认值合并 |
| 7 | `POST /api/selected-action/test` | url 命中样本 | 200，`matched:true` |
| 8 | `POST /api/selected-action/test` | 自定义 textType `type:` 命中样本 | 200，`matched:true` |
| 9 | `POST /api/selected-action/test` | 不命中样本 | 200，`{"matched":false}` |

副作用场景（10-23，顺序即状态依赖；沙箱每轮重建，Go 实际执行并录响应）：

| step | 请求 | corpus | Go 实际行为（已实录） |
|---|---|---|---|
| 10 | `PUT /config` | body = GET /config 基线字节做**确定微改**（首个 `"comment":"label:36"` → `"label:36|parity"`，原文手术式替换，非 JSON 重序列化） | 校验通过 → 覆盖写 `../data/config.json`；沙箱 stub 引擎启动成功 → 200 `{"message":"ok","restartFailed":false}` |
| 11-13 | `POST /server/command/2|3|4` | — | 白名单分发（WindowSpy / 开机自启 On/Off）；stub 引擎吞掉参数即退 → 恒 200 `{}` |
| 14 | `POST /api/behaviors` | `behavior_pack_demo.json`（plain 包，builtin copy entry） | 校验通过 → 写 `../data/behaviors/parity_demo/` → 200 回显包 JSON（含 `source:"user"`） |
| 15 | `PUT /api/behaviors/parity_demo` | `behavior_pack_demo_update.json`（改 name/version） | 覆写包 → 200 回显修改后包 JSON |
| 16 | `POST /api/behaviors/apply` | — | 重启引擎（stub 成功）→ 200 `{"restartFailed":false}` |
| 17 | `DELETE /api/behaviors/parity_demo` | — | 无引用校验通过 → 删目录 → 200 `{"message":"ok"}` |
| 18 | `POST /api/plugins/import` | `demo_plugin.manifest.json` → 运行期打 zip → multipart `file` | 解压 + manifest 校验 + 按 id 原子落盘 → 200 回显 manifest |
| 19 | `GET /api/plugins` | — | 200，列表含 demo_plugin（ID 字典序） |
| 20 | `GET /api/plugins/demo_plugin/settings` | — | 200，声明 + 默认值（`values` 全默认） |
| 21 | `PUT /api/plugins/demo_plugin/settings` | `plugin_settings_put.json`（`{"values":{"demoPath":"D:/demo/target.exe"}}`） | 键校验 + 值校验 → 写 `../data/plugin-settings.json` → 200 回显合并后 DTO |
| 22 | `GET /api/plugins/demo_plugin/settings` | — | 200，`values.demoPath` 为已存值（合并生效） |
| 23 | `DELETE /api/plugins/demo_plugin` | — | 删 `data/plugins/demo_plugin/`（**不**清 plugin-settings.json）→ 200 `{"message":"ok"}` |

注 1：`restartFailed=false` 的由来——`proc.ExecCmd` 两级启动（breakaway 直启 /
explorer 中转）只要进程能 `Start` 即返回 true；沙箱里 stub（或无 stub 时的 explorer
中转）都能 Start。真实部署树中 KeyFlux.exe 存在，同样是 false。
注 2：`PUT /config` 的回显体含 `"startup":true`（schtasks 回填，见「Go 行为备忘」#1），
会随保存写进沙箱 config.json——只影响沙箱内状态，不影响响应字节。

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
- 基线文件必须是**合法 strict JSON**：`callLine` 等字符串字段内的控制字符已做
  `\u00XX` 转义（历史上曾内嵌字面换行导致 python `json.loads` /
  `ConvertFrom-Json` 均失败，已修复；`json.loads`（strict）可作验收口径）。
- 响应字节含中文时为**原始 UTF-8 多字节序列**（gin 只转义 HTML 敏感字符，不转中文）。

## GET /config 基线要点

- **夹具模式（`KEYFLUX_API_PARITY=1`）**：harness 全程设置该环境变量（本脚本顶部注入，
  子进程继承），Go `startupFixture` / Rust `query_startup_from_task` 据此跳过
  `schtasks` 真实查询、`options.startup` 恒为 `false`。否则计划任务存在性这一
  **机器态**会冻进基线 —— 实测本机（有任务）`"startup":true` 29184 字节，CI（无任务）
  `"startup":false` 29185 字节，恰差 1 字节 ⇒ step 2 在任何其他机器必挂。
  **重录基线也必须带此模式**（-Capture 同样注入）。
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
6. **`PUT /config` 会真实落盘并拉起引擎**（`SaveConfigFile` + `proc.ExecCmd`），
   外观（commandFont/commandInputSkin）未变时不杀 `KeyFlux-CommandInput.exe`——
   corpus 的 config-echo 体保持外观原样，避免 QA 机器上真实的命令框进程被 taskkill。
7. **插件导入的校验口径**（`plugins.ValidateManifest`）：ID `^[a-z][a-z0-9_]{0,31}$`、
   `specVersion=1`、entry 仅 `script`（file+func 必填）、声明 settings 必须申请
   `settings` 权限位；zip 根或唯一顶层目录含 plugin.json 均可；同名 ID 重复导入拒绝
   （「已存在」）。删除插件**刻意不清** plugin-settings.json（卸载重装保留用户值）。
8. **行为包/插件 DTO 回显**：POST/PUT 成功响应是对解析后结构体的直接 JSON 编码
   （字段序 = struct 定义序，非请求原文），缺省 `omitempty` 字段不出现在响应中
   （如 demo 插件 settings 项无 `default` 键）。

## 文件清单

```
tools/api-parity/
├── .gitattributes                  # reference/** 与 corpus/** 标 -text
├── README.md                       # 本文件
├── run_api_parity.ps1              # Capture / Check 双模式脚本（ASCII-only）
├── corpus/
│   ├── test_selected-action_url_hit.json
│   ├── test_selected-action_textfeature_hit.json
│   ├── test_selected-action_nomatch.json
│   ├── behavior_pack_demo.json             # POST /api/behaviors 创建样本
│   ├── behavior_pack_demo_update.json      # PUT 修改样本
│   ├── demo_plugin.manifest.json           # 运行期打 zip → multipart 导入
│   └── plugin_settings_put.json            # PUT /api/plugins/:id/settings 样本
└── reference/
    ├── manifest.json               # schema v2，23 步有序索引
    └── go/*.json                   # 23 条基线（status + bodyBase64 + callLine）
```
