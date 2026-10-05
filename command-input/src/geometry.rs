//! 几何公式 (R11/R12/R13/R28; 纯逻辑): DPI 采集结果 → 窗口/白框/网格/字号 的像素值。
//!
//! 公式全部**截断取整** (R11: int(), 非四舍五入 —— X=497.5 → 497 是活体定案)。
//! 闭环基准 (R11): dpi=120、windowWidth=700、windowYPos=0.25、屏 1920×1200
//! → W=925、H=200、X=497、Y=300。R12: 仅创建期调用一次, 之后用存值。

use crate::compose;
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

// ---- 结果列表面板几何 (2026-10-04: 命令框向下延伸) ----
//
// 展开形态 = 同一个圆角白框向下长高 list_extra_px:
//   [ 查询区 (基准内容高, 网格 + 大字 ) | 分隔线 | 结果行 × N | 底部留白 ]
// 侧边/底部透明带 (band) 与查询区完全同构 ⇒ 轮廓从框顶直线贯通到列表底,
// 与 Flow Launcher 的单窗长高形态一致 (不再需要第二个窗口拼轮廓)。

fn dip_px(dip: f64, dpi: f64) -> i32 {
    (dip * dpi / config::DIP_BASE_DPI).round() as i32
}

/// 结果行高像素 (@125% = 38px)。
pub fn list_row_h_px(dpi: f64) -> i32 {
    dip_px(config::LIST_ROW_DIP, dpi).max(8)
}

/// 结果行**标题**字号像素 (CreateFontW 取负值; @125% = 18px)。
pub fn list_title_font_px(dpi: f64) -> i32 {
    dip_px(config::LIST_TITLE_FONT_DIP, dpi).max(8)
}

/// 结果行**副标题**字号像素 (@125% = 14px)。
pub fn list_sub_font_px(dpi: f64) -> i32 {
    dip_px(config::LIST_SUB_FONT_DIP, dpi).max(6)
}

/// 结果行左侧文件图标边长像素 (@125% = 35px)。
pub fn list_icon_px(dpi: f64) -> i32 {
    dip_px(config::LIST_ICON_DIP, dpi).max(8)
}

/// 图标与文本的水平间距像素 (@125% = 13px)。
pub fn list_icon_gap_px(dpi: f64) -> i32 {
    dip_px(config::LIST_ICON_GAP_DIP, dpi).max(0)
}

/// 查询区/结果区分隔线高像素 (@125% = 1px)。
pub fn list_separator_px(dpi: f64) -> i32 {
    dip_px(config::LIST_SEPARATOR_DIP, dpi).max(1)
}

/// 列表底部留白像素 (@125% = 10px)。
pub fn list_bottom_pad_px(dpi: f64) -> i32 {
    dip_px(config::LIST_BOTTOM_PAD_DIP, dpi).max(0)
}

/// 结果行文本水平内边距像素 (@125% = 20px)。
pub fn list_text_pad_px(dpi: f64) -> i32 {
    dip_px(config::LIST_TEXT_PAD_DIP, dpi).max(0)
}

/// 选中行强调条宽像素 (@125% = 4px)。
pub fn list_accent_px(dpi: f64) -> i32 {
    dip_px(config::LIST_ACCENT_DIP, dpi).max(1)
}

/// 滚动条宽像素 (@125% = 4px)。
pub fn list_scrollbar_px(dpi: f64) -> i32 {
    dip_px(config::LIST_SCROLLBAR_DIP, dpi).max(2)
}

/// 滚动条距内缘像素 (@125% = 10px)。
pub fn list_scrollbar_margin_px(dpi: f64) -> i32 {
    dip_px(config::LIST_SCROLLBAR_MARGIN_DIP, dpi).max(0)
}

/// 列表面板附加高度 (px)。`0` 行 = 无列表 = 不加高 (窗口保持基准几何, R11 不变)。
pub fn list_extra_px(visible_rows: usize, dpi: f64) -> i32 {
    if visible_rows == 0 {
        return 0;
    }
    list_separator_px(dpi) + visible_rows as i32 * list_row_h_px(dpi) + list_bottom_pad_px(dpi)
}

/// 查询区内容底边 (窗口坐标) —— 也是列表分隔线的上沿。
/// = 基准窗口高 − band (>0 时恒等于「白框内查询区的下边界」)。
pub fn query_bottom_px(base_height_px: i32, dpi: f64) -> i32 {
    (base_height_px - band_inset_px(dpi)).max(0)
}

/// 展开后屏幕能容纳的可见行数 (≥1; 兜底不越屏)。
pub fn max_list_rows(dpi: f64, screen_h: i32, y: i32, base_height_px: i32) -> usize {
    let row = list_row_h_px(dpi).max(1);
    let avail = screen_h
        - y
        - base_height_px
        - config::LIST_MIN_SCREEN_MARGIN_PX
        - list_separator_px(dpi)
        - list_bottom_pad_px(dpi);
    if avail < row {
        return 1;
    }
    ((avail / row) as usize).clamp(1, config::LIST_MAX_ROWS)
}

