//! 逐像素合成 (per-pixel alpha) —— 还原原版观感的合成层 (2026-10-04, 纯逻辑零 Win32)。
//!
//! ## 为什么需要这一层
//!
//! v1 用 `SetLayeredWindowAttributes(LWA_ALPHA)` 做**整窗**不透明度: 全窗一个 alpha
//! ⇒ **白边与内部不可能有不同不透明度** ⇒ 原版那种「3px 纯白描边 + 半透明内部」表达
//! 不出来(白边被同 alpha 拉平成与内部同色 ⇒ 视觉上就是没有边框)。
//! 原版 (= 上游 exe) 的机制是 **DirectComposition 自合成** (spec.md:259「原链 = D3D11 →
//! D2D1 → DComp → DirectWrite + D2D1Shadow」, ex-style 0x08200008 的
//! `WS_EX_NOREDIRECTIONBITMAP` 即自合成绑定位) ⇒ 每个视觉各自带 opacity。
//! 本模块是它的**等价物**: 由本进程算出逐像素 alpha 掩码与预乘 RGB, 交
//! `UpdateLayeredWindow` 呈现 (GDI crate 内完成, 不引入 D3D/D2D —— design-A §4.4
//! 早已写明这条升级路径: 「换 UpdateLayeredWindow 逐像素 alpha」)。
//!
//! ## 层模型 (与原版三个视觉一一对应)
//!
//! ```text
//!   [阴影 = 黑色圆角矩形轮廓的高斯模糊, 只画在框外]        ← D2D1Shadow
//!   [白边 = 框体最外 borderWidth 像素的**不透明**色带]      ← 不透明底板 (borderOpacity=1.0)
//!   [内部 = 内容 (背景/网格/文字/结果行) @ fillAlpha]       ← 半透明内容视觉
//! ```
//!
//! 合成顺序 = 阴影 → 内容/白边 (同一层, 互不重叠) ⇒
//! `out = frame over shadow`, 即 `a = a_f + a_s·(1−a_f)`、RGB 相应预乘。
//!
//! ⚠ 内部视觉是**半透明**的, 由 `UpdateLayeredWindow` 与**真实桌面**混合 (逐像素 alpha)。
//! 因此这里**不**采样/模糊背景 —— 原版 (DComp 自合成) 同样不采样背景 (spec.md:259 的
//! 视觉链里没有背景采样环节, 实测填充亮度 239 与「纯填充」模型也一致)。
//! 若将来要真·毛玻璃 (采样 + 模糊背景), 正解是 design-C 规划的 **DComp 后端 + 系统
//! Acrylic/Mica**, 而不是在 ULW 路径上叠 `SetWindowCompositionAttribute`(两者语义互斥:
//! 隔离探针实测只多一层噪声纹理, 背景细节完全没被模糊)。
//!
//! ## 实测标定 (原版 vs 新版**同背景活体 A/B**, 详见 `%TEMP%\kf_list_smoke\ab_style.py`)
//!
//! 受控背景 = 纯白 255 / 浅底深字 / 纯黑 0 三条带, 由 `Lw = α·F + (1−α)·255` 与
//! `Lb = α·F` **联立**解出净不透明度 α 与内容色 F (两个未知量两个方程, 不依赖插值):
//!
//! | 观测 | 原版 (非 Rust) | 新版 (改前) | 新版 (改后) |
//! |---|---|---|---|
//! | 净有效 alpha | **0.945** | 0.812 | 0.945 |
//! | 内容色 F | 242.9 | 254.9 | 242.9 |
//! | 透过率 (独立测法: 文字调制度比) | 0.072 | 0.202 | 0.055 |
//! | 白边 | **3px 纯白** | 3px 纯白 | 3px 纯白 |
//! | 框外阴影 (距框 1..10px 压暗) | 50,45,33,24,16,10,6,3,1,0 | 55,48,41,34,28,22,17,12,9,6 | ≈ 原版 |
//!
//! ⇒ 还原口径 (三者合起来的净观感精确等价于原版的 `out = bg色×b + (1−b)×0.55×bg`):
//!   - 白边 = `borderOpacity`(=1.0) 全不透明;
//!   - 填充 = `skin::fill_alpha` = `1 − (1 − backgroundOpacity)×0.55` (**0.945**);
//!   - 面板色 = `skin::panel_content_color` = 皮肤色×`backgroundOpacity`/α (**242.9**);
//!   - 阴影 = 高斯轮廓 (σ≈3.0px, 峰值 0.30)。
//!
//! ⚠ 上一轮 (2026-10-04 白天) 标出的「原版 0.784 / 填充 = 皮肤值²」**已作废**: 那套依据是
//! 「用框上/框下两条桌面带做垂直插值」, 假设背景沿垂直方向平滑 —— 在文字密集的背景上失效
//! (对纯白底实测 243, 按 0.784 预测应 248, 差 5 级)。

