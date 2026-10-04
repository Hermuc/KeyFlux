//! 皮肤解析与合成数学 (R26/R27/R28; 纯逻辑, 零 Win32, 可脱离窗口 cargo test)。
//!
//! 解析口径: 行式 `key = value`,`#RRGGBB` / 十进制 / 小数; fail-safe —— 未知键忽略、
//! 单键非法回落该键内建默认、全文件缺失由调用方传 `""` 得 `DEFAULT`
//! (R27; 对齐参考实现 EverythingQueryEdit.ahk:163-192 的 `_Skin` 语义)。
//! 合成公式逐行移植自参考实现 (已实测净观感与原框 Δ2 以内):
//!   - 整窗 alpha = round(backgroundOpacity × 255)      (同 :158-159);
//!   - 网格内容色反解 `_GridContentColor`               (同 :199-215);
//!   - 2026-10-04 起不再用整窗 alpha: 逐像素合成 (`fill_alpha` / `ring_alpha` /
//!     `shadow_peak` → `crate::compose` → UpdateLayeredWindow), 理由见各函数注释。

use crate::config;

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

/// R28/参考实现 :199-215 `_GridContentColor` 的口径 (2026-10-04 起按本实现的合成式重写):
/// 「网格色 @gridlineOpacity 叠在**净背景**上」→ 反解回窗口**内容色** (GDI 该画的值)。
///
/// 合成式 (逐像素, 见 `crate::compose`): `out = F·α + (1−α)·desk`, 其中
/// `F` = 内容色 (GDI 画的), `α` = `fill_alpha`, `desk` = 桌面 (取 13 = #0D0D0D 深色,
/// 浅色桌面偏差极小, 同参考实现口径)。
///   - 面板净实体 `body_ch = backgroundColor_ch × backgroundOpacity` (原版实测 229.5 = 255×0.9);
///   - 净背景 `net_bg_ch = body_ch + (1−α)·desk`;
///   - 目标网格净色 `net_grid_ch = gridlineOpacity·gridline_ch + (1−gridlineOpacity)·net_bg_ch`;
///   - 反解 `F_ch = (net_grid_ch − (1−α)·desk) / α`。
pub fn grid_content_color(s: &Skin) -> Rgb {
    let a = fill_alpha(s);
    if !a.is_finite() || a <= 0.0 {
        // 除零保护 (皮肤值非法时): 回落白色 (网格不可见的最保守解)
        return Rgb(255, 255, 255);
    }
    let b = s.background_opacity.clamp(0.0, 1.0);
    let g_op = s.gridline_opacity;
    let desk: f64 = 13.0;
    let invert = |base: f64, grid: f64| -> u8 {
        let net_bg = base * b + (1.0 - a) * desk;
        let net = g_op * grid + (1.0 - g_op) * net_bg;
        ((net - (1.0 - a) * desk) / a).round().clamp(0.0, 255.0) as u8
    };
    Rgb(
        invert(s.background_color.0 as f64, s.gridline_color.0 as f64),
        invert(s.background_color.1 as f64, s.gridline_color.1 as f64),
        invert(s.background_color.2 as f64, s.gridline_color.2 as f64),
    )
}

/// R28/参考实现 :158-159: 整窗不透明度 = round(backgroundOpacity × 255)。
/// 0.9 → 229.5 → 230 (round half away from zero, 与 AHK Round 同口径)。
pub fn window_alpha(s: &Skin) -> u8 {
    (s.background_opacity * 255.0).round().clamp(0.0, 255.0) as u8
}

// ---- 逐像素合成所需的不透明度 (2026-10-04 样式还原) ----
//
// v1 只有一个整窗 alpha (`window_alpha`), 于是白边与内部的不透明度**必然相同** ——
// 原版「3px 纯白描边 + 半透明填充」因此表达不出来。改走 `UpdateLayeredWindow` 逐像素
// alpha 后, 三者各取自己的键:
//   白边 = `borderOpacity`(1.0, 不透明);
//   填充不透明度 = `fill_alpha` (皮肤 `backgroundOpacity` + 原版的底板衰减);
//   面板实体色   = `panel_content_color` (皮肤色 × `backgroundOpacity`, 再折回内容色空间);
//   网格/文字    = 由 `grid_content_color` / `key_color` 反解到同一内容色空间。
// 三条合起来的净观感精确等价于原版: `out = backgroundColor×b + (1−b)×0.55×bg`。