// ---- 逐像素合成几何 (2026-10-04: 白边 / 阴影; 见 crate::compose) ----

/// 白边宽度 (**像素, 保留小数**): 3 DIP @125% = 3.75px。原版实测「3 个纯白 + 1 个
/// 248 弱像素」正是 3.75px 的 AA 表现; 取整成 4px 会多出一列实色 (v1 的 `.round()`
/// 口径即如此, 但 v1 根本没画边 —— 本函数是新口径的唯一来源)。
pub fn ring_width_px(s: &Skin, dpi: f64) -> f64 {
    s.border_width.max(0.0) * dpi / config::DIP_BASE_DPI
}

/// 圆角半径 (像素, 保留小数): 10 DIP @125% = 12.5px (原版圆角实测 dx≈11@dy0)。
pub fn corner_radius_px(s: &Skin, dpi: f64) -> f64 {
    s.border_radius.max(0.0) * dpi / config::DIP_BASE_DPI
}

/// 阴影高斯 σ (像素) = `windowShadowSize`(DIP) × 增益 (实测反解, 见 config 注释)。
pub fn shadow_sigma_px(s: &Skin, dpi: f64) -> f64 {
    s.window_shadow_size.max(0.0) * dpi / config::DIP_BASE_DPI * config::SHADOW_SIGMA_GAIN
}

/// 阴影垂直偏移 (像素; 正 = 向下)。
pub fn shadow_dy_px(dpi: f64) -> f64 {
    config::SHADOW_DY_DIP * dpi / config::DIP_BASE_DPI
}

/// 白框圆角矩形 (像素坐标, 供 `compose::Plan`)。
pub fn frame_shape(s: &Skin, dpi: f64, w: i32, h: i32) -> compose::Shape {
    let inset = band_inset_px(dpi) as f64;
    compose::Shape {
        l: inset,
        t: inset,
        r: w as f64 - inset,
        b: h as f64 - inset,
        radius: corner_radius_px(s, dpi),
    }
}

// ---- 搜索徽标几何 (2026-10-04: 查询区右侧固定图标, 见 crate::badge) ----

/// 徽标字形盒边长 (像素): round(28 DIP × dpi / 96) (@125% = 35px)。
pub fn badge_size_px(dpi: f64) -> i32 {
    dip_px(config::BADGE_SIZE_DIP, dpi).max(8)
}

/// 徽标描边宽 (像素, 保留小数): 1.6 DIP @125% = 2.0px。
pub fn badge_stroke_px(dpi: f64) -> f64 {
    config::BADGE_STROKE_DIP * dpi / config::DIP_BASE_DPI
}

