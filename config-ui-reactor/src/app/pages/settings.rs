//! `app::pages::settings` —— 选项页：方案卡 + 手风琴分区（`section` 是本页专用的分区外壳）。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。
//!
//! 2026-10-09 结构优化：`settings_page` 原为单函数 496 行（整页 10 个分区全内联）。
//! 现按 `self.section(...)` 的分区边界各抽一个私有方法（`settings_section_*`），
//! 左列方案卡抽为 `settings_scheme_column`；`settings_page` 只负责**组合顺序**。
//! 分区的 `sections.push` 先后与旧实现逐字一致（顺序敏感，见方法头注释）。

use super::super::*;

impl Shell {
    /// 手风琴分区卡包装：展开时才构建内容（一次只展开一张，复刻旧版）。
    pub(in crate::app) fn section(
        &self,
        context: &mut ViewContext<Self>,
        id: &'static str,
        title_key: &str,
        body: impl FnOnce(&Self, &mut ViewContext<Self>) -> View,
    ) -> View {
        let open = self.settings_open == Some(id);
        let content = if open {
            body(self, context)
        } else {
            View::empty()
        };
        settings_view::section_card(
            i18n::t(title_key),
            open,
            context.message(Message::SettingsSection(id)),
            content,
        )
    }

    /// 选项页（keymap id=4）：左列「快捷键方案」卡 + 右列手风琴分区栈。
    ///
    /// 分区顺序对齐旧 `SettingsPageView.axaml`：其他设置 / 程序分组 / 自定义热键 /
    /// 鼠标参数 / 滚轮 / 键盘布局 / 触发延时 / 命令框皮肤 / 命令框字体 / 路径变量。
    pub(in crate::app) fn settings_page(&self, context: &mut ViewContext<Self>) -> View {
        if self.config.is_none() {
            return TextBlock::new().text("配置未加载").into();
        }
        let left = self.settings_scheme_column(context);

        // 右列：分区栈（下推顺序 = 显示顺序，改动前先确认用户定版）
        let mut sections: Vec<(usize, View)> = Vec::new();

        // 一次性提示（自启命令失败 / 保存校验失败）
        if let Some(notice) = &self.settings_notice {
            sections.push((
                sections.len(),
                TextBlock::new()
                    .text(notice.clone())
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::ERROR_CRIMSON))
                    .text_wrapping(TextWrapping::Wrap)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
                    .into(),
            ));
        }

        sections.push((
            sections.len(),
            self.section(context, "other", "505", Self::settings_section_other),
        ));
        sections.push((
            sections.len(),
            self.section(context, "groups", "601", Self::settings_section_groups),
        ));
        sections.push((
            sections.len(),
            self.section(
                context,
                "customhotkeys",
                "1116",
                Self::settings_section_custom_hotkeys,
            ),
        ));
        sections.push((
            sections.len(),
            self.section(context, "mouse", "701", Self::settings_section_mouse),
        ));
        sections.push((
            sections.len(),
            self.section(context, "scroll", "712", Self::settings_section_scroll),
        ));
        sections.push((
            sections.len(),
            self.section(context, "layout", "721", Self::settings_section_layout),
        ));
        sections.push((
            sections.len(),
            self.section(context, "delay", "761", Self::settings_section_delay),
        ));
        sections.push((
            sections.len(),
            self.section(context, "skin", "741", Self::settings_section_skin),
        ));
        sections.push((
            sections.len(),
            self.section(context, "font", "2503", Self::settings_section_font),
        ));
        sections.push((
            sections.len(),
            self.section(context, "pathvars", "907", Self::settings_section_path_vars),
        ));

        // 右列：旧 `StackPanel Width="460" Spacing="16" Margin="24,0,24,24"`（卡间距由
        // section_card 自带底距 16 承担）；整页容器 = 旧 `Margin="24,20,24,28"`。
        // 宽度 460 → 520 (2026-10-06)：程序分组行单行四控件需 ~440 DIP，460 卡内
        // (380) 放不下（标识符被裁的根因）；页面右列 STAR 实际有 ~580 DIP 余量，
        // 收窄到 520 仍留 ~60 缓冲。右列所有卡片都是弹性内容，整体加宽无副作用。
        let right: View = ScrollViewer::new().content(
            StackPanel::new()
                .width(520.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .margin(Thickness::new(24.0, 0.0, 24.0, 24.0))
                .keyed_children(sections),
        );

        // 右边距必须为 0：纵向滚动条贴在右列 ScrollViewer 的右缘，若这里再留 24，
        // 滚动条会悬在离窗口右缘 ~35px 处（使用指南页 ScrollViewer 是页面根、贴边，
        // 两页并排看滚动条位置不一致——2026-10-06 用户报障）。右列内容是 Left 对齐
        // 的定宽 520 卡片列，右缘外扩不挪卡片；左列 560 定宽亦不受影响。
        Grid::new()
            .margin(Thickness::new(24.0, 20.0, 0.0, 28.0))
            .columns([GridLength::Pixel(560.0), GridLength::STAR])
            .children((left, Border::new().grid_column(1).content(right)))
    }

    /// 左列：快捷键方案（915）——行内直接编辑名称/触发键/开关。
    ///
    /// 旧 `Border.leftPanel`（Ivory 面 + 淡冷边 2px + 圆角 14 + Padding 16）；
    /// 标题 915 旧 16 SemiBold + 底距 12。
    fn settings_scheme_column(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return View::empty();
        };
        let schemes: Vec<&Keymap> = config.keymaps.iter().filter(|km| km.id > 4).collect();

        let mut scheme_rows: Vec<(usize, View)> = Vec::new();
        for (index, keymap) in schemes.iter().enumerate() {
            scheme_rows.push((
                index,
                settings_view::scheme_row(
                    &keymap.name,
                    &keymap.hotkey,
                    keymap.enable,
                    !keymap.enable,
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::SchemeName(index, value))
                    }),
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::SchemeHotkey(index, value))
                    }),
                    context.callback(move |value: bool| {
                        Message::Opt(OptEdit::SchemeEnable(index, value))
                    }),
                    // 已启用方案不可删除（用户定版）：先关再删
                    context.message(Message::Opt(OptEdit::SchemeDelete(index))),
                ),
            ));
        }

        Border::new()
            .padding(theme::pad_md())
            .margin(Thickness::new(0.0, 0.0, 16.0, 0.0))
            .background(theme::ivory())
            .border_brush(theme::border_faint())
            .border_thickness(theme::card_border())
            .corner_radius(theme::radius_card())
            .vertical_alignment(VerticalAlignment::Top)
            .content(
                StackPanel::new().spacing(10.0).children((
                    TextBlock::new()
                        .text(i18n::t("915"))
                        .font_size(theme::FONT_SECTION_TITLE)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .foreground(theme::near_black())
                        .margin(Thickness::new(0.0, 0.0, 0.0, 12.0)),
                    settings_view::scheme_header(),
                    StackPanel::new().keyed_children(scheme_rows),
                    Button::new()
                        .margin(Thickness::new(0.0, 10.0, 0.0, 0.0))
                        .on_click(context.message(Message::Opt(OptEdit::SchemeAdd)))
                        .content(TextBlock::new().text(i18n::t("405"))),
                )),
            )
    }

    /// 505 其他设置：506 开机自启（命令入队，随保存生效）/ 901 隐藏矩阵 / 781 语言。
    fn settings_section_other(&self, context: &mut ViewContext<Self>) -> View {
        let languages = vec!["中文".to_string(), "English".to_string()];
        let language_index = match self
            .config
            .as_ref()
            .map(|config| config.options.language.as_str())
        {
            Some("en") => 1,
            _ => 0,
        };
        StackPanel::new().children((
            settings_view::toggle_row(
                i18n::t("506"),
                self.config
                    .as_ref()
                    .map(|c| c.options.startup)
                    .unwrap_or(false),
                context.callback(|value: bool| Message::StartupToggle(value)),
            ),
            settings_view::check_row(
                i18n::t("902"),
                self.config
                    .as_ref()
                    .map(|c| c.options.hide_matrix)
                    .unwrap_or(false),
                context.callback(|value: bool| Message::Opt(OptEdit::HideMatrix(value))),
            ),
            settings_view::toggle_row(
                i18n::t("2593"),
                self.acrylic,
                context.callback(|value: bool| Message::AcrylicToggle(value)),
            ),
            settings_view::combo_row(
                i18n::t("781"),
                &languages,
                language_index,
                context.callback(|value: Option<usize>| {
                    Message::Opt(OptEdit::Language(value.unwrap_or(0)))
                }),
            ),
            // 引擎可观测性闭环（报告 #14）：错误 Tip 已带日志路径，这里给一键入口。
            // 路径是产物字面量（非文案），故不占 i18n 键。
            settings_view::field_row_with_end_button(
                i18n::t("2600"),
                TextBlock::new()
                    .text("logs\\engine_error.log")
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .vertical_alignment(VerticalAlignment::Center)
                    .into(),
                i18n::t("2601"),
                context.message(Message::OpenEngineLog),
            ),
        ))
    }

    /// 601 编辑程序分组（哨兵 Exclude/Global 不进编辑器）。
    fn settings_section_groups(&self, context: &mut ViewContext<Self>) -> View {
        let groups: Vec<&crate::models::WindowGroup> = self
            .config
            .as_ref()
            .map(|config| {
                config
                    .options
                    .window_groups
                    .iter()
                    .filter(|group| group.id > 0)
                    .collect()
            })
            .unwrap_or_default();
        let mut rows: Vec<(usize, View)> = Vec::new();
        for (row, group) in groups.iter().enumerate() {
            let name_cb =
                context.callback(move |value: String| Message::Opt(OptEdit::GroupName(row, value)));
            let value_cb = context
                .callback(move |value: String| Message::Opt(OptEdit::GroupValue(row, value)));
            let condition_cb = context.callback(move |value: Option<usize>| {
                Message::Opt(OptEdit::GroupCondition(row, value.unwrap_or(0)))
            });
            let delete_cb = context.message(Message::Opt(OptEdit::GroupRemove(row)));
            rows.push((
                row,
                settings_view::group_row(
                    &group.name,
                    &group.value,
                    // 条件下拉只声明 4 档：conditionType 5（自定义表达式，数据层仍合法）
                    // 渲染时钳回 0，防 selected_index 越界
                    group.condition_type.saturating_sub(1).min(3) as usize,
                    name_cb,
                    value_cb,
                    condition_cb,
                    delete_cb,
                ),
            ));
        }
        StackPanel::new().children((
            StackPanel::new().keyed_children(rows),
            Button::new()
                .on_click(context.message(Message::Opt(OptEdit::GroupAdd)))
                .content(TextBlock::new().text(i18n::t("609"))),
            settings_view::hint_row(i18n::t("612")),
        ))
    }

    /// 1116 自定义热键（keymap id=1；「功能」列点击打开动作编辑对话框，
    /// 复刻旧 SettingsPageView「功能列点击弹 ActionEditorWindow」交互）。
    fn settings_section_custom_hotkeys(&self, context: &mut ViewContext<Self>) -> View {
        let rows: Vec<(String, String)> = self
            .config
            .as_ref()
            .and_then(|config| config.keymaps.iter().find(|km| km.id == 1))
            .map(|keymap| {
                keymap
                    .hotkeys
                    .iter()
                    .map(|(hotkey, actions)| {
                        // 功能列显示翻译后的功能名（comment 存的是 "label:<i18n键>"
                        // 引用，i18n::t 负责剥前缀查表；与右侧备注汇总列同口径）
                        let function = actions
                            .iter()
                            .find(|action| !action.comment.is_empty())
                            .map(|action| i18n::t(&action.comment))
                            .unwrap_or_default();
                        (hotkey.clone(), function)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut list: Vec<(usize, View)> = vec![(usize::MAX, settings_view::hotkey_header())];
        for (row, (hotkey, function)) in rows.iter().enumerate() {
            list.push((
                row,
                settings_view::hotkey_row(
                    hotkey,
                    function,
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::CustomHotkey(row, value))
                    }),
                    context.message(Message::CustomHotkeyEdit(row)),
                    context.message(Message::Opt(OptEdit::CustomHotkeyRemove(row))),
                ),
            ));
        }
        StackPanel::new().children((
            StackPanel::new().keyed_children(list),
            Button::new()
                .on_click(context.message(Message::Opt(OptEdit::CustomHotkeyAdd)))
                .content(TextBlock::new().text(i18n::t("1118"))),
        ))
    }

    /// 701 修改鼠标参数（9 字段；字符串型数值原样透传，与旧版自由文本框一致）。
    fn settings_section_mouse(&self, context: &mut ViewContext<Self>) -> View {
        let mouse = self
            .config
            .as_ref()
            .map(|config| config.options.mouse.clone())
            .unwrap_or_default();
        fn text<C: Fn(String) -> Message + 'static>(
            callback: C,
        ) -> impl Fn(String) -> Message + 'static {
            move |value: String| callback(value)
        }
        StackPanel::new().children((
            settings_view::hint_row(i18n::t("702")),
            settings_view::text_field(
                i18n::t("703"),
                &mouse.delay1,
                context.callback(text(|value| Message::Opt(OptEdit::MouseDelay1(value)))),
            ),
            settings_view::text_field(
                i18n::t("704"),
                &mouse.delay2,
                context.callback(text(|value| Message::Opt(OptEdit::MouseDelay2(value)))),
            ),
            settings_view::text_field(
                i18n::t("705"),
                &mouse.fast_single,
                context.callback(text(|value| Message::Opt(OptEdit::MouseFastSingle(value)))),
            ),
            settings_view::text_field(
                i18n::t("706"),
                &mouse.fast_repeat,
                context.callback(text(|value| Message::Opt(OptEdit::MouseFastRepeat(value)))),
            ),
            settings_view::text_field(
                i18n::t("707"),
                &mouse.slow_single,
                context.callback(text(|value| Message::Opt(OptEdit::MouseSlowSingle(value)))),
            ),
            settings_view::text_field(
                i18n::t("708"),
                &mouse.slow_repeat,
                context.callback(text(|value| Message::Opt(OptEdit::MouseSlowRepeat(value)))),
            ),
            settings_view::text_field(
                i18n::t("709"),
                &mouse.tip_symbol,
                context.callback(text(|value| Message::Opt(OptEdit::MouseTipSymbol(value)))),
            ),
            settings_view::check_row(
                i18n::t("710"),
                mouse.show_tip,
                context.callback(|value: bool| Message::Opt(OptEdit::MouseShowTip(value))),
            ),
            settings_view::check_row(
                i18n::t("711"),
                mouse.keep_mouse_mode,
                context.callback(|value: bool| Message::Opt(OptEdit::MouseKeepMode(value))),
            ),
        ))
    }

    /// 712 滚轮相关参数（3 字段）。
    fn settings_section_scroll(&self, context: &mut ViewContext<Self>) -> View {
        let scroll = self
            .config
            .as_ref()
            .map(|config| config.options.scroll.clone())
            .unwrap_or_default();
        StackPanel::new().children((
            settings_view::text_field_wide_label(
                i18n::t("713"),
                &scroll.delay1,
                context.callback(|value: String| Message::Opt(OptEdit::ScrollDelay1(value))),
            ),
            settings_view::text_field_wide_label(
                i18n::t("714"),
                &scroll.delay2,
                context.callback(|value: String| Message::Opt(OptEdit::ScrollDelay2(value))),
            ),
            settings_view::text_field_wide_label(
                i18n::t("715"),
                &scroll.once_line_count,
                context.callback(|value: String| Message::Opt(OptEdit::ScrollOnceLine(value))),
            ),
        ))
    }

    /// 721 修改键盘布局：多行文本 + 四个预设按钮。
    fn settings_section_layout(&self, context: &mut ViewContext<Self>) -> View {
        let layout = self
            .config
            .as_ref()
            .map(|config| config.options.keyboard_layout.clone())
            .unwrap_or_default();
        StackPanel::new().spacing(8.0).children((
            settings_view::hint_row(i18n::t("722")),
            TextBox::new()
                .text(layout)
                .accepts_return(true)
                .min_height(180.0)
                .on_text_changed(
                    context
                        .callback(|value: String| Message::Opt(OptEdit::KeyboardLayoutSet(value))),
                ),
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .children((
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("0"))))
                        .content(TextBlock::new().text(i18n::t("723"))),
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("74"))))
                        .content(TextBlock::new().text(i18n::t("724"))),
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("104"))))
                        .content(TextBlock::new().text(i18n::t("725"))),
                    Button::new()
                        .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("1"))))
                        .content(TextBlock::new().text(i18n::t("726"))),
                )),
        ))
    }

    /// 761 设置触发延时：方案下拉 + 毫秒数（提示 763）。
    fn settings_section_delay(&self, context: &mut ViewContext<Self>) -> View {
        let schemes: Vec<String> = self
            .config
            .as_ref()
            .map(|config| {
                config
                    .keymaps
                    .iter()
                    .filter(|km| km.id > 4)
                    .map(|km| km.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let index = self.delay_scheme.min(schemes.len().saturating_sub(1));
        let delay = self
            .config
            .as_ref()
            .and_then(|config| config.keymaps.iter().filter(|km| km.id > 4).nth(index))
            .map(|keymap| keymap.delay.to_string())
            .unwrap_or_default();
        StackPanel::new().children((
            settings_view::combo_row(
                i18n::t("915"),
                &schemes,
                index,
                context.callback(|value: Option<usize>| Message::DelayScheme(value.unwrap_or(0))),
            ),
            settings_view::text_field(
                i18n::t("762"),
                &delay,
                context.callback(move |value: String| {
                    Message::Opt(OptEdit::SchemeDelay(index, value))
                }),
            ),
            settings_view::hint_row(i18n::t("763")),
        ))
    }

    /// 741 命令框皮肤（18 字段；748 透明度共用文案）。
    fn settings_section_skin(&self, context: &mut ViewContext<Self>) -> View {
        let mut rows: Vec<(usize, View)> = Vec::new();
        for (index, field) in settings::SKIN_FIELDS.iter().enumerate() {
            let value = self
                .config
                .as_ref()
                .and_then(|config| {
                    settings::skin_get(&config.options.command_input_skin, field.key)
                })
                .unwrap_or("")
                .to_string();
            rows.push((
                index,
                settings_view::skin_row(
                    i18n::t(field.label_key),
                    &value,
                    context
                        .callback(move |value: String| Message::Opt(OptEdit::Skin(index, value))),
                ),
            ));
        }
        StackPanel::new().keyed_children(rows)
    }

    /// 2503 命令框字体：路径+[浏览] / 字重+[恢复默认] 两行 (按钮在对应控件右侧)。
    fn settings_section_font(&self, context: &mut ViewContext<Self>) -> View {
        let (source, weight_index) = self
            .config
            .as_ref()
            .map(|config| {
                let index = settings::FONT_WEIGHTS
                    .iter()
                    .position(|weight| *weight == config.options.command_font.weight)
                    .unwrap_or(2);
                (config.options.command_font.source_path.clone(), index)
            })
            .unwrap_or_default();
        let weights: Vec<String> = settings::FONT_WEIGHTS
            .iter()
            .map(|w| i18n::t(settings::font_weight_label_key(w)))
            .collect();
        StackPanel::new().children((
            settings_view::text_field_end_button(
                i18n::t("2504"),
                &source,
                context.callback(|value: String| Message::Opt(OptEdit::FontSource(value))),
                i18n::t("2582"),
                context.message(Message::FontBrowse),
            ),
            settings_view::combo_row_end_button(
                i18n::t("2508"),
                &weights,
                weight_index,
                context.callback(|value: Option<usize>| {
                    Message::Opt(OptEdit::FontWeight(value.unwrap_or(2)))
                }),
                i18n::t("2507"),
                context.message(Message::Opt(OptEdit::FontReset)),
            ),
        ))
    }

    /// 907 编辑路径变量：行编辑 + 新增（933）。
    fn settings_section_path_vars(&self, context: &mut ViewContext<Self>) -> View {
        let variables = self
            .config
            .as_ref()
            .map(|config| config.options.path_variables.clone())
            .unwrap_or_default();
        let mut rows: Vec<(usize, View)> = Vec::new();
        for (row, variable) in variables.iter().enumerate() {
            rows.push((
                row,
                settings_view::pathvar_row(
                    &variable.name,
                    &variable.value,
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::PathVarName(row, value))
                    }),
                    context.callback(move |value: String| {
                        Message::Opt(OptEdit::PathVarValue(row, value))
                    }),
                    context.message(Message::Opt(OptEdit::PathVarRemove(row))),
                ),
            ));
        }
        StackPanel::new().children((
            settings_view::pathvar_header(),
            settings_view::hint_row(i18n::t("911")),
            StackPanel::new().keyed_children(rows),
            Button::new()
                .on_click(context.message(Message::Opt(OptEdit::PathVarAdd)))
                .content(TextBlock::new().text(i18n::t("933"))),
        ))
    }
}
