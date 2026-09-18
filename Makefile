version = 1.0-beta1
ahkVersion = 2.0.19
folder = KeyFlux-$(version)
zip = $(folder).7z

buildServer:
	go.exe build -C ./config-server -tags=nomsgpack -ldflags "-s -w -X settings/internal/script.KeyfluxVersion=$(version)" -o ../bin/settings.exe ./cmd/settings
	rm -f -r bin/templates
	cp -r config-server/templates bin/templates

# 发布 Avalonia 原生设置界面到 bin/ui/ (自包含, 免装 .NET 运行时)
# 2026-09-12 裁剪+R2R: 发布调优 (PublishTrimmed/TrimMode/R2R/TrimmerRoots/STJ 反射开关)
#   统一收进 KeyFlux.Settings.csproj 单一真源 —— 命令行 -p: 会覆盖 csproj, 本文件勿再传。
#   实测: bin/ui 118M→65M (deploy 140M→87M), 冷启动 598ms (R2R 保 <1s), 面板稳态内存 <200MB。
# 注意 PATH 陷阱: C:\Program Files\dotnet 可能只有运行时没有 SDK, 须显式探测
# 2026-09-17 修 i18n 散资源断言配方 (原写法在 POSIX shell 下必然失败): 原用双引号包裹 pwsh -Command
#   载荷, make 折半后的 $src/$dst/$h1 会被 sh 当变量展开成空 ⇒ 载荷退化为 `+ +`, pwsh 报 ParserError
#   (且载荷里的中文会被 pwsh 按 ANSI 码页解码, 可能吞掉紧随的引号)。改为**单引号包裹**载荷 +
#   载荷内只用双引号与 ASCII 文案 (注释仍可中文): 单引号阻断 sh 展开, make 的 $$ 折半后原样送进 pwsh。
buildClientAvalonia:
	@dotnet --list-sdks | grep -q . || (echo "[错误] dotnet --list-sdks 为空: 未找到 .NET SDK (PATH 陷阱: C:\\Program Files\\dotnet 可能只有运行时无 SDK), 请安装 SDK 或将 PATH 指向含 SDK 的 dotnet.exe"; exit 1)
	rm -f -r bin/ui
	cd config-ui-avalonia; dotnet publish -c Release -r win-x64 --self-contained true -o ../bin/ui
	@pwsh -NoProfile -Command '$$src="config-ui-avalonia/Resources/i18n.json"; $$dst="bin/ui/Resources/i18n.json"; if(!(Test-Path $$dst)){Write-Error ("[FAIL] missing loose resource: " + $$dst); exit 1}; $$h1=(Get-FileHash $$src -Algorithm SHA256).Hash; $$h2=(Get-FileHash $$dst -Algorithm SHA256).Hash; if($$h1 -ne $$h2){Write-Error ("[FAIL] i18n.json SHA256 mismatch: src=" + $$h1 + " out=" + $$h2); exit 1}; Write-Host ("[OK] i18n.json SHA256 match: " + $$h1)'

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

build: buildServer buildClientAvalonia copyFiles
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

server: buildServer
	@cd config-server; ../bin/settings.exe debug

ahk: buildServer
	@bin/settings.exe GenerateAHK ./data/config.json ./config-server/templates/keyflux.tmpl ./bin/KeyFlux.ahk

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

# lint: 标识符冲突静态闸门 (AHK 大小写不敏感, /Validate 查不出「局部变量遮蔽同名函数」类
# 运行时崩溃; 详见阶段 0 §7.3)。扫描 bin/lib 下全部 AHK 源文件, 有 ERROR 即非零退出。
lint:
	python tools/lint_ident.py $$(find bin/lib -name '*.ahk')

# check-texttypes: 内置文本特征**三端一致性**闸门 (Go 注册表 behaviors/textfeatures.go ⇄
# AHK TextFeatureSpecs ⇄ 共享向量 config-server/internal/script/testdata/text_types.json)。
# 两层: ① 静态对账 (值/大小写开关/正则/顺序逐项比对, 兜底项唯一且居末);
#       ② 运行时对账 (从 AHK 源逐字提取函数体跑向量全部用例 —— 2026-09-17 之前
#          SelectedAction.ahk 声称由 match_ops.json 守护, 但该向量只有 Go 侧消费, 属无强制契约)。
check-texttypes:
	python tools/texttype_conformance.py

# check: 一键回归 = 标识符 lint + 文本特征一致性 + Go 单测 + 重新生成产物 + AHK 语法校验 + Oracle 运行时对账
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
check: buildServer lint check-texttypes sync-plugins | $(OUT_DIR)
	@mkdir -p "$(DEPLOY_DIR)/bin"
	MSYS_NO_PATHCONV=1 bin/settings.exe GenerateAHK "$(CHECK_CONFIG)" ./config-server/templates/keyflux.tmpl "$(DEPLOY_DIR)/bin/KeyFlux.ahk"
	cp "$(DEPLOY_DIR)/bin/KeyFlux.ahk" ./bin/KeyFlux.ahk
	MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut /Validate "$(DEPLOY_DIR)/bin/KeyFlux.ahk"
	pwsh -NoProfile -ExecutionPolicy Bypass -File tools/oracle.ps1

