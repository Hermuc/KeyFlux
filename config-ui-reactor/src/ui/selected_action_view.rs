//! 选中动作页视图构件（无状态布局；回调由调用方注入）。
//!
//! 旧实现：`Views/SelectedActionPageView.axaml`（33KB）+ `SelectedActionPageViewModel.cs`。
//! 页面骨架 = 页头 + 热键卡（手输热键 + 启用开关）+ **两张聚合卡**（文本特征 / 文件后缀：
//! toggle 行 + 详情编辑器）。
//!
//! 几何以旧版截图为基线复刻（2026-09-29 对齐轮）：页头标题与三入口同排、说明条
//! Sand 横幅、热键卡（标题+开关同排 / 全宽输入行 + ✕ 清除）、类型 toggle **胶囊**
//! （高 36 / 半径 18 / 选中陶土底白字）、行编辑器（行为名胶囊头 + 序号方块 +
//! 下拉 + ↑↓✕ / 无参提示 1013+1014 / 工作目录占位 1015）。
//!
//! ⚠️ 与旧版的结构性差异（0.100.0 能力约束，已记 `16/18 号`）：
//! * **无 `HotkeyCapture`**：reactor `TextBox` 没有键盘事件回调（#19）⇒ 热键手输。
//! * toggle 换行用 `VariableSizedWrapGrid`（统一格宽，reactor 无 WrapPanel）。

use windows_reactor::*;

use crate::services::i18n;
use crate::services::selected_action::TypeToggle;
use crate::theme;

/// toggle 胶囊格宽下限/上限（旧版胶囊按标签自然宽；reactor 无 WrapPanel ⇒ 统一格宽）。
const TOGGLE_MIN_WIDTH: f64 = 78.0;
const TOGGLE_MAX_WIDTH: f64 = 180.0;
/// 旧版胶囊高度（截图实测 ≈36）。
const TOGGLE_HEIGHT: f64 = 36.0;
/// 胶囊圆角（= 高度一半，old pill 造型）。
const TOGGLE_RADIUS: f64 = 18.0;

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

