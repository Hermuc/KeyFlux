//! 唯一 Win32→core 翻译层 (design C §2.7 `wndproc`)。
//!
//! 分派表 = 协议速查表 (spec.md:162-172) 的逐行翻译, 仅下列分支:
//!   WM_CREATE / 0x401 / 0x402 / 0x403 / WM_CHAR / WM_PAINT / WM_DESTROY
//!   (+ WM_NCCREATE 锚点写入, 内部细节无消息契约, R3 注)。
//!
//! ⚠ R19 阴性面 (原版契约): 原版对 WM_GETTEXT / WM_SETTEXT / WM_IME_* 等一律
//!   `DefWindowProcW` (文本只写)。2026-10-04 Rust 版**有意扩展**: WM_GETTEXT(_LENGTH)
//!   = 读回通道 / WM_IME_START·END = 组合态标志 / 0x404·0x405 = 搜索激活与查询 ——
//!   其余消息 (含 WM_CLOSE / WM_SETTEXT / WM_KEYDOWN / WM_DPICHANGED …) 仍一律
//!   `DefWindowProcW`, 特别是:
//!   - **WM_CLOSE 故意不在分派表** → DefWindowProc 默认销毁 → WM_DESTROY (R18 链);
//!   - WM_GETTEXT 默认只返回标题 " " (文本只写, R19);
//!   - WM_SETTEXT 走默认标题通道, 绝不触碰文本缓冲 (R19 活体铁证语义);
//!   - 0x401/0x402/0x403 的 wParam/lParam 完全忽略 (R14/R15/R16)。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, InvalidateRect, SetWindowRgn,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetSystemMetrics, GetWindowLongPtrW, PostQuitMessage,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_USERDATA,
    WINDOW_LONG_PTR_INDEX,
    HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, WM_CREATE, WM_DESTROY, WM_NCCREATE,
    WM_PAINT, CREATESTRUCTW,
};

use crate::config;
use crate::geometry::{self, FrameGeom};
use crate::protocol::{on_event, AppEvent, Command, AppState};
use crate::render::{BackendError, FrameState, RenderBackend};
use crate::skin::Skin;
use crate::sound::SoundBackend;
use crate::win::dpi;
use crate::win::error;

/// 一进程一窗口 (R31): 全部会话状态挂在唯一 HWND 上。
pub struct Shell {
    /// 皮肤 (R26: 构造期读一次)
    pub skin: Skin,
    /// 会话状态 (文本 + 可见性 + 淡出时长, R22)
    pub state: AppState,
    /// DPI (R12: WM_CREATE 一次定案; dpi.0 = dpiX, dpi.1 = dpiY)
    pub dpi: (f64, f64),
    /// 窗口几何存值 (R12: 0x401 显示用创建期存值 X/Y, 不重算)
    pub geom: FrameGeom,
    /// 渲染后端 (design C §2.3 唯一渲染缝)
    pub backend: Box<dyn RenderBackend>,
    /// 音效后端 (R24)
    pub sound: Box<dyn SoundBackend>,
    /// 渲染设备是否已初始化 (R23 懒创建: 首个 WM_PAINT / 0x401 预绘)
    pub inited: bool,
    /// IME 组合态 (2026-10-04 扩展: WM_IME_START/END 维护; 0x404 查询; 回车语义判定)
    pub composing: bool,
}

impl Shell {
    /// FrameState 构造 (字段级借用, 供 init_backend/on_paint/redraw 拆分使用)。
    #[cfg(test)]
    pub(crate) fn frame_state(&self) -> FrameState<'_> {
        FrameState {
            text: self.state.text.units(),
            skin: &self.skin,
            dpi: self.dpi.0,
            width_px: self.geom.w,
            height_px: self.geom.h,
        }
    }
}

/// WM_CREATE: R11 几何一次定案 (R12: 之后不再查询)。
fn on_create(hwnd: HWND, shell: &mut Shell) -> Result<(), String> {
    let (dx, dy) = dpi::monitor_dpi(hwnd)
        .map_err(|hr| format!("GetDpiForMonitor failed (HRESULT {hr:#X})"))?;
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let geom = geometry::window_rect(dx, dy, sw, sh, &shell.skin);
    shell.dpi = (dx, dy);
    shell.geom = geom;
    // R11: SetWindowPos(hwnd, 0, X, Y, W, H, 0x14 = NOZORDER|NOACTIVATE)
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND::default()),
            geom.x,
            geom.y,
            geom.w,
            geom.h,
            SWP_FLAGS_CREATE,
        )
    }
    .map_err(|e| format!("SetWindowPos failed: {e}"))?;
    Ok(())
}

