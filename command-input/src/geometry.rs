//! 几何公式 (R11/R12/R13/R28; 纯逻辑): DPI 采集结果 → 窗口/白框/网格/字号 的像素值。
//!
//! 公式全部**截断取整** (R11: int(), 非四舍五入 —— X=497.5 → 497 是活体定案)。
//! 闭环基准 (R11): dpi=120、windowWidth=700、windowYPos=0.25、屏 1920×1200
//! → W=925、H=200、X=497、Y=300。R12: 仅创建期调用一次, 之后用存值。

use crate::config;
use crate::skin::Skin;

/// 窗口矩形 (屏幕坐标) 与渲染消费的框内布局。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// R11: 窗口矩形 (截断取整)。
pub fn window_rect(dpi_x: f64, dpi_y: f64, screen_w: i32, screen_h: i32, s: &Skin) -> FrameGeom {
    let w = ((s.window_width + config::WIDTH_MARGIN_DIP) * dpi_x / config::DIP_BASE_DPI) as i32;
    let h = (config::HEIGHT_DIP * dpi_y / config::DIP_BASE_DPI) as i32;
    let x = ((screen_w as f64 - w as f64) * config::SCREEN_CENTER_RATIO) as i32;
    let y = (screen_h as f64 * s.window_y_pos) as i32;
    FrameGeom { x, y, w, h }
}

/// R13: 可见白框 = 窗口矩形四周各缩 band px (42px @125%; 引擎 CommandBoxAnchor 同款常数)。
/// `band_inset_px(dpi) = round(33.6 × dpi / 96)` (42px/1.25 的 DIP 化, design A §2.4)。
pub fn band_inset_px(dpi: f64) -> i32 {
    (config::BAND_INSET_DIP * dpi / config::DIP_BASE_DPI).round() as i32
}

/// R13: 白框矩形 (窗口客户区坐标): (inset, inset) → (W−inset, H−inset)。
pub fn content_rect_client(win: FrameGeom, dpi: f64) -> (i32, i32, i32, i32) {
    let inset = band_inset_px(dpi);
    (
        inset,
        inset,
        (win.w - 2 * inset).max(0),
        (win.h - 2 * inset).max(0),
    )
}

/// R28/参考实现 :143-145: 网格间距 = round(20 DIP × dpi / 96) 且 ≥8 (@125% = 25px)。
pub fn grid_step_px(dpi: f64) -> i32 {
    let step = (config::GRID_STEP_DIP * dpi / config::DIP_BASE_DPI).round() as i32;
    step.max(config::GRID_STEP_MIN_PX)
}

/// R28/参考实现 :146: 首线偏移 = 间距 − 1 (@125% = 24px, 图1 实测)。
pub fn grid_first_offset(step: i32) -> i32 {
    step - 1
}

/// R23/R28: 字号像素 = round(44.0 DIP × dpi / 96) (@125% = 55px; CreateFontW 取负值)。
pub fn font_height_px(dpi: f64) -> i32 {
    (config::FONT_HEIGHT_DIP * dpi / config::DIP_BASE_DPI).round() as i32
}

/// 原版文字排版 (2026-10-04 活体定案, probe_diag.py): 每字形占**固定步距**
/// = 4×网格步距 DIP (80 DIP → 100px @125%; 原版 8 字符 pitch 总和 700.5px/7 = 100.07px),
/// 字形居中于各自单元格, 整串以白框中心为轴 (单字符中心 463 vs 框中心 462.5)。
pub const TEXT_PITCH_GRID_CELLS: f64 = 4.0;

/// 文字步距像素 = round(4 × 20 DIP × dpi / 96) (@125% = 100px)。
pub fn text_pitch_px(dpi: f64) -> i32 {
    (TEXT_PITCH_GRID_CELLS * config::GRID_STEP_DIP * dpi / config::DIP_BASE_DPI).round() as i32
}

