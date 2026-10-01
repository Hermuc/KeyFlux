//! 毛玻璃（材质）策略 —— 窗口背景材质与透明度的**单一决策点**。
//!
//! 动机（2026-09-30 透明度重做）：此前「亚克力开关」的语义散落三处
//! （`theme::set_acrylic` 进程级开关 / `WindowSpec::with_acrylic` /
//! `navigation_glass_resources` 门控），背板材质选型与各层表面 alpha 互不关联。
//! 本模块把「开毛玻璃时整个窗口长什么样」收敛为一个 [`GlassPolicy`]：
//! 背板材质 + 三层自绘表面 alpha + 导航窗格 alpha。各消费方只读策略；
//! 后续任何调参（例如透明度滑条、参数化 `DesktopAcrylicController`）只改这里。
//!
//! 材质选型依据（2026-09-30 调研 + 实测，MS Learn《System backdrops》/《Materials》）：
//! * 关 = `Mica` + 全不透明表面（未引入毛玻璃前的默认观感，逐像素一致）；
//! * 开 = `MicaAlt`（壁纸采样更浓的资源管理器同族材质）+ 表面按 alpha 稀释。
//!   此前开 = `Acrylic`（`DesktopAcrylicBackdrop` 默认参数，不可调）在浅色主题下
//!   呈奶白，用户反馈「不够透明」；Mica Alt 由系统处理节电/「透明效果」关闭时的
//!   自动回退，无需手管 `FallbackColor`。
//!
//! 沿革补记（2026-09-30）：曾三轮试过「完全看见背景」（vendor 参数化
//! `DesktopAcrylicController`，经 `backdrop_shim.rs`），因黑字在暗背景上可读性
//! 代价过大，按用户裁定**整体回退**到本 Mica Alt 版（vendor 补丁已撤销；该路线
//! 的技术结论见 memory-export 五.9，需要时可在 glass.rs 单点恢复）。

use windows_reactor::{Color, ResourceOverrides};

/// 窗口背板材质（平台无关语义；到 reactor 枚举的映射只允许在 `platform` 层做）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassBackdrop {
    /// 不用系统材质（纯色自绘）。当前无策略使用，保留作显式回退。
    Solid,
    /// 系统 Mica（默认桌面融合）。
    Mica,
    /// Mica Alt（壁纸染色更浓，资源管理器同族）。
    MicaAlt,
    /// 桌面亚克力（`DesktopAcrylicBackdrop` 默认参数；首轮观感，已被 MicaAlt 取代）。
    Acrylic,
}

/// 一套完整的毛玻璃观感参数（背板 + 各层表面透明度）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassPolicy {
    pub backdrop: GlassBackdrop,
    /// 大面积页面底 alpha（255=不透明，0=全透）。
    pub page: u8,
    /// 卡片面 alpha（承载正文，可读性优先，是三层中最实的）。
    pub card: u8,
    /// 次级面（内嵌块/弱化卡）alpha。
    pub secondary: u8,
    /// NavigationView 窗格底（白）alpha。
    pub pane: u8,
}

/// 关：Mica 背板 + 全不透明表面（与未引入毛玻璃前逐像素一致）。
const OPAQUE: GlassPolicy = GlassPolicy {
    backdrop: GlassBackdrop::Mica,
    page: 255,
    card: 255,
    secondary: 255,
    pane: 255,
};

/// 开：Mica Alt 背板 + 表面按 alpha 稀释（色相不变）。
const GLASS: GlassPolicy = GlassPolicy {
    backdrop: GlassBackdrop::MicaAlt,
    page: 31,      // ~12%：材质透出最明显
    card: 97,      // ~38%：正文可读性兜底
    secondary: 77, // ~30%
    pane: 36,      // ~14% 白：保留层次又高度透光
};

