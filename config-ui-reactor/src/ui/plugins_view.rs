//! 插件页视图：统一插件卡 + 页头入口 + 加载/告警/空态。
//!
//! 与旧 `Views/PluginsPageView.axaml`（18.6KB）的对应：
//! * 页头：「插件市场」（2428）+「导入插件」（2427）；
//! * 分区：「第三方插件」（2421）+ 说明（2425）；
//! * 卡片：显示名 + 版本徽标 + 启停开关 + 状态文字（2423/2424）+ 描述 + 作者；
//!   **信息区可点即配置**（复刻旧版「点击卡片即可调整」，提示 2426），用户卡带「删除」（912，
//!   2026-10-02 定版：删除钮在头部开关块内、ON 状态字右边）；
//! * 三态：加载中（2439）/ 逐包告警 / 空态（2429 + 2430）。
//!
//! Fluent 化取舍：旧版固定尺寸列表 + 内部滚动，新版改**自适应高度卡片 + 外层滚动**。

use windows_reactor::*;

use crate::services::i18n;
use crate::services::plugins::PluginCard;
use crate::theme;

/// 页头：页标题 + 两个入口按钮（旧 `Grid *,Auto,Auto`：标题/市场 12 / 导入 8）。
pub fn page_header<M, I>(on_market: M, on_import: I) -> View
where
    M: IntoUnitCallback,
    I: IntoUnitCallback,
{
    Grid::new()
        .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
        .children((
            TextBlock::new()
                .text(i18n::t("2418"))
                .font_size(theme::FONT_PAGE_TITLE)
                .font_weight(FontWeight::MEDIUM)
                .foreground(theme::near_black())
                .vertical_alignment(VerticalAlignment::Center),
            Button::new()
                .grid_column(1)
                .margin(Thickness::new(12.0, 0.0, 0.0, 0.0))
                .on_click(on_market)
                .content(TextBlock::new().text(i18n::t("2428"))),
            Button::new()
                .grid_column(2)
                .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                .on_click(on_import)
                .content(TextBlock::new().text(i18n::t("2427"))),
        ))
}

/// 运行时边界说明（2425，stone 12）+ 配置引导（2426，强调蓝 11）——页尾两行。
pub fn footer_notes() -> View {
    StackPanel::new().spacing(4.0).children((
        TextBlock::new()
            .text(i18n::t("2425"))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::stone_gray())
            .text_wrapping(TextWrapping::Wrap),
        TextBlock::new()
            .text(i18n::t("2426"))
            .font_size(theme::FONT_BADGE)
            .foreground(theme::accent_solid())
            .text_wrapping(TextWrapping::Wrap),
    ))
}

/// 徽标胶囊（Warm Sand 底、圆角 10、内边距 8×2）。
fn badge(text: &str, color: Color) -> View {
    Border::new()
        .background(theme::sand())
        .corner_radius(windows_reactor::CornerRadius::uniform(10.0))
        .padding(Thickness::new(8.0, 2.0, 8.0, 2.0))
        .vertical_alignment(VerticalAlignment::Center)
        .content(
            TextBlock::new()
                .text(text.to_string())
                .font_size(theme::FONT_BADGE)
                .foreground(theme::solid(color)),
        )
}

