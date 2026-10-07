//! 窗口拾取「准星」机制 —— 移植自旧 Avalonia 面板 `Services/WindowPickerSession.cs`
//! (git `1f3dc9f^`, 890 行; 用户报障 2026-10-05「旧版有准星按钮, 新面板没有了」)。
//!
//! ## 交互 (与旧版逐条对齐)
//! 点击准星 → 鼠标变系统级十字光标 → 移动时实时高亮指向窗口 (3px 空心框) →
//! 左键提交 (回填窗口标识符, [`MatchKind`] 决定格式) → Esc / 右键取消。
//! 提交点无窗口 / 命中自身进程 → 会话终止并返回 `NoWindow` (UI 层提示 i18n 1082);
//! 探高完整性进程被拒 → `AccessDenied` (提示 1081「请以管理员身份运行设置界面」)。
//!
//! ## 机制 (方案 A, 与旧版同源)
//! `WH_MOUSE_LL` + `WH_KEYBOARD_LL` 全局低级钩子 + WM_TIMER 节流探测 + 分层高亮窗,
//! 全部跑在**调用线程**上 (本面板经 `spawn_background` 的线程池线程调用, 阻塞至
//! 会话结束)。探测 (WindowFromPoint/取标题类名/进程映像) 只发生在 WM_TIMER 与
//! 提交处理里, 绝不进钩子回调。
//!
//! 两条铁律 (旧版文档原样继承, 决定成败):
//! * **铁律1 —— 钩子回调严格 O(1)**: 回调只写原子坐标/标志 + `PostThreadMessage`
//!   后立即 `CallNextHookEx`; 绝不在回调里探测 (跨进程消息遇挂死程序阻塞 ≈1s,
//!   超 LowLevelHooksTimeout 300ms 被系统静默摘钩 + 拖慢全局鼠标)。
//! * **铁律2 —— 钩子绝不泄漏**: 会话资源全部收进 [`PickGuard`], RAII Drop 兜底
//!   (panic / 早返回 / 正常退出统一走同一清理序列); 终止路径先「有界排空」配对
//!   UP (≤500ms) 再摘钩, 防孤立 RBUTTONUP 在目标窗口弹残留上下文菜单夺焦
//!   (旧版 H-1: 右键取消 100% 复现过的坑)。
//!
//! ## 与旧版的**有意**差异 (架构对齐, 非省略)
//! * **无 owner.Closed 取消联动**: 旧 Avalonia 关窗不退进程, 须显式联动; 本面板
//!   主窗口关闭 = `run_component` 返回 = 进程退出 = OS 直接回收钩子与窗口,
//!   天然无泄漏路径。
//! * **无诊断日志层** (旧 PickLog): 旧版证据链为排查拾取失灵而建; 移植版交互面
//!   更窄 (失败必有 1081/1082 提示), 不引入文件 IO, 模块零副作用。
//! * 会话状态用**静态原子**而非实例字段: `extern "system"` 钩子/定时器回调无法
//!   捕获环境; 全部状态单写单读 (钩子写、泵线程读), 原子足够, 无锁。
//!
//! ## 可移植性
//! 仅依赖 `windows-sys` + std, 不 import 任何 reactor 类型; 上层 (app) 只接触
//! [`PickOutcome`] 与 [`MatchKind`], Win32 细节不出本模块。

// 理由: 全模块即 Win32 FFI 通道 (钩子/高亮窗/光标/进程探测), 封装面已收敛到本文件;
// 单测覆盖纯逻辑部分, 铁律1/2 的安全论证见模块头注。
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{
    COLORREF, CloseHandle, ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, HANDLE, HWND, LPARAM,
    LRESULT, POINT, RECT,
};
use windows_sys::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, RGN_DIFF, SetWindowRgn,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CopyImage, CreateWindowExW, DefWindowProcW, DestroyCursor, DestroyWindow,
    DispatchMessageW, EnumChildWindows, GA_ROOT, GetAncestor, GetClassNameW, GetMessageW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HCURSOR, HHOOK,
    KBDLLHOOKSTRUCT, KillTimer, LoadCursorW, MSG, MSLLHOOKSTRUCT, PM_REMOVE, PeekMessageW,
    PostThreadMessageW, RegisterClassW, SPI_SETCURSORS, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SetLayeredWindowAttributes, SetSystemCursor, SetTimer, SetWindowPos, SetWindowsHookExW,
    ShowWindow, SystemParametersInfoW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WindowFromPoint,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDC_CROSS, IMAGE_CURSOR, LR_COPYRETURNORG, LWA_ALPHA, OCR_NORMAL, SW_HIDE,
};