/// 热键卡（旧截图复刻）：标题「快捷键」+ ON/OFF + 开关同排；下一行全宽输入行
/// `[⌨ 徽章 | 热键输入 | ✕ 清除]`；条件提示条 Sand 横幅垫底。
pub fn hotkey_card<C1, C2, C3>(
    hotkey: &str,
    enable: bool,
    hint: &str,
    on_hotkey: C1,
    on_enable: C2,
    on_clear: C3,
) -> View
where
    C1: IntoPayloadCallback<String>,
    C2: IntoPayloadCallback<bool>,
    C3: IntoUnitCallback,
{
    // 卡头：标题（15 SemiBold）+ 右侧 [ON/OFF 指示 + 开关]（复刻旧 `.axaml:388-400`）
    let state = if enable { "ON" } else { "OFF" };
    let head: View = Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .children((
            TextBlock::new()
                .text(i18n::t("1063"))
                .font_size(theme::FONT_CARD_TITLE)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::near_black())
                .vertical_alignment(VerticalAlignment::Center),
            StackPanel::new()
                .grid_column(1)
                .orientation(Orientation::Horizontal)
                .spacing(12.0)
                .children((
                    TextBlock::new()
                        .text(state)
                        .font_size(theme::FONT_CAPTION)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .foreground(if enable {
                            theme::solid(theme::MUTED_GREEN)
                        } else {
                            theme::stone_gray()
                        })
                        .vertical_alignment(VerticalAlignment::Center),
                    // 内置「开/关」文案置空（只保留左侧 ON/OFF 指示，避免重复）
                    ToggleSwitch::new()
                        .is_on(enable)
                        .on_toggled(on_enable)
                        .slots(crate::ui::empty_on_off_slots()),
                )),
        ));

    // 全宽输入行：⌨ 徽章 + 热键输入 + ✕ 清除（旧 HotkeyCapture 的形；手输能力受 #19 限制）
    let icon: View = Border::new()
        .width(28.0)
        .height(28.0)
        .corner_radius(theme::radius_panel())
        .background(theme::sand())
        .vertical_alignment(VerticalAlignment::Center)
        .content(
            TextBlock::new()
                .text("⌨")
                .font_size(13.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        );
    let clear: View = Button::new().on_click(on_clear).content(
        TextBlock::new()
            .text("✕")
            .foreground(theme::solid(theme::ERROR_CRIMSON)),
    );
    let input_row: View = Grid::new()
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .children((
            Border::new()
                .margin(Thickness::new(0.0, 0.0, 10.0, 0.0))
                .content(icon),
            Border::new().grid_column(1).content(
                TextBox::new()
                    .text(hotkey.to_string())
                    .min_height(40.0)
                    .border_brush(theme::solid(INPUT_STROKE))
                    .on_text_changed(on_hotkey),
            ),
            Border::new()
                .grid_column(2)
                .margin(Thickness::new(10.0, 0.0, 0.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center)
                .content(clear),
        ));

    let mut panel_children: Vec<(usize, View)> = vec![(0, head), (1, input_row)];
    // 提示条（旧 `.hint`）：Sand 面 + 圆角 4 + Padding 12,8
    if !hint.is_empty() {
        panel_children.push((2, hint_bar(hint)));
    }

    // 旧热键卡内 Spacing=12；⚠️ `IntoViews` 未为 `Vec` 实现 ⇒ 动态集合一律 `keyed_children`
    card(
        StackPanel::new()
            .spacing(12.0)
            .keyed_children(panel_children),
    )
}

/// Sand 说明横幅（旧 960 说明条 / 热键卡提示条共用造型：Sand 面 + 圆角 4 + Padding 12,8）。
pub fn hint_bar(text: &str) -> View {
    Border::new()
        .background(theme::sand())
        .corner_radius(theme::radius_panel())
        .padding(Thickness::new(12.0, 8.0, 12.0, 8.0))
        .content(
            TextBlock::new()
                .text(text.to_string())
                .font_size(13.0)
                .foreground(theme::solid(theme::DARK_WARM))
                .text_wrapping(TextWrapping::Wrap),
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
            {
                // CTA（旧「＋ 添加规则」）：陶土实底 + 白字 + PointerOver 提亮
                let cta_overrides = ResourceOverrides::new()
                    .set("ButtonBackground", theme::TERRACOTTA)
                    .set("ButtonForeground", theme::WHITE)
                    .set("ButtonBorderBrush", theme::TERRACOTTA)
                    .set("ButtonBackgroundPointerOver", theme::CORAL)
                    .set("ButtonForegroundPointerOver", theme::WHITE)
                    .set("ButtonBorderBrushPointerOver", theme::CORAL);
                let cta: View = Button::new()
                    .resource_overrides(cta_overrides)
                    .on_click(on_add)
                    .content(
                        TextBlock::new()
                            .text(format!("＋ {}", i18n::t("1105")))
                            .foreground(theme::WHITE),
                    );
                cta
            },
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
    // 旧截图：▶/✕ **右对齐到卡边**（无边框图形按钮；▶ 用 U+25B8 避开 emoji 回退）
    let ghost = ResourceOverrides::new()
        .set("ButtonBackground", Color::transparent())
        .set("ButtonBorderBrush", Color::transparent())
        .set("ButtonBackgroundPointerOver", Color::transparent())
        .set("ButtonBorderBrushPointerOver", Color::transparent());
    Grid::new()
        .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
        .children((
            TextBlock::new()
                .text(title.to_string())
                .font_size(theme::FONT_CARD_TITLE)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::terracotta())
                .vertical_alignment(VerticalAlignment::Center),
            Button::new()
                .grid_column(1)
                .is_enabled(can_play)
                .resource_overrides(ghost.clone())
                .on_click(on_play)
                .content(
                    TextBlock::new()
                        .text("▸")
                        .font_size(16.0)
                        .foreground(theme::near_black()),
                ),
            Border::new()
                .grid_column(2)
                .margin(Thickness::new(8.0, 0.0, 4.0, 0.0))
                .content(
                    Button::new()
                        .is_enabled(can_delete)
                        .resource_overrides(ghost)
                        .on_click(on_delete)
                        .content(
                            TextBlock::new()
                                .text("✕")
                                .font_size(15.0)
                                .foreground(theme::solid(theme::ERROR_CRIMSON)),
                        ),
                ),
        ))
}

/// 类型 toggle 行：**自然宽度胶囊** + 按估宽手动换行。
///
/// 旧版为 `WrapPanel` 自然宽（reactor 无 WrapPanel ⇒ 按标签估宽分行，每行一个
/// 横排 StackPanel）；胶囊 = Border 半径 18（Button 的 `ControlCornerRadius`
/// 资源覆盖实测不生效，改用 Border + `on_pointer_pressed` 承载点击——全透明之外的
/// 任意背景即可命中）。
pub fn toggles_row<F, C>(toggles: &[TypeToggle], selected_id: &str, mut make_callback: F) -> View
where
    F: FnMut(String) -> C,
    C: IntoPayloadCallback<PointerEventInfo>,
{
    // 估算可用行宽：卡内容 ≈ 700（卡 760 − 内边距 40 − 卡边）
    const LINE_WIDTH: f64 = 690.0;
    const SPACING: f64 = 10.0;

    // 贪心分行：宽度 = 估宽 + 胶囊左右内边距 44
    let widths: Vec<f64> = toggles
        .iter()
        .map(|toggle| estimate_width(&toggle.label) + 44.0)
        .collect();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut cursor = 0.0_f64;
    for (index, width) in widths.iter().enumerate() {
        let current = lines.last_mut().expect("至少一行");
        if !current.is_empty() && cursor + SPACING + width > LINE_WIDTH {
            lines.push(Vec::new());
            cursor = 0.0;
        }
        if !lines.last().expect("至少一行").is_empty() {
            cursor += SPACING;
        }
        lines.last_mut().expect("至少一行").push(index);
        cursor += width;
    }

    let rows: Vec<(usize, View)> = lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(row_index, line)| {
            let chips: Vec<(usize, View)> = line
                .into_iter()
                .map(|index| {
                    let toggle = &toggles[index];
                    let is_selected = toggle.id == selected_id;
                    let callback = make_callback(toggle.id.clone());
                    (index, build_toggle_pill(toggle, is_selected, callback))
                })
                .collect();
            (
                row_index,
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(SPACING)
                    .keyed_children(chips),
            )
        })
        .collect();

    StackPanel::new()
        .orientation(Orientation::Vertical)
        .spacing(SPACING)
        .keyed_children(rows)
}