/// 插件卡。
///
/// `on_configure` 挂在**信息区**（复刻旧版点卡片开配置）；不可配置时该区域点击无效。
pub fn plugin_card<T, D, P>(
    card: &PluginCard,
    english: bool,
    on_toggle: T,
    on_delete: D,
    on_configure: P,
) -> View
where
    T: IntoPayloadCallback<bool>,
    D: IntoUnitCallback,
    P: IntoPayloadCallback<PointerEventInfo>,
{
    let version = card.version_text();
    // 标题行：名称 15 SemiBold + 版本徽标（Sand 胶囊 11）+ 运行时标注（仅用户插件，2421）
    let mut title_items: Vec<(usize, View)> = Vec::new();
    title_items.push((
        title_items.len(),
        TextBlock::new()
            .text(card.display_name(english))
            .font_size(theme::FONT_CARD_TITLE)
            .font_weight(FontWeight::SEMI_BOLD)
            .foreground(theme::near_black())
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    ));
    if !version.is_empty() {
        title_items.push((title_items.len(), badge(&version, theme::CHARCOAL)));
    }
    if !card.is_builtin {
        title_items.push((
            title_items.len(),
            badge(&i18n::t("2421"), theme::SLATE_GRAY),
        ));
    }
    let title: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .keyed_children(title_items);

    // 说明区（描述 13 olive + 作者 11 stone），整体可点 ⇒ 打开配置
    let mut details: Vec<(usize, View)> = Vec::new();
    if !card.description.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(card.description.clone())
                .font_size(13.0)
                .foreground(theme::solid(theme::SLATE_GRAY))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ));
    }
    // 作者行：官方（KeyFlux）不出——无信息量（2026-10-02 用户定版）；第三方保留。
    if !card.author.is_empty() && card.author != "KeyFlux" {
        details.push((
            details.len(),
            TextBlock::new()
                .text(card.author.clone())
                .font_size(theme::FONT_BADGE)
                .foreground(theme::stone_gray())
                .into(),
        ));
    }

    // 信息区：可配置时挂指针按下（整个信息区即配置入口）。
    // ⚠️ 必须给 Border 一个背景色才会参与命中测试（无背景的 Border 收不到指针事件），
    //    故用**全透明**背景占位；且 `on_pointer_pressed` 必须在 `.content()` **之前**调用
    //    （`.content()` 收尾为 `View`，而 `View` 不再具备该 builder 方法）。
    let inner: View = StackPanel::new().spacing(4.0).keyed_children(details);
    let info: View = if card.can_configure {
        Border::new()
            .background(theme::solid(Color::transparent()))
            .on_pointer_pressed(on_configure)
            .content(inner)
    } else {
        Border::new().content(inner)
    };

    // 紧凑开关（修 WinUI 默认 MinWidth=154，否则状态字被推远；见 ui::compact_switch）
    let switch: View = crate::ui::compact_switch(card.enabled, on_toggle);
    // 状态字在开关**正右边**（用户定版：不再放开关下方），且走共享实现 `ui::on_off_indicator`
    // —— 措辞/字号/字重/取色/垂直居中与「快捷键方案」页、「选中动作」页**同源**。
    //
    // 🔴 这里此前是**第三份**私有拷贝，且漏了 `vertical_alignment(Center)`：横向 StackPanel 给
    //    子级的槽位高 = 面板高（开关 ≈32 DIP），`TextBlock` 默认 `Stretch` ⇒ 占满槽位、文本贴
    //    上沿，「ON」比开关中心高约 10 DIP（2026-10-01 用户报障："ON 的文字提示和开关按钮没有
    //    对齐"）。修法不是就地补一句对齐，而是**回到唯一实现**：拷贝少一份，"某一页漏了某个
    //    属性"的漂移入口就少一个。
    // 开关块右侧 12px 留白挂到列 1 的 `Border.margin` 上（原挂在状态字自己的 margin 上，等效）。
    //
    // 删除钮（2026-10-02 用户定版）：从卡片底部的独立行移入头部开关块，**位于 ON 状态字
    // 再右边**——开关 / 状态字 / 删除横向一行；不可删除（内置）时不渲染，不留空槽。
    // 卸载入口：旧版 `IsVisible=CanDelete`（内置卡**不渲染**此钮），tooltip 912。
    let delete: View = if card.can_delete {
        Border::new()
            .vertical_alignment(VerticalAlignment::Center)
            .content(crate::ui::icon_button(
                "✕",
                15.0,
                theme::ERROR_CRIMSON,
                true,
                on_delete,
            ))
    } else {
        View::empty()
    };
    let mut toggle_items: Vec<(usize, View)> =
        vec![(0, switch), (1, crate::ui::on_off_indicator(card.enabled))];
    if card.can_delete {
        toggle_items.push((2, delete));
    }
    let toggle_block: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(10.0)
        .keyed_children(toggle_items);

    let header: View = Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .children((
            title,
            Border::new()
                .grid_column(1)
                .margin(Thickness::new(0.0, 0.0, 12.0, 0.0))
                .content(toggle_block),
        ));

    let rows: Vec<(usize, View)> = vec![(0, header), (1, info)];

    // 旧 `Border.pluginCard`：Ivory 面 + 淡冷边 2px + 圆角 14 + Padding 20,16 + 底距 10
    Border::new()
        .padding(Thickness::new(20.0, 16.0, 20.0, 16.0))
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .background(theme::ivory())
        .border_brush(theme::border_faint())
        .border_thickness(theme::card_border())
        .corner_radius(theme::radius_card())
        .content(StackPanel::new().spacing(0.0).keyed_children(rows))
}

/// 加载中（2439）：旧版为纯文字 13。
pub fn loading() -> View {
    TextBlock::new()
        .text(i18n::t("2439"))
        .font_size(13.0)
        .foreground(theme::stone_gray())
        .into()
}