use crate::skin::Rgb;

/// 圆角矩形 (f64 像素坐标; 左闭右开约定与 GDI 一致: `r`/`b` 为**边界**, 不是末像素)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub l: f64,
    pub t: f64,
    pub r: f64,
    pub b: f64,
    /// 圆角半径 (像素; 会被 clamp 到半宽/半高)
    pub radius: f64,
}

impl Shape {
    /// 有符号距离 (像素; 负数 = 形内)。构造 = 「圆角矩形的标准 SDF」:
    /// 先算到「内矩形 (外矩形各边缩 radius)」的距离, 再减 radius。
    pub fn sdf(&self, x: f64, y: f64) -> f64 {
        let hw = (self.r - self.l) * 0.5;
        let hh = (self.b - self.t) * 0.5;
        let cx = self.l + hw;
        let cy = self.t + hh;
        let rr = self.radius.min(hw).min(hh).max(0.0);
        let qx = (x - cx).abs() - (hw - rr);
        let qy = (y - cy).abs() - (hh - rr);
        let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        outside + qx.max(qy).min(0.0) - rr
    }

    /// 覆盖率 (像素中心采样 + 1px 线性 AA): 形内 1, 形外 0, 边界 0.5。
    pub fn coverage(&self, x: f64, y: f64) -> f64 {
        (0.5 - self.sdf(x, y)).clamp(0.0, 1.0)
    }
}

/// 高斯阴影参数 (原版 = D2D1Shadow)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Rgb,
    /// 峰值不透明度 (0..1; 边缘处 = 峰值×0.5, 因为轮廓被模糊掉一半)
    pub opacity: f64,
    /// 高斯 σ (像素)
    pub sigma: f64,
    /// 垂直偏移 (像素; 正 = 向下)
    pub dy: f64,
}

impl Shadow {
    /// 阴影覆盖率 (0..1): 轮廓的高斯模糊 = 半空间的高斯 CDF ⇒ `0.5·erfc(d/(σ√2))`,
    /// `d` = 该像素到**框体轮廓**的有符号距离 (负 = 形内); 轮廓整体下移 `dy`。
    pub fn coverage(&self, d_frame: f64) -> f64 {
        let d = d_frame - self.dy;
        0.5 * erfc(d / (self.sigma.max(0.001) * std::f64::consts::SQRT_2))
    }
}

