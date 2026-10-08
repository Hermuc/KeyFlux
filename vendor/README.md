# vendor —— 第三方随包内容清单（含「勿改清单」）

> **为什么有这份文件**（多维度优化报告 #13）：本仓混着若干**第三方**文件（一个 AHK 类、引擎
> 运行时、一个小工具，以及一个被 fork 的 crate）。它们看起来像自家源码，于是「顺手修正上游拼写 /
> 删掉看着像死代码的分支」很容易发生，而且**不留痕迹**。[Monitor.ahk](../bin/lib/Monitor.ahk)
> 头部的告诫（`retreive` / `currentRefue` 等上游拼写属 vendored 原样特征，勿"修正"）证明这类坑
> 真实存在过。
>
> 本文件把这些内容集中登记，并由 [`tools/check-vendor-hashes.ps1`](../tools/check-vendor-hashes.ps1)
> 在 CI 逐个 hash 校验：**任何改动都必须显式更新清单**（`-Write`），从而把"静默漂移"变成"有记录的决定"。

## 清单一览

| 路径 | 内容 | 来源 | 许可 | 备注 |
|---|---|---|---|---|
| `bin/lib/Monitor.ahk` | Monitor 配置 WinAPI 包装类 **v2.4.1** | [tigerlily-dev/Monitor-Configuration-Class](https://github.com/tigerlily-dev/Monitor-Configuration-Class) | The Unlicense | 上游代码**零改动**；仅在文件头追加 KeyFlux 注释块 |
| `bin/AutoHotkey64.exe` | AutoHotkey **v2.0.19** 运行时 | autohotkey.com | GPL-2.0 | 引擎运行时；版本单一真源 = `Makefile` 的 `ahkVersion` |
| `bin/SoundControl.exe` | 音量 / 静音控制 CLI | ⚠️ **来源未记录（待补）** | 未知 | 被 `bin/lib/actions/builtins/type2_system.ahk` 的 `SoundControl()` 调用 |
| `tools/Rexplorer_x64.exe` | 重启资源管理器 v1.7 | [sordum.org/9192](https://www.sordum.org/9192/restart-explorer-v1-7/) | 免费软件 | 见 [`tools/工具来源.txt`](../tools/工具来源.txt) |
| `config-ui-reactor/vendor/windows-reactor` | `windows-reactor` **0.100.0** 的本地 fork（P1–P8 补丁） | crates.io / microsoft/windows-rs | MIT OR Apache-2.0 | 补丁面见其 [`PATCHES.md`](../config-ui-reactor/vendor/windows-reactor/PATCHES.md)；上游新版由 `.github/workflows/deps-watch.yml` 监控 |

> `bin/KeyFlux-CommandInput.exe` **不在此列**：它是自研 Rust 产物（源码 `command-input/`），
> 不是第三方。它的"是否当前源码构建"由部署链保证，不属于本清单范畴。

## 勿改清单（Monitor.ahk）

以下均为**上游原样特征**，改动会破坏与 `tigerlily-dev v2.4.1` 的可 diff 性：

- 上游拼写：`retreive`、`currentRefue`；
- `GetMonitorBrightness` 内的调试 `MsgBox "Failed"`；
- `SaveCurrentMonitorSettings` 内实际调用的是 `RestoreMonitorFactoryDefaults`（名字与行为不符）；
- 约 580 行"未使用面"（contrast / gamma ramp / RGB drive & gain / display area / VCP / sharpness /
  color temperature / technology type / power mode / degauss / restore factory / capabilities 系列）
  **是为保持可 diff 而刻意保留**，勿当死代码删除。

**唯一例外**：`GetMonitorBlueDrive` 内同款调试 `MsgBox` 已于 2026-09-13 经用户批准清理
（`throw` 保留）。改动该处时须同步更新本清单与 hash。

## 如何更新

1. **有意变更**某个登记项后：

   ```powershell
   pwsh -NoProfile -ExecutionPolicy Bypass -File tools/check-vendor-hashes.ps1 -Write
   ```

   重新生成 `tools/vendor-manifest.json`；
2. 在本次提交里说明**为什么**改（上游升版 / 批准过的清理 / 新增第三方文件）；
3. 新增第三方文件时，先在 `tools/check-vendor-hashes.ps1` 的 `$targets` 里登记（含 kind / path /
   exclude），再 `-Write`；
4. 门禁：CI 的 `analyzers.yml` → `vendor-hashes` job（`make check-vendor`）。本地同款：`make check-vendor`。