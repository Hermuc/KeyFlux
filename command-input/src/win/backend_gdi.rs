//! v1.1 渲染后端 (2026-10-04 样式还原): **逐像素 alpha 自合成** (`UpdateLayeredWindow`)
//! + GDI 绘制内容 + 解析式白边/AA 圆角/高斯阴影。
//!
//! ## 为什么从「整窗 LWA_ALPHA + SetWindowRgn」换到逐像素
//!
//! v1 用 `SetLayeredWindowAttributes(LWA_ALPHA)` 给整窗一个不透明度 ⇒ 白边与内部
//! **必然同 alpha** ⇒ 原版「3px 纯白描边 + 半透明内部 + 框外柔和阴影」根本表达不出来
//! (用户 2026-10-04 报障: 不加白边、不透明度偏高、没有阴影/模糊感)。原版是
//! DirectComposition 自合成 (`WS_EX_NOREDIRECTIONBITMAP`, 见 spec.md:259), 每视觉各带
//! opacity; 这里用 `UpdateLayeredWindow` + 32bpp 预乘 DIB 做它的**等价物**, 合成数学在
//! `crate::compose` (纯逻辑 + 单测锁定): 阴影 → 内容/白边 (同层互不重叠) → 整帧增益。
//!
//! ## 与既有口径的关系 (无列表时**逐像素不变**)
//!
//! * 查询区几何/网格/文字排版/结果列表面板 —— 全部照旧 (GDI 绘制路径未动);
//! * 变的只有**合成**: 白框内的 alpha 由「整窗 0.9」变成「填充净 α (`skin::fill_alpha`)
//!   = 0.945 + 白边 `borderOpacity` = 1.0」(依据 = 原版同背景 A/B, 见 compose 模块头);
//! * `SetWindowRgn` 仍在, 但**扩展到框外阴影带** (区域同时约束合成与命中, 不扩就把阴影
//!   整条裁掉); 框外 42px 带不再是「纯透明」而是「阴影渐变 + 全透明」。
//!
//! ## GDI 与逐像素 alpha 的接口 (踩坑点)
//!
//! GDI 绘制**不写 alpha 通道** (FillRect/DrawText 只动 RGB) ⇒ 不能直接把 GDI 结果当
//! 预乘面用。本后端的做法: GDI 只负责画**内容色** (背景/网格/文字/结果行), alpha 由
//! `compose::composite` 按几何解析式地写入, 并把 RGB 就地预乘 ⇒ 交给
//! `UpdateLayeredWindow` (`AC_SRC_ALPHA`) 即得正确混合。
//!
//! 内部视觉是**半透明**的 (净 α = 0.945, 背景只透出 5.5%) ⇒ 与**真实桌面**逐像素混合。
//! 本后端**不**采样也不模糊背景 (`UpdateLayeredWindow` 的语义就是逐像素 alpha 混合),
//! 原版 (DComp 自合成) 同样不采样背景。真·毛玻璃需要另起 DComp 后端 + 系统 Acrylic/Mica,
//! 在 ULW 路径上叠 `SetWindowCompositionAttribute` 无效 (语义互斥, 见 `compose` 模块头注)。
//!
//! 文字注记 (R23/R28): 查询区显示层**大写化** + 逐字形固定步距 (pitch = 4×网格步距 DIP);
//! 结果行**不**大写化 (`DT_PATH_ELLIPSIS` 保留首尾, 文件名大小写有语义)。

use std::time::{Duration, Instant};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmIsCompositionEnabled, DwmSetWindowAttribute,
    DWMNCRENDERINGPOLICY, DWMNCRP_ENABLED, DWMWA_NCRENDERING_POLICY,
};
use windows::Win32::Graphics::Gdi::{
    AddFontResourceExW, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateRoundRectRgn,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, FillRect, GdiFlush, GetDC, ReleaseDC,
    SelectClipRgn, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, ValidateRect, AC_SRC_ALPHA,
    AC_SRC_OVER, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CENTER, DT_NOPREFIX,
    DT_PATH_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, FR_PRIVATE, FW_BOLD, HBITMAP, HBRUSH, HDC, HFONT,
    HGDIOBJ, HRGN, OUT_DEFAULT_PRECIS, RGBQUAD, TRANSPARENT,
};
use windows::Win32::System::Threading::Sleep;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    CharUpperW, GetWindowRect, UpdateLayeredWindow, ULW_ALPHA,
};

