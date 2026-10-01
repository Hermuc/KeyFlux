//! 选项页视图构件（keymap id=4「Settings」）。
//!
//! 与旧 `Views/SettingsPageView.axaml` 的对应：手风琴分区卡（点击头部开合）+ 左列
//! 「快捷键方案」卡（方案行内直接编辑名称/触发键/开关）。全部走 theme.rs 的 Claude 令牌。

use windows_reactor::*;

use crate::services::i18n;
use crate::theme;

/// 分区卡：头部按钮开合，展开时渲染 `body`（不展开时不渲染，控件随建随弃）。
///
/// 几何对齐旧 `Border.settingsCard`：Ivory 面 + 淡冷边 2px + 圆角 14 + Padding 16；
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
                    .foreground(theme::solid(theme::ACCENT)),
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
        .border_brush(theme::border_faint())
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
                .foreground(theme::solid(theme::CHARCOAL))
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
    let control: View = crate::ui::compact_switch(is_on, on_change);
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
/// 名称列 **STAR 自适应**：卡片宽 560（旧版同宽 + 1.12 缩放），固定 124px 会把
/// 「CapsLock + Space」截断；开关列补 504 表头。
pub fn scheme_header() -> View {
    Grid::new()
        .columns([GridLength::STAR, GridLength::Pixel(130.0), GridLength::Auto])
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
            Border::new()
                .grid_column(2)
                .margin(Thickness::new(16.0, 0.0, 0.0, 0.0))
                .content(
                    TextBlock::new()
                        .text(i18n::t("504"))
                        .font_size(theme::FONT_CAPTION)
                        .foreground(theme::stone_gray()),
                ),
        ))
}

/// 「快捷键方案」行：名称(501) / 触发键(502) / 开关(504) 三列内联编辑。
/// 行距 8px（此前 2px 过挤，12 行连成一片）；开关列左留 16px。
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
        .min_height(32.0)
        .on_text_changed(on_name)
        .into();
    let hotkey_box: View = TextBox::new()
        .text(hotkey.to_string())
        .min_width(110.0)
        .min_height(32.0)
        .on_text_changed(on_hotkey)
        .into();
    // 开关统一 ON/OFF 指示在**正右边**（compact_switch 已修 WinUI 默认 MinWidth=154，三页同口径）
    let switch: View = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((
            crate::ui::compact_switch(enabled, on_enable),
            crate::ui::on_off_indicator(enabled),
        ));

    Grid::new()
        .columns([GridLength::STAR, GridLength::Pixel(130.0), GridLength::Auto])
        .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
        .children((
            name_box,
            Border::new()
                .grid_column(1)
                .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                .content(hotkey_box),
            Border::new()
                .grid_column(2)
                .margin(Thickness::new(16.0, 0.0, 0.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center)
                .content(switch),
        ))
}

/// 「自定义热键」列头（404 触发键 / 1117 功能）。
pub fn hotkey_header() -> View {
    Grid::new()
        .columns([GridLength::Pixel(100.0), GridLength::STAR])
        .children((
            TextBlock::new()
                .text(i18n::t("404"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray()),
            Border::new().grid_column(1).content(
                TextBlock::new()
                    .text(i18n::t("1117"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray()),
            ),
        ))
}

/// 「自定义热键」行：热键(404) 内联编辑 + 功能(1117) **可点击编辑**（复刻旧版
/// 「点击弹 ActionEditorWindow」交互）+ 行删除。
pub fn hotkey_row<C, E, D>(
    hotkey: &str,
    function: &str,
    on_change: C,
    on_edit: E,
    on_delete: D,
) -> View
where
    C: IntoPayloadCallback<String>,
    E: IntoUnitCallback,
    D: IntoUnitCallback,
{
    let hotkey_box: View = TextBox::new()
        .text(hotkey.to_string())
        .min_width(110.0)
        .on_text_changed(on_change)
        .into();
    let function_button: View = Button::new().on_click(on_edit).content(
        TextBlock::new()
            .text(if function.is_empty() {
                "-".to_string()
            } else {
                function.to_string()
            })
            .font_size(theme::FONT_BODY)
            .foreground(theme::solid(theme::CHARCOAL))
            .text_wrapping(TextWrapping::Wrap),
    );
    let delete: View = Button::new().on_click(on_delete).content(
        TextBlock::new()
            .text("✕")
            .foreground(theme::solid(theme::ERROR_CRIMSON)),
    );

    Grid::new()
        .columns([GridLength::Pixel(100.0), GridLength::STAR, GridLength::Auto])
        .margin(Thickness::new(0.0, 0.0, 0.0, 2.0))
        .children((
            hotkey_box,
            Border::new().grid_column(1).content(function_button),
            Border::new()
                .grid_column(2)
                .margin(Thickness::new(4.0, 0.0, 0.0, 0.0))
                .content(delete),
        ))
}

/// 「路径变量」列头（909 变量名 / 910 路径）——旧表头在说明行下方、行区上方。
pub fn pathvar_header() -> View {
    Grid::new()
        .columns([GridLength::Pixel(112.0), GridLength::STAR])
        .children((
            TextBlock::new()
                .text(i18n::t("909"))
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray()),
            Border::new().grid_column(1).content(
                TextBlock::new()
                    .text(i18n::t("910"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray()),
            ),
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
