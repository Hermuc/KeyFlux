//! v1 渲染后端 (design C §2.5): GDI + 整窗 layered alpha + LWA_COLORKEY 透明带 + DWM 阴影钩子。
//!
//! 合成数学照抄参考实现 (净观感与原框逐像素 Δ2 已验证):
//!   * 整窗 alpha = round(backgroundOpacity × 255)      (EverythingQueryEdit.ahk:158-159);
//!   * 网格内容色反解 `_GridContentColor`               (同 :199-215, 经 crate::skin::grid_content_color);
//!   * 网格间距 = round(20 DIP × dpi/96) ≥8, 首线偏移 = 间距−1 (:143-146, crate::geometry);
//!   * 白框圆角 = CreateRoundRectRgn 2r (:234-246 口径, r = borderRadius 物理像素);
//!   * DWM 阴影 = DwmSetWindowAttribute(NCRENDERING_POLICY) + DwmExtendFrameIntoClientArea
//!     (:252-262 口径; R28「阴影 = DWM 系统阴影机制」)。
//!
//! 机制注记 (design A 探针实证, 本机当场运行):
//!   * 42px 透明带 + 圆角裁切用 **LWA_COLORKEY** (#FF00FF, 与内容色域恒不相交) ——
//!     SetWindowRgn 在 SetLayeredWindowAttributes 分层窗口上返回 TRUE 但不参与合成
//!     (design A 探针 V2 实证), 故不采 region 方案;
//!   * ⚠ **LWA_ALPHA 与 LWA_COLORKEY 必须在同一次 SetLayeredWindowAttributes 调用里
//!     同时声明** (2026-10-04 活体踩坑, cmdinput-re/probe_diag.py): 分两次调用时,
//!     后一次会把未声明的属性**重置回默认** (先 LWA_ALPHA(230) 再 LWA_COLORKEY → alpha
//!     被重置为 255, 整窗变不透明, 净背景 #FFFFFF ≠ 原版 #E6E6E6; 反向单 flag 调用亦
//!     清掉色键, 带 #FF00FF 外露)。本文件所有 SLWA 调用一律两 flag 同发;
//!   * 分层窗口无 DWM 阴影 (探针 outer5px 逐位同底色) → 42px 带 = 纯透明, 带**无内阴影**
//!     (design A §4.4 的诚实降级, R28 为 should 级); DWM 钩子零成本零风险保留, 有效则白赚;
//!     windowShadow* 三键在 v1 只解析不渲染 (v2 DComp 后端以 D2D1Shadow 消费三键)。
//!
//! 文字注记 (R23/R28 + 2026-10-04 原版活体排版定案, probe_diag.py):
//!   * 显示层对拉丁字母做**大写化** (原版对 'abc' 输入渲染 'A B C' 观感, 字形放大图确证;
//!     CharUpperW 同源语义; 文本缓冲不动 —— R17/R22 只约束缓冲, 渲染层变换不计);
//!   * 排版 = **逐字形固定步距** pitch = 4×网格步距 DIP (80 DIP → 100px @125%; 原版 8 字符
//!     实测 pitch 总和 700.5px/7 = 100.07px), 字形居中于各自单元格 (DT_CENTER), 整串以
//!     白框中心为轴 (原版单字符中心 463 vs 框中心 462.5), 垂直居中 (DT_VCENTER);
//!   * GDI DrawTextW + ANTIALIASED_QUALITY (灰度 AA, 避开分层半透明窗上 ClearType 彩边);
//!     字体经 AddFontResourceExW FR_PRIVATE 加载 bin\font\font.ttf + font.bold.ttf,
//!     CreateFontW 以族名 + FW_BOLD 请求; 缺 font.ttf 按 R29 判渲染初始化失败
//!     (弹窗终止, 不静默错渲染)。

