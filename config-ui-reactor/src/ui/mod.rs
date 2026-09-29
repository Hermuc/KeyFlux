//! UI 层：块模型/状态 → WinUI 控件的构建器。
//!
//! 与 `services` 的分工：`services` 只做纯逻辑（解析/契约/HTTP），**不含任何控件**；
//! 本层负责把纯逻辑产物翻译成 reactor `View`。这样纯逻辑可被单元测试直接覆盖，
//! 而视图构造保持「薄」——避免旧版 `MarkdownRenderer` 那种解析与控件深度交织。

use windows_reactor::*;

pub mod abbr_view;
pub mod action_editor;
pub mod keymap_view;
pub mod markdown_view;
pub mod plugins_view;
pub mod selected_action_view;
pub mod settings_view;

/// ToggleSwitch 的「开/关」内置文案置空槽。
///
/// ⚠️ 必须放入**非 null 的空元素**（空文本 TextBlock）：`View::empty()` 会被引擎
/// 当作未设置跳过，OnContent/OffContent 保持 null ⇒ Fluent 模板回退显示系统本地化
/// 「开/关」（2026-09-29 真机实证）。与 [`on_off_indicator`] 搭配可统一为 ON/OFF 指示。
pub fn empty_on_off_slots() -> Vec<SlotView<windows_reactor::ToggleSwitchSlot>> {
    vec![
        SlotView::new(windows_reactor::ToggleSwitchSlot::OnContent, {
            let empty: View = TextBlock::new().text(String::new()).into();
            empty
        }),
        SlotView::new(windows_reactor::ToggleSwitchSlot::OffContent, {
            let empty: View = TextBlock::new().text(String::new()).into();
            empty
        }),
    ]
}

/// ON/OFF 状态指示字（配合 [`empty_on_off_slots`] 使用，替代内置「开/关」）。
pub fn on_off_indicator(is_on: bool) -> View {
    TextBlock::new()
        .text(if is_on { "ON" } else { "OFF" })
        .font_size(12.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .foreground(if is_on {
            crate::theme::solid(crate::theme::MUTED_GREEN)
        } else {
            crate::theme::stone_gray()
        })
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}