/// 泵线程自定义消息: 提交 (左键)。
const PUMP_COMMIT: u32 = WM_APP + 1;
/// 泵线程自定义消息: 取消 (Esc / 右键)。
const PUMP_CANCEL: u32 = WM_APP + 2;
/// 高亮框空心边宽 (px, 与旧版 FrameThickness 一致)。
const FRAME_THICKNESS: i32 = 3;
/// 高亮窗类名 "KF.Pick" (UTF-16 + NUL; 进程内注册一次)。
const CLASS_NAME: &[u16] = &[
    0x004B, 0x0046, 0x002E, 0x0050, 0x0069, 0x0063, 0x006B, 0x0000,
];
/// 探测节流下限 (ms)。
const THROTTLE_MIN: u32 = 16;
/// root hwnd 未变时完整探测的时间兜底 (ms): 让窗口自身移动/缩放时高亮 ≤500ms 跟上。
const PROBE_FALLBACK_MS: u64 = 500;
/// 配对 UP 排空预算 (ms, 旧版 M-A: 300→500)。
const DRAIN_BUDGET_MS: u64 = 500;

// ---------------------------------------------------------------- 结果类型

/// 拾取结果状态 (旧 `WindowPickStatus` 的外泄子集; `FirstNoWindow` 细节已并入
/// `NoWindow` —— 本版首次无窗口即终止会话, 无「存活重瞄」路径)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickStatus {
    /// 成功拾取 (`text` 已按 `MatchKind` 格式化, 可直接回填)。
    Success,
    /// 用户取消 (Esc / 右键) 或钩子安装失败。
    Cancelled,
    /// 目标进程拒绝访问 (非提权运行探高完整性窗口), 需明确提示而非静默错值。
    AccessDenied,
    /// 提交点无窗口 / 命中自身进程 (旧 M3: 首次即终止会话并外泄)。
    NoWindow,
}

/// 拾取结果 (UI 层唯一接触面): 成功时 `text` 非空, 其余态 `text` 为空串。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickOutcome {
    pub status: PickStatus,
    pub text: String,
}

impl PickOutcome {
    fn cancelled() -> Self {
        Self {
            status: PickStatus::Cancelled,
            text: String::new(),
        }
    }
}

/// 窗口标识符格式 (旧 `WindowMatchKind`; 当前 UI 只用 `TitleAndExe`,
/// 全量保留以保持与旧版行为等价, 供后续「窗口分组」等入口复用)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchKind {
    Exe,
    FullPath,
    Class,
    Title,
    Pid,
    HwndId,
    /// 组合: 窗口名 + 进程名 (动作编辑面板默认)。
    #[default]
    TitleAndExe,
}

/// 拾取到的窗口描述 (会话内部使用; 不外泄 —— HWND 非线程安全类型)。
struct Descriptor {
    hwnd: HWND,
    title: String,
    class_name: String,
    pid: u32,
    exe_name: String,
    full_path: String,
    bounds: RECT,
}

/// 按 `kind` 格式化窗口标识符 (旧 `WindowMatchFormatter.Format` 逐分支移植)。
/// 私有: Descriptor 不外泄 (含 HWND), 结果经 [`PickOutcome.text`] 给 UI 层。
fn format_match(kind: MatchKind, d: &Descriptor) -> String {
    match kind {
        MatchKind::Exe => format!("ahk_exe {}", d.exe_name),
        MatchKind::FullPath => format!("ahk_exe {}", d.full_path),
        MatchKind::Class => format!("ahk_class {}", d.class_name),
        MatchKind::Title => d.title.clone(),
        MatchKind::Pid => format!("ahk_pid {}", d.pid),
        MatchKind::HwndId => format!("ahk_id {:?}", d.hwnd as isize),
        MatchKind::TitleAndExe => format_title_and_exe(d),
    }
}

fn format_title_and_exe(d: &Descriptor) -> String {
    let title = d.title.trim();
    if title.is_empty() {
        format!("ahk_exe {}", d.exe_name)
    } else {
        format!("{title} ahk_exe {}", d.exe_name)
    }
}

/// 高亮框颜色 (RGB; COLORREF 转换有单测锁定字节序)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

// ---------------------------------------------------------------- 会话状态 (静态原子)

// 🔴 钩子/定时器回调无法捕获环境 ⇒ 会话交互状态全部静态化。写读约定:
//    钩子回调只写 (坐标/标志), 泵线程 (timer + 主循环) 只读或 swap —— 单写单读,
//    原子即可。会话开始时统一复位 (防上一会话异常残留)。

