version = 1.0-beta1
ahkVersion = 2.0.19
folder = KeyFlux-$(version)
zip = $(folder).7z

buildServer:
	go.exe build -C ./config-server -tags=nomsgpack -ldflags "-s -w -X settings/internal/script.KeyfluxVersion=$(version)" -o ../bin/settings.exe ./cmd/settings
	rm -f -r bin/templates
	cp -r config-server/templates bin/templates

# 发布 Avalonia 原生设置界面到 bin/ui/ (自包含, 免装 .NET 运行时)
# 2026-09-12 关闭 ReadyToRun: R2R 预编译镜像使 bin/ui 体积接近翻倍、运行时常驻内存更高;
#   关闭后磁盘显著缩小、面板内存占用降低, 代价仅为面板冷启动多一轮 JIT
#   (AHK 唤起侧本就有 3s WinWait + 约 10s 兜底轮询, 可容忍; 实测数据见对应提交)
# 注意 PATH 陷阱: C:\Program Files\dotnet 可能只有运行时没有 SDK, 须显式探测
buildClientAvalonia:
	@dotnet --list-sdks | grep -q . || (echo "[错误] dotnet --list-sdks 为空: 未找到 .NET SDK (PATH 陷阱: C:\\Program Files\\dotnet 可能只有运行时无 SDK), 请安装 SDK 或将 PATH 指向含 SDK 的 dotnet.exe"; exit 1)
	rm -f -r bin/ui
	cd config-ui-avalonia; dotnet publish -c Release -r win-x64 --self-contained true -p:PublishReadyToRun=false -o ../bin/ui
	@pwsh -NoProfile -Command "$$src='config-ui-avalonia/Resources/i18n.json'; $$dst='bin/ui/Resources/i18n.json'; if(!(Test-Path $$dst)){Write-Error '[断言失败] publish 后缺少散资源: '$$dst; exit 1}; $$h1=(Get-FileHash $$src -Algorithm SHA256).Hash; $$h2=(Get-FileHash $$dst -Algorithm SHA256).Hash; if($$h1 -ne $$h2){Write-Error ('[断言失败] i18n.json SHA256 不一致: 源=' + $$h1 + ' 产出=' + $$h2); exit 1}; Write-Host '[OK] i18n.json SHA256 一致: '$$h1"

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

# check: 一键回归 = 标识符 lint + Go 单测 + 重新生成产物 + AHK 语法校验 + Oracle 运行时对账
# (MSYS_NO_PATHCONV: 防止 Git Bash 把 /ErrorStdOut /Validate 等开关误转换为路径)
check: buildServer lint
	MSYS_NO_PATHCONV=1 bin/settings.exe GenerateAHK "$(CHECK_CONFIG)" ./config-server/templates/keyflux.tmpl ./bin/KeyFlux.ahk
	MSYS_NO_PATHCONV=1 bin/AutoHotkey64.exe /ErrorStdOut /Validate ./bin/KeyFlux.ahk
	pwsh -NoProfile -ExecutionPolicy Bypass -File tools/oracle.ps1

# check-cs: C# 设置界面单元测试 (dotnet SDK 须在 PATH; 本机 SDK 在 Scoop 的 dotnet-sdk)
check-cs:
	dotnet test KeyFlux.Settings.Tests/KeyFlux.Settings.Tests.csproj --nologo

# sync-out: 把编译产物同步到 OUT_DIR (robocopy 退出码 0-7 均为成功)
sync-out: | $(OUT_DIR)
	MSYS_NO_PATHCONV=1 robocopy bin/lib $(OUT_DIR)/bin/lib /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy bin/templates $(OUT_DIR)/bin/templates /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy site-assets $(OUT_DIR)/bin/site /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	MSYS_NO_PATHCONV=1 robocopy bin/ui $(OUT_DIR)/bin/ui /MIR /NFL /NDL /NJH /NJS; [ $$? -le 7 ]
	# 文件模式必须加引号: 否则在 make 工作目录(仓库根)被 shell 展开, *.exe 会变成根目录下的 KeyFlux.exe,
	# 导致 bin/settings.exe 等永远同步不到部署目录 (历史遗留缺陷)
	MSYS_NO_PATHCONV=1 robocopy bin $(OUT_DIR)/bin '*.ahk' '*.exe' '*.ps1' '*.txt' '*.dll' /XF KeyFlux.ahk /NFL /NDL /NJH /NJS; [ $$? -le 7 ]

# out: 只编译并把产物落到 OUT_DIR (不跑回归、不重启实例, 便于验证输出目录配置)
out: buildServer buildClientAvalonia sync-out
	@echo ------------------------- out ok -> $(OUT_DIR) -------------------------------

# deploy: 回归通过后编译并同步到部署目录, 重启实例 (robocopy 退出码 0-7 均为成功)
deploy: check buildClientAvalonia sync-out
	pwsh -NoProfile -Command "$$d=(Resolve-Path '$(OUT_DIR)').Path; Stop-Process -Name KeyFlux,KeyFlux-CommandInput -Force -ErrorAction SilentlyContinue; Start-Sleep 1; Start-Process (Join-Path $$d 'KeyFlux.exe') -WorkingDirectory $$d"

.PHONY: server ahk buildServer buildClientAvalonia copyFiles upload build check check-cs lint sync-out out deploy