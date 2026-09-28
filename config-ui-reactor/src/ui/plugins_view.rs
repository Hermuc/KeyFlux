//! 插件页视图：统一插件卡 + 页头入口 + 加载/告警/空态。
//!
//! 与旧 `Views/PluginsPageView.axaml`（18.6KB）的对应：
//! * 页头：「插件市场」（2428）+「导入插件」（2427）；
//! * 分区：「第三方插件」（2421）+ 说明（2425）；
//! * 卡片：显示名 + 版本徽标 + 启停开关 + 状态文字（2423/2424）+ 描述 + 作者；
//!   **信息区可点即配置**（复刻旧版「点击卡片即可调整」，提示 2426），用户卡带「删除」（912）；
//! * 三态：加载中（2439）/ 逐包告警 / 空态（2429 + 2430）。
//!
//! Fluent 化取舍：旧版固定尺寸列表 + 内部滚动，新版改**自适应高度卡片 + 外层滚动**。

use windows_reactor::*;

use crate::services::i18n;
use crate::services::plugins::PluginCard;
use crate::theme;

/// 页头：页标题 + 两个入口按钮。
pub fn page_header<M, I>(on_market: M, on_import: I) -> View
where
    M: IntoUnitCallback,
    I: IntoUnitCallback,
{
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(10.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 14.0))
        .children((
            TextBlock::new()
                .text(i18n::t("2418"))
                .font_size(28.0)
                .font_weight(FontWeight::BOLD)
                .foreground(theme::near_black())
                .vertical_alignment(VerticalAlignment::Center),
            Button::new()
                .on_click(on_market)
                .content(TextBlock::new().text(i18n::t("2428"))),
            Button::new()
                .on_click(on_import)
                .content(TextBlock::new().text(i18n::t("2427"))),
        ))
}

/// 第三方插件分区标题 + 说明。
pub fn section_note() -> View {
    StackPanel::new()
        .spacing(2.0)
        .margin(Thickness::new(0.0, 6.0, 0.0, 10.0))
        .children((
            TextBlock::new()
                .text(i18n::t("2421"))
                .font_size(theme::FONT_SUBTITLE)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::near_black()),
            TextBlock::new()
                .text(i18n::t("2425"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .text_wrapping(TextWrapping::Wrap),
        ))
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
    let title: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(10.0)
        .children((
            TextBlock::new()
                .text(card.display_name(english))
                .font_size(theme::FONT_SUBTITLE)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::near_black()),
            TextBlock::new()
                .text(version)
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::OLIVE_GRAY))
                .vertical_alignment(VerticalAlignment::Center),
        ));

    // 说明区（描述 + 作者 + 可配置提示），整体可点 ⇒ 打开配置
    let mut details: Vec<(usize, View)> = Vec::new();
    if !card.description.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(card.description.clone())
                .font_size(theme::FONT_BODY)
                .foreground(theme::solid(theme::CHARCOAL_WARM))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ));
    }
    if !card.author.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(card.author.clone())
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .into(),
        ));
    }
    if card.can_configure {
        details.push((
            details.len(),
            TextBlock::new()
                .text(i18n::t("2426"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::MUTED_GREEN))
                .text_wrapping(TextWrapping::Wrap)
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

    let switch: View = ToggleSwitch::new()
        .is_on(card.enabled)
        .on_toggled(on_toggle)
        .into();
    let status: View = TextBlock::new()
        .text(card.status_text())
        .font_size(theme::FONT_CAPTION)
        .foreground(theme::stone_gray())
        .into();
    let toggle_block: View = StackPanel::new()
        .spacing(2.0)
        .horizontal_alignment(HorizontalAlignment::Right)
        .children((switch, status));

    let header: View = Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .children((title, Border::new().grid_column(1).content(toggle_block)));

    let delete: View = Button::new()
        .is_enabled(card.can_delete)
        .on_click(on_delete)
        .content(TextBlock::new().text(i18n::t("912")));

    let rows: Vec<(usize, View)> = vec![(0, header), (1, info), (2, delete)];

    Border::new()
        .padding(theme::pad_md())
        .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
        .background(theme::card_background())
        .border_brush(theme::card_stroke())
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_md())
        .content(StackPanel::new().spacing(0.0).keyed_children(rows))
}

/// 加载中（2439）。
pub fn loading() -> View {
    StackPanel::new()
        .spacing(10.0)
        .horizontal_alignment(HorizontalAlignment::Center)
        .children((
            ProgressRing::new().is_active(true).width(36.0).height(36.0),
            TextBlock::new()
                .text(i18n::t("2439"))
                .foreground(theme::stone_gray()),
        ))
}

/// 目录加载告警（逐包错误汇总，多行）+ 重试。
pub fn load_error(message: &str, on_retry: impl IntoUnitCallback) -> View {
    StackPanel::new().spacing(10.0).children((
        TextBlock::new()
            .text(message.to_string())
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::solid(theme::ERROR_CRIMSON))
            .text_wrapping(TextWrapping::Wrap),
        Button::new()
            .on_click(on_retry)
            .content(TextBlock::new().text(i18n::t("920"))),
    ))
}

