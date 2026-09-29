//! 主题令牌（Claude 暖色体系 → reactor 基元）。
//!
//! ⚠️ reactor 0.100 没有 `ResourceDictionary`/`Style`/`Setter`，也没有阴影 API，
//! 所以旧版「覆盖 Fluent 资源键」的换肤路线**不存在**；本模块用 reactor 基元
//! （`Color` / `Brush` / `CornerRadius` / `Thickness` / `WindowTheme`）自建主题层。
//!
//! 取值与旧 `Styles/Skins/Claude.axaml` + `ViewModels/ClaudePalette.cs` **逐色对齐**
//! （便于迁移期 A/B 对照）；阴影改用 1px 描边替代（见 [`hairline`] 与 `border_warm`）。
//!
//! 对比度基线（WCAG 非文字元素 3:1；相对卡片底 `Ivory`）——Phase 5 视觉验收判据：
//! `BorderCream` 1.10（几乎不可见，仅作分隔）· `BorderWarm` 1.19 · `RingWarm` 1.48 ·
//! `RingDeep` 1.73 · `StoneGray` 3.47 ✅ · `Terracotta` 3.70 ✅ · `Coral` 2.96（聚焦环）。

use windows_reactor::{Brush, Color, CornerRadius, Thickness};

// ---------------------------------------------------------------- 色板（逐色对齐旧皮肤）

pub const PARCHMENT: Color = Color::rgb(0xf5, 0xf4, 0xed);
pub const IVORY: Color = Color::rgb(0xfa, 0xf9, 0xf5);
pub const SAND: Color = Color::rgb(0xe8, 0xe6, 0xdc);
pub const NEAR_BLACK: Color = Color::rgb(0x14, 0x14, 0x13);
pub const CHARCOAL_WARM: Color = Color::rgb(0x4d, 0x4c, 0x48);
pub const OLIVE_GRAY: Color = Color::rgb(0x5e, 0x5d, 0x59);
pub const STONE_GRAY: Color = Color::rgb(0x87, 0x86, 0x7f);
pub const DARK_WARM: Color = Color::rgb(0x3d, 0x3d, 0x3a);
pub const TERRACOTTA: Color = Color::rgb(0xc9, 0x64, 0x42);
pub const CORAL: Color = Color::rgb(0xd9, 0x77, 0x57);
pub const ERROR_CRIMSON: Color = Color::rgb(0xb5, 0x33, 0x33);
/// 纯白（键格选中态底色）。
pub const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);
/// 柔和暖绿（键格「已绑定」底色）。
pub const MUTED_GREEN_SOFT: Color = Color::rgb(0xe7, 0xeb, 0xe3);
pub const BORDER_CREAM: Color = Color::rgb(0xf0, 0xee, 0xe6);
pub const BORDER_WARM: Color = Color::rgb(0xe8, 0xe6, 0xdc);
pub const RING_WARM: Color = Color::rgb(0xd1, 0xcf, 0xc5);
pub const RING_DEEP: Color = Color::rgb(0xc2, 0xc0, 0xb6);

/// 兼容旧命名（Phase 1 PoC 用的 `TEXT_PRIMARY`/`TEXT_MUTED`/`BORDER`）。
pub const TEXT_PRIMARY: Color = NEAR_BLACK;
pub const TEXT_MUTED: Color = STONE_GRAY;
pub const BORDER: Color = RING_DEEP;

/// 正向语义色（低饱和暖绿，仅用于状态点）。
pub const MUTED_GREEN: Color = Color::rgb(0x5e, 0x7d, 0x5a);

// ---------------------------------------------------------------- 画刷

pub fn solid(color: Color) -> Brush {
    Brush::Solid(color)
}

pub fn parchment() -> Brush {
    solid(PARCHMENT)
}

pub fn ivory() -> Brush {
    solid(IVORY)
}

pub fn sand() -> Brush {
    solid(SAND)
}

pub fn terracotta() -> Brush {
    solid(TERRACOTTA)
}