use crate::compose;
use crate::config;
use crate::easing::accelerate_decelerate;
use crate::geometry;
use crate::render::{BackendError, FrameState, RenderBackend};
use crate::skin::{self, Rgb};
use crate::win::resources;

/// 结果行文本格式: 单行 + 垂直居中 + 路径省略 (保留首尾) + 不解析 & 前缀。
/// `DT_LEFT` = 0, 无需显式或入。
const LIST_TEXT_FORMAT: DRAW_TEXT_FORMAT =
    DRAW_TEXT_FORMAT(DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0 | DT_PATH_ELLIPSIS.0);

/// 常规字重 (FW_NORMAL = 400; 查询区用 FW_BOLD 与其分层)。
const FW_NORMAL: i32 = 400;

impl Default for GdiBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// 32bpp 顶层 DIB (自合成面): 位即 UpdateLayeredWindow 的源。
struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    len: usize,
}

impl Dib {
    fn new(w: i32, h: i32) -> Result<Dib, BackendError> {
        // 屏幕 DC 只作创建参数 (格式/色板用), 用完立即释放 (否则每次 resize 漏一个 DC)
        let screen = unsafe { GetDC(None) };
        let dc = unsafe { CreateCompatibleDC(Some(screen)) };
        if dc.0.is_null() {
            unsafe {
                let _ = ReleaseDC(None, screen);
            }
            return Err(BackendError::new("CreateCompatibleDC failed"));
        }
        let hdr = BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // 负 = top-down (行序与 x/y 索引一致)
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let info = BITMAPINFO {
            bmiHeader: hdr,
            bmiColors: [RGBQUAD::default(); 1],
        };
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let bmp =
            unsafe { CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .map_err(|e| {
                    BackendError::with_hresult("CreateDIBSection failed", e.code().0 as u32)
                })?;
        unsafe {
            let _ = ReleaseDC(None, screen);
        }
        if bits.is_null() {
            unsafe {
                let _ = DeleteDC(dc);
            }
            return Err(BackendError::new("CreateDIBSection returned null bits"));
        }
        let old = unsafe { SelectObject(dc, HGDIOBJ(bmp.0)) };
        Ok(Dib {
            dc,
            bmp,
            old,
            bits: bits as *mut u8,
            len: (w as usize) * (h as usize) * 4,
        })
    }

    /// 供合成器读写的字节切片 (调用前必须先 `flush`, GDI 批次未必已落到位内存)。
    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.bits, self.len) }
    }

    fn flush(&self) {
        unsafe {
            let _ = GdiFlush();
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

/// 懒创建的 GDI 资源 (R23: 首次 WM_PAINT 才创建) + 自合成面与合成计划。
struct Resources {
    font: HFONT,
    /// 结果行字体 (常规字重, 小字号)
    list_font: HFONT,
    bg_brush: HBRUSH,     // 背景刷 (backgroundColor)
    grid_brush: HBRUSH,   // 网格刷 (grid_content_color 反解色)
    shadow_brush: HBRUSH, // 框外带底色 = 阴影色 (预乘后只留 alpha)
    select_brush: HBRUSH, // 结果行选中底色 (skin::list_select_color)
    sep_brush: HBRUSH,    // 查询区/结果区分隔线 (skin::list_separator_color)
    accent_brush: HBRUSH, // 选中行左侧强调条 (skin::list_accent_color)
    scroll_brush: HBRUSH, // 滚动条滑块 (skin::list_scroll_color)
    clip: HRGN,           // 白框圆角区域 (GDI 绘制裁剪; 与窗口区域分开, 见 hit_region)
    dib: Dib,             // 自合成面 (UpdateLayeredWindow 源)
    plan: compose::Plan,  // 逐像素合成计划 (几何决定, 尺寸/皮肤变化时重建)
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            for h in [
                HGDIOBJ(self.font.0),
                HGDIOBJ(self.list_font.0),
                HGDIOBJ(self.bg_brush.0),
                HGDIOBJ(self.grid_brush.0),
                HGDIOBJ(self.shadow_brush.0),
                HGDIOBJ(self.select_brush.0),
                HGDIOBJ(self.sep_brush.0),
                HGDIOBJ(self.accent_brush.0),
                HGDIOBJ(self.scroll_brush.0),
                HGDIOBJ(self.clip.0),
            ] {
                let _ = DeleteObject(h);
            }
        }
    }
}

