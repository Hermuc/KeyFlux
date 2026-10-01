//! 缩写命令页视图：**缩写 chips 换行网格** + **命令框**。
//!
//! 与旧 `Views/AbbrPageView.axaml` 的对应：
//! * chips：旧用 `WrapPanel`（每项自然宽度，`MaxWidth=570` 限每行 10 格）；
//!   reactor **无 `WrapPanel`** ⇒ 改用 `VariableSizedWrapGrid`（统一单元尺寸，
//!   宽度由 [`abbr::chip_item_width`] 依最长标签估算）。
//! * 命令框「回车执行」：旧在 code-behind 挂 `KeyDown`。
//!   ⚠️ **0.100.0 无键盘事件 API**（`on_preview_key_down` / `RoutedCallback` 是 master-only，
//!   实测本机 `generated.rs` 中不存在）⇒ 采用 `accepts_return(true)` 让回车**插入换行**，
//!   再由 `on_text_changed` 检测尾部换行触发执行，并额外提供显式「执行」按钮兜底。

use windows_reactor::*;

use crate::services::abbr::{self, AbbrChip};
use crate::theme;

/// 缩写 chips 换行网格（每项一个按钮，字色固定 NearBlack，无绑定态）。
///
/// `item_width` 由调用方用 [`abbr::chip_item_width`] 依标签集算出（统一格宽布局的必然要求）。
pub fn chip_grid<F, C>(chips: &[AbbrChip], item_width: f64, mut make_callback: F) -> View
where
    F: FnMut(AbbrChip) -> C,
    C: IntoUnitCallback,
{
    let items: Vec<(String, View)> = chips
        .iter()
        .enumerate()
        .map(|(index, chip)| {
            let callback = make_callback(chip.clone());
            // ⚠️ key 必须含状态：`resource_overrides` 只在新元素创建时生效（见 13 号文档 §3）
            let key = format!("{index}|{}|{}", chip.selected, chip.enabled);
            (key, chip_button(chip, callback))
        })
        .collect();

    VariableSizedWrapGrid::new()
        .orientation(Orientation::Horizontal)
        .item_width(item_width)
        .item_height(abbr::CHIP_HEIGHT)
        .keyed_children(items)
}

fn chip_button<C: IntoUnitCallback>(chip: &AbbrChip, on_click: C) -> View {
    // 选中 = Sand 底；其余 = Ivory 底（缩写页**没有**绿色绑定态）
    let background = if chip.selected {
        theme::SAND
    } else {
        theme::IVORY
    };

    Button::new()
        .height(abbr::CHIP_HEIGHT)
        .min_width(abbr::CHIP_MIN_WIDTH)
        .is_enabled(chip.enabled)
        .resource_overrides(
            ResourceOverrides::new()
                .set("ButtonBackground", background)
                .set("ButtonForeground", theme::NEAR_BLACK)
                .set("ButtonBorderBrush", theme::RING_SOFT)
                .set("ButtonBackgroundPointerOver", theme::RING_SOFT)
                .set("ButtonBackgroundPressed", theme::RING_STRONG),
        )
        .on_click(on_click)
        .content(
            TextBlock::new()
                .text(chip.label.clone())
                .font_size(17.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        )
}

/// 命令框（多行接受回车；尾部换行即触发执行）。
///
/// `on_text_changed` 收到含尾换行的文本时，调用方应执行命令并清空输入。
pub fn command_box<C: IntoPayloadCallback<String>>(
    value: &str,
    watermark: &str,
    on_text_changed: C,
) -> View {
    TextBox::new()
        .text(value.to_string())
        .placeholder_text(watermark.to_string())
        .accepts_return(true)
        .min_height(36.0)
        .margin(Thickness::new(2.0, 16.0, 0.0, 0.0))
        .on_text_changed(on_text_changed)
        .into()
}

/// 命令框旁的显式执行按钮（回车之外的兜底入口，Fluent 友好）。
pub fn run_button<C: IntoUnitCallback>(label: impl Into<String>, on_click: C) -> View {
    Button::new()
        .margin(Thickness::new(8.0, 16.0, 0.0, 0.0))
        .on_click(on_click)
        .content(TextBlock::new().text(label.into()))
}

/// 无缩写条目时的占位提示。
pub fn empty_hint(message: impl Into<String>) -> View {
    TextBlock::new()
        .text(message.into())
        .font_size(theme::FONT_BODY)
        .foreground(theme::stone_gray())
        .text_wrapping(TextWrapping::Wrap)
        .into()
}
