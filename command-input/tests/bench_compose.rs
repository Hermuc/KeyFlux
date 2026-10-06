use std::time::Instant;

#[test]
fn bench_composite_full_frame() {
    // 近似真机几何: 926x880 (用户截窗口径)
    let mut p = plan_big();
    p.badge = None;
    let n = (p.w * p.h * 4) as usize;
    let mut dib = vec![128u8; n];
    // 预热
    cmdinput::compose::composite(&mut dib, &p, 1.0);
    let t = Instant::now();
    const N: u32 = 20;
    for _ in 0..N {
        cmdinput::compose::composite(&mut dib, &p, 1.0);
    }
    let el = t.elapsed() / N;
    println!("composite 926x880: {:?}", el);
    assert!(el.as_millis() < 100);
}

fn plan_big() -> cmdinput::compose::Plan {
    use cmdinput::compose::{Plan, Shadow, Shape};
    use cmdinput::skin::Rgb;
    Plan {
        w: 926,
        h: 880,
        frame: Shape {
            l: 12.0,
            t: 12.0,
            r: 914.0,
            b: 868.0,
            radius: 8.0,
        },
        ring_px: 3.75,
        ring_rgb: Rgb(255, 255, 255),
        ring_alpha: 1.0,
        fill_alpha: 0.945,
        shadow: Shadow {
            color: Rgb(0, 0, 0),
            opacity: 0.30,
            sigma: 3.0,
            dy: 2.0,
        },
        badge: None,
    }
}
