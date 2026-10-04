//! 皮肤解析与合成数学 (R26/R27/R28; 纯逻辑, 零 Win32, 可脱离窗口 cargo test)。
//!
//! 解析口径: 行式 `key = value`,`#RRGGBB` / 十进制 / 小数; fail-safe —— 未知键忽略、
//! 单键非法回落该键内建默认、全文件缺失由调用方传 `""` 得 `DEFAULT`
//! (R27; 对齐参考实现 EverythingQueryEdit.ahk:163-192 的 `_Skin` 语义)。
//! 合成公式逐行移植自参考实现 (已实测净观感与原框 Δ2 以内):
//!   - 整窗 alpha = round(backgroundOpacity × 255)      (同 :158-159);
//!   - 网格内容色反解 `_GridContentColor`               (同 :199-215);
//!   - keyOpacity / borderOpacity 以「向背景混色」消费   (GDI 单层近似, 见 render 注记)。

/// 8bit RGB (皮肤 `#RRGGBB` 词法)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// COLORREF 序 = 0x00BBGGRR (GDI 契约)。
    pub fn as_colorref(self) -> u32 {
        self.0 as u32 | (self.1 as u32) << 8 | (self.2 as u32) << 16
    }
}

/// R26: 18 键全表 (键名 = 皮肤文件键名; 现值 = 内建默认, R27 回落口径)。
#[derive(Clone, Debug)]
pub struct Skin {
    pub background_color: Rgb,
    pub background_opacity: f64,
    pub border_width: f64,
    pub border_color: Rgb,
    pub border_opacity: f64,
    pub border_radius: f64,
    pub corner_color: Rgb,
    pub corner_opacity: f64,
    pub gridline_color: Rgb,
    pub gridline_opacity: f64,
    pub key_color: Rgb,
    pub key_opacity: f64,
    pub hide_animation_duration: f64,
    pub window_y_pos: f64,
    pub window_width: f64,
    pub window_shadow_color: Rgb,
    pub window_shadow_opacity: f64,
    pub window_shadow_size: f64,
}

/// 18 键现值 = `D:\PortableApps\KeyFlux\bin\CommandInputSkin.txt` 逐键核对 (R26 表)。
pub const DEFAULT: Skin = Skin {
    background_color: Rgb(0xFF, 0xFF, 0xFF),
    background_opacity: 0.9,
    border_width: 3.0,
    border_color: Rgb(0xFF, 0xFF, 0xFF),
    border_opacity: 1.0,
    border_radius: 10.0,
    corner_color: Rgb(0x00, 0x00, 0x00),
    corner_opacity: 0.0,
    gridline_color: Rgb(0x28, 0x43, 0xAD),
    gridline_opacity: 0.04,
    key_color: Rgb(0x00, 0x00, 0x00),
    key_opacity: 1.0,
    hide_animation_duration: 0.34,
    window_y_pos: 0.25,
    window_width: 700.0,
    window_shadow_color: Rgb(0x00, 0x00, 0x00),
    window_shadow_opacity: 0.5,
    window_shadow_size: 3.0,
};

/// fail-safe 解析 (R27): 永不失败、不崩溃; 非法值回落该键默认; 未知键忽略。
pub fn parse(text: &str) -> Skin {
    let mut s = DEFAULT;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue; // 无 '=' 的行 (空行/注释) 跳过
        };
        s.apply(k.trim(), v.trim());
    }
    s
}

/// 多余空白容忍 (R27): 值先 trim 再词法; 解析失败保持该键默认。
fn parse_f64(v: &str) -> Option<f64> {
    v.parse::<f64>().ok().filter(|f| f.is_finite())
}

