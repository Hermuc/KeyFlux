//! 全部对外契约常量与标定常数, 集中一处 (任务工程要求: 常量集中在 config 模块)。
//!
//! ⚠ 以下「契约类」常量属二进制 ABI / 引擎对接面, 修改前必读 spec.md 对应条目:
//!
//! - `CLASS_NAME` / `WINDOW_TITLE` / `MUTEX_NAME` —— R3/R4/R30, 逐字符锁定;
//! - `APP_SHOW` / `APP_HIDE` / `APP_CANCEL` / `WM_CHAR_VAL` —— R7, 绝对值锁定。
//!
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

// ---- 2026-10-04 结果列表面板协议 (命令框向下延伸; 引擎对接面) ----
//
// 为什么用窗口消息而不是让插件自建窗口: 用户需求 —— 搜索结果必须是命令框**本体的
// 向下延伸** (Flow Launcher / uTools 形态), 而不是框下另挂的独立组件。命令框因此
// 自身具备「长高 + 绘制列表」的能力, 插件只负责把数据推过来。
// 载荷 = WM_COPYDATA + 自定义 dwData 魔数 (见 `results::PAYLOAD_MAGIC`), 逐字节
// 定义在 `results::encode_payload`; AHK 侧镜像 = `EverythingHost.BuildResultsPayload`。
/// 0x406 = 设置结果列表: **WM_COPYDATA**(0x004A), dwData = 载荷魔数,
/// lpData/cbData = `results::decode_payload` 载荷。wParam = 发送方窗口 (回推目标)。
pub const APP_RESULTS_DATA: u32 = 0x004A;
/// 0x407 = 移动结果高亮: wParam = 0 基下标 (`-1` = 无高亮)。
pub const APP_RESULTS_SELECT: u32 = 0x0407;
/// 0x408 = 收起结果列表 (窗口回落到基准高度)。
pub const APP_RESULTS_CLEAR: u32 = 0x0408;
/// 0x409 = 命令框 → 引擎的**回推**通知 (发往 0x406 的 wParam 窗口):
/// wParam = 1 基行号 (0 = 无), lParam = 1 点选 (打开) / 2 高亮变化 (悬停/滚轮)。
pub const APP_RESULTS_NOTIFY: u32 = 0x0409;

// ---- 2026-10-04 搜索徽标协议 (查询区右侧固定图标; 插件提供, 命令框只认字形编号) ----
//
// 解耦口径: 命令框不知道任何插件 —— 它只知道「徽标 = 一个编号的矢量字形」。
// 插件 (everything_search) 在搜索模式激活时发 0x40A, 会话收尾发 0x40B; 命令框侧
// 另有兜底: 徽标随会话存在 (0x401/0x402/0x403 一律清除, 同结果列表的「活不过一次
// 会话」), 即使插件漏发隐藏消息也不会残留。
/// 0x40A = 显示徽标: wParam = 字形编号 (`badge::GLYPH_*`; 未注册编号忽略)。
pub const APP_BADGE_SHOW: u32 = 0x040A;
/// 0x40B = 隐藏徽标 (wParam/lParam 忽略)。
pub const APP_BADGE_HIDE: u32 = 0x040B;

// ---- 搜索徽标几何 (DIP; 与皮肤 18 键解耦, 同 LIST_* 口径) ----
/// 字形盒边长 (DIP): @125% = 25px (对照参照图中的放大镜占位)。
pub const BADGE_SIZE_DIP: f64 = 20.0;
/// 字形盒右缘距白框右缘的距离 (DIP): @125% = 20px。
pub const BADGE_MARGIN_DIP: f64 = 16.0;
/// 描边宽 (DIP): @125% = 2px (与查询区 1px 网格线区分层, 与列表 3px 强调条之间)。
pub const BADGE_STROKE_DIP: f64 = 1.6;

// ---- 结果列表面板几何 (DIP; 与皮肤 18 键解耦 —— 皮肤文件由生成端产出,
//      新增列表键会牵动生成器与 parity 基线, 故 v1 走内建常数) ----
/// 结果行高 (DIP)。
pub const LIST_ROW_DIP: f64 = 30.0;
/// 结果行字号 (DIP; 常规字重, 与查询区 44 DIP 粗体区分层)。
pub const LIST_FONT_DIP: f64 = 17.0;
/// 查询区/结果区分隔线高 (DIP)。
pub const LIST_SEPARATOR_DIP: f64 = 1.0;
/// 列表底部留白 (DIP): 末行不贴圆角。
pub const LIST_BOTTOM_PAD_DIP: f64 = 8.0;
/// 结果行文本水平内边距 (DIP)。
pub const LIST_TEXT_PAD_DIP: f64 = 16.0;
/// 选中行左侧强调条宽 (DIP)。
pub const LIST_ACCENT_DIP: f64 = 3.0;
/// 滚动条宽 (DIP)。
pub const LIST_SCROLLBAR_DIP: f64 = 3.0;
/// 滚动条距内缘 (DIP)。
pub const LIST_SCROLLBAR_MARGIN_DIP: f64 = 8.0;
/// 可视行数上限 (兜底; 实际值按屏幕高度收敛, 见 `geometry::max_list_rows`)。
pub const LIST_MAX_ROWS: usize = 12;
/// 展开后窗口底边距屏幕底的最小留白 (px): 不越屏。
pub const LIST_MIN_SCREEN_MARGIN_PX: i32 = 8;

