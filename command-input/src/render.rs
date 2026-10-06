//! 渲染后端契约 (design C §2.3): 唯一渲染缝。core 侧零 Win32 类型 (hwnd 用 usize)。
//!
//! - v1 实现 = `win::backend_gdi` (GDI 画内容色 → `UpdateLayeredWindow` 逐像素 alpha 自合成;
//!   早期的整窗 `LWA_ALPHA` + 色键透明带已被 b9428ec/5e314dd 两轮替换);
//! - v2 扩展点 = DComp 后端 (D3D11+D2D1+DComp+DirectWrite+UIAnimation, 原链复刻,
//!   `ex_style_additions` 改声明 WS_EX_NOREDIRECTIONBITMAP(0x20_0000) → ex-style
//!   0x0820_0008 与原版全等)。两端互不引用, 壳不感知差异。
//!
//! 为什么 ex-style 由后端声明: R4 的三样式位中 TOPMOST/NOACTIVATE 是显示语义
//! (不可放弃, 壳固定持有 `config::WS_EX_BASE`); NOREDIRECTIONBITMAP 是渲染机制的
//! 绑定位 (spec 附录 C #11 允许条件性放弃), 该决定权归后端。
//! 为什么 fade_out 在 trait 上: 淡出机制与渲染载体强耦合
//! (GDI = layered alpha 步进; DComp = UIAnimation 挂 DComp 属性)。

use crate::results::ResultsState;
use crate::skin::Skin;

/// 一次绘制所需的全部状态 (壳在 WM_PAINT / 0x401 / 0x402 时构造)。
pub struct FrameState<'a> {
    /// UTF-16 码元序列 (UTF-8 化/布局由后端自行处理)
    pub text: &'a [u16],
    /// 皮肤 (R26: 构造期读一次)
    pub skin: &'a Skin,
    /// DPI (R12: 创建期定值, 不变量; 渲染缩放一律用它)
    pub dpi: f64,
    /// 窗口宽 (物理像素, 925 @125%)
    pub width_px: i32,
    /// 当前窗口高 (物理像素; 有结果列表时 = 基准高 + `geometry::list_extra_px`)
    pub height_px: i32,
    /// 基准窗口高 (无列表态, R11 创建期定案) —— 决定查询区高度与列表分隔线位置
    pub base_height_px: i32,
    /// 结果列表面板状态 (2026-10-04: 命令框向下延伸的列表)
    pub results: &'a ResultsState,
    /// 搜索徽标 (2026-10-04): Some(字形编号) = 查询区右侧绘制; None = 不绘制
    pub badge: Option<u32>,
}

/// 后端错误 (R29: 壳弹原版格式错误框并立即终止, 不静默带病运行)。
#[derive(Debug)]
pub struct BackendError {
    pub message: String,
    pub hresult: u32,
}

impl BackendError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hresult: 0,
        }
    }

    pub fn with_hresult(message: impl Into<String>, hresult: u32) -> Self {
        Self {
            message: message.into(),
            hresult,
        }
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.hresult != 0 {
            write!(f, "{} (HRESULT {:#X})", self.message, self.hresult)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

pub trait RenderBackend {
    /// 后端对 WS_EX_ 样式的追加位 (GDI → WS_EX_LAYERED 0x8_0000; DComp v2 →
    /// WS_EX_NOREDIRECTIONBITMAP 0x20_0000)。壳固定持有 TOPMOST|NOACTIVATE。
    fn ex_style_additions(&self) -> u32;

    /// 渲染设备初始化。壳在**首个 WM_PAINT / 0x401 预绘**时调用
    /// (对齐原版懒创建 R23: 0xA5B3 判空); 失败 → 壳按 R29 弹错误框并立即终止。
    fn init(&mut self, hwnd: usize, state: &FrameState) -> Result<(), BackendError>;

    /// 全窗口重绘 (WM_PAINT)。契约: 返回后窗口须处于已验证 (validated) 状态 (R23)。
    fn paint(&mut self, state: &FrameState) -> Result<(), BackendError>;

    /// 0x401 显示前的同步预绘 (GDI 重定向路径防首帧白闪; DComp 原生零闪)。
    /// 默认转发 paint。
    fn pre_show(&mut self, state: &FrameState) -> Result<(), BackendError> {
        self.paint(state)
    }

    /// 0x402 淡出。契约: **阻塞**当前 (窗口) 线程直到动画结束, 期间不取消息
    /// (复刻原版 Sleep(50) 轮询的线程阻塞语义, R15 —— 期间排队消息顺延);
    /// 返回后壳执行 ShowWindow(SW_HIDE)。
    fn fade_out(&mut self, duration_secs: f64);

    /// 隐藏完成后的回调 (GDI: 把预乘帧缓存增益从 0 复原, 保证 R10-5「引擎直接 WinShow
    /// 亦正常显示」; DComp 可 no-op)。design A §2.10: 复原必须发生在 SW_HIDE **之后**。
    fn on_hidden(&mut self) {}

    /// 窗口尺寸变化 (结果列表展开/收起): 重建形状区域 (SetWindowRgn) 与绘制裁剪区域。
    /// 壳在 SetWindowPos 之后、重绘之前调用; 默认 no-op (无形状概念的实现无需关心)。
    fn resize(&mut self, state: &FrameState) -> Result<(), BackendError> {
        let _ = state;
        Ok(())
    }

    /// 选择变化的**增量重绘** (2026-10-06 悬停卡顿第二轮): 只重绘 `rows` 里的
    /// 结果行 (0 基绝对下标) 并呈现, 其余像素不动 —— 悬停高亮是全应用最高频的
    /// 重绘, 全帧重画 (含全部行的 DrawTextW) 在此是纯浪费。默认实现 = 全量
    /// [`Self::paint`] (语义等价; 支持增量的后端自行覆写, 不支持的原样正确)。
    ///
    /// 契约: 调用前**行集未变** (可视窗口与上一帧相同 —— 由协议层守卫, 见
    /// `protocol::AppEvent::SetSelection`); 后端自行跳过不在可视窗口内的行。
    /// 行集/文本/徽标/几何任一变化都必须走 [`Self::paint`], 不经此路径。
    fn repaint_rows(&mut self, state: &FrameState, rows: &[usize]) -> Result<(), BackendError> {
        let _ = rows;
        self.paint(state)
    }
}
