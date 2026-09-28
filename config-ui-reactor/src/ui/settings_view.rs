//! 选项页视图构件（keymap id=4「Settings」）。
//!
//! 与旧 `Views/SettingsPageView.axaml` 的对应：手风琴分区卡（点击头部开合）+ 左列
//! 「快捷键方案」卡（方案行内直接编辑名称/触发键/开关）。全部走 theme.rs 的 Claude 令牌。

use windows_reactor::*;

use crate::services::i18n;
use crate::theme;

/// 分区卡：头部按钮开合，展开时渲染 `body`（不展开时不渲染，控件随建随弃）。
///
/// 几何对齐旧 `Border.settingsCard`：Ivory 面 + cream 边 2px + 圆角 14 + Padding 16；
/// 头部标题 15 SemiBold（旧 `.sectionHeader`）；卡间距 16（旧右列 `Spacing="16"`）。
pub fn section_card<C: IntoUnitCallback>(
    title: impl Into<String>,
    open: bool,
    on_toggle: C,
    body: View,
) -> View {
    let indicator = if open { "-" } else { "+" };
    let header: View = Button::new().on_click(on_toggle).content(
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .children((
                TextBlock::new()
                    .text(indicator)
                    .font_size(theme::FONT_CARD_TITLE)
                    .foreground(theme::solid(theme::TERRACOTTA)),
                TextBlock::new()
                    .text(title.into())
                    .font_size(theme::FONT_CARD_TITLE)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(theme::near_black()),
            )),
    );

    let mut children: Vec<(usize, View)> = vec![(0, header)];
    if open {
        children.push((1, body));
    }

    Border::new()
        .padding(theme::pad_md())
        .margin(Thickness::new(0.0, 0.0, 0.0, 16.0))
        .background(theme::ivory())
        .border_brush(theme::border_cream())
        .border_thickness(theme::card_border())
        .corner_radius(theme::radius_card())
        .content(StackPanel::new().spacing(8.0).keyed_children(children))
}

/// 标签 + 控件行（旧字段标签列宽 176）。
pub fn field_row(label: impl Into<String>, control: View) -> View {
    Grid::new()
        .columns([GridLength::Pixel(176.0), GridLength::STAR])
        .children((
            TextBlock::new()
                .text(label.into())
                .font_size(theme::FONT_BODY)
                .foreground(theme::solid(theme::CHARCOAL_WARM))
                .vertical_alignment(VerticalAlignment::Center),
            Border::new().grid_column(1).content(control),
        ))
}

/// 文本框行。
pub fn text_field<C: IntoPayloadCallback<String>>(
    label: impl Into<String>,
    value: &str,
    on_change: C,
) -> View {
    let control: View = TextBox::new()
        .text(value.to_string())
        .min_width(200.0)
        .on_text_changed(on_change)
        .into();
    field_row(label, control)
}

/// 开关行。
pub fn toggle_row<C: IntoPayloadCallback<bool>>(
    label: impl Into<String>,
    is_on: bool,
    on_change: C,
) -> View {
    let control: View = ToggleSwitch::new()
        .is_on(is_on)
        .on_toggled(on_change)
        .into();
    field_row(label, control)
}

