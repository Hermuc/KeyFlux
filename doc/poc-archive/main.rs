//! KeyFlux 设置面板迁移 —— Phase 1 PoC
//!
//! 目的：在**不使用任何未核实 API** 的前提下，用最小代码验证六项高风险面。
//!
//! 验证结果（2026-09-28 实机）：
//!   ✅ 1. 材质：Acrylic/Mica 通过 `WindowVisuals::backdrop` 生效
//!   ✅ 2. 主题表达力：Color / Brush / ThemeBrush / CornerRadius / Thickness 可用
//!   ✅ 3. 自绘网格：Grid rows/columns + Border 卡片 + 1px 描边
//!   ✅ 4. chips 换行：VariableSizedWrapGrid
//!   ✅ 5. 交互：受控 TextBox / ToggleSwitch(slot) / Button / 导航切换
//!   ❌ 6. ContentDialog：**放进 view 树会导致启动崩溃（0xC000027B / -1073741189）**
//!         已排除：挂载位置（Fragment vs StackPanel.keyed_children）、开关初值（true/false）均崩溃。
//!         待验证替代方案：`open_window` 独立窗口 / `run_window` + 原生 MessageBox /
//!         或确认 0.100.0 的 ContentDialog 是否必须走特定宿主路径。
//!
//! ⚠️ 0.100.0 与 master 文档存在差异（重要）：
//!   - 无 `window_frame`（集成标题栏是 master 未发布特性）⇒ 目前只有 `window_title` + `window_visuals`
//!   - 控件用 **slot 体系**：`SlotsControl::{slot, collection_slot, slots}` + `SlotView::{new, collection}`
//!   - `content(..)`/`children(..)`/`slot(..)` 是**收尾方法**，返回 `View`，必须在链尾
//!
//! 构建：见同目录 README.md

use windows_reactor::*;

mod theme;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Controls,
    Theme,
}

#[derive(Clone)]
enum Message {
    Nav(Option<String>),
    Name(String),
    AcrylicToggled(bool),
}

struct Shell {
    page: Page,
    name: String,
    acrylic: bool,
}

impl Component for Shell {
    type Input = ();
    type Message = Message;

    fn create(_input: &(), _context: &ComponentContext<Self>) -> Self {
        Self {
            page: Page::Controls,
            name: String::new(),
            acrylic: true,
        }
    }

    fn update(&mut self, message: Message, _context: &ComponentContext<Self>) {
        match message {
            Message::Nav(tag) => {
                self.page = match tag.as_deref() {
                    Some("theme") => Page::Theme,
                    _ => Page::Controls,
                };
            }
            Message::Name(value) => self.name = value,
            Message::AcrylicToggled(value) => self.acrylic = value,
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        // (1) 材质验证：默认 Acrylic，可切 Mica。
        let backdrop = if self.acrylic {
            WindowBackdrop::Acrylic
        } else {
            WindowBackdrop::Mica
        };
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(backdrop)
                // KeyFlux 要求强制浅色（对齐旧版 RequestedThemeVariant="Light"）
                .theme(WindowTheme::Light)
                .client_size(1100.0, 720.0),
        );

        let body = match self.page {
            Page::Controls => self.controls_page(context),
            Page::Theme => theme_page(),
        };

        // 0.100.0 真实 API：控件用 slot 体系，没有 menu_items()/content()。
        let nav: View = NavigationView::new()
            .pane_title("KeyFlux PoC")
            .is_settings_visible(false)
            .on_selected_tag_changed(context.callback(|tag: Option<String>| Message::Nav(tag)))
            .slots([
                SlotView::collection(
                    NavigationViewSlot::MenuItems,
                    [
                        (
                            "controls",
                            NavigationViewItem::new()
                                .tag("controls")
                                .slot(NavigationViewItemSlot::Content, "控件与布局"),
                        ),
                        (
                            "theme",
                            NavigationViewItem::new()
                                .tag("theme")
                                .slot(NavigationViewItemSlot::Content, "主题令牌"),
                        ),
                    ],
                ),
                SlotView::new(NavigationViewSlot::Content, body),
            ]);

        context.window_title("KeyFlux 设置面板 PoC（Rust + windows-reactor / WinUI）");
        nav
    }
}

impl Shell {
    fn controls_page(&self, context: &mut ViewContext<Self>) -> View {
        // (3) 自绘网格：Grid 行列 + Border 卡片 + 1px 描边。
        let grid: View = Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .columns([GridLength::STAR, GridLength::STAR])
            .row_spacing(12.0)
            .column_spacing(12.0)
            .children((
                TextBlock::new()
                    .text("自绘网格（Grid rows/columns + Border 卡片）")
                    .font_size(16.0)
                    .font_weight(FontWeight::BOLD)
                    .foreground(theme::solid(theme::TEXT_PRIMARY))
                    .grid_row(0)
                    .grid_column(0)
                    .grid_column_span(2),
                card(1, 0, "卡片 A", "1px 描边 + 8px 圆角 + 暖色底；验证对比度与层级。"),
                card(1, 1, "卡片 B", "与卡片 A 同配方，验证网格对齐与间距。"),
            ));

        let controls: View = Border::new()
            .padding(theme::pad_md())
            .background(theme::solid(theme::IVORY))
            .border_brush(theme::solid(theme::BORDER))
            .border_thickness(theme::hairline())
            .corner_radius(theme::radius_md())
            .content(
                StackPanel::new().spacing(10.0).children((
                    TextBlock::new()
                        .text("受控控件（状态回流组件）")
                        .font_size(14.0)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::solid(theme::TEXT_PRIMARY)),
                    TextBox::new()
                        .text(self.name.clone())
                        .placeholder_text("输入内容会回流到组件状态并在下方显示")
                        .on_text_changed(context.callback(|value: String| Message::Name(value))),
                    TextBlock::new()
                        .text(format!("当前输入：{}", self.name))
                        .foreground(theme::solid(theme::TEXT_MUTED)),
                    ToggleSwitch::new()
                        .is_on(self.acrylic)
                        .on_toggled(context.callback(|value: bool| Message::AcrylicToggled(value)))
                        .slot(ToggleSwitchSlot::Header, "亚克力材质（关闭则切 Mica）"),
                )),
            );