/// 填充层**净不透明度** (0..1): `1 − (1 − backgroundOpacity) × BACKDROP_DIM`。
///
/// 现值皮肤 (b = 0.9, DIM = 0.55) → **0.945**。标定依据见 `config::BACKDROP_DIM`:
/// 原版 vs 新版**同背景 A/B** (受控白/黑双底色联立) 解出原版 0.945、新版(改前) 0.812。
/// 0.812 让背景透出 18.8%, 是原版 (5.5%) 的 3.4 倍 —— 用户口径「过于透明、不够实」。
///
/// 与皮肤**单调同向**: 调低 `backgroundOpacity` ⇒ 更透 (且仍留一层底板衰减的上限)。
pub fn fill_alpha(s: &Skin) -> f64 {
    let b = s.background_opacity.clamp(0.0, 1.0);
    (1.0 - (1.0 - b) * config::BACKDROP_DIM).clamp(0.0, 1.0)
}

/// 面板**实体色** (0..255, 每通道): 皮肤色 × `backgroundOpacity`。
///
/// 语义 = 原版 DComp 视觉树里「填充视觉 (皮肤色 @ `backgroundOpacity`) 叠在窗口自己的
/// 实心底板上」的结果 —— 原版实测实体亮度 **229.5 = 255 × 0.9** (受控黑底上面板亮度)。
pub fn panel_body(s: &Skin) -> Rgb {
    let b = s.background_opacity.clamp(0.0, 1.0);
    let ch = |c: u8| (c as f64 * b).round().clamp(0.0, 255.0) as u8;
    Rgb(
        ch(s.background_color.0),
        ch(s.background_color.1),
        ch(s.background_color.2),
    )
}

/// 面板**内容色** (GDI 该画的平色, 每通道) = `实体色 / 净不透明度`。
///
/// 本实现的合成式是 `out = F·α + (1−α)·bg` (α = `fill_alpha`), 要得到实体亮度
/// `body`, 内容色必须取 `F = body/α`。现值皮肤 → 255×0.9 / 0.945 = **242.9**:
///   白底 (255): 0.945×242.9 + 0.055×255 = **243.5**  (原版实测 243 ✓)
///   黑底 (0):  0.945×242.9            = **229.5**  (原版实测 229 ✓)
/// 若直接画纯白 (改前), 白底上会得到 255 —— 与背景同化, 面板失去实体感。
pub fn panel_content_color(s: &Skin) -> Rgb {
    let a = fill_alpha(s);
    if !a.is_finite() || a <= 0.0 {
        return s.background_color; // 除零保护: 皮肤值非法时退回原色
    }
    let body = panel_body(s);
    let ch = |c: u8| (c as f64 / a).round().clamp(0.0, 255.0) as u8;
    Rgb(ch(body.0), ch(body.1), ch(body.2))
}

/// 白边层不透明度 (0..1): 皮肤 `borderOpacity` (现值 1.0 ⇒ 纯白不透明环, 原版实测 255)。
pub fn ring_alpha(s: &Skin) -> f64 {
    s.border_opacity.clamp(0.0, 1.0)
}

/// 阴影峰值不透明度 (0..1): 皮肤 `windowShadowOpacity` × 实测增益 (见 config 注释)。
/// 皮肤值先 clamp 到 [0,1] 再乘增益 ⇒ 峰值上限 = 增益 (手工改坏皮肤也不会出现全黑阴影)。
pub fn shadow_peak(s: &Skin) -> f64 {
    (s.window_shadow_opacity.clamp(0.0, 1.0) * config::SHADOW_PEAK_GAIN).clamp(0.0, 1.0)
}

/// GDI 单层近似的透明度混色: fg @alpha 叠 bg (R28 keyOpacity/borderOpacity 消费;
/// 默认 1.0 时无差异; 半透明时为向背景按不透明度混色近似 —— design C §2.5 如实标注口径)。
pub fn over(fg: Rgb, alpha: f64, bg: Rgb) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let mix = |f: u8, b: u8| -> u8 {
        (f as f64 * a + b as f64 * (1.0 - a))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb(mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2))
}

