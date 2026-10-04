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

纯逻辑模块(config/skin/textbuf/geometry/easing/protocol/sound)不建窗口即可全量单测
(几何闭环 925/200/497/300、网格反解 #F7F8FC、R17 语义矩阵、皮肤 18 键等 40 项)。

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
  sound.rs         R24 触发点枚举 + SoundBackend trait
  render.rs        RenderBackend trait(唯一渲染缝,零 Win32 类型)
  win/
    app.rs             装配壳:dpi -> COM -> 皮肤 -> 单实例 -> 类注册 -> 建窗 -> 消息循环
    dpi.rs             PMv2(实测必要前提)+ R11 采集
    single_instance.rs R30 命名互斥 + 接管
    resources.rs       R25 exe 目录基点
    error.rs           R29 原版格式错误弹窗 + 终止
    audio.rs           R24 winmm PlaySoundW 后端(SND_NODEFAULT 缺文件静默)
    backend_gdi.rs     v1 渲染后端(layered alpha + 色键透明带 + DWM 阴影钩子)
    wndproc.rs         唯一 Win32->core 翻译层(R19 阴性面:分派表仅 7 分支)
```

## 活体冒烟

`D:\PortableApps\cmdinput-re\smoke.ps1`:自行启动非提权独立实例(真实实例提权,勿直接
对其发消息 —— UIPI/R31),按 pid 过滤定位窗口,走查 0x401/0x402/0x403/WM_CHAR/WM_CLOSE、
初始隐藏、925x200@497,300 几何、类名/标题、淡出后再显示。

## v2 扩展点

- `render.rs` 的 `RenderBackend` trait + `ex_style_additions()`:DComp 后端
  (D3D11+D2D1+DComp+DirectWrite)接入后恢复 `WS_EX_NOREDIRECTIONBITMAP`
  (ex-style 0x08200008 与原版全等)并消费 windowShadow* 三键(D2D1Shadow)。
- `win/app.rs` 的 `select_backend()`:`CMDINPUT_BACKEND=dcomp` 选择点已预留。
- §K 读回通道(R32-R36):`wndproc` WM_NCCREATE 的 GWLP_USERDATA 锚点已按 R32
  形态写入,textbuf 容量语义与 R33/R14 对齐 —— 落地时按 spec §K 逐条补。