// ---- 逐像素合成 (2026-10-04 样式还原: 白边 + 阴影 + 半透明填充) ----
//
// 为什么需要: v1 用 `SetLayeredWindowAttributes(LWA_ALPHA)` 做整窗 alpha ⇒ 白边与内部
// 不可能是**不同**的不透明度 ⇒ 原版「3px 纯白描边 + 半透明内部」表达不出来 (白边被拉平
// 成内部同色 = 视觉上无边框)。改走 `UpdateLayeredWindow` 逐像素 alpha (design-A §4.4
// 写明的升级路径), 合成数学在 `crate::compose` (纯逻辑 + 单测)。
/// 背景透过率的**衰减系数**: 净透过率 = `(1 − backgroundOpacity) × 本值`。
///
/// 标定依据 (2026-10-04 原版 vs 新版**同背景活体 A/B**, 见 `%TEMP%\kf_list_smoke\ab_style.py`
/// 与 `ab_rows.py`): 受控背景做成三条带 —— 纯白 255 / 浅底深字 / 纯黑 0, 由
/// `Lw = α·F + (1−α)·255` 与 `Lb = α·F` 两式**联立**解出 (两个未知量两个方程, 不依赖插值):
///   - 原版: 净有效 alpha **0.945**, 内容色 F = 242.9, 透过率 0.055;
///   - 新版 (改前): 0.812 (= 皮肤 0.9², 与理论自校验吻合), F = 254.9, 透过率 0.188。
///
/// 独立测法 (文字笔画调制度 内部/外部之比) 给出 0.072 vs 0.202 —— 两个测法互相印证:
/// **新版透出的背景是原版的 3.4 倍**, 这正是用户「整体过于透明、不够实」的量化来源。
///
/// 0.055 / (1 − 0.9) = 0.55 ⇒ 原版在填充之下还有一层把背景压暗 45% 的底板 (其 DComp
/// 视觉树里的实心底板视觉)。本实现没有那一层 ⇒ 把它的效果并入 α 与内容色两项
/// (见 `skin::fill_alpha` / `skin::panel_content_color`), 逐像素观感等价。
pub const BACKDROP_DIM: f64 = 0.55;

/// 皮肤 `windowShadowSize` (DIP) → 高斯 σ 的增益。
///
/// 标定依据 (同 A/B 的框外剖面, 距框 1..10px 压暗级数):
///   原版 **[50, 45, 33, 24, 16, 10, 6, 3, 1, 0]** → 拟合 σ ≈ 3.0px (拖尾 10px 内归零);
///   新版(改前) [55, 48, 41, 34, 28, 22, 17, 12, 9, 6] → σ ≈ 5.06px (拖尾过长、偏「散」)。
/// `windowShadowSize=3.0 DIP` = 3.75px ⇒ 增益 3.0/3.75 = 0.80。
/// ⚠ 上一轮按用户截图估的 5.1px 偏大 (同一插值法失效所致, 见 `BACKDROP_DIM` 注)。
pub const SHADOW_SIGMA_GAIN: f64 = 0.80;

/// 阴影峰值不透明度的增益: 皮肤 `windowShadowOpacity`(0.5) × 本增益 = 轮廓模糊后的峰值。
///
/// 标定 (同 A/B 框外剖面): 原版框外 1px 处压暗 **50 级** (峰值) ⇒ 有效峰值 alpha ≈ 0.30
/// ⇒ 增益 0.60。上一轮 0.7 (峰值 55 级) 略重, 已按实测收敛。
pub const SHADOW_PEAK_GAIN: f64 = 0.60;

/// 阴影垂直偏移 (DIP, 正 = 向下): 原版左/上边缘剖面峰值都在边缘**外** 2px ⇒ 阴影下移。
pub const SHADOW_DY_DIP: f64 = 1.6;

/// 窗口 region (命中测试用) 相对白框的扩展半径 = σ × 本系数 (+ dy)。
/// 必须扩展, 否则 `SetWindowRgn` 会把框外阴影整条裁掉 (区域同时管合成与命中)。
pub const SHADOW_REGION_SIGMA: f64 = 3.0;
/// WS_EX_NOACTIVATE 位。
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

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

// ---- R13/R23/R28 渲染标定常数 (参考实现 doc/reference/EverythingQueryEdit.ahk 实测口径) ----
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

    /// 结果列表面板协议: 消息值与载荷魔数逐字节锁定 (AHK 侧 EverythingHost 镜像)。
    #[test]
    fn results_protocol_values_are_locked() {
        assert_eq!(APP_RESULTS_DATA, 0x004A); // WM_COPYDATA
        assert_eq!(APP_RESULTS_SELECT, 0x0407);
        assert_eq!(APP_RESULTS_CLEAR, 0x0408);
        assert_eq!(APP_RESULTS_NOTIFY, 0x0409);
        assert_eq!(crate::results::PAYLOAD_MAGIC.to_le_bytes(), *b"KFR1");
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
