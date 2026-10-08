//! 唯一 Win32→core 翻译层 (design C §2.7 `wndproc`)。
//!
//! 分派表 = 协议速查表 (spec.md:162-172) 的逐行翻译, 仅下列分支:
//!   WM_CREATE / 0x401 / 0x402 / 0x403 / WM_CHAR / WM_PAINT / WM_DESTROY
//!   (+ WM_NCCREATE 锚点写入, 内部细节无消息契约, R3 注)
//!   (+ 2026-10-04 结果列表面板: 0x406 WM_COPYDATA / 0x407 / 0x408 / 鼠标三消息)。
//!
//! ⚠ R19 阴性面 (原版契约): 原版对 WM_GETTEXT / WM_SETTEXT / WM_IME_* 等一律
//!   `DefWindowProcW` (文本只写)。2026-10-04 Rust 版**有意扩展**: WM_GETTEXT(_LENGTH)
//!   = 读回通道 / WM_IME_START·END = 组合态标志 / 0x404·0x405 = 搜索激活与查询 /
//!   0x406-0x408 + WM_COPYDATA = 结果列表面板 —— 其余消息 (含 WM_CLOSE / WM_SETTEXT /
//!   WM_KEYDOWN / WM_DPICHANGED …) 仍一律 `DefWindowProcW`, 特别是:
//!   - **WM_CLOSE 故意不在分派表** → DefWindowProc 默认销毁 → WM_DESTROY (R18 链);
//!   - WM_GETTEXT 默认只返回标题 " " (文本只写, R19);
//!   - WM_SETTEXT 走默认标题通道, 绝不触碰文本缓冲 (R19 活体铁证语义);
//!   - 0x401/0x402/0x403 的 wParam/lParam 完全忽略 (R14/R15/R16)。
//!
//! 窗口高度与结果列表 (2026-10-04): `Shell.cur_h` = 当前窗口高 = 基准高 (R11 定案) +
//!   `geometry::list_extra_px`。列表结构变化 ⇒ `Command::Relayout` ⇒ SetWindowPos +
//!   后端区域重建 (区域是窗口坐标, 变高后必须重建, 否则新增的下半部分被旧区域裁掉)。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetSystemMetrics, GetWindowLongPtrW, PeekMessageW, PostMessageW,
    PostQuitMessage, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, GWLP_USERDATA, HWND_TOPMOST, MSG, PEEK_MESSAGE_REMOVE_TYPE, PM_REMOVE,
    SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SW_HIDE, WM_CREATE, WM_DESTROY, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT,
};

use crate::config;
use crate::geometry::{self, FrameGeom};
use crate::protocol::{on_event, AppEvent, AppState, Command};
use crate::render::{BackendError, FrameState, RenderBackend};
use crate::results;
use crate::skin::Skin;
use crate::sound::SoundBackend;
use crate::win::dpi;
use crate::win::error;

/// 一进程一窗口 (R31): 全部会话状态挂在唯一 HWND 上。
pub struct Shell {
    /// 皮肤 (R26: 构造期读一次)
    pub skin: Skin,
    /// 会话状态 (文本 + 可见性 + 淡出时长 + 结果列表, R22)
    pub state: AppState,
    /// DPI (R12: WM_CREATE 一次定案; dpi.0 = dpiX, dpi.1 = dpiY)
    pub dpi: (f64, f64),
    /// **基准**窗口几何存值 (R12: 0x401 显示用创建期存值 X/Y, 不重算; 高 = 无列表态)
    pub geom: FrameGeom,
    /// 当前窗口高 (基准高 + 结果列表附加高; 无列表时 == geom.h)
    pub cur_h: i32,
    /// 渲染后端 (design C §2.3 唯一渲染缝)
    pub backend: Box<dyn RenderBackend>,
    /// 音效后端 (R24)
    pub sound: Box<dyn SoundBackend>,
    /// 渲染设备是否已初始化 (R23 懒创建: 首个 WM_PAINT / 0x401 预绘)
    pub inited: bool,
    /// IME 组合态 (2026-10-04 扩展: WM_IME_START/END 维护; 0x404 查询; 回车语义判定)
    pub composing: bool,
    /// 结果交互回推目标 = 0x406 的 wParam (引擎脚本窗口); 空 = 从未收到过结果
    pub notify_target: HWND,
}

