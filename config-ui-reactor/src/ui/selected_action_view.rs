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
use crate::services::selected_action::TypeToggle;
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

/// 页头右侧动作行：管理匹配类型（2519）/ 管理行为（1083）/ 添加映射（1105）
/// （复刻旧 `SelectedActionPageView.axaml:349-368` 页头三按钮）。
pub fn page_actions<M, B, A>(on_match_types: M, on_behaviors: B, on_add: A) -> View
where
    M: IntoUnitCallback,
    B: IntoUnitCallback,
    A: IntoUnitCallback,
{
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((
            Button::new()
                .on_click(on_match_types)
                .content(TextBlock::new().text(i18n::t("2519"))),
            Button::new()
                .on_click(on_behaviors)
                .content(TextBlock::new().text(i18n::t("1083"))),
            Button::new()
                .on_click(on_add)
                .content(TextBlock::new().text(i18n::t("1105"))),
        ))
}

/// 聚合卡头：标题 + ▶ 测试（990，真实执行）+ 红 ✕ 删除（未配置时禁用）。
/// 旧 `SelectedActionPageView.axaml:124-137`。
pub fn card_header<P, D>(
    title: &str,
    can_play: bool,
    can_delete: bool,
    on_play: P,
    on_delete: D,
) -> View
where
    P: IntoUnitCallback,
    D: IntoUnitCallback,
{
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
                .is_enabled(can_play)
                .on_click(on_play)
                .content(TextBlock::new().text(i18n::t("990"))),
            Border::new().width(1.0),
            Button::new()
                .is_enabled(can_delete)
                .on_click(on_delete)
                .content(
                    TextBlock::new()
                        .text("✕")
                        .foreground(theme::solid(theme::ERROR_CRIMSON)),
                ),
        ))
}

/// 类型 toggle 行：chips 换行（`VariableSizedWrapGrid` 统一格宽，按最长标签估算）。
/// 选中态 = Sand 底深字；状态点已随旧版 2026-09-22 定版移除。
pub fn toggles_row<F, C>(toggles: &[TypeToggle], selected_id: &str, mut make_callback: F) -> View
where
    F: FnMut(String) -> C,
    C: IntoUnitCallback,
{
    let width = toggles
        .iter()
        .map(|toggle| estimate_width(&toggle.label))
        .fold(0.0_f64, f64::max)
        + 20.0; // 内边距
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
            TextBlock::new()
                .text(toggle.label.clone())
                .font_size(13.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        )
}

/// 详情面板的一行行为编辑器（配 [`row_editor`] 子卡使用），复刻旧 `EntryRowVm`：
/// `序号徽章 + 行为下拉（切换重置默认模板）+ ↑↓ 排序 + ✕ 删除`，有参行为带
/// 命令模板框与工作目录框（304）。
#[allow(clippy::too_many_arguments)]
pub fn entry_row<C1, C2, C3, C4, C5, C6>(
    index: usize,
    switch_items: Vec<String>,
    switch_selected: Option<usize>,
    value: &str,
    working_dir: &str,
    is_no_value: bool,
    can_up: bool,
    can_down: bool,
    on_switch: C1,
    on_value: C2,
    on_working_dir: C3,
    on_up: C4,
    on_down: C5,
    on_remove: C6,
) -> View
where
    C1: IntoPayloadCallback<Option<usize>>,
    C2: IntoPayloadCallback<String>,
    C3: IntoPayloadCallback<String>,
    C4: IntoUnitCallback,
    C5: IntoUnitCallback,
    C6: IntoUnitCallback,
{
    // 序号徽章（旧 `rowEditor` 内：Sand 底 + RadiusSm + Padding 7,2 + 字 13 terracotta SemiBold）
    let badge: View = Border::new()
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

    // 行为下拉：切换即重置该行为默认模板（复刻 `EntryRowVm` 的 `OnBehaviorChanged`）
    let switch: View = ComboBox::new()
        .min_width(160.0)
        .items_source(switch_items)
        .selected_index(switch_selected)
        .on_selection_changed(on_switch)
        .into();

    let up: View = Button::new()
        .is_enabled(can_up)
        .on_click(on_up)
        .content(TextBlock::new().text("↑"));
    let down: View = Button::new()
        .is_enabled(can_down)
        .on_click(on_down)
        .content(TextBlock::new().text("↓"));
    // 旧版删除 = 红 ✕（tooltip 1108 因 reactor 无 Tooltip API 暂缺）
    let remove: View = Button::new().on_click(on_remove).content(
        TextBlock::new()
            .text("✕")
            .foreground(theme::solid(theme::ERROR_CRIMSON)),
    );

    let head: View = Grid::new()
        .columns([
            GridLength::Auto,
            GridLength::STAR,
            GridLength::Auto,
            GridLength::Auto,
            GridLength::Auto,
        ])
        .children((
            badge,
            Border::new()
                .grid_column(1)
                .margin(Thickness::new(10.0, 0.0, 10.0, 0.0))
                .content(switch),
            Border::new().grid_column(2).content(up),
            Border::new()
                .grid_column(3)
                .margin(Thickness::new(4.0, 0.0, 4.0, 0.0))
                .content(down),
            Border::new().grid_column(4).content(remove),
        ));

    // 有参行为：命令模板框（2532）+ 工作目录框（2533）；无参行为两者皆无，复刻 isEmpty 隐藏
    let input_grid: View = Grid::new()
        .columns([GridLength::Pixel(64.0), GridLength::STAR])
        .row_spacing(6.0)
        .children((
            TextBlock::new()
                .text(i18n::t("2532"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .vertical_alignment(VerticalAlignment::Center),
            Border::new().grid_column(1).content(
                TextBox::new()
                    .text(value.to_string())
                    .min_height(30.0)
                    .border_brush(theme::solid(INPUT_STROKE))
                    .on_text_changed(on_value),
            ),
            TextBlock::new()
                .grid_row(1)
                .text(i18n::t("2533"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .vertical_alignment(VerticalAlignment::Center),
            Border::new().grid_row(1).grid_column(1).content(
                TextBox::new()
                    .text(working_dir.to_string())
                    .min_height(30.0)
                    .border_brush(theme::solid(INPUT_STROKE))
                    .on_text_changed(on_working_dir),
            ),
        ));
    let inputs: View = if is_no_value {
        View::empty()
    } else {
        Border::new()
            .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
            .content(input_grid)
    };

    StackPanel::new().spacing(0.0).children((head, inputs))
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

/// 「添加行为」行：覆盖行为下拉 + 添加按钮。
/// `hint` = 禁用原因（1107 满 9 / 1119 全部已加），无则不渲染。
pub fn add_behavior_row<C1, C2>(
    covering_labels: Vec<String>,
    picked: Option<usize>,
    can_add: bool,
    hint: Option<String>,
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

    let mut children: Vec<(usize, View)> = vec![
        (0, combo),
        (
            1,
            Button::new()
                .is_enabled(can_add)
                .on_click(on_add)
                .content(TextBlock::new().text(i18n::t("1106"))),
        ),
    ];
    if let Some(hint) = hint {
        children.push((
            children.len(),
            TextBlock::new()
                .text(hint)
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray())
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
        ));
    }

    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
        .keyed_children(children)
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
