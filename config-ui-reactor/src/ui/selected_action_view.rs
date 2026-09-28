//! 选中动作页视图构件（无状态布局；回调由调用方注入）。
//!
//! 旧实现：`Views/SelectedActionPageView.axaml`（33KB）+ `SelectedActionPageViewModel.cs`。
//! 页面骨架 = 页头 + 热键卡（手输热键 + 启用开关）+ **两张聚合卡**（文本特征 / 文件后缀：
//! toggle 行 + 详情编辑器）。
//!
//! ⚠️ 与旧版的结构性差异（0.100.0 能力约束，已记 `16-Phase3b-选中动作页.md`）：
//! * **无 `HotkeyCapture`**：reactor `TextBox` 没有键盘事件回调（#19），任意键捕获控件做不了
//!   ⇒ 热键改为**手输 AHK 格式**文本框。
//! * 删除映射**无确认弹窗**（`ContentDialog` 在 0.100.0 会崩溃；确认框需 `open_window`
//!   二级窗口，留待 Phase 3 弹窗批次）——直接删除 + 立即保存。
//! * toggle 行复用缩写页的 chips 配方（`VariableSizedWrapGrid` 统一格宽，#17）。

use windows_reactor::*;

use crate::services::i18n;
use crate::services::selected_action::{BadgeColor, TypeToggle};
use crate::theme;

/// toggle 格宽下限/上限（复用缩写页口径）。
const TOGGLE_MIN_WIDTH: f64 = 53.0;
const TOGGLE_MAX_WIDTH: f64 = 160.0;
/// 旧 `ToggleButton.type-toggle` 的 `MinHeight=30`。
const TOGGLE_HEIGHT: f64 = 30.0;

/// 旧页级输入框描边色（2026-09-23 三轮定色：Sand 与 StoneGray 之间的暖中灰）。
const INPUT_STROKE: Color = Color::rgb(0xc8, 0xc3, 0xb4);

/// 卡片外框（旧 `Border.actionCard`：Ivory 面 + cream 边 2px + 圆角 14 + Padding 16）。
pub fn card(body: View) -> View {
    Border::new()
        .padding(theme::pad_md())
        .background(theme::ivory())
        .border_brush(theme::border_cream())
        .border_thickness(theme::card_border())
        .corner_radius(theme::radius_card())
        .content(body)
}

/// 聚合卡外框（旧 `Border.type-card`：同 `card` 但 Padding 12 + 底距 8）。
pub fn type_card(body: View) -> View {
    Border::new()
        .padding(Thickness::uniform(12.0))
        .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
        .background(theme::ivory())
        .border_brush(theme::border_cream())
        .border_thickness(theme::card_border())
        .corner_radius(theme::radius_card())
        .content(body)
}

/// 热键卡：标签 + 热键输入 + 启用开关 + 条件提示条（未保存 1077 优先于空热键 976）。
pub fn hotkey_card<C1, C2>(
    hotkey: &str,
    enable: bool,
    hint: &str,
    on_hotkey: C1,
    on_enable: C2,
) -> View
where
    C1: IntoPayloadCallback<String>,
    C2: IntoPayloadCallback<bool>,
{
    let input: View = TextBox::new()
        .text(hotkey.to_string())
        .min_width(220.0)
        .min_height(32.0)
        .border_brush(theme::solid(INPUT_STROKE))
        .on_text_changed(on_hotkey)
        .into();

    let switch: View = ToggleSwitch::new()
        .is_on(enable)
        .on_toggled(on_enable)
        .into();

    let row: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(16.0)
        .children((
            StackPanel::new().spacing(4.0).children((
                TextBlock::new()
                    .text(i18n::t("1063"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray()),
                input,
            )),
            TextBlock::new()
                .text(i18n::t("1117"))
                .font_size(theme::FONT_BODY)
                .vertical_alignment(VerticalAlignment::Center),
            switch,
        ));

    let mut panel_children: Vec<(usize, View)> = vec![(0, row)];
    // 提示条（旧 `.hint`）：Sand 面 + 圆角 4 + Padding 12,8 + 字 12
    if !hint.is_empty() {
        panel_children.push((
            1,
            Border::new()
                .background(theme::sand())
                .corner_radius(theme::radius_panel())
                .padding(Thickness::new(12.0, 8.0, 12.0, 8.0))
                .content(
                    TextBlock::new()
                        .text(hint.to_string())
                        .font_size(theme::FONT_CAPTION)
                        .foreground(if hint == i18n::t("976") {
                            theme::solid(theme::ERROR_CRIMSON)
                        } else {
                            theme::solid(theme::DARK_WARM)
                        })
                        .text_wrapping(TextWrapping::Wrap),
                ),
        ));
    }

    // 旧热键卡内 Spacing=10；⚠️ `IntoViews` 未为 `Vec` 实现 ⇒ 动态集合一律 `keyed_children`
    card(
        StackPanel::new()
            .spacing(10.0)
            .keyed_children(panel_children),
    )
}

/// 聚合卡头：标题（旧 15 SemiBold terracotta）+ 删除按钮（未配置时禁用）。
pub fn card_header(title: &str, can_delete: bool, on_delete: impl IntoUnitCallback) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .children((
            TextBlock::new()
                .text(title.to_string())
                .font_size(theme::FONT_CARD_TITLE)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::terracotta())
                .vertical_alignment(VerticalAlignment::Center),
            Button::new()
                .is_enabled(can_delete)
                .on_click(on_delete)
                .content(
                    TextBlock::new()
                        .text(i18n::t("967"))
                        .foreground(theme::solid(theme::ERROR_CRIMSON)),
                ),
        ))
}