static PUMP_TID: AtomicU32 = AtomicU32::new(0);
static CUR_X: AtomicI32 = AtomicI32::new(0);
static CUR_Y: AtomicI32 = AtomicI32::new(0);
static COMMIT_X: AtomicI32 = AtomicI32::new(0);
static COMMIT_Y: AtomicI32 = AtomicI32::new(0);
static DIRTY: AtomicBool = AtomicBool::new(false);
static SWALLOW_L_UP: AtomicBool = AtomicBool::new(false);
static SWALLOW_R_UP: AtomicBool = AtomicBool::new(false);
static SWALLOW_ESC_UP: AtomicBool = AtomicBool::new(false);
/// 钩子内 PostThreadMessage 失败闩锁: timer 检测补投 (旧 M2)。
static COMMIT_POST_FAILED: AtomicBool = AtomicBool::new(false);
/// 排空中标志: timer 直接 return, 不再探测/刷新高亮 (旧 L-A)。
static DRAINING: AtomicBool = AtomicBool::new(false);
/// 防重入 (旧 Shared._busy): 已有会话在跑时再次 pick 直接返回 Cancelled。
static BUSY: AtomicBool = AtomicBool::new(false);
/// 探测短路缓存: 上次完整探测的 root hwnd (0 = 无)。
static LAST_ROOT: AtomicIsize = AtomicIsize::new(0);
/// 上次完整探测时刻 (ms; 与 LAST_ROOT 配对使用)。
static LAST_PROBE_MS: AtomicU64 = AtomicU64::new(0);
/// 高亮窗句柄 (timer/清理需要; extern 回调无法捕获)。
static HIGHLIGHT_HWND: AtomicIsize = AtomicIsize::new(0);

// 高亮窗类与画刷 (进程内注册一次, 旧 EnsureHighlightClass 同款)。
static CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();

/// 读取毫秒时钟 (探测时间兜底用; GetTickCount64 的 std 等价物)。
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

// ---------------------------------------------------------------- 入口

/// 发起一次拾取会话 (默认 `TitleAndExe` 格式)。阻塞至会话结束。
pub fn pick(highlight: Rgb) -> PickOutcome {
    pick_with(MatchKind::TitleAndExe, highlight, 60)
}

/// 发起一次拾取会话 (完整参数版)。
///
/// `throttle_ms` = 高亮探测节流 (自动夹到 ≥[`THROTTLE_MIN`])。
pub fn pick_with(kind: MatchKind, highlight: Rgb, throttle_ms: u32) -> PickOutcome {
    // 防重入: 已有会话在跑则立即返回 Cancelled (不排队、不并发装钩子)。
    if BUSY.swap(true, Ordering::SeqCst) {
        return PickOutcome::cancelled();
    }
    // panic 兜底: 会话内 panic 也要复位 BUSY + 恢复现场 (guard 在 run_session 内
    // 已按栈展开清理; catch_unwind 保证外层标志位不残留)。
    let outcome = std::panic::catch_unwind(|| run_session(kind, highlight, throttle_ms))
        .unwrap_or_else(|_| PickOutcome::cancelled());
    BUSY.store(false, Ordering::SeqCst);
    outcome
}