use std::time::{Duration, Instant};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmIsCompositionEnabled, DwmSetWindowAttribute,
    DWMNCRENDERINGPOLICY, DWMWA_NCRENDERING_POLICY, DWMNCRP_ENABLED,
};
use windows::Win32::Graphics::Gdi::{
    AddFontResourceExW, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW,
    FillRect, FR_PRIVATE, GetDC, HBRUSH, HFONT, HRGN, HGDIOBJ, ReleaseDC, SelectClipRgn,
    SelectObject, SetBkMode, SetTextColor, ValidateRect, ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DRAW_TEXT_FORMAT, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::System::Threading::Sleep;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    CharUpperW, SetLayeredWindowAttributes, LWA_ALPHA, LWA_COLORKEY,
};

use crate::config;
use crate::easing::accelerate_decelerate;
use crate::geometry;
use crate::render::{BackendError, FrameState, RenderBackend};
use crate::skin::{self, Rgb};
use crate::win::resources;

/// 懒创建的 GDI 资源 (R23: 首次 WM_PAINT 才创建)。
struct Resources {
    font: HFONT,
    key_brush: HBRUSH,    // 色键刷 (透明裁切; 绝不用于可见内容)
    bg_brush: HBRUSH,     // 背景刷 (backgroundColor)
    grid_brush: HBRUSH,   // 网格刷 (grid_content_color 反解色)
    border_brush: HBRUSH, // 边框刷 (borderColor@borderOpacity 向背景混色)
    clip: HRGN,           // 白框圆角区域 (窗口坐标, 2r 椭圆)
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.font.0));
            let _ = DeleteObject(HGDIOBJ(self.key_brush.0));
            let _ = DeleteObject(HGDIOBJ(self.bg_brush.0));
            let _ = DeleteObject(HGDIOBJ(self.grid_brush.0));
            let _ = DeleteObject(HGDIOBJ(self.border_brush.0));
            let _ = DeleteObject(HGDIOBJ(self.clip.0));
        }
    }
}

/// v1 GDI 后端。
pub struct GdiBackend {
    hwnd: HWND,
    /// 整窗 alpha (init 时定案 = round(backgroundOpacity×255); fade 终态后由 on_hidden 复原)
    alpha: u8,
    /// 边框宽 (像素, init 时按 DPI 折算)
    border_px: i32,
    /// 文本色 COLORREF (keyColor@keyOpacity 向背景混色)
    text_color: COLORREF,
    res: Option<Resources>,
}

impl GdiBackend {
    pub fn new() -> Self {
        Self {
            hwnd: HWND::default(),
            alpha: 255,
            border_px: 0,
            text_color: COLORREF(0),
            res: None,
        }
    }

    /// 整窗 layered 属性: alpha + 色键**单次调用**同时声明 (见模块注记 —— 分次调用会互相重置)。
    fn lwa(&self, alpha: u8) {
        unsafe {
            let _ = SetLayeredWindowAttributes(
                self.hwnd,
                COLORREF(config::COLORKEY),
                alpha,
                LWA_ALPHA | LWA_COLORKEY,
            );
        }
    }