/// 徽标字形盒左上角 (像素)。锚定 = **查询区**右缘内缩 `BADGE_MARGIN_DIP`、查询区垂直居中。
/// 🔴 「固定位置不受布局变化影响」的机制: 查询区几何只依赖**基准高** (R11 创建期定案)
/// 与窗口顶 (0x401 存值, 展开时顶边不动) ⇒ 列表展开/收起只向下长高, 本值恒不变。
pub fn badge_origin_px(width_px: i32, base_height_px: i32, dpi: f64) -> (i32, i32) {
    let inset = band_inset_px(dpi);
    let size = badge_size_px(dpi);
    let margin = dip_px(config::BADGE_MARGIN_DIP, dpi);
    let top = inset;
    let bottom = query_bottom_px(base_height_px, dpi);
    let l = width_px - inset - margin - size;
    let t = top + ((bottom - top) - size) / 2;
    (l.max(0), t.max(0))
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

    /// 搜索徽标布局 @125%: 盒 35px、右缘距框 20px、查询区 (42..158) 垂直居中。
    #[test]
    fn badge_layout_matches_reference() {
        assert_eq!(badge_size_px(120.0), 35);
        assert!((badge_stroke_px(120.0) - 2.0).abs() < 1e-9);
        // R11 闭环几何: 925×200, band 42 ⇒ 查询区 42..158
        let (l, t) = badge_origin_px(925, 200, 120.0);
        assert_eq!(l, 925 - 42 - 20 - 35);
        assert_eq!(t, 42 + (116 - 35) / 2);
        // 列表展开不改基准高 ⇒ 徽标位置逐字节不变 (「固定位置不受布局变化影响」)
        let (l2, t2) = badge_origin_px(925, 200, 120.0);
        assert_eq!((l, t), (l2, t2));
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

    /// 结果列表面板 @125%: 行高 58 (Flow Launcher 58px@1x) / 标题 18 / 副标题 14 /
    /// 图标 35 / 图标间距 13 / 分隔 1 / 底部留白 10 / 文本内边距 20。
    #[test]
    fn list_metrics_at_125() {
        assert_eq!(list_row_h_px(120.0), 58); // round(46 × 1.25) = 58
        assert_eq!(list_title_font_px(120.0), 18); // round(14 × 1.25)
        assert_eq!(list_sub_font_px(120.0), 14); // round(11 × 1.25)
        assert_eq!(list_icon_px(120.0), 35); // round(28 × 1.25)
        assert_eq!(list_icon_gap_px(120.0), 13); // round(10 × 1.25)
        assert_eq!(list_separator_px(120.0), 1); // round(1 × 1.25)
        assert_eq!(list_bottom_pad_px(120.0), 10); // round(8 × 1.25)
        assert_eq!(list_text_pad_px(120.0), 20); // round(16 × 1.25)
        assert_eq!(list_accent_px(120.0), 4); // round(3 × 1.25)
        assert_eq!(list_scrollbar_px(120.0), 4); // round(3 × 1.25)
                                                 // 100% 下即为 DIP 原值
        assert_eq!(list_row_h_px(96.0), 46);
        assert_eq!(list_title_font_px(96.0), 14);
        assert_eq!(list_sub_font_px(96.0), 11);
        assert_eq!(list_icon_px(96.0), 28);
    }

    /// 列表附加高度: 0 行 = 不加高 (基准几何逐字节不变); N 行 = 分隔 + N×行高 + 留白。
    #[test]
    fn list_extra_height() {
        assert_eq!(list_extra_px(0, 120.0), 0);
        assert_eq!(list_extra_px(4, 120.0), 1 + 4 * 58 + 10);
        // 闭环: 11 行 (max_list_rows @1200 高屏的收敛值) @125% → 200 + 649 = 849 高,
        // 底边 300+849=1149 < 1200 (不越屏)
        let base = window_rect(120.0, 120.0, 1920, 1200, &default_skin());
        assert_eq!(list_extra_px(11, 120.0), 649);
        assert_eq!(base.h + list_extra_px(11, 120.0), 849);
        assert!(base.y + 849 <= 1200);
    }

    /// 查询区内容底边: base_h − band (= 116 + 42 = 158 处的白框下沿)。
    #[test]
    fn query_bottom_matches_frame() {
        let base = window_rect(120.0, 120.0, 1920, 1200, &default_skin());
        // 白框下沿 = base_h − inset = 158; 查询区内容高 = 158 − 42 = 116 ✓
        assert_eq!(query_bottom_px(base.h, 120.0), 158);
        assert_eq!(query_bottom_px(base.h, 120.0) - band_inset_px(120.0), 116);
    }

    /// 可见行数按屏幕收敛: 常规 1200 高屏 → 11 行 (行高 58px); 极矮屏 → 至少 1 行。
    #[test]
    fn max_rows_clamps_to_screen() {
        let base = window_rect(120.0, 120.0, 1920, 1200, &default_skin());
        assert_eq!(max_list_rows(120.0, 1200, base.y, base.h), 11);
        assert_eq!(max_list_rows(120.0, 420, base.y, base.h), 1);
    }

    /// 逐像素合成几何 @125%: 白边 **3.75px** (原版实测「3 满 + 1 弱(248)」正是 3.75 的
    /// AA 表现; 取整成 4 会多一列实色), 圆角 12.5px, 阴影 σ≈3.0px, 下移 2px。
    /// 100% 下退回 DIP 原值 ⇒ 皮肤语义与 DPI 解耦。
    #[test]
    fn per_pixel_style_metrics() {
        let s = crate::skin::DEFAULT;
        assert!((ring_width_px(&s, 120.0) - 3.75).abs() < 1e-9);
        assert!((ring_width_px(&s, 96.0) - 3.0).abs() < 1e-9);
        assert!((corner_radius_px(&s, 120.0) - 12.5).abs() < 1e-9);
        assert!((corner_radius_px(&s, 96.0) - 10.0).abs() < 1e-9);
        // σ = windowShadowSize(3.0 DIP)×dpi/96 × SHADOW_SIGMA_GAIN(0.80) = 3.0px @125%
        // (原版框外剖面 A/B 拟合值; 见 config::SHADOW_SIGMA_GAIN)
        assert!((shadow_sigma_px(&s, 120.0) - 3.0).abs() < 1e-9);
        assert!((shadow_dy_px(120.0) - 2.0).abs() < 1e-9);
    }

    /// 白框形状 = 窗口四周缩 band 的圆角矩形 (与 `content_rect_client` 同源 ⇒
    /// 「框体可见范围」在两处必须一致); 列表展开只改底边, 圆角/顶边不动。
    #[test]
    fn frame_shape_matches_content_rect() {
        let s = crate::skin::DEFAULT;
        let g = window_rect(120.0, 120.0, 1920, 1200, &s);
        let sh = frame_shape(&s, 120.0, g.w, g.h);
        let (cx, cy, cw, ch) = content_rect_client(g, 120.0);
        assert_eq!((sh.l as i32, sh.t as i32), (cx, cy));
        assert_eq!(
            (sh.r as i32 - sh.l as i32, sh.b as i32 - sh.t as i32),
            (cw, ch)
        );
        let sh2 = frame_shape(&s, 120.0, g.w, g.h + list_extra_px(11, 120.0));
        assert_eq!(sh2.t, sh.t, "展开只向下长高, 顶边不动");
        assert_eq!(sh2.b - sh.b, 649.0, "底边 = +11 行附加高");
        assert!((sh2.radius - 12.5).abs() < 1e-9, "圆角不随高度变");
    }
}
