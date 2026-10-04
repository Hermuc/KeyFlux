# KeyFlux-CommandInput (Rust rewrite)

KeyFlux 命令输入框(窗口类 `MyKeymap_Command_Input`)的 Rust 重写。部署为
`KeyFlux-CommandInput.exe` 即为引擎零改动的 drop-in 替换(判据 = 窗口类名 +
exe 名 + 消息语义 + 标题 `" "`,spec.md:19)。

- 行为规格:`D:\PortableApps\cmdinput-re\spec.md`(R1-R37,24 must)
- 选定设计:方案 C(模块化优先:`RenderBackend` trait + GDI 首版落地 + DComp 渐进补齐),
  `D:\PortableApps\cmdinput-re\design-C.md`
- 皮肤数学移植源:`D:\PortableApps\KeyFlux-main\data\plugins\everything_search\src\EverythingQueryEdit.ahk`

## 构建(本机必须经 env.ps1)

rustc 的 VS 自动检测在本机失效(见 `config-ui-reactor/env.ps1` 头注),必须先 source 环境:

```powershell
# 方式一:一键脚本
powershell -NoProfile -ExecutionPolicy Bypass -File D:\PortableApps\cmdinput-re\build.ps1

# 方式二:等价手工流程
. D:\PortableApps\KeyFlux-main\config-ui-reactor\env.ps1
cargo build --release --manifest-path D:\PortableApps\KeyFlux-main\command-input\Cargo.toml
```

产物:`target\release\keyflux-command-input.exe` → **复制并重命名为
`KeyFlux-CommandInput.exe`** 放入引擎 `bin\`(与 `CommandInputSkin.txt`、`sound\`、
`font\` 同层;资源一律按 exe 目录解析,R25,与 CWD 无关)。

## 测试

```powershell
. D:\PortableApps\KeyFlux-main\config-ui-reactor\env.ps1
cargo test --manifest-path D:\PortableApps\KeyFlux-main\command-input\Cargo.toml
```

纯逻辑模块(config/skin/textbuf/geometry/easing/protocol/sound/results/compose)不建窗口即可全量单测
(几何闭环 925/200/497/300、网格反解 #F7F8FC、R17 语义矩阵、皮肤 18 键、结果列表窗口/滚动/
0x406 载荷编解码、逐像素合成 (圆角 AA/白边不透明/填充半透明/阴影只落框外) 等 74 项)。

## 模块图(bin 薄壳 + lib)

```
src/
  main.rs          薄壳(仅调 win::app::run;release 无控制台)
  lib.rs           库出口
  config.rs        契约常量集中(类名/互斥名/消息值/几何标定)—— 修改前必读 spec
  skin.rs          皮肤 18 键 fail-safe 解析 + 合成数学(R26/R27/R28)
  textbuf.rs       文本状态机(R17/R22:无白名单追加/退格/清空不缩容)
  geometry.rs      R11/R13/R28 几何公式(截断取整;125% 闭环)
  easing.rs        R15 Accelerate-Decelerate 0.5/0.5
  protocol.rs      R14-R19/R22 事件分派状态机(AppEvent -> Command)
  results.rs       结果列表面板模型 + 0x406 载荷编解码(纯逻辑; 与 AHK 侧逐字节对齐)
  compose.rs       逐像素合成(圆角矩形 SDF/覆盖率 AA/高斯阴影/预乘) —— 白边与填充各自 alpha
  sound.rs         R24 触发点枚举 + SoundBackend trait
  render.rs        RenderBackend trait(唯一渲染缝,零 Win32 类型)
  win/
    app.rs             装配壳:dpi -> COM -> 皮肤 -> 单实例 -> 类注册 -> 建窗 -> 消息循环
    dpi.rs             PMv2(实测必要前提)+ R11 采集
    single_instance.rs R30 命名互斥 + 接管
    resources.rs       R25 exe 目录基点
    error.rs           R29 原版格式错误弹窗 + 终止
    audio.rs           R24 winmm PlaySoundW 后端(SND_NODEFAULT 缺文件静默)
    backend_gdi.rs     v1.1 渲染后端(逐像素 alpha 自合成: UpdateLayeredWindow + compose, 见下)
    wndproc.rs         唯一 Win32->core 翻译层(R19 阴性面 + 0x404/0x405/0x406-0x408 + 结果区鼠标)