    fn create_resources(state: &FrameState) -> Result<Resources, BackendError> {
        let dpi = state.dpi;
        let s = state.skin;
        let exe_dir = resources::exe_dir();

        // 字体 (R23/R25): FR_PRIVATE = 仅本进程可见, 直接 CreateFontW 按族名解析可用
        // (参考实现 :218-232 的「AHK SetFont 需 0x00 公共档」教训只针对其枚举校验路径)
        let font_ttf = resources::font_file(&exe_dir);
        let path_w = super::wide(&font_ttf.display().to_string());
        let loaded = unsafe {
            AddFontResourceExW(PCWSTR::from_raw(path_w.as_ptr()), FR_PRIVATE, None)
        };
        if loaded == 0 {
            // R29: 字体缺失 = 渲染初始化失败 → 弹窗终止 (不静默错渲染; 音效/皮肤才降级)
            return Err(BackendError::with_hresult(
                format!("AddFontResourceExW failed: {}", font_ttf.display()),
                0x8007_0002, // ERROR_FILE_NOT_FOUND 语义
            ));
        }
        let bold_ttf = resources::font_bold_file(&exe_dir);
        let bold_w = super::wide(&bold_ttf.display().to_string());
        unsafe { AddFontResourceExW(PCWSTR::from_raw(bold_w.as_ptr()), FR_PRIVATE, None) }; // 真粗体面 best effort

        // 粗体 44.0 DIP → -55px @125% (负值 = 字符高度; R23)
        let h_px = geometry::font_height_px(dpi);
        let face = super::wide(config::FONT_FAMILY);
        let font = unsafe {
            CreateFontW(
                -h_px,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                0, // DEFAULT_PITCH | FF_DONTCARE
                PCWSTR::from_raw(face.as_ptr()),
            )
        };
        if font.0.is_null() {
            return Err(BackendError::new("CreateFontW failed"));
        }

        // 网格内容色 (反解) 与边框混色 (R28); 文本色在 init 里算 (存 self.text_color)
        let grid = skin::grid_content_color(s);
        let border = skin::over(s.border_color, s.border_opacity, s.background_color);

        let brush = |c: Rgb| -> Result<HBRUSH, BackendError> {
            let b = unsafe { CreateSolidBrush(COLORREF(c.as_colorref())) };
            if b.0.is_null() {
                Err(BackendError::new("CreateSolidBrush failed"))
            } else {
                Ok(b)
            }
        };

        let key_brush = brush(Rgb(0xFF, 0x00, 0xFF))?; // = config::COLORKEY (#FF00FF)
        let bg_brush = brush(s.background_color)?;
        let grid_brush = brush(grid)?;
        let border_brush = brush(border)?;

        // 白框圆角区域 (窗口坐标): 四周缩 band, 椭圆 2r (参考实现 :241 口径, r 物理像素)
        let inset = geometry::band_inset_px(dpi);
        let r = (s.border_radius.round() as i32).max(0);
        let clip = unsafe {
            CreateRoundRectRgn(
                inset,
                inset,
                state.width_px - inset,
                state.height_px - inset,
                2 * r,
                2 * r,
            )
        };
        if clip.0.is_null() {
            return Err(BackendError::new("CreateRoundRectRgn failed"));
        }

        Ok(Resources {
            font,
            key_brush,
            bg_brush,
            grid_brush,
            border_brush,
            clip,
        })
    }