/// v1.1 GDI + 逐像素 alpha 后端。
pub struct GdiBackend {
    hwnd: HWND,
    /// 整帧不透明度增益 (0..255): 静息 255; 淡出时步进到 0 (对应原版 DComp 属性动画)
    gain: u8,
    /// 框内文字色 COLORREF (keyColor@keyOpacity 向背景混色)
    text_color: COLORREF,
    /// 结果行文字色 COLORREF (当前与查询区同色, 独立字段便于后续分层调优)
    list_text_color: COLORREF,
    res: Option<Resources>,
    /// 满不透明度时的**预乘**帧缓存 (淡出时逐帧缩放它, 避免重绘 GDI)
    frame_full: Vec<u8>,
}

impl GdiBackend {
    pub fn new() -> Self {
        Self {
            hwnd: HWND::default(),
            gain: 255,
            text_color: COLORREF(0),
            list_text_color: COLORREF(0),
            res: None,
            frame_full: Vec::new(),
        }
    }

    /// 白框圆角区域 (窗口坐标; GDI 绘制裁剪用 —— 框外带留给阴影)。
    fn frame_region(w: i32, h: i32, inset: i32, radius: f64) -> HRGN {
        let rr = (radius.round() as i32).max(0);
        unsafe { CreateRoundRectRgn(inset, inset, w - inset, h - inset, 2 * rr, 2 * rr) }
    }

    /// 命中测试区域 = 白框 + 框外阴影带 (区域同时约束合成与命中: 不扩展就把阴影裁掉)。
    fn hit_region(w: i32, h: i32, inset: i32, radius: f64, ext: i32) -> HRGN {
        let rr = (radius.round() as i32).max(0) + ext;
        unsafe {
            CreateRoundRectRgn(
                (inset - ext).max(0),
                (inset - ext).max(0),
                w - inset + ext,
                h - inset + ext,
                2 * rr,
                2 * rr,
            )
        }
    }

    /// 逐像素合成计划 (几何 + 皮肤 → 解析式 alpha/RGB)。
    fn build_plan(state: &FrameState) -> compose::Plan {
        let s = state.skin;
        let dpi = state.dpi;
        compose::Plan {
            w: state.width_px,
            h: state.height_px,
            frame: geometry::frame_shape(s, dpi, state.width_px, state.height_px),
            ring_px: geometry::ring_width_px(s, dpi),
            ring_rgb: s.border_color,
            ring_alpha: skin::ring_alpha(s),
            fill_alpha: skin::fill_alpha(s),
            shadow: compose::Shadow {
                color: s.window_shadow_color,
                opacity: skin::shadow_peak(s),
                sigma: geometry::shadow_sigma_px(s, dpi),
                dy: geometry::shadow_dy_px(dpi),
            },
        }
    }