const SWP_FLAGS_CREATE: windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS =
    windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(config::SWP_CREATE_FLAGS);
const SWP_FLAGS_SHOW: windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS =
    windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(config::SWP_SHOW_FLAGS);

/// 指令执行 (Win32 侧)。渲染失败 → R29 弹窗终止 (渲染调用失败 → 错误 MessageBoxW + 进程终止)。
fn execute(shell: &mut Shell, hwnd: HWND, cmds: Vec<Command>) {
    for cmd in cmds {
        match cmd {
            Command::PlaySound(key) => shell.sound.play(key), // R24: 失败静默, 不阻断
            Command::Redraw { pre_show } => {
                if let Err(e) = redraw(shell, hwnd, pre_show) {
                    error::fatal(file!(), line!(), &format!("render failed: {e}"), e.hresult);
                }
            }
            Command::ShowWindow => {
                // R14⑤: SetWindowPos(HWND_TOPMOST, 存值X/Y, cx=cy=0, 0x51) ——
                // 重申置顶、显示、位置重设为创建期存值, 不改尺寸 (spec.md:176)
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        shell.geom.x,
                        shell.geom.y,
                        0,
                        0,
                        SWP_FLAGS_SHOW,
                    );
                }
            }
            Command::BeginFade { duration_secs } => {
                // R15①: 阻塞式淡出 (窗口线程不取消息, 排队消息顺延 —— 原版同语义)
                shell.backend.fade_out(duration_secs);
            }
            Command::HideWindow => {
                // R15③/R16: ShowWindow(SW_HIDE); 随后 alpha 复原 (R10-5)
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                shell.backend.on_hidden();
            }
            Command::Quit => unsafe { PostQuitMessage(0) }, // R18
        }
    }
}

/// Redraw 指令: pre_show = 0x401 的同步预绘 (消白闪), 否则 InvalidateRect 走下一绘制周期。
fn redraw(shell: &mut Shell, hwnd: HWND, pre_show: bool) -> Result<(), BackendError> {
    if !shell.inited {
        init_backend(shell, hwnd)?;
    }
    if pre_show {
        // 字段级不相交借用: FrameState 借 self.{state,skin,dpi,geom}, backend 可变借用
        let st = FrameState {
            text: shell.state.text.units(),
            skin: &shell.skin,
            dpi: shell.dpi.0,
            width_px: shell.geom.w,
            height_px: shell.geom.h,
        };
        shell.backend.pre_show(&st)?;
    }
    // R14④/R17: InvalidateRect(hwnd, NULL, FALSE) —— 下一绘制周期呈现 (R8);
    // 对预绘路径也是保险 (区域失效 → 显示后 WM_PAINT 再全量重绘一次)
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    Ok(())
}

/// WM_PAINT: 全窗口自绘 + ValidateRect (R23)。
fn on_paint(shell: &mut Shell, hwnd: HWND) -> Result<(), BackendError> {
    if !shell.inited {
        init_backend(shell, hwnd)?;
    }
    let st = FrameState {
        text: shell.state.text.units(),
        skin: &shell.skin,
        dpi: shell.dpi.0,
        width_px: shell.geom.w,
        height_px: shell.geom.h,
    };
    shell.backend.paint(&st)
}

/// R23: 渲染资源懒创建 (首个 WM_PAINT / 0x401 预绘时); 失败 → R29。
fn init_backend(shell: &mut Shell, hwnd: HWND) -> Result<(), BackendError> {
    let st = FrameState {
        text: shell.state.text.units(),
        skin: &shell.skin,
        dpi: shell.dpi.0,
        width_px: shell.geom.w,
        height_px: shell.geom.h,
    };
    shell.backend.init(hwnd.0 as usize, &st)?;
    shell.inited = true;
    Ok(())
}

