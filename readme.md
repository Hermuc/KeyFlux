[![en](https://img.shields.io/badge/lang-en-red.svg)](https://github.com/Hermuc/KeyFlux/blob/main/readme.en.md)
[![Platform](https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows)](https://github.com/Hermuc/KeyFlux)
[![License](https://img.shields.io/badge/License-GPL--3.0-blue)](./LICENSE)

# ⌨️ KeyFlux

> 基于 [AutoHotkey](https://www.autohotkey.com/) 的 Windows 键盘效率工具 —— **让双手不必离开主键区**。

程序启动切换、键盘控制鼠标、按键重映射，一套键位覆盖大部分重复操作。

## ✨ 核心特性

|  | 特性 | 说明 |
| --- | --- | --- |
| 🚀 | **程序启动切换** | 快捷键直达应用与窗口，比搜索型启动器更快 |
| 🖱️ | **键盘控制鼠标** | 减少键鼠来回切换，不必为了点一下而移动手掌 |
| ⌨️ | **按键重映射** | 常用按键搬到主键区，内置「光标控制 / 数字输入 / 符号输入」三套键位 |

## 📦 快速开始

1. 到 [Releases](https://github.com/Hermuc/KeyFlux/releases/latest) 下载并解压 📥
2. 双击主程序 `KeyFlux.exe` 运行 ▶️
3. 按 <kbd>CapsLock</kbd> + <kbd>S</kbd> + <kbd>E</kbd> 打开设置界面 ⚙️

📖 [快速入门](https://xianyukang.com/MyKeymap.html#mykeymap-%E7%AE%80%E4%BB%8B) · 🎬 [视频介绍](https://www.bilibili.com/video/BV1Sf4y1c7p8)

## 🖼️ 界面预览

| ![features](./doc/features.png) | ![夏日大作战](./doc/夏日大作战.gif) |
| ------------------------------- | ----------------------------------- |

![settings](./doc/settings.png)

## 🔀 与原版的主要差异

本 fork 基于上游 [xianyukang/MyKeymap](https://github.com/xianyukang/MyKeymap)，主要改造如下：

- 🖥️ **原生设置窗口**：Avalonia 桌面端（`config-ui-avalonia/`）取代旧 Vue 浏览器版，热键/托盘直接唤起，经 localhost HTTP 与 Go 后端通信，配置写盘仍由后端统一负责
- ⚡ **「选中动作」系统**：选中文本或文件后按快捷键即执行预设操作，规则可可视化配置；支持「文件后缀」「文本特征」两类匹配，文本特征与行为强制联动，避免「选中链接却被程序打开」的错配
- 🎨 **CommandInput 皮肤可配置**：输入框外观（背景、边框、圆角、网格线、按键配色、窗口位置、阴影等 18 项）可视化调整，存于 `commandInputSkin`
- 🛡️ **更不容易崩溃**：单个热键配置出错只跳过并提示，不会导致整个程序退出
- 🧰 **开机自启 + 一键卸载**：自启改用注册表 `HKCU\Run`；附带全中文「卸载软件.bat」，二次确认后结束进程、清理自启与残留
- 🚪 **托盘程序一键唤出**：微信/QQ 等最小化到托盘后，快捷键秒级唤出主窗口，不会重复启动新实例
- 🌈 **细节打磨**：总览页文档可阅读编辑、滚动条 Fluent 化、强制浅色主题、窗口标识符写法即时校验等

<details>
<summary>🔍 展开完整细节</summary>

- 💊 **黑客帝国代码雨**：直接运行 `bin\settings.exe` 时控制台显示「黑客帝国」风格代码雨动画（`options.hideMatrix` 可关闭）
- ⚡ **选中动作（完整）**：
  - **文件后缀**：`jpg`、`png` … 支持「常用分组快捷填入」，一键填入图片/文档/代码/压缩包/视频/音频等分组后缀，分组表可在配置文件自定义
  - **文本特征**：自动识别 链接 / 路径 / 磁力链接 / 纯文本
  - **内置 5 类文本行为**：打开网址、打开路径、打开文件夹、磁力下载（唤起默认 BT 工具）、注册表定位
- 🚪 **托盘唤出增强**：唤出后鼠标自动还原到唤起前位置；FlClash 最小化到状态栏可秒开、资源管理器冷启动与切换、AyuGram 动态标题同步等问题均已修复（v13）
- 🪟 **窗口标识符校验引导**：裸写 `xxx.exe` 会即时红字报错并提示正确写法（`ahk_exe` 前缀）；标题会变的软件建议用进程匹配
- 📝 **总览页文档**：还原 Web UI 版式，正文可直接选中复制，编辑入口在页面底部编辑区，保存即时回显
- 🖱️ **滚动条交互**：悬停即时加粗、移开立即变细（Fluent 默认 0.5s/2s 延迟 → 0），Thumb 两端胶囊圆角，宽度 14px → 13px
- 🔧 **其他**：设置页菜单高亮跟随页面、快捷键输入框对齐、保存按钮常驻、导航图标换回 MDI 风格

> 🔧 开发者如需逐条修改记录（涉及文件、原因、技术细节），见 [与原版的差异（开发者版）](./doc/与原版的差异.md)。

</details>

## 🧩 架构与扩展生态

本 fork 已完成七阶段模块化重构（**零行为变更**，每阶段均通过生成产物对比与运行时对账验证），为插件生态打好地基：

| 模块 | 职责 |
| --- | --- |
| `bin/lib/core/` | 核心引擎 |
| `bin/lib/actions/` | 动作注册表 + 内置动作 |
| `bin/lib/commands/` | 缩写命令运行时查表 |
| `bin/lib/context/` | 选中文本统一入口 |
| `bin/lib/rules/` | 选中动作规则 |
| `bin/lib/plugins/` | 插件框架（L1 进程内 / L2 子进程 JSON-RPC） |

- 📇 **缩写命令查表化**：由「生成时硬编码 switch」改为「运行时注册表查表」，CapsLock / 分号两域隔离，并与生成端自动对账（Oracle），两侧数据不一致即时暴露
- 📡 **事件总线**：`IKeyEventBus` 抽象落地，模式进出 / 缩写提交 / 选中动作 / 插件生命周期统一广播；接口预留 Rust FFI 重写
- 🔌 **插件框架**：`PluginManager` + `APIBridge` 按冻结契约落地 —— 6 项权限词表、按权限裁剪的 7 组 API 命名空间（selection / window / send / run / ui / events / config）、错误隔离

<details>
<summary>🗺️ 功能进展</summary>

| 功能 | 状态 |
| --- | --- |
| 命令模糊输入 | ✅ 已实现：逐字符实时后缀校验（误输 `dfc` 时尾部命中 `fc` 立即执行，最长后缀优先）；编辑距离容错与候选提示仍在规划 |
| Everything 本地搜索插件 | 框架就位，插件本体待做 |
| 外接脚本/函数正式化（L1 插件） | ScriptHost 已实现，生成端接入待做 |
| 开放 API / 插件市场 | 接口与 L2 协议已冻结，待接入 + 首个真实插件 |
| Rust 重写（键盘捕获与事件分发） | `IKeyEventBus` 已就位，守护进程待做 |

</details>

详细契约（接口签名、插件清单格式、L2 JSON-RPC 协议、7 条不可协商约束）见 [docs/CONTRACTS.md](./docs/CONTRACTS.md)。

<details>
<summary>🛠️ 构建说明（开发者）</summary>

- **源码**：`config-ui-avalonia/`（.NET 10 / Avalonia 11 / CommunityToolkit.Mvvm），单元测试在仓库根目录 `KeyFlux.Settings.Tests/`
- **构建**：`make buildClientAvalonia`（完整构建 `make build` 已包含），`dotnet publish` 自包含 win-x64 + ReadyToRun 发布到 `bin/ui/`（不入库），需 .NET 10 SDK
- **运行原理**：GUI 拉起 `settings.exe --headless` 子进程（Mutex 单实例 + Job Object 兜底回收），经 localhost HTTP 调用既有 API；GUI 不直接写 config.json
- **入口**：AHK 设置入口（`bin/lib/core/Functions.ahk`）启动 `bin\ui\KeyFlux.Settings.exe`；旧浏览器版设置页已随 Vue 源码移除

</details>

---

🙌 上游项目：[xianyukang/MyKeymap](https://github.com/xianyukang/MyKeymap) · 📄 许可：[GPL-3.0](./LICENSE)