fn erfc(x: f64) -> f64 {
    // Abramowitz & Stegun 7.1.26, |误差| ≤ 1.5e-7 —— 足够做 8bit 合成
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let ans = t
        * (-z * z - 1.265_512_23
            + t * (1.000_023_68
                + t * (0.374_091_96
                    + t * (0.096_784_18
                        + t * (-0.186_288_06
                            + t * (0.278_868_07
                                + t * (-1.135_203_98
                                    + t * (1.488_515_87
                                        + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
            .exp();
    if x >= 0.0 {
        ans
    } else {
        2.0 - ans
    }
}

/// 搜索徽标绘制层 (几何来自 `crate::badge`, 颜色由皮肤派生)。
#[derive(Clone, Debug)]
pub struct BadgeLayer {
    pub geom: crate::badge::BadgePaint,
    pub rgb: Rgb,
}

/// 一帧的合成计划 (只依赖几何/皮肤 ⇒ 尺寸变化时重建一次, 逐帧复用)。
#[derive(Clone, Debug)]
pub struct Plan {
    pub w: i32,
    pub h: i32,
    /// 白框 (可见圆角矩形)
    pub frame: Shape,
    /// 白边宽度 (像素; 允许小数 —— 原版 3 DIP @125% = 3.75px, 实测 3 满 + 1 弱)
    pub ring_px: f64,
    pub ring_rgb: Rgb,
    /// 白边不透明度 (0..1; 皮肤 borderOpacity)
    pub ring_alpha: f64,
    /// 内部填充净不透明度 (0..1; `skin::fill_alpha` = 1−(1−b)×0.55 = **0.945**, 同背景 A/B 实测)
    pub fill_alpha: f64,
    pub shadow: Shadow,
    /// 搜索徽标 (2026-10-04; None = 不绘制)。绘制在**内容之上**、填充 alpha 之内
    /// (徽标是查询区内部元素, 不改变面板净透明度)。
    pub badge: Option<BadgeLayer>,
}

impl Plan {
    /// 单像素合成结果: `(alpha 0..1, 预乘 RGB 0..255)`。
    /// `drawn_rgb` = GDI 画出的内容色 (背景/网格/文字/图标/结果行), 语义 = **[R,G,B]**
    /// (调用方负责从 DIB 的 B,G,R 内存序转序, 见 [`composite`]; 越界时传任意值)。
    /// 返回的预乘 RGB 同为 [R,G,B] 序。
    pub fn pixel(&self, x: i32, y: i32, drawn_rgb: [f64; 3]) -> (f64, [f64; 3]) {
        let fx = x as f64 + 0.5;
        let fy = y as f64 + 0.5;
        let d = self.frame.sdf(fx, fy);
        let cov = (0.5 - d).clamp(0.0, 1.0);
        // 白边相对覆盖率: 1 = 环带核心, 0 = 内部 (内沿 1px 过渡 = 原版实测的 248 弱像素)
        //   环带 = 框边向内 ring_px 的那一圈 ⇒ d > −ring_px 且 d < 0 的区域。
        let ring_cov = ((0.5 + d + self.ring_px).clamp(0.0, 1.0)) * cov;
        // 框体自身不透明度 (白边 / 内容 同层不重叠)
        let a_frame = cov * (ring_cov * self.ring_alpha + (1.0 - ring_cov) * self.fill_alpha);
        let fr = mix(drawn_rgb[0], self.ring_rgb.0 as f64, ring_cov);
        let fg = mix(drawn_rgb[1], self.ring_rgb.1 as f64, ring_cov);
        let fb = mix(drawn_rgb[2], self.ring_rgb.2 as f64, ring_cov);
        // 徽标: 画在内容之上 (先内容后徽标的 over 叠色), 不改 alpha —— 徽标是查询区
        // 内部的描边元素, 净观感仍 = 填充 (α 不变); 包围盒外的像素零成本跳过。
        let (mut fr, mut fg, mut fb) = (fr, fg, fb);
        if let Some(b) = &self.badge {
            if b.geom.hits(x, y) {
                let cov = b.geom.coverage(fx, fy);
                if cov > 0.0 {
                    fr = mix(fr, b.rgb.0 as f64, cov);
                    fg = mix(fg, b.rgb.1 as f64, cov);
                    fb = mix(fb, b.rgb.2 as f64, cov);
                }
            }
        }
        // 阴影: 只落在框外 (原版阴影视觉在**不透明底板**之下 ⇒ 框内不外溢;
        //   本条同时保证内部观感 = 纯填充, 与实测 239 (fill-only 模型 240) 一致)
        let a_sh = self.shadow.coverage(d) * (1.0 - cov) * self.shadow.opacity;
        let a_out = a_frame + a_sh * (1.0 - a_frame);
        let sr = self.shadow.color.0 as f64;
        let sg = self.shadow.color.1 as f64;
        let sb = self.shadow.color.2 as f64;
        (
            a_out,
            [
                fr * a_frame + sr * a_sh * (1.0 - a_frame),
                fg * a_frame + sg * a_sh * (1.0 - a_frame),
                fb * a_frame + sb * a_sh * (1.0 - a_frame),
            ],
        )
    }

    /// 阴影在框外需要占用的宽度 (像素): 窗口 region 必须覆盖到它, 否则阴影被裁掉。
    pub fn shadow_extent_px(&self) -> i32 {
        (self.shadow.sigma * crate::config::SHADOW_REGION_SIGMA + self.shadow.dy).ceil() as i32 + 1
    }

    /// **内部快速区** (左闭右开像素矩形): 区内每个像素中心必然满足
    /// `d ≤ −(ring_px + 0.5)` ⇒ 在 [`Plan::pixel`] 里 `cov = 1`、`ring_cov = 0`、
    /// 阴影项乘 0 —— 输出精确退化为「内容色 × fill_alpha」的常数乘, 无需逐像素
    /// SDF (sqrt) 与阴影 erfc (exp)。几何依据: 点落在圆角矩形**各边内缩
    /// (radius + ring_px + 0.5)** 的矩形内时, 到四边的距离 ≥ 内缩量, 且圆角弧只
    /// 存在于四个角方 (边长 = 圆角半径, 已被内缩量整个避开) —— `radius` 取未钳制
    /// 原值只会更保守 (钳制只会让实际弧更小)。徽标包围盒 (若有) 从快速区**剔除**:
    /// 徽标覆盖须逐像素算, 不能走常数乘。
    ///
    /// 返回 `None` = 无有效快速区 (框体太小)。返回值仅供 [`composite`] 使用 ——
    /// 快速路径与 [`Plan::pixel`] **逐位一致** (乘法结合序对齐, 见 composite 注),
    /// 由 `composite_fast_path_matches_pixel` 测试锁定。
    pub fn fast_zone(&self) -> Option<(i32, i32, i32, i32)> {
        let m = self.frame.radius + self.ring_px + 0.5;
        // 像素中心 = 整数坐标 + 0.5: x+0.5 ≥ l+m ⇒ x ≥ l+m−0.5 (ceil 收窄);
        // x+0.5 ≤ r−m ⇒ x ≤ r−m−0.5 (floor 收窄)。左右闭开: [x0, x1)。
        let x0 = (self.frame.l + m - 0.5).ceil() as i32;
        let y0 = (self.frame.t + m - 0.5).ceil() as i32;
        let x1 = (self.frame.r - m - 0.5).floor() as i32 + 1;
        let y1 = (self.frame.b - m - 0.5).floor() as i32 + 1;
        // 剔除徽标包围盒 (徽标是内部元素, 包围盒通常整体含于快速区; 含于才需要挖):
        // 挖洞会把矩形撕成四块 ⇒ 交由调用方逐像素排除更简单 —— 这里返回「含徽标区」
        // 的矩形, composite 里对徽标包围盒内的像素回落慢路径。
        if x1 > x0 && y1 > y0 {
            Some((x0, y0, x1, y1))
        } else {
            None
        }
    }
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn to_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// 就地合成: 读 GDI 画好的内容色, 写入**预乘**色 (`UpdateLayeredWindow` + `AC_SRC_ALPHA` 的要求)。
/// `gain` = 整体不透明度增益 (0..1; 淡出动效用), `dib` 必须与 `plan.w×plan.h` 同尺寸。
/// 通道口径 (🔴 唯一真源, 两端都必须遵守):
///   * DIB 内存序 = **B,G,R,A** (32bpp `BI_RGB` 的小端像素布局, GDI/`DrawIconEx` 同此);
///   * [`Plan::pixel`] 的 `drawn_rgb` 语义 = **[R,G,B]** (与其 `Rgb(R,G,B)` 字段序一致),
///     返回的预乘 RGB 同为 [R,G,B]。
///   ⇒ 读入时首尾交换一次, 写回时 `pm[0]→R 槽 / pm[2]→B 槽`。历史上两端各错半边
///     (读入当 [R,G,B] + 写回按 B 槽←pm[2]), 抵消成「非灰内容色 R/B 互换」—— 文件
///     图标变互补色、选中行/强调条变暖色 (2026-10-05 用户报障修复)。
///
/// ## 性能: 内部快速路径 (2026-10-06, 悬停高亮卡顿修复的一半)
///
/// 全帧逐像素 `Plan::pixel` 含 SDF (sqrt) 与阴影 erfc (exp) —— 每帧 ~百万像素两次
/// 超越函数, 是悬停重绘管线的大头。而框体内部 (`fast_zone`) 的合成结果数学上恒等于
/// 「内容色 × fill_alpha」: `cov=1`、`ring_cov=0`、阴影项乘 `(1−cov)=0` 归零。
/// 快速路径的 f64 运算**顺序与 `Plan::pixel` 完全对齐** (`(drawn×a)×g` 与
/// `pm[0]×g` 同构; alpha `fill_alpha + 0.0` 恒等) ⇒ **逐位一致**, 由测试
/// `composite_fast_path_matches_pixel` 全帧锁定。快速区外 (边缘带 / 圆角 / 徽标
/// 包围盒) 仍走 [`Plan::pixel`] —— 那部分只占周长一圈, 面积占比可忽略。
pub fn composite(dib: &mut [u8], plan: &Plan, gain: f64) {
    let g = gain.clamp(0.0, 1.0);
    let fast = plan.fast_zone();
    // 徽标包围盒 (整数, 左闭右开): 区内像素回落慢路径 (覆盖须逐像素算)。
    let badge_box = plan
        .badge
        .as_ref()
        .map(|b| (b.geom.bx0, b.geom.by0, b.geom.bx1, b.geom.by1));
    let fill_alpha = plan.fill_alpha;
    for y in 0..plan.h {
        for x in 0..plan.w {
            let i = ((y * plan.w + x) * 4) as usize;
            if i + 3 >= dib.len() {
                return;
            }
            // DIB 内存序 B,G,R → pixel 语义 [R,G,B]
            let drawn = [dib[i + 2] as f64, dib[i + 1] as f64, dib[i] as f64];
            let in_fast = match fast {
                Some((x0, y0, x1, y1)) => {
                    x >= x0
                        && x < x1
                        && y >= y0
                        && y < y1
                        && !badge_box.is_some_and(|(bx0, by0, bx1, by1)| {
                            x >= bx0 && x < bx1 && y >= by0 && y < by1
                        })
                }
                None => false,
            };
            if in_fast {
                // 快速路径: 与 Plan::pixel 逐位一致 (见模块级「性能」注) ——
                // a_out = fill_alpha, pm = drawn × fill_alpha (乘法结合序对齐)。
                dib[i] = to_u8(drawn[2] * fill_alpha * g); // B
                dib[i + 1] = to_u8(drawn[1] * fill_alpha * g); // G
                dib[i + 2] = to_u8(drawn[0] * fill_alpha * g); // R
                dib[i + 3] = to_u8(fill_alpha * 255.0 * g); // A
            } else {
                let (a, pm) = plan.pixel(x, y, drawn);
                // pm 已是**预乘**值 (含各自的 alpha) ⇒ 这里只再叠整体增益 g, 不能再乘一次 a。
                // 写回按 DIB 内存序: B 槽 ← pm[2](B), G 槽 ← pm[1], R 槽 ← pm[0](R)。
                dib[i] = to_u8(pm[2] * g); // B
                dib[i + 1] = to_u8(pm[1] * g); // G
                dib[i + 2] = to_u8(pm[0] * g); // R
                dib[i + 3] = to_u8(a * 255.0 * g); // A
            }
        }
    }
}

/// 淡出整帧缩放 (源 = 已预乘的帧; 预乘面上整体缩放等价于逐通道乘法)。
pub fn scale_frame(src: &[u8], dst: &mut [u8], gain: f64) {
    let g = gain.clamp(0.0, 1.0);
    let n = src.len().min(dst.len());
    for i in 0..n {
        dst[i] = to_u8(src[i] as f64 * g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        Plan {
            w: 100,
            h: 60,
            frame: Shape {
                l: 10.0,
                t: 10.0,
                r: 90.0,
                b: 50.0,
                radius: 8.0,
            },
            ring_px: 3.75,
            ring_rgb: Rgb(255, 255, 255),
            ring_alpha: 1.0,
            fill_alpha: 0.945,
            shadow: Shadow {
                color: Rgb(0, 0, 0),
                opacity: 0.0,
                sigma: 5.0,
                dy: 2.0,
            },
            badge: None,
        }
    }

    #[test]
    fn sdf_sign_and_radius() {
        let s = Shape {
            l: 0.0,
            t: 0.0,
            r: 100.0,
            b: 50.0,
            radius: 10.0,
        };
        assert!(s.sdf(50.0, 25.0) < 0.0, "中心必在形内");
        assert!(s.sdf(-5.0, 25.0) > 0.0, "形外必为正");
        assert!(s.sdf(0.0, 25.0).abs() < 1e-9, "边界 (x = l) 处 SDF = 0");
        assert!(
            (s.sdf(-0.5, 25.0) - 0.5).abs() < 1e-9,
            "边界外半像素 = +0.5"
        );
        // 圆角: 左上角 (0.5,0.5) 距边界更远 ⇒ SDF 比直边同 x 处大
        assert!(s.sdf(0.5, 0.5) > s.sdf(0.5, 25.0));
    }

    #[test]
    fn coverage_is_antialiased_at_edge() {
        let s = Shape {
            l: 0.0,
            t: 0.0,
            r: 100.0,
            b: 50.0,
            radius: 0.0,
        };
        assert!((s.coverage(50.0, 25.0) - 1.0).abs() < 1e-9);
        assert!((s.coverage(-5.0, 25.0)).abs() < 1e-9);
        // 边界像素中心恰落在边上 (x = 0) ⇒ 覆盖率 0.5
        assert!((s.coverage(0.0, 25.0) - 0.5).abs() < 1e-9);
        // 边界内半像素 ⇒ 全覆 (像素中心 0.5 在形内)
        assert!((s.coverage(0.5, 25.0) - 1.0).abs() < 1e-9);
    }

    /// 核心口径: **白边不透明 (255) 而内部半透明** —— v1 的整窗 alpha 表达不出来的东西。
    #[test]
    fn ring_is_opaque_fill_is_translucent() {
        let p = plan();
        let border = p.pixel(10 + 1, 30, [255.0; 3]).0 * 255.0;
        let inner = p.pixel(50, 30, [255.0; 3]).0 * 255.0;
        assert!(
            (border - 255.0).abs() < 1.5,
            "白边 alpha 应 ≈255, 实际 {border}"
        );
        assert!(
            (inner - 241.0).abs() < 1.5,
            "内部 alpha 应 ≈241 (0.945 = 1−0.1×0.55), 实际 {inner}"
        );
        assert!(
            border > inner + 10.0,
            "白边必须比内部实 (0.945 口径下差 ≈14 级)"
        );
    }

    /// 环带宽度: `ring_px` 内为不透明, 越过后落到填充值; 内沿有 1px 过渡。
    #[test]
    fn ring_width_follows_skin() {
        let p = plan();
        let alphas: Vec<f64> = (0..10)
            .map(|k| p.pixel(10 + k, 30, [255.0; 3]).0 * 255.0)
            .collect();
        assert!((alphas[0] - 255.0).abs() < 1.5, "第 0 px 在白边内");
        assert!(
            (alphas[3] - 255.0).abs() < 20.0,
            "第 3 px 仍在白边内 ({})",
            alphas[3]
        );
        assert!(
            (alphas[6] - 241.0).abs() < 1.5,
            "第 6 px 已在内部 ({})",
            alphas[6]
        );
        assert!(alphas[0] > alphas[5], "白边 → 内部必须单调下降");
    }

    /// 阴影: 只在框外出现 (框内 alpha 不含阴影 ⇒ 内部观感 = 纯填充), 且随距离衰减。
    #[test]
    fn shadow_only_outside_and_fades() {
        let mut p = plan();
        p.shadow.opacity = 0.5;
        p.shadow.sigma = 4.0;
        let outside: Vec<f64> = (1..9)
            .map(|k| p.pixel(10 - k, 30, [255.0; 3]).0 * 255.0)
            .collect();
        assert!(outside[0] > 0.0, "紧贴框外必须有阴影 alpha");
        assert!(
            outside.iter().all(|v| *v < 200.0),
            "阴影最浓处也应远小于白边"
        );
        assert!(outside[0] > outside[7], "阴影随距离衰减: {outside:?}");
        // 框内不受阴影影响 (与 fill-only 模型一致)
        assert!((p.pixel(50, 30, [255.0; 3]).0 * 255.0 - 241.0).abs() < 1.5);
    }

    /// 徽标: 描边处内容色被徽标色覆盖、alpha 不变; 包围盒外逐字节不变。
    #[test]
    fn badge_blends_color_keeps_alpha() {
        let mut p = plan();
        let geom = crate::badge::BadgePaint::magnifier(30.0, 12.0, 25.0, 2.0);
        p.badge = Some(BadgeLayer {
            geom,
            rgb: Rgb(40, 40, 40),
        });
        // 手柄末端 (圆帽中心) 必在形内
        let x = geom.hx2.floor() as i32;
        let y = geom.hy2.floor() as i32;
        let (a_badge, pm) = p.pixel(x, y, [243.0; 3]);
        assert!(
            pm[0] < 200.0,
            "描边处内容色 (243) 必须被徽标色 (40) 拉深, 实际 {}",
            pm[0]
        );
        // alpha 与无徽标时相同 (徽标不改面板净透明度)
        let mut p0 = plan();
        p0.badge = None;
        let (a_plain, _) = p0.pixel(x, y, [243.0; 3]);
        assert!((a_badge - a_plain).abs() < 1e-9);
        // 包围盒外 (但仍在框内) 的像素与无徽标计划逐位相同
        let with = p.pixel(70, 30, [243.0; 3]);
        let without = p0.pixel(70, 30, [243.0; 3]);
        assert_eq!(with.0, without.0);
        assert_eq!(with.1, without.1);
    }

    /// 圆角: 框外 1px 的角落必须透明 (AA 不会让角变方)。
    #[test]
    fn corners_are_cut() {
        let p = plan();
        assert_eq!(p.pixel(11, 11, [255.0; 3]).0, 0.0, "直角处应在形外 (圆角)");
        assert!(p.pixel(13, 13, [255.0; 3]).0 > 0.0);
    }

    /// 预乘: 内部白填充 net α=0.945 ⇒ RGB≈A≈241; 框外全 0; 整体增益线性缩放。
    #[test]
    fn composite_premultiplies_and_scales() {
        let p = plan();
        let n = (p.w * p.h * 4) as usize;
        let mut dib = vec![255u8; n];
        composite(&mut dib, &p, 1.0);
        let inner = ((30 * p.w + 50) * 4) as usize;
        assert!((dib[inner + 3] as i32 - 241).abs() <= 1);
        assert!((dib[inner] as i32 - 241).abs() <= 1, "预乘后 B ≈ A");
        let ring = ((30 * p.w + 11) * 4) as usize;
        assert_eq!(dib[ring + 3], 255);
        let far = ((2 * p.w + 2) * 4) as usize;
        assert_eq!(dib[far + 3], 0);
        assert_eq!(dib[far], 0);

        // 淡出: gain=0.5 ⇒ 全通道减半
        let mut faded = dib.clone();
        composite(&mut faded, &p, 0.5);
        assert!((faded[inner + 3] as i32 - 120).abs() <= 2);
        assert_eq!(faded[ring + 3], 128);
    }

    /// 阴影色必须真的参与 RGB 预乘 (黑阴影在框外 ⇒ RGB = 0, 只留 alpha)。
    #[test]
    fn shadow_is_black_in_band() {
        let mut p = plan();
        p.shadow.opacity = 0.5;
        let n = (p.w * p.h * 4) as usize;
        let mut dib = vec![255u8; n];
        composite(&mut dib, &p, 1.0);
        let band = ((30 * p.w + 6) * 4) as usize;
        assert!(dib[band + 3] > 0, "带内应有阴影 alpha");
        assert!(dib[band] < 4, "阴影 RGB 必须是黑 (预乘后 B={})", dib[band]);
    }

    /// 回归 (2026-10-05 用户报障「文件图标颜色和实际不同」): 非**灰**内容色不得被
    /// R/B 互换。旧实现读入把 DIB 的 B,G,R 内存序当 [R,G,B] 喂给 `Plan::pixel`,
    /// 视觉上 = 所有 GDI/DrawIconEx 落盘的彩色像素红蓝互换 (蓝青图标变橙绿)。
    /// 本测试用框内的非灰内容色锁定通道序: R/G/B 的相对大小必须原样保留。
    #[test]
    fn composite_keeps_channel_order() {
        let p = plan();
        let n = (p.w * p.h * 4) as usize;
        let mut dib = vec![255u8; n];
        // 模拟 GDI 落盘的靛蓝内容 (皮肤 gridlineColor #2843AD) —— BGRA 内存序
        for px in dib.chunks_exact_mut(4) {
            px[0] = 0xAD; // B
            px[1] = 0x43; // G
            px[2] = 0x28; // R
            px[3] = 0; // GDI 不写 alpha
        }
        composite(&mut dib, &p, 1.0);
        // 框内深处 (50,30): 无白边/徽标/阴影 ⇒ 各通道只按 fill_alpha 等比预乘
        let inner = ((30 * p.w + 50) * 4) as usize;
        let (b, g, r) = (
            dib[inner] as f64,
            dib[inner + 1] as f64,
            dib[inner + 2] as f64,
        );
        assert!(
            r < g && g < b,
            "R<G<B 的相对大小必须保留 (旧 bug 会变成 B<G<R): R={r} G={g} B={b}"
        );
        // 预乘比: r/0x28 ≈ b/0xAD ≈ fill_alpha (同倍率 ⇒ 无逐通道畸变)
        let rb = r / 0x28 as f64;
        let bb = b / 0xAD as f64;
        assert!((rb - bb).abs() < 0.02, "各通道须同倍率预乘: {rb} vs {bb}");
    }

    /// 快速路径锁定 (2026-10-06 悬停卡顿修复): `composite` 的内部快速路径必须与
    /// 逐像素 `Plan::pixel` **全帧逐位一致** —— 含徽标与不含徽标两种计划都要过。
    /// 这条测试是「快速区几何推导」的唯一护栏: 推导有误 (比如圆角内缩不足) 时,
    /// 边缘/角/徽标带内会出现逐位偏差。
    #[test]
    fn composite_fast_path_matches_pixel() {
        let mut p = plan();
        let n = (p.w * p.h * 4) as usize;
        let mut source = vec![0u8; n];
        // 位置相关伪随机内容 (含非灰彩色), 覆盖内部/边缘/角全部区域
        for (k, px) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            px[0] = (k * 37 % 251) as u8;
            px[1] = (k * 91 % 249) as u8;
            px[2] = (k * 53 % 253) as u8;
        }
        for with_badge in [false, true] {
            p.badge = with_badge.then(|| BadgeLayer {
                geom: crate::badge::BadgePaint::magnifier(30.0, 12.0, 25.0, 2.0),
                rgb: Rgb(40, 40, 40),
            });
            let mut fast = source.clone();
            composite(&mut fast, &p, 1.0);
            // 慢参考: 逐像素 Plan::pixel 直算 (原始内容另存一份, 因为 composite 就地写)
            let mut slow = source.clone();
            for y in 0..p.h {
                for x in 0..p.w {
                    let i = ((y * p.w + x) * 4) as usize;
                    let drawn = [slow[i + 2] as f64, slow[i + 1] as f64, slow[i] as f64];
                    let (a, pm) = p.pixel(x, y, drawn);
                    slow[i] = to_u8(pm[2]);
                    slow[i + 1] = to_u8(pm[1]);
                    slow[i + 2] = to_u8(pm[0]);
                    slow[i + 3] = to_u8(a * 255.0);
                }
            }
            assert_eq!(fast, slow, "with_badge={with_badge}");
        }
    }

    /// 快速区必须严格内缩: 区内任意像素中心到框边的有符号距离满足
    /// `d ≤ −(ring_px + 0.5)` (即 `cov=1` 且 `ring_cov=0` 的充分条件)。
    #[test]
    fn fast_zone_is_conservative() {
        let p = plan();
        let Some((x0, y0, x1, y1)) = p.fast_zone() else {
            panic!("测试计划应存在有效快速区");
        };
        let m = p.frame.radius + p.ring_px + 0.5;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = p.frame.sdf(x as f64 + 0.5, y as f64 + 0.5);
                assert!(
                    d <= -(p.ring_px + 0.5) + 1e-9,
                    "快速区像素 ({x},{y}) d={d} 不满足内缩量 {m}"
                );
            }
        }
    }
}