pub fn near_black() -> Brush {
    solid(NEAR_BLACK)
}

pub fn stone_gray() -> Brush {
    solid(STONE_GRAY)
}

pub fn border_warm() -> Brush {
    solid(BORDER_WARM)
}

pub fn border_cream() -> Brush {
    solid(BORDER_CREAM)
}

/// 系统主题画刷通路（验证 `ThemeBrush`）：Accent / 卡片底 / 卡片描边。
pub fn accent() -> Brush {
    Brush::Theme(windows_reactor::ThemeBrush::Accent)
}

pub fn card_background() -> Brush {
    Brush::Theme(windows_reactor::ThemeBrush::CardBackground)
}

pub fn card_stroke() -> Brush {
    Brush::Theme(windows_reactor::ThemeBrush::CardStroke)
}

// ---------------------------------------------------------------- 圆角（旧令牌 Sm6/Md8/Lg12/Xl16）

pub fn radius_sm() -> CornerRadius {
    CornerRadius::uniform(6.0)
}

pub fn radius_md() -> CornerRadius {
    CornerRadius::uniform(8.0)
}

pub fn radius_lg() -> CornerRadius {
    CornerRadius::uniform(12.0)
}

pub fn radius_xl() -> CornerRadius {
    CornerRadius::uniform(16.0)
}

/// 旧 `ClaudeRadiusCard`（14）—— 内容卡（插件卡/热键卡/分区卡）统一圆角。
/// 迁移期曾误统一为 `radius_md`(8)，2026-09-28 按旧 UI 比对归还。
pub fn radius_card() -> CornerRadius {
    CornerRadius::uniform(14.0)
}

/// 旧 `ClaudeRadiusPanel`（4）—— 说明条/提示条/空态等次级面板圆角。
pub fn radius_panel() -> CornerRadius {
    CornerRadius::uniform(4.0)
}

// ---------------------------------------------------------------- 间距 / 描边

pub fn pad_sm() -> Thickness {
    Thickness::uniform(8.0)
}

pub fn pad_md() -> Thickness {
    Thickness::uniform(16.0)
}

pub fn pad_lg() -> Thickness {
    Thickness::uniform(24.0)
}

/// 1px 细环 —— **阴影替代方案**（旧版亦为 1px ring；reactor 无阴影 API）。
pub fn hairline() -> Thickness {
    Thickness::uniform(1.0)
}

/// 内容卡描边宽度（旧 2px，2026-09-16 用户裁定「组件框描边有点细」1→2）。
pub fn card_border() -> Thickness {
    Thickness::uniform(2.0)
}

/// 侧栏分隔线（单向）。
pub fn divider_right() -> Thickness {
    Thickness::new(0.0, 0.0, 1.0, 0.0)
}

/// 窗口外框（无边框窗口的 1px 暖环）。
pub fn window_frame_ring() -> Thickness {
    Thickness::uniform(1.0)
}

// ---------------------------------------------------------------- 排版（旧 App.axaml 令牌）

/// UI 字体栈（单源事实）：随包 MiSans 四字重经 `platform::fonts` 进程内私有加载，
/// 由 fork `install_global_ui_font` 覆盖 `ContentControlThemeFontFamily` 全局生效
/// （reactor 无 per-control font_family builder，故不在此消费；此处仅作事实记录）。
pub const UI_FONT: &str = "MiSans, Segoe UI Emoji";
/// 标题（衬线）字体栈（旧版 DESIGN 的 serif 档；reactor 无 font_family 暂不可达）。
pub const SERIF_FONT: &str = "Georgia, Segoe UI Variable, serif";

pub const FONT_TITLE: f64 = 18.0;
pub const FONT_SUBTITLE: f64 = 20.0;
pub const FONT_BODY: f64 = 14.0;
pub const FONT_CAPTION: f64 = 12.0;