# check-cs: C# 设置界面单元测试 (dotnet SDK 须在 PATH; 本机 SDK 在 Scoop 的 dotnet-sdk)
check-cs:
	dotnet test KeyFlux.Settings.Tests/KeyFlux.Settings.Tests.csproj --nologo

# analyzers: IDE 代码风格诊断闸门 (未使用 using / 未使用私有成员 / 未使用变量与参数 / Substring 简化)
# **必须两个项目都跑**: 2026-09-16 发现本闸门此前只覆盖 config-ui-avalonia, 测试项目从未受检,
# 首次补跑即命中 5 处 (IDE0059 ×3 / IDE0060 ×1 / IDE0057 ×1, 均已修复)。
# 前置: 需先 build/restore (故带 --no-restore); 与 .github/workflows/analyzers.yml 的命令须保持一致。
analyzers:
	dotnet format style config-ui-avalonia/KeyFlux.Settings.csproj --verify-no-changes --no-restore --severity info --diagnostics IDE0005 IDE0051 IDE0052 IDE0060 IDE0057 IDE0059
	dotnet format style KeyFlux.Settings.Tests/KeyFlux.Settings.Tests.csproj --verify-no-changes --no-restore --severity info --diagnostics IDE0005 IDE0051 IDE0052 IDE0060 IDE0057 IDE0059

# sync-plugins: 把官方示例插件放到部署树 data/plugins。
#   由来 (2026-09-18): 生成端只扫 `<config.json 同级>/plugins` (generators/plugins.go),
#   而 check 用**部署树**的 config.json 生成 + /Validate, 所以插件必须先落到
#   $(OUT_DIR)/data/plugins 才在真实的插件注入路径上被校验到 —— 否则 check 会
#   在「插件未部署」的空目录上假绿, 插件自身的语法错误要等运行时才炸。
#   robocopy 刻意**不带 /MIR**: 只增改, 不删 —— 用户自己导入到 data/plugins 的插件
#   不会被这一步清掉 (仓库里的 plugins/examples 是「随软件分发的官方插件」单一真源)。
sync-plugins: | $(OUT_DIR)
	MSYS_NO_PATHCONV=1 robocopy plugins/examples $(OUT_DIR)/data/plugins /E /NFL /NDL /NJH /NJS; [ $$? -le 7 ]

# sync-out: 把编译产物同步到 OUT_DIR (robocopy 退出码 0-7 均为成功)
sync-out: sync-plugins | $(OUT_DIR)
	MSYS_NO_PATHCONV=1 robocopy bin/lib $(OUT_DIR)/bin/lib /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy bin/templates $(OUT_DIR)/bin/templates /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy site-assets $(OUT_DIR)/bin/site /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy bin/ui $(OUT_DIR)/bin/ui /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	# 文件模式必须加引号: 否则在 make 工作目录(仓库根)被 shell 展开, *.exe 会变成根目录下的 KeyFlux.exe,
	# 导致 bin/settings.exe 等永远同步不到部署目录 (历史遗留缺陷)
	MSYS_NO_PATHCONV=1 robocopy bin $(OUT_DIR)/bin '*.ahk' '*.exe' '*.ps1' '*.txt' '*.dll' /XF KeyFlux.ahk /NFL /NDL /NJH /NJS; [ $$? -le 7 ]

# out: 只编译并把产物落到 OUT_DIR (不跑回归、不重启实例, 便于验证输出目录配置)
# ⚠️ 配方里的 echo 串必须带引号: 裸写的 `-> $(OUT_DIR)` 会被 sh 解析成**重定向**
#    (`-` 后跟 `>`), 报 "D:/...: Is a directory" 并让 make 以 Error 1 收尾 —— 依赖链已全部执行完,
#    但退出码骗人 (2026-09-17 实测踩到, 已加引号)。同一坑对 `build` 目标不适用 (其 echo 无 `->`)。
out: buildServer buildClientAvalonia sync-out
	@echo "------------------------- out ok -> $(OUT_DIR) -------------------------------"

# deploy: 回归通过后编译并同步到部署目录, 重启实例 (robocopy 退出码 0-7 均为成功)
# 2026-09-17 修重启步骤的 shell 展开缺陷: 原写法用**双引号**包裹 pwsh 载荷, make 折半后的 $$d
#   会被 /bin/sh 先按变量展开成空串 ⇒ pwsh 收到 `=(Resolve-Path ...).Path`, 报
#   "The term '=' is not recognized" + "Join-Path: missing mandatory parameters: ChildPath"
#   (实测 make Error 1; 此时前置步骤其实已全部成功, 只是实例没被重启)。
#   改为**单引号**包裹载荷 (与 buildClientAvalonia 的 i18n 校验行同款): 单引号阻断 sh 展开,
#   make 的 $$ 折半后原样送进 pwsh; 载荷内只用双引号与 ASCII。
deploy: check buildClientAvalonia sync-out
	@pwsh -NoProfile -Command '$$d=(Resolve-Path "$(OUT_DIR)").Path; Stop-Process -Name KeyFlux,KeyFlux-CommandInput -Force -ErrorAction SilentlyContinue; Start-Sleep 1; Start-Process (Join-Path $$d "KeyFlux.exe") -WorkingDirectory $$d'

.PHONY: server ahk buildServer buildClientAvalonia copyFiles upload build check check-texttypes check-cs analyzers lint sync-out sync-plugins out deploy