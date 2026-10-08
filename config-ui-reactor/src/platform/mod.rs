//! 平台/UI 原语层 —— **reactor 与 windows 原生互操作只允许出现在本模块**。
//!
//! 这是 Ports & Adapters 的「adapter」侧：上层（`app`/`services`）只依赖这里的
//! 抽象类型，从而把 reactor 0.x 的升级/替换影响面收敛到本层。
//!
//! Phase 1.5 实测结论（见 `09-Phase1.5-专项结论.md`）：
//! * 每个独立窗口都必须**各自**声明标题与视觉（主题/材质/尺寸），**不继承**父窗口；
//! * 自绘标题栏用 `TitleBar` 控件（`TitleBarSlot::{Content,RightHeader}`）；
//! * 弹窗现统一走 `ContentDialog`（2026-10 生产验证正常；旧「崩溃」结论不再成立，
//! * `open_window` 通道保留未用）。

use windows_reactor::{WindowBackdrop, WindowTheme, WindowVisuals};

/// UAC 提权启动（ShellExecuteW runas；自启计划任务命令 3/4 用）。
pub mod elevation;
/// 原生文件对话框（打开/保存；Win32 过滤器串拼接见 `single_filter`）。
pub mod file_dialog;
/// 随包字体（MiSans）进程内私有加载。
pub mod fonts;
/// 毛玻璃（材质）背板策略 —— 窗口材质的**单一决策点**（见模块文档）。
///
/// 2026-10-08 自 crate 根下沉至此（模块化审查 §3.4 + §4.1）：它本质是**窗口背板
/// 策略**，与平台强相关，且唯一的下层消费者就是本模块的 [`WindowSpec::with_glass`]。
/// 下沉前 `platform -> crate::glass` 是 `lib.rs` 分层规范明令禁止的**反向依赖**；
/// 下沉后该引用变成同模块引用，依赖方向自然合法（`theme -> platform::glass` 不变）。
pub mod glass;
/// Windows Job Object 封装（保证 GUI 死亡时连带回收后端子进程树）。
pub mod job;
/// 用系统默认关联程序打开路径（ShellExecuteW `open` 动词；「查看引擎日志」入口用）。
pub mod shell;
/// 窗口拾取「准星」会话（WH_MOUSE_LL/WH_KEYBOARD_LL + 高亮框 + 十字光标；移植自旧 Avalonia）。
pub mod window_picker;

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
    /// 按「毛玻璃开关」派生窗口背板材质：开 = [`glass`] 策略背板（当前
    /// Mica Alt），关 = 保持默认 Mica。策略详情与选型依据见 [`glass`] 模块文档。
    ///
    /// `GlassBackdrop` → reactor `WindowBackdrop` 枚举的映射只允许在本层出现
    /// （平台互操作边界），上层只传布尔语义。
    pub fn with_glass(mut self, glass_on: bool) -> Self {
        if glass_on {
            self.backdrop = match glass::current().backdrop {
                glass::GlassBackdrop::Solid => WindowBackdrop::None,
                glass::GlassBackdrop::Mica => WindowBackdrop::Mica,
                glass::GlassBackdrop::MicaAlt => WindowBackdrop::MicaAlt,
                glass::GlassBackdrop::Acrylic => WindowBackdrop::Acrylic,
            };
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
            // 每次打开面板都居中于所在显示器的工作区（任务栏感知）；vendor P8 通道，
            // 仅首次应用生效，用户拖动后的位置不会被后续发布覆盖。
            .centered(true)
    }
}