/// 目录加载告警（逐包错误汇总，多行）+ 重试。
/// 旧版容器：Ivory 面 + 淡冷边 1px + 圆角 4（RadiusPanel）+ Padding 14。
/// 市场对话框传入 `title`（2433「无法加载插件市场目录」，旧 `PluginMarketWindow.axaml:117`）。
pub fn load_error(message: &str, title: Option<&str>, on_retry: impl IntoUnitCallback) -> View {
    let mut head: Vec<(usize, View)> = Vec::new();
    if let Some(title) = title {
        head.push((
            head.len(),
            TextBlock::new()
                .text(title.to_string())
                .font_size(theme::FONT_BODY)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::solid(theme::ERROR_CRIMSON))
                .into(),
        ));
    }
    head.push((
        head.len(),
        TextBlock::new()
            .text(message.to_string())
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::solid(theme::ERROR_CRIMSON))
            .text_wrapping(TextWrapping::Wrap)
            .into(),
    ));

    StackPanel::new().spacing(10.0).children((
        Border::new()
            .background(theme::ivory())
            .border_brush(theme::border_faint())
            .border_thickness(theme::hairline())
            .corner_radius(theme::radius_panel())
            .padding(Thickness::uniform(14.0))
            .content(StackPanel::new().spacing(4.0).keyed_children(head)),
        Button::new()
            .on_click(on_retry)
            .content(TextBlock::new().text(i18n::t("920"))),
    ))
}

/// 插件市场空态（2438 单行，区别于插件页的导入引导 2429+2430）。
pub fn market_empty() -> View {
    TextBlock::new()
        .text(i18n::t("2438"))
        .font_size(theme::FONT_BODY)
        .foreground(theme::stone_gray())
        .into()
}

/// 一次性操作失败横幅（导入/删除失败：纯文字、**无重试按钮**——重试语义只属于目录加载，
/// 复刻旧版这两类失败走模态消息、目录失败才是 banner 的分流）。
pub fn action_error(message: &str) -> View {
    Border::new()
        .background(theme::ivory())
        .border_brush(theme::border_faint())
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_panel())
        .padding(Thickness::uniform(14.0))
        .content(
            TextBlock::new()
                .text(message.to_string())
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::ERROR_CRIMSON))
                .text_wrapping(TextWrapping::Wrap),
        )
}

/// 空态引导（2429 + 2430）：旧版 Warm Sand 面、无边、圆角 4、Padding 14、行距 5。
pub fn empty_state() -> View {
    Border::new()
        .background(theme::sand())
        .corner_radius(theme::radius_panel())
        .padding(Thickness::uniform(14.0))
        .content(
            StackPanel::new().spacing(5.0).children((
                TextBlock::new()
                    .text(i18n::t("2429"))
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(theme::solid(theme::CHARCOAL)),
                TextBlock::new()
                    .text(i18n::t("2430"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::SLATE_GRAY))
                    .text_wrapping(TextWrapping::Wrap),
            )),
        )
}

/// 市场条目卡：名称 + 版本 + 描述 + 作者 + 「安装 / 已安装」。
pub fn market_entry<C: IntoUnitCallback>(
    entry: &crate::services::market::MarketEntry,
    english: bool,
    installing: bool,
    on_install: C,
) -> View {
    // 标题行：名称 15 SemiBold + 版本徽标（对齐插件卡口径）
    let version = entry.version_text();
    let mut title_items: Vec<(usize, View)> = Vec::new();
    title_items.push((
        title_items.len(),
        TextBlock::new()
            .text(entry.display_name(english))
            .font_size(theme::FONT_CARD_TITLE)
            .font_weight(FontWeight::SEMI_BOLD)
            .foreground(theme::near_black())
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    ));
    if !version.is_empty() {
        title_items.push((title_items.len(), badge(&version, theme::CHARCOAL)));
    }
    let title: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .keyed_children(title_items);

    let mut details: Vec<(usize, View)> = Vec::new();
    if !entry.description.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(entry.description.clone())
                .font_size(13.0)
                .foreground(theme::solid(theme::SLATE_GRAY))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ));
    }
    if !entry.author.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(entry.author.clone())
                .font_size(theme::FONT_BADGE)
                .foreground(theme::stone_gray())
                .into(),
        ));
    }

    // 已安装 ⇒ 显示「已安装」；可安装 ⇒ 「安装」按钮（安装中禁用）
    let action: View = if crate::services::market::can_install(entry) {
        Button::new()
            .is_enabled(!installing)
            .on_click(on_install)
            .content(TextBlock::new().text(i18n::t("2434")))
    } else {
        TextBlock::new()
            .text(i18n::t("2435"))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::solid(theme::MUTED_GREEN))
            .into()
    };

    let rows: Vec<(usize, View)> = vec![
        (0, title),
        (1, StackPanel::new().spacing(2.0).keyed_children(details)),
        (2, action),
    ];

    // 几何对齐 `Border.pluginCard`（Ivory 面 / 淡冷边 2px / 圆角 14 / Padding 20,16）
    Border::new()
        .padding(Thickness::new(20.0, 16.0, 20.0, 16.0))
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .background(theme::ivory())
        .border_brush(theme::border_faint())
        .border_thickness(theme::card_border())
        .corner_radius(theme::radius_card())
        .content(StackPanel::new().spacing(0.0).keyed_children(rows))
}

