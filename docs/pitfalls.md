# KeyFlux 踩坑库（pitfalls）

> **为什么有这份文件**（多维度优化报告 #11）：本项目大量关键运维知识长期只活在
> 代码注释、提交信息与维护者个人记忆里（同一类事故在历史上**翻车多次**）。注释随文件
> 走、提交信息搜得到，但「不知道去哪找」本身就是门槛。本文件把**跨文件、可复现**的
> 事故模式集中到一处，让发现者不必先知道它在哪一节。
>
> **边界**：这里只写「症状 / 根因 / 现在由谁守」三段式，**不复制**门禁实现
> （单一真源见各节「防线」列出的文件）。契约权威在 [CONTRACTS.md](CONTRACTS.md)，
> 变更叙事在 [CHANGELOG.md](CHANGELOG.md)。
>
> **维护约定**：新踩的坑，当天补一条；修好的坑，把「防线」从「人工」升级为
> 「自动化门禁」（这正是本仓库的主线：把隐性知识变成 CI 能替人记住的检查）。

---

## A. 部署「双腿」

后端与面板是**同一份 cargo release 的两个独立产物**，由**不同步骤**落地 —— 历史上
每一次「部署其实没问题」的误判，都是某条腿悄悄变旧或两条腿被写错槽位。

| # | 症状 | 根因 | 防线（自动化） |
|---|---|---|---|
| A1 | 两侧 md5 全一致，却行为异常 | 一个 exe 被拷进了 `settings.exe` 与 `KeyFlux.Settings.exe` 两个槽（「四侧一致」反而是**危险信号**） | `make verify-deploy` → `devtools verify-deploy`（原 `tools/verify_deploy.ps1`）：各腿 `built == staged`，**且两腿彼此不等** |
| A2 | 本地产物无版本号，CI 产物有 | `buildClientReactor` 漏传 `-Version`（版本经 `KEYFLUX_VERSION` 注入 `settings.exe`/面板） | `Makefile` 的 `buildClientReactor` 显式带 `-Version $(version)` |
| A3 | 部署树后端悄悄停在旧构建 | 本地**没有**任何目标生产 `bin/settings.exe`，它只会变旧然后被 `sync-out` 推走 | `buildClientReactor` 末尾复制并打印 md5 |
| A4 | 部署树命令框 exe 被回退 | `sync-out` 的 `'*.exe'` 白名单会用仓库副本覆盖部署树那一份 | `sync-out` 的 `/XF KeyFlux-CommandInput.exe` |
| A5 | `make` 卡死（不是失败） | 目标 exe 被运行中的进程独占打开，robocopy **无限重试** | `Assert-KfEngineStopped`（`tools/lib/kf-tools.ps1`），`sync-out` 前置 |
| A6 | 陈旧产物被打包/上线 | 部署链某步失败却**没有停止**后续步骤（2026-10-01 连翻两次） | `tools/deploy_panel.ps1`：每步硬门禁 + probe-marker 守卫 |

---

## B. 「假绿」判读

本地全绿、CI 红（或反之），且原因与代码无关 —— 全部是**证据链**问题，不是实现问题。

| # | 症状 | 根因 | 防线 |
|---|---|---|---|
| B1 | 在**旧生成器**上 check 通过、发布包带**旧面板** | 二进制没重建；文件「在」所以没人报警 | `make check-freshness` → `devtools check-freshness`（原 `tools/check-freshness.ps1`）：`mtime > HEAD` |
| B2 | 报「body N bytes != baseline M bytes」，按差值排障却对不上 | 打印的是 **base64 字符串长度**，不是字节数（base64 ≈ 4/3 源长） | `tools/api-parity/run_api_parity.ps1`：解码回**真实字节数** + 首个差异偏移 |
| B3 | `GET /api/plugins` 少数内置插件报 MISMATCH | 沙箱缺 `data/plugins`（staging 步骤漏了） | `analyzers.yml` / `release.yml` 的 staging 步骤 |
| B4 | `GET /config` 的 `keyfluxVersion` 与基线不同字节 | settings.exe 未带 `KEYFLUX_VERSION` 构建（`option_env!` 注入） | `api-parity` 的构建口径注释 + `cargo-gates.ps1 -Version` |
| B5 | 二进制被「调包」，旧构建当成新构建 | 构建产物没有溯源标记 | 构建目标打印 md5；`check-freshness` 的 mtime 门禁 |

---

## C. robocopy 陷阱