fn estimate_width(label: &str) -> f64 {
    label
        .chars()
        .map(|character| if character.is_ascii() { 9.0 } else { 17.0 })
        .sum()
}

/// 单个胶囊（Border 承载造型与命中；选中 = 陶土底白字，未选 = 白底暖边）。
fn build_toggle_pill(
    toggle: &TypeToggle,
    is_selected: bool,
    on_click: impl IntoPayloadCallback<PointerEventInfo>,
) -> View {
    let (background, foreground, border) = if is_selected {
        (theme::TERRACOTTA, theme::WHITE, theme::TERRACOTTA)
    } else {
        (theme::WHITE, theme::NEAR_BLACK, theme::RING_WARM)
    };

    Border::new()
        .corner_radius(CornerRadius::uniform(TOGGLE_RADIUS))
        .background(theme::solid(background))
        .border_brush(theme::solid(border))
        .border_thickness(theme::hairline())
        .padding(Thickness::new(20.0, 7.0, 20.0, 7.0))
        .vertical_alignment(VerticalAlignment::Center)
        .on_pointer_pressed(on_click)
        .content(
            TextBlock::new()
                .text(toggle.label.clone())
                .font_size(13.0)
                .foreground(theme::solid(foreground))
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        )
}

/// 详情面板的一行行为编辑器（配 [`row_editor`] 子卡使用），复刻旧 `EntryRowVm` 截图造型：
/// 行为名胶囊头（菜单序 + 显示名）→ `[序号方块 | 行为下拉 | ↑ | ↓ | ✕]` →
/// 无参行为显示 1013+1014 提示（有参行为显示模板框 1012）→ 工作目录框（占位 1015，全宽）。
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
    // 行为名胶囊头（旧 `.axaml:69-73`：白底暖边圆角胶囊，菜单序 + 行为显示名）
    let pill: View = Border::new()
        .corner_radius(CornerRadius::uniform(14.0))
        .border_brush(theme::solid(theme::RING_WARM))
        .border_thickness(theme::hairline())
        .background(theme::solid(theme::WHITE))
        .padding(Thickness::new(12.0, 4.0, 12.0, 4.0))
        .horizontal_alignment(HorizontalAlignment::Left)
        .content(
            TextBlock::new()
                .text(format!(
                    "{}  {}",
                    index + 1,
                    switch_items
                        .get(switch_selected.unwrap_or(usize::MAX))
                        .cloned()
                        .unwrap_or_default()
                ))
                .font_size(13.0)
                .foreground(theme::near_black()),
        );

    // 序号方块（陶土底白字，26×26 圆角 4）
    let badge: View = Border::new()
        .width(26.0)
        .height(26.0)
        .corner_radius(theme::radius_sm())
        .background(theme::sand())
        .vertical_alignment(VerticalAlignment::Center)
        .content(
            TextBlock::new()
                .text((index + 1).to_string())
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::terracotta())
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        );

    // 行为下拉：切换即重置该行为默认模板（复刻 `EntryRowVm` 的 `OnBehaviorChanged`）
    let switch_combo: View = ComboBox::new()
        .min_height(36.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .items_source(switch_items)
        .selected_index(switch_selected)
        .on_selection_changed(on_switch)
        .into();

    // ↑ ↓ ✕ = ghost 紧凑排（旧截图：无边框细符号）
    let ghost = ResourceOverrides::new()
        .set("ButtonBackground", Color::transparent())
        .set("ButtonBorderBrush", Color::transparent())
        .set("ButtonBackgroundPointerOver", Color::transparent())
        .set("ButtonBorderBrushPointerOver", Color::transparent());
    let up: View = Button::new()
        .is_enabled(can_up)
        .resource_overrides(ghost.clone())
        .on_click(on_up)
        .content(
            TextBlock::new()
                .text("↑")
                .font_size(15.0)
                .foreground(theme::stone_gray()),
        );
    let down: View = Button::new()
        .is_enabled(can_down)
        .resource_overrides(ghost.clone())
        .on_click(on_down)
        .content(
            TextBlock::new()
                .text("↓")
                .font_size(15.0)
                .foreground(theme::stone_gray()),
        );
    // 旧版删除 = 淡红 ✕（tooltip 1108 因 reactor 无 Tooltip API 暂缺）
    let remove: View = Button::new()
        .resource_overrides(ghost)
        .on_click(on_remove)
        .content(
            TextBlock::new()
                .text("✕")
                .font_size(15.0)
                .foreground(theme::solid(theme::ERROR_CRIMSON)),
        );

    let selector: View = Grid::new()
        .columns([
            GridLength::Auto,
            GridLength::STAR,
            GridLength::Auto,
            GridLength::Auto,
            GridLength::Auto,
        ])
        .children((
            Border::new()
                .vertical_alignment(VerticalAlignment::Center)
                .content(badge),
            Border::new()
                .grid_column(1)
                .margin(Thickness::new(10.0, 0.0, 12.0, 0.0))
                .content(switch_combo),
            Border::new()
                .grid_column(2)
                .vertical_alignment(VerticalAlignment::Center)
                .content(up),
            Border::new()
                .grid_column(3)
                .vertical_alignment(VerticalAlignment::Center)
                .content(down),
            Border::new()
                .grid_column(4)
                .margin(Thickness::new(4.0, 0.0, 4.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center)
                .content(remove),
        ));

    // 无参行为：1013+1014 提示（复刻旧 `.axaml:75-77`）；有参行为：模板框（1012）
    let body: View = if is_no_value {
        TextBlock::new()
            .text(format!("{}{}", i18n::t("1013"), i18n::t("1014")))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::stone_gray())
            .text_wrapping(TextWrapping::Wrap)
            .into()
    } else {
        Grid::new()
            .columns([GridLength::Pixel(64.0), GridLength::STAR])
            .row_spacing(6.0)
            .children((
                TextBlock::new()
                    .text(i18n::t("1012"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .vertical_alignment(VerticalAlignment::Center),
                Border::new().grid_column(1).content(
                    TextBox::new()
                        .text(value.to_string())
                        .min_height(34.0)
                        .border_brush(theme::solid(INPUT_STROKE))
                        .on_text_changed(on_value),
                ),
            ))
    };

    // 工作目录：全宽 + 占位 1015（旧 `.axaml:81-83` 的 placeholder 造型）
    let working: View = TextBox::new()
        .text(working_dir.to_string())
        .placeholder_text(i18n::t("1015"))
        .min_height(34.0)
        .border_brush(theme::solid(INPUT_STROKE))
        .on_text_changed(on_working_dir)
        .into();

    StackPanel::new()
        .spacing(10.0)
        .children((pill, selector, body, working))
}

/// 行为编辑行的**子卡外框**（旧 `Border.rowEditor`：Ivory 面 + 圆角 4 + Padding 10 + 底距 6）。
pub fn row_editor(body: View) -> View {
    Border::new()
        .background(theme::solid(theme::WHITE))
        .border_brush(theme::solid(theme::RING_WARM))
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_panel())
        .padding(Thickness::uniform(14.0))
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
                .foreground(theme::terracotta())
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

/// 卡内「＋ 新建匹配类型」文字链（2553；旧 `TypeCardVm.CreateType` 入口）。
pub fn new_type_link(on_click: impl IntoUnitCallback) -> View {
    Button::new()
        .resource_overrides(
            ResourceOverrides::new()
                .set("ButtonBackground", Color::transparent())
                .set("ButtonBorderBrush", Color::transparent())
                .set("ButtonBackgroundPointerOver", Color::transparent())
                .set("ButtonBorderBrushPointerOver", Color::transparent()),
        )
        .on_click(on_click)
        .content(
            TextBlock::new()
                .text(i18n::t("2553"))
                .font_size(13.0)
                .foreground(theme::terracotta()),
        )
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
