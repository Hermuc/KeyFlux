# Phase 3b：配置驱动导航 + 使用指南页（✅ 已完成）

> 日期：2026-09-28 ｜ 证据：`config-ui-reactor/evidence-phase3b-guide.png`
> 本阶段依据用户补充授权：**「一些原先软件的 UI 设计可以改成更适合 WinUI 的类型」**

---

## 1. 交付

| 项 | 内容 | 旧实现 |
|---|---|---|
| **配置驱动导航** | 3 固定项 + 全部 `enable && id != 1` 的 keymap；`id=4` 标题取 i18n 2581；`id∉{2,3,4}` 用 `km.name`（空则 `hotkey`） | `MainViewModel.BuildNav`（`:175-233`）|
| **页面路由** | `id=4`→设置页 · `id∈{2,3}`→缩写页 · 其余→矩阵页 | `MainViewModel.PageForKeymap`（`:237-248`）|
| **使用指南页** | 消费 `services::markdown` 块模型 → WinUI 原生控件；文档源 = `config.overviewDocMd`，为空则 `GET /config_doc.md` | `HomePageViewModel.LoadAsync` |
| **Markdown 渲染层** | 新增 `src/ui/markdown_view.rs`：标题/段落/列表/图片 + 行内 文本·代码·链接 | `Services/MarkdownRenderer.cs` |
| **图片** | `Image::source("http://127.0.0.1:{port}/{src}")` **直连后端静态站**（无需字节中转）| `LoadImageAsync` 走 `GetBytesAsync` |
| **链接** | `RichTextHyperlink{text, uri}`；`/` 开头视为后端站点内部路径并拼本机地址 | `LinkOpener.Open` |

**闸门**：`cargo clippy --all-targets` **0 告警**；`cargo test` **74/74 通过**（新增 11 项）；`cargo fmt --check` **exit 0**。

## 2. 真机验证（截图实证）

| 观察项 | 结果 |
|---|---|
| 导航 | `使用指南 / 选中动作 / 插件 / CapsLock / F 模式 / Command / 选项` —— **全部来自真实 config.json**（证明 `enable && id!=1` 过滤与 id 路由正确）|
| 指南文档 | 渲染后端真实 `config_doc.md`：H1/H2/H3 层级、emoji 原生显示、有序+无序列表（含嵌套缩进）、**链接以系统 Accent 色可点** |
| Job Object | 强杀面板后 `settings = 0`（后端被连带回收），`KeyFlux` 引擎不受影响 ✅ |

## 3. 按授权做的 Fluent 化调整（与旧设计的有意差异）

| 项 | 旧（Avalonia/Claude） | 新（WinUI/Fluent） | 依据 |
|---|---|---|---|
| 标题字号 | 24 / 22 / 18 / 16 | **28 / 20 / 16 / 14** | Fluent 排版层级（Title / Subtitle / Body）|
| 链接 | 自绘暖绿 `#5e7d5a` + 下划线补偿基线 | **`RichTextHyperlink`（系统 Accent 色 + 原生交互）** | Fluent「链接 = Accent」 |
| 标题栏 | 自绘三键 + 手工拖动区 | **`TitleBar` 控件（系统三键 + 原生拖动/双击最大化）** | Fluent「标题栏可承载导航」 |
| 导航容器 | 自绘 `ListBox` + 每项图标/哈希配色 | **`NavigationView` 三档响应式模式** | Fluent 标准导航 |
| 窗口材质 | 实色 Parchment + 运行期 alpha 计算 | **Mica 背景 + `WindowTheme::Light`** | Fluent 材质规则 |
| 段落滚动 | 页面级 `Viewbox` 缩放 | 内容区 `ScrollViewer`（页面不整体滚动）| Fluent 内容区语义 |

### 已知的 API 上限（非设计取舍）

* **行内代码无法着色**：`RichTextRun` 只有 `text` / `is_bold` / `is_italic` 三个字段（无 `foreground`/`font_family`）⇒ 行内 code 退化为纯文本。旧版用 Coral + Consolas。
* **无 `WrapPanel`/`ItemsWrapGrid`**：需要换行布局时必须另想办法（`VariableSizedWrapGrid` 在 API 内，Phase 3c 若需换行 chips 需先验证）。

## 4. 本阶段新掌握的 0.100.0 硬事实（补入迁移知识库）

| # | 事实 | 影响 |
|---|---|---|
| 1 | **无 `Inlines`/`Run`/`Span`，但有 `RichTextBlock::paragraphs(RichText)`** | 行内富文本走 `RichText`/`RichTextParagraph`/`RichTextRun`/`RichTextHyperlink` |
| 2 | `RichTextRun` 仅 `text`/`is_bold`/`is_italic` 三个 pub 字段；`RichTextHyperlink` 为 `{text, uri}` pub 字段 | 行内样式能力受限 |
| 3 | **`Image::source(String)` 存在**（返回 `Result`），另有 `source_data(EncodedImage)` / `source_file(Path)` | 图片可直接用 URL；⚠️ `source` 必须放**链尾**（返回 `Result`）|
| 4 | **`IntoViews` 只实现 `()`/`[T; N]`/元组，不含 `Vec`** | 动态集合必须走 `keyed_children`；`KeyedView: From<(K, V)>` 是泛型的 |
| 5 | `NavigationViewItem` **没有** `content()`（`ContentControl` 未实现） | 内容必须走 `.slot(NavigationViewItemSlot::Content, v)` |
| 6 | `NavigationViewItem::tag(impl Into<String>)` | 动态字符串 tag 可用 |

## 5. 下一步（Phase 3b 续）

1. **模式页（按键矩阵）**：布局解析 → 网格；键格 Fluent 化（MinWidth 68 / Height 50 的旧约束已解除，可按 Fluent 控件尺寸重定）
2. **缩写页**（id 2/3）：词表 + 命令表（旧版 chips 无 `WrapPanel` 可用 ⇒ 需先定换行方案）
3. **选中动作页**：主热键 + 文本/文件双聚合卡 + 匹配类型选择
4. 移植 `NavBadge.EffectiveHotkey`（当前 name 为空时退化用 `keymap.hotkey`，已在代码留 TODO）
