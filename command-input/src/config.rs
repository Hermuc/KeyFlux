//! 全部对外契约常量与标定常数, 集中一处 (任务工程要求: 常量集中在 config 模块)。
//!
//! ⚠ 以下「契约类」常量属二进制 ABI / 引擎对接面, 修改前必读 spec.md 对应条目:
//!   - `CLASS_NAME` / `WINDOW_TITLE` / `MUTEX_NAME` —— R3/R4/R30, 逐字符锁定;
//!   - `APP_SHOW` / `APP_HIDE` / `APP_CANCEL` / `WM_CHAR_VAL` —— R7, 绝对值锁定。
//! 「标定类」常数来自反汇编常量与参考实现实测口径 (spec R11/R13/R23/R28), 修改需活体对拍。

/// R3/R5: 窗口类名 (strings.txt:7596, 逐字节一致; 引擎按 `ahk_class` 过滤)。
pub const CLASS_NAME: &str = "MyKeymap_Command_Input";

/// R4/R19: 窗口标题 = 单个空格 (WM_GETTEXT 默认只返回它; 文本无读取通道)。
pub const WINDOW_TITLE: &str = " ";

/// R30: 命名互斥体 (strings.txt:7638, 含前缀文案逐字符)。
pub const MUTEX_NAME: &str = "Some unique string for your app: MyKeymap Command Input";

/// R7/R14: 0x401 = 显示 + 清空 + show 音效 (引擎 type9_keyflux.ahk WM_USER+1)。
pub const APP_SHOW: u32 = 0x0401;
/// R7/R15: 0x402 = 淡出隐藏 (不清空, 无音效)。
pub const APP_HIDE: u32 = 0x0402;
/// R7/R16: 0x403 = cancel 音效 + 立即隐藏 (不清空)。
pub const APP_CANCEL: u32 = 0x0403;
/// R7/R17: WM_CHAR = 唯一文本写入通道 (wParam 低 16 位 = 码元)。
pub const WM_CHAR_VAL: u32 = 0x0102;

// ---- 2026-10-04 协议扩展 (Rust 版自有; 引擎对接面) ----
/// 0x404 = 搜索激活: 摘除 WS_EX_NOACTIVATE + SetForegroundWindow + SetFocus
/// (IME 组合窗跟随本窗口, 上屏中文经 WM_CHAR 进入文本缓冲)。
pub const APP_SEARCH_ACTIVATE: u32 = 0x0404;
/// 0x405 = 查询 IME 组合态 (LRESULT 1 = 组合中, 0 = 否)。引擎回车语义判定用。
pub const APP_SEARCH_STATE: u32 = 0x0405;
/// WM_GETTEXT (0x000D) = 读回通道: 返回文本缓冲 (原版只返回标题空格)。
pub const WM_GETTEXT_VAL: u32 = 0x000D;
/// WM_GETTEXTLENGTH (0x000E) = 返回文本码元数。
pub const WM_GETTEXTLENGTH_VAL: u32 = 0x000E;
/// IME 组合消息 (焦点窗口原生收到; 只记标志, 其余交 DefWindowProc)。
pub const WM_IME_START: u32 = 0x010D;
pub const WM_IME_END: u32 = 0x010E;
/// WS_EX_NOACTIVATE 位。
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
/// GWL_EXSTYLE 索引。
pub const GWL_EXSTYLE_IDX: i32 = -20;

/// R17: WM_CHAR wParam 低 16 位 == 8 → 退格。
pub const CHAR_BACKSPACE: u16 = 8;
/// R17/R24: WM_CHAR ch == 0x20 (空格) → spaceKey.wav; 其余 → keydown.wav。
pub const CHAR_SPACE: u16 = 0x20;

// ---- R11 几何公式常数 (反汇编 .rdata 常量, 截断取整) ----
/// R11: W = int((windowWidth + 40.0) × dpiX / 96.0) —— +40.0 是固有 DIP 边距。
pub const WIDTH_MARGIN_DIP: f64 = 40.0;
/// R11: H = int(dpiY × 160.0 / 96.0) —— 高度固定 160 DIP, 皮肤无高度键。
pub const HEIGHT_DIP: f64 = 160.0;
/// R11: X = int((screenW − W) × 0.5)。
pub const SCREEN_CENTER_RATIO: f64 = 0.5;
/// R11 公式中的 96.0 DIP 基准。
pub const DIP_BASE_DPI: f64 = 96.0;

