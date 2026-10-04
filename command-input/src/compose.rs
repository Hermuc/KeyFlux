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
}

impl Plan {
    /// 单像素合成结果: `(alpha 0..1, 预乘 RGB 0..255)`。
    /// `drawn_rgb` = GDI 画出的内容色 (背景/网格/文字/结果行; 越界时传任意值)。
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

    /// 供调试/单测的 alpha 掩码。
    pub fn alpha_mask(&self) -> Vec<u8> {
        let mut out = vec![0u8; (self.w * self.h) as usize];
        for y in 0..self.h {
            for x in 0..self.w {
                let (a, _) = self.pixel(x, y, [255.0; 3]);
                out[(y * self.w + x) as usize] = to_u8(a * 255.0);
            }
        }
        out
    }
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn to_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// 就地合成: 读 GDI 画好的 RGB (BGRA 序, 与 `BITMAPINFOHEADER` 的 32bpp DIB 一致),
/// 写入**预乘** RGBA (UpdateLayeredWindow + `AC_SRC_ALPHA` 的要求)。
/// `gain` = 整体不透明度增益 (0..1; 淡出动效用), `dib` 必须与 `plan.w×plan.h` 同尺寸。
pub fn composite(dib: &mut [u8], plan: &Plan, gain: f64) {
    let g = gain.clamp(0.0, 1.0);
    for y in 0..plan.h {
        for x in 0..plan.w {
            let i = ((y * plan.w + x) * 4) as usize;
            if i + 3 >= dib.len() {
                return;
            }
            let drawn = [dib[i] as f64, dib[i + 1] as f64, dib[i + 2] as f64];
            let (a, pm) = plan.pixel(x, y, drawn);
            // pm 已是**预乘**值 (含各自的 alpha) ⇒ 这里只再叠整体增益 g, 不能再乘一次 a。
            dib[i] = to_u8(pm[2] * g); // B
            dib[i + 1] = to_u8(pm[1] * g); // G
            dib[i + 2] = to_u8(pm[0] * g); // R
            dib[i + 3] = to_u8(a * 255.0 * g); // A
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
}
