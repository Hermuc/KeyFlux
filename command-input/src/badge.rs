//! 搜索徽标 (badge) —— 查询区右侧的固定图标 (2026-10-04, 纯逻辑零 Win32)。
//!
//! ## 解耦口径 (用户需求「图标由插件提供, 与命令框解耦」)
//!
//! 命令框**不知道任何插件**: 它只知道「徽标」这一通用概念 —— 一个编号的矢量字形
//! (glyph), 由插件经 0x40A/0x40B 指定显示/隐藏。字形注册表在这里 (本仓内建的少量
//! 矢量字形), 插件只传编号 ⇒ 插件可独立接入/移除, 命令框零改动; 未来迁移 (DComp
//! 后端) 时字形绘制跟着 compose 走, 协议不变。
//!
//! ## 为什么用解析式 SDF 而不是 GDI 画
//!
//! GDI 的椭圆/直线**无抗锯齿**, 与既有逐像素 AA 观感 (白边内沿 248 弱像素、圆角
//! dx 11/4/2/1/0) 不一致; `compose` 已是逐像素覆盖率管线, 把字形做成 SDF (有符号
//! 距离场) 即可拿到与几何完全同源的 AA。放大镜 = 圆环 + 45° 手柄 (圆帽), 两个
//! 基本形的 min —— 全部解析式, 单测可锁定覆盖率。

/// 字形编号: 放大镜 (搜索徽标)。
pub const GLYPH_MAGNIFIER: u32 = 1;

/// 字形编号是否已注册 (未注册 = 插件端错误, 命令框忽略该消息 —— 对端错误不得带崩框)。
pub fn is_known(glyph: u32) -> bool {
    glyph == GLYPH_MAGNIFIER
}

/// 一个已放置字形的绘制参数 (像素坐标; 由 `geometry::badge_rect_px` 布局)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BadgePaint {
    /// 圆环中心 / 半径
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    /// 手柄线段 (圆帽)
    pub hx1: f64,
    pub hy1: f64,
    pub hx2: f64,
    pub hy2: f64,
    /// 描边半宽 (像素)
    pub half_w: f64,
    /// 包围盒 (整数像素范围, 外扩 1px 供 AA; 逐像素循环用它快速剔除)
    pub bx0: i32,
    pub by0: i32,
    pub bx1: i32,
    pub by1: i32,
}

impl BadgePaint {
    /// 在字形盒 (左上角 `l,t`, 边长 `size`) 内放一个放大镜; `stroke` = 描边宽 (像素)。
    ///
    /// 布局 (单位 = 盒边长比例, 对照参照图): 圆环略偏左上 (圆心 0.40, 半径 0.27),
    /// 手柄从圆环 45° 边缘延伸到 (0.78, 0.78) —— 即参照图中的放大镜姿态。
    pub fn magnifier(l: f64, t: f64, size: f64, stroke: f64) -> Self {
        let hw = (stroke * 0.5).max(0.25);
        let cx = l + 0.40 * size;
        let cy = t + 0.40 * size;
        let r = 0.27 * size;
        let k = std::f64::consts::FRAC_1_SQRT_2;
        let sx = cx + r * k;
        let sy = cy + r * k;
        let hx2 = l + 0.78 * size;
        let hy2 = t + 0.78 * size;
        let pad = hw + 1.0;
        Self {
            cx,
            cy,
            r,
            hx1: sx,
            hy1: sy,
            hx2,
            hy2,
            half_w: hw,
            bx0: (cx - r - pad).floor() as i32,
            by0: (cy - r - pad).floor() as i32,
            bx1: (hx2 + pad).ceil() as i32,
            by1: (hy2 + pad).ceil() as i32,
        }
    }

    /// 线段距离 (点到线段的欧氏距离)。
    fn seg_dist(&self, x: f64, y: f64) -> f64 {
        let vx = self.hx2 - self.hx1;
        let vy = self.hy2 - self.hy1;
        let wx = x - self.hx1;
        let wy = y - self.hy1;
        let vv = vx * vx + vy * vy;
        let t = if vv <= 0.0 {
            0.0
        } else {
            ((wx * vx + wy * vy) / vv).clamp(0.0, 1.0)
        };
        let dx = wx - t * vx;
        let dy = wy - t * vy;
        (dx * dx + dy * dy).sqrt()
    }