```

## 活体冒烟

`D:\PortableApps\cmdinput-re\smoke.ps1`:自行启动非提权独立实例(真实实例提权,勿直接
对其发消息 —— UIPI/R31),按 pid 过滤定位窗口,走查 0x401/0x402/0x403/WM_CHAR/WM_CLOSE、
初始隐藏、925x200@497,300 几何、类名/标题、淡出后再显示。

## v2 扩展点

### v1.1 已落地：逐像素 alpha 自合成（2026-10-04，样式还原）

原版是 DirectComposition 自合成（每个视觉各带 opacity ⇒ 白边可以不透明、填充可以半透明、
两者之上还有 D2D1Shadow）；v1 的整窗 `LWA_ALPHA` 表达不出这三点（用户报障「白边没有 /
不透明度偏高 / 没有阴影」）。v1.1 用 `UpdateLayeredWindow` + 32bpp 预乘 DIB 做等价物：

- GDI 只画**内容色**（背景/网格/文字/结果行）到 DIB；alpha 由 `compose::composite` 按几何
  解析式写入并就地预乘（GDI 不写 alpha 通道，这是本路径唯一的硬约束）；
- 白边 = 框体最外 `borderWidth` 像素（**保留小数**：3 DIP @125% = 3.75px，实测「3 满 + 1 弱」）；
- 填充净不透明度 = `1 − (1 − backgroundOpacity) × 0.55` = **0.945**（`skin::fill_alpha`）；
- 面板内容色 = 皮肤色×`backgroundOpacity` / 净不透明度 = **242.9**（`skin::panel_content_color`）——
  两条合起来的净观感精确等于原版的 `out = 皮肤色×b + (1−b)×0.55×bg`（白底 243.5 / 黑底 229.5，
  与原版实测 243 / 229 逐项吻合）。**填充色不是纯白**：画纯白会让白底上「与背景同化、失去实体感」；
- 阴影 = 圆角矩形轮廓的高斯（`windowShadowSize/Opacity/Color` 三键首次被消费），**只落框外**，
  σ≈3.0px / 峰值 0.30（原版框外剖面 1..10px = 50,45,33,24,16,10,6,3,1,0）；
- 其余内容色（网格/文字/列表底纹）一律**在内容色空间**派生（基准 = `panel_content_color` 而非
  皮肤原色，见 `skin::grid_content_color` / `themed`），否则会整体偏亮一档；
- `SetWindowRgn` 仍在但**扩展到框外阴影带**（区域同时管合成与命中，不扩就把阴影裁掉）；
- 🔴 **`UpdateLayeredWindow` 的 `pptdst/psize` 必须显式传**：本机实测两者都传 NULL 时函数
  返回 TRUE、DIB 内容正确、窗口 `IsWindowVisible=TRUE`，但**屏幕上一个像素都不出现**
  （`%TEMP%\kf_list_smoke\ulw_probe.py` 隔离对照 + `verify_visible.py` 差分 99.3% 确认）。
- ⚠ **本路径没有、也不做背景模糊**（2026-10-04 三轮「毛玻璃」尝试后经用户裁定**回退**）：
  内部视觉就是半透明的（0.945），与**真实桌面**逐像素混合 —— 原版（DComp 自合成）同样不采样
  背景。看似「糊」的观感来自 AA 圆角 + 白边内沿 AA + 真实高斯阴影 + 半透明填充四处。
  若要**真**毛玻璃（背景文字不可辨），那是另一条机制：DComp 后端 + 系统 Acrylic/Mica（见下节
  「仍未落地」）。在 ULW 路径上叠 `SetWindowCompositionAttribute` 无解 —— 隔离探针实测两者
  语义互斥（只多一层噪声纹理，背景细节完全没被模糊）。
- 验证：两套互补探针（都在 `%TEMP%\kf_list_smoke\`，且都要求**引擎全停**腾出命名互斥）：
  * `ab_style.py` + `ab_rows.py` —— **原版 vs 新版同背景活体 A/B**（受控背景 = 白 255 / 浅底深字 /
    黑 0 三条带；`Lw = α·F + (1−α)·255` 与 `Lb = α·F` 联立解 α、F，不依赖插值）：
    净 alpha **0.055→0.055**、透过率 **0.072→0.071**、阴影剖面 50,45,33,24,16,10 vs 53,43,33,24,15,9 ——
    全部落在 1–3 级内；同时它也是**推翻上一轮标定**的证据来源（见下条 ⚠）。
  * `probe_style.py` —— 真实桌面渲染回归 **11/11**（四边各 3px 纯白、有效 alpha **0.945**、
    阴影单调衰减且 40px 无外溢、圆角 AA dx 11/4/2/1/0）。
  🔴 抓屏验证分层窗口**必须带 `CAPTUREBLT`**（Pillow `ImageGrab` 的无 CAPTUREBLT 路径会漏整窗）。
  🔴 反解面板不透明度时必须用**真实的填充色 F**（= `panel_content_color` 242.86），用 255 会系统性偏低。
- ⚠ **已作废的旧标定**（2026-10-04 白天版）：曾按「框上/框下两条桌面带垂直插值」解出原版
  有效 alpha 0.784 并因此取「填充 = `backgroundOpacity²`」。该方法假设背景沿垂直方向平滑，
  在文字密集的背景上失效（对纯白底实测 243，按 0.784 预测应 248，差 5 级）⇒ 被同背景 A/B 取代。

### 仍未落地

- `render.rs` 的 `RenderBackend` trait + `ex_style_additions()`: DComp 后端
  (D3D11+D2D1+DComp+DirectWrite)接入后恢复 `WS_EX_NOREDIRECTIONBITMAP`
  (ex-style 0x08200008 与原版全等); 届时阴影可换 D2D1Shadow、背景模糊可走系统 Acrylic。
- `win/app.rs` 的 `select_backend()`:`CMDINPUT_BACKEND=dcomp` 选择点已预留。
- §K 读回通道(R32-R36):`wndproc` WM_NCCREATE 的 GWLP_USERDATA 锚点已按 R32
  形态写入,textbuf 容量语义与 R33/R14 对齐 —— 落地时按 spec §K 逐条补。