/// FrameState 构造 (字段级借用 —— 宏原地展开, 保证只借 state/skin/dpi/geom,
/// 与 `backend` 的可变借用不相交, 供 init/redraw/paint/resize 复用)。
macro_rules! frame_state {
    ($shell:expr) => {
        FrameState {
            text: $shell.state.text.units(),
            skin: &$shell.skin,
            dpi: $shell.dpi.0,
            width_px: $shell.geom.w,
            height_px: $shell.cur_h,
            base_height_px: $shell.geom.h,
            results: &$shell.state.results,
            badge: $shell.state.badge,
        }
    };
}

/// WM_CREATE: R11 几何一次定案 (R12: 之后不再查询); 列表可视行数按屏幕收敛。
fn on_create(hwnd: HWND, shell: &mut Shell) -> Result<(), String> {
    let (dx, dy) = dpi::monitor_dpi(hwnd)
        .map_err(|hr| format!("GetDpiForMonitor failed (HRESULT {hr:#X})"))?;
    // SAFETY: GetSystemMetrics 仅查询系统度量, SM_CXSCREEN 是合法索引, 无指针参数与副作用。
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    // SAFETY: 同上, SM_CYSCREEN 是合法索引, 无指针参数与副作用。
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let geom = geometry::window_rect(dx, dy, sw, sh, &shell.skin);
    shell.dpi = (dx, dy);
    shell.geom = geom;
    shell.cur_h = geom.h;
    // 结果列表: 展开后不得越屏 ⇒ 可视行数按屏幕高度收敛 (基准几何不受影响)
    shell
        .state
        .results
        .set_visible_max(geometry::max_list_rows(dx, sh, geom.y, geom.h));
    // R11: SetWindowPos(hwnd, 0, X, Y, W, H, 0x14 = NOZORDER|NOACTIVATE)
    // SAFETY: hwnd 是本 wndproc 收到的有效窗口句柄; hwndInsertAfter=NULL 且 flags 含 SWP_NOZORDER, 该参数不被使用。
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

/// 列表展开/收起时的 SetWindowPos flags = SWP_NOACTIVATE (保持不激活; 置顶由
/// hwndInsertAfter = HWND_TOPMOST 重申, 故**不得**同时给 SWP_NOZORDER)。
const SWP_FLAGS_RESIZE: windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS =
    SWP_NOACTIVATE;

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
            Command::RedrawRows { prev } => {
                // 选择变化增量路径 (2026-10-06): 行集未变 (协议层守卫), 只重绘旧/新两行。
                // 后端不支持/内容面失效时自动回落全量重绘 —— 两态视觉等价。
                if let Err(e) = repaint_rows(shell, hwnd, prev) {
                    error::fatal(file!(), line!(), &format!("render failed: {e}"), e.hresult);
                }
            }
            Command::Relayout => {
                if let Err(e) = relayout(shell, hwnd) {
                    error::fatal(
                        file!(),
                        line!(),
                        &format!("relayout failed: {e}"),
                        e.hresult,
                    );
                }
            }
            Command::ShowWindow => {
                // R14⑤: SetWindowPos(HWND_TOPMOST, 存值X/Y, cx=cy=0, 0x51) ——
                // 重申置顶、显示、位置重设为创建期存值, 不改尺寸 (spec.md:176)
                // SAFETY: hwnd 由消息循环传入且有效; HWND_TOPMOST 是预定义合法插入句柄。
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
                // SAFETY: hwnd 是当前有效窗口句柄; SW_HIDE 是合法命令, 返回值被忽略。
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                shell.backend.on_hidden();
            }
            // SAFETY: PostQuitMessage 只接受整型退出码, 无指针/线程前置条件。
            Command::Quit => unsafe { PostQuitMessage(0) }, // R18
        }
    }
}

