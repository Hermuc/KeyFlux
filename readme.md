[![en](https://img.shields.io/badge/lang-en-red.svg)](https://github.com/Hermuc/KeyFlux/blob/main/readme.en.md)

# KeyFlux

KeyFlux 是一款基于 [AutoHotkey](https://www.autohotkey.com/) 的键盘映射工具，用于增强 Windows 的键盘输入体验和窗口操作效率。

## Features

- **程序启动切换**: 用快捷键启动程序和切换窗口，对比搜索型的启动器，效率更高
- **键盘控制鼠标**: 用键盘控制鼠标，能减少键鼠切换，不必为了点一下而大幅移动手掌
- **按键重新映射**: 
  - 把常用按键重映射到主键区，能大大提升输入速度、编辑文字的效率
  - 内置了几套键位负责:「 光标控制 」、「 数字输入 」、「 符号输入 」

## Usage

- [快速入门](https://xianyukang.com/MyKeymap.html#mykeymap-%E7%AE%80%E4%BB%8B) & [视频介绍](https://www.bilibili.com/video/BV1Sf4y1c7p8)

| ![features](./doc/features.png) | ![夏日大作战](./doc/夏日大作战.gif) |
| ------------------------------- | ----------------------------------- |

## Screenshots
![settings](./doc/settings.png)

## 与原版的差异

本 fork 基于上游 [xianyukang/MyKeymap](https://github.com/xianyukang/MyKeymap)，在原版基础上主要有这些不同：

- **黑客帝国代码雨**：直接运行 `bin\settings.exe` 控制台时显示「黑客帝国」风格的代码雨动画（配置 `options.hideMatrix` 可关闭）；打开设置已改由 Avalonia 原生窗口承载，不再经 Windows Terminal（见下条）
- **原生设置窗口（Avalonia GUI）**：设置界面提供独立原生窗口版（源码见 `config-ui-avalonia/`），由托盘/热键入口直接打开，不再依赖浏览器；GUI 以子进程方式拉起 `settings.exe --headless` 并通过 localhost HTTP 通信，配置写盘仍由 Go 后端统一负责；旧 Vue 浏览器版设置页已整体移除（源码与内置页面 2026-08 清除，文档资产迁移至 `site-assets/`）
- **新增「选中动作」系统**：选中文字或文件后按快捷键，即可执行预设操作；规则可在设置界面可视化配置，无需改动任何代码。匹配类型 2 类——「文件后缀」（jpg, png, … 支持「常用分组快捷填入」，一键填入图片/文档/代码/压缩包/视频/音频等分组后缀，分组表可在配置文件自定义增改）、「文本特征」（自动识别链接 / 路径 / 磁力链接 / 纯文本）；文本特征与可选行为强制联动（如磁力链接只能配「磁力下载」），避免「选中链接却程序打开」的错配；内置 5 类文本专用行为：打开网址、打开路径、打开文件夹、磁力下载（自动唤起默认 BT 工具）、注册表定位（自动跳转到指定注册表项）
- **CommandInput 皮肤可配置**：命令输入框外观（背景、边框、圆角、网格线、按键配色、窗口位置/宽度、阴影、隐藏动画时长等 18 项参数）可在设置页可视化调整，配置存于 config.json 的 `commandInputSkin` 字段
- **更不容易崩溃**：某个热键配置出错时会自动跳过并提示，不会再导致整个程序退出；收集文本、关机动作等细节行为也更稳定
- **开机自启 + 一键卸载**：开机自启机制改为注册表 HKCU\Run（比启动文件夹更可靠）；附带全中文「卸载软件.bat」一键卸载（二次确认、结束进程、清理自启与残留，防误删）
- **细节体验优化**：设置页菜单高亮跟随页面切换、快捷键输入框对齐、保存按钮始终可见；强制浅色主题修复系统深色模式下白底白字；导航图标换回 Vue 版 MDI 图标样式
- **总览页文档阅读与编辑**：总览页还原 Web UI 版式；正文文字可直接选中复制；编辑入口移至页面底部编辑区（EditZoneHint 控件），一键进入编辑、保存后即时回显，无需再开独立编辑窗口
- **设置界面滚动条交互优化**：悬停即时加粗（Fluent 默认延迟 0.5s → 0）、移开立即变细（默认 2s → 0，粗细过渡仍由 100ms 渲染动画平滑完成）、Thumb 两端胶囊圆角、整体宽度收窄（约 14px → 13px），视觉贴合 Fluent/WinUI 风格
- **后台程序托盘一键唤出**：目标程序（如微信/QQ）最小化到托盘后，按快捷键可直接唤出主窗口（亚秒级，不会误触发「登录新账号」重复启动；唤起引擎已切 pwsh 7 进一步提速）
- **FlClash/资源管理器/AyuGram 唤起修复**：FlClash 关闭后驻留状态栏时按快捷键秒开（直显隐藏窗口），可见时按快捷键可正常最小化/激活切换；资源管理器冷启动失效（桌面壳窗口误匹配）与切换、AyuGram 动态标题切换同步修复（v13）
- **窗口标识符配置校验引导**：设置界面输入窗口标识符时，裸写 xxx.exe 会即时红字报错并提示正确写法（ahk_exe 前缀）；窗口标题会变化的软件（如聊天软件频道页）不要用标题匹配，应使用 ahk_exe 进程匹配
- **托盘唤起后鼠标归位**：通过托盘图标唤出窗口后，鼠标自动还原到唤起前位置，不再停留在托盘图标上（v13）

> 🔧 开发者如需查看逐条详细修改记录（涉及的文件、修改原因、技术细节），请参阅 [与原版的差异（开发者版）](./doc/与原版的差异.md)。

## 架构与扩展生态（模块化重构 2026-08）

本 fork 已完成七阶段模块化重构（**零行为变更**，每阶段均通过生成产物对比与运行时对账回归验证），为扩展生态打好地基：

- **代码分层**：`bin/lib/` 按职责拆分 —— `core/`（核心引擎）/ `actions/`（动作注册表 + 内置动作）/ `commands/`（缩写命令运行时查表）/ `context/`（选中文本统一入口）/ `plugins/`（插件框架）/ `rules/`（选中动作）
- **缩写命令查表化**：缩写命令由「生成时硬编码 switch」改为「运行时注册表精确查表」，CapsLock / 分号两域隔离，并与生成端内置自动对账机制（Oracle），两边数据不一致可即时发现
- **事件总线**：`IKeyEventBus` 抽象已落地，模式进出 / 缩写提交 / 选中动作 / 插件生命周期均广播事件；接口设计**预留 Rust FFI 重写**（未来 Rust 守护进程可直接替换实现，订阅方零改动）
- **插件框架**：`PluginManager` + `APIBridge` 按冻结契约落地 —— 6 项权限词表、按权限裁剪的 API 视图（selection / window / send / run / ui / events / config 七组命名空间）、错误隔离、L1（进程内 AHK）/ L2（子进程 JSON-RPC）双层设计

**功能进展**（命令模糊输入已实现，其余为接口就绪、功能待续）：

| 功能 | 状态 |
|---|---|
| 命令模糊输入 | ✅ 已实现：逐字符实时后缀校验（误输 `dfc` 时尾部命中 `fc` 立即执行，最长后缀优先）；编辑距离容错 + 候选提示仍在规划 |
| Everything 本地搜索插件（设置页填路径，自动打开并搜索） | 插件框架就位，插件本体待做 |
| 外接脚本/函数正式化（L1 插件 = custom_functions 升级） | ScriptHost 已实现，生成端接入待做 |
| 开放 API / 插件市场 | 接口与 L2 协议已冻结，生成端接入 + 首个真实插件待做 |
| Rust 重写（键盘捕获与事件分发） | IKeyEventBus 抽象已就位，守护进程待做 |

详细契约（接口签名、插件清单格式、L2 JSON-RPC 协议、7 条不可协商约束）见 [docs/CONTRACTS.md](./docs/CONTRACTS.md)。

### 原生设置界面（Avalonia GUI）构建说明（2026-08）

- **源码位置**：`config-ui-avalonia/`（GUI 壳，.NET 10 / Avalonia 11 / CommunityToolkit.Mvvm）；单元测试位于仓库根目录 `KeyFlux.Settings.Tests/`
- **构建**：`make buildClientAvalonia`（完整构建 `make build` 已包含），`dotnet publish` 自包含 win-x64 + ReadyToRun 发布到 `bin/ui/`（不入库）；开发环境需 .NET 10 SDK
- **运行原理**：GUI 拉起 `settings.exe --headless` 子进程（命名 Mutex 单实例、Job Object 兜底回收），经 localhost HTTP 调用既有全部 API；配置写盘仍由 Go 后端统一负责，GUI 不直接写 config.json
- **入口**：AHK 设置入口（`bin/lib/core/Functions.ahk`）启动 `bin\ui\KeyFlux.Settings.exe`；旧浏览器版设置页已随 Vue 源码一并移除（无参运行 settings.exe 现返回 404）