/// 复选行。
pub fn check_row<C: IntoPayloadCallback<bool>>(
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

/// 下拉行（按索引选中/回调）。
pub fn combo_row<C: IntoPayloadCallback<Option<usize>>>(
    label: impl Into<String>,
    items: &[String],
    selected: usize,
    on_change: C,
) -> View {
    let control: View = ComboBox::new()
        .items_source(items.to_vec())
        .selected_index(selected)
        .min_width(180.0)
        .on_selection_changed(on_change)
        .into();
    field_row(label, control)
}

/// 标签 + 按钮行。
pub fn button_row<C: IntoUnitCallback>(
    label: impl Into<String>,
    text: impl Into<String>,
    on_click: C,
) -> View {
    let control: View = Button::new()
        .on_click(on_click)
        .content(TextBlock::new().text(text.into()));
    field_row(label, control)
}

/// 说明文字（无绑定提示，如 763/911）。
pub fn hint_row(text: impl Into<String>) -> View {
    TextBlock::new()
        .text(text.into())
        .font_size(theme::FONT_CAPTION)
        .foreground(theme::stone_gray())
        .text_wrapping(TextWrapping::Wrap)
        .into()
}

/// 「快捷键方案」表头（501 名称 / 502 触发键 / 504 开关）。
/// 列宽对齐旧表前两列（124 / 120；旧表还有修饰键列与图标列，新版为结构性精简）。
pub fn scheme_header() -> View {
    Grid::new()
        .columns([
            GridLength::Pixel(124.0),
            GridLength::Pixel(120.0),
            GridLength::Auto,
        ])
        .children((
            TextBlock::new()
                .text(i18n::t("501"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray()),
            Border::new().grid_column(1).content(
                TextBlock::new()
                    .text(i18n::t("502"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray()),
            ),
        ))
}

/// 「快捷键方案」行：名称(501) / 触发键(502) / 开关(504) 三列内联编辑。
pub fn scheme_row<N, H, E>(
    name: &str,
    hotkey: &str,
    enabled: bool,
    on_name: N,
    on_hotkey: H,
    on_enable: E,
) -> View
where
    N: IntoPayloadCallback<String>,
    H: IntoPayloadCallback<String>,
    E: IntoPayloadCallback<bool>,
{
    let name_box: View = TextBox::new()
        .text(name.to_string())
        .min_width(120.0)
        .on_text_changed(on_name)
        .into();
    let hotkey_box: View = TextBox::new()
        .text(hotkey.to_string())
        .min_width(90.0)
        .on_text_changed(on_hotkey)
        .into();
    let switch: View = ToggleSwitch::new()
        .is_on(enabled)
        .on_toggled(on_enable)
        .into();

    Grid::new()
        .columns([
            GridLength::Pixel(124.0),
            GridLength::Pixel(120.0),
            GridLength::Auto,
        ])
        .margin(Thickness::new(0.0, 0.0, 0.0, 2.0))
        .children((
            name_box,
            Border::new().grid_column(1).content(hotkey_box),
            Border::new().grid_column(2).content(switch),
        ))
}

/// 「自定义热键」行：热键(404) 内联编辑 + 功能(1117) 展示。
pub fn hotkey_row<C: IntoPayloadCallback<String>>(
    hotkey: &str,
    function: &str,
    on_change: C,
) -> View {
    let hotkey_box: View = TextBox::new()
        .text(hotkey.to_string())
        .min_width(110.0)
        .on_text_changed(on_change)
        .into();
    let function_text: View = TextBlock::new()
        .text(if function.is_empty() { "-" } else { function })
        .font_size(theme::FONT_BODY)
        .foreground(theme::solid(theme::CHARCOAL_WARM))
        .text_wrapping(TextWrapping::Wrap)
        .vertical_alignment(VerticalAlignment::Center)
        .into();

    Grid::new()
        .columns([GridLength::Pixel(100.0), GridLength::STAR])
        .margin(Thickness::new(0.0, 0.0, 0.0, 2.0))
        .children((
            hotkey_box,
            Border::new().grid_column(1).content(function_text),
        ))
}

/// 「路径变量」行：变量名(909) / 路径(910) / 删除(912)。
pub fn pathvar_row<N, V, D>(name: &str, value: &str, on_name: N, on_value: V, on_delete: D) -> View
where
    N: IntoPayloadCallback<String>,
    V: IntoPayloadCallback<String>,
    D: IntoUnitCallback,
{
    let name_box: View = TextBox::new()
        .text(name.to_string())
        .min_width(110.0)
        .on_text_changed(on_name)
        .into();
    let value_box: View = TextBox::new()
        .text(value.to_string())
        .min_width(220.0)
        .on_text_changed(on_value)
        .into();
    let delete: View = Button::new()
        .on_click(on_delete)
        .content(TextBlock::new().text(i18n::t("912")));

    Grid::new()
        .columns([GridLength::Pixel(112.0), GridLength::STAR, GridLength::Auto])
        .margin(Thickness::new(0.0, 0.0, 0.0, 2.0))
        .children((
            name_box,
            Border::new().grid_column(1).content(value_box),
            Border::new().grid_column(2).content(delete),
        ))
}

/// 「程序分组」行：组名(602) / 条件(605-608 下拉) / 窗口标识符(603) / 删除(912)。
pub fn group_row<N, V, C, D>(
    name: &str,
    value: &str,
    condition_index: usize,
    on_name: N,
    on_value: V,
    on_condition: C,
    on_delete: D,
) -> View
where
    N: IntoPayloadCallback<String>,
    V: IntoPayloadCallback<String>,
    C: IntoPayloadCallback<Option<usize>>,
    D: IntoUnitCallback,
{
    let conditions: Vec<String> = ["605", "606", "607", "608"]
        .iter()
        .map(|key| i18n::t(key))
        .collect();
    let name_box: View = TextBox::new()
        .text(name.to_string())
        .min_width(100.0)
        .on_text_changed(on_name)
        .into();
    let condition: View = ComboBox::new()
        .items_source(conditions)
        .selected_index(condition_index)
        .min_width(120.0)
        .on_selection_changed(on_condition)
        .into();
    let value_box: View = TextBox::new()
        .text(value.to_string())
        .min_width(200.0)
        .on_text_changed(on_value)
        .into();
    let delete: View = Button::new()
        .on_click(on_delete)
        .content(TextBlock::new().text(i18n::t("912")));

    Grid::new()
        .columns([
            GridLength::Pixel(120.0),
            GridLength::Pixel(130.0),
            GridLength::STAR,
            GridLength::Auto,
        ])
        .margin(Thickness::new(0.0, 0.0, 0.0, 2.0))
        .children((
            name_box,
            Border::new().grid_column(1).content(condition),
            Border::new().grid_column(2).content(value_box),
            Border::new().grid_column(3).content(delete),
        ))
}

/// 「命令框皮肤」字段行：标签（键号文案）+ 文本框。
pub fn skin_row<C: IntoPayloadCallback<String>>(
    label: impl Into<String>,
    value: &str,
    on_change: C,
) -> View {
    text_field(label, value, on_change)
}