    /// 一次完整重绘。`validate = true` 时结尾 ValidateRect (R23: WM_PAINT 处理尾)。
    fn draw(&mut self, state: &FrameState, validate: bool) -> Result<(), BackendError> {
        let res = match self.res.as_ref() {
            Some(r) => r,
            None => return Err(BackendError::new("GDI backend not initialized")),
        };
        unsafe {
            let hdc = GetDC(Some(self.hwnd));
            if hdc.0.is_null() {
                return Err(BackendError::new("GetDC failed"));
            }

            // 1) 全客户区色键 → 42px 边带 + 圆角外区域合成时完全剔除 (探针 V1 实证)
            let whole = RECT {
                left: 0,
                top: 0,
                right: state.width_px,
                bottom: state.height_px,
            };
            FillRect(hdc, &whole, res.key_brush);

            // 裁剪到白框圆角区域 (GDI 裁剪; 与色键配合, 边带像素恒为色键)
            SelectClipRgn(hdc, Some(res.clip));

            // 2) 背景: 白框矩形填 backgroundColor → ×0.9 叠深色桌面 ⇒ 净 #E6E6E6
            let inset = geometry::band_inset_px(state.dpi);
            let frame = RECT {
                left: inset,
                top: inset,
                right: (state.width_px - inset).max(inset),
                bottom: (state.height_px - inset).max(inset),
            };
            FillRect(hdc, &frame, res.bg_brush);

            // 3) 边框环 (R28; 默认白边叠白底 = 不可见, 与原版观感一致)
            self.draw_border(hdc, &frame, res);

            // 4) 网格 (R28): 横竖双向 1px, 首线偏移 = 间距−1 (参考实现 :148-157 坐标口径)
            let (fw, fh) = (frame.right - frame.left, frame.bottom - frame.top);
            let step = geometry::grid_step_px(state.dpi);
            let off = geometry::grid_first_offset(step);
            let mut x = off;
            while x < fw - 2 {
                let line = RECT {
                    left: frame.left + x,
                    top: frame.top + 2,
                    right: frame.left + x + 1,
                    bottom: frame.top + 2 + (fh - 4),
                };
                FillRect(hdc, &line, res.grid_brush);
                x += step;
            }
            let mut y = off;
            while y < fh - 2 {
                let line = RECT {
                    left: frame.left + 2,
                    top: frame.top + y,
                    right: frame.left + 2 + (fw - 4),
                    bottom: frame.top + y + 1,
                };
                FillRect(hdc, &line, res.grid_brush);
                y += step;
            }

            // 5) 文本 (R23 + 原版排版活体定案): 显示层大写化 + 逐字形固定步距布局。
            //    缓冲 (state.text) 不动 —— 大写化只作用于显示副本 (R17/R22)。
            let mut units: Vec<u16> = state.text.to_vec();
            if !units.is_empty() {
                let old = SelectObject(hdc, HGDIOBJ(res.font.0));
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, self.text_color);
                // 显示层大写化 (原版观感 'abc'→'A B C', CharUpperW 同源; NUL 结尾就地变换)
                units.push(0);
                CharUpperW(PWSTR(units.as_mut_ptr()));
                units.pop();
                // 码元 → 字形单元 (代理对两码元并作一格)
                let mut cells: Vec<(usize, usize)> = Vec::new(); // (start, len)
                let mut i = 0usize;
                while i < units.len() {
                    let len = if (0xD800..0xDC00).contains(&units[i])
                        && i + 1 < units.len()
                        && (0xDC00..0xE000).contains(&units[i + 1])
                    {
                        2
                    } else {
                        1
                    };
                    cells.push((i, len));
                    i += len;
                }
                // 每字形: pitch 步距单元格内水平居中 (DT_CENTER), 整串以白框中心为轴,
                // 垂直居中 (DT_VCENTER); 超宽裁剪不加省略号 (region 裁剪)
                let pitch = geometry::text_pitch_px(state.dpi);
                let n = cells.len() as i32;
                let center_x = frame.left + fw / 2;
                for (idx, &(start, len)) in cells.iter().enumerate() {
                    let cell_left = center_x - (n * pitch) / 2 + idx as i32 * pitch;
                    let mut tr = RECT {
                        left: cell_left,
                        top: frame.top,
                        right: cell_left + pitch,
                        bottom: frame.bottom,
                    };
                    let mut cellu = units[start..start + len].to_vec();
                    DrawTextW(
                        hdc,
                        &mut cellu,
                        &mut tr,
                        DRAW_TEXT_FORMAT(DT_SINGLELINE.0 | DT_CENTER.0 | DT_VCENTER.0 | DT_NOPREFIX.0),
                    );
                }
                SelectObject(hdc, old);
            }

            SelectClipRgn(hdc, None);
            ReleaseDC(Some(self.hwnd), hdc);
            if validate {
                let _ = ValidateRect(Some(self.hwnd), None); // R23: 处理尾 ValidateRect
            }
        }
        Ok(())
    }

    fn draw_border(&self, hdc: windows::Win32::Graphics::Gdi::HDC, frame: &RECT, res: &Resources) {
        let bw = self.border_px;
        if bw <= 0 {
            return;
        }
        let (fw, fh) = (frame.right - frame.left, frame.bottom - frame.top);
        let bw = bw.min(fw / 2).min(fh / 2);
        unsafe {
            for i in 0..bw {
                let strips = [
                    RECT { left: frame.left + i, top: frame.top + i, right: frame.right - i, bottom: frame.top + i + 1 },                    // top
                    RECT { left: frame.left + i, top: frame.bottom - i - 1, right: frame.right - i, bottom: frame.bottom - i },                // bottom
                    RECT { left: frame.left + i, top: frame.top + i, right: frame.left + i + 1, bottom: frame.bottom - i },                    // left
                    RECT { left: frame.right - i - 1, top: frame.top + i, right: frame.right - i, bottom: frame.bottom - i },                  // right
                ];
                for r in &strips {
                    FillRect(hdc, r, res.border_brush);
                }
            }
        }
    }
}

