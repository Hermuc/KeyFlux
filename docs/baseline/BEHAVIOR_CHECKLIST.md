# KeyFlux 热键行为核对清单(基线 v2:部署真源)

> 本清单是重构期间"零行为变更"约束的验收依据。
> 每完成一个重构阶段,须逐项核对后方可进入下一阶段。

## 基线声明(2026-08-22 修正)

| 项 | 值 |
|---|---|
| 基线脚本 | `docs/baseline/MyKeymap.ahk.snapshot`(= 重构前部署目录 `MyKeymap-2.0-beta33\bin\KeyFlux.ahk`,**255 行**,备份自 `MyKeymap-deploy-backup-before-phase1`) |
| 配置真源 | **部署目录 `MyKeymap-2.0-beta33\data\config.json`**(用户日常使用与保存配置的版本,16 keymaps,启用 4 模式);仓库 `data/config.json` 已同步为部署配置 |
| 生成分支 | `dev`(原 refactor/modularize) |
| 模式过滤规则 | `EnabledKeymaps`:仅 `id=1` 与 `id>=5` 且 `enable=true` 的 keymap 进入生成(id=2/3/4 由 CommandInput/静态 switch 处理)。部署配置启用:id=5 CapsLock / id=8 J 模式 / id=9 F 模式 / id=1 Custom Hotkeys;其余(id=6,7,10,11,12,13,14,16,17)均为 `enable=false`,不生成 |
| 已知手工行 | ~~`!f17` 手工行~~ **已证伪**:`!f17` 由 `Preprocess` 自动注入(运行时路径 `GenerateScripts`),非手工。此前误判原因:验证用的 `GenerateAHK` 命令不调 `Preprocess`。阶段 3 已对齐两条路径,`!f17` 由生成器自动产出 |
| 配置自变记录 | 8/20 备份后,用户经设置页新增 actionSchemes(id=1 "新的选中动作",enable=true,单条空规则)→ 生成脚本多出**空的** `ActionSchemeList := Array()` 段,无实际热键。此为配置变化,非代码引起 |
| 已废弃 | 仓库原始 `data/config.json`(上游样例,57686 字节)与部署配置不同(多出 CapsLock+F/Space、3/分号/句号模式、28 条缩写、gg/bb 等),不再作为基线依据 |

## 全局设置(生成脚本头部)

- [ ] `SendMode "Event"`、`SetKeyDelay 0`、`SetMouseDelay 0`、`SetWinDelay 0`
- [ ] `A_MaxHotkeysPerInterval := 256`、`ProcessSetPriority "High"`、`#UseHook true`
- [ ] DPI 感知 DllCall、`SetWorkingDir("../")`
- [ ] 启动即运行 `bin\KeyFlux-CommandInput.exe`
- [ ] 托盘菜单 5 项:暂停/退出/重载/设置/WindowSpy,单击=暂停,图标 `bin/icons/logo.ico`
- [ ] 退出时 `KeyFluxExit` 关闭 CommandInput 进程
- [ ] `InitKeymap` 结尾 `KeymapManager.GlobalKeymap.Enable()`;脚本末尾空 `#HotIf`

## 模式 1 — CapsLock(`*CapsLock`,id=5)

禁用条件:`KeymapManager.GlobalKeymap.DisabledAt = "ahk_group MY_WINDOW_GROUP__1"`(Stardew Valley + Rune Factory 3 Special)

窗口操作:
- [ ] `c` 音量控制 / `z` 复制选中为纯文本(`CopySelectedAsPlainText`)/ `.` 窗口拖拽 / `a` 居中 1370x930
- [ ] `b` 最小化 / `e` 任务切换(TaskSwitchKeymap:e/d/s/f/c/space)/ `g` 置顶切换
- [ ] `p` 下一虚拟桌面 / `y` 上一虚拟桌面 / `q` 最大化 / `r` 循环相关窗口
- [ ] `s` 居中 1200x800 / `d` 居中 1740x1000 / `t` 绑定窗口 / `v` 移到下一显示器
- [ ] `w` 上一窗口 / `x` 智能关闭

鼠标(快慢双速注册:fast=110/70,slow=10/13,delay 0.13/0.01/0.2/0.03):
- [ ] `ijlk` 上下左右移动 / `nm` 左/右键 / `,` 左键按下 / `uo` 滚轮上下 / `h;` 滚轮左右
- [ ] `/` 鼠标移到光标处

特殊:
- [ ] `0` 清行并输入固定文本段落(含 Sleep 1000 + 回车确认)
- [ ] 单击(450ms 内松开)→ 进入 CapsLock 缩写输入(`EnterCapslockAbbr`,InputHook 词表 35 条)