/// 参考实现 :69 口径: 文字水平内边距 = round(白框高 × 0.14)。
pub fn text_pad_px(frame_h: i32) -> i32 {
    (frame_h as f64 * config::TEXT_PAD_RATIO).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_skin() -> Skin {
        crate::skin::DEFAULT
    }

    /// R11 数值闭环: dpi=120、700、0.25、1920×1200 → 925/200/497/300 (spec.md:138)。
    #[test]
    fn r11_closed_loop_125_percent() {
        let g = window_rect(120.0, 120.0, 1920, 1200, &default_skin());
        assert_eq!(g.w, 925); // int((700+40)×120/96)
        assert_eq!(g.h, 200); // int(120×160/96)
        assert_eq!(g.x, 497); // int((1920−925)×0.5) —— 截断, 非 498
        assert_eq!(g.y, 300); // int(1200×0.25)
    }

    /// R11: 截断取整语义 (半值必须向下, 不得四舍五入)。
    #[test]
    fn truncation_not_rounding() {
        let mut s = default_skin();
        s.window_width = 701.0; // (701+40)×120/96 = 926.25 → 926
        s.window_y_pos = 0.251; // 1200×0.251 = 301.2 → 301
        let g = window_rect(120.0, 120.0, 1920, 1200, &s);
        assert_eq!(g.w, 926);
        assert_eq!(g.y, 301);
        s.window_width = 700.5; // 740.5×1.25 = 925.625 → 925 (非 926)
        let g = window_rect(120.0, 120.0, 1920, 1200, &s);
        assert_eq!(g.w, 925);
    }

    /// R11: 几何键必须生效 (windowWidth 参与宽公式, windowYPos 为 Y 乘子)。
    #[test]
    fn geometry_keys_take_effect() {
        let mut s = default_skin();
        s.window_width = 800.0;
        s.window_y_pos = 0.5;
        let g = window_rect(120.0, 120.0, 1920, 1200, &s);
        assert_eq!(g.w, 1050); // (800+40)×1.25
        assert_eq!(g.y, 600); // 1200×0.5
    }

    /// R13: band = 42px @125%。
    #[test]
    fn band_inset_matches_calibration() {
        assert_eq!(band_inset_px(120.0), 42);
        assert_eq!(band_inset_px(96.0), 34);
    }

    /// R13: 白框 = 925×200 四周缩 42 → 841×116。
    #[test]
    fn content_frame_matches_reference() {
        let g = window_rect(120.0, 120.0, 1920, 1200, &default_skin());
        let (cx, cy, cw, ch) = content_rect_client(g, 120.0);
        assert_eq!((cx, cy, cw, ch), (42, 42, 841, 116));
    }

    /// R28: 网格 25px / 首线 24px @125% (图1 实测)。
    #[test]
    fn grid_layout_matches_calibration() {
        let step = grid_step_px(120.0);
        assert_eq!(step, 25);
        assert_eq!(grid_first_offset(step), 24);
        assert_eq!(grid_step_px(96.0), 20);
        // 下限 8 (参考实现 :144-145)
        let mut s = default_skin();
        let _ = &mut s;
        assert_eq!(grid_step_px(30.0), 8); // round(20×30/96)=6 → clamp 8
    }

    /// R23: 字号 55px @125% (=44 DIP)。
    #[test]
    fn font_height_matches_calibration() {
        assert_eq!(font_height_px(120.0), 55);
        assert_eq!(font_height_px(96.0), 44);
    }

    /// 原版排版活体定案: 步距 100px @125% (=4×20 DIP; 实测 pitch 100.07px)。
    #[test]
    fn text_pitch_matches_calibration() {
        assert_eq!(text_pitch_px(120.0), 100);
        assert_eq!(text_pitch_px(96.0), 80);
    }

    /// 参考实现 :69: pad = round(框高 × 0.14) → 116×0.14 = 16.24 → 16。
    #[test]
    fn text_pad_matches_reference() {
        assert_eq!(text_pad_px(116), 16);
    }
}
