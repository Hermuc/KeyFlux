//! 主题令牌（reactor 基元）。
//!
//! ⚠️ reactor 0.100 没有 `ResourceDictionary`/`Style`/`Setter`，也没有阴影 API，
//! 所以旧版「覆盖 Fluent 资源键」的换肤路线**不存在**；本模块用 reactor 基元
//! （`Color` / `Brush` / `CornerRadius` / `Thickness` / `WindowTheme`）自建主题层。
//!
//! **表面色**取值与旧 `Styles/Skins/Claude.axaml` + `ViewModels/ClaudePalette.cs`
//! **逐色对齐**（便于迁移期 A/B 对照）；阴影改用 1px 描边替代（见 [`hairline`]）。
//!
//! ## 冷色化沿革（2026-10-01，两轮用户裁定，同一口径）
//!
//! 旧皮肤是 Claude 暖色体系，但**运行态是毛玻璃 + 冷色壁纸**：`glass::dilute` 只作用于
//! 表面（parchment / ivory / sand），页面与卡面被 Mica Alt 采样的壁纸染成冷色
//! （截图实测页面 `(208,221,228)`、卡面 `(246,248,249)`，Lab b* ≈ −1 ~ −5），
//! 于是残留暖相的令牌就成了「和 UI 不匹配」的那一层。两轮修法同一口径：
//! **保 CIELAB 亮度 L（WCAG 对比度只由 L 决定，故随之逐位不变），只把色相搬到冷侧。**
//!
//! ### ① 描边（裁定：「组件框描边是暖色调的，和 UI 不匹配」）
//!
//! 描边是唯一**不参与 alpha 稀释**的一层，旧暖奶白 `(240,238,230)`（b* = +4.1，色相 100°）
//! 与四周 216~245° 的冷色相族相差约 120° ⇒ 冷底上一条暖线。
//! 修法 = **镜像 b\***：相对 `Ivory` 的对比度 1.10 / 1.19 / 1.48 / 1.73 一个不差。
//!
//! ### ② 强调色 + 墨阶（裁定：「选中动作页面出现大量暖色，全部换成冷色」）
//!
//! * 强调色 `TERRACOTTA → ACCENT`：`#C96442`（L* 54.0 / C 53.0 / h 45°）→
//!   `#4980DB`（L* 54.0 / C 53.1 / **h 282°**）。色相刻意对齐 **WinUI 系统强调色**
//!   `#005FB8`（h 282°），使自绘强调色与开关/系统控件的蓝**同族**，不再出现
//!   「陶土 + 系统蓝」两种强调色互相打架；对白字对比度 3.90 → 3.89（只由 L* 决定）。
//! * hover `CORAL → ACCENT_HOVER`：`#D97757` → `#6591E7`（保 L* 60.5 / C 49.1、同 h 282°）。
//! * 墨阶（近黑 / dark / charcoal / olive / stone）：保 L*、镜像 b*，暖灰 → 冷灰。
//! * **不动**：三个**表面**令牌（parchment / ivory / sand）与 `WHITE`；
//!   以及**语义色** `ERROR_CRIMSON`（✕ 删除 / 错误）、`MUTED_GREEN*`（ON / 已绑定）
//!   —— 危险红与状态绿属功能语义，不随装饰色冷色化（如需一并冷化请单独裁定）。
//!
//! 对比度基线（WCAG 非文字元素 3:1；相对卡片底 `Ivory`）——Phase 5 视觉验收判据：
//! `BorderFaint` 1.10（几乎不可见，仅作分隔）· `BorderSoft` 1.19 · `RingSoft` 1.48 ·
//! `RingStrong` 1.73 · `StoneGray` 3.47 ✅ · `Accent` 3.70 ✅ · `AccentHover` 2.96（聚焦环）。

use windows_reactor::{Brush, Color, CornerRadius, Thickness};

// ---------------------------------------------------------------- 色板
// 表面三色逐色对齐旧皮肤；描边 / 墨阶 / 强调色为冷色（见模块头「冷色化沿革」）。