/// 单次拾取会话主体: 装钩子 → 高亮/光标 → 消息泵 → 排空 → 清理 (guard)。
fn run_session(kind: MatchKind, highlight: Rgb, throttle_ms: u32) -> PickOutcome {
    // 会话状态复位 (上一会话可能异常残留)。
    DIRTY.store(false, Ordering::Relaxed);
    SWALLOW_L_UP.store(false, Ordering::Relaxed);
    SWALLOW_R_UP.store(false, Ordering::Relaxed);
    SWALLOW_ESC_UP.store(false, Ordering::Relaxed);
    COMMIT_POST_FAILED.store(false, Ordering::Relaxed);
    DRAINING.store(false, Ordering::Relaxed);
    LAST_ROOT.store(0, Ordering::Relaxed);
    LAST_PROBE_MS.store(0, Ordering::Relaxed);
    HIGHLIGHT_HWND.store(0, Ordering::Relaxed);
    PATH_CACHE.with(|cache| cache.borrow_mut().clear());

    let mut guard = PickGuard::default();

    unsafe {
        // 先 PeekMessage(PM_NOREMOVE) 强制创建本线程消息队列, 再发布 tid —— 保证
        // 任何线程读到非零 tid 时队列必已存在, PostThreadMessage 不会因队列未建
        // 而丢信号 (旧 High#3)。
        let mut probe_msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut probe_msg, std::ptr::null_mut(), 0, 0, 0);
        PUMP_TID.store(GetCurrentThreadId(), Ordering::SeqCst);

        // 安装全局低级钩子 (本线程已有消息泵, LL 钩子回调经系统投递到本线程)。
        let hmod = GetModuleHandleW(std::ptr::null());
        guard.mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0);
        guard.kb_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), hmod, 0);
        if guard.mouse_hook.is_null() || guard.kb_hook.is_null() {
            // 钩子安装失败: 无法拾取, 按取消处理 (guard 会摘掉可能已装上的那个)。
            return PickOutcome::cancelled();
        }

        // 高亮窗句柄由 `create_highlight` 存入全局 `HIGHLIGHT_HWND`（Drop 从全局取用清理）。
        create_highlight(highlight);
        guard.cursor_applied = apply_cross_cursor().is_some();

        let throttle = throttle_ms.max(THROTTLE_MIN);
        guard.timer = SetTimer(std::ptr::null_mut(), 1, throttle, Some(timer_proc));
    }

    // 消息泵: 提交/取消消息驱动状态机, WM_TIMER 驱动节流探测。
    let mut outcome: Option<PickOutcome> = None;
    unsafe {
        loop {
            let mut msg: MSG = std::mem::zeroed();
            let got = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if got <= 0 {
                break; // WM_QUIT / 错误
            }
            if msg.message == PUMP_CANCEL {
                outcome = Some(PickOutcome::cancelled());
                break;
            }
            if msg.message == PUMP_COMMIT {
                outcome = Some(handle_commit(kind));
                // M3: 首次 NoWindow 即终止会话 (outcome 必已就绪), 无「存活重瞄」。
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // guard.drop: 先排空配对 UP, 再摘钩/杀定时器/销毁高亮窗/恢复光标 (铁律2)。
    drop(guard);
    outcome.unwrap_or_else(PickOutcome::cancelled)
}

/// 向泵线程投递消息 (带 5 次有界重试; 旧 PostToPump)。
fn post_to_pump(msg: u32) -> bool {
    let tid = PUMP_TID.load(Ordering::SeqCst);
    if tid == 0 {
        return false;
    }
    for _ in 0..5 {
        unsafe {
            if PostThreadMessageW(tid, msg, 0, 0) != 0 {
                return true;
            }
        }
        sleep(Duration::from_millis(1));
    }
    false
}

// ---------------------------------------------------------------- 提交 / 取消

/// 提交处理: 探测提交点 → 按格式产出结果 (旧 HandleCommit)。
fn handle_commit(kind: MatchKind) -> PickOutcome {
    let x = COMMIT_X.load(Ordering::Relaxed);
    let y = COMMIT_Y.load(Ordering::Relaxed);
    match Probe.probe_at(x, y) {
        Ok(d) => PickOutcome {
            status: PickStatus::Success,
            text: format_match(kind, &d),
        },
        Err(PickStatus::AccessDenied) => PickOutcome {
            status: PickStatus::AccessDenied,
            text: String::new(),
        },
        Err(_) => PickOutcome {
            status: PickStatus::NoWindow,
            text: String::new(),
        },
    }
}

// ---------------------------------------------------------------- 钩子回调 (铁律1: 严格 O(1))

unsafe extern "system" fn mouse_proc(ncode: i32, wparam: usize, lparam: LPARAM) -> LRESULT {
    // 钩子回调体内只做原子写 + post (铁律1); unsafe 块 = 解引用系统给的 lParam。
    unsafe {
        if ncode >= 0 {
            let data = &*(lparam as *const MSLLHOOKSTRUCT);
            match wparam as u32 {
                WM_MOUSEMOVE => {
                    CUR_X.store(data.pt.x, Ordering::Relaxed);
                    CUR_Y.store(data.pt.y, Ordering::Relaxed);
                    DIRTY.store(true, Ordering::Relaxed);
                }
                WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                    CUR_X.store(data.pt.x, Ordering::Relaxed);
                    CUR_Y.store(data.pt.y, Ordering::Relaxed);
                    COMMIT_X.store(data.pt.x, Ordering::Relaxed);
                    COMMIT_Y.store(data.pt.y, Ordering::Relaxed);
                    // Medium#4: 标记吞配对 UP, 防向目标投递不成对事件。
                    SWALLOW_L_UP.store(true, Ordering::Relaxed);
                    if !post_to_pump(PUMP_COMMIT) {
                        COMMIT_POST_FAILED.store(true, Ordering::Relaxed);
                    }
                    return 1; // 吞掉这次 down (防误激活目标程序)
                }
                WM_LBUTTONUP => {
                    if SWALLOW_L_UP.swap(false, Ordering::Relaxed) {
                        return 1;
                    }
                }
                WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => {
                    // 防孤立 RBUTTONUP 弹残留上下文菜单夺焦。
                    SWALLOW_R_UP.store(true, Ordering::Relaxed);
                    // 与旧版差异: 取消消息不设 timer 补投闩锁 (旧 L-B) —— 取消丢失时
                    // 表现为「右键无效」, Esc/左键仍能结束会话, 无资源泄漏。
                    let _ = post_to_pump(PUMP_CANCEL);
                    return 1;
                }
                WM_RBUTTONUP if SWALLOW_R_UP.swap(false, Ordering::Relaxed) => {
                    return 1;
                }
                _ => {}
            }
        }
        CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam)
    }
}