/// 结果列表结构变化 (项集/展开/收起): 重算窗口高 → 需要时 SetWindowPos + 后端区域重建
/// → 同步预绘 (可见窗口上避免新区域出现未定义像素的闪帧)。
fn relayout(shell: &mut Shell, hwnd: HWND) -> Result<(), BackendError> {
    let h = shell.geom.h + geometry::list_extra_px(shell.state.results.visible_rows(), shell.dpi.0);
    if h != shell.cur_h {
        shell.cur_h = h;
        // SAFETY: hwnd 有效; 句柄 HWND_TOPMOST 合法; flags 含 SWP_NOACTIVATE 不激活窗口。
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                shell.geom.x,
                shell.geom.y,
                shell.geom.w,
                h,
                SWP_FLAGS_RESIZE,
            );
        }
        if shell.inited {
            let st = frame_state!(shell);
            shell.backend.resize(&st)?;
        }
    }
    if shell.inited {
        let st = frame_state!(shell);
        shell.backend.pre_show(&st)?;
    }
    // R8: 区域失效 → 下一绘制周期再全量重绘一次 (保险)
    // SAFETY: hwnd 有效; 矩形传 None 表示整个客户区, 无越界写。
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    Ok(())
}

/// Redraw 指令: pre_show = 0x401 的同步预绘 (消白闪), 否则 InvalidateRect 走下一绘制周期。
fn redraw(shell: &mut Shell, hwnd: HWND, pre_show: bool) -> Result<(), BackendError> {
    if !shell.inited {
        init_backend(shell, hwnd)?;
    }
    if pre_show {
        let st = frame_state!(shell);
        shell.backend.pre_show(&st)?;
    }
    // R14④/R17: InvalidateRect(hwnd, NULL, FALSE) —— 下一绘制周期呈现 (R8);
    // 对预绘路径也是保险 (区域失效 → 显示后 WM_PAINT 再全量重绘一次)
    // SAFETY: 同上: hwnd 有效, 矩形 None 表示整客户区, 无越界写。
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
    let st = frame_state!(shell);
    shell.backend.paint(&st)
}

/// 选择变化的增量重绘 (Command::RedrawRows): 行集 = 旧行 + 新高亮行 (各自可能
/// 为「无高亮」而缺席)。未初始化时先懒创建 (与 redraw 同款前置)。
fn repaint_rows(shell: &mut Shell, hwnd: HWND, prev: i32) -> Result<(), BackendError> {
    if !shell.inited {
        init_backend(shell, hwnd)?;
    }
    let sel = shell.state.results.selected();
    let mut rows: Vec<usize> = Vec::with_capacity(2);
    if prev >= 0 {
        rows.push(prev as usize);
    }
    if sel >= 0 && rows.last() != Some(&(sel as usize)) {
        rows.push(sel as usize);
    }
    let st = frame_state!(shell);
    shell.backend.repaint_rows(&st, &rows)
}

/// R23: 渲染资源懒创建 (首个 WM_PAINT / 0x401 预绘时); 失败 → R29。
fn init_backend(shell: &mut Shell, hwnd: HWND) -> Result<(), BackendError> {
    let st = frame_state!(shell);
    shell.backend.init(hwnd.0 as usize, &st)?;
    shell.inited = true;
    Ok(())
}

// ---- 结果列表面板的鼠标交互 (2026-10-04) ----

/// 客户区坐标 → 结果行 (0 基)。不在结果区 / 落在底部留白 / 越界 = None。
/// 窗口为 WS_POPUP 无标题无边框 ⇒ 客户区坐标 == 窗口坐标。
fn hit_row(shell: &Shell, x: i32, y: i32) -> Option<usize> {
    let n = shell.state.results.len();
    if n == 0 {
        return None;
    }
    let dpi = shell.dpi.0;
    let inset = geometry::band_inset_px(dpi);
    if x < inset || x >= shell.geom.w - inset {
        return None;
    }
    let list_top = geometry::query_bottom_px(shell.geom.h, dpi) + geometry::list_separator_px(dpi);
    if y < list_top {
        return None;
    }
    let row_h = geometry::list_row_h_px(dpi).max(1);
    let idx = ((y - list_top) / row_h) as usize;
    let (start, end) = shell.state.results.window();
    if idx >= end - start {
        return None; // 底部留白区
    }
    Some(start + idx)
}

