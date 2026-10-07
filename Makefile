version = 1.0-beta1
ahkVersion = 2.0.19
folder = KeyFlux-$(version)
zip = $(folder).7z

# sync-templates: 模板资产 → bin/templates (运行时引擎读: Launcher.ahk 的
# NeedsRegenerate 判定 + MiscTools.ahk 的计划任务模板)。模板真源 = 仓库根 templates/
# (2026-10-06 自 config-server/templates 迁出, Go 后端同日退役)。
# settings.exe 本体由 cargo 直接产出 (config-ui-reactor, bin 目标 settings),
# 版本注入走 KEYFLUX_VERSION 环境变量 (见 tools/cargo-gates.ps1 -Version)。
sync-templates:
	rm -f -r bin/templates
	cp -r templates bin/templates

# 发布 Rust/WinUI3 原生设置界面 (config-ui-reactor) 到 bin/ui/。
# 三道闸门 (迁移知识库 12 号): fmt --check / clippy -D warnings / test, 全绿才允许出包。
# ⚠️ 单一真源: 闸门序列只在 tools/cargo-gates.ps1 定义一处 (analyzers / release.yml /
#   两个 GitHub workflow 全部调用它), 勿在本文件再抄一份。
# 自包含: build.rs 的 as_self_contained() 把 pinned Windows App Runtime stage 进 target/release
#   (exe + 28 DLL + 4 PRI + ~87 语言资源目录), 排除 cargo 中间产物后整体拷入 bin/ui
#   ⇒ 实测 207 文件 / 65MB。
# ⚠️ 产物必须改名 KeyFlux.Settings.exe: 引擎按该文件名拉起面板
#   (bin/lib/core/Functions.ahk: Run('...ui\KeyFlux.Settings.exe') / ProcessClose 同名)。
# i18n 用 include_str! 编译期内嵌 (唯一真源 resources/i18n.json), 故无「散资源」拷贝;
#   键数与语义一致性由 i18n 单测闸门守护 (cargo test)。
# robocopy 退出码 0-7 均为成功 (与 deploy 目标同一约定)。
buildClientReactor:
	rm -f -r bin/ui
	mkdir bin/ui
	@pwsh -NoProfile -ExecutionPolicy Bypass -File tools/cargo-gates.ps1 -Release -EnvScript config-ui-reactor/env.ps1
	@pwsh -NoProfile -Command '$$src=(Resolve-Path "config-ui-reactor/target/release").Path; $$dst="bin/ui"; robocopy $$src $$dst /E /XD .fingerprint build deps examples incremental /XF *.pdb *.d *.rlib *.rmeta *.cargo-lock *.cargo-build-lock *.cargo-artifact-lock keyflux-settings.exe settings.exe | Out-Null; if($$LASTEXITCODE -ge 8){Write-Error ("[FAIL] robocopy exit " + $$LASTEXITCODE); exit 1}; Copy-Item "$$src/keyflux-settings.exe" "$$dst/KeyFlux.Settings.exe" -Force; Write-Host ("[OK] reactor client -> bin/ui: " + (Get-ChildItem $$dst -Recurse -File | Measure-Object).Count + " files")'
	@pwsh -NoProfile -Command 'New-Item -ItemType Directory -Force -Path "bin/ui/fonts" | Out-Null; Copy-Item "config-ui-reactor/resources/fonts/*.ttf" "bin/ui/fonts/" -Force; Write-Host "[OK] bundled fonts -> bin/ui/fonts: " + (Get-ChildItem "bin/ui/fonts" -File).Count + " files"'
	@test -f bin/ui/KeyFlux.Settings.exe || (echo "[FAIL] missing bin/ui/KeyFlux.Settings.exe"; exit 1)

copyFiles: CopyAHK
	rm -f -r $(folder)
	mkdir $(folder)
	mkdir $(folder)/shortcuts

	rm -f -r bin/site
	cp -r site-assets bin/site

	cp -r data $(folder)/
	cp -r bin $(folder)/
	cp -r tools $(folder)/
	rm -f $(folder)/tools/oracle.ps1
	cp KeyFlux.exe $(folder)/
	cp 误报病毒时执行这个.bat $(folder)/

