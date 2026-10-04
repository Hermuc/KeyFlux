//! 淡出缓动 (R15; 纯逻辑)。
//!
//! 原版 = Windows.UI.Animation **Accelerate-Decelerate** 过渡, 加速比 = 减速比 = 0.5
//! (断言串 @0x1C880: `CreateAccelerateDecelerateTransition(c.hideAnimationDuration,
//! clientSize.width / 2.0, 0.5, 0.5, &transition)`)。0.5/0.5 参数下该曲线即 smoothstep。

/// Accelerate-Decelerate (0.5, 0.5) → smoothstep: 输入 t ∈ [0,1] (时间归一),
/// 输出 ∈ [0,1] (进度)。端点精确 0/1, 单调递增; 非有限输入防毒
/// (NaN → 0.0, +∞ → 1.0, −∞ → 0.0 —— 异常皮肤值时不产生 NaN 进度)。
pub fn accelerate_decelerate(t: f64) -> f64 {
    if !t.is_finite() {
        return if t > 0.0 { 1.0 } else { 0.0 };
    }
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_center() {
        assert_eq!(accelerate_decelerate(0.0), 0.0);
        assert_eq!(accelerate_decelerate(1.0), 1.0);
        assert_eq!(accelerate_decelerate(0.5), 0.5);
    }

    #[test]
    fn monotonic_and_bounded() {
        let mut prev = -1.0;
        let steps = 200;
        for i in 0..=steps {
            let v = accelerate_decelerate(i as f64 / steps as f64);
            assert!(v >= prev, "曲线必须单调不减");
            assert!((0.0..=1.0).contains(&v));
            prev = v;
        }
    }

    /// 越界输入 clamp (时长为 0 / 异常皮肤值时调用方直接短路, 不应产生 NaN)。
    #[test]
    fn clamps_out_of_range() {
        assert_eq!(accelerate_decelerate(-0.5), 0.0);
        assert_eq!(accelerate_decelerate(1.5), 1.0);
        assert!(!accelerate_decelerate(f64::NAN).is_nan());
    }
}