pub const PARCHMENT: Color = Color::rgb(0xf5, 0xf4, 0xed);
pub const IVORY: Color = Color::rgb(0xfa, 0xf9, 0xf5);
pub const SAND: Color = Color::rgb(0xe8, 0xe6, 0xdc);
/// 正文墨色（旧 `#141413` 冷色对位，L* 6.3 不变）。
pub const NEAR_BLACK: Color = Color::rgb(0x12, 0x14, 0x15);
/// 次级墨色（旧 `ClaudeCharcoalWarm #4D4C48` 的冷色对位，L* 32.3 → 32.2）。
pub const CHARCOAL: Color = Color::rgb(0x46, 0x4d, 0x50);
/// 三级墨色（旧 `ClaudeOliveGray #5E5D59` 的冷色对位，L* 39.5 → 39.4）。
pub const SLATE_GRAY: Color = Color::rgb(0x57, 0x5e, 0x61);
/// 弱化文字（旧 `ClaudeStoneGray #87867F` 的冷色对位，L* 55.8 → 55.8）。
pub const STONE_GRAY: Color = Color::rgb(0x7b, 0x88, 0x8c);
/// 深墨色（旧 `ClaudeDarkWarm #3D3D3A` 的冷色对位，L* 25.7 → 25.7）。
pub const DARK_SLATE: Color = Color::rgb(0x38, 0x3e, 0x40);
/// **品牌强调色**（旧 `ClaudeTerracotta #C96442` 的冷色对位：保 L* 54.0 / C 53.0，
/// 色相 45° → 282°，即与 WinUI 系统强调色 `#005FB8` 同族）。用于 CTA 实底、选中胶囊、
/// 选中键格、分区指示符、链接式文本。
pub const ACCENT: Color = Color::rgb(0x49, 0x80, 0xdb);
/// 强调色 hover / 按下档（旧 `ClaudeCoral #D97757` 的冷色对位，保 L* 60.5 / C 49.1）。
pub const ACCENT_HOVER: Color = Color::rgb(0x65, 0x91, 0xe7);
/// 语义危险色（✕ 删除 / 错误提示）—— **不参与冷色化**。
pub const ERROR_CRIMSON: Color = Color::rgb(0xb5, 0x33, 0x33);
/// 纯白（键格选中态底色）。
pub const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);
/// 柔和暖绿（键格「已绑定」底色）—— 语义色，**不参与冷色化**。
pub const MUTED_GREEN_SOFT: Color = Color::rgb(0xe7, 0xeb, 0xe3);
/// 组件框淡描边（2px 卡框）—— 旧 `ClaudeBorderCream` 的**冷色对应**（L* 94.05 → 94.01）。
pub const BORDER_FAINT: Color = Color::rgb(0xea, 0xee, 0xf6);
/// 次级描边 / 分隔线 —— 旧 `ClaudeBorderWarm` 的冷色对应（L* 91.20 → 91.16）。
/// ⚠️ 旧版此值曾与表面令牌 `SAND` 同值 `#E8E6DC`；冷色化后二者**不再相等**，勿再互用。
pub const BORDER_SOFT: Color = Color::rgb(0xe1, 0xe6, 0xef);
/// 控件细环（1px 胶囊/键格边框）—— 旧 `ClaudeRingWarm` 的冷色对应（L* 83.03 → 82.99）。
pub const RING_SOFT: Color = Color::rgb(0xca, 0xcf, 0xd8);
/// 控件强环 / 按下态 —— 旧 `ClaudeRingDeep` 的冷色对应（L* 77.61 → 77.57）。
pub const RING_STRONG: Color = Color::rgb(0xbb, 0xc0, 0xc9);

/// 兼容旧命名（Phase 1 PoC 用的 `TEXT_PRIMARY`/`TEXT_MUTED`/`BORDER`）。
pub const TEXT_PRIMARY: Color = NEAR_BLACK;
pub const TEXT_MUTED: Color = STONE_GRAY;
pub const BORDER: Color = RING_STRONG;

/// 正向语义色（状态点 / ON 指示）—— 语义色，**不参与冷色化**。
pub const MUTED_GREEN: Color = Color::rgb(0x5e, 0x7d, 0x5a);

// ---------------------------------------------------------------- 画刷

pub fn solid(color: Color) -> Brush {
    Brush::Solid(color)
}

// 表面画刷的透明度全部由 `crate::glass` 策略决定（毛玻璃开 = alpha 稀释，
// 关 = 原色）；色板令牌与透明度参数在此解耦。

pub fn parchment() -> Brush {
    solid(crate::glass::dilute(
        PARCHMENT,
        crate::glass::current().page,
    ))
}

pub fn ivory() -> Brush {
    solid(crate::glass::dilute(IVORY, crate::glass::current().card))
}

pub fn sand() -> Brush {
    solid(crate::glass::dilute(
        SAND,
        crate::glass::current().secondary,
    ))
}

/// 强调色刷（CTA / 选中胶囊 / 选中键格 / 分区指示符）。
pub fn accent_solid() -> Brush {
    solid(ACCENT)
}

pub fn near_black() -> Brush {
    solid(NEAR_BLACK)
}

pub fn stone_gray() -> Brush {
    solid(STONE_GRAY)
}

/// 组件框描边刷（2px 卡框 / 分隔线）。
pub fn border_faint() -> Brush {
    solid(BORDER_FAINT)
}

/// 次级描边刷。
pub fn border_soft() -> Brush {
    solid(BORDER_SOFT)
}