/// 类型 toggle 行：chips 换行（`VariableSizedWrapGrid` 统一格宽，按最长标签估算）。
///
/// 选中态 = Sand 底深字（对齐缩写页）；已配置 toggle 前缀一个 ● 状态点。
pub fn toggles_row<F, C>(toggles: &[TypeToggle], selected_id: &str, mut make_callback: F) -> View
where
    F: FnMut(String) -> C,
    C: IntoUnitCallback,
{
    let width = toggles
        .iter()
        .map(|toggle| estimate_width(&toggle.label))
        .fold(0.0_f64, f64::max)
        + 28.0; // 状态点 + 内边距
    let width = width.clamp(TOGGLE_MIN_WIDTH, TOGGLE_MAX_WIDTH);

    let items: Vec<(String, View)> = toggles
        .iter()
        .map(|toggle| {
            let is_selected = toggle.id == selected_id;
            let callback = make_callback(toggle.id.clone());
            (
                format!("{}|{is_selected}", toggle.id),
                build_toggle_button(toggle, is_selected, width, callback),
            )
        })
        .collect();

    VariableSizedWrapGrid::new()
        .item_width(width)
        .item_height(TOGGLE_HEIGHT)
        .orientation(Orientation::Horizontal)
        .keyed_children(items)
}

fn estimate_width(label: &str) -> f64 {
    label
        .chars()
        .map(|character| if character.is_ascii() { 9.0 } else { 17.0 })
        .sum()
}

fn build_toggle_button(
    toggle: &TypeToggle,
    is_selected: bool,
    width: f64,
    on_click: impl IntoUnitCallback,
) -> View {
    let (background, foreground, border) = if is_selected {
        (theme::SAND, theme::NEAR_BLACK, theme::RING_DEEP)
    } else {
        (theme::IVORY, theme::NEAR_BLACK, theme::RING_WARM)
    };

    Button::new()
        .height(TOGGLE_HEIGHT)
        .width(width)
        .resource_overrides(
            ResourceOverrides::new()
                .set("ButtonBackground", background)
                .set("ButtonForeground", foreground)
                .set("ButtonBorderBrush", border),
        )
        .on_click(on_click)
        .content(
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .children((
                    TextBlock::new()
                        .text("●")
                        .font_size(8.0)
                        .foreground(if is_selected {
                            theme::solid(theme::TERRACOTTA)
                        } else {
                            theme::solid(theme::MUTED_GREEN)
                        })
                        .vertical_alignment(VerticalAlignment::Center),
                    TextBlock::new()
                        .text(toggle.label.clone())
                        .font_size(13.0)
                        .vertical_alignment(VerticalAlignment::Center),
                )),
        )
}

