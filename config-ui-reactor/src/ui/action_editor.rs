//! 动作编辑面板的**布局构件**（无状态；控件与回调由调用方注入）。
//!
//! 与旧 `Views/Controls/ActionEditorPanel.axaml`（19.6KB）的对应：
//! * 两级选择（窗口分组 / 动作类型）→ [`field`] + 调用方传入的 `ComboBox`
//! * 各类型编辑器 → 调用方按 `typeId` 分发（`ui` 只提供表单骨架）
//! * 单选组「两两一行」布局 → [`radio_rows`]（复刻 `RadioGroupEditorVm.BuildRows`）
//!
//! Fluent 化取舍：旧版为固定高度 + 内部滚动 + 精确像素；新版改**自然高度 + 卡片内边距**，
//! 由外层 `ScrollViewer` 承担滚动（与模式页一致）。

use windows_reactor::*;

use crate::services::action_editor::RadioItem;
use crate::theme;

/// 面板外框（卡片）。
pub fn frame(body: View) -> View {
    Border::new()
        .padding(theme::pad_md())
        .background(theme::card_background())
        .border_brush(theme::card_stroke())
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_md())
        .content(body)
}

/// 字段：标签在上、控件在下（纵向），底部留 10px。
pub fn field(label: impl Into<String>, control: View) -> View {
    StackPanel::new()
        .spacing(4.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .children((
            TextBlock::new()
                .text(label.into())
                .font_size(theme::FONT_CAPTION)
                .foreground(theme::stone_gray()),
            control,
        ))
}

/// 字段行（横向：控件在左、标签在右）——用于开关类。
pub fn toggle_row(label: impl Into<String>, control: View) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(10.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
        .children((
            control,
            TextBlock::new()
                .text(label.into())
                .font_size(theme::FONT_BODY)
                .vertical_alignment(VerticalAlignment::Center),
        ))
}

/// 字段级错误提示（暗红，复刻 `WinTitleError` 展示）。
pub fn field_error(message: impl Into<String>) -> View {
    TextBlock::new()
        .text(message.into())
        .font_size(theme::FONT_CAPTION)
        .foreground(theme::solid(theme::ERROR_CRIMSON))
        .text_wrapping(TextWrapping::Wrap)
        .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
        .into()
}

/// 文本框（`multiline = true` 时接受回车，用于按键串/代码等长文本）。
pub fn text_box<C: IntoPayloadCallback<String>>(
    value: &str,
    multiline: bool,
    on_change: C,
) -> View {
    TextBox::new()
        .text(value.to_string())
        .accepts_return(multiline)
        .min_height(if multiline { 72.0 } else { 32.0 })
        .on_text_changed(on_change)
        .into()
}

/// 开关行（开关在左、标签在右，复刻旧版三开关的排布语义）。
pub fn toggle<C: IntoPayloadCallback<bool>>(
    label: impl Into<String>,
    is_on: bool,
    on_toggle: C,
) -> View {
    let switch: View = crate::ui::compact_switch(is_on, on_toggle);
    toggle_row(label, switch)
}

/// 下拉框（`items` 为展示文本；`selected` 为当前索引）。
pub fn combo<C: IntoPayloadCallback<Option<usize>>>(
    items: Vec<String>,
    selected: Option<usize>,
    enabled: bool,
    on_selection: C,
) -> View {
    ComboBox::new()
        .is_enabled(enabled)
        .min_width(160.0)
        .items_source(items)
        .selected_index(selected)
        .on_selection_changed(on_selection)
        .into()
}

/// 区块间的细分隔线。
pub fn divider() -> View {
    Border::new()
        .height(1.0)
        .background(theme::border_cream())
        .margin(Thickness::new(0.0, 4.0, 0.0, 12.0))
        .content(TextBlock::new().text(""))
}

/// 面板内提示文字（无选中键 / 未配置等）。
pub fn hint(message: impl Into<String>) -> View {
    TextBlock::new()
        .text(message.into())
        .font_size(theme::FONT_BODY)
        .foreground(theme::stone_gray())
        .text_wrapping(TextWrapping::Wrap)
        .into()
}

/// 单选组布局：**每行最多 2 组**，组内纵向排列、组间横向等距（复刻 `BuildRows` + `RadioGroup.vue`）。
///
/// `group_name_base` 参与 `RadioButton::group_name`：同一组内互斥，不同组互不干扰。
pub fn radio_rows<F, C>(
    rows: &[Vec<Vec<RadioItem>>],
    group_name_base: &str,
    selected_value_id: i32,
    mut make_callback: F,
) -> View
where
    F: FnMut(RadioItem) -> C,
    C: IntoPayloadCallback<bool>,
{
    let row_views: Vec<(usize, View)> = rows
        .iter()
        .enumerate()
        .map(|(row_index, groups)| {
            let columns: Vec<(usize, View)> = groups
                .iter()
                .enumerate()
                .map(|(column_index, group)| {
                    // ⚠️ keyed diff 的 key 只接受 `usize`/`String`（`i32` 不满足 `Key: From<i32>`）
                    let items: Vec<(usize, View)> = group
                        .iter()
                        .enumerate()
                        .map(|(item_index, item)| {
                            let callback = make_callback(*item);
                            let radio: View = RadioButton::new()
                                .group_name(format!("{group_name_base}-{row_index}-{column_index}"))
                                .is_checked(item.value_id == selected_value_id)
                                .on_checked(callback)
                                .content(item.label());
                            (item_index, radio)
                        })
                        .collect();
                    (
                        column_index,
                        StackPanel::new().spacing(4.0).keyed_children(items),
                    )
                })
                .collect();
            (
                row_index,
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(28.0)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
                    .keyed_children(columns),
            )
        })
        .collect();

    StackPanel::new().spacing(4.0).keyed_children(row_views)
}
