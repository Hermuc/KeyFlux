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
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CENTER,
    DT_END_ELLIPSIS, DT_NOPREFIX, DT_PATH_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, FR_PRIVATE, FW_BOLD,
    HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ, HRGN, OUT_DEFAULT_PRECIS, RGBQUAD, TRANSPARENT,
};
use windows::Win32::System::Threading::Sleep;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    CharUpperW, DrawIconEx, GetWindowRect, UpdateLayeredWindow, DI_NORMAL, ULW_ALPHA,
};

use super::shell_icon::IconCache;
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

/// 结果行**标题** (文件名) 格式: 尾部省略 (文件名不做路径省略 —— 首段丢了不可读)。
const LIST_TITLE_FORMAT: DRAW_TEXT_FORMAT =
    DRAW_TEXT_FORMAT(DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0 | DT_END_ELLIPSIS.0);

/// 结果行**副标题** (路径) 格式: 路径省略 (保留首尾, 同 Flow Launcher SubTitle 的
/// `TextTrimming="CharacterEllipsis"` + 完整路径语义)。
const LIST_SUB_FORMAT: DRAW_TEXT_FORMAT =
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
        // SAFETY: GetDC(None) 请求整个屏幕的 DC, 无窗口句柄前置条件。
        let screen = unsafe { GetDC(None) };
        // SAFETY: screen 是上一行取得且尚未释放的合法屏幕 DC。
        let dc = unsafe { CreateCompatibleDC(Some(screen)) };
        if dc.0.is_null() {
            // SAFETY: screen 与 GetDC(None) 配对 (NULL 窗口); 此处提前释放避免泄漏。
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
            // SAFETY: screen 有效; &info/&mut bits 均为本地变量, 生命周期覆盖调用; DIB_RGB_COLORS 与 32bpp 匹配。
            unsafe { CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .map_err(|e| {
                    BackendError::with_hresult("CreateDIBSection failed", e.code().0 as u32)
                })?;
        // SAFETY: screen 与 GetDC(None) 配对, 此处用完最后释放一次。
        unsafe {
            let _ = ReleaseDC(None, screen);
        }
        if bits.is_null() {
            // SAFETY: dc 由本次 CreateCompatibleDC 成功创建且非空, 只在此销毁一次。
            unsafe {
                let _ = DeleteDC(dc);
            }
            return Err(BackendError::new("CreateDIBSection returned null bits"));
        }
        // SAFETY: dc 与 bmp 均为本函数新建的有效 GDI 对象; 返回值是被替换的旧位图对象。
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
        // SAFETY: bits 指向 CreateDIBSection 分配的面, len = w*h*4 即其字节数; &mut self 保证独占。
        unsafe { core::slice::from_raw_parts_mut(self.bits, self.len) }
    }

    /// 只读字节切片 (合成器从内容面读、写呈现面时用; 同样先 `flush`)。
    fn bytes(&self) -> &[u8] {
        // SAFETY: 同 bytes_mut: bits 指向存活的内容面, len 为其实际字节数; &self 不改写。
        unsafe { core::slice::from_raw_parts(self.bits, self.len) }
    }

    fn flush(&self) {
        // SAFETY: GdiFlush 无参数, 刷新当前线程 GDI 批次, 无前置条件。
        unsafe {
            let _ = GdiFlush();
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: dc/bmp 由构造保证有效且本结构独占; old 是创建时被替换的原始对象; 三者仅在此释放一次。
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
    /// 结果行**标题**字体 (常规字重, Flow 版式上行)
    list_title_font: HFONT,
    /// 结果行**副标题**字体 (常规字重, 小一号灰)
    list_sub_font: HFONT,
    bg_brush: HBRUSH,     // 背景刷 (backgroundColor)
    grid_brush: HBRUSH,   // 网格刷 (grid_content_color 反解色)
    shadow_brush: HBRUSH, // 框外带底色 = 阴影色 (预乘后只留 alpha)
    select_brush: HBRUSH, // 结果行选中底色 (skin::list_select_color)
    sep_brush: HBRUSH,    // 查询区/结果区分隔线 (skin::list_separator_color)
    accent_brush: HBRUSH, // 选中行左侧强调条 (skin::list_accent_color)
    scroll_brush: HBRUSH, // 滚动条滑块 (skin::list_scroll_color)
    clip: HRGN,           // 白框圆角区域 (GDI 绘制裁剪; 与窗口区域分开, 见 hit_region)
    content: Dib,         // 内容面 (GDI 画原始内容色; 持久保留, 不被合成破坏)
    layer: Dib,           // 呈现面 (合成后的预乘帧 = UpdateLayeredWindow 源)
    plan: compose::Plan,  // 逐像素合成计划 (几何决定, 尺寸/皮肤变化时重建)
}

impl Drop for Resources {
    fn drop(&mut self) {
        // SAFETY: 列表中全部句柄由 create_resources 创建并被本结构独占, 仅在此 delete 一次。
        unsafe {
            for h in [
                HGDIOBJ(self.font.0),
                HGDIOBJ(self.list_title_font.0),
                HGDIOBJ(self.list_sub_font.0),
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
    /// 结果行副标题 (路径) 色 COLORREF (主文字色向面板内容色混 45%, Flow 灰)
    list_sub_color: COLORREF,
    /// 系统文件图标缓存 (按路径; 见 `shell_icon` 模块解耦注)
    icons: IconCache,
    res: Option<Resources>,
    /// 满不透明度时的**预乘**帧缓存 (淡出时逐帧缩放它, 避免重绘 GDI)
    frame_full: Vec<u8>,
    /// 内容面是否持有当前帧的内容色 (2026-10-06 双面化): 全量 draw 置真,
    /// resize (DIB 重建) 置假 —— 增量行重绘 (`repaint_rows`) 的前置守卫,
    /// 假时回退全量重绘 (语义等价, 只慢不错)。
    content_valid: bool,
}

impl GdiBackend {
    pub fn new() -> Self {
        Self {
            hwnd: HWND::default(),
            gain: 255,
            text_color: COLORREF(0),
            list_text_color: COLORREF(0),
            list_sub_color: COLORREF(0),
            icons: IconCache::new(),
            res: None,
            frame_full: Vec::new(),
            content_valid: false,
        }
    }

    /// 白框圆角区域 (窗口坐标; GDI 绘制裁剪用 —— 框外带留给阴影)。
    fn frame_region(w: i32, h: i32, inset: i32, radius: f64) -> HRGN {
        let rr = (radius.round() as i32).max(0);
        // SAFETY: CreateRoundRectRgn 只接受坐标/半径整型参数, 返回的新区域由调用方接管。
        unsafe { CreateRoundRectRgn(inset, inset, w - inset, h - inset, 2 * rr, 2 * rr) }
    }

    /// 命中测试区域 = 白框 + 框外阴影带 (区域同时约束合成与命中: 不扩展就把阴影裁掉)。
    fn hit_region(w: i32, h: i32, inset: i32, radius: f64, ext: i32) -> HRGN {
        let rr = (radius.round() as i32).max(0) + ext;
        // SAFETY: 同上; 坐标已 clamp 到非负, 半径 rr ≥ 0, 无非法参数。
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
            // 搜索徽标 (2026-10-04): 插件经 0x40A 指定字形, 命令框只认编号; 颜色由
            // 既有皮肤键派生 (同网格内容色空间, 见 skin::badge_color)。
            badge: state.badge.and_then(|glyph| {
                if !crate::badge::is_known(glyph) {
                    return None;
                }
                let size = geometry::badge_size_px(dpi) as f64;
                let (l, t) = geometry::badge_origin_px(state.width_px, state.base_height_px, dpi);
                Some(compose::BadgeLayer {
                    geom: crate::badge::BadgePaint::magnifier(
                        l as f64,
                        t as f64,
                        size,
                        geometry::badge_stroke_px(dpi),
                    ),
                    rgb: skin::badge_color(s),
                })
            }),
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
            // SAFETY: path_w 是 super::wide 生成的 NUL 结尾 UTF-16 且存活至调用结束; FR_PRIVATE 仅本进程可见。
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
        // SAFETY: bold_w 同上为 NUL 结尾 UTF-16 且在本语句内存活; 失败仅降级为无粗体面。
        unsafe { AddFontResourceExW(PCWSTR::from_raw(bold_w.as_ptr()), FR_PRIVATE, None) }; // 真粗体面 best effort

        let face = super::wide(config::FONT_FAMILY);
        let make_font = |height_px: i32, weight: i32| -> Result<HFONT, BackendError> {
            // SAFETY: face 是 NUL 结尾 UTF-16 (闭包捕获, 生命周期覆盖调用); 其余参数均为整型字面量。
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
        // 结果行标题: 常规字重 14.0 DIP → -18px @125% (Flow Launcher 标定)
        let list_title_font = make_font(geometry::list_title_font_px(dpi), FW_NORMAL)?;
        // 结果行副标题: 常规字重 11.0 DIP → -14px @125%
        let list_sub_font = make_font(geometry::list_sub_font_px(dpi), FW_NORMAL)?;

        // 网格内容色 (反解) 与结果区派生色 (R28; 见 skin 模块注)
        let grid = skin::grid_content_color(s);

        let brush = |c: Rgb| -> Result<HBRUSH, BackendError> {
            // SAFETY: CreateSolidBrush 只接受 COLORREF 值, 无指针前置条件。
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
        let content = Dib::new(state.width_px, state.height_px)?;
        let layer = Dib::new(state.width_px, state.height_px)?;
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
            list_title_font,
            list_sub_font,
            bg_brush,
            grid_brush,
            shadow_brush,
            select_brush,
            sep_brush,
            accent_brush,
            scroll_brush,
            clip,
            content,
            layer,
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
        // SAFETY: self.hwnd 由 init 存入的合法窗口句柄; &mut wr 是本地 RECT, 生命周期覆盖调用。
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
        // SAFETY: self.hwnd 有效且已置 WS_EX_LAYERED; dst/size/pptSrc/blend 均指向本地值且存活至调用; res.layer.dc 是有效的 DIB 呈现面。
        let r = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&dst),
                Some(&size),
                Some(res.layer.dc),
                Some(&POINT { x: 0, y: 0 }), // pptSrc: 层的原点
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        r.map_err(|e| BackendError::with_hresult("UpdateLayeredWindow failed", e.code().0 as u32))
    }

    /// 全帧合成 + 缓存满帧 + 呈现 (2026-10-06 双面化): 内容面 (GDI 原始内容色,
    /// **持久保留**) → 拷入呈现面 → 就地合成 → `UpdateLayeredWindow`。
    /// 内容面不再被合成破坏是增量行重绘 (`repaint_rows`) 的机制前提。
    fn compose_and_present(&mut self) -> Result<(), BackendError> {
        {
            let Some(res) = self.res.as_mut() else {
                return Err(BackendError::new("GDI backend not initialized"));
            };
            res.content.flush(); // GDI 批次必须落到位内存后才能被合成器读
            let plan = res.plan.clone();
            let layer = res.layer.bytes_mut();
            layer.copy_from_slice(res.content.bytes());
            compose::composite(layer, &plan, 1.0);
            // 满不透明度帧缓存 (淡出/复原的源) 跟随本次呈现 (合成是纯内存写,
            // 读回无需 GdiFlush —— flush 留给 present 前的最后一次保险)
            self.frame_full.clear();
            self.frame_full.extend_from_slice(layer);
        }
        if let Some(res) = self.res.as_ref() {
            res.layer.flush();
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
        // 合成计划与状态同步 (徽标显隐不改尺寸/资源, 只需重算计划 —— 纯浮点几何, 逐帧可负担;
        // 此前计划仅在 init/resize 重建, 徽标走 0x40A/0x40B 时不会触发 resize)
        if let Some(res) = self.res.as_mut() {
            res.plan = Self::build_plan(state);
        }
        let Some(res) = self.res.as_ref() else {
            return Err(BackendError::new("GDI backend not initialized"));
        };
        let hdc = res.content.dc;
        // 白框矩形 = 后续各步骤共用的几何基准
        let frame = RECT {
            left: inset,
            top: inset,
            right: (state.width_px - inset).max(inset),
            bottom: (state.height_px - inset).max(inset),
        };

        // SAFETY: hdc 取自 res.content.dc (有效的 DIB 兼容 DC, 由 Dib 独占); 刷/区域/字体均
        // 来自同一 res 且生命周期覆盖; RECT 局部构造。
        unsafe {
            self.fill_frame(hdc, state, res, &frame); // 1)+2) 阴影底 → 裁剪 → 白框底
            self.draw_grid(hdc, res, &frame, query_bottom, dpi); // 4) 查询区网格 (R28)

            // 5) 查询文字 (R23 + 原版排版活体定案): 显示层大写化 + 逐字形固定步距布局。
            let query_rect = RECT {
                left: frame.left,
                top: frame.top,
                right: frame.right,
                bottom: query_bottom.max(frame.top),
            };
            self.draw_query_text(hdc, state, res, &query_rect);

            // 6) 结果列表面板 (0x406 推进来的行; 空表时整段不绘制 ⇒ 无列表 = 基准观感)
            if !state.results.is_empty() {
                let sep = geometry::list_separator_px(dpi);
                let line = RECT {
                    left: frame.left,
                    top: query_bottom,
                    right: frame.right,
                    bottom: query_bottom + sep,
                };
                FillRect(hdc, &line, res.sep_brush);
                self.draw_results(hdc, state, res, inset, query_bottom + sep);
            }

            SelectClipRgn(hdc, None);
        }

        // 7) 逐像素合成 (alpha 掩码 + 预乘) → 呈现
        self.compose_and_present()?;
        // 全量重绘后内容面与当前帧一致 ⇒ 增量行重绘可用
        self.content_valid = true;

        if validate {
            // SAFETY: self.hwnd 有效; 矩形 None 表示整客户区, 无指针参数。
            unsafe {
                let _ = ValidateRect(Some(self.hwnd), None); // R23: 处理尾 ValidateRect
            }
        }
        Ok(())
    }

    /// 步骤 1+2: 全客户区先铺**阴影色** (框外带的底色 = 阴影色; 合成器按阴影 alpha 预乘后,
    /// 带内只剩黑 + alpha ⇒ 桌面被压暗 = 阴影) → 裁剪到白框圆角区 (GDI 只画框内) →
    /// 白框矩形填 backgroundColor (净观感由 compose 的 fill_alpha 决定)。
    /// 注: 白边**不**在此画 —— 它必须是**不透明**色带 (borderOpacity), 而 GDI 画不出逐像素
    /// alpha, 由 compose 按几何解析式生成 (含 AA 内外沿)。
    fn fill_frame(&self, hdc: HDC, state: &FrameState, res: &Resources, frame: &RECT) {
        // SAFETY: hdc 是调用方保证有效的 DIB DC; 刷/区域来自 res 且在调用期间存活; whole 局部构造。
        unsafe {
            let whole = RECT {
                left: 0,
                top: 0,
                right: state.width_px,
                bottom: state.height_px,
            };
            FillRect(hdc, &whole, res.shadow_brush);
            SelectClipRgn(hdc, Some(res.clip));
            FillRect(hdc, frame, res.bg_brush);
        }
    }

    /// 步骤 4 (R28): 查询区网格 —— 横竖双向 1px, 首线偏移 = 间距−1, 只覆盖查询区。
    fn draw_grid(&self, hdc: HDC, res: &Resources, frame: &RECT, query_bottom: i32, dpi: f64) {
        let fw = frame.right - frame.left;
        let qh = (query_bottom - frame.top).max(0);
        let step = geometry::grid_step_px(dpi);
        let off = geometry::grid_first_offset(step);
        // SAFETY: hdc 有效; res.grid_brush 来自 res 且存活; line 逐次局部构造。
        unsafe {
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
    }

    /// 查询区文字: 显示层大写化 + 逐字形固定步距 (pitch = 4×网格步距 DIP) 水平居中于
    /// 各单元格、整串以查询区中心为轴、垂直居中 (原版活体排版定案)。
    fn draw_query_text(&self, hdc: HDC, state: &FrameState, res: &Resources, rect: &RECT) {
        let mut units: Vec<u16> = state.text.to_vec();
        if units.is_empty() {
            return;
        }
        let fw = rect.right - rect.left;
        // SAFETY: hdc 为调用方传入的有效 DIB DC; res.font 有效; units 为本地 Vec, CharUpperW 前已 push(0) 保证 NUL 结尾且独占可变; 退出前还原字体。
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

    /// 结果行绘制 (2026-10-04 Flow Launcher 版式): 选中底色 + 左侧强调条 +
    /// [文件图标 | 标题(黑)/路径(灰) 双行] + 超一屏时的滚动条。
    /// 单行提示 (subtitle 空) 退化为整行居中的旧观感。
    fn draw_results(
        &self,
        hdc: HDC,
        state: &FrameState,
        res: &Resources,
        inset: i32,
        list_top: i32,
    ) {
        let lay = ListLayout::of(state, inset, list_top);
        // SAFETY: hdc/res 由调用方保证有效; 字体在块内选入并在退出前还原; 被调的 paint_result_row 前置同已满足。
        unsafe {
            let old_font = SelectObject(hdc, HGDIOBJ(res.list_title_font.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, self.list_text_color);
            let (start, end) = state.results.window();
            for (i, ri) in (start..end).enumerate() {
                let top = list_top + i as i32 * lay.row_h;
                self.paint_result_row(hdc, state, res, &lay, ri, top);
            }
            SelectObject(hdc, old_font);
            self.draw_scrollbar(hdc, state, res, &lay);
        }
    }

    /// 增量行重绘 (2026-10-06 悬停卡顿第二轮): 只重画 `rows` 里的结果行 + 滚动条,
    /// 内容面其余像素**原样保留** (它是持久面, 不被合成破坏 —— `compose_and_present`
    /// 的双面化正是本方法的前提), 再把这些条带从内容面合成进呈现面并呈现。
    /// 悬停高亮从「全帧 GDI 重画 + 全帧合成」降到「两行 GDI + 两段条带合成」。
    ///
    /// 守卫链: `content_valid` 为假 (resize 后未全量重绘) 时回退全量 [`Self::paint`];
    /// 行号落在可视窗口外的静默跳过 (协议层已保证行集未变, 这里只兜底)。
    fn repaint_rows_impl(
        &mut self,
        state: &FrameState,
        rows: &[usize],
    ) -> Result<(), BackendError> {
        if !self.content_valid {
            return self.paint(state);
        }
        let Some(res) = self.res.as_ref() else {
            return Err(BackendError::new("GDI backend not initialized"));
        };
        let dpi = state.dpi;
        let inset = geometry::band_inset_px(dpi);
        let list_top =
            geometry::query_bottom_px(state.base_height_px, dpi) + geometry::list_separator_px(dpi);
        let lay = ListLayout::of(state, inset, list_top);
        let (start, end) = state.results.window();
        // 条带几何 (窗口坐标): 行条带 + 滚动条列 (行重画会擦掉与行重叠的滑块段)
        let mut strips: Vec<(i32, i32, i32, i32)> = Vec::with_capacity(rows.len() + 1);
        // SAFETY: res.content.dc 是有效的 DIB DC 且由 Dib 独占; 刷/字体取自 res; RECT 局部构造; 退出前还原字体。
        unsafe {
            let hdc = res.content.dc;
            let old_font = SelectObject(hdc, HGDIOBJ(res.list_title_font.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, self.list_text_color);
            for &row in rows {
                if (row as i32) < start as i32 || row >= end {
                    continue; // 不可见 (理论不可达; 兜底)
                }
                let top = list_top + (row - start) as i32 * lay.row_h;
                // 行条带整体回铺框底色 (旧行的选中底/强调条一并擦除), 再按新状态画行
                let strip = RECT {
                    left: lay.inner_l,
                    top,
                    right: lay.inner_r.max(lay.inner_l),
                    bottom: top + lay.row_h,
                };
                FillRect(hdc, &strip, res.bg_brush);
                self.paint_result_row(hdc, state, res, &lay, row, top);
                strips.push((strip.left, strip.top, strip.right, strip.bottom));
            }
            SelectObject(hdc, old_font);
            // 滑块与行条带同列: 行内区域被擦过 ⇒ 无条件整列重画 + 整列入合成清单
            if lay.has_bar {
                self.draw_scrollbar(hdc, state, res, &lay);
                let track_top = list_top + 4;
                let track_bot = (state.height_px - inset - 4).max(track_top + 1);
                strips.push((
                    (lay.inner_r - lay.sb_m - lay.sb_w - 2).max(lay.inner_l),
                    track_top,
                    lay.inner_r,
                    track_bot,
                ));
            }
        }
        res.content.flush();
        // 条带合成: 内容面 → 呈现面 (条带互不重叠, 逐段处理)
        {
            let res = self.res.as_mut().unwrap();
            let plan = res.plan.clone();
            let content = res.content.bytes();
            let layer = res.layer.bytes_mut();
            for strip in &strips {
                compose::composite_rect(content, layer, &plan, *strip, 1.0);
            }
            // 满不透明度帧缓存跟随呈现 (淡出/复原语义与全量路径一致)
            self.frame_full.clear();
            self.frame_full.extend_from_slice(layer);
        }
        if let Some(res) = self.res.as_ref() {
            res.layer.flush();
        }
        self.gain = 255;
        self.present()
    }

    /// 滚动条: 细滑块, 位置 = scroll / (total − vis) 比例 (draw_results /
    /// repaint_rows 共用; 只画滑块不画轨道 —— 轨道即行底色, 2026-10-04 口径)。
    fn draw_scrollbar(&self, hdc: HDC, state: &FrameState, res: &Resources, lay: &ListLayout) {
        if !lay.has_bar {
            return;
        }
        let total = state.results.len();
        let vis = state.results.visible_rows();
        // SAFETY: hdc 由调用方传入有效; bar 为本地 RECT; res.scroll_brush 有效。
        unsafe {
            let track_top = lay.list_top + 4;
            let track_bot = (state.height_px - lay.inset - 4).max(track_top + 1);
            let track_h = track_bot - track_top;
            let thumb_h = (((track_h as f64) * (vis as f64) / (total as f64)).round() as i32)
                .clamp(16, track_h);
            let denom = (total - vis).max(1) as f64;
            let off = (((track_h - thumb_h) as f64) * (state.results.scroll() as f64 / denom))
                .round() as i32;
            let right = lay.inner_r - lay.sb_m;
            let bar = RECT {
                left: (right - lay.sb_w).max(lay.inner_l),
                top: track_top + off,
                right: right.max(lay.inner_l + 1),
                bottom: track_top + off + thumb_h,
            };
            FillRect(hdc, &bar, res.scroll_brush);
        }
    }

    /// 单行内容 (draw_results / repaint_rows 共用): 选中底 + 强调条 + 图标 +
    /// 标题/副标题双行 (或无路径时的单行提示)。调用方保证字体/文字色已选入。
    // SAFETY: 调用方须传入有效的 DIB DC (hdc) 与有效的 res 资源集合, 且已按契约选入标题/副标题字体与文字色; 本函数只绘制, 不获取/释放句柄。
    unsafe fn paint_result_row(
        &self,
        hdc: HDC,
        state: &FrameState,
        res: &Resources,
        lay: &ListLayout,
        ri: usize,
        top: i32,
    ) {
        let row_h = lay.row_h;
        let inner_l = lay.inner_l;
        if ri as i32 == state.results.selected() {
            let row = RECT {
                left: inner_l,
                top,
                right: lay.inner_r.max(inner_l),
                bottom: top + row_h,
            };
            FillRect(hdc, &row, res.select_brush);
            // 强调条上下各留 4px 呼吸 (贴满整行会显得笨重)
            let acc = RECT {
                left: inner_l,
                top: top + 4,
                right: inner_l + lay.accent_w,
                bottom: (top + row_h - 4).max(top + 5),
            };
            FillRect(hdc, &acc, res.accent_brush);
        }
        let Some(item) = state.results.item(ri) else {
            return;
        };

        // ---- Flow Launcher 版式 (2026-10-04): [图标 | 标题(黑) / 路径(灰)] ----
        if item.subtitle.is_empty() {
            // 单行提示 (ShowHint): 无图标, 整行垂直居中 (与旧观感连续)
            let mut u: Vec<u16> = item.title.encode_utf16().collect();
            if u.is_empty() {
                return;
            }
            let mut tr = RECT {
                left: inner_l + lay.pad,
                top,
                right: lay.text_right.max(inner_l + lay.pad + 1),
                bottom: top + row_h,
            };
            DrawTextW(hdc, &mut u, &mut tr, LIST_TEXT_FORMAT);
            return;
        }

        // 文件图标: 左侧图标盒, 行内垂直居中 (提取失败 = 只有文字, 不带崩渲染)。
        // 系统图标 RGB 经 DrawIconEx 落到 DIB, alpha 仍由 compose 按几何写入
        // ⇒ 图标与文字同享面板净不透明度, 观感一致。
        let icon_left = inner_l + lay.pad;
        if let Some(hicon) = self.icons.get(&item.subtitle) {
            let iy = top + (row_h - lay.icon_sz) / 2;
            let _ = DrawIconEx(
                hdc,
                icon_left,
                iy,
                hicon,
                lay.icon_sz,
                lay.icon_sz,
                0,
                None,
                DI_NORMAL,
            );
        }
        let text_left = icon_left + lay.icon_sz + lay.icon_gap;

        // 标题 (上行, 主文字色): 文件名含后缀, 上半带垂直居中
        let mut tu: Vec<u16> = item.title.encode_utf16().collect();
        if !tu.is_empty() {
            let mut tr = RECT {
                left: text_left,
                top: top + 2,
                right: lay.text_right.max(text_left + 1),
                bottom: top + row_h / 2 + 2,
            };
            DrawTextW(hdc, &mut tu, &mut tr, LIST_TITLE_FORMAT);
        }
        // 副标题 (下行, 灰): 完整路径, 下半带垂直居中, 路径省略保留首尾
        let mut su: Vec<u16> = item.subtitle.encode_utf16().collect();
        if !su.is_empty() {
            SelectObject(hdc, HGDIOBJ(res.list_sub_font.0));
            SetTextColor(hdc, self.list_sub_color);
            let mut sr = RECT {
                left: text_left,
                top: top + row_h / 2 - 2,
                right: lay.text_right.max(text_left + 1),
                bottom: top + row_h - 2,
            };
            DrawTextW(hdc, &mut su, &mut sr, LIST_SUB_FORMAT);
            SelectObject(hdc, HGDIOBJ(res.list_title_font.0));
            SetTextColor(hdc, self.list_text_color);
        }
    }
}

/// 结果区一次性推导的布局常量 (`draw_results` 与 `repaint_rows` 共用 ——
/// 两处各推一遍迟早漂移, 2026-10-06 抽取)。
struct ListLayout {
    /// 行高 (物理像素)
    row_h: i32,
    /// 内容区左右缘 (含环形内边距)
    inner_l: i32,
    inner_r: i32,
    /// 文字右缘 (有滚动条时让出滑块列)
    text_right: i32,
    /// 文字内边距 / 强调条宽 / 图标盒尺寸与间距
    pad: i32,
    accent_w: i32,
    icon_sz: i32,
    icon_gap: i32,
    /// 滚动条宽 / 右缘留白
    sb_w: i32,
    sb_m: i32,
    /// 是否有滚动条 (项数 > 可视行数)
    has_bar: bool,
    /// 列表顶 (窗口坐标)
    list_top: i32,
    /// 框外带内缩 (repaint_rows 的滚动条轨道推导用)
    inset: i32,
}

impl ListLayout {
    fn of(state: &FrameState, inset: i32, list_top: i32) -> Self {
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
        let icon_sz = geometry::list_icon_px(dpi);
        let icon_gap = geometry::list_icon_gap_px(dpi);
        Self {
            row_h,
            inner_l,
            inner_r,
            text_right,
            pad,
            accent_w,
            icon_sz,
            icon_gap,
            sb_w,
            sb_m,
            has_bar,
            list_top,
            inset,
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
        // SAFETY: self.hwnd 为刚存入的有效窗口句柄; policy/margins 为本地值, 指针与长度取自同一结构。
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
        self.list_sub_color = COLORREF(skin::list_sub_color(state.skin).as_colorref());
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

    /// 结果列表展开/收起: 尺寸变了 ⇒ 内容面/呈现面/合成计划/区域全部重建
    /// (区域与自合成面都是**窗口坐标**, 不重建则新增的下半部分被裁掉或未绘制)。
    /// 重建后内容面失效 ⇒ `content_valid = false`, 增量行重绘回退全量。
    fn resize(&mut self, state: &FrameState) -> Result<(), BackendError> {
        let dpi = state.dpi;
        let s = state.skin;
        let plan = Self::build_plan(state);
        if let Some(res) = self.res.as_mut() {
            // 新面 (旧 Dib 在赋值时 Drop: 解绑 + 删对象)
            res.content = Dib::new(state.width_px, state.height_px)?;
            res.layer = Dib::new(state.width_px, state.height_px)?;
            self.content_valid = false;
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
                    // SAFETY: old 是本结构持有的旧区域句柄, 已被 new 取代, 只在此删除一次。
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

    /// 选择变化增量重绘 (2026-10-06): 双面化后内容面持久有效, 只重画新旧两行。
    fn repaint_rows(&mut self, state: &FrameState, rows: &[usize]) -> Result<(), BackendError> {
        self.repaint_rows_impl(state, rows)
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
                // SAFETY: Sleep 只接受毫秒整型, 无线程/指针前置条件。
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
        if self.frame_full.len() != res.layer.len {
            return;
        }
        let full = self.frame_full.clone();
        compose::scale_frame(&full, res.layer.bytes_mut(), gain01);
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
            // SAFETY: self.hwnd 有效; rgn 由 hit_region 新建且已判非空, 所有权移交系统。
            unsafe {
                let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            }
        }
    }
}