/// lParam 的低/高 16 位 → 客户区坐标 (带符号)。
fn client_xy(lparam: LPARAM) -> (i32, i32) {
    let v = lparam.0 as u32;
    (
        (v & 0xFFFF) as u16 as i16 as i32,
        ((v >> 16) & 0xFFFF) as u16 as i16 as i32,
    )
}

/// 命令框 → 引擎的回推: wParam = 1 基行号, lParam = 1 点选 / 2 高亮变化。
/// 目标 = 0x406 的 wParam (引擎脚本窗口); 从未收到过结果时静默丢弃。
fn notify_engine(shell: &Shell, row_1based: usize, kind: i32) {
    if shell.notify_target.0.is_null() {
        return;
    }
    // SAFETY: shell.notify_target 已在上方判非空, 是 0x406 发送方 (引擎脚本) 的窗口句柄。
    unsafe {
        let _ = PostMessageW(
            Some(shell.notify_target),
            config::APP_RESULTS_NOTIFY,
            WPARAM(row_1based),
            LPARAM(kind as isize),
        );
    }
}

/// 高亮移动 (悬停/滚轮共用): 本地立即重绘 + 回推引擎同步其 index。
fn move_selection(shell: &mut Shell, hwnd: HWND, row: usize) {
    if shell.state.results.selected() == row as i32 {
        return;
    }
    let cmds = on_event(AppEvent::SetSelection(row as i32), &mut shell.state);
    execute(shell, hwnd, cmds);
    notify_engine(shell, row + 1, 2);
}

/// 合并排队的 WM_MOUSEMOVE (2026-10-06 悬停高亮「不跟手」修复):
/// 一次快速划过 N 行会排队 N 条 WM_MOUSEMOVE, 若逐条消费, 每条都走一遍
/// 「全帧 GDI 重画 → 逐像素合成 → UpdateLayeredWindow」管线, 高亮永远追着
/// 光标跑 (中文搜索词的 DrawTextW 变形更贵, 放大延迟)。标准 Win32 手法:
/// 取消息前先用 PeekMessage(PM_REMOVE) 排干队列里**其余**的 WM_MOUSEMOVE、
/// 只处理最后一条 —— 高亮直接跳到光标最新位置, 中间帧零渲染。只取不改任何
/// 既有语义: 被排干的移动本就只会落在光标途经的行上, 终态一致。
fn coalesce_mousemove(hwnd: HWND, lparam: LPARAM) -> LPARAM {
    let mut last = lparam;
    let mut msg = MSG::default();
    // SAFETY: &mut msg 是本地栈变量, 生命周期覆盖整个循环; hwnd 有效; 过滤器常量合法。
    unsafe {
        while PeekMessageW(
            &mut msg,
            Some(hwnd),
            WM_MOUSEMOVE,
            WM_MOUSEMOVE,
            PEEK_MESSAGE_REMOVE_TYPE(PM_REMOVE.0),
        )
        .as_bool()
        {
            last = msg.lParam;
        }
    }
    last
}

/// 9 个协议消息共用的臂：`on_event` 产出的命令序列交给 `execute`，统一返回 0。
/// 抽出以消除「on_event + execute + LRESULT(0)」的 9 次重复（2026-10-09 结构优化）。
fn dispatch_event(shell: &mut Shell, hwnd: HWND, event: AppEvent) -> LRESULT {
    let cmds = on_event(event, &mut shell.state);
    execute(shell, hwnd, cmds);
    LRESULT(0)
}

/// 前台 + 焦点: IME 组合窗跟随本窗口, 上屏中文经 WM_CHAR 进入文本缓冲。
/// 🔴 不改 WS_EX_NOACTIVATE —— SetWindowLongPtrW(GWL_EXSTYLE) 会重置 SetWindowRgn 窗口
///   区域 → 42px 透明边带瞬间全部可见 (用户报障闪现)。MSDN: NOACTIVATE 仅阻止鼠标点击
///   激活, 程序化 SetForegroundWindow 不受影响。
fn activate_for_search(hwnd: HWND) -> LRESULT {
    // SAFETY: hwnd 为当前有效窗口; SetFocus 要求窗口与调用线程同属一个消息队列,
    // 本进程单窗口单线程满足。
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
    LRESULT(0)
}