/// 声明式设置行：标签 + 提示 + 编辑器（`char`/`text`/`number`/`file` 用文本框；
/// `bool` 用开关且并入标签行右侧，无标签时独立成行）。
pub fn setting_row<C: IntoPayloadCallback<String>, T: IntoPayloadCallback<bool>>(
    setting: &crate::models::PluginSetting,
    value: &str,
    english: bool,
    on_change: C,
    on_toggle: T,
) -> View {
    let mut children: Vec<(usize, View)> = Vec::new();

    let label = crate::services::plugins::setting_label(setting, english);
    // `bool` 类型（2026-10-01 协议扩展）：渲染成开关，**不是**文本框。
    // 值仍走同一条字符串通道（"true"/"false"）⇒ 无需新增消息、后端无需新分支 ——
    // 协议里 `bool` 本来就是字符串承载（`ConfigProvider` 只有扁平字符串存储）。
    let is_bool = crate::services::plugins::is_bool(setting);
    // 开关放文字**右边**（用户定版 2026-10-03）：此前开关独占一行压在标签下方。
    // 开关只构建一次（回调被消费），放标签行还是独立行由有无标签决定。
    let mut switch: Option<View> = if is_bool {
        Some(crate::ui::compact_switch(
            crate::services::plugins::bool_value(value),
            on_toggle,
        ))
    } else {
        None
    };
    if !label.is_empty() {
        if let Some(row_switch) = switch.take() {
            let label_text: View = TextBlock::new()
                .text(label)
                .font_size(theme::FONT_BODY)
                .foreground(theme::near_black())
                .vertical_alignment(VerticalAlignment::Center)
                .into();
            children.push((
                children.len(),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(12.0)
                    .children((label_text, row_switch)),
            ));
        } else {
            children.push((
                children.len(),
                TextBlock::new()
                    .text(label)
                    .font_size(theme::FONT_BODY)
                    .foreground(theme::near_black())
                    .into(),
            ));
        }
    }

    let hint = crate::services::plugins::setting_hint(setting, english);
    if !hint.is_empty() {
        children.push((
            children.len(),
            TextBlock::new()
                .text(hint)
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ));
    }

    // 数字项的上下限说明
    let range = crate::services::plugins::range_hint(setting);
    if !range.is_empty() {
        children.push((
            children.len(),
            TextBlock::new()
                .text(range)
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::SLATE_GRAY))
                .into(),
        ));
    }

    // `bool` 且无标签：开关单独成行（有标签时已并入标签行，见上）
    if let Some(standalone) = switch {
        children.push((children.len(), standalone));
    } else if !is_bool {
        // ⚠️ 0.100.0 的 TextBox 无 `MaxLength`（长度上限由保存前的校验兜底，口径同后端）
        let mut editor = TextBox::new()
            .text(value.to_string())
            .min_width(280.0)
            .on_text_changed(on_change);
        // `text` 且声明 `multiline`（2026-10-02 P5）：接受回车 + 换行分隔值
        // （首个消费方 = quick_switch.excludedPrefixes），复用 action_editor 的形态。
        if crate::services::plugins::is_multiline(setting) {
            editor = editor
                .accepts_return(true)
                .text_wrapping(TextWrapping::Wrap)
                .min_height(96.0);
        }
        let editor: View = editor.into();

        // `file` 类型不再配「浏览」按钮（2026-10-04 用户定版：长路径下按钮被截断，
        // 且可直接手输路径）⇒ 所有非 bool 类型统一只渲染文本框。
        children.push((children.len(), editor));
    }

    // `char` 且为空/空格：单字符输入框放不下视觉线索，补一行可读回显（复刻 ShowSpaceToken）
    if crate::services::plugins::is_char(setting) && (value.is_empty() || value == " ") {
        children.push((
            children.len(),
            TextBlock::new()
                .text(i18n::t("2587"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::MUTED_GREEN))
                .into(),
        ));
    }

    StackPanel::new()
        .spacing(4.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 14.0))
        .keyed_children(children)
}

/// 一次性操作回显（导入/删除成功提示）：旧版 13 SemiBold terracotta，现走主题强调色（冷蓝）。
pub fn status_banner(message: &str) -> View {
    TextBlock::new()
        .text(message.to_string())
        .font_size(13.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .foreground(theme::accent_solid())
        .text_wrapping(TextWrapping::Wrap)
        .into()
}