/// 页标题（旧 `TextBlock.pageTitle` = 24 Medium；迁移期误用 28/BOLD）。
pub const FONT_PAGE_TITLE: f64 = 24.0;
/// 分栏/区块标题（旧选项页左右列标题 = 16 SemiBold）。
pub const FONT_SECTION_TITLE: f64 = 16.0;
/// 卡片标题（旧 `.pluginName`/`sectionHeader`/`cardTitle` = 15 SemiBold）。
pub const FONT_CARD_TITLE: f64 = 15.0;
/// 徽标/小注文字（旧版本徽标、运行时标注 = 11）。
pub const FONT_BADGE: f64 = 11.0;
/// 开关状态等微字号（旧 ON/OFF 状态文字 = 10）。
pub const FONT_MICRO: f64 = 10.0;

// ---------------------------------------------------------------- 尺寸（旧 MainWindow）

pub const WINDOW_WIDTH: f64 = 1200.0;
pub const WINDOW_HEIGHT: f64 = 760.0;
/// 侧栏宽度（旧 ColumnDefinitions="264,*"）。
pub const SIDEBAR_WIDTH: f64 = 264.0;
/// 旧自绘标题栏高度（改用 `TitleBar` 后为 `WindowTitleBarHeight::Tall`）。
pub const TITLE_BAR_HEIGHT: f64 = 36.0;
pub const LOGO_SIZE: f64 = 52.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_legacy_skin_values() {
        // 逐色对齐旧 Claude.axaml / ClaudePalette.cs（改值即视为主题重设计，需同步此断言）
        assert_eq!(PARCHMENT, Color::rgb(0xf5, 0xf4, 0xed));
        assert_eq!(IVORY, Color::rgb(0xfa, 0xf9, 0xf5));
        assert_eq!(SAND, Color::rgb(0xe8, 0xe6, 0xdc));
        assert_eq!(NEAR_BLACK, Color::rgb(0x14, 0x14, 0x13));
        assert_eq!(TERRACOTTA, Color::rgb(0xc9, 0x64, 0x42));
        assert_eq!(CORAL, Color::rgb(0xd9, 0x77, 0x57));
        assert_eq!(ERROR_CRIMSON, Color::rgb(0xb5, 0x33, 0x33));
        assert_eq!(BORDER_CREAM, Color::rgb(0xf0, 0xee, 0xe6));
        assert_eq!(BORDER_WARM, Color::rgb(0xe8, 0xe6, 0xdc));
        assert_eq!(RING_WARM, Color::rgb(0xd1, 0xcf, 0xc5));
        assert_eq!(RING_DEEP, Color::rgb(0xc2, 0xc0, 0xb6));
        assert_eq!(STONE_GRAY, Color::rgb(0x87, 0x86, 0x7f));
        assert_eq!(
            OLIVE_GRAY,
            Color::rgb(0x5e, 0x5d, 0x59),
            "皮肤 ClaudeOliveGrayBrush"
        );
        assert_eq!(WHITE, Color::rgb(0xff, 0xff, 0xff));
        assert_eq!(MUTED_GREEN_SOFT, Color::rgb(0xe7, 0xeb, 0xe3));
    }

    #[test]
    fn radius_tokens_are_sm_md_lg_xl_card_panel() {
        assert_eq!(radius_sm(), CornerRadius::uniform(6.0));
        assert_eq!(radius_md(), CornerRadius::uniform(8.0));
        assert_eq!(radius_lg(), CornerRadius::uniform(12.0));
        assert_eq!(radius_xl(), CornerRadius::uniform(16.0));
        assert_eq!(radius_card(), CornerRadius::uniform(14.0));
        assert_eq!(radius_panel(), CornerRadius::uniform(4.0));
    }

    #[test]
    fn layout_tokens_match_legacy_window() {
        assert_eq!(WINDOW_WIDTH, 1200.0);
        assert_eq!(WINDOW_HEIGHT, 760.0);
        assert_eq!(SIDEBAR_WIDTH, 264.0);
        assert_eq!(TITLE_BAR_HEIGHT, 36.0);
    }
}