    fn create_resources(state: &FrameState) -> Result<Resources, BackendError> {
        let dpi = state.dpi;
        let s = state.skin;
        let exe_dir = resources::exe_dir();

        // 字体 (R23/R25): FR_PRIVATE = 仅本进程可见, 直接 CreateFontW 按族名解析可用
        let font_ttf = resources::font_file(&exe_dir);
        let path_w = super::wide(&font_ttf.display().to_string());
        let loaded =
            unsafe { AddFontResourceExW(PCWSTR::from_raw(path_w.as_ptr()), FR_PRIVATE, None) };
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

        let face = super::wide(config::FONT_FAMILY);
        let make_font = |height_px: i32, weight: i32| -> Result<HFONT, BackendError> {
            let f = unsafe {
                CreateFontW(
                    -height_px,
                    0,
                    0,
                    0,
                    weight,
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
            if f.0.is_null() {
                Err(BackendError::new("CreateFontW failed"))
            } else {
                Ok(f)
            }
        };
        // 查询区: 粗体 44.0 DIP → -55px @125% (负值 = 字符高度; R23)
        let font = make_font(geometry::font_height_px(dpi), FW_BOLD.0 as i32)?;
        // 结果行: 常规字重 17.0 DIP → -21px @125% (2026-10-04 列表)
        let list_font = make_font(geometry::list_font_px(dpi), FW_NORMAL)?;

        // 网格内容色 (反解) 与结果区派生色 (R28; 见 skin 模块注)
        let grid = skin::grid_content_color(s);

        let brush = |c: Rgb| -> Result<HBRUSH, BackendError> {
            let b = unsafe { CreateSolidBrush(COLORREF(c.as_colorref())) };
            if b.0.is_null() {
                Err(BackendError::new("CreateSolidBrush failed"))
            } else {
                Ok(b)
            }
        };

        let bg_brush = brush(skin::panel_content_color(s))?;
        let grid_brush = brush(grid)?;
        let shadow_brush = brush(s.window_shadow_color)?;
        // 结果列表面板配色 (由既有皮肤键派生, 见 skin::list_*)
        let select_brush = brush(skin::list_select_color(s))?;
        let sep_brush = brush(skin::list_separator_color(s))?;
        let accent_brush = brush(skin::list_accent_color(s))?;
        let scroll_brush = brush(skin::list_scroll_color(s))?;

        // 自合成面 + 计划
        let dib = Dib::new(state.width_px, state.height_px)?;
        let plan = Self::build_plan(state);
        let clip = Self::frame_region(
            state.width_px,
            state.height_px,
            geometry::band_inset_px(dpi),
            geometry::corner_radius_px(s, dpi),
        );
        if clip.0.is_null() {
            return Err(BackendError::new("CreateRoundRectRgn failed"));
        }

        Ok(Resources {
            font,
            list_font,
            bg_brush,
            grid_brush,
            shadow_brush,
            select_brush,
            sep_brush,
            accent_brush,
            scroll_brush,
            clip,
            dib,
            plan,
        })
    }

    /// 呈现 (唯一 `UpdateLayeredWindow` 出口): 位置/尺寸不变, 只换内容。
    /// 预乘 + `AC_SRC_ALPHA` ⇒ 逐像素 alpha 生效 (白边/填充/阴影各按自己的 alpha)。
    fn present(&self) -> Result<(), BackendError> {
        let Some(res) = self.res.as_ref() else {
            return Err(BackendError::new("GDI backend not initialized"));
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // 显式给出目标矩形 (与窗口当前矩形同值): 实测 NULL/NULL 组合在本机不产生可见像素,
        // 与隔离探针 (ulw_probe.py, pptdst/psize 都显式传) 的唯一差别就是这里。
        let mut wr = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut wr);
        }
        let dst = POINT {
            x: wr.left,
            y: wr.top,
        };
        let size = SIZE {
            cx: wr.right - wr.left,
            cy: wr.bottom - wr.top,
        };
        let r = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&dst),
                Some(&size),
                Some(res.dib.dc),
                Some(&POINT { x: 0, y: 0 }), // pptSrc: 层的原点
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        r.map_err(|e| BackendError::with_hresult("UpdateLayeredWindow failed", e.code().0 as u32))
    }