/// 系统主题画刷通路（验证 `ThemeBrush`）：Accent / 卡片底 / 卡片描边。
/// ⚠️ 与自绘 [`ACCENT`] 区分：本函数取的是 **WinUI 系统强调色**（开关、系统控件用），
/// [`accent_solid`] 是本皮肤自绘的强调色。二者色相刻意同族（见模块头 ②）。
pub fn system_accent() -> Brush {
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
        // 表面色逐色对齐旧 Claude.axaml / ClaudePalette.cs（改值即视为主题重设计，需同步此断言）
        assert_eq!(PARCHMENT, Color::rgb(0xf5, 0xf4, 0xed));
        assert_eq!(IVORY, Color::rgb(0xfa, 0xf9, 0xf5));
        assert_eq!(SAND, Color::rgb(0xe8, 0xe6, 0xdc));
        // 强调色 / 墨阶 = 冷色版（2026-10-01 第二轮）：旧暖值为 NEAR_BLACK #141413 /
        // DARK_WARM #3D3D3A / CHARCOAL_WARM #4D4C48 / OLIVE_GRAY #5E5D59 / STONE_GRAY #87867F /
        // TERRACOTTA #C96442 / CORAL #D97757。L* 必须与旧值一致，仅色相搬到冷侧。
        assert_eq!(NEAR_BLACK, Color::rgb(0x12, 0x14, 0x15));
        assert_eq!(DARK_SLATE, Color::rgb(0x38, 0x3e, 0x40));
        assert_eq!(CHARCOAL, Color::rgb(0x46, 0x4d, 0x50));
        assert_eq!(SLATE_GRAY, Color::rgb(0x57, 0x5e, 0x61));
        assert_eq!(STONE_GRAY, Color::rgb(0x7b, 0x88, 0x8c));
        assert_eq!(ACCENT, Color::rgb(0x49, 0x80, 0xdb));
        assert_eq!(ACCENT_HOVER, Color::rgb(0x65, 0x91, 0xe7));
        // 语义色不参与冷色化
        assert_eq!(ERROR_CRIMSON, Color::rgb(0xb5, 0x33, 0x33));
        assert_eq!(MUTED_GREEN, Color::rgb(0x5e, 0x7d, 0x5a));
        assert_eq!(WHITE, Color::rgb(0xff, 0xff, 0xff));
        assert_eq!(MUTED_GREEN_SOFT, Color::rgb(0xe7, 0xeb, 0xe3));
        // 描边色板 = 冷色版（2026-10-01 第一轮）：旧暖值为 #F0EEE6 / #E8E6DC / #D1CFC5 / #C2C0B6，
        // 见模块头「### ① 描边」；L* 与 WCAG 对比度必须与旧值一致，仅色相翻到冷侧。
        assert_eq!(BORDER_FAINT, Color::rgb(0xea, 0xee, 0xf6));
        assert_eq!(BORDER_SOFT, Color::rgb(0xe1, 0xe6, 0xef));
        assert_eq!(RING_SOFT, Color::rgb(0xca, 0xcf, 0xd8));
        assert_eq!(RING_STRONG, Color::rgb(0xbb, 0xc0, 0xc9));
    }

    /// 冷色化防回退闸门（2026-10-01）：描边 / 墨阶 / 强调色必须落在冷侧
    /// （蓝分量 >= 红分量），表面色必须仍为暖色（红 > 蓝）。
    /// 语义色（危险红 / 状态绿）**不在此闸门内** —— 它们是功能语义，允许偏暖。
    #[test]
    fn decorative_palette_is_cool_surfaces_stay_warm() {
        for (name, c) in [
            ("BORDER_FAINT", BORDER_FAINT),
            ("BORDER_SOFT", BORDER_SOFT),
            ("RING_SOFT", RING_SOFT),
            ("RING_STRONG", RING_STRONG),
            ("NEAR_BLACK", NEAR_BLACK),
            ("DARK_SLATE", DARK_SLATE),
            ("CHARCOAL", CHARCOAL),
            ("SLATE_GRAY", SLATE_GRAY),
            ("STONE_GRAY", STONE_GRAY),
            ("ACCENT", ACCENT),
            ("ACCENT_HOVER", ACCENT_HOVER),
        ] {
            assert!(
                c.b > c.r,
                "{name} 应为冷色（蓝 > 红），实测 r={} b={}",
                c.r,
                c.b
            );
        }
        for (name, c) in [("PARCHMENT", PARCHMENT), ("IVORY", IVORY), ("SAND", SAND)] {
            assert!(
                c.r > c.b,
                "{name} 为表面令牌，应保持暖色（红 > 蓝），实测 r={} b={}",
                c.r,
                c.b
            );
        }
    }

    /// 强调色族必须与 WinUI 系统强调色同族（Lab 色相 ≈ 282°），否则自绘强调色会与
    /// 开关/系统控件的蓝打架 —— 这正是第二轮冷色化选 282° 的理由（见模块头 ②）。
    #[test]
    fn accent_shares_hue_family_with_system_accent() {
        for (name, c) in [("ACCENT", ACCENT), ("ACCENT_HOVER", ACCENT_HOVER)] {
            // 冷蓝判据：蓝分量显著高于红分量（≥ 0x28 ≈ 40），保证蓝色相明确而非灰蓝
            assert!(
                c.b as i32 - c.r as i32 >= 0x28,
                "{name} 蓝色相不足（b-r={}），应贴近 WinUI 系统强调色蓝",
                c.b as i32 - c.r as i32
            );
        }
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