| # | 症状 | 根因 | 防线 |
|---|---|---|---|
| C1 | 明明有失败却报成功 / 反之 | robocopy **退出码 0–7 都是成功**（位标志），`>=8` 才是失败 | PowerShell 用 `Test-KfRobocopyOk`；sh 内联 `[ $$? -le 7 ]` |
| C2 | `make` 挂死不返回 | 见 A5（锁定文件无限重试） | `Assert-KfEngineStopped` |
| C3 | 排除集在改一处后仍带旧行为 | 同一个 `/XD`+`/XF` 列表曾在 **3 处**各抄一份 | 单一真源 `Get-KfReactorExcludes` / `Invoke-KfReactorStaging`（`kf-tools.ps1`） |

---

## D. 构建 / 工具链 / shell

| # | 症状 | 根因 | 防线 |
|---|---|---|---|
| D1 | make 报与真实原因无关的语法错误（如引号不配对） | 本机 make 是 Windows 原生版，把 recipe 经 **ANSI 代码页**转给 sh；UTF-8 中文被错位解码 | recipe **只写 ASCII**（中文只放注释） |
| D2 | `make check-hooks` 报「`MSYS_NO_PATHCONV` 不是内部或外部命令」 | Windows 用 `cmd` 执行配方，不认 POSIX 的 `VAR=1 cmd` 前缀 | 用 Bash 直跑同款命令（CI 的 sh 不受影响） |
| D3 | pwsh 收到被截断/展开的载荷 | `/bin/sh` 先展开了双引号里的 `$` | 用**单引号**包裹 pwsh 载荷，`$$` 折半后原样送入 |
| D4 | `echo "-> $(OUT_DIR)"` 报 `Is a directory` | 裸写的 `->` 被 sh 当成**重定向** | echo 串加引号 |
| D5 | 某个 `make` 目标静默停摆数天 | 说明行漏了行首 `#`，被 make 当成前置依赖 | `analyzers.yml` 的 `make-parse` job（`make -n check` 只解析） |
| D6 | `pwsh -File` 报奇怪的解析错 | `pwsh -File` 与 Windows PowerShell 5.1 会误解析**非 BOM** UTF-8 | 工具脚本 **ASCII-only** |
| D7 | CI 里 `data/plugins` 落空（`19/23` 假红） | `Copy-Item` 把结尾的 `/.` 解析为「空拷贝」 | CI 用 `bash` 的 `cp -r src/. dst` |
| D8 | 同一源码重编两次 md5 不同，被误判为「部署不一致」 | reactor 两个 bin **非逐字节可复现** | 只在「副本 vs 源」用 md5/sha256（`verify-deploy`），**不**拿「重编相等」当证据 |
| D9 | 本机 `make` 「假绿」：守卫形同虚设 | PATH 上无 `sh` 时 GNU make 退回 **cmd** 执行配方，而配方是 POSIX sh 风格（`test` / `rm` / `cp` / `||` / `; exit 1`）—— `test` 报「不是内部命令」后 `; exit 1` 在 cmd 里不成立，守卫仍 `exit 0`（2026-10-08 实测 `make check-deploy-tree`） | `Makefile` 顶部**解析期** sh 守卫：`$(shell printf ok)` ≠ `ok` 即 `$(error)`（缺 sh 直接失败，而不是等配方走样） |

---

## E. 契约 / 生成端

| # | 症状 | 根因 | 防线 |
|---|---|---|---|
| E1 | `/Validate` 报 `#Include ../data/plugins/... cannot be opened` | 生成产物落点不在部署树 `bin/`（相对 `../data/plugins` 只在那里成立） | `check` 主落点 = 部署树 `bin/KeyFlux.ahk` |
| E2 | oracle 对账缺模块 / 误报 | `oracle.ps1` 曾手抄一份 14 行 include 列表，已与模板漂移（漏 4 个命令框模块） | `Get-KfIncludeList` 从 `templates/keyflux.tmpl` **派生** |
| E3 | 部署时用旧插件覆盖生产树 | `plugins/examples` 与 `data/plugins` 两棵官方插件树静默分叉 | `make check-plugins-mirror`；`make sync-plugin-mirror` 单向同步 |
| E4 | 改一处、另一处仍旧行为 | 同一命令/常量抄多份，必然漂移（gate 复制 N 份尤其危险） | 单一真源：`kf-tools.ps1` / `cargo-gates.ps1` / `CONTRACTS.md` |
| E5 | 文档与实现「各说各话」 | `CONTRACTS.md` §4 长期是**设计期草稿**（`runtime` / `entry` 字符串 / `provides{}` / `settings` map），与实现不符 | 2026-10-02 订正为**实现真源**；§4 顶部有横幅 |
| E6 | 面板连不上后端 | `KEYFLUX_PORT=<n>` 必须是 stdout **第一行**且**无任何装饰输出** | `server::run_headless` + `services::backend::parse_port_line` 同源解析 |
| E7 | 命令框八角 keycap 又冒出来 | 上游命令框的 keycap 数据 patch 被 `sync-out` 的 `'*.exe'` 覆盖冲掉 | 现为自研命令框（不含该白名单）；历史路径见 `patch-commandinput` 注释 |

