//! UI 层：块模型/状态 → WinUI 控件的构建器。
//!
//! 与 `services` 的分工：`services` 只做纯逻辑（解析/契约/HTTP），**不含任何控件**；
//! 本层负责把纯逻辑产物翻译成 reactor `View`。这样纯逻辑可被单元测试直接覆盖，
//! 而视图构造保持「薄」——避免旧版 `MarkdownRenderer` 那种解析与控件深度交织。

use windows_reactor::*;

use crate::theme;

pub mod abbr_view;
pub mod action_editor;
pub mod doc_assets;
pub mod keymap_view;
pub mod markdown_view;
pub mod plugins_view;
pub mod selected_action_view;
pub mod settings_view;

/// 无边框图形按钮（▶ 测试 / ✕ 删除等图标钮的**唯一构造点**）。
///
/// 2026-10-01 用户定版：此类按钮一律无边框无底色。WinUI Button 默认模板带
/// 边框底色，须覆盖 **全部 8 个视觉态**资源键（含 Pressed/Disabled——只盖静态
/// 态会在按下时闪回默认边框）；回调走 `on_click`。
///
/// 2026-10-02 用户定版：**禁用态图标置灰**（`STONE_GRAY`）——ghost 覆盖透明化了
/// WinUI 自身的禁用灰化，若仍按传入色（多为红色 ✕）渲染，会"看着能点、点了没反应"
/// （快捷键方案启用行删除钮报障）。
pub fn ghost_button_overrides() -> ResourceOverrides {
    let transparent = Color::transparent();
    ResourceOverrides::new()
        .set("ButtonBackground", transparent)
        .set("ButtonBorderBrush", transparent)
        .set("ButtonBackgroundPointerOver", transparent)
        .set("ButtonBorderBrushPointerOver", transparent)
        .set("ButtonBackgroundPressed", transparent)
        .set("ButtonBorderBrushPressed", transparent)
        .set("ButtonBackgroundDisabled", transparent)
        .set("ButtonBorderBrushDisabled", transparent)
}

pub fn icon_button<C: IntoUnitCallback>(
    glyph: &str,
    font_size: f64,
    color: Color,
    enabled: bool,
    on_click: C,
) -> View {
    Button::new()
        .is_enabled(enabled)
        .resource_overrides(ghost_button_overrides())
        .on_click(on_click)
        .content(
            TextBlock::new()
                .text(glyph.to_string())
                .font_size(font_size)
                .foreground(theme::solid(if enabled {
                    color
                } else {
                    theme::STONE_GRAY
                })),
        )
}

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

/// ON/OFF 状态指示字的**唯一取色点**（纯函数 —— 把"配色契约"变成可断言的函数，
/// 而不是埋在构造器里的三元表达式）。
///
/// * `ON` → [`crate::theme::ACCENT`]（**主题强调色**）。2026-10-01 由状态绿 `MUTED_GREEN`
///   改为 `ACCENT`：开关本身渲染的就是系统强调蓝，紧挨着它的 `ON` 字再用绿，等于在同一行
///   并列两种强调色，与「整体配色统一协调」相悖。对照度（相对卡片底 `Ivory`）：
///   绿 4.38 → 蓝 3.70；仍高于同为 12px 小字的 `OFF` 次级墨色（`STONE_GRAY` 3.47），
///   即"未劣化到本页既有的可读性基线以下"。
/// * `OFF` → [`crate::theme::stone_gray`]：表示"未启用"，**不应**抢强调。
pub fn on_off_color(is_on: bool) -> Color {
    if is_on {
        crate::theme::ACCENT
    } else {
        crate::theme::STONE_GRAY
    }
}

/// ON/OFF 状态指示字（配合 [`empty_on_off_slots`] 使用，替代内置「开/关」）。
///
/// 措辞取 i18n `2423`/`2424`（现值 zh/en 均为 `ON`/`OFF`），字号 =
/// [`crate::theme::FONT_CAPTION`]（12）。
///
/// **本函数是 ON/OFF 指示字的唯一实现**，三处开关（快捷键方案 / 选中动作 / 插件卡）都必须走它：
/// 前两页曾各复制一份同规格的私有拷贝；插件卡那第三份还**漏了
/// [`VerticalAlignment::Center`]** —— 横向 `StackPanel` 给子级的槽位高 = 面板高（开关 ≈32 DIP），
/// `TextBlock` 默认 `Stretch` 于是占满槽位、文本贴上沿，`ON` 比开关中心高约 10 DIP
/// （2026-10-01 用户报障「ON 的文字提示和开关按钮没有对齐」）。
/// 教训：**别在页面里手搓状态字** —— 拷贝越少，"某一页漏了某个属性"的漂移入口越少。
pub fn on_off_indicator(is_on: bool) -> View {
    TextBlock::new()
        .text(crate::services::i18n::t(if is_on {
            "2423"
        } else {
            "2424"
        }))
        .font_size(crate::theme::FONT_CAPTION)
        .font_weight(FontWeight::SEMI_BOLD)
        .foreground(crate::theme::solid(on_off_color(is_on)))
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}

/// 紧凑开关（**唯一的 ToggleSwitch 构造点**，勿在页面里裸建 `ToggleSwitch::new()`）。
///
/// 🔴 WinUI `ToggleSwitch` 默认样式经 `ToggleSwitchThemeMinWidth` 内建
/// `MinWidth=154`（`microsoft-ui-xaml#3652`）——**即使 On/Off 内容为空也预留 154px**。
/// 于是「开关 + 右侧 ON/OFF 字」的排布里，状态字被推到约 154px 之外（2026-09-29 用户报告
/// 选中动作/插件/选项三页「ON 离开关太远」的根因）。官方 workaround = 直接在控件上设
/// `MinWidth`（改 `ToggleSwitchThemeMinWidth` 资源实测无效）⇒ 这里统一 `min_width(0.0)`。
///
/// 同时置空内置「开/关」文案（见 [`empty_on_off_slots`]），由调用方在右侧自行放
/// [`on_off_indicator`] 或自定义状态字。
pub fn compact_switch<C: IntoPayloadCallback<bool>>(is_on: bool, on_toggle: C) -> View {
    ToggleSwitch::new()
        .is_on(is_on)
        .on_toggled(on_toggle)
        .min_width(0.0)
        .slots(empty_on_off_slots())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 配色契约：`ON` 指示字必须取**主题强调色**，`OFF` 取次级墨色。
    ///
    /// 钉住这条是因为 `ON` 的颜色曾长期是状态绿 `MUTED_GREEN` —— 开关本身渲染的是系统
    /// 强调蓝，紧挨着的 `ON` 字再用绿，等于同一行并列两种强调色（2026-10-01 用户报障：
    /// `ON` 应"与主题色一致"）。`assert_ne!` 是防回退闸门：改回绿即红。
    #[test]
    fn on_off_indicator_uses_theme_accent_for_on() {
        assert_eq!(on_off_color(true), crate::theme::ACCENT);
        assert_eq!(on_off_color(false), crate::theme::STONE_GRAY);
        assert_ne!(
            on_off_color(true),
            crate::theme::MUTED_GREEN,
            "ON 不应回到状态绿"
        );
    }
}
