//! 键位图页视图：**键盘网格** + **右侧备注汇总**。
//!
//! 几何与配色逐项对齐旧 `KeymapPageView.axaml` + `KeyCellVm`（键格 43 高、单字符 43×43
//! 正方形、多字符最小 58、间距 4、圆角 6、字号 20/15.7）；
//! 状态色沿用 Claude 令牌（选中=白底陶土字、禁用=奶油底、已绑定=柔和暖绿底、空键=象牙底）。
//!
//! 与旧版的有意差异：旧版左列用 `Viewbox Stretch=Uniform` **整列等比缩放**（窗口变小时文字一起缩），
//! 新版按 Fluent 习惯改为**自然尺寸 + 滚动**（不再缩放文字）。

use windows_reactor::*;

use crate::models::Keymap;
use crate::services::i18n;
use crate::services::keymap::{self, CellState, CommentEntry, KeyboardRow};
use crate::theme;

/// 键格高度（旧 43）。
pub const CELL_HEIGHT: f64 = 43.0;
/// 键格间距（旧 4）。
pub const CELL_SPACING: f64 = 4.0;
/// 单字符键格边长（旧 43，成正方形）。
const SINGLE_CHAR_SIZE: f64 = 43.0;
/// 多字符键格最小宽度（旧 58）。
const MULTI_CHAR_MIN_WIDTH: f64 = 58.0;

/// 键格配色（静止态必填；悬停/按下可选，`None` 表示沿用 Fluent 默认）。
struct CellPalette {
    background: Color,
    foreground: Color,
    border: Color,
    hover_background: Option<Color>,
    hover_foreground: Option<Color>,
    pressed_background: Option<Color>,
    pressed_foreground: Option<Color>,
}

impl CellPalette {
    /// 只有静止态的普通键格（悬停/按下走 Fluent 默认）。
    fn static_only(background: Color) -> Self {
        Self {
            background,
            foreground: theme::NEAR_BLACK,
            border: theme::RING_WARM,
            hover_background: None,
            hover_foreground: None,
            pressed_background: None,
            pressed_foreground: None,
        }
    }
}

/// 状态 → 配色。对齐旧 `KeyCellVm` + `KeymapPageView.axaml` 的
/// `.selected:pointerover/.pressed` 样式：选中键悬停转陶土底白字、按下转 Coral 底白字。
fn cell_palette(state: CellState) -> CellPalette {
    match state {
        // 选中：白底 + 陶土字 + 陶土描边（2026-09-13 用户指定）
        CellState::Selected => CellPalette {
            background: theme::WHITE,
            foreground: theme::TERRACOTTA,
            border: theme::TERRACOTTA,
            hover_background: Some(theme::TERRACOTTA),
            hover_foreground: Some(theme::WHITE),
            pressed_background: Some(theme::CORAL),
            pressed_foreground: Some(theme::WHITE),
        },
        // 禁用：奶油底（触发键自身不可点）
        CellState::Disabled => CellPalette::static_only(theme::BORDER_CREAM),
        // 已绑定：柔和暖绿底
        CellState::Bound => CellPalette::static_only(theme::MUTED_GREEN_SOFT),
        // 空键：象牙底
        CellState::Empty => CellPalette::static_only(theme::IVORY),
    }
}

/// 把状态翻译成 WinUI 轻量样式资源覆盖（资源键经官方文档核实：
/// `ButtonBackground` + `PointerOver/Pressed/Disabled` 后缀，`ButtonForeground*`、`ButtonBorderBrush*`）。
///
/// 这是 Fluent 推荐做法（官方文档明言「Modifying these resources is preferred to setting
/// properties such as Background and Foreground」），也让悬停/按下反馈由 WinUI 模板自动处理。
fn cell_overrides(state: CellState) -> ResourceOverrides {
    let palette = cell_palette(state);

    let mut overrides = ResourceOverrides::new()
        .set("ButtonBackground", palette.background)
        .set("ButtonForeground", palette.foreground)
        .set("ButtonBorderBrush", palette.border);

    if let Some(color) = palette.hover_background {
        overrides = overrides.set("ButtonBackgroundPointerOver", color);
    }
    if let Some(color) = palette.hover_foreground {
        overrides = overrides.set("ButtonForegroundPointerOver", color);
    }
    if let Some(color) = palette.pressed_background {
        overrides = overrides.set("ButtonBackgroundPressed", color);
    }
    if let Some(color) = palette.pressed_foreground {
        overrides = overrides.set("ButtonForegroundPressed", color);
    }

    overrides
}

/// 单个键格按钮（单字符 43×43 正方形；多字符自然宽 + 最小 58）。
fn build_cell_button<C: IntoUnitCallback>(
    label: &str,
    state: CellState,
    font_size: f64,
    on_click: C,
) -> View {
    let single_char = label.chars().count() == 1;

    Button::new()
        .height(CELL_HEIGHT)
        .width(if single_char {
            Some(SINGLE_CHAR_SIZE)
        } else {
            None
        })
        .min_width(if single_char {
            SINGLE_CHAR_SIZE
        } else {
            MULTI_CHAR_MIN_WIDTH
        })
        .is_enabled(state != CellState::Disabled)
        .resource_overrides(cell_overrides(state))
        .on_click(on_click)
        .content(
            TextBlock::new()
                .text(label.to_string())
                .font_size(font_size)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        )
}