    /// 合成 + 缓存满帧 + 呈现 (`gain` = 1.0 时同时刷新 `frame_full`)。
    fn compose_and_present(&mut self, keep_full: bool) -> Result<(), BackendError> {
        let Some(res) = self.res.as_mut() else {
            return Err(BackendError::new("GDI backend not initialized"));
        };
        res.dib.flush(); // GDI 批次必须落到位内存后才能被合成器读
        let plan = res.plan.clone();
        compose::composite(res.dib.bytes_mut(), &plan, 1.0);
        if keep_full {
            self.frame_full = res.dib.bytes_mut().to_vec();
        }
        self.gain = 255;
        self.present()
    }

    /// 一次完整重绘 (GDI → 逐像素合成 → 呈现)。`validate = true` 时结尾 ValidateRect
    /// (R23: WM_PAINT 处理尾)。
    fn draw(&mut self, state: &FrameState, validate: bool) -> Result<(), BackendError> {
        let dpi = state.dpi;
        let inset = geometry::band_inset_px(dpi);
        // 查询区/结果区分界 = 白框内查询区下沿 (无列表时 == frame.bottom, 口径与旧实现等价)
        let query_bottom = geometry::query_bottom_px(state.base_height_px, dpi);
        unsafe {
            let hdc = {
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                res.dib.dc
            };
            // 1) 全客户区先铺**阴影色** (框外带的底色 = 阴影色; 合成器按阴影 alpha 预乘后,
            //    带内只剩黑 + alpha ⇒ 桌面被压暗 = 阴影)
            {
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                let whole = RECT {
                    left: 0,
                    top: 0,
                    right: state.width_px,
                    bottom: state.height_px,
                };
                FillRect(hdc, &whole, res.shadow_brush);
            }

            // 裁剪到白框圆角区域 (GDI 只画框内; 框外带留给解析式阴影)
            {
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                SelectClipRgn(hdc, Some(res.clip));
            }

            // 2) 白框矩形填 backgroundColor (净观感由 compose 的 fill_alpha 决定)
            let frame = RECT {
                left: inset,
                top: inset,
                right: (state.width_px - inset).max(inset),
                bottom: (state.height_px - inset).max(inset),
            };
            {
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                FillRect(hdc, &frame, res.bg_brush);
            }
            // 3) 白边不再由 GDI 画: 它必须是**不透明**色带 (borderOpacity), 而 GDI 画不出
            //    逐像素 alpha —— 由 compose 按几何解析式生成 (含 AA 内外沿)。

            // 4) 网格 (R28): 横竖双向 1px, 首线偏移 = 间距−1。只覆盖**查询区**。
            let fw = frame.right - frame.left;
            let qh = (query_bottom - frame.top).max(0);
            let step = geometry::grid_step_px(dpi);
            let off = geometry::grid_first_offset(step);
            {
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                let mut x = off;
                while x < fw - 2 {
                    let line = RECT {
                        left: frame.left + x,
                        top: frame.top + 2,
                        right: frame.left + x + 1,
                        bottom: frame.top + 2 + (qh - 4).max(0),
                    };
                    FillRect(hdc, &line, res.grid_brush);
                    x += step;
                }
                let mut y = off;
                while y < qh - 2 {
                    let line = RECT {
                        left: frame.left + 2,
                        top: frame.top + y,
                        right: frame.left + 2 + (fw - 4),
                        bottom: frame.top + y + 1,
                    };
                    FillRect(hdc, &line, res.grid_brush);
                    y += step;
                }
            }

            // 5) 查询文字 (R23 + 原版排版活体定案): 显示层大写化 + 逐字形固定步距布局。
            let query_rect = RECT {
                left: frame.left,
                top: frame.top,
                right: frame.right,
                bottom: query_bottom.max(frame.top),
            };
            {
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                self.draw_query_text(hdc, state, res, &query_rect);
            }

            // 6) 结果列表面板 (0x406 推进来的行; 空表时整段不绘制 ⇒ 无列表 = 基准观感)
            if !state.results.is_empty() {
                let sep = geometry::list_separator_px(dpi);
                let line = RECT {
                    left: frame.left,
                    top: query_bottom,
                    right: frame.right,
                    bottom: query_bottom + sep,
                };
                {
                    let Some(res) = self.res.as_ref() else {
                        return Err(BackendError::new("GDI backend not initialized"));
                    };
                    FillRect(hdc, &line, res.sep_brush);
                }
                let Some(res) = self.res.as_ref() else {
                    return Err(BackendError::new("GDI backend not initialized"));
                };
                self.draw_results(hdc, state, res, inset, query_bottom + sep);
            }

            SelectClipRgn(hdc, None);
        }

        // 7) 逐像素合成 (alpha 掩码 + 预乘) → 呈现
        self.compose_and_present(true)?;

        if validate {
            unsafe {
                let _ = ValidateRect(Some(self.hwnd), None); // R23: 处理尾 ValidateRect
            }
        }
        Ok(())
    }