        // (4) chips 换行：VariableSizedWrapGrid（reactor 无 WrapPanel）。
        let chips: Vec<(u32, String)> = vec![
            (1, "纯文本".to_string()),
            (2, "URL".to_string()),
            (3, "文件路径".to_string()),
            (4, "磁力链接".to_string()),
            (5, "B 站 AV/BV".to_string()),
            (6, "自定义匹配类型".to_string()),
            (7, "type:my-type".to_string()),
        ];
        let chip_area: View = Border::new()
            .padding(theme::pad_md())
            .background(theme::solid(theme::SAND))
            .corner_radius(theme::radius_md())
            .content(
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("chips 换行（VariableSizedWrapGrid）")
                        .font_size(14.0)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::solid(theme::TEXT_PRIMARY)),
                    VariableSizedWrapGrid::new()
                        .item_width(130.0)
                        .item_height(34.0)
                        .orientation(Orientation::Horizontal)
                        .keyed_children(chips),
                )),
            );

        // 动态列表（keyed_children，key 用逻辑 ID 而非索引）
        let rows: Vec<(u32, String)> = (1..=4)
            .map(|index| (index, format!("映射行 {index}：action = builtin://demo")))
            .collect();
        let list: View = StackPanel::new().spacing(4.0).keyed_children(rows);

        StackPanel::new()
            .spacing(12.0)
            .children((grid, controls, chip_area, list))
    }
}

/// 卡片：注意 `content(..)` 是收尾方法，故 `grid_row/grid_column` 必须在它之前设置。
fn card(row: i32, column: i32, title: &str, detail: &str) -> View {
    let inner = StackPanel::new().spacing(6.0).children((
        TextBlock::new()
            .text(title)
            .font_size(14.0)
            .font_weight(FontWeight::BOLD)
            .foreground(theme::solid(theme::TEXT_PRIMARY)),
        TextBlock::new()
            .text(detail)
            .foreground(theme::solid(theme::TEXT_MUTED)),
    ));

    Border::new()
        .grid_row(row)
        .grid_column(column)
        .padding(theme::pad_md())
        .background(theme::solid(theme::PARCHMENT))
        .border_brush(theme::solid(theme::BORDER))
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_md())
        .content(inner)
}

fn swatch(label: &str, color: Color) -> View {
    Border::new()
        .width(190.0)
        .height(52.0)
        .background(theme::solid(color))
        .border_brush(theme::solid(theme::BORDER))
        .border_thickness(theme::hairline())
        .corner_radius(theme::radius_sm())
        .content(
            TextBlock::new()
                .text(label)
                .font_size(12.0)
                .foreground(theme::solid(theme::TEXT_PRIMARY)),
        )
}

fn theme_page() -> View {
    let swatches: Vec<(u32, View)> = vec![
        (1, swatch("Parchment #f5f4ed", theme::PARCHMENT)),
        (2, swatch("Ivory #faf9f5", theme::IVORY)),
        (3, swatch("Sand #e8e6dc", theme::SAND)),
        (4, swatch("Terracotta #c96442", theme::TERRACOTTA)),
        (5, swatch("Coral #d97757", theme::CORAL)),
        (6, swatch("MutedGreen #5e7d5a", theme::MUTED_GREEN)),
        (7, swatch("Border #b0aea5", theme::BORDER)),
        (8, swatch("TextPrimary #141413", theme::TEXT_PRIMARY)),
    ];

    StackPanel::new().spacing(12.0).children((
        TextBlock::new()
            .text("主题令牌（Color / Brush / CornerRadius / Thickness）")
            .font_size(16.0)
            .font_weight(FontWeight::BOLD)
            .foreground(theme::solid(theme::TEXT_PRIMARY)),
        TextBlock::new()
            .text("同时验证系统主题画刷通路：ThemeBrush::Accent 与 ThemeBrush::CardBackground。")
            .foreground(theme::solid(theme::TEXT_MUTED)),
        Border::new()
            .height(36.0)
            .background(theme::accent())
            .corner_radius(theme::radius_sm())
            .content(TextBlock::new().text("ThemeBrush::Accent").font_size(12.0)),
        Border::new()
            .height(36.0)
            .background(Brush::Theme(ThemeBrush::CardBackground))
            .border_brush(Brush::Theme(ThemeBrush::CardStroke))
            .border_thickness(theme::hairline())
            .corner_radius(theme::radius_sm())
            .content(TextBlock::new().text("ThemeBrush::CardBackground").font_size(12.0)),
        VariableSizedWrapGrid::new()
            .item_width(198.0)
            .item_height(60.0)
            .orientation(Orientation::Horizontal)
            .keyed_children(swatches),
    ))
}

fn main() {
    App::run_component::<Shell>(()).unwrap();
}