unsafe extern "system" fn kb_proc(ncode: i32, wparam: usize, lparam: LPARAM) -> LRESULT {
    unsafe {
        if ncode >= 0 {
            let data = &*(lparam as *const KBDLLHOOKSTRUCT);
            if data.vkCode == VK_ESCAPE as u32 {
                match wparam as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        SWALLOW_ESC_UP.store(true, Ordering::Relaxed);
                        let _ = post_to_pump(PUMP_CANCEL);
                        return 1;
                    }
                    WM_KEYUP | WM_SYSKEYUP if SWALLOW_ESC_UP.swap(false, Ordering::Relaxed) => {
                        return 1;
                    }
                    _ => {}
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam)
    }
}

// ---------------------------------------------------------------- 定时器 (节流探测)

unsafe extern "system" fn timer_proc(_hwnd: HWND, _msg: u32, _id: usize, _tick: u32) {
    // 排空中: 不探测、不刷新高亮 (高亮已在排空开头隐藏)。
    if DRAINING.load(Ordering::Relaxed) {
        return;
    }
    // 提交消息补投 (旧 M2: 钩子内 post 失败的闩锁自愈)。
    if COMMIT_POST_FAILED.load(Ordering::Relaxed) && post_to_pump(PUMP_COMMIT) {
        COMMIT_POST_FAILED.store(false, Ordering::Relaxed);
    }
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let x = CUR_X.load(Ordering::Relaxed);
    let y = CUR_Y.load(Ordering::Relaxed);
    // High#1: 先 WindowFromPoint + GA_ROOT, root 未变且 <500ms 则短路 —— 不跑整条
    // 跨进程链 (标题/类名/进程映像), 杜绝鼠标每移一格就重探的输入迟滞。
    let (root, now) = unsafe {
        let hit = WindowFromPoint(POINT { x, y });
        let root = if hit.is_null() {
            0
        } else {
            GetAncestor(hit, GA_ROOT) as isize
        };
        (root, now_ms())
    };
    if root != 0
        && root == LAST_ROOT.load(Ordering::Relaxed)
        && now - LAST_PROBE_MS.load(Ordering::Relaxed) < PROBE_FALLBACK_MS
    {
        return;
    }
    match Probe.probe_at(x, y) {
        Ok(d) => update_highlight(d.bounds),
        // 探测失败 (无窗口/自身/拒绝) 一律收高亮 (旧版探测异常同路径)。
        Err(_) => hide_highlight(),
    }
    LAST_ROOT.store(root, Ordering::Relaxed);
    LAST_PROBE_MS.store(now, Ordering::Relaxed);
}

// ---------------------------------------------------------------- 高亮窗