/// 读回通道: 文本缓冲按 UTF-16 写入调用方缓冲 (系统跨进程编组), 返回拷贝码元数 (不含 NUL)。
fn copy_text_out(shell: &Shell, cap: usize, lparam: LPARAM) -> usize {
    let units = shell.state.text.units();
    let n = units.len().min(cap.saturating_sub(1));
    if n > 0 {
        let dst = lparam.0 as *mut u16;
        for (i, u) in units.iter().take(n).enumerate() {
            // SAFETY: dst 是 WM_GETTEXT 调用方提供的缓冲; 循环上限 n ≤ cap-1, 写入落在容量内。
            unsafe { *dst.add(i) = *u };
        }
        // SAFETY: n ≤ cap-1, dst.add(n) 仍在调用方缓冲容量内, 用于写入终止 NUL。
        unsafe { *dst.add(n) = 0 };
    }
    n
}

/// 0x406 = WM_COPYDATA: dwData 必须等于载荷魔数 (自定义 tag; 拒收他方数据),
/// lParam = 系统已编组到本进程的 COPYDATASTRUCT。
fn on_results_data(shell: &mut Shell, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = lparam.0 as *const COPYDATASTRUCT;
    if ptr.is_null() {
        return LRESULT(0);
    }
    // SAFETY: ptr 已判非空; lParam 是系统为 WM_COPYDATA 编组的 COPYDATASTRUCT, 在消息处理期间有效。
    let cds = unsafe { &*ptr };
    let size = cds.cbData as usize;
    if (cds.dwData as u32) != results::PAYLOAD_MAGIC
        || cds.lpData.is_null()
        || size > results::MAX_PAYLOAD_BYTES
    {
        return LRESULT(0);
    }
    // SAFETY: cds.lpData 已判非空, size = cds.cbData 且 ≤ MAX_PAYLOAD_BYTES, 缓冲在 0x406 消息存活期内有效。
    let bytes = unsafe { core::slice::from_raw_parts(cds.lpData as *const u8, size) };
    let Some((items, selected)) = results::decode_payload(bytes) else {
        return LRESULT(0); // 结构非法 → 忽略 (对端错误不得带崩命令框)
    };
    // 回推目标 = wParam (发送方窗口 = 引擎脚本窗口); 鼠标点选/悬停要发回它
    let sender = HWND(wparam.0 as *mut core::ffi::c_void);
    if !sender.0.is_null() {
        shell.notify_target = sender;
    }
    let cmds = on_event(AppEvent::SetResults { items, selected }, &mut shell.state);
    execute(shell, hwnd, cmds);
    LRESULT(1) // 非 0 = 已处理
}

/// 滚轮换行目标行 (`None` = 空表, 交默认流程)。delta>0(上滚) = 减 3, 否则加 3, clamp 到 [0,n)。
fn wheel_target(shell: &Shell, wparam: WPARAM) -> Option<usize> {
    let n = shell.state.results.len();
    if n == 0 {
        return None;
    }
    let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    let step: i32 = if delta > 0 { -3 } else { 3 };
    let cur = shell.state.results.selected();
    let target = if cur < 0 {
        0
    } else {
        (cur + step).clamp(0, n as i32 - 1)
    };
    Some(target.max(0) as usize)
}