/// 详情面板的一行行为编辑器（配 [`row_editor`] 子卡使用）：
/// `序号徽章 | 行为标签（STAR） | 值输入框（无参行为不显示）+ 删除按钮`。
///
/// `badge`（行为色）为旧版自绘方徽章的遗留参数：旧样式 2026-09 起统一为
/// Sand 底 + terracotta 序号（颜色不再承载语义），参数保留以免动作面板联动改造。
#[allow(clippy::too_many_arguments)]
pub fn entry_row<C1, C2>(
    index: usize,
    label: &str,
    value: &str,
    _badge: BadgeColor,
    is_no_value: bool,
    on_value: C1,
    on_remove: C2,
) -> View
where
    C1: IntoPayloadCallback<String>,
    C2: IntoUnitCallback,
{
    // 序号徽章（旧 `rowEditor` 内：Sand 底 + RadiusSm + Padding 7,2 + 字 13 terracotta SemiBold）
    let badge: View = Border::new()
        .grid_column(0)
        .corner_radius(theme::radius_sm())
        .background(theme::sand())
        .padding(Thickness::new(7.0, 2.0, 7.0, 2.0))
        .vertical_alignment(VerticalAlignment::Center)
        .content(
            TextBlock::new()
                .text((index + 1).to_string())
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::terracotta()),
        );

    let name: View = TextBlock::new()
        .grid_column(1)
        .text(label.to_string())
        .font_size(theme::FONT_BODY)
        .vertical_alignment(VerticalAlignment::Center)
        .margin(Thickness::new(10.0, 0.0, 10.0, 0.0))
        .into();

    // 无参行为没有值输入 ⇒ 用零宽占位保持 children 元组结构稳定
    let value_control: View = if is_no_value {
        Border::new().width(0.0).into()
    } else {
        TextBox::new()
            .text(value.to_string())
            .min_width(240.0)
            .min_height(30.0)
            .border_brush(theme::solid(INPUT_STROKE))
            .on_text_changed(on_value)
            .into()
    };

    let remove: View = Button::new()
        .on_click(on_remove)
        .content(TextBlock::new().text(i18n::t("967")));

    let trailing: View = StackPanel::new()
        .grid_column(2)
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((value_control, remove));

    Grid::new()
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .children((badge, name, trailing))
}

/// 行为编辑行的**子卡外框**（旧 `Border.rowEditor`：Ivory 面 + 圆角 4 + Padding 10 + 底距 6）。
pub fn row_editor(body: View) -> View {
    Border::new()
        .background(theme::ivory())
        .corner_radius(theme::radius_panel())
        .padding(Thickness::uniform(10.0))
        .margin(Thickness::new(0.0, 0.0, 0.0, 6.0))
        .content(body)
}

/// 「添加行为」行：覆盖行为下拉 + 添加按钮（`max_entries` 已满 9 时禁用）。
pub fn add_behavior_row<C1, C2>(
    covering_labels: Vec<String>,
    picked: Option<usize>,
    can_add: bool,
    on_pick: C1,
    on_add: C2,
) -> View
where
    C1: IntoPayloadCallback<Option<usize>>,
    C2: IntoUnitCallback,
{
    let combo: View = ComboBox::new()
        .min_width(200.0)
        .is_enabled(can_add)
        .items_source(covering_labels)
        .selected_index(picked)
        .on_selection_changed(on_pick)
        .into();

    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
        .children((
            combo,
            Button::new()
                .is_enabled(can_add)
                .on_click(on_add)
                .content(TextBlock::new().text(i18n::t("1106"))),
        ))
}

/// 未配置类型的「待配置」提示（2537 + 专属行为缺位提示 2517）。
pub fn pending_hint(has_dedicated: bool) -> View {
    let mut children = vec![
        TextBlock::new()
            .text(i18n::t("2537"))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::solid(theme::DARK_WARM)),
    ];

    if !has_dedicated {
        children.push(
            TextBlock::new()
                .text(i18n::t("2517"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .text_wrapping(TextWrapping::Wrap)
                .margin(Thickness::new(0.0, 4.0, 0.0, 0.0)),
        );
    }

    // ⚠️ 同上：`Vec<TextBlock>` 不能直接作为 `IntoViews`
    let rows: Vec<(usize, View)> = children
        .into_iter()
        .enumerate()
        .map(|(index, block)| (index, block.into()))
        .collect();
    // 旧「待配置」容器：Sand 面 + 圆角 4 + Padding 10,8 + 字 12 DarkWarm
    Border::new()
        .background(theme::sand())
        .corner_radius(theme::radius_panel())
        .padding(Thickness::new(10.0, 8.0, 10.0, 8.0))
        .content(StackPanel::new().spacing(4.0).keyed_children(rows))
}

// NOTE(测试边界)：本模块**无单元测试** —— 构件需要 `ComponentContext` 产出的 `Callback`
// （`IntoUnitCallback`/`IntoPayloadCallback` 只为 `Callback` 实现），单测环境无法构造。
// 可判定的部分（toggles/覆盖推导/目录推导）已在 `services::selected_action` 覆盖；
// 视图层由真机 UIA + 截图验收（见 `16-Phase3b-选中动作页.md`）。
