//! 平台/UI 原语层 —— **reactor 与 windows 原生互操作只允许出现在本模块**。
//!
//! 这是 Ports & Adapters 的「adapter」侧：上层（`app`/`services`）只依赖这里的
//! 抽象类型，从而把 reactor 0.x 的升级/替换影响面收敛到本层。
//!
//! Phase 1.5 实测结论（见 `09-Phase1.5-专项结论.md`）：
//! * 每个独立窗口都必须**各自**声明标题与视觉（主题/材质/尺寸），**不继承**父窗口；
//! * 自绘标题栏用 `TitleBar` 控件（`TitleBarSlot::{Content,RightHeader}`）；
//! * 弹窗用 `ComponentContext::open_window` 独立窗口（`ContentDialog` 会崩溃）。

use windows_reactor::{WindowBackdrop, WindowTheme, WindowVisuals};

/// Windows Job Object 封装（保证 GUI 死亡时连带回收后端子进程树）。
pub mod file_dialog;
/// 随包字体（MiSans）进程内私有加载。
pub mod fonts;
pub mod job;

/// 一个窗口的声明式描述。所有开窗点都应经由它，避免逐处硬编码。
pub struct WindowSpec {
    pub title: String,
    pub theme: WindowTheme,
    pub backdrop: WindowBackdrop,
    /// 客户区尺寸（DIP），必须是有限正数（reactor 会 panic）。
    pub size: (f64, f64),
}

impl Default for WindowSpec {
    /// 主窗口默认：浅色（KeyFlux 强制浅色）+ Mica + **1200×760**（对齐旧 `MainWindow.axaml:11`）。
    fn default() -> Self {
        Self {
            title: "KeyFlux Settings".to_string(),
            theme: WindowTheme::Light,
            backdrop: WindowBackdrop::Mica,
            size: (1200.0, 760.0),
        }
    }
}

impl WindowSpec {
    /// 按「亚克力开关」派生窗口背景材质：开 = Acrylic（毛玻璃），关 = 保持既有 Mica。
    ///
    /// reactor 的 `WindowBackdrop` 枚举只允许在本层出现（平台互操作边界），上层只传布尔语义。
    pub fn with_acrylic(mut self, acrylic: bool) -> Self {
        if acrylic {
            self.backdrop = WindowBackdrop::Acrylic;
        }
        self
    }

    /// 转成 reactor 的窗口视觉声明。
    pub fn visuals(&self) -> WindowVisuals {
        assert!(
            self.size.0.is_finite()
                && self.size.0 > 0.0
                && self.size.1.is_finite()
                && self.size.1 > 0.0,
            "WindowSpec size must be finite and positive"
        );
        WindowVisuals::new()
            .backdrop(self.backdrop)
            .theme(self.theme)
            .client_size(self.size.0, self.size.1)
    }
}