/// 键盘网格：行 → 键格。
pub fn keyboard_grid<F, C>(
    rows: &[KeyboardRow],
    row_states: &[Vec<CellState>],
    font_size: f64,
    mut make_callback: F,
) -> View
where
    F: FnMut(String) -> C,
    C: IntoUnitCallback,
{
    let row_views: Vec<(usize, View)> = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let cells: Vec<(String, View)> = row
                .cells
                .iter()
                .enumerate()
                .map(|(cell_index, cell)| {
                    let state = row_states
                        .get(row_index)
                        .and_then(|states| states.get(cell_index))
                        .copied()
                        .unwrap_or(CellState::Empty);
                    let callback = make_callback(cell.hotkey.clone());
                    // ⚠️ key 必须**含状态**：`resource_overrides` 只在新元素创建时生效，
                    // 重渲染不会重应用（2026-09-28 实测：内容 diff 生效、资源覆盖不生效）。
                    // 让状态进 key ⇒ 状态变化时 reactor 重建该键格元素 ⇒ 覆盖落到实处。
                    (
                        format!("{}|{state:?}", cell.hotkey),
                        build_cell_button(&cell.label, state, font_size, callback),
                    )
                })
                .collect();

            (
                row_index,
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(CELL_SPACING)
                    .keyed_children(cells),
            )
        })
        .collect();

    StackPanel::new()
        .orientation(Orientation::Vertical)
        .spacing(CELL_SPACING)
        .keyed_children(row_views)
}

/// 页头：模式名称 + 子模式上层信息（label:503）。标题字号对齐旧版 18。
pub fn page_header(title: &str, parent_info: Option<&str>) -> View {
    let parent: View = match parent_info {
        Some(text) => TextBlock::new()
            .text(text.to_string())
            .font_size(13.0)
            .foreground(theme::stone_gray())
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
        None => View::empty(),
    };

    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(14.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
        .children((
            TextBlock::new()
                .text(title.to_string())
                .font_size(18.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(theme::near_black()),
            parent,
        ))
}

/// 键格状态矩阵（与 `rows` 同形）：供 [`keyboard_grid`] 直接消费。
pub fn compute_states(
    rows: &[KeyboardRow],
    keymap: &Keymap,
    disabled: &std::collections::HashMap<i32, std::collections::HashSet<String>>,
    selected: Option<&str>,
    window_group_id: i32,
) -> Vec<Vec<CellState>> {
    let abbr = keymap::is_abbr(keymap);
    rows.iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    let is_selected = selected == Some(cell.hotkey.as_str());
                    let is_disabled = keymap::is_disabled(disabled, keymap.id, &cell.hotkey);
                    let is_bound = keymap::is_bound(keymap, &cell.hotkey, window_group_id);
                    keymap::cell_state(is_selected, is_disabled, is_bound, abbr)
                })
                .collect()
        })
        .collect()
}

/// 右侧备注汇总（复刻 `ActionCommentTable` + `CommentSummaryPanel`）：
/// 标题（305，serif 档）+ 条目「**键** - 备注」单行（reactor 无富文本 ⇒ 同块拼接）。
pub fn comment_summary(entries: &[CommentEntry]) -> View {
    let items: Vec<(usize, View)> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            // 旧版单行 Run 混排「Key - Comment」；键与备注用「 - 」拼接、整体换行
            let text = if entry.comment.is_empty() {
                entry.key_text.clone()
            } else {
                format!("{} - {}", entry.key_text, entry.comment)
            };
            let block: View = TextBlock::new()
                .text(text)
                .font_size(theme::FONT_BODY)
                .foreground(theme::solid(theme::CHARCOAL_WARM))
                .text_wrapping(TextWrapping::Wrap)
                .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
                .into();
            (index, block)
        })
        .collect();

    // 标题（旧 `CommentSummaryPanel` 的 label:305）作为 keyed 列表的首项
    let mut rows: Vec<(usize, View)> = vec![(
        usize::MAX,
        TextBlock::new()
            .text(i18n::t("305"))
            .font_size(15.0)
            .font_weight(FontWeight::MEDIUM)
            .foreground(theme::near_black())
            .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
            .into(),
    )];
    rows.extend(items);
    ScrollViewer::new().content(
        StackPanel::new()
            .orientation(Orientation::Vertical)
            .spacing(0.0)
            .margin(Thickness::new(0.0, 0.0, 12.0, 0.0))
            .keyed_children(rows),
    )
}

/// 空备注占位（无备注时不显示空白滚动区）。
pub fn comment_empty_hint() -> View {
    TextBlock::new()
        .text("（暂无备注）")
        .font_size(theme::FONT_CAPTION)
        .foreground(theme::stone_gray())
        .into()
}