    /// 有符号距离 (像素; 负 = 描边内): 圆环与手柄两个胶囊形的 min。
    pub fn sdf(&self, x: f64, y: f64) -> f64 {
        let dr = ((x - self.cx).powi(2) + (y - self.cy).powi(2)).sqrt();
        let ring = (dr - self.r).abs() - self.half_w;
        let cap = self.seg_dist(x, y) - self.half_w;
        ring.min(cap)
    }

    /// 覆盖率 (0..1; 1px 线性 AA, 与 `compose::Shape::coverage` 同口径)。
    pub fn coverage(&self, x: f64, y: f64) -> f64 {
        (0.5 - self.sdf(x, y)).clamp(0.0, 1.0)
    }

    /// 该像素是否在包围盒内 (整数像素坐标; 供逐像素循环快速剔除)。
    pub fn hits(&self, x: i32, y: i32) -> bool {
        x >= self.bx0 && x < self.bx1 && y >= self.by0 && y < self.by1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 25px 盒 (20 DIP @125%), 2px 描边 —— 与生产布局同参数。
    fn paint() -> BadgePaint {
        BadgePaint::magnifier(100.0, 50.0, 25.0, 2.0)
    }

    #[test]
    fn ring_stroke_is_inside_outside_is_not() {
        let p = paint();
        // 圆环正上点的描边中心 = (cx, cy − r) ⇒ SDF ≈ 0 (描边内)
        assert!(p.sdf(p.cx, p.cy - p.r) <= 0.0, "环带中心必在形内");
        // 圆心 = 环洞中心, 距描边 r − half_w ⇒ 形外
        assert!(p.sdf(p.cx, p.cy) > 0.0, "圆心是洞");
        // 盒外远点
        assert!(p.sdf(0.0, 0.0) > 0.0);
    }

    #[test]
    fn handle_endpoint_is_covered() {
        let p = paint();
        assert!(
            (p.coverage(p.hx2, p.hy2) - 1.0).abs() < 1e-9,
            "手柄末端 (圆帽中心) 必全覆盖"
        );
        assert!(p.coverage(p.hx1, p.hy1) > 0.99, "手柄起点 (环缘) 有覆盖");
    }

    #[test]
    fn coverage_is_antialiased() {
        let p = paint();
        // 描边边界外 0.25px 处: SDF = +0.25 ⇒ 覆盖率 0.25 (1px 线性 AA 过渡带)
        let edge = p.cx + p.r + p.half_w;
        let c = p.coverage(edge + 0.25, p.cy);
        assert!(
            (c - 0.25).abs() < 1e-9,
            "边界外 0.25px 覆盖率应 = 0.25, 实际 {c}"
        );
        // 描边内 0.25px 处: SDF = −0.25 ⇒ 覆盖率 0.75
        let c2 = p.coverage(edge - 0.25, p.cy);
        assert!(
            (c2 - 0.75).abs() < 1e-9,
            "边界内 0.25px 覆盖率应 = 0.75, 实际 {c2}"
        );
    }

    #[test]
    fn bbox_covers_glyph_and_hits_works() {
        let p = paint();
        // 环顶点 / 手柄末端都应在包围盒内
        assert!(p.hits(p.cx.floor() as i32, (p.cy - p.r).floor() as i32));
        assert!(p.hits(p.hx2.floor() as i32, p.hy2.floor() as i32));
        // 盒外不命中
        assert!(!p.hits(0, 0));
    }

    #[test]
    fn glyph_registry_locks_magnifier_id() {
        assert!(is_known(GLYPH_MAGNIFIER));
        assert_eq!(GLYPH_MAGNIFIER, 1);
        assert!(!is_known(0));
        assert!(!is_known(2));
    }
}
