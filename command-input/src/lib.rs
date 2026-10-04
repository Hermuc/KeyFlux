//! KeyFlux-CommandInput 库形态 —— KeyFlux 命令输入框 (窗口类 `MyKeymap_Command_Input`)
//! 的 Rust 重写。部署为 `KeyFlux-CommandInput.exe` 即为引擎零改动的 drop-in 替换
//! (判据 = 窗口类名 + exe 名 + 消息语义 + 标题 " ", spec.md:19)。
//!
//! 模块划分 (design C §2, 单 crate 形态, bin 薄壳 + lib 模块):
//!   纯逻辑 (零 Win32, 可脱离窗口 cargo test):
//!     - `config`    全部契约常量与标定常数 (类名/互斥名/消息值/几何标定)
//!     - `skin`      皮肤 18 键解析 + 合成数学 (R26/R27/R28; 公式移植自参考实现)
//!     - `textbuf`   文本状态机 (R17/R22)
//!     - `geometry`  R11/R13/R28 几何公式 (截断取整; 闭环单测 925/200/497/300)
//!     - `easing`    R15 Accelerate-Decelerate 0.5/0.5
//!     - `protocol`  R14-R19/R22 事件分派状态机 (AppEvent → Command)
//!     - `results`   结果列表面板模型 + 0x406 载荷编解码 (命令框向下延伸的列表状态)
//!     - `compose`   逐像素合成数学 (白边/填充/阴影 → 预乘 RGBA)
//!     - `sound`     R24 触发点枚举 + SoundBackend trait
//!     - `render`    RenderBackend trait (唯一渲染缝; 零 Win32 类型)
//!   系统粘合 (windows crate 唯一出口):
//!     - `win`       dpi / error / resources / single_instance / audio /
//!                   backend_gdi (v1 渲染后端) / wndproc (协议翻译层) / app (装配壳)
//!
//! 行为规格: `D:\PortableApps\cmdinput-re\spec.md` (R1-R37; 24 must)。
//! 选定设计: 方案 C (模块化优先: RenderBackend trait + GDI 首版落地 + DComp 渐进补齐),
//! 见 `D:\PortableApps\cmdinput-re\design-C.md`; §K 读回通道 (R32-R36) 本轮不实现,
//! 扩展点 = wndproc 的 WM_NCCREATE 锚点与 textbuf 容量语义。

pub mod badge;
pub mod compose;
pub mod config;
pub mod easing;
pub mod geometry;
pub mod protocol;
pub mod render;
pub mod results;
pub mod skin;
pub mod sound;
pub mod textbuf;
pub mod win;

pub use geometry::FrameGeom;
pub use protocol::{on_event, AppEvent, AppState, Command};
pub use render::{BackendError, FrameState, RenderBackend};
pub use results::ResultsState;
pub use skin::{Skin, DEFAULT as DEFAULT_SKIN};
pub use textbuf::TextBuf;
