# KeyFlux-CommandInput Rust 重写报告

> ⚠️ **文档定位（2026-10-07 标注）**：本文是**重写期的设计与实施记录（v1 口径）** —— 其中
> 「模块结构」表的行数与渲染机制描述停留在 v1（`LWA_COLORKEY` + 整窗 layered alpha）。
> v1.1 起改为 `UpdateLayeredWindow` 逐像素自合成（白边/填充/阴影各自 alpha），后续又加了
> 结果列表（0x406 `KFR2` 双行版式）、搜索徽标（0x40A/0x40B）、系统文件图标缓存。
> **当前实现口径以 `command-input/README.md` + `docs/CONTRACTS.md` 为准**，本文只作历史依据。

KeyFlux 命令输入框（窗口类 `MyKeymap_Command_Input`）从原版可执行文件逆向后以 Rust 重写的
终版记录：设计决策 / 模块结构 / 验证证据 / 已知差距 / 部署与回退。

- crate 位置：`command-input/`（包名 `cmdinput`，lib = `cmdinput`，bin = `keyflux-command-input`）
- 部署形态：复制产物并重命名为 `bin\KeyFlux-CommandInput.exe`，引擎零改动
- 行为规格与评审工件在仓库外 `D:\PortableApps\cmdinput-re\`（spec.md / design-A|B|C.md /
  matrix.md / 探针脚本 / 截图），有意不入库
- 参考实现（皮肤数学移植源）：
  `doc\reference\EverythingQueryEdit.ahk`（2026-10-04 自插件树收编的只读快照）

---

## 0. 摘要

| 项 | 原版 | 重写 |
|---|---|---|
| 体积 | 580,096 字节 | **197,120 字节**（−66%） |
| 形态 | 单 exe（逆向对象） | 单 crate：bin 薄壳 + lib 模块，19 个源文件 2,414 行 |
| 依赖 | — | 仅 `windows` crate（=0.62.2，本机缓存实测可用版本） |

**drop-in 判据**（spec.md:19）：窗口类名 + exe 名 + 消息语义 + 标题 —— 四项全部保持：

- 类名 `MyKeymap_Command_Input` 逐字节（`src/config.rs:10`，strings.txt:7596）
- 标题为单空格 `" "`（`src/config.rs:13`，R4/R19）
- 消息协议 0x401 显示 / 0x402 淡出隐藏 / 0x403 幂等隐藏 / WM_CHAR 文本写入
  （`src/config.rs:20-27`，R7/R14-R17）
- 互斥名逐字符（`src/config.rs:16`，R30）

引擎对接点（零改动证据）：`bin/KeyFlux.ahk:75` `Run("bin\KeyFlux-CommandInput.exe")`
启动拉起；`bin/lib/core/Functions.ahk:38` `KeyFluxExit()` 内
`ProcessClose("KeyFlux-CommandInput.exe")` 退出收尾；输入通道经
`lib/core/CommandInputHooks.ahk`（`KeyFlux.ahk:15`）按类名/标题定位窗口投递消息。

---

## 1. 设计决策

### 1.1 选型：方案 C（模块化优先）

三份候选设计（A：贴近原版单包；B：DirectComposition 重渲染；C：`RenderBackend` trait +
GDI 首版落地 + DComp 渐进补齐）经八准则字典序评审，矩阵 v2 定案 C 胜出：

- 最高优先准则 ① 模块化：C=5（trait 定义体内含 DComp 侧行为约定三处，GDI 后端完整按
  trait 设计示范，core 零 Win32 污染）严格领先 B=4（DcompRenderer 同为纸面设计，且
  design-B.md:446 自认「换后端要重写 device+scene 大半」）与 A=3（单包文件级隔离 +
  mirror.rs 自认脏面）。
- 交叉校验：④ 视觉确定性不劣于 A（机制组合与参考实现同款）；⑤ 成本居中
  （v1 同口径 ≈11~12.5 人日 vs A 5 / B 16；终态全等 ≈16.5~18 ≈ B，无终态成本优势，已如实标注）。
- 评审含反方八项攻击逐条复裁，全文见 `D:\PortableApps\cmdinput-re\matrix.md`（§5）。

### 1.2 形态：4-crate workspace 折叠为单 crate

设计 C 原案为 4-crate workspace；按任务工程要求折叠为单 crate（bin 薄壳 + lib 模块），
模块边界按 design-C §2.4/§2.7 原样保留：纯逻辑模块零 Win32 类型，可脱离窗口 `cargo test`。

### 1.3 对设计文档的三处有意偏差（均有会话证据）

1. **42px 透明带/圆角用 `LWA_COLORKEY`，而非 design C §2.5 主案的
   `SetWindowRgn`+`DwmExtendFrameIntoClientArea`**。依据：design A 探针实证 region 在
   `SetLayeredWindowAttributes` 分层窗上不参与合成；参考实现
   `EverythingQueryEdit.ahk:159`（`WinSetTransparent` = LWA_ALPHA 分层；收编副本见
   `doc\reference\`，行号同旧版）+ :76-78（每次
   Show 内依次 `_RoundAll` :241-244 `CreateRoundRectRgn`+`SetWindowRgn` 与
   `FrameShadow` :252-262）正是同一机制组合，且经多轮 Δ2 活体验收。DWM 阴影钩子保留
   （分层窗上仍有效，截图证实）。
2. **`RenderBackend`/`SoundBackend` trait 增加默认方法 `on_hidden()`**：alpha 复原须在
   `SW_HIDE` 之后（design A §2.10），否则破坏 R10-5（引擎直接 WinShow 复显时整窗透明）。
3. **`Command::ClearText` 并入 `on_event` 的状态变更**：语义等价，协议测试覆盖。

### 1.4 未定案项处置

- **§K 读回通道（R32-R36）按设计不实现**（`src/lib.rs:22`）：扩展点已留——
  `WM_NCCREATE` 写 `GWLP_USERDATA` 锚点（`src/win/wndproc.rs`），textbuf 容量语义与
  R33/R14 对齐。
- **`windowShadow*`/`corner*` 皮肤键解析存储、v1 不渲染**：v2 DComp 后端消费三键；
  v1 由保留的 DWM 阴影钩子兜底观感。
- **`CMDINPUT_BACKEND=dcomp` 选择点已留**：值非 dcomp 一律回退 gdi。

---

## 2. 模块结构

`wc -l` 实测（2026-10-04，含活体修复增量），19 文件共 **2,414 行**：

| 文件 | 行数 | 职责 |
|---|---:|---|
| `src/main.rs` | 7 | 薄壳（仅调 `win::app::run`；release 无控制台） |
| `src/lib.rs` | 38 | 库出口与模块图注释 |
| **纯逻辑层（零 Win32，可脱离窗口测试）** | | |
| `src/config.rs` | 123 | 契约常量集中：类名/互斥名/标题/0x401-3/WM_CHAR/几何标定 33.6·160·40.0·20·44.0/色键（附常量锁定单测） |
| `src/skin.rs` | 354 | 皮肤 18 键 fail-safe 解析 + 参考实现 :158-159/:199-215 逐行移植的整窗 alpha/网格反解/混色 |
| `src/textbuf.rs` | 119 | R17/R22：无白名单追加/退格/清空不缩容 |
| `src/geometry.rs` | 166 | R11 截断取整公式/R13 白框 42/网格 25+24/字号 55；活体修复新增 `text_pitch_px` 逐字形固定步距排版 |
| `src/easing.rs` | 48 | R15 Accelerate-Decelerate 0.5/0.5（smoothstep） |
| `src/protocol.rs` | 296 | R14-R19/R22 事件分派状态机 `AppEvent`→`Command` |
| `src/sound.rs` | 45 | R24 四触发点 + `SoundBackend` trait |
| `src/render.rs` | 89 | `RenderBackend` trait（唯一渲染缝；零 Win32 类型；含 `on_hidden()` 默认方法） |
| **壳层 `src/win/`（windows crate 唯一出口）** | | |
| `win/app.rs` | 164 | 装配壳：dpi→COM→皮肤→单实例→类注册→建窗→消息循环 |
| `win/wndproc.rs` | 267 | 唯一 Win32→core 翻译层；分派表仅 WM_NCCREATE/CREATE/0x401/0x402/0x403/WM_CHAR/PAINT/DESTROY，其余一律 `DefWindowProc` |
| `win/backend_gdi.rs` | 458 | v1 渲染后端：layered alpha+色键+DWM 阴影钩子+网格+44DIP 粗体文字+阻塞淡出；活体修复 `lwa()` 单次双 flag、逐格 `DT_CENTER`、显示层 `CharUpperW` |
| `win/dpi.rs` | 49 | PMv2（实测必要前提）+ R11 采集 |
| `win/single_instance.rs` | 37 | R30 命名互斥 + 接管 |
| `win/resources.rs` | 36 | R25 按 `GetModuleFileNameW` exe 目录解析资源（与 CWD 无关） |
| `win/error.rs` | 30 | R29 原版格式错误弹窗 + 终止 |
| `win/audio.rs` | 63 | R24 winmm `PlaySoundW` 后端（SND_NODEFAULT 缺文件静默） |
| `win/mod.rs` | 25 | 模块声明 |

> 行数对账：实施报告逐文件数合计 2,345（含 lib.rs），活体验证修复 +83
> （geometry.rs 149→166 排版定案、backend_gdi.rs 406→458 lwa/排版/大写化）= 今日 2,414。

---

## 3. 构建与测试

本机 rustc 的 VS 自动检测失效（VS "18" 未注册 vswhere 所查组件），**必须先 source
`config-ui-reactor/env.ps1`**（`D:\PortableApps\cmdinput-re\build.ps1` 已封装）：

```powershell
# 构建
powershell -NoProfile -ExecutionPolicy Bypass -File D:\PortableApps\cmdinput-re\build.ps1
# 测试
. D:\PortableApps\KeyFlux-main\config-ui-reactor\env.ps1
cargo test --manifest-path D:\PortableApps\KeyFlux-main\command-input\Cargo.toml
```

- 实施会话：`cargo build --release` exit=0 零警告；产物 197,120 字节。
- **收尾会话复跑（2026-10-04）**：`cargo test` → **41 passed / 0 failed, EXIT=0**
  （含新增 `text_pitch` 标定测试；实施会话为 40 项，活体修复后 +1）。
- 部署产物 sha256：`5cc58d4a102da9e9b9feab7b56b208f885dde3141a4eb05b8f79385fd54e89cb`。

---

## 4. 验证证据（三层）

> 4.1 为收尾会话复跑；4.2/4.3 由本工作流实施/活体验证 ask 执行，收尾会话未复跑，
> 结论为前序 ask 报告转引（证据文件路径均已实地核存）。

### 4.1 单元测试（收尾复跑）

`cargo test` 41/41 PASS：几何闭环（925×200@497,300，125% DPI）、网格反解
#F7F8FC、皮肤 18 键逐键 fail-safe、R17 语义矩阵、常量锁定、text_pitch 标定等。

### 4.2 活体冒烟 12/12（smoke.ps1）

自行启动非提权独立实例，按 pid 过滤定位窗口（前置安全检查确认真实提权实例不受触碰，
顺带实证 R31 的 UIPI 现实）：

窗口隐藏态可发现 ✓ 类名逐字节 ✓ 标题单空格 ✓ 初始隐藏 ✓ 0x401→可见 ✓
0x402→淡出后隐藏 ✓ 0x403 幂等隐藏且进程存活 ✓ 0x402 后再 0x401 正常显示（alpha 已复原）✓
WM_CHAR 混发 ASCII/CJK/退格不崩 ✓ WM_CLOSE→退出码 0 ✓
`GetWindowRect` = 925×200@497,300（125% 闭环精确命中）✓
截图（`cmdinput-re/out/smoke/shot1.png`）可见白框+网格+圆角+42px 透明带+DWM 阴影灰晕。

### 4.3 活体协议 + 像素对拍 27/27（probe_live.py / probe_compare.py）

独立部署实例（livedeploy）× 非提权 Python ctypes 探针 × 与原版 exe 同屏像素对比。
要点摘录：

- R3/R4/R19：类名逐字节、样式位 `GWL_STYLE=0x84000000`、`GWL_EXSTYLE` 含
  TOPMOST|NOACTIVATE|LAYERED（NOREDIRECTIONBITMAP 按 spec 附录C#11 许可放弃）、
  标题 WM_GETTEXT 单空格、WM_SETTEXT 只改标题不动文本 bbox（R19 隔离铁证）。
- R11/R12：窗口矩形与闭环值精确一致；0x401 后 rect 不变。
- R14/R15/R16：0x401 0.04s 可见且幂等、不抢前台（R9）；0x402 淡出 0.385s
  （原版 0.386s）后隐藏、不清空（SW_SHOW 复显 bbox 完全复现）；0x403 0.010s 立即隐藏。
- R17：WM_CHAR 逐字符注入 ASCII/CJK/退格，PrintWindow 截图确认上屏；隐藏态静默累积。
- R18：WM_CLOSE → 进程退出 rc=0。
- 像素级（净观感 ImageGrab）：净背景 Δ2（rw 232,232,232 vs orig #E6E6E6）；alpha 反解
  0.86 vs 0.89（同 0.9 机制）；网格间距 25.0px、首线偏移 24px、横线 y=66/91/116/141
  逐一相同；文字 bbox 差 ≤2px、水平/垂直中心一致；圆角 ~10。
- **活体发现并修复 3 项偏差后复测全绿**：
  1. 【must】`SetLayeredWindowAttributes` 分两次调用时后一次重置前一次属性
     （色键调用清掉 alpha → 净背景 #FFFFFF）——修复为单次调用双 flag 同发
     （`backend_gdi.rs` `lwa()` helper，init/fade_out/on_hidden 统一）；
  2. 【must】原版为逐字形固定步距排版（8 字符 pitch 总和 100.07px/字 = 4×网格步距，
     字形居中于格）——修复 `geometry.rs text_pitch_px` + `backend_gdi.rs` 逐格
     `DT_CENTER`，复测 bbox 差 ≤2px；
  3. 【should】原版对 'abc' 渲染大写字形——修复为显示层 `CharUpperW` 大写化
     （文本缓冲不动，R17/R22 不受影响）。
- 证据文件：`cmdinput-re/live-shots/`（`rw_*`/`orig_*` 成对：空框净观感、文字、
  PrintWindow 原图、字形放大、排版定案证据）；探针脚本
  `probe_live.py`/`probe_diag.py`/`probe_compare.py` 在 `cmdinput-re/`。

---

## 5. 已知差距与风险（如实清单）

1. **§K 读回通道（R32-R36）未实现**——设计明确本轮不做；扩展点已留（§1.4）。
2. **R24 音效未做听感/录制验证**——四触发点映射经代码走查
   （`sound.rs`/`audio.rs`/`backend_gdi.rs` 调用点）+ 单测覆盖。
3. **NOREDIRECTIONBITMAP 样式位放弃**——spec 附录C#11 明示许可（LAYERED 为 GDI 后端
   透明机制），非缺陷。
4. **42px 带内无 DWM 阴影**——分层窗无区域合成，design A 声明的 should 级降级；
   带外阴影经 DWM 钩子保留（截图证实）。
5. **网格线垂直范围微差**——orig 至 y≈145、重写至 y≈157（should 级，需逐像素才可见）。
6. **R25 CWD 不匹配启动场景未单独活体**——按 `GetModuleFileNameW` exe 目录实现
   （`resources.rs`），常规与 CWD 不匹配场景均依赖该基点。
7. **单实例接管的提权并存现实（R31）**——非提权新实例对提权存量实例的接管 WM_CLOSE
   被 UIPI 过滤，两进程并存；Z 序保证引擎 `FindWindowW` 命中后建实例
   （部署实录 §6 中两度实测）。
8. **字体缺失即终止（R29）**——部署必须与 `bin\font`、`bin\sound`、
   `CommandInputSkin.txt` 同层（资源一律按 exe 目录解析）。
9. **0x402 水平位移分量**——spec 附录B 未定案，未实现（不强求）。

---

## 6. 部署与回退

### 6.1 部署实录（2026-10-04 收尾会话）

目标：`D:\PortableApps\KeyFlux\bin\KeyFlux-CommandInput.exe`。运行时依赖
`CommandInputSkin.txt`/`font\font.ttf`/`sound\`（4 文件）部署前已核存。

| 步骤 | 命令/操作 | 结果 |
|---|---|---|
| 备份 | `cp bin\KeyFlux-CommandInput.exe → KeyFlux-CommandInput.exe.upstream.bak` | ✅ 580,096 字节，sha256 `2aed3232…c935fbe` |
| 劝退存量实例 | 对 pid 12456 窗口 `PostMessageW(WM_CLOSE)` | ❌ 进程存活 → UIPI 过滤（提权实例，实测实证 §5-7） |
| 停引擎（ask 处方） | `pwsh Stop-Process -Name KeyFlux -Force` | ❌ 「拒绝访问」——引擎 pid 14164 为提权进程，非提权 shell 不可停 |
| 替换 | 直接覆盖 `cp` 重写产物 | ✅ 成功（该文件未被任何进程锁定，实测证明 pid 12456 并非从 bin 路径运行）；sha256 `5cc58d4a…e89cb` 与 `cmdinput-re\out` 产物逐字节一致 |
| 重启引擎 | 主会话以提权 shell 执行 Stop-Process + Start-Process | ✅ KeyFlux.exe 13:29:10 启动（新 pid 19164） |
| 拉起验证（收尾会话只读复测） | `EnumWindows` 按类名过滤 + `tasklist` | ✅ 全系统**唯一** `MyKeymap_Command_Input` 窗口，属 pid 17996 = 引擎启动时按 `KeyFlux.ahk:75` 从 bin 拉起的新进程；提权孤儿 12456 已消失 |

> 注意：经非提权 shell 重启的引擎将以非提权运行；本次由主会话提权重启，无此问题。

### 6.2 回退方式

```powershell
Stop-Process -Name KeyFlux -Force          # 停引擎（会级联 ProcessClose 命令框）
Copy-Item D:\PortableApps\KeyFlux\bin\KeyFlux-CommandInput.exe.upstream.bak `
          D:\PortableApps\KeyFlux\bin\KeyFlux-CommandInput.exe -Force
Start-Process D:\PortableApps\KeyFlux\KeyFlux.exe -WorkingDirectory D:\PortableApps\KeyFlux
```

原版二进制完整保存在 `KeyFlux-CommandInput.exe.upstream.bak`
（sha256 `2aed32327faf64323ec25c9361d23fa460ef5cf59e1fb78dd34782190c935fbe`）。

### 6.3 复现构建

见 §3；产物 `target\release\keyflux-command-input.exe` 复制重命名为
`KeyFlux-CommandInput.exe` 即 drop-in。

---

## 7. 参考文档

- 行为规格（R1-R37，24 must）：`D:\PortableApps\cmdinput-re\spec.md`
- 设计三案与评审矩阵：`design-A.md` / `design-B.md` / `design-C.md` / `matrix.md`（v2，含反方攻击复裁）
- 逆向中间产物：`wndproc.md`、`imports.txt`、`strings.txt` 等（`cmdinput-re/`）
- 参考实现：`doc\reference\EverythingQueryEdit.ahk`（收编快照，见该目录 README）