impl RenderBackend for GdiBackend {
    /// GDI 后端追加位 = WS_EX_LAYERED (0x0008_0000) → 实际 ex-style 0x0808_0008
    /// (NOREDIRECTIONBITMAP 按 spec 附录 C #11 明示许可放弃, 显示语义不变)。
    fn ex_style_additions(&self) -> u32 {
        0x0008_0000
    }

    /// 渲染设备初始化 (壳在首个 WM_PAINT / 0x401 预绘时调用, R23 懒创建)。
    fn init(&mut self, hwnd: usize, state: &FrameState) -> Result<(), BackendError> {
        self.hwnd = HWND(hwnd as *mut core::ffi::c_void);
        let alpha = skin::window_alpha(state.skin);

        unsafe {
            // 整窗 alpha + 色键 —— 一次调用两 flag 同发 (R28: alpha = round(opacity×255);
            // 活体定案: 分次调用时后一次重置前一次属性 → 必须合并, 见模块注记)
            SetLayeredWindowAttributes(
                self.hwnd,
                COLORREF(config::COLORKEY),
                alpha,
                LWA_ALPHA | LWA_COLORKEY,
            )
            .map_err(|e| {
                BackendError::with_hresult("SetLayeredWindowAttributes(LWA_ALPHA|LWA_COLORKEY) failed", e.code().0 as u32)
            })?;

            // DWM 阴影钩子 (R28, 参考实现 :252-262 同款; 分层窗上大概率无效, 零成本保留)
            if let Ok(enabled) = DwmIsCompositionEnabled() {
                if enabled.as_bool() {
                    let policy = DWMNCRP_ENABLED;
                    let _ = DwmSetWindowAttribute(
                        self.hwnd,
                        DWMWA_NCRENDERING_POLICY,
                        &policy as *const _ as *const core::ffi::c_void,
                        core::mem::size_of::<DWMNCRENDERINGPOLICY>() as u32,
                    );
                    let margins = MARGINS {
                        cxLeftWidth: 1,
                        cxRightWidth: 1,
                        cyTopHeight: 1,
                        cyBottomHeight: 1,
                    };
                    let _ = DwmExtendFrameIntoClientArea(self.hwnd, &margins);
                }
            }
        }

        self.alpha = alpha;
        self.border_px = ((state.skin.border_width * state.dpi / config::DIP_BASE_DPI).round()
            as i32)
            .max(0);
        self.text_color = COLORREF(skin::over(
            state.skin.key_color,
            state.skin.key_opacity,
            state.skin.background_color,
        )
        .as_colorref());
        self.res = Some(Self::create_resources(state)?);
        Ok(())
    }

    /// 全窗口重绘 + ValidateRect (R23)。
    fn paint(&mut self, state: &FrameState) -> Result<(), BackendError> {
        self.draw(state, true)
    }

    /// 0x402 阻塞式淡出 (R15): alpha 自当前值步进到 0, 曲线 = Accelerate-Decelerate
    /// 0.5/0.5 (= smoothstep, 原版断言串参数), 16ms 步进 (原版 50ms 是轮询粒度, 不可观察;
    /// 16ms 保 R15「淡出平滑无跳变」)。期间不取消息 —— 与原版阻塞语义一致, 排队消息顺延。
    fn fade_out(&mut self, duration_secs: f64) {
        let from = self.alpha as f64;
        let dur = if duration_secs.is_finite() && duration_secs > 0.0 {
            Duration::from_secs_f64(duration_secs)
        } else {
            Duration::ZERO
        };
        let start = Instant::now();
        unsafe {
            if !dur.is_zero() {
                loop {
                    let t = start.elapsed().as_secs_f64() / dur.as_secs_f64();
                    if t >= 1.0 {
                        break;
                    }
                    let a = (from * (1.0 - accelerate_decelerate(t)))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    self.lwa(a); // 两 flag 同发 (淡出中色键保持, 带不外露)
                    Sleep(16);
                }
            }
            self.lwa(0); // 终态 0
        }
    }

    /// SW_HIDE 之后由壳调用: alpha 复原 → R10-5「引擎直接 WinShow 亦正常显示」。
    fn on_hidden(&mut self) {
        self.lwa(self.alpha);
    }
}
