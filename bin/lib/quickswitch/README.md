# quickswitch/ —— QuickSwitch 模块边界

「快速切换」功能的 AHK 引擎内模块, 严格单向依赖 (无环), 由 `keyflux.tmpl` 按
「内层 → 外层」顺序 `#Include`。

| 文件 | 唯一职责 / 危险面 |
|---|---|
| `FolderRanker.ahk` | **纯函数**: 排序、相对时间。无窗口 API / 无 COM / 无磁盘。 |
| `HistoryStore.ahk` | **唯一磁盘**: 历史 TSV 读写 (UTF-8 显式)、裁剪、清空、前缀排除。 |
| `FolderHistory.ahk` | **唯一 Shell COM**: 枚举资源管理器当前目录 (≈16 ms, 必须离开热键回调)。 |
| `DialogInspector.ahk` | **唯一 Win32/窗口**: 判别式、读目录 (双变体)、跳转 (双闸门)。 |
| `QuickSwitchUI.ahk` | 浮层 (Gui + ListView)。**不知道 hwnd**: 入参=候选数组+锚点矩形, 出参=选定路径。 |
| `QuickSwitch.ahk` | 编排; **唯一持有对话框 hwnd**; 800 ms 轮询 + 会话状态机。 |

## 依赖方向 (严格单向)

```
Template  -> QuickSwitch -> {DialogInspector, FolderHistory, HistoryStore, FolderRanker, QuickSwitchUI}
                              \______________________________________________/
T9(type9_keyflux.ahk) -> QuickSwitch          UI 只依赖 core/*
Core(bin/lib/core/*)  <- 各层只读依赖
```

- UI 与 Ranker **永不持有对话框句柄** (为阶段 2 sidecar 迁移而设计)。
- 下层只通过**返回值**向上层汇报, 不反向调用上层。
- **入口命名**: 生成端 `callMap[9]` 调用的公开符号是 `QuickSwitchGoto()`, 它定义在
  `type9_keyflux.ahk` (薄壳, 转调编排层)。因 AHK 全局函数命名空间唯一, 编排入口在
  `QuickSwitch.ahk` 中名为 `QuickSwitchRun()` (语义等价于设计 N1 的 `QuickSwitchGoto`)。

## 命名前缀 (避免全局函数冲突, 交由 tools/lint_ident.py 静态闸门守护)

`Rank*` / `Hist*` (Store) / `HistCollect*`·`HistLast*`·`HistNote*` (FolderHistory) /
`Dlg*` / `QSUI*` / `QuickSwitch*`·`InitQuickSwitch`·`_QuickSwitch*`。

## 安全红线 (实现层强制)

1. 全路径零 `Esc` 发送 (实测会关闭用户对话框)。
2. 跳转只走 `Alt+D`(首选)/`Ctrl+L`(回退) + `SendText` + `Enter`; 禁用「控件直写」式赋值。
3. 发键前双闸门: `DirExist(path)` **且** `WinActive(dialogHwnd)`。
4. 采集绝不在热键回调内同步执行。
5. 浮层显示期 `Suspend(true)`, 隐藏后 `Suspend(false)`。
6. 读当前目录首选直读地址栏面包屑窗口文本 (免聚焦免发键), `Ctrl+L` 仅回退。
7. 日志与 TSV 写入显式 `"UTF-8"`。
8. 所有 Win32/COM 调用 try/catch: 失败静默 + 一次性 Tip, 绝不抛进热键链路。
9. 绝不 `Run`/`ShellExecute` 历史字符串; 注入前拒绝含 `\r\n\t\0` 的路径。

## 独立测试注意

各文件头部含 `#Warn All, Off` —— 单独 `/Validate` 这些文件时, 交叉模块调用会被 AHK
判为「未定义函数」并在**加载期弹 #32770 警告框**阻塞进程 (该类别恰是文件对话框的窗口类)。
关掉警告后每个文件可独立 `/Validate` exit=0。
