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

// ------------------------------------------- 「快捷键方案」三列契约（表头 ⇄ 数据行共用）

/// 第 2 列（触发键）宽。
const SCHEME_COL_HOTKEY: f64 = 130.0;
/// 第 3 列（开关）宽 —— 取值见 [`scheme_columns`] 的推导：贴近数据行原本 `Auto` 的实测宽度，
/// 这样「把列宽钉死」不会挪动开关自身的位置。
///
/// 🔴 2026-10-02：96 → 104。96 是拿 `ON`（墨迹 ≈18.4 DIP）反推的上界，**量错了字符串**
/// —— `OFF`（最宽那条，墨迹 ≈24 DIP）在 96 下只剩 ≈20 DIP 槽位被裁成 `OF|`（用户报障）。
/// 教训：**固定宽容器的契约测试必须拿"最宽的那条字符串"去量**（见
/// [`scheme_switch_column_fits_switch_and_indicator`]）。
const SCHEME_COL_SWITCH: f64 = 104.0;
/// 第 4 列（删除钮）宽 —— 红 ✕ 图形钮（icon_button）含 WinUI 默认 padding
/// 自然宽 ≈40 DIP；36 曾把 ✕ 裁掉右缘（用户截图 2026-10-02）。
const SCHEME_COL_DELETE: f64 = 44.0;
/// 第 2 列内容左缩进（表头与数据行经 [`scheme_inset`] 共用，不可能只改一边）。
const SCHEME_INSET_HOTKEY: f64 = 8.0;
/// 第 3 列内容左缩进（表头与数据行经 [`scheme_inset`] 共用，不可能只改一边）。
const SCHEME_INSET_SWITCH: f64 = 16.0;
/// 第 4 列内容左缩进（表头与数据行共用）。
const SCHEME_INSET_DELETE: f64 = 4.0;

/// 「快捷键方案」三列宽度（**表头与数据行共用同一份**）。
///
/// 🔴 表头与数据行是**两个独立的 Grid**（卡片里隔着 StackPanel 的间距），而 WinUI 的 `Auto`
/// 列只能按**各自 Grid 内**的内容测量 —— 第 3 列曾用 `Auto`：表头量到的是「开关」二字，
/// 数据行量到的是 [开关 + 8 间隔 + ON/OFF 字]，两张 Grid 的第 3 列**实测相差 ≈56 DIP**。
/// 该差值把表头的前两列整体右推，于是「触发键」「开关」不再落在其组件的正上方
/// （用户报障；像素实测 @125%：`触发键` 偏右 61px、`开关` 偏右 69px）。
/// 叠加第 2 列缩进表头漏了那 8px ⇒ 再偏 8。**修法 = 列宽一律固定 + 逐列缩进两边相同。**
///
/// 第 3 列取 104：数据行原 `Auto` 的宽度实测落在 **92~96 DIP**（两条互相独立的像素实测：
/// ① 由"第 3 列宽度差 56 DIP + 表头自身 ≈40 DIP"反解得 ≈95；② 由"卡片内容宽 503 − 名称列
/// 281 − 触发键列 130"直接解得 ≈92）。96 曾拿 `ON` 反推、裁掉 `OFF`（见
/// [`SCHEME_COL_SWITCH`] 注释），2026-10-02 放宽到 104 ⇒ 开关位置左移 ≤8 DIP
/// （表头与数据行同步移动，对齐不变），`OFF` 槽位 ≥28 DIP。
///
/// 第 1 列仍是 STAR：它没有"需要与另一个 Grid 对齐的同名列头"之外的约束，且固定 124px 会把
/// 「CapsLock + Space」截断（旧注释的结论仍然成立）。
///
/// ⚠️ 更彻底的做法是把表头与数据行合进**同一个** Grid（表头行 + N 个数据行），那样用 `Auto`
/// 也能对齐；但那是页面结构的重构（`views.rs::settings_page` 的组装方式也要改），收益不抵风险，
/// 故本轮只固化列契约。**改动本页布局时，表头与数据行必须继续共用本函数与 [`scheme_inset`]。**
fn scheme_columns() -> [GridLength; 4] {
    [
        GridLength::STAR,
        GridLength::Pixel(SCHEME_COL_HOTKEY),
        GridLength::Pixel(SCHEME_COL_SWITCH),
        GridLength::Pixel(SCHEME_COL_DELETE),
    ]
}

/// 第 `column` 列**内容容器**的左缩进 —— 表头与数据行的唯一来源。
///
/// 把缩进收进一个函数（而不是两边各写一遍 `Thickness::new(...)`）是刻意的：列宽固定只解决了
/// 大半偏移，剩下的第 2 列 8px 差正是"表头没抄数据行那个 margin"造成的。共用本函数后，
/// **两边缩进不可能再分叉**。
fn scheme_inset(column: usize) -> Thickness {
    match column {
        1 => Thickness::new(SCHEME_INSET_HOTKEY, 0.0, 0.0, 0.0),
        2 => Thickness::new(SCHEME_INSET_SWITCH, 0.0, 0.0, 0.0),
        3 => Thickness::new(SCHEME_INSET_DELETE, 0.0, 0.0, 0.0),
        _ => Thickness::new(0.0, 0.0, 0.0, 0.0),
    }
}