// ---- 结果列表面板配色 (2026-10-04) ----
//
// 口径: **不新增皮肤键** —— 结果区的配色一律由既有 18 键派生, 于是
//   ① 皮肤文件/生成端/parity 基线零改动;
//   ② 用户改皮肤时列表自动同基调 (与查询区网格同源)。
// 主题色取 `gridlineColor` (查询区网格就是它, 属框内的「accent」角色);
// 派生色与背景的对比度不足时 (用户把网格色改成了白/近白) 回落到文字色中性灰,
// 保证选中态永远可见 (对比度是唯一有效杠杆, 见项目 UI 基线)。

/// 两色最大通道差 (判断派生色是否被背景吃掉)。
fn channel_delta(a: Rgb, b: Rgb) -> u8 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).unsigned_abs() as u8;
    d(a.0, b.0).max(d(a.1, b.1)).max(d(a.2, b.2))
}

/// 主题色派生 + 可见性兜底: 与**面板内容色**差 < 8 时改用文字色中性灰。
///
/// 基准是 `panel_content_color` (而非皮肤原色): 面板底色在内容色空间里是 `242.9` 而不是
/// 255 (见 `panel_content_color` 的推导), 派生色必须落在**同一个空间**里, 否则列表底色/分隔线
/// 会相对面板整体偏亮一档 (实测会让选中行底色从「可见的浅蓝」变成「几乎看不见」)。
fn themed(s: &Skin, alpha: f64, fallback_alpha: f64) -> Rgb {
    let base = panel_content_color(s);
    let c = over(s.gridline_color, alpha, base);
    if channel_delta(c, base) < 8 {
        over(s.key_color, fallback_alpha, base)
    } else {
        c
    }
}

/// 选中行底色 (默认皮肤 → #EAECF7)。
pub fn list_select_color(s: &Skin) -> Rgb {
    themed(s, 0.10, 0.08)
}

/// 查询区/结果区分隔线色 (默认皮肤 → #E1E5F4)。
pub fn list_separator_color(s: &Skin) -> Rgb {
    themed(s, 0.14, 0.12)
}

/// 选中行左侧强调条色 (默认皮肤 → #697BC6)。
pub fn list_accent_color(s: &Skin) -> Rgb {
    themed(s, 0.70, 0.45)
}