---

## F. 闸门覆盖 / 口径缺口（2026-10-09）

| # | 症状 | 根因 | 防线 |
|---|---|---|---|
| F1 | ~5800 行的 `command-input`（装着全部 Win32 `unsafe`）从未过 fmt/clippy，却产出正式发布的 `bin/KeyFlux-CommandInput.exe` | `cargo-gates.ps1` 硬编码 `config-ui-reactor`，CI 只调它一次 | `cargo-gates.ps1 -Project <dir>` 参数化；新增 `make check-command-input`（并入 `check` 前置、`command-input` 构建前置）；`make analyzers` 与 CI `reactor-gates` 都跑**两个** crate；`Cargo.toml` 加 `[lints.clippy] all + too_many_lines`（与 config-ui-reactor 同口径，不设 `unsafe_code`）——顺带把 `wndproc`/`draw` 拆到 <100 行、全库 66 处 unsafe 补 `// SAFETY:` |
| F2 | 守卫"声称"覆盖 `bin/*.ahk` 与 `plugins/`，实际只扫 `bin/lib`（`.gitattributes` 为那几类声明了 `eol=lf` 却无人校验） | `lint_ahk_style.py` 的扫描根只指向 `bin/lib` | 文本形态检查（bom/crlf/tab/spelling）扩到与 `.gitattributes` **逐一对应**的四类 scope；静默失败面仍限引擎核心 `bin/lib`；`MIN_EXPECTED_FILES` 提到 65 |
| F3 | 基线 `spelling_drift_hits: 7` 与实际 `0` 漂移（批 O 修好后没重录） | 压债后未重录基线 | 重录基线（`--write-baseline`）⇒ 现值 0，回退到 7 即红灯 |
| F4 | 换机器 / 换 clone 位置后 `OUT_DIR` 失效 | `OUT_DIR ?= D:/PortableApps/KeyFlux-compiled` 是绝对路径 | 默认改**相对** `../KeyFlux-compiled`（与仓库同级，本机同值） |
| F5 | 注释称"固定 channel 保证 CI 一致"，值却是 `stable` | 注释与取值矛盾 | 按实情订正注释（channel = stable）；锁版需本机预装该工具链 + 同步 CI —— 本机 `toolchains/` 只有 stable，硬锁会联网拉取而失败（离线实测） |
| F6 | `make upload` 发不出去 | `createRelease` POST 到 `xianyukang/KeyFlux`，而 remote 是 `Hermuc/KeyFlux` | 仓库名改为 `Hermuc/KeyFlux` |

---

## 速查：门禁 → 坑位

| 门禁 | 主要守护 |
|---|---|
| `make verify-deploy` | A1、A3 |
| `make check-freshness`（`make check` 前置） | B1、B5 |
| `make check-vendor` | 第三方随包内容「勿改」（好人误修正上游） |
| `make check-upstream`（`deps-watch.yml` 每周） | 上游 `windows-reactor` 新版（#7） |
| `make check-deps`（`deps-watch.yml` 每周） | 依赖漏洞 / 许可 / 来源（#8） |
| `make api-parity` | B2、B4、E6 |
| `make check-plugins-mirror` | E3 |
| `make -n check`（CI `make-parse`） | D5 |
| `make check-hooks` / `check-fuzzy` | 命令框 provider 分发 / 容错匹配（运行时契约） |
| `make check-command-input`（`check` 前置；`make analyzers` 双工程） | F1 |
| `make lint-ahk-style`（`devtools lint-ahk-style`，原 `tools/lint_ahk_style.py`，2026-10-08 移植为 Rust） | F2、F3 |
| `tools/deploy_panel.ps1` 内建门禁 | A5、A6 |
| `tools/lib/kf-tools.ps1` | C1、C3、E4（单一真源本身） |