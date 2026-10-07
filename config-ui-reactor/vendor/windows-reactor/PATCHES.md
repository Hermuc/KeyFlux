# `vendor/windows-reactor` —— 本地补丁记录

本目录是 **`windows-reactor` 0.100.0 的 vendored fork**，由 `config-ui-reactor/Cargo.toml`
的 `[patch.crates-io]` 指向。它是**上游的一整份拷贝**，不是 `.patch` 文件集 —— 所以
「我们到底改了上游的哪些地方」**无法靠读这个目录本身得知**，必须有外部基线。
本文件就是那份基线记录：升级上游时以它为准。

> 为什么必须有这份文件：vendor 树里的补丁对编译器、测试、CI 全都是**不可见的**
> （依赖 crate 会被 `--cap-lints allow`，且没有任何闸门比对上游）。补丁丢了、
> 被上游新版覆盖了、或有人多改了东西，都不会有任何提示。

## 一、上游身份（可复核）

| 项 | 值 |
|---|---|
| crate / 版本 | `windows-reactor` **0.100.0**（crates.io）|
| 上游 git sha | `a57cc321bfdbac6bb98c488ba49f399da8fcb98a` |
| 上游仓库内路径 | `crates/libs/reactor`（`microsoft/windows-rs`）|
| sha 出处 | 本目录 `.cargo_vcs_info.json`（crate 打包时写入，**勿手改**）|
| 导入提交 | `c719514` — "feat(ui): 全局字体回归 MiSans（vendor fork windows-reactor）" |

## 二、权威差异面（2026-10-01 实测）

拉上游原包解包后与本目录逐文件比对：

```bash
curl -sSL -o wr.crate \
  https://static.crates.io/crates/windows-reactor/windows-reactor-0.100.0.crate
tar -xzf wr.crate
```

实测结果：

- **真实代码差异 = 8 个文件**（2026-10-01 实测为 7 个文件 / 21 个 hunk / `+221 −1`；
  2026-10-07 起 +P8 新增 `src/element.rs` 差异，逐文件行数见第三节各行）（见第三节）
- **另有 39 个文件只是行尾不同**：本目录工作面是 **CRLF**（仓库 `core.autocrlf=true`），
  而 crates.io tarball 是 LF。⚠️ **裸 `diff -r` 会把 `src/element.rs`（±2503 行）与
  `src/lib.rs`（±22 行）报成「整个文件都改了」—— 那是行尾假阳性，不是补丁。**
  比对前必须先把 CRLF 归一成 LF（脚本见第五节）。
- 只存在于本目录的两项，**都不是上游产物、也都不是对上游代码的修改**：
  - `.cargo-ok` —— cargo 的 vendoring 标记文件；
  - `PATCHES.md` —— 本记录。

## 三、补丁清单

| # | 补丁 | 文件（hunk / 增删）| 引入提交 | 消费点 |
|---|---|---|---|---|
| P1 | **全局 UI 字体通道** | `native/winui/app_shim.rs`（与 P5 共处 1 个 hunk）| `c719514` 初版 · `fb0db36` 改走 ThemeDictionaries | vendor `src/app.rs:807` |
| P2 | **字体族注入通道** | `native/winui/bindings.rs`（1 / +37）· `native/winui/generated.rs`（1 / +16 −1）| `a3025cb` | vendor `native/winui/generated.rs:164`（`set_font_family_raw`）|
| P3 | **ContentDialog 跟随宿主窗口主题** | `native/winui/content_dialog.rs`（7 / +31）· `native/winui/mod.rs`（2 / +8）| `6da66d1` | 弹窗 `show` 路径内部 |
| P4 | **`Element::resource_overrides` builder** | `src/generated.rs`（8 / +26）| `d8ffebd` | 面板 `src/app.rs:1827`、`ui/abbr_view.rs:55`、`ui/keymap_view.rs:135`、`ui/selected_action_view.rs`（7 处）|
| P5 | **ContentDialog 遮罩层覆盖** | `native/winui/app_shim.rs`（同上 hunk）· `src/app.rs`（1 / +2 中的 1 行）| `10eee9a` | vendor `src/app.rs:808` |
| P6 | **TextBox 挂载顺序：AcceptsReturn 先于 Text** | `src/generated.rs`（1 / 移动 7 行，零增删）| `eff21fd+`（本次）| 指南编辑弹窗等一切**程序化灌入多行文本**的 TextBox |
| P7 | **滚动条响应计时覆盖（隐式 ScrollBar 样式）** | `native/winui/app_shim.rs`（同上 hunk 续扩）· `src/app.rs`（1 / +3 中的 1 行）| 本次 | vendor `src/app.rs:809` |
| P8 | **窗口居中通道（`WindowVisuals.centered`）** | `src/element.rs`（2 / +10）· `native/winui/bindings.rs`（2 / +23）· `native/winui/mod.rs`（6 / +55，含 diff 语义测试）| `2026-10-07` 用户报障：面板打开不在屏幕正中央 | 面板 `src/platform/mod.rs`（`WindowSpec::visuals` 恒 `centered(true)`）|

- `native/winui/app_shim.rs`：**一个 hunk，P1 / P5 / P7 都落在这里**（P7 引入后共
  约 `+740 −0`，其中约 530 行是 P7 的整段 XAML 字面量）——
  升级上游时这是最容易冲突的文件。