/// 空态引导（2429 + 2430）。
pub fn empty_state() -> View {
    Border::new()
        .padding(theme::pad_md())
        .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
        .background(theme::card_background())
        .border_brush(theme::card_stroke())
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_md())
        .content(
            StackPanel::new().spacing(4.0).children((
                TextBlock::new()
                    .text(i18n::t("2429"))
                    .font_size(theme::FONT_BODY)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(theme::solid(theme::CHARCOAL_WARM)),
                TextBlock::new()
                    .text(i18n::t("2430"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
            )),
        )
}

// ---------------------------------------------------------------- 对话框构件

/// 复选行（QuickSwitch 配置项）。
pub fn qs_check<C: IntoPayloadCallback<bool>>(
    label: impl Into<String>,
    checked: bool,
    on_change: C,
) -> View {
    CheckBox::new()
        .is_checked(checked)
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .on_is_checked_changed(on_change)
        .content(TextBlock::new().text(label.into()))
}

/// 数值行（左标签 + 右 `NumberBox`）。
pub fn qs_number<C: IntoPayloadCallback<Option<f64>>>(
    label: impl Into<String>,
    value: f64,
    minimum: f64,
    maximum: f64,
    on_change: C,
) -> View {
    let box_view: View = NumberBox::new()
        .value(value)
        .minimum(minimum)
        .maximum(maximum)
        .min_width(120.0)
        .on_value_changed(on_change)
        .into();

    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .children((
            TextBlock::new()
                .text(label.into())
                .font_size(theme::FONT_BODY)
                .vertical_alignment(VerticalAlignment::Center),
            box_view,
        ))
}

/// 单行文本行（排除目录条目）。
pub fn qs_text<C: IntoPayloadCallback<String>>(
    value: &str,
    placeholder: &str,
    on_change: C,
) -> View {
    TextBox::new()
        .text(value.to_string())
        .placeholder_text(placeholder.to_string())
        .min_width(320.0)
        .on_text_changed(on_change)
        .into()
}

/// 分组标题（对话框内小节）。
pub fn qs_section(title: impl Into<String>) -> View {
    TextBlock::new()
        .text(title.into())
        .font_size(theme::FONT_BODY)
        .font_weight(FontWeight::SEMI_BOLD)
        .foreground(theme::solid(theme::CHARCOAL_WARM))
        .margin(Thickness::new(0.0, 6.0, 0.0, 8.0))
        .into()
}

/// 市场条目卡：名称 + 版本 + 描述 + 作者 + 「安装 / 已安装」。
pub fn market_entry<C: IntoUnitCallback>(
    entry: &crate::services::market::MarketEntry,
    english: bool,
    installing: bool,
    on_install: C,
) -> View {
    let title: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(10.0)
        .children((
            TextBlock::new()
                .text(entry.display_name(english))
                .font_size(theme::FONT_BODY)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::near_black()),
            TextBlock::new()
                .text(entry.version_text())
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::OLIVE_GRAY))
                .vertical_alignment(VerticalAlignment::Center),
        ));

    let mut details: Vec<(usize, View)> = Vec::new();
    if !entry.description.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(entry.description.clone())
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::solid(theme::CHARCOAL_WARM))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ));
    }
    if !entry.author.is_empty() {
        details.push((
            details.len(),
            TextBlock::new()
                .text(entry.author.clone())
                .font_size(theme::FONT_CAPTION)
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

    Border::new()
        .padding(theme::pad_md())
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .background(theme::card_background())
        .border_brush(theme::card_stroke())
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_md())
        .content(StackPanel::new().spacing(0.0).keyed_children(rows))
}

/// 声明式设置行：标签 + 提示 + 编辑器（`char`/`text`/`number` 用文本框，`file` 加「选择文件」）。
pub fn setting_row<C: IntoPayloadCallback<String>, F: IntoUnitCallback>(
    setting: &crate::models::PluginSetting,
    value: &str,
    english: bool,
    on_change: C,
    on_pick_file: F,
) -> View {
    let mut children: Vec<(usize, View)> = Vec::new();

    let label = crate::services::plugins::setting_label(setting, english);
    if !label.is_empty() {
        children.push((
            children.len(),
            TextBlock::new()
                .text(label)
                .font_size(theme::FONT_BODY)
                .foreground(theme::near_black())
                .into(),
        ));
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
                .foreground(theme::solid(theme::OLIVE_GRAY))
                .into(),
        ));
    }

    // ⚠️ 0.100.0 的 TextBox 无 `MaxLength`（长度上限由保存前的校验兜底，口径同后端）
    let editor: View = TextBox::new()
        .text(value.to_string())
        .min_width(280.0)
        .on_text_changed(on_change)
        .into();

    // `file` 类型：文本框 + 「选择文件」按钮
    if crate::services::plugins::is_file(setting) {
        let pick: View = Button::new()
            .on_click(on_pick_file)
            .content(TextBlock::new().text(i18n::t("2583")));
        children.push((
            children.len(),
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .children((editor, pick)),
        ));
    } else {
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

/// 一次性操作回显（导入/删除成功提示，文案由调用方按 2431 等格式化）。
pub fn status_banner(message: &str) -> View {
    TextBlock::new()
        .text(message.to_string())
        .font_size(theme::FONT_CAPTION)
        .foreground(theme::solid(theme::MUTED_GREEN))
        .text_wrapping(TextWrapping::Wrap)
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .into()
}
