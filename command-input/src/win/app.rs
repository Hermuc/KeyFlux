//! 进程装配壳 (design C §2.7 `app`):
//! dpi → COM → 皮肤 → 单实例 → 类注册 → 建窗 (初始隐藏) → 消息循环 (常驻)。
//!
//! - `#![windows_subsystem = "windows"]` (release): 独立 GUI 子进程, 无控制台交互 (R1);
//! - 后端选择: 环境变量 `CMDINPUT_BACKEND=gdi|dcomp`, 默认 gdi; dcomp 未编译进本二进制
//!   (v2 渐进补齐) 时回退 gdi —— 选择点已存在, 接入新后端不动装配流程;
//! - 进程级阴性 (R19): 无网络 / 无注册表 / 无钩子 / 无文件日志 / 不读命令行参数 (R6)。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::Com::{
    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, LoadCursorW, RegisterClassW, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HCURSOR, HICON, IDC_ARROW, MSG, WINDOW_EX_STYLE,
    WNDCLASSW, WS_POPUP,
};

use crate::config;
use crate::protocol::AppState;
use crate::skin;
use crate::win::audio::WinmmSound;
use crate::win::backend_gdi::GdiBackend;
use crate::win::dpi;
use crate::win::error;
use crate::win::resources;
use crate::win::single_instance;
use crate::win::wide;
use crate::win::wndproc::{wndproc, Shell};