- vendor `src/app.rs` 的 `+3` = `install_global_ui_font`(P1)、
  `install_dialog_layer_overrides`(P5)、`install_scroll_bar_overrides`(P7) 三行调用。

### 各补丁动机（各一句话）

- **P1** —— 上游 0.100.0 没有字体通道（无 per-control `font_family` builder、
  `ResourceOverrides` 只接受色值、`AppContext` 不暴露 `Resources`）⇒ fork 在装完 Fluent
  资源后经 `XamlReader` 往 App 级字典合并 **ThemeDictionaries**，覆盖
  `ContentControlThemeFontFamily`。（普通合并字典条目**压不过** `XamlControlsResources`
  自身主题字典里的同名键 —— 这正是 `fb0db36` 修的原始 bug。）
- **P2** —— 补一条直接改 `FontFamily` 的绑定通路 `set_font_family_raw`，
  用于把面板字体链里的微软雅黑摘掉、收敛到随包 MiSans。
- **P3** —— 内容弹窗默认不继承宿主窗口主题，深色宿主 + 浅色弹窗会打架 ⇒ `show` 前显式跟随。
- **P4** —— 上游 `Element` 没有挂 `ResourceOverrides` 的 builder，而面板多处在按元素
  覆盖主题资源（⚠️ 覆盖键**必须含状态**，否则只在新元素创建时生效 —— 见
  `ui/abbr_view.rs` 注释「13 号文档 §3」）。
- **P5** —— `ContentDialog` 的遮罩层是 Popup 根 Canvas 上的兄弟 `Rectangle`，**不走模板**，
  只能按主题资源键覆盖：`ContentDialogSmokeFill`（真正的键名，从
  `Microsoft.UI.Xaml.Controls.pri` 里查到）、`ContentDialogTopOverlay`、
  `ContentDialogDimmingThemeBrush` ⇒ 三者置 `Transparent`。
- **P7** —— Fluent `ScrollBar` 模板把悬停展开 / 移出收起动画的 `BeginTime` 硬编码为
  400ms / 500ms 起手延迟（`ScrollBarExpandBeginTime` / `ScrollBarContractBeginTime`），
  滚动条状态切换永远慢半拍；而这两个键在模板里是 `{StaticResource}` 引用——XBF 编译期
  绑定，App 级资源覆盖（顶层条目 + ThemeDictionaries 双写）**实测无效**（只有
  `{ThemeResource}` 可被 App 级主题字典覆盖，见 P1 教训）⇒ 整段模板转为 **App 级隐式
  样式**：标量全部内联为字面量、BeginTime 归零、别名刷子解析到底层主题刷子键
  （保留浅色/深色运行时跟随；HighContrast 的 ScrollBar 专用重定向回落标准刷子）。
  模板来源与转换规则见 `app_shim.rs::install_scroll_bar_overrides` 文档注释。

## 四、升级上游时怎么做

1. 取新版原包（`static.crates.io/crates/windows-reactor/windows-reactor-<ver>.crate`）解包；
2. 跑第五节的脚本，得到**当前**补丁面，与第三节对照 —— 有出入先查清是谁多改的；
3. 把 P1–P7 按文件重放到新版（`app_shim.rs` 的 P1+P5+P7 最容易与上游漂移冲突；
   P7 的 ScrollBar 模板需按新版 `ScrollBar_themeresources.xaml` 重新转换，见
   `install_scroll_bar_overrides` 文档注释里的转换规则）；
4. `tools/cargo-gates.ps1` 三道闸门 + `make api-parity` 必须全过；
5. 更新本文件：上游版本 / sha、差异面数字、引入提交。
6. `Cargo.toml` 的 `[patch.crates-io]` 注释**只指向本文件**，不要在注释里重复清单
   （历史教训：那条注释停留在 P1 时代，P2–P5 一直没写进去，直到 2026-10-01 才订正）。

## 五、比对脚本（复核用）

归一化行尾后列出真实差异；`ONLY in vendor` 里除 `.cargo-ok` / `PATCHES.md` 外
出现别的东西，说明有人往 vendor 树里加了非上游文件。

```python
import os, difflib
UP = r'<解包后的 windows-reactor-0.100.0 路径>'
VD = r'<仓库>/config-ui-reactor/vendor/windows-reactor'

def snap(root):
    return {os.path.relpath(os.path.join(d, n), root).replace(os.sep, '/'): os.path.join(d, n)
            for d, _, ns in os.walk(root) for n in ns}

up, vd = snap(UP), snap(VD)
print('ONLY in vendor  :', sorted(set(vd) - set(up)))
print('ONLY in upstream:', sorted(set(up) - set(vd)))

for rel in sorted(set(up) & set(vd)):
    a = open(up[rel], 'rb').read()
    b = open(vd[rel], 'rb').read().replace(b'\r\n', b'\n')   # 行尾归一：否则假阳性
    if a == b:
        continue
    al, bl = a.decode('utf-8', 'replace').splitlines(), b.decode('utf-8', 'replace').splitlines()
    h = sum(1 for l in difflib.unified_diff(al, bl, n=0) if l.startswith('@@'))
    print('REAL DIFF %-40s hunks=%d' % (rel, h))
```

预期输出：`ONLY in vendor` 只有 `.cargo-ok` 与 `PATCHES.md`；
`REAL DIFF` 恰好 7 行（第三节表格的 7 个文件）。
