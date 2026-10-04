# 17 - 全局字体回归 MiSans

日期：2026-09-28。用户问：面板字体（不含 command 命令框）是否同重构前一样为 MiSans？

## 结论（改前）

**不是**。`theme.rs` 的 `UI_FONT = "Segoe UI Variable..."` 为零引用死常量，全 UI 实际
渲染 WinUI Fluent 默认字体（Segoe UI Variable）。旧版为 MiSans 四字重随应用打包，
经 `ContentControlThemeFontFamily` 资源键全局覆盖 Fluent 模板（退役源码
`App.axaml`，git `1f3dc9f~1`）。

## 为什么必须 fork reactor

reactor 0.100.0 **没有任何字体设置通道**：
* 无 `font_family` builder（TextBlock/Control 均未生成该属性）；
* `ResourceOverrides` 仅支持 Color/CornerRadius/Thickness；
* `AppContext` 不暴露 `Application.Resources`。

## 实现（vendor 补丁）

1. **字体入库**：`config-ui-reactor/resources/fonts/MiSans-{Regular,Medium,Semibold,Bold}.ttf`
   （从 git 历史 `1f3dc9f~1:config-ui-avalonia/Assets/Fonts` 提取，字节数核对一致；
   Twemoji/MDI 不需要——前者命令框用、后者是 Avalonia 图标字体）。
2. **vendor fork**：windows-reactor 0.100.0 拷入 `config-ui-reactor/vendor/windows-reactor/`，
   根 `[patch.crates-io]` 指向。补丁面仅两处（其余与上游逐字一致）：
   * `src/native/winui/app_shim.rs` 新增 `install_global_ui_font`：`XamlReader::Load`
     解析含 `<FontFamily x:Key="ContentControlThemeFontFamily">MiSans, Microsoft YaHei UI,
     Segoe UI Emoji</FontFamily>` 的 ResourceDictionary，Append 进
     `Application.Resources.MergedDictionaries`（后加优先，压过 Fluent 字典；与旧
     App.axaml 同键同值）。`XamlReader::Load` 是上游已有绑定 ⇒ 无需手工补
     FontFamily 工厂/IMap IID。
   * `src/app.rs` 装资源处加一行调用（`install_xaml_controls_resources` 之后）。
3. **进程内私有加载**：`src/platform/fonts.rs` 经 GDI `AddFontResourceExW(…, FR_PRIVATE, …)`
   把随包 ttf 装入本进程字体表（不写系统目录/注册表；卸载即消失），在 `main()` 最早期调用；
   目录探测 `<exe>/fonts` →（开发态）`resources/fonts`。
4. **发布链路**：Makefile `buildClientReactor` 增加一行把 `resources/fonts/*.ttf` 拷到
   `bin/ui/fonts/`（4 档）。命令框不涉及（引擎侧渲染，天然排除）。

## 验收

* 闸门：**169/169**（+2 字体测试：资源目录含四档 ttf；私有加载计数 = 4）+ fmt/clippy 0。
* 字形：dev 真机截图 `evidence-misans-plugins.png` / `evidence-misans-options.png`
  与改前 `evidence-layout-plugins.png` 对比——笔画形态与换行点均变化（MiSans 生效，
  回落链 YaHei 未接管）。
* 发布：release → `bin/ui`（211 文件，含 fonts 4）→ `/MIR` 同步生产树；生产树
  WMI 拉起复验：无黑窗（ConsoleWindowClass 0→0）、UIA 导航就绪（`evidence-misans-prod.png`
  两次取证时面板均被操作者手动终止，渲染状态以 UIA 就绪判定 + dev 截图为准）。

## 已知边界

* 停止随包字体后回落链 = Microsoft YaHei UI（不致渲染失败）。
* `SERIF_FONT`（Georgia 衬线档，旧版页标题用）reactor 无 font_family 不可达，
  维持 Fluent 默认——页标题以 24 Medium 呈现，视觉差异极小。
* 若未来升级 reactor 且其原生支持字体资源，可移除 vendor 补丁（改动集中、易回滚）。

## ⚠️ 2026-09-29 真机复验更正：原全局覆盖**并未生效**（本轮已修）

- **用户截图取证**：面板标题字形实为雅黑回退（喇叭口撇捺），非 MiSans。当时（六轮）的
  "截图字形验收"结论**误判**——对比法只看了粗细档位，未做字形族比对。
- **根因**：`ContentControlThemeFontFamily` 被 XamlControlsResources **自身主题字典**
  占据，而库默认样式（TextBlock 等）的 `{ThemeResource}` 引用**在库字典内部解析**——
  App 级**普通**合并字典条目（原实现）不可达。
- **修法**（vendor fork `app_shim.rs install_global_ui_font`，本轮唯一改动）：把覆盖
  字典改为 **`ResourceDictionary.ThemeDictionaries`**（Light/Default 各一份同键
  FontFamily）再 Append 到 App 合并字典末位——文档化的 WinUI 主题资源覆盖姿势。
- **验证方法（可复用）**：`PrintWindow(PW_RENDERFULLCONTENT)` 直接截面板窗口
  （被遮挡也可用；ImageGrab 会被前台窗口污染）→ 与 PIL 用同字号渲染的
  MiSans-Semibold / 雅黑参照图逐字形比对。南字宽帽、等线无出锋 = MiSans 命中。