## 模式 2 — J 模式(`*j`,id=8,Vim 风格方向键)

- [ ] 重映射:`a`=home `g`=end `s`=left `f`=right `e`=up `d`=down `c`=backspace
  `x`=esc `r`=tab `q`=appskey `,`=delete `.`=insert
- [ ] `z/v` 按词左右(`^{left}`/`^{right}`)/ `b` 删词(`^{backspace}`)/ `t` 清行
- [ ] `2/3` 切换标签页(`^+{tab}`/`^{tab}`)/ `w` `+{tab}` / `k` 按住 LShift
- [ ] `i` 输入 "ji" / `space` 回车 / 单击 → 发送 `{blind}{j}`

## 模式 3 — F 模式(`f`,id=9,延迟 0.100)

- [ ] 重映射:`;`=end `b`=backspace `e`=esc `h`=home `i`=up `j`=left `k`=down `l`=right
  `o`=tab `q`=appskey `,`=delete `.`=insert
- [ ] `m/n` 按词右/左(`^{right}`/`^{left}`)/ `p` `^{tab}` / `u` `+{tab}` / `y` `^+{tab}`
- [ ] `w` `^{backspace}` / `s` 按住 LShift / `space` 回车 / 单击 → 发送 `{blind}{f}`

## 模式 4 — Custom Hotkeys(id=1)

- [ ] `!'` 重载(免疫挂起)/ `!+'` 暂停切换(免疫挂起)/ **`!f17` 重载(`Preprocess` 自动注入,阶段 3 已验证逐行复现)**
- [ ] `#+F23` 切换大写锁定状态(`ToggleCapslock`)
- [ ] 窗口组:`MY_WINDOW_GROUP_1` = chrome+msedge+firefox(web 浏览器组);`MY_WINDOW_GROUP__1` = 游戏组(禁用全局)
- [ ] 无按键重映射(#HotIf 块为空)

## CapsLock 缩写(35 条,`ExecCapslockAbbr` switch)

- [ ] 窗口/程序:fc FlClash / fe 资源管理器 / me Edge / ka KugouAvaloniaPlayer / kzm Kazumi
  ob Obsidian / pp PiliPlus / qq QQ / steam Steam / tg AyuGram / wx 微信 / hm HypoMux
- [ ] 路径类:dl 下载目录 / dm 工作目录 / rb 回收站 / jy 剪映专业版(绝对路径)
  ls Lossless Scaling(绝对路径)/ tb 图吧工具箱(绝对路径)/ tu Total Uninstall(绝对路径)
  et Everything(绝对路径)/ mm VSCode 打开 `D:\MyFiles\KeyFlux2`
- [ ] 系统类:sd 关机 / sl 睡眠 / re / tm 任务管理器(`^+{esc}`)/ rd 显示桌面(`#d`)
  kp 关进程 / pd 进程所在目录 / ly 蓝牙设置 / cw 智能关窗 / gd Ghost Downloader
- [ ] KeyFlux 类:se 打开设置 / ex 退出 / wt 终端打开选中路径(`-d "{selected}"`,唯一 {selected} 占位)
- [ ] 窗口激活类:zg Zed

## 分号缩写(0 条)

- [ ] `ExecSemicolonAbbr` 为空 switch,分号模式未启用(id=13 enable=false)——不生成任何代码

## 选中动作方案

- [ ] 基线(重构前)不含 `ActionSchemeList` 段;当前部署配置含 1 个空规则 scheme(enable=true)
  → 生成空的 `ActionSchemeList := Array()` + `InitActionScheme(ActionSchemeList)`,不注册任何热键
- 注记 (2026-09 方案 D 单键分发重构): 生成产物已改为 `SelectedActionData := Array(...)` +
  `SelectedActionInit(...)` (顶层 selectedAction), 旧 `ActionSchemeList`/`InitActionScheme` 不再生成
- [ ] 选中动作路径(SelectionContext/SelectedAction.ahk)须保持可用,为阶段 2+ 拆分做铺垫

## 核对方法

1. 阶段 3 验证结论(最强形式):重构后代码 + 部署配置生成的脚本与部署运行产物(260 行)
   **逐行完全一致**(含 `!f17` 自动注入),零行为变更 PASS
2. 阶段 3+:与上一阶段产物做行为 diff,逐项人工验证本清单中标注受影响的条目
3. 关键交互(模式进栈/出栈、单击检测、缩写输入)须在真实键盘上验证