/// 滚动条滑块色 (默认皮肤 → #D0D6ED)。
pub fn list_scroll_color(s: &Skin) -> Rgb {
    themed(s, 0.22, 0.18)
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

    /// 网格反解 (新口径): 内容色 **#EBECF0** ⇒ 净观感仍为 **#DEDFE4** (与旧口径逐通道 Δ≤1)。
    ///
    /// 关键点: 内容色随合成式改变 (旧 #F7F8FC), 但**用户看到的净值不变** —— `gridlineColor`
    /// @4% 叠在净背景 (#E6E6E6 级) 上的结果与皮肤的设计意图一致。
    #[test]
    fn grid_content_color_matches_reference() {
        let c = grid_content_color(&DEFAULT);
        assert_eq!(c, Rgb(0xEB, 0xEC, 0xF0));
        let a = fill_alpha(&DEFAULT);
        let desk: f64 = 13.0;
        // 净值 = 内容色·α + (1−α)·桌面 (本实现的合成式)
        let net = |ch: u8| a * ch as f64 + (1.0 - a) * desk;
        assert!((net(c.0) - 222.6).abs() <= 1.0, "R 净 {}", net(c.0));
        assert!((net(c.1) - 223.7).abs() <= 1.0, "G 净 {}", net(c.1));
        assert!((net(c.2) - 227.9).abs() <= 1.0, "B 净 {}", net(c.2));
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
        assert_eq!(
            over(Rgb(0, 0, 0), 0.5, Rgb(255, 255, 255)),
            Rgb(128, 128, 128)
        );
    }

    /// COLORREF 序 = 0x00BBGGRR。
    #[test]
    fn colorref_layout() {
        assert_eq!(Rgb(0x28, 0x43, 0xAD).as_colorref(), 0x00AD_4328);
    }

    /// 结果列表面板配色: 由既有 18 键派生, 默认皮肤下的具体色值 (AHK 侧无镜像, 仅本地口径)。
    ///
    /// 基准 = `panel_content_color` (243) 而非皮肤原色 (255) —— 面板底色在内容色空间里
    /// 就是 243 (2026-10-04 第三轮 A/B 收敛), 派生色必须同空间。
    #[test]
    fn list_palette_derived_from_skin() {
        assert_eq!(panel_content_color(&DEFAULT), Rgb(243, 243, 243));
        assert_eq!(list_select_color(&DEFAULT), Rgb(0xDF, 0xE1, 0xEC));
        assert_eq!(list_separator_color(&DEFAULT), Rgb(0xD7, 0xDA, 0xE9));
        assert_eq!(list_accent_color(&DEFAULT), Rgb(0x65, 0x78, 0xC2));
        assert_eq!(list_scroll_color(&DEFAULT), Rgb(0xC6, 0xCC, 0xE4));
    }

    /// 可见性兜底: 网格色改成近白 (派生色被背景吃掉) 时改用文字色中性灰。
    #[test]
    fn list_palette_falls_back_when_invisible() {
        // gridlineColor = #FFFFFF (= 背景) ⇒ 派生色恒等于背景 ⇒ 走兜底
        let s = parse("gridlineColor = #FFFFFF");
        let sel = list_select_color(&s);
        assert_eq!(sel, over(Rgb(0, 0, 0), 0.08, panel_content_color(&s)));
        assert!(channel_delta(sel, panel_content_color(&s)) >= 8);
        // 正常皮肤不触发兜底 (相对**面板内容色**的通道差: 243→223 = 20)
        assert_eq!(
            channel_delta(list_select_color(&DEFAULT), panel_content_color(&DEFAULT)),
            20
        );
    }

    /// 逐像素合成口径 (2026-10-04 样式还原 + 第三轮 A/B 收敛):
    /// 填充净不透明度 = `1 − (1−b)×0.55` = 0.945、面板内容色 = `皮肤色×b/α` = 242.9,
    /// 白边 = `borderOpacity` = 1.0, 阴影峰值 = `windowShadowOpacity` × 实测增益。
    #[test]
    fn per_pixel_alpha_matches_measured_original() {
        assert!((fill_alpha(&DEFAULT) - 0.945).abs() < 1e-9);
        assert!((ring_alpha(&DEFAULT) - 1.0).abs() < 1e-9);
        assert!((shadow_peak(&DEFAULT) - 0.30).abs() < 1e-9);
        // 实体/内容色: 白皮 0.9 ⇒ 实体 229.5, 内容 242.9 (白底净 243.5 / 黑底净 229.5)
        assert_eq!(panel_body(&DEFAULT), Rgb(230, 230, 230));
        assert_eq!(panel_content_color(&DEFAULT), Rgb(243, 243, 243));
        let a = fill_alpha(&DEFAULT);
        let over_white = a * 242.857 + (1.0 - a) * 255.0;
        let over_black = a * 242.857;
        assert!(
            (over_white - 243.5).abs() < 1.0,
            "白底净亮度 {over_white:.1}"
        );
        assert!(
            (over_black - 229.5).abs() < 1.0,
            "黑底净亮度 {over_black:.1}"
        );
        // 皮肤语义必须可退: 不透明度调回 1.0 ⇒ 填充真的不透明 (白边/填充不再区分)
        let opaque =
            parse("backgroundOpacity = 1.0\nborderOpacity = 0.5\nwindowShadowOpacity = 1.0");
        assert!((fill_alpha(&opaque) - 1.0).abs() < 1e-9);
        assert!((ring_alpha(&opaque) - 0.5).abs() < 1e-9);
        assert!((shadow_peak(&opaque) - 0.6).abs() < 1e-9);
        // 越界值必须被 clamp (皮肤文件可被手工改坏)
        let bad = parse("backgroundOpacity = 9\nborderOpacity = -3\nwindowShadowOpacity = 9");
        assert!((fill_alpha(&bad) - 1.0).abs() < 1e-9);
        assert_eq!(ring_alpha(&bad), 0.0);
        assert!((shadow_peak(&bad) - 0.6).abs() < 1e-9);
    }
}