# 如果直接用 wsl 的 cp 命令复制, 复制出的文件会有 read-only 属性, 比较奇怪
CopyAHK:
	@echo '@copy /y "C:\\Program Files\\AutoHotkey\\v2\AutoHotkey64.exe" .\\bin\\' > CopyAHK.bat
	cmd.exe /c CopyAHK.bat
	rm CopyAHK.bat

build: sync-templates buildClientReactor copyFiles
	cd bin; ./settings.exe ChangeVersion $(version)
	rm -f KeyFlux-*.7z
	7z.exe a $(zip) $(folder)
	rm -f -r $(folder)
	@echo ------------------------- build ok -------------------------------

createRelease:
	curl -L \
		-X POST \
		-H "Accept: application/vnd.github+json" \
		-H "Authorization: Bearer $$(cat ~/gh_token)" \
		-H "X-GitHub-Api-Version: 2022-11-28" \
		https://api.github.com/repos/xianyukang/KeyFlux/releases \
		-d '{"tag_name":"v$(version)","target_commitish":"main","name":"v$(version)","body":"Description of the release"}' 2>/dev/null | jq -r '.id' > release_id
	curl -L \
		-X POST \
		-H "Accept: application/vnd.github+json" \
		-H "Authorization: Bearer $$(cat ~/gh_token)" \
		-H "X-GitHub-Api-Version: 2022-11-28" \
		-H "Content-Type: application/octet-stream" \
		"https://uploads.github.com/repos/xianyukang/KeyFlux/releases/$$(cat release_id)/assets?name=$(zip)" \
		--data-binary "@$(zip)" | jq
	rm release_id


uploadLanZou:
	go run scripts/build_tools.go checkForAHKUpdate $(ahkVersion)
	python scripts/lanzou_client.py $(zip) 2> share_link.json
	go run scripts/build_tools.go updateShareLink $(version)
	rm -f share_link.json

upload: uploadLanZou createRelease
	@echo ------------------------- upload ok -------------------------------

# 下面是开发时用到的命令:

ahk:
	@bin/settings.exe GenerateAHK ./data/config.json ./templates/keyflux.tmpl ./bin/KeyFlux.ahk

# ===== 编译输出目录 (单一真源) =====
# 编译产物的最终落点, 固定为本机部署目录 D:\PortableApps\KeyFlux-1.0-beta1。
# 路径用正斜杠写法 (MSYS/Git Bash 与 robocopy 均可识别); 需要临时换落点时用 make OUT_DIR=<路径> 覆盖。
# 说明: bin/ 只是「暂存区」(check 回归、build 打 7z 包都要读它, 不能取消),
#       真正对外生效的产物由 sync-out 从这里同步到 OUT_DIR, 不会留在项目目录里。
OUT_DIR ?= D:/PortableApps/KeyFlux-1.0-beta1

# 输出目录不存在时自动创建 (order-only 前置目标: 目录已存在则直接跳过, 不会误触发重建)
$(OUT_DIR):
	@mkdir -p "$(OUT_DIR)"
	@echo "[mkdir] 编译输出目录已就绪: $(OUT_DIR)"

# ===== 本机回归与部署 (2026-09-03 新增) =====
# 部署目录 = 正在使用的软件 (行为基线配置所在, 见 docs/CONTRACTS.md 约束 1)
DEPLOY_DIR := $(OUT_DIR)
CHECK_CONFIG := $(DEPLOY_DIR)/data/config.json

