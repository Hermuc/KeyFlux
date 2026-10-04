//! 主题令牌 PoC：验证 reactor 的 Color / Brush / CornerRadius / Thickness 表达力。
//! 令牌取自已归档的 Claude 暖色体系（取值仅作 PoC 参考，正式主题将重新设计）。

use windows_reactor::{Brush, Color, CornerRadius, Thickness};

pub const PARCHMENT: Color = Color::rgb(0xf5, 0xf4, 0xed);
pub const IVORY: Color = Color::rgb(0xfa, 0xf9, 0xf5);
pub const SAND: Color = Color::rgb(0xe8, 0xe6, 0xdc);
pub const TERRACOTTA: Color = Color::rgb(0xc9, 0x64, 0x42);
pub const CORAL: Color = Color::rgb(0xd9, 0x77, 0x57);
pub const TEXT_PRIMARY: Color = Color::rgb(0x14, 0x14, 0x13);
pub const TEXT_MUTED: Color = Color::rgb(0x5e, 0x5d, 0x59);
/// 描边色：取「可见起点」WarmSilver（相对卡片底约 2.11:1）。
pub const BORDER: Color = Color::rgb(0xb0, 0xae, 0xa5);
pub const MUTED_GREEN: Color = Color::rgb(0x5e, 0x7d, 0x5a);

pub fn solid(c: Color) -> Brush {
    Brush::Solid(c)
}

/// 系统主题画刷（验证 ThemeBrush 通路），如 Accent / CardBackground。
pub fn accent() -> Brush {
    Brush::Theme(windows_reactor::ThemeBrush::Accent)
}

pub fn radius_md() -> CornerRadius {
    CornerRadius::uniform(8.0)
}

pub fn radius_sm() -> CornerRadius {
    CornerRadius::uniform(4.0)
}

pub fn pad_md() -> Thickness {
    Thickness::uniform(16.0)
}

pub fn hairline() -> Thickness {
    Thickness::uniform(1.0)
}