/// 注册高亮窗类 (进程内一次) 并创建分层高亮窗。失败返回空句柄 (无高亮, 钩子仍可用)。
fn create_highlight(highlight: Rgb) -> HWND {
    unsafe {
        CLASS_REGISTERED.call_once(|| {
            let brush = CreateSolidBrush(rgb_to_colorref(highlight));
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(def_wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: brush,
                lpszMenuName: std::ptr::null(),
                lpszClassName: CLASS_NAME.as_ptr(),
            };
            // 失败 (已注册/受限) 静默: CreateWindowExW 会随之失败。
            RegisterClassW(&wc);
        });
        let hwnd = CreateWindowExW(
            WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            CLASS_NAME.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if !hwnd.is_null() {
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            HIGHLIGHT_HWND.store(hwnd as isize, Ordering::SeqCst);
        }
        hwnd
    }
}

unsafe extern "system" fn def_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 更新高亮框位置/尺寸 (尺寸变化才重建空心区域; 旧 UpdateHighlight/ApplyFrameRegion)。
fn update_highlight(bounds: RECT) {
    let hwnd = HIGHLIGHT_HWND.load(Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return;
    }
    let x = bounds.left;
    let y = bounds.top;
    let w = (bounds.right - bounds.left).max(1);
    let h = (bounds.bottom - bounds.top).max(1);
    unsafe {
        apply_frame_region(hwnd, w, h);
        SetWindowPos(
            hwnd,
            -1isize as HWND,
            x,
            y,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// 空心框区域: 外矩形 − 内矩形 (RGN_DIFF)。
///
/// # Panics / Safety
/// `hwnd` 必须是本线程创建的合法窗口 (Win32 窗口句柄线程亲和)。
unsafe fn apply_frame_region(hwnd: HWND, w: i32, h: i32) {
    unsafe {
        let outer = CreateRectRgn(0, 0, w, h);
        let inner = CreateRectRgn(
            FRAME_THICKNESS,
            FRAME_THICKNESS,
            FRAME_THICKNESS.max(w - FRAME_THICKNESS),
            FRAME_THICKNESS.max(h - FRAME_THICKNESS),
        );
        CombineRgn(outer, outer, inner, RGN_DIFF);
        DeleteObject(inner);
        SetWindowRgn(hwnd, outer, 1);
        // SetWindowRgn 成功后系统接管 region 所有权, outer 不再手动 DeleteObject。
    }
}

fn hide_highlight() {
    let hwnd = HIGHLIGHT_HWND.load(Ordering::Relaxed) as HWND;
    if !hwnd.is_null() {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

// ---------------------------------------------------------------- 系统光标 (准星)

/// 把系统箭头光标临时替换为十字 (旧 ApplySystemCrossCursor):
/// LoadCursor 取的是**共享**系统句柄, 绝不能直接交给 SetSystemCursor (它会销毁
/// 传入句柄) —— 先 CopyImage 复制私有副本, 加固不变量与旧 L-1 相同。
unsafe fn apply_cross_cursor() -> Option<()> {
    unsafe {
        let cross = LoadCursorW(std::ptr::null_mut(), IDC_CROSS);
        if cross.is_null() {
            return None;
        }
        let copy = CopyImage(cross, IMAGE_CURSOR, 0, 0, LR_COPYRETURNORG) as HCURSOR;
        if copy.is_null() || copy == cross {
            return None;
        }
        if SetSystemCursor(copy, OCR_NORMAL) == 0 {
            DestroyCursor(copy);
            return None;
        }
        Some(()) // 成功后 copy 由系统接管
    }
}

/// 无条件幂等重载用户既有光标方案 (恢复箭头; 旧 RestoreSystemCursors)。
unsafe fn restore_cursors() {
    unsafe {
        SystemParametersInfoW(SPI_SETCURSORS, 0, std::ptr::null_mut(), 0);
    }
}

// ---------------------------------------------------------------- 清理 (铁律2)

/// 会话资源的 RAII 载体: Drop 按固定顺序清理 —— 排空配对 UP → 摘钩 → 杀定时器 →
/// 销毁高亮窗 → 恢复光标。任何退出路径 (正常/panic/早返回) 都经过同一序列。
#[derive(Default)]
struct PickGuard {
    mouse_hook: HHOOK,
    kb_hook: HHOOK,
    timer: usize,
    cursor_applied: bool,
}

impl Drop for PickGuard {
    fn drop(&mut self) {
        unsafe {
            // H-1: 终止路径吞配对 UP 的**有界**排空, 覆盖全部退出路径 (排空必须
            // 发生在摘钩之前 —— 目的所在); PeekMessage 轮询保证有界性与定时器成败
            // 无关 (旧 Low-1)。
            self.drain_paired_up();
            if !self.mouse_hook.is_null() {
                UnhookWindowsHookEx(self.mouse_hook);
                self.mouse_hook = std::ptr::null_mut();
            }
            if !self.kb_hook.is_null() {
                UnhookWindowsHookEx(self.kb_hook);
                self.kb_hook = std::ptr::null_mut();
            }
            if self.timer != 0 {
                KillTimer(std::ptr::null_mut(), self.timer);
                self.timer = 0;
            }
            let hwnd = HIGHLIGHT_HWND.swap(0, Ordering::SeqCst) as HWND;
            if !hwnd.is_null() {
                DestroyWindow(hwnd); // 于创建它的泵线程销毁
            }
            if self.cursor_applied {
                restore_cursors();
            }
        }
    }
}

impl PickGuard {
    /// 排空已吞 DOWN 的配对 UP (有界): PeekMessage 轮询替代 GetMessage 阻塞,
    /// 期间 DRAINING 让 timer 零探测 (旧 DrainPairedUp / L-A / H-A)。
    fn drain_paired_up(&mut self) {
        let pending = || {
            SWALLOW_L_UP.load(Ordering::Relaxed)
                || SWALLOW_R_UP.load(Ordering::Relaxed)
                || SWALLOW_ESC_UP.load(Ordering::Relaxed)
        };
        if !pending() {
            return;
        }
        DRAINING.store(true, Ordering::Relaxed);
        hide_highlight();
        let deadline = now_ms() + DRAIN_BUDGET_MS;
        while pending() && now_ms() < deadline {
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        break;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else {
                    sleep(Duration::from_millis(1));
                }
            }
        }
        DRAINING.store(false, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------- 探测 (窗口描述)

// pid → 进程映像路径缓存 (仅泵线程使用; 会话开始时清空, 防 pid 复用错值)。
thread_local! {
    static PATH_CACHE: std::cell::RefCell<HashMap<u32, String>> =
        std::cell::RefCell::new(HashMap::new());
}

/// UWP 壳修正的枚举上下文 (EnumChildWindows 回调无法捕获环境 ⇒ thread_local)。
struct UwpEnum {
    afh_pid: u32,
    found: Option<Result<(u32, String), PickStatus>>,
}

thread_local! {
    static UWP_ENUM: std::cell::RefCell<Option<UwpEnum>> = const { std::cell::RefCell::new(None) };
}

/// 窗口探测器 (旧 Win32WindowProbe 的移植; 无状态, 缓存在线程局部)。
struct Probe;

impl Probe {
    /// 探测屏幕坐标 (物理像素) 指向的顶层窗口。
    fn probe_at(&mut self, x: i32, y: i32) -> Result<Descriptor, PickStatus> {
        unsafe {
            let hit = WindowFromPoint(POINT { x, y });
            if hit.is_null() {
                return Err(PickStatus::NoWindow);
            }
            let hwnd = GetAncestor(hit, GA_ROOT);
            if hwnd.is_null() {
                return Err(PickStatus::NoWindow);
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 || pid == std::process::id() {
                // 命中自身进程 = 拾取面板自己的窗口, 与无窗口同语义 (旧 M3)。
                return Err(PickStatus::NoWindow);
            }
            let title = window_text(hwnd);
            let class_name = window_class(hwnd);
            let mut full_path = resolve_process_image(pid)?;

            // UWP 壳修正: ApplicationFrameHost 承载的 UWP 窗口, 真实宿主在子窗口
            // 的另一个进程里 (旧 TryResolveUwpChild)。
            if file_name_of(&full_path).eq_ignore_ascii_case("ApplicationFrameHost.exe") {
                match resolve_uwp_child(hwnd, pid) {
                    Ok((child_pid, child_path)) => {
                        pid = child_pid;
                        full_path = child_path;
                    }
                    Err(status) => return Err(status),
                }
            }

            let mut bounds = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetWindowRect(hwnd, &mut bounds);
            Ok(Descriptor {
                hwnd,
                title,
                class_name,
                pid,
                exe_name: file_name_of(&full_path),
                full_path,
                bounds,
            })
        }
    }
}

/// 进程映像路径 (带 pid 缓存); 拒绝访问 → `AccessDenied` (旧 ResolveProcessImage)。
fn resolve_process_image(pid: u32) -> Result<String, PickStatus> {
    if let Some(cached) = PATH_CACHE.with(|cache| cache.borrow().get(&pid).cloned()) {
        return Ok(cached);
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // GetLastError 区分: 拒绝访问 → 明确提示; 其余 → 按无窗口。
            return Err(match windows_sys::Win32::Foundation::GetLastError() {
                ERROR_ACCESS_DENIED => PickStatus::AccessDenied,
                _ => PickStatus::NoWindow,
            });
        }
        let path = query_full_image_name(handle);
        CloseHandle(handle);
        if path.is_empty() {
            return Err(PickStatus::NoWindow);
        }
        PATH_CACHE.with(|cache| cache.borrow_mut().insert(pid, path.clone()));
        Ok(path)
    }
}

/// 查询进程完整映像名 (缓冲不足自动扩容, 上限 32K; 旧 QueryFullImageName)。
unsafe fn query_full_image_name(handle: HANDLE) -> String {
    unsafe {
        let mut capacity = 1024usize;
        loop {
            let mut buf = vec![0u16; capacity];
            let mut size = capacity as u32;
            if QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) != 0 {
                return String::from_utf16_lossy(&buf[..size as usize]);
            }
            let err = windows_sys::Win32::Foundation::GetLastError();
            if err == ERROR_INSUFFICIENT_BUFFER && capacity < 32768 {
                capacity = (capacity * 2).min(32768);
                continue;
            }
            return String::new();
        }
    }
}

/// UWP 壳修正: 枚举子窗口找「pid 不同于 AFH 且映像可解析」的宿主
/// (拒绝访问 → 上抛 AccessDenied; 找不到 → 保持无窗口语义)。
fn resolve_uwp_child(afh_hwnd: HWND, afh_pid: u32) -> Result<(u32, String), PickStatus> {
    UWP_ENUM.with(|cell| {
        *cell.borrow_mut() = Some(UwpEnum {
            afh_pid,
            found: None,
        });
    });
    unsafe {
        EnumChildWindows(afh_hwnd, Some(enum_child_proc), 0);
    }
    UWP_ENUM.with(|cell| {
        let mut slot = cell.borrow_mut();
        let state = slot.take().expect("UWP enum context must exist");
        match state.found {
            Some(Ok(pair)) => Ok(pair),
            Some(Err(status)) => Err(status),
            None => Err(PickStatus::NoWindow),
        }
    })
}

unsafe extern "system" fn enum_child_proc(child: HWND, _lparam: LPARAM) -> i32 {
    unsafe {
        UWP_ENUM.with(|cell| {
            let mut slot = cell.borrow_mut();
            let Some(state) = slot.as_mut() else {
                return 0;
            };
            let mut child_pid: u32 = 0;
            GetWindowThreadProcessId(child, &mut child_pid);
            if child_pid == 0 || child_pid == state.afh_pid {
                return 1; // 继续枚举
            }
            match resolve_process_image(child_pid) {
                Ok(path) => {
                    state.found = Some(Ok((child_pid, path)));
                    0 // 停止
                }
                Err(PickStatus::AccessDenied) => {
                    state.found = Some(Err(PickStatus::AccessDenied));
                    0 // 停止
                }
                Err(_) => 1, // 该子窗口解析不了, 试下一个
            }
        })
    }
}

/// 窗口标题 (长度前置探测 + 定长缓冲; 空串合法)。
unsafe fn window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd).max(0) as usize;
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len + 1];
        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), (len + 1) as i32);
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..copied as usize])
    }
}