    /// 查询区文字: 显示层大写化 + 逐字形固定步距 (pitch = 4×网格步距 DIP) 水平居中于
    /// 各单元格、整串以查询区中心为轴、垂直居中 (原版活体排版定案)。
    fn draw_query_text(&self, hdc: HDC, state: &FrameState, res: &Resources, rect: &RECT) {
        let mut units: Vec<u16> = state.text.to_vec();
        if units.is_empty() {
            return;
        }
        let fw = rect.right - rect.left;
        unsafe {
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
            let pitch = geometry::text_pitch_px(state.dpi);
            let n = cells.len() as i32;
            let center_x = rect.left + fw / 2;
            for (idx, &(start, len)) in cells.iter().enumerate() {
                let cell_left = center_x - (n * pitch) / 2 + idx as i32 * pitch;
                let mut tr = RECT {
                    left: cell_left,
                    top: rect.top,
                    right: cell_left + pitch,
                    bottom: rect.bottom,
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
    }

    /// 结果行绘制: 选中底色 + 左侧强调条 + 路径省略文本 + 超一屏时的滚动条。
    fn draw_results(
        &self,
        hdc: HDC,
        state: &FrameState,
        res: &Resources,
        inset: i32,
        list_top: i32,
    ) {
        let dpi = state.dpi;
        let row_h = geometry::list_row_h_px(dpi);
        let pad = geometry::list_text_pad_px(dpi);
        let accent_w = geometry::list_accent_px(dpi);
        let sb_w = geometry::list_scrollbar_px(dpi);
        let sb_m = geometry::list_scrollbar_margin_px(dpi);
        let bw = geometry::ring_width_px(state.skin, dpi).round() as i32;
        let inner_l = inset + bw;
        let inner_r = state.width_px - inset - bw;
        let total = state.results.len();
        let vis = state.results.visible_rows();
        let has_bar = total > vis && vis > 0;
        let text_right = inner_r - pad - if has_bar { sb_w + sb_m } else { 0 };
        let (start, end) = state.results.window();
        let sel = state.results.selected();

        unsafe {
            let old_font = SelectObject(hdc, HGDIOBJ(res.list_font.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, self.list_text_color);
            for (i, ri) in (start..end).enumerate() {
                let top = list_top + i as i32 * row_h;
                let row = RECT {
                    left: inner_l,
                    top,
                    right: inner_r.max(inner_l),
                    bottom: top + row_h,
                };
                if ri as i32 == sel {
                    FillRect(hdc, &row, res.select_brush);
                    // 强调条上下各留 4px 呼吸 (贴满整行会显得笨重)
                    let acc = RECT {
                        left: inner_l,
                        top: top + 4,
                        right: inner_l + accent_w,
                        bottom: (top + row_h - 4).max(top + 5),
                    };
                    FillRect(hdc, &acc, res.accent_brush);
                }
                let Some(text) = state.results.item(ri) else {
                    continue;
                };
                let mut u: Vec<u16> = text.encode_utf16().collect();
                if u.is_empty() {
                    continue;
                }
                let mut tr = RECT {
                    left: inner_l + pad,
                    top,
                    right: text_right.max(inner_l + pad + 1),
                    bottom: top + row_h,
                };
                // 🔴 结果行**不**做显示层大写化 (那是查询区/原版观感的口径);
                //    文件名大小写有语义, 不得改写。
                DrawTextW(hdc, &mut u, &mut tr, LIST_TEXT_FORMAT);
            }
            SelectObject(hdc, old_font);

            // 滚动条: 细滑块, 位置 = scroll / (total − vis) 比例。
            if has_bar {
                let track_top = list_top + 4;
                let track_bot = (state.height_px - inset - 4).max(track_top + 1);
                let track_h = track_bot - track_top;
                let thumb_h = (((track_h as f64) * (vis as f64) / (total as f64)).round() as i32)
                    .clamp(16, track_h);
                let denom = (total - vis).max(1) as f64;
                let off = (((track_h - thumb_h) as f64) * (state.results.scroll() as f64 / denom))
                    .round() as i32;
                let right = inner_r - sb_m;
                let bar = RECT {
                    left: (right - sb_w).max(inner_l),
                    top: track_top + off,
                    right: right.max(inner_l + 1),
                    bottom: track_top + off + thumb_h,
                };
                FillRect(hdc, &bar, res.scroll_brush);
            }
        }
    }
}

impl RenderBackend for GdiBackend {
    /// 追加位 = WS_EX_LAYERED (0x0008_0000) → 实际 ex-style 0x0808_0008
    /// (NOREDIRECTIONBITMAP 按 spec 附录 C #11 明示许可放弃; 逐像素 alpha 由
    /// `UpdateLayeredWindow` 自合成提供, 白边/填充/阴影各自独立 alpha)。
    fn ex_style_additions(&self) -> u32 {
        0x0008_0000
    }

    /// 渲染设备初始化 (壳在首个 WM_PAINT / 0x401 预绘时调用, R23 懒创建)。
    fn init(&mut self, hwnd: usize, state: &FrameState) -> Result<(), BackendError> {
        self.hwnd = HWND(hwnd as *mut core::ffi::c_void);
        self.gain = 255;

        // 🔴 不再调用 SetLayeredWindowAttributes: 一旦用过它, UpdateLayeredWindow 会失效
        //    (两者互斥), 而逐像素 alpha 正是本次样式还原的机制前提。
        unsafe {
            // DWM 阴影钩子 (R28, 参考实现同款; 逐像素自合成窗上大概率无效, 零成本保留)
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

        // 文字色 = keyColor @keyOpacity 向**面板内容色**混 (不是皮肤原色: 面板底色已被
        // `panel_content_color` 折到内容色空间, 混色基准必须同在内容色空间)。
        let text_col = skin::over(
            state.skin.key_color,
            state.skin.key_opacity,
            skin::panel_content_color(state.skin),
        )
        .as_colorref();
        self.text_color = COLORREF(text_col);
        self.list_text_color = COLORREF(text_col);
        let res = Self::create_resources(state)?;
        let plan = res.plan.clone();
        self.res = Some(res);

        // 形状: 白框 + 框外阴影带 (区域同时约束合成与命中, 不扩就把阴影裁掉)
        self.apply_hit_region(&plan);
        Ok(())
    }

    /// 全窗口重绘 + ValidateRect (R23)。
    fn paint(&mut self, state: &FrameState) -> Result<(), BackendError> {
        self.draw(state, true)
    }

    /// 0x401 的同步预绘 (防首帧白闪): 逐像素合成路径必须先呈现, 否则窗口显示时无内容。
    fn pre_show(&mut self, state: &FrameState) -> Result<(), BackendError> {
        self.draw(state, false)
    }

    /// 结果列表展开/收起: 尺寸变了 ⇒ DIB / 合成计划 / 区域全部重建
    /// (区域与自合成面都是**窗口坐标**, 不重建则新增的下半部分被裁掉或未绘制)。
    fn resize(&mut self, state: &FrameState) -> Result<(), BackendError> {
        let dpi = state.dpi;
        let s = state.skin;
        let plan = Self::build_plan(state);
        if let Some(res) = self.res.as_mut() {
            // 新面 (旧 Dib 在赋值时 Drop: 解绑 + 删对象)
            res.dib = Dib::new(state.width_px, state.height_px)?;
            res.plan = plan.clone();
            let old = res.clip;
            let new = Self::frame_region(
                state.width_px,
                state.height_px,
                geometry::band_inset_px(dpi),
                geometry::corner_radius_px(s, dpi),
            );
            if !new.0.is_null() {
                if !old.0.is_null() {
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(old.0));
                    }
                }
                res.clip = new;
            }
        }
        self.frame_full.clear();
        // 背景层不必重抓: 会话初抓的那张已按「基准高 + 全展开」一次抓够, 顶部不动 ⇒ 裁剪复用
        self.apply_hit_region(&plan);
        Ok(())
    }

    /// 0x402 阻塞式淡出 (R15): 增益 255→0 步进, 曲线 = Accelerate-Decelerate 0.5/0.5
    /// (= smoothstep, 原版断言串参数), 16ms 步进。逐帧缩放**预乘帧缓存** ⇒ 与整窗 alpha
    /// 步进等价而无重绘成本; 期间不取消息 (与原版消息排队语义一致)。
    fn fade_out(&mut self, duration_secs: f64) {
        if self.frame_full.is_empty() {
            // 从未呈现过 (理论上不可达): 直接置 0
            self.gain = 0;
            return;
        }
        let from = self.gain as f64;
        let dur = if duration_secs.is_finite() && duration_secs > 0.0 {
            Duration::from_secs_f64(duration_secs)
        } else {
            Duration::ZERO
        };
        let start = Instant::now();
        if !dur.is_zero() {
            loop {
                let t = start.elapsed().as_secs_f64() / dur.as_secs_f64();
                if t >= 1.0 {
                    break;
                }
                let g = from * (1.0 - accelerate_decelerate(t));
                self.blit_gain(g);
                unsafe {
                    Sleep(16);
                }
            }
        }
        self.blit_gain(0.0);
    }

    /// SW_HIDE 之后由壳调用: 增益复原 → R10-5「引擎直接 WinShow 亦正常显示」。
    fn on_hidden(&mut self) {
        self.blit_gain(1.0);
    }
}

impl GdiBackend {
    /// 把缓存的满帧按增益整帧缩放后呈现 (淡出/复原共用)。
    fn blit_gain(&mut self, gain01: f64) {
        let Some(res) = self.res.as_mut() else {
            return;
        };
        if self.frame_full.len() != res.dib.len {
            return;
        }
        let full = self.frame_full.clone();
        compose::scale_frame(&full, res.dib.bytes_mut(), gain01);
        self.gain = (gain01 * 255.0).round().clamp(0.0, 255.0) as u8;
        let _ = self.present();
    }

    /// 命中测试区域 (白框 + 阴影带) 施加到窗口。
    fn apply_hit_region(&self, plan: &compose::Plan) {
        let ext = plan.shadow_extent_px();
        // 白框左上角像素坐标即 band 内缩量 (compose::Plan 由 geometry::frame_shape 构造)
        let inset = plan.frame.l.round() as i32;
        let rgn = Self::hit_region(plan.w, plan.h, inset, plan.frame.radius, ext);
        if !rgn.0.is_null() {
            unsafe {
                let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            }
        }
    }
}