/// 窗口过程 (R3: 类注册的 lpfnWndProc)。
pub(crate) unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // WM_NCCREATE: 对象指针存入 GWLP_USERDATA (R3 注「自选等价机制」; 若未来提供
    // §K 读回通道 R32, 此锚点即该通道强制形态)
    if msg == WM_NCCREATE {
        let cs = lparam.0 as *const CREATESTRUCTW;
        if !cs.is_null() {
            let params: *mut core::ffi::c_void = (*cs).lpCreateParams;
            if !params.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, params as isize);
            }
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam); // 必须继续默认流程完成创建
    }

    let shell_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Shell;
    let Some(shell) = shell_ptr.as_mut() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };

    match msg {
        WM_CREATE => match on_create(hwnd, shell) {
            Ok(()) => LRESULT(0),
            // R11 失败路径: 弹错误详情后进程立即终止, 绝不以错误几何继续运行
            Err(e) => error::fatal(file!(), line!(), &e, 0),
        },

        // ---- 引擎消息协议 (R7 常量绝对值) ----
        config::APP_SHOW => {
            // R14: 0x401 = show 音效 → 清空 → 预绘+重绘 → SetWindowPos 显示; wParam/lParam 忽略
            let cmds = on_event(AppEvent::ShowClear, &mut shell.state);
            execute(shell, hwnd, cmds);
            LRESULT(0)
        }
        config::APP_HIDE => {
            // R15: 0x402 = 阻塞淡出 → SW_HIDE; 不清空无音效; wParam/lParam 忽略
            let cmds = on_event(AppEvent::HideFade, &mut shell.state);
            execute(shell, hwnd, cmds);
            LRESULT(0)
        }
        config::APP_CANCEL => {
            // R16: 0x403 = cancel 音效 → 立即 SW_HIDE; 不清空; wParam/lParam 忽略
            let cmds = on_event(AppEvent::CancelHide, &mut shell.state);
            execute(shell, hwnd, cmds);
            LRESULT(0)
        }
        config::WM_CHAR_VAL => {
            // R17: WM_CHAR = 唯一文本写入通道; wParam 低 16 位 = 码元; lParam 无语义不读
            let ch = (wparam.0 & 0xFFFF) as u16;
            let cmds = on_event(AppEvent::Char(ch), &mut shell.state);
            execute(shell, hwnd, cmds);
            LRESULT(0)
        }

        // ---- 2026-10-04 协议扩展 (Rust 版自有; 中文检索读回/激活/组合态) ----
        config::APP_SEARCH_ACTIVATE => {
            // 前台 + 焦点: IME 组合窗跟随本窗口, 上屏中文经 WM_CHAR 进入文本缓冲。
            // 🔴 不改 WS_EX_NOACTIVATE —— SetWindowLongPtrW(GWL_EXSTYLE) 会重置
            //   SetWindowRgn 窗口区域 → 42px 透明边带瞬间全部可见 (用户报障闪现)。
            //   MSDN: NOACTIVATE 仅阻止鼠标点击激活, 程序化 SetForegroundWindow 不受影响。
            unsafe {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
            }
            LRESULT(0)
        }
        config::APP_SEARCH_STATE => LRESULT(shell.composing as isize),
        config::WM_GETTEXT_VAL => {
            // 读回通道: 文本缓冲按 UTF-16 写入调用方缓冲 (系统跨进程编组),
            // 返回拷贝码元数 (不含 NUL); wParam = 容量, lParam = 缓冲
            let cap = wparam.0 as usize;
            let units = shell.state.text.units();
            let n = units.len().min(cap.saturating_sub(1));
            if n > 0 {
                let dst = lparam.0 as *mut u16;
                for (i, u) in units.iter().take(n).enumerate() {
                    unsafe { *dst.add(i) = *u };
                }
                unsafe { *dst.add(n) = 0 };
            }
            LRESULT(n as isize)
        }
        config::WM_GETTEXTLENGTH_VAL => LRESULT(shell.state.text.len() as isize),
        config::WM_IME_START => {
            shell.composing = true;
            DefWindowProcW(hwnd, msg, wparam, lparam) // IME UI 交默认流程
        }
        config::WM_IME_END => {
            shell.composing = false;
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_PAINT => match on_paint(shell, hwnd) {
            Ok(()) => LRESULT(0),
            Err(e) => error::fatal(
                file!(),
                line!(),
                &format!("render failed: {e}"),
                e.hresult,
            ),
        },

        WM_DESTROY => {
            // R18: → PostQuitMessage(0) → 消息循环退出 (唯一退出路径)
            let cmds = on_event(AppEvent::Destroy, &mut shell.state);
            execute(shell, hwnd, cmds);
            LRESULT(0)
        }

        // R19 阴性面: 其余一切消息交 DefWindowProcW (WM_CLOSE 由此默认销毁, R18)
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