/// 库形态入口: 阻塞消息循环直至 WM_QUIT, 返回进程退出码。
/// 嵌入方 (未来 Rust 化的宿主) 可在任意线程调用 —— 注意: 类名 / 互斥名为全局单例
/// 资源, 一进程只能内嵌一个实例 (design C §8 可嵌入性限制, 如实声明)。
pub fn run() -> i32 {
    // ① DPI awareness —— 实测必要前提 (design C §1 #8), 先于一切窗口 / DPI 查询
    dpi::make_process_dpi_aware();

    // ② R2: CoInitializeEx(NULL, 6) = APARTMENTTHREADED | DISABLE_OLE1DDE; 失败不建窗
    // SAFETY: CoInitializeEx 无指针入参，None = 默认并发模型（本线程初始化 STA）；
    // 返回 HRESULT 已显式判错并走 R29 致命路径。
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        if hr.is_err() {
            error::fatal(file!(), line!(), "CoInitializeEx failed", hr.0 as u32);
        }
    }

    // ③ R25/R27: 皮肤构造期读一次; 缺失 / 损坏 → "" → DEFAULT (fail-safe, 不阻断)
    let exe_dir = resources::exe_dir();
    let skin_text = std::fs::read_to_string(resources::skin_file(&exe_dir)).unwrap_or_default();
    let skin = skin::parse(&skin_text);

    // ④ R30: 命名互斥体 + ALREADY_EXISTS → 旧实例 WM_CLOSE → 照常接管
    single_instance::acquire_and_notify();

    // ⑤ R24: 音效后端 (winmm; 文件缺失静默)
    let sound = WinmmSound::new(&exe_dir);

    // ⑥ 渲染后端选择 (design C §2.7): 默认 gdi; CMDINPUT_BACKEND=dcomp 时回退 gdi
    //    (v2 DComp 后端未编译进本二进制; 接入后此处按枚举装配)
    let backend = select_backend();

    let shell = Box::new(Shell {
        state: AppState::new(skin.hide_animation_duration), // R26: hideAnimationDuration → R15
        skin,
        dpi: (config::DIP_BASE_DPI, config::DIP_BASE_DPI), // WM_CREATE 定案 (R11/R12)
        geom: Default::default(),
        cur_h: 0, // WM_CREATE 定案 (= geom.h; 有结果列表时 = geom.h + list_extra)
        backend: Box::new(backend),
        sound: Box::new(sound),
        inited: false,
        composing: false,
        notify_target: HWND::default(), // 0x406 的 wParam (结果交互回推目标)
    });

    // SAFETY: GetModuleHandleW(None) 取本进程主模块句柄，无指针入参；失败返回 Err 已处理。
    let hmodule: HMODULE = match unsafe { GetModuleHandleW(None) } {
        Ok(m) => m,
        Err(e) => error::fatal(
            file!(),
            line!(),
            &format!("GetModuleHandleW failed: {e}"),
            0,
        ),
    };

    // ⑦ R3: RegisterClassW (非 ExW, 无小图标 —— R37 裁定图标可省略)
    let class_name = wide(config::CLASS_NAME);
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW, // 类风格 = 3
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: HINSTANCE(hmodule.0),
        hIcon: HICON::default(), // 无图标 (R37)
        hCursor: load_arrow_cursor(),
        hbrBackground: HBRUSH::default(), // NULL: 全自绘, 不让系统擦除 (R23)
        lpszMenuName: PCWSTR::null(),
        lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
    };
    // SAFETY: `wc` 的指针字段（lpszClassName / lpfnWndProc / hCursor）都指向本函数内、
    // 调用期间存活的实体；RegisterClassW 只读 `wc`，返回 0 已判为失败。
    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        error::fatal(file!(), line!(), "RegisterClassW failed", 0);
    }

    // ⑧ R4: CreateWindowExW —— WS_POPUP; ex = TOPMOST|NOACTIVATE|后端位;
    //    标题 = " "; CW_USEDEFAULT×4 (创建后 WM_CREATE 一次定位, R11); 初始隐藏
    let title = wide(config::WINDOW_TITLE);
    let ex_style = WINDOW_EX_STYLE(config::WS_EX_BASE | shell.backend.ex_style_additions());
    // SAFETY: 类名 / 标题都是存活于本函数的 NUL 结尾宽字符串；lpParam 传入指向 `shell`
    // 的裸指针，随后立即 `mem::forget(shell)` 使该分配与窗口同生命周期（进程常驻），
    // WM_NCCREATE 依约取回同一指针 ⇒ 不存在悬垂。
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            PCWSTR::from_raw(class_name.as_ptr()),
            PCWSTR::from_raw(title.as_ptr()),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(HINSTANCE(hmodule.0)),
            Some(&*shell as *const Shell as *const core::ffi::c_void), // → WM_NCCREATE 锚点
        )
    };
    if hwnd.is_err() {
        error::fatal(
            file!(),
            line!(),
            &format!(
                "CreateWindowExW failed: {:?}",
                hwnd.as_ref().err().map(|e| e.to_string())
            ),
            0,
        );
    }
    // shell 有意泄漏: 指针已存入 GWLP_USERDATA, 与窗口同生命周期 (进程常驻, R1)
    std::mem::forget(shell);

    // ⑨ R1: GetMessageW → TranslateMessage → DispatchMessageW 常驻循环;
    //     TranslateMessage 必须 (物理键盘 / IME 文本全靠它转 WM_CHAR, R19);
    //     返回 0 (WM_QUIT) 退出; -1 (错误) 视为致命退出防死循环
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` 是本函数持有的可写 MSG，生命周期覆盖循环；hwnd=None + 过滤
        // (0,0) = 取本线程全部消息。返回 0 表示 WM_QUIT（退出），非 0 即有效消息。
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if !r.as_bool() {
            break;
        }
        // SAFETY: `msg` 已由上面的 GetMessageW 成功填充；两者只读它，不改所有权。
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    msg.wParam.0 as i32
}

/// R3: hCursor = 标准 IDC_ARROW。加载失败回落 NULL 光标 (不阻断建窗)。
fn load_arrow_cursor() -> HCURSOR {
    // SAFETY: LoadCursorW(None, IDC_ARROW) 取系统标准箭头光标，无指针入参；失败返回 Err，
    // 已用 unwrap_or_default 回落到空句柄（不阻断建窗）。
    unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() }
}

/// 渲染后端选择点 (design C §2.7): `CMDINPUT_BACKEND=gdi|dcomp`。
/// v1 仅内置 GDI 后端; "dcomp" 命名时回退 gdi (v2 接入后此处装配新后端)。
fn select_backend() -> GdiBackend {
    match std::env::var("CMDINPUT_BACKEND").as_deref() {
        Ok("dcomp") => {
            // v2 扩展点: DComp 后端 (D3D11+D2D1+DComp+DirectWrite) 接入后在此返回
            GdiBackend::new()
        }
        _ => GdiBackend::new(),
    }
}