static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 由 UI 层同步毛玻璃开关（关闭时所有表面恢复不透明 = 与改动前逐像素一致）。
pub fn set_enabled(on: bool) {
    ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// 当前生效策略（渲染路径每帧读取，保持无锁）。
pub fn current() -> &'static GlassPolicy {
    if enabled() { &GLASS } else { &OPAQUE }
}

/// 表面色 → 玻璃色：毛玻璃开启时按 alpha 稀释（保留色相），否则原样返回。
///
/// 依据（MS Learn「Structure a modern WinUI 3 desktop app」）：系统背板材质只在
/// 自身表面不完全不透明处可见 —— 任何不透明背景都会盖住材质。
pub fn dilute(color: Color, alpha: u8) -> Color {
    if enabled() {
        Color::argb(alpha, color.r, color.g, color.b)
    } else {
        color
    }
}

/// 毛玻璃模式下给 NavigationView 的资源覆盖（WinUI 用**主题资源**而非属性控制
/// NavigationView 背景，资源键名见 MS Learn《NavigationView / Pane Backgrounds》）：
/// * `NavigationViewContentBackground` = 全透明 —— 官方文档明确不透明背景盖住材质；
/// * `NavigationViewExpandedPaneBackground` / `...DefaultPaneBackground` =
///   白色按 [`GlassPolicy::pane`] alpha（保留层次又高度透光）。
pub fn navigation_glass_resources() -> ResourceOverrides {
    let pane = Color::argb(current().pane, 0xFF, 0xFF, 0xFF);
    ResourceOverrides::new()
        .set("NavigationViewContentBackground", Color::transparent())
        .set("NavigationViewExpandedPaneBackground", pane)
        .set("NavigationViewDefaultPaneBackground", pane)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    // 策略预设是编译期常量 ⇒ 用模块级 `const _` 做编译期锚定断言（clippy
    // assertions_on_constants 会拦运行期常量断言；编译期判定让违规直接编译失败）：
    // 关 = 全不透明 Mica（默认外观逐像素不变）；开 = 卡片最实 > 次级 > 页面最透。
    const _: () = {
        assert!(matches!(OPAQUE.backdrop, GlassBackdrop::Mica));
        assert!(OPAQUE.page == 255 && OPAQUE.card == 255);
        assert!(OPAQUE.secondary == 255 && OPAQUE.pane == 255);
        assert!(matches!(GLASS.backdrop, GlassBackdrop::MicaAlt));
        assert!(GLASS.card > GLASS.secondary && GLASS.secondary > GLASS.page);
        assert!(GLASS.card < 128 && GLASS.page < 64);
    };

    /// 开关改变表面 alpha；该用例触碰进程级开关 ⇒ 全仓唯一触碰点
    /// （theme 的同名用例已并入此处，避免跨模块并行测试互踩全局量）。
    #[test]
    fn surface_dilution_tracks_policy() {
        set_enabled(false);
        let p = current();
        assert_eq!(p, &OPAQUE);
        for brush in [theme::parchment(), theme::ivory(), theme::sand()] {
            let windows_reactor::Brush::Solid(color) = brush else {
                panic!("surface brushes are solid colors");
            };
            assert_eq!(color.a, 255);
        }
        let plain = theme::IVORY;
        assert_eq!(dilute(plain, 97), plain, "关闭时稀释必须不生效");

        set_enabled(true);
        let p = current();
        assert_eq!(p, &GLASS);
        let windows_reactor::Brush::Solid(card) = theme::ivory() else {
            panic!()
        };
        assert_eq!(card.a, GLASS.card);
        assert_eq!(
            (card.r, card.g, card.b),
            (plain.r, plain.g, plain.b),
            "色相不得改变"
        );
        let windows_reactor::Brush::Solid(page) = theme::parchment() else {
            panic!()
        };
        assert_eq!(page.a, GLASS.page);
        let windows_reactor::Brush::Solid(secondary) = theme::sand() else {
            panic!()
        };
        assert_eq!(secondary.a, GLASS.secondary);

        set_enabled(false);
    }
}