# check-deploy-tree: 「部署树已就绪」守卫。
#   由来 (2026-09-30): check 直接读 $(CHECK_CONFIG) 生成 + /Validate, 但此前没有任何前置
#   断言 —— 全新环境(make out 到临时 OUT_DIR、或 OUT_DIR 写错)会先跑完 sync-templates/lint/
#   check-texttypes/check-hooks 才在 GenerateAHK 处报一句含糊的失败, 分不清是「部署树没
#   就绪」还是「生成器坏了」。本目标把「缺什么、怎么补」提前说清楚。
#   必须声明为 order-only (|) 前置: 它只做存在性断言、不产出任何文件, 不能参与时间戳比较。
# ⚠️ 本目标的 recipe(含后面 api-parity)刻意只输出 ASCII: 本机 make 是 Windows 原生版, 它把
#   recipe 行经 ANSI 代码页转给 sh —— UTF-8 中文会被错位解码, 个别字节组合甚至会把引号吞掉,
#   报出 "unexpected EOF while looking for matching `"' 这类与真实原因毫不相干的语法错误
#   (2026-09-30 实测)。结论: 中文说明只放注释(注释走 make 自己, 安全), 不放进 recipe。
check-deploy-tree:
	@test -f "$(CHECK_CONFIG)" || (echo "[FAIL] deploy tree not ready: missing $(CHECK_CONFIG)"; echo "       deploy tree = the live installed app dir (OUT_DIR=$(OUT_DIR))"; echo "       fix: set OUT_DIR to the installed app, or seed the factory config first:"; echo "         mkdir -p \"$(DEPLOY_DIR)/data\" && cp data/config.json \"$(CHECK_CONFIG)\""; exit 1)
	@echo "[ok] deploy tree ready: $(CHECK_CONFIG)"

# lint: 标识符冲突静态闸门 (AHK 大小写不敏感, /Validate 查不出「局部变量遮蔽同名函数」类
# 运行时崩溃; 详见阶段 0 §7.3)。扫描 bin/lib 下全部 AHK 源文件, 有 ERROR 即非零退出。
lint:
	python tools/lint_ident.py $$(find bin/lib -name '*.ahk')

# check-texttypes: 内置文本特征**三端一致性**闸门 (Rust 镜像 services/selected_action.rs
# ::TEXT_TYPES ⇄ AHK TextFeatureSpecs ⇄ 共享向量 testdata/text_types.json)。
# 两层: ① 静态对账 (值/大小写开关/正则/顺序逐项比对, 兜底项唯一且居末);
#       ② 运行时对账 (从 AHK 源逐字提取函数体跑向量全部用例 —— 2026-09-17 之前
#          SelectedAction.ahk 声称由 match_ops.json 守护, 但该向量只有 Go 侧消费, 属无强制契约)。
check-texttypes:
	python tools/texttype_conformance.py

# # deploy-panel: guarded manual deploy of the settings panel (probe-marker gate +
# cargo-gates single source + staging + prod hash gate + relaunch). Replaces the
# error-prone hand-rolled chain; -SkipGates skips fmt/clippy/test but never the marker gate.
deploy-panel:
	MSYS_NO_PATHCONV=1 pwsh -NoProfile -ExecutionPolicy Bypass -File tools/deploy_panel.ps1

.PHONY: deploy-panel

check-hooks: CommandInputHooks 的 provider 分发契约回归探针 (AHK 运行时断言, 自带 0/1 退出码)。
# 2026-09-19: _Call 曾以「取方法引用再 .Call()」的方式派发, 而 AHK v2 的 `obj.Method` **不绑定 this**
#   (this 只是普通首参, 取值前无值) ⇒ 首个实参被顶成 this、末位实参缺失 ⇒ 每次回调抛
#   `Missing a required parameter.`, 被分发的 try/catch 吞掉并视为「未消费」。
#   症状: provider 从未执行 = 命令框按空格触发键完全无反应, 且与「插件没挂上」无法区分 ——
#   /Validate 与 lint 都查不出 (纯运行时语义)。本目标即该缺陷的守门人。
#   实测: 旧实现 4 项断言红, 修复后全绿。
# (MSYS_NO_PATHCONV: 防止 Git Bash 把 /ErrorStdOut 误转换为路径)
check-hooks:
	MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut tools/command_input_hooks_test.ahk

# check-ime: ImeInputHost 的存在理由回归闸门 —— 断言 InputHook **无 V 时**确实无法承载 IME 输入
# (该事实正是 §3.12 动态可见性方案的理论基础: IME 开启的会话由 MakeCapsHook 建为 V 透传)。
# ⚠ 本目标需要**按键注入**, 且 KeyFlux 必须已退出 (其 #UseHook 独占键盘钩子会让全部用例收 0 字符)。
#   因此刻意**不并入 check**: check 是「随时可跑」的无副作用闸门, 而本目标会短暂占用键盘钩子
#   并依赖系统已装中文输入法。改动 command-box 输入通道 (ImeInputHost / CommandDisplay /
#   CommandInputHooks) 时手动跑一次; 若断言变红 (无 V 也收到了中文码点), 说明 AHK 默认形态
#   已透传 IME, 应更新 §3.12 的断言前提, 而非退役模块 (2026-09-19 v3 起动态 V 方案已落地)。
check-ime:
	MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut tools/ime_input_test.ahk