/// 窗口类名 (定长 256, Win32 类名上限)。
unsafe fn window_class(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let copied = GetClassNameW(hwnd, buf.as_mut_ptr(), 256);
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..copied as usize])
    }
}

/// 路径 → 文件名 (旧 Path.GetFileName 语义: 取最后一个分隔符之后)。
fn file_name_of(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}

/// `Rgb` → Win32 COLORREF (0x00BBGGRR; 单测锁定字节序)。
fn rgb_to_colorref(c: Rgb) -> COLORREF {
    (c.0 as u32) | ((c.1 as u32) << 8) | ((c.2 as u32) << 16)
}

// ---------------------------------------------------------------- 单测 (纯逻辑部分)

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(title: &str, exe: &str) -> Descriptor {
        Descriptor {
            hwnd: std::ptr::null_mut(),
            title: title.to_string(),
            class_name: "Cls".to_string(),
            pid: 42,
            exe_name: exe.to_string(),
            full_path: format!(r"C:\Apps\{exe}"),
            bounds: RECT {
                left: 0,
                top: 0,
                right: 100,
                bottom: 80,
            },
        }
    }

    /// COLORREF 字节序: 0x00BBGGRR (GDI 契约, 与 theme 侧 Rgb(R,G,B) 对位)。
    #[test]
    fn colorref_layout() {
        assert_eq!(rgb_to_colorref(Rgb(0x12, 0x34, 0x56)), 0x0056_3412);
    }

    /// 组合格式: 标题非空 = "标题 ahk_exe x.exe"; 标题空白 = "ahk_exe x.exe"。
    #[test]
    fn title_and_exe_format() {
        assert_eq!(
            format_match(MatchKind::TitleAndExe, &descriptor("记事本", "notepad.exe")),
            "记事本 ahk_exe notepad.exe"
        );
        assert_eq!(
            format_match(MatchKind::TitleAndExe, &descriptor("  ", "notepad.exe")),
            "ahk_exe notepad.exe"
        );
    }

    /// 其余格式分支 (ahk_* 前缀 / 裸标题), 锁定与旧版逐字一致。
    #[test]
    fn other_kinds_format() {
        let d = descriptor("Foo", "app.exe");
        assert_eq!(format_match(MatchKind::Exe, &d), "ahk_exe app.exe");
        assert_eq!(
            format_match(MatchKind::FullPath, &d),
            r"ahk_exe C:\Apps\app.exe"
        );
        assert_eq!(format_match(MatchKind::Class, &d), "ahk_class Cls");
        assert_eq!(format_match(MatchKind::Title, &d), "Foo");
        assert_eq!(format_match(MatchKind::Pid, &d), "ahk_pid 42");
        let with_hwnd = Descriptor {
            hwnd: 0xFF as HWND,
            ..descriptor("Foo", "app.exe")
        };
        assert_eq!(format_match(MatchKind::HwndId, &with_hwnd), "ahk_id 255");
    }

    /// 文件名提取: 正反斜杠混合路径都取末段 (UWP 修正的判定前提)。
    #[test]
    fn file_name_extraction() {
        assert_eq!(file_name_of(r"C:\a\b\App.exe"), "App.exe");
        assert_eq!(file_name_of("C:/a/b/App.exe"), "App.exe");
        assert_eq!(file_name_of("App.exe"), "App.exe");
    }
}