fn parse_color(v: &str) -> Option<Rgb> {
    let hex = v.strip_prefix('#').unwrap_or(v);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

impl Skin {
    /// 单键写入; 键名 ASCII 精确匹配 18 键集合 (R26); 其余一律忽略。
    fn apply(&mut self, key: &str, value: &str) {
        match key {
            "backgroundColor" => {
                if let Some(c) = parse_color(value) {
                    self.background_color = c;
                }
            }
            "backgroundOpacity" => {
                if let Some(f) = parse_f64(value) {
                    self.background_opacity = f;
                }
            }
            "borderWidth" => {
                if let Some(f) = parse_f64(value) {
                    self.border_width = f;
                }
            }
            "borderColor" => {
                if let Some(c) = parse_color(value) {
                    self.border_color = c;
                }
            }
            "borderOpacity" => {
                if let Some(f) = parse_f64(value) {
                    self.border_opacity = f;
                }
            }
            "borderRadius" => {
                if let Some(f) = parse_f64(value) {
                    self.border_radius = f;
                }
            }
            "cornerColor" => {
                if let Some(c) = parse_color(value) {
                    self.corner_color = c;
                }
            }
            "cornerOpacity" => {
                if let Some(f) = parse_f64(value) {
                    self.corner_opacity = f;
                }
            }
            "gridlineColor" => {
                if let Some(c) = parse_color(value) {
                    self.gridline_color = c;
                }
            }
            "gridlineOpacity" => {
                if let Some(f) = parse_f64(value) {
                    self.gridline_opacity = f;
                }
            }
            "keyColor" => {
                if let Some(c) = parse_color(value) {
                    self.key_color = c;
                }
            }
            "keyOpacity" => {
                if let Some(f) = parse_f64(value) {
                    self.key_opacity = f;
                }
            }
            "hideAnimationDuration" => {
                if let Some(f) = parse_f64(value) {
                    self.hide_animation_duration = f;
                }
            }
            "windowYPos" => {
                if let Some(f) = parse_f64(value) {
                    self.window_y_pos = f;
                }
            }
            "windowWidth" => {
                if let Some(f) = parse_f64(value) {
                    self.window_width = f;
                }
            }
            "windowShadowColor" => {
                if let Some(c) = parse_color(value) {
                    self.window_shadow_color = c;
                }
            }
            "windowShadowOpacity" => {
                if let Some(f) = parse_f64(value) {
                    self.window_shadow_opacity = f;
                }
            }
            "windowShadowSize" => {
                if let Some(f) = parse_f64(value) {
                    self.window_shadow_size = f;
                }
            }
            _ => {} // R27: 未知键忽略
        }
    }
}

/// R28/参考实现 :199-215 `_GridContentColor` 逐行移植:
/// 网格色 @gridlineOpacity 叠在净背景上, 再按整窗 alpha (backgroundOpacity) 反解回
/// 窗口内容色 —— 桌面假定 13 (#0D0D0D 深色, 浅色桌面偏差极小, 同参考实现口径)。
/// 现值皮肤 → 内容 #F7F8FC, 0.9 叠深色桌面 → 净 #DFE0E4 (图1 实测 Δ1)。
pub fn grid_content_color(s: &Skin) -> Rgb {
    let bg_op = s.background_opacity;
    if !(bg_op > 0.0) {
        // 除零保护 (皮肤值非法时): 回落白色 (网格不可见的最保守解)
        return Rgb(255, 255, 255);
    }
    let g_op = s.gridline_opacity;
    let desk: f64 = 13.0;
    let invert = |ch: f64| -> u8 {
        let nb = bg_op * 255.0 + (1.0 - bg_op) * desk;
        let ng = g_op * ch + (1.0 - g_op) * nb;
        let c = ((ng - (1.0 - bg_op) * desk) / bg_op).round();
        c.clamp(0.0, 255.0) as u8
    };
    Rgb(
        invert(s.gridline_color.0 as f64),
        invert(s.gridline_color.1 as f64),
        invert(s.gridline_color.2 as f64),
    )
}

/// R28/参考实现 :158-159: 整窗不透明度 = round(backgroundOpacity × 255)。
/// 0.9 → 229.5 → 230 (round half away from zero, 与 AHK Round 同口径)。
pub fn window_alpha(s: &Skin) -> u8 {
    (s.background_opacity * 255.0).round().clamp(0.0, 255.0) as u8
}

/// GDI 单层近似的透明度混色: fg @alpha 叠 bg (R28 keyOpacity/borderOpacity 消费;
/// 默认 1.0 时无差异; 半透明时为向背景按不透明度混色近似 —— design C §2.5 如实标注口径)。
pub fn over(fg: Rgb, alpha: f64, bg: Rgb) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let mix = |f: u8, b: u8| -> u8 {
        (f as f64 * a + b as f64 * (1.0 - a)).round().clamp(0.0, 255.0) as u8
    };
    Rgb(
        mix(fg.0, bg.0),
        mix(fg.1, bg.1),
        mix(fg.2, bg.2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R26: 真实皮肤文件 (D:\PortableApps\KeyFlux\bin\CommandInputSkin.txt 原文) 全键解析。
    #[test]
    fn parse_real_skin_all_18_keys() {
        let text = concat!(
            "backgroundColor         = #FFFFFF\n",
            "backgroundOpacity       = 0.9\n",
            "borderWidth             = 3\n",
            "borderColor             = #FFFFFF\n",
            "borderOpacity           = 1.0\n",
            "borderRadius            = 10\n",
            "cornerColor             = #000000\n",
            "cornerOpacity           = 0.0\n",
            "gridlineColor           = #2843AD\n",
            "gridlineOpacity         = 0.04\n",
            "keyColor                = #000000\n",
            "keyOpacity              = 1.0\n",
            "hideAnimationDuration   = 0.34\n",
            "windowYPos              = 0.25\n",
            "windowWidth             = 700\n",
            "windowShadowColor       = #000000\n",
            "windowShadowOpacity     = 0.5\n",
            "windowShadowSize        = 3.0",
        );
        let s = parse(text);
        assert_eq!(s.background_color, Rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(s.background_opacity, 0.9);
        assert_eq!(s.border_width, 3.0);
        assert_eq!(s.border_color, Rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(s.border_opacity, 1.0);
        assert_eq!(s.border_radius, 10.0);
        assert_eq!(s.corner_color, Rgb(0x00, 0x00, 0x00));
        assert_eq!(s.corner_opacity, 0.0);
        assert_eq!(s.gridline_color, Rgb(0x28, 0x43, 0xAD));
        assert_eq!(s.gridline_opacity, 0.04);
        assert_eq!(s.key_color, Rgb(0x00, 0x00, 0x00));
        assert_eq!(s.key_opacity, 1.0);
        assert_eq!(s.hide_animation_duration, 0.34);
        assert_eq!(s.window_y_pos, 0.25);
        assert_eq!(s.window_width, 700.0);
        assert_eq!(s.window_shadow_color, Rgb(0x00, 0x00, 0x00));
        assert_eq!(s.window_shadow_opacity, 0.5);
        assert_eq!(s.window_shadow_size, 3.0);
    }

    /// R27: 文件缺失 (空文本) → 全默认。
    #[test]
    fn parse_empty_falls_back_to_default() {
        let s = parse("");
        assert_eq!(s.background_opacity, DEFAULT.background_opacity);
        assert_eq!(s.gridline_color, DEFAULT.gridline_color);
        assert_eq!(s.border_radius, DEFAULT.border_radius);
        assert_eq!(s.window_width, DEFAULT.window_width);
    }

    /// R27: 单键非法回落该键默认; 未知键忽略; 空白容忍。
    #[test]
    fn parse_bad_values_fall_back_per_key() {
        let s = parse(concat!(
            "borderRadius = abc\n",
            "backgroundOpacity = \n",
            "gridlineColor = #XYZ123\n",
            "unknownKey = 42\n",
            "   windowWidth   =   800  \n",
            "windowYPos = 2\n", // 无 '=' 前导空白也容忍
        ));
        assert_eq!(s.border_radius, 10.0); // 非法 → 默认
        assert_eq!(s.background_opacity, 0.9); // 空值 → 默认
        assert_eq!(s.gridline_color, Rgb(0x28, 0x43, 0xAD)); // 坏色 → 默认
        assert_eq!(s.window_width, 800.0); // 合法键生效
        assert_eq!(s.window_y_pos, 2.0); // 数值照收 (消费侧 clamp 由几何/渲染负责)
    }

    /// #RRGGBB 词法 (含无 # 形式, 对齐参考实现 RegExReplace 口径)。
    #[test]
    fn color_lexing() {
        let s = parse("gridlineColor = #2843AD\nborderColor = FFFFFF\n");
        assert_eq!(s.gridline_color, Rgb(0x28, 0x43, 0xAD));
        assert_eq!(s.border_color, Rgb(0xFF, 0xFF, 0xFF));
    }

    /// R28: 网格反解 → 内容 #F7F8FC; 净观感 → #DFE0E4 (图1 实测, 容差 Δ1)。
    #[test]
    fn grid_content_color_matches_reference() {
        let c = grid_content_color(&DEFAULT);
        assert_eq!(c, Rgb(0xF7, 0xF8, 0xFC));
        let desk: f64 = 13.0;
        let net = |rgb: Rgb| -> f64 { DEFAULT.background_opacity * rgb.0 as f64 + (1.0 - DEFAULT.background_opacity) * desk };
        assert!((net(c) - 0xDF as f64).abs() <= 1.0);
        assert!((net(Rgb(c.1, c.1, c.1)) - 0xE0 as f64).abs() <= 1.0); // G 通道
        let net_b = DEFAULT.background_opacity * c.2 as f64 + (1.0 - DEFAULT.background_opacity) * desk;
        assert!((net_b - 0xE4 as f64).abs() <= 1.0); // B 通道
    }

    /// R28: 整窗 alpha = round(0.9×255) = 230 (参考实现 :159 同口径)。
    #[test]
    fn window_alpha_matches_reference() {
        assert_eq!(window_alpha(&DEFAULT), 230);
        assert_eq!(window_alpha(&parse("backgroundOpacity = 1.0")), 255);
        assert_eq!(window_alpha(&parse("backgroundOpacity = 0")), 0);
    }

    /// R28: keyOpacity=1 无差异; 0.5 半混。
    #[test]
    fn opacity_blending() {
        assert_eq!(over(Rgb(0, 0, 0), 1.0, Rgb(255, 255, 255)), Rgb(0, 0, 0));
        assert_eq!(over(Rgb(0, 0, 0), 0.5, Rgb(255, 255, 255)), Rgb(128, 128, 128));
    }

    /// COLORREF 序 = 0x00BBGGRR。
    #[test]
    fn colorref_layout() {
        assert_eq!(Rgb(0x28, 0x43, 0xAD).as_colorref(), 0x00AD_4328);
    }
}