# check: 一键回归 = 标识符 lint + 文本特征一致性 + 命令框拦截点契约 + Go 单测 + 重新生成产物 + AHK 语法校验 + Oracle 运行时对账
# (MSYS_NO_PATHCONV: 防止 Git Bash 把 /ErrorStdOut /Validate 等开关误转换为路径)
# 2026-09-17 修「生成产物落点」缺陷 (原配方只要部署配置启用了插件就**必然**失败, 实测阻断 make deploy):
#   生成器给插件入口发的是**相对输出文件所在目录**的 `#Include ../data/plugins/<id>/<file>`
#   (generators/plugins.go renderPluginBlocks, 硬编码 ../data/plugins); 这套相对关系只在
#   部署树成立 (bin/ 与 data/ 同级), 而仓库根根本没有 data/plugins ⇒ 产物写回 ./bin/KeyFlux.ahk
#   后 /Validate 必然报 (26): #Include file "../data/plugins/sample_greeter/main.ahk" cannot be opened。
#   修法: 主生成落点改为**部署树** bin/KeyFlux.ahk (相对关系正确, 且校验覆盖真实的插件注入路径),
#         /Validate 校验这一份; 再复制一份回仓库 bin/ 供 oracle.ps1 用 (它硬编码读 $repo\bin\KeyFlux.ahk)。
#   注: 生成幂等 (同一 config ⇒ 同一字节, 已用 SHA256 验证), 不改变运行时行为;
#       唯一新增约束是校验期间实例不应正持锁写入同一文件 (deploy 流程本就要求先关窗)。
check: sync-templates lint check-texttypes check-hooks sync-plugins | check-deploy-tree $(OUT_DIR)
	@mkdir -p "$(DEPLOY_DIR)/bin"
	MSYS_NO_PATHCONV=1 bin/settings.exe GenerateAHK "$(CHECK_CONFIG)" ./templates/keyflux.tmpl "$(DEPLOY_DIR)/bin/KeyFlux.ahk"
	cp "$(DEPLOY_DIR)/bin/KeyFlux.ahk" ./bin/KeyFlux.ahk
	MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut /Validate "$(DEPLOY_DIR)/bin/KeyFlux.ahk"
	pwsh -NoProfile -ExecutionPolicy Bypass -File tools/oracle.ps1 -Config "$(CHECK_CONFIG)"

# parity: 生成端差分对账闸门 (P0, 见 docs/plan-rust-migration.md)。
#   用当前 bin/settings.exe 复现 tools/parity/reference 的逐字节基线 —— 守护「基准不漂移」；
#   Rust 生成器接入后, 同一脚本即为 Go vs Rust 的等价性闸门 (迁移的切换许可证)。
#   -Capture 仅在**有意的**生成规则变更后重新录制 reference。
parity:
	@pwsh -NoProfile -ExecutionPolicy Bypass -File tools/parity/run_parity.ps1

# api-parity: API 级差分对账闸门 (P0, 见 tools/api-parity/README.md)。
#   对 bin/settings.exe (Rust) 复现 tools/api-parity/reference 基线 (2026-09-30 自 Go 版
#   settings.exe 冻结录制 —— Go 后端已于 2026-10-06 退役, 基线即契约快照)。
#   脚本末行 "API-PARITY: 23/23 PASS [CHECK]" (exit 0); FAIL (exit 1) = 端点行为漂移。
#   ⚠️ bin/settings.exe 必须带 KEYFLUX_VERSION 构建 (option_env! 注入), 否则 GET /config 的
#      keyfluxVersion 字段与基线不同字节 ⇒ step 2 报 MISMATCH。构建口径: KEYFLUX_VERSION=$(version)
#      cargo build --release (或 tools/cargo-gates.ps1 -Release -Version $(version))。
api-parity:
	@test -f bin/settings.exe || (echo "[FAIL] api-parity: missing bin/settings.exe -- build: KEYFLUX_VERSION=$(version) cargo build --release -C config-ui-reactor (bin settings), then copy to bin/"; exit 1)
	@test -d bin/templates && test -d data/plugins || (echo "[FAIL] api-parity: sandbox source tree incomplete (needs bin/templates and data/plugins)"; echo "       run: make sync-templates; mkdir -p data/plugins && cp -r plugins/examples/. data/plugins/"; exit 1)
	@pwsh -NoProfile -ExecutionPolicy Bypass -File tools/api-parity/run_api_parity.ps1 -Check -Exe bin/settings.exe