// ---- R13/R23/R28 渲染标定常数 (参考实现 EverythingQueryEdit.ahk 实测口径) ----
/// R13: 白框缩进 = 42px @125% → DIP 化 33.6 (band = round(33.6 × dpi / 96))。
/// 引擎锚点常数 margin := 42 (EverythingHost.ahk:111) 依赖此几何。
pub const BAND_INSET_DIP: f64 = 33.6;
/// R28: 网格间距 = 20 DIP 按 DPI 缩放 (@125% = 25px 实测)。
pub const GRID_STEP_DIP: f64 = 20.0;
/// R28: 网格间距下限 (参考实现 :144-145 的 step<8 → 8)。
pub const GRID_STEP_MIN_PX: i32 = 8;
/// R23/R28: 文字 = 粗体 44.0 DIP (DWRITE_FONT_WEIGHT_BOLD 44.0f)。
pub const FONT_HEIGHT_DIP: f64 = 44.0;
/// R23/参考实现 :69: 文字水平内边距 = round(白框高 × 0.14)。
pub const TEXT_PAD_RATIO: f64 = 0.14;
/// 参考实现 :48: bin/font/font.ttf 实测家族名 (用户更换命令字体后需同步)。
pub const FONT_FAMILY: &str = "更纱黑体 SC";

/// 渲染透明机制的色键 (design A 探针 V1 实证): #FF00FF, 与内容色域
/// (白/黑/#F7F8FC 网格/灰阶 AA) 恒不相交。COLORREF = 0x00BBGGRR。
pub const COLORKEY: u32 = 0x00FF00FF;

/// R4: 壳固定持有的 WS_EX_ 位 = WS_EX_TOPMOST(0x0800_0000) | WS_EX_NOACTIVATE(0x8)
/// —— 显示语义, 不可放弃 (R4/R9)。后端经 `RenderBackend::ex_style_additions()` 追加:
/// GDI 后端 += WS_EX_LAYERED(0x0008_0000) → 实际 0x0808_0008
/// (NOREDIRECTIONBITMAP 与自合成绑定, 按 spec 附录 C #11 明示许可放弃)。
pub const WS_EX_BASE: u32 = 0x0800_0000 | 0x8;

/// R14: 0x401 的 SetWindowPos flags = SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW = 0x51。
pub const SWP_SHOW_FLAGS: u32 = 0x51;
/// R11: 创建期 SetWindowPos flags = SWP_NOZORDER | SWP_NOACTIVATE = 0x14。
pub const SWP_CREATE_FLAGS: u32 = 0x14;

#[cfg(test)]
mod tests {
    use super::*;

    /// R3: 类名逐字节 (strings.txt:7596)。
    #[test]
    fn class_name_is_locked() {
        assert_eq!(CLASS_NAME, "MyKeymap_Command_Input");
        assert_eq!(CLASS_NAME.as_bytes(), b"MyKeymap_Command_Input");
    }

    /// R4/R19: 标题 = 单个空格。
    #[test]
    fn title_is_single_space() {
        assert_eq!(WINDOW_TITLE, " ");
        assert_eq!(WINDOW_TITLE.chars().count(), 1);
    }

    /// R30: 互斥名逐字符 (strings.txt:7638; 一个冒号一个空格都不能变)。
    #[test]
    fn mutex_name_is_locked() {
        assert_eq!(
            MUTEX_NAME,
            "Some unique string for your app: MyKeymap Command Input"
        );
    }

    /// R7: 消息常量绝对值 (引擎 type9_keyflux.ahk:26-30 / AbbrInput.ahk)。
    #[test]
    fn message_values_are_locked() {
        assert_eq!(APP_SHOW, 0x401);
        assert_eq!(APP_HIDE, 0x402);
        assert_eq!(APP_CANCEL, 0x403);
        assert_eq!(WM_CHAR_VAL, 0x102);
        assert_eq!(CHAR_BACKSPACE, 8);
        assert_eq!(CHAR_SPACE, 0x20);
    }

    /// R4/附录 C #11: GDI 后端实际 ex-style = 0x08080008 (design C §4 表)。
    #[test]
    fn gdi_ex_style_composition() {
        let gdi_addition: u32 = 0x0008_0000; // WS_EX_LAYERED (GdiBackend 声明位)
        assert_eq!(WS_EX_BASE | gdi_addition, 0x0808_0008);
        assert_eq!(WS_EX_BASE & !(0x0800_0000 | 0x8), 0); // 不含额外位
    }

    /// R14/R11: SetWindowPos flags 位值。
    #[test]
    fn swp_flags() {
        assert_eq!(SWP_SHOW_FLAGS, 0x51); // NOSIZE|NOACTIVATE|SHOWWINDOW
        assert_eq!(SWP_CREATE_FLAGS, 0x14); // NOZORDER|NOACTIVATE
    }
}