/// 「快捷键方案」表头（501 名称 / 502 触发键 / 504 开关）。
///
/// 列定义与逐列缩进一律走 [`scheme_columns`] / [`scheme_inset`]（与 [`scheme_row`] 同源）——
/// 这是「标题落在组件正上方、且与组件左边缘对齐」的**唯一保证**。
pub fn scheme_header() -> View {
    Grid::new().columns(scheme_columns()).children((
        TextBlock::new()
            .text(i18n::t("501"))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::stone_gray()),
        Border::new()
            .grid_column(1)
            .margin(scheme_inset(1))
            .content(
                TextBlock::new()
                    .text(i18n::t("502"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray()),
            ),
        // 第 4 列（删除钮）无列头文字，仅占位保持列契约对齐
        Border::new().grid_column(3),
        Border::new()
            .grid_column(2)
            .margin(scheme_inset(2))
            .content(
                TextBlock::new()
                    .text(i18n::t("504"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray()),
            ),
    ))
}

/// 「快捷键方案」行：名称(501) / 触发键(502) / 开关(504) 三列内联编辑。
/// 行距 8px（此前 2px 过挤，12 行连成一片）；开关列左留 [`scheme_inset`]`(2)`。
/// 列定义与缩进与 [`scheme_header`] 同源（见 [`scheme_columns`] 的"为什么必须固定"）。
#[allow(clippy::too_many_arguments)] // 造型函数: 名称/触发键/开关/删除四组件+三回调, 拆结构体反而晦涩
pub fn scheme_row<N, H, E, D>(
    name: &str,
    hotkey: &str,
    enabled: bool,
    can_delete: bool,
    on_name: N,
    on_hotkey: H,
    on_enable: E,
    on_delete: D,
) -> View
where
    N: IntoPayloadCallback<String>,
    H: IntoPayloadCallback<String>,
    E: IntoPayloadCallback<bool>,
    D: IntoUnitCallback,
{
    // 名称框收窄（用户定版 2026-10-02）：STAR 列仍对齐表头，框自身限宽左对齐
    let name_box: View = TextBox::new()
        .text(name.to_string())
        .max_width(240.0)
        .min_height(32.0)
        .horizontal_alignment(HorizontalAlignment::Left)
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
        .columns(scheme_columns())
        .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
        .children((
            name_box,
            Border::new()
                .grid_column(1)
                .margin(scheme_inset(1))
                .content(hotkey_box),
            Border::new()
                .grid_column(2)
                .margin(scheme_inset(2))
                .vertical_alignment(VerticalAlignment::Center)
                .content(switch),
            Border::new()
                .grid_column(3)
                .margin(scheme_inset(3))
                .vertical_alignment(VerticalAlignment::Center)
                .content(crate::ui::icon_button(
                    "✕",
                    15.0,
                    theme::ERROR_CRIMSON,
                    // 已启用方案不可删除（用户定版 2026-10-02）：禁用态由 WinUI
                    // 自动灰化内容。
                    can_delete && !enabled,
                    on_delete,
                )),
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
    let delete: View = crate::ui::icon_button("✕", 15.0, theme::ERROR_CRIMSON, true, on_delete);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 列契约：开关列的**固定**宽度必须容得下「缩进 + 开关轨道 + 间隔 + ON/OFF 字」。
    ///
    /// 列宽被钉死之后就不再随内容自适应 ⇒ 窄了会把开关或指示字裁掉（这是"固化列宽"引入的
    /// **新**风险，用它兜住）。
    ///
    /// 说明：本用例**不**（也无法）证明表头与数据行对齐 —— `View` 不可内省，比对不了两张 Grid
    /// 的实测列宽。对齐由结构保证（两边同走 [`scheme_columns`] / [`scheme_inset`]），
    /// 真值由**像素回读**判定。数值用 `let` 而非 `const` 绑定，避免 clippy 的
    /// `assertions_on_constants`（与 `keymap_view::grid_max_height_fits_default_layout_with_margin` 同法）。
    #[test]
    fn scheme_switch_column_fits_switch_and_indicator() {
        let track = 40.0; // WinUI ToggleSwitch 轨道宽
        let gap = 8.0; // 开关与指示字之间 StackPanel::spacing
        // 🔴 必须拿**最宽的那条字符串**量：`OFF` 实测墨迹 ≈24 DIP（`ON` 仅 ≈18.4）。
        // 96 时代拿 `ON` 反推留 24 上界 ⇒ `OFF` 被裁成 `OF|`（2026-10-02 用户报障）。
        // 上界再放 4 DIP 余量 = 28。
        let indicator = 28.0;
        let needed = SCHEME_INSET_SWITCH + track + gap + indicator;
        assert!(
            SCHEME_COL_SWITCH >= needed,
            "SCHEME_COL_SWITCH({SCHEME_COL_SWITCH}) 容不下开关列内容（需要 {needed}）"
        );
    }
}