/// 窗口过程 (R3: 类注册的 lpfnWndProc)。
// SAFETY: 本函数注册为窗口类 lpfnWndProc, 仅由系统消息派发以合法 HWND/消息参数调用;
// 其内将 GWLP_USERDATA 解引用为 *mut Shell, 故调用方须保证该槽要么为 NUL 要么指向
// WM_NCCREATE 存入的、生命周期覆盖消息处理的 Shell。
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
        // R14: 0x401 = show 音效 → 清空 → 收起列表 → 预绘+重绘 → SetWindowPos 显示
        config::APP_SHOW => dispatch_event(shell, hwnd, AppEvent::ShowClear),
        // R15: 0x402 = 阻塞淡出 → SW_HIDE → 收起列表; 不清空无音效; wParam/lParam 忽略
        config::APP_HIDE => dispatch_event(shell, hwnd, AppEvent::HideFade),
        // R16: 0x403 = cancel 音效 → 立即 SW_HIDE → 收起列表; 不清空; wParam/lParam 忽略
        config::APP_CANCEL => dispatch_event(shell, hwnd, AppEvent::CancelHide),
        // R17: WM_CHAR = 唯一文本写入通道; wParam 低 16 位 = 码元; lParam 无语义不读
        config::WM_CHAR_VAL => {
            dispatch_event(shell, hwnd, AppEvent::Char((wparam.0 & 0xFFFF) as u16))
        }

        // ---- 2026-10-04 协议扩展 (Rust 版自有; 中文检索读回/激活/组合态) ----
        config::APP_SEARCH_ACTIVATE => activate_for_search(hwnd),
        config::APP_SEARCH_STATE => LRESULT(shell.composing as isize),
        config::WM_GETTEXT_VAL => LRESULT(copy_text_out(shell, wparam.0, lparam) as isize),
        config::WM_GETTEXTLENGTH_VAL => LRESULT(shell.state.text.len() as isize),
        config::WM_IME_START => {
            shell.composing = true;
            DefWindowProcW(hwnd, msg, wparam, lparam) // IME UI 交默认流程
        }
        config::WM_IME_END => {
            shell.composing = false;
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        // ---- 2026-10-04 结果列表面板 (命令框向下延伸; 引擎对接面) ----
        config::APP_RESULTS_DATA => on_results_data(shell, hwnd, wparam, lparam),
        // 0x407: wParam = 0 基下标 (-1 = 无高亮)
        config::APP_RESULTS_SELECT => {
            dispatch_event(shell, hwnd, AppEvent::SetSelection(wparam.0 as i32))
        }
        // 0x408: 收起列表 (窗口回落基准高)
        config::APP_RESULTS_CLEAR => dispatch_event(shell, hwnd, AppEvent::ClearResults),
        // 0x40A: wParam = 字形编号 (未注册编号由 on_event 忽略 —— 对端错误不带崩框)
        config::APP_BADGE_SHOW => dispatch_event(shell, hwnd, AppEvent::ShowBadge(wparam.0 as u32)),
        // 0x40B: 隐藏徽标 (wParam/lParam 忽略)
        config::APP_BADGE_HIDE => dispatch_event(shell, hwnd, AppEvent::HideBadge),

        // 结果区鼠标交互: 点选 (回推 1) / 悬停高亮 (回推 2) / 滚轮换行。
        // 窗口为 WS_EX_NOACTIVATE + 区域裁切 ⇒ 区域外点击本就不落在窗口上;
        // 命中结果行时才消费消息, 其余交默认流程 (不改任何既有语义)。
        WM_LBUTTONDOWN => {
            let (x, y) = client_xy(lparam);
            match hit_row(shell, x, y) {
                Some(row) => {
                    move_selection(shell, hwnd, row);
                    notify_engine(shell, row + 1, 1);
                    LRESULT(0)
                }
                None => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_MOUSEMOVE => {
            let (x, y) = client_xy(coalesce_mousemove(hwnd, lparam));
            if let Some(row) = hit_row(shell, x, y) {
                move_selection(shell, hwnd, row);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEWHEEL => match wheel_target(shell, wparam) {
            Some(row) => {
                move_selection(shell, hwnd, row);
                LRESULT(0)
            }
            None => DefWindowProcW(hwnd, msg, wparam, lparam),
        },

        WM_PAINT => match on_paint(shell, hwnd) {
            Ok(()) => LRESULT(0),
            Err(e) => error::fatal(file!(), line!(), &format!("render failed: {e}"), e.hresult),
        },

        // R18: → PostQuitMessage(0) → 消息循环退出 (唯一退出路径)
        WM_DESTROY => dispatch_event(shell, hwnd, AppEvent::Destroy),

        // R19 阴性面: 其余一切消息交 DefWindowProcW (WM_CLOSE 由此默认销毁, R18)
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