# check-cs: (已退役) 原 C# 设置界面单测随 config-ui-avalonia 一并移除;
#   等价契约覆盖在 config-ui-reactor 的 cargo test (models/services 单测) 中。
# analyzers: 代码风格闸门。.NET 闸门随 Avalonia 退役; Rust 侧由 clippy -D warnings 承担
#   (与 .github/workflows/analyzers.yml 的 reactor-gates 保持一致: 同一个 tools/cargo-gates.ps1)。
# 前置: 工具链经 config-ui-reactor/env.ps1 注入 (脚本内以 -EnvScript 传入)。
analyzers:
	@pwsh -NoProfile -ExecutionPolicy Bypass -File tools/cargo-gates.ps1 -NoTest -EnvScript config-ui-reactor/env.ps1

# sync-plugins: 把官方示例插件放到部署树 data/plugins。
#   由来 (2026-09-18): 生成端只扫 `<config.json 同级>/plugins` (generators/plugins.go),
#   而 check 用**部署树**的 config.json 生成 + /Validate, 所以插件必须先落到
#   $(OUT_DIR)/data/plugins 才在真实的插件注入路径上被校验到 —— 否则 check 会
#   在「插件未部署」的空目录上假绿, 插件自身的语法错误要等运行时才炸。
#   robocopy 刻意**不带 /MIR**: 只增改, 不删 —— 用户自己导入到 data/plugins 的插件
#   不会被这一步清掉 (仓库里的 plugins/examples 是「随软件分发的官方插件」单一真源)。
#   2026-10-02 P4 墓碑: 复制后按 config.json options.plugins.removed 删除对应目录 ——
#   用户主动删除的随包插件不再被同步带回 (tools/sync-plugins.ps1)。
sync-plugins: | $(OUT_DIR)
	@pwsh -NoProfile -ExecutionPolicy Bypass -File tools/sync-plugins.ps1 -OutDir "$(OUT_DIR)"

# sync-out: 把编译产物同步到 OUT_DIR (robocopy 退出码 0-7 均为成功)
# 守卫 ① 「部署树已就绪」(check-deploy-tree): 见该目标注释 —— 防止 OUT_DIR 写错后在别的
#   地方静默造出一棵新树, 或把产物同步到不存在的部署树上。
# 守卫 ② 「引擎已停止」: 覆盖 bin/*.exe / bin/ui 之前必须先确认引擎与命令框 exe 未在运行。
#   🔴 实测经验 (2026-09-30): robocopy 遇到被独占打开(运行中 exe 自锁)的目标文件**不报错,
#      而是无限重试**(默认 /R:1000000 /W:30 ≈ 每次等 30 秒), 表现为 make 卡死而非失败 ——
#      与本文件其它「失败即失败」的闸门语义完全相反, 只能靠前置检测兜住。
#      patch-commandinput 也提到同一件事(exe 自锁不可写), 但它只在白名单 robocopy **之后**
#      才结束命令框进程, 挡不住这一条; 故这里提前断言, 报错信息里直接给出要关的进程名。
sync-out: sync-plugins | check-deploy-tree $(OUT_DIR)
	@pwsh -NoProfile -Command '. ./tools/lib/kf-tools.ps1; Assert-KfEngineStopped; Write-Host "[ok] engine not running: bin/*.exe can be overwritten"'
	MSYS_NO_PATHCONV=1 robocopy bin/lib $(OUT_DIR)/bin/lib /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy bin/templates $(OUT_DIR)/bin/templates /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy site-assets $(OUT_DIR)/bin/site /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy bin/ui $(OUT_DIR)/bin/ui /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	# 文件模式必须加引号: 否则在 make 工作目录(仓库根)被 shell 展开, *.exe 会变成根目录下的 KeyFlux.exe,
	# 导致 bin/settings.exe 等永远同步不到部署目录 (历史遗留缺陷)
	MSYS_NO_PATHCONV=1 robocopy bin $(OUT_DIR)/bin '*.ahk' '*.exe' '*.ps1' '*.txt' '*.dll' /XF KeyFlux.ahk /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	@$(MAKE) --no-print-directory patch-commandinput

# patch-commandinput: 重新施加命令框 exe 的「抑制八角 keycap」数据 patch。
# 🔴 为什么必须放在 sync-out **之后**: 上面那条 robocopy 的白名单含 '*.exe', 会用仓库侧
#    **未 patch** 的 KeyFlux-CommandInput.exe 覆盖部署树 ⇒ keycap patch 每次都被冲掉,
#    症状是命令框里 a-zA-Z0-9 又被套上八角框 (2026-09-21 实测复现: sync-out 后部署树 exe
#    SHA 退回 f14bba71… = 官方原版。契约 §3.11 硬约束 4 早已警告, 但此前只靠人工纪律执行)。
# 🔴 运行中的 exe 自锁不可写 ⇒ 先结束命令框进程 (懒加载, 引擎会在下次唤起时重建;
#    deploy 末步本就 Stop-Process 重启实例, 此处提前结束不引入新的状态破坏)。
patch-commandinput: | $(OUT_DIR)
	@pwsh -NoProfile -Command 'Stop-Process -Name KeyFlux-CommandInput -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 400'
	python tools/patch_command_input.py "$(OUT_DIR)/bin/KeyFlux-CommandInput.exe"

# check-commandinput-patch: 断言部署树 exe 的 keycap patch 在位 (只读, 供诊断/CI 用)。
check-commandinput-patch:
	python tools/patch_command_input.py "$(OUT_DIR)/bin/KeyFlux-CommandInput.exe" --check

# out: 只编译并把产物落到 OUT_DIR (不跑回归、不重启实例, 便于验证输出目录配置)
# ⚠️ 配方里的 echo 串必须带引号: 裸写的 `-> $(OUT_DIR)` 会被 sh 解析成**重定向**
#    (`-` 后跟 `>`), 报 "D:/...: Is a directory" 并让 make 以 Error 1 收尾 —— 依赖链已全部执行完,
#    但退出码骗人 (2026-09-17 实测踩到, 已加引号)。同一坑对 `build` 目标不适用 (其 echo 无 `->`)。
out: sync-templates buildClientReactor sync-out
	@echo "------------------------- out ok -> $(OUT_DIR) -------------------------------"

# deploy: 回归通过后编译并同步到部署目录, 重启实例 (robocopy 退出码 0-7 均为成功)
# 2026-09-17 修重启步骤的 shell 展开缺陷: 原写法用**双引号**包裹 pwsh 载荷, make 折半后的 $$d
#   会被 /bin/sh 先按变量展开成空串 ⇒ pwsh 收到 `=(Resolve-Path ...).Path`, 报
#   "The term '=' is not recognized" + "Join-Path: missing mandatory parameters: ChildPath"
#   (实测 make Error 1; 此时前置步骤其实已全部成功, 只是实例没被重启)。
#   改为**单引号**包裹载荷 (与 buildClientAvalonia 的 i18n 校验行同款): 单引号阻断 sh 展开,
#   make 的 $$ 折半后原样送进 pwsh; 载荷内只用双引号与 ASCII。
deploy: check buildClientReactor sync-out
	@pwsh -NoProfile -Command '$$d=(Resolve-Path "$(OUT_DIR)").Path; Stop-Process -Name KeyFlux,KeyFlux-CommandInput -Force -ErrorAction SilentlyContinue; Start-Sleep 1; Start-Process (Join-Path $$d "KeyFlux.exe") -WorkingDirectory $$d'

.PHONY: ahk sync-templates buildClientReactor copyFiles upload build check check-texttypes check-hooks check-ime analyzers lint sync-out sync-plugins patch-commandinput check-commandinput-patch out deploy parity api-parity check-deploy-tree