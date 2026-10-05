//! `app` 的 views：页面装配方法（把 Shell 状态 + ViewContext 翻译成 View）。
//!
//! 自原 `app.rs` 的 `impl Shell` 拆分；纯代码搬移，行为不变。

use super::*;

impl Shell {
    /// 侧栏底部：分隔线 + 保存提示（成功绿 / 失败红）+ 保存按钮（旧 `DockPanel.Dock="Bottom"` 区）。
    pub(super) fn pane_footer(&self, context: &mut ViewContext<Self>) -> View {
        // 紧凑窄轨（浮层未展开）：只放得下图标按钮 —— 完整页脚（分隔线/提示/文字按钮）
        // 在 48px 轨内会被裁成窄条（2026-09-29 实测）。
        if !self.pane_overlay_open {
            return Button::new()
                .on_click(context.message(Message::Save))
                .horizontal_alignment(HorizontalAlignment::Center)
                .margin(theme::pad_md())
                .content(FontIcon::new().glyph("\u{E74E}")); // Save
        }
        let notice: View = match &self.notice {
            Some(text) => TextBlock::new()
                .text(text.clone())
                .font_size(theme::FONT_CAPTION)
                .foreground(if self.notice_error {
                    theme::solid(theme::ERROR_CRIMSON)
                } else {
                    theme::solid(theme::MUTED_GREEN)
                })
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            None => View::empty(),
        };

        StackPanel::new()
            .spacing(8.0)
            .margin(theme::pad_md())
            .children((
                Border::new()
                    .height(1.0)
                    .background(theme::border_faint())
                    .content(TextBlock::new().text("")),
                notice,
                Button::new()
                    .on_click(context.message(Message::Save))
                    .content(
                        TextBlock::new()
                            .text(i18n::t("507"))
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ),
            ))
    }

    /// 内容区三态互斥：加载中 / 错误 / 页面。
    pub(super) fn content(&self, context: &mut ViewContext<Self>) -> View {
        if self.loading {
            return StackPanel::new()
                .spacing(14.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    ProgressRing::new().is_active(true).width(48.0).height(48.0),
                    TextBlock::new()
                        .text(i18n::t("917"))
                        .foreground(theme::stone_gray())
                        .horizontal_alignment(HorizontalAlignment::Center),
                ));
        }

        if let Some(error) = &self.error {
            return StackPanel::new()
                .spacing(12.0)
                .max_width(560.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    // 旧错误页的 42px ⚠ 图标（`MainWindow.axaml:208-212`）
                    TextBlock::new()
                        .text("⚠")
                        .font_size(42.0)
                        .horizontal_alignment(HorizontalAlignment::Center),
                    TextBlock::new()
                        .text(i18n::t("919"))
                        .font_size(theme::FONT_SUBTITLE)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::near_black())
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(error.clone())
                        .foreground(theme::stone_gray())
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_wrapping(TextWrapping::Wrap),
                    Button::new()
                        .on_click(context.message(Message::Retry))
                        .content(i18n::t("920")),
                ));
        }

        self.page_view(context)
    }

    /// 当前页面标题（keymap 页用配置里的 name/hotkey）。
    pub(super) fn current_title(&self) -> String {
        match self.nav.get(self.page_index) {
            Some(entry) => {
                if entry.kind.title().is_empty() {
                    entry.label.clone()
                } else {
                    entry.kind.title()
                }
            }
            None => String::new(),
        }
    }

    pub(super) fn page_view(&self, context: &mut ViewContext<Self>) -> View {
        let Some(entry) = self.nav.get(self.page_index) else {
            return TextBlock::new().text("（无导航项）").into();
        };

        // 使用指南：消费 services::markdown 的块模型 → 原生控件（含链接/图片）。
        if entry.kind == PageKind::Guide {
            return self.guide_view(context);
        }

        // 插件页：统一插件卡（内置 QuickSwitch + 用户插件）
        if entry.kind == PageKind::Plugins {
            return self.plugins_page(context);
        }

        // 选项页（keymap id=4）：方案卡 + 手风琴分区
        if entry.kind == PageKind::Settings {
            return self.settings_page(context);
        }

        let hint = match entry.kind {
            PageKind::SelectedAction => {
                return self.selected_action_page(context);
            }
            PageKind::Plugins => "内置 QuickSwitch 卡 + 用户插件卡 + zip 导入 + 市场入口",
            PageKind::Settings => "快捷键方案 / 外观材质 / 语言 / 路径变量 / 其他设置",
            // 缩写页与矩阵页共享同一编辑核心（旧 `KeymapEditorCore`）：左侧换成 chips + 命令框。
            PageKind::Abbr(id) => {
                return self.abbr_page(context, id);
            }
            PageKind::Keymap(id) => {
                return self.keymap_page(context, id);
            }
            PageKind::Guide => unreachable!("Guide 已提前返回"),
        };
        self.placeholder_page(hint, &self.current_title())
    }

    // ---------------------------------------------------------- 选中动作页

    /// 选中动作页：页头 + 热键卡 + 文本/文件两张聚合卡（toggle + 详情编辑器）。
    pub(super) fn selected_action_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let sa_config = &config.selected_action;

        // 页头（旧截图复刻）：标题与三入口**同排**（标题 STAR 左，按钮 Auto 右）
        let actions: View = selected_action_view::page_actions(
            context.message(Message::MatchTypesOpen),
            context.message(Message::BehaviorsOpen),
            context.message(Message::SaAddOpen),
        );
        let header: View = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                TextBlock::new()
                    .text(i18n::t("914"))
                    .font_size(theme::FONT_PAGE_TITLE)
                    .font_weight(FontWeight::MEDIUM)
                    .foreground(theme::near_black())
                    .vertical_alignment(VerticalAlignment::Center),
                Border::new().grid_column(1).content(actions),
            ));
        // 说明条（960）：Sand 横幅（复刻旧 `.axaml:374-378`）
        let banner: View = selected_action_view::hint_bar(&i18n::t("960"));

        // 热键提示条：未保存（1077）优先于冲突（1025）与空热键警示（976）
        let mut hint = if self.hotkey_pending_save {
            i18n::t("1077")
        } else if self.sa_hotkey_conflict {
            i18n::t("1025")
        } else if sa_config.hotkey.is_empty() {
            i18n::t("976")
        } else {
            String::new()
        };
        if self.sa_hotkey_conflict && self.hotkey_pending_save {
            hint = format!(
                "{hint}
{}",
                i18n::t("1025")
            );
        }
        let hotkey: View = selected_action_view::hotkey_card(
            &sa_config.hotkey,
            sa_config.enable,
            &hint,
            context.callback(|text: String| Message::SaHotkey(text)),
            context.callback(|enabled: bool| Message::SaEnable(enabled)),
            context.message(Message::SaHotkeyClear),
        );

        let text_card = self.sa_type_card(context, MATCH_TEXT_TYPE);
        let file_card = self.sa_type_card(context, MATCH_FILE_EXT);

        // 旧页面容器：`Grid Margin="8,20,20,20"` + `StackPanel Spacing="14"`（内容宽 ≈740）
        ScrollViewer::new().content(
            StackPanel::new()
                .margin(Thickness::new(8.0, 20.0, 20.0, 20.0))
                .spacing(14.0)
                .max_width(760.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .children((header, banner, hotkey, text_card, file_card)),
        )
    }

    /// 一张聚合卡（`match_type` 分区）：卡头 + toggle 行 + 详情编辑器。
    pub(super) fn sa_type_card(
        &self,
        context: &mut ViewContext<Self>,
        match_type: &'static str,
    ) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("").into();
        };
        let toggles = sa::build_toggles(config, match_type);
        if toggles.is_empty() {
            return selected_action_view::type_card(
                TextBlock::new()
                    .text(sa::match_type_label(match_type))
                    .font_size(theme::FONT_CARD_TITLE)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(theme::accent_solid())
                    .into(),
            );
        }

        // 选择恢复规则（复刻 RebuildToggles）：保持原选（仍存在）→ 首个已配置 → 首个
        let stored = self.sa_selected_id(match_type).unwrap_or_default();
        let sel_id = if toggles.iter().any(|toggle| toggle.id == stored) {
            stored
        } else {
            toggles
                .iter()
                .find(|toggle| sa::find_mapping_for_type(config, match_type, &toggle.id).is_some())
                .map(|toggle| toggle.id.clone())
                .or_else(|| toggles.first().map(|toggle| toggle.id.clone()))
                .unwrap_or_default()
        };

        let mapping = sa::find_mapping_for_type(config, match_type, &sel_id);
        let is_selected_type = self.sa_selected_id(match_type).as_deref() == Some(sel_id.as_str());
        let header: View = selected_action_view::card_header(
            &sa::match_type_label(match_type),
            mapping.is_some(),
            mapping.is_some(),
            context.message(Message::SaPlaySample),
            context.message(Message::SaDeleteAsk),
        );
        let toggles_area: View = selected_action_view::toggles_row(&toggles, &sel_id, |id| {
            // 胶囊 = Border + on_pointer_pressed（Button 的 ControlCornerRadius 覆盖不生效）
            context.callback(move |_info: PointerEventInfo| Message::SaSelectToggle {
                match_type,
                id: id.clone(),
            })
        });

        // 文件后缀卡专属：常驻可编辑后缀框（直接展示/编辑当前选中**分组**的后缀串；
        // 数据源 = `config.file_groups`（chips 即由此渲染，预设分组出厂自带后缀）；
        // 文本特征卡不渲染。改动经 800ms 尾随防抖归一写回内存 file_groups（chips 数据源），
        // 落盘统一走页脚「保存配置」（无自动保存，2026-10-02 定版）。
        // 仅「group:」chip 可编辑（自定义类型 type: / 孤儿 orphan: 无 exts 数据源）。
        let exts_editor: View = if match_type == MATCH_FILE_EXT {
            let group_index = sel_id
                .strip_prefix("group:")
                .and_then(|name| config.file_groups.iter().position(|fg| fg.name == name));
            let dirty = match (&self.exts_edit, group_index) {
                (Some((dirty_index, text)), Some(index)) if *dirty_index == index => {
                    Some(text.clone())
                }
                _ => None,
            };
            let display_text = dirty.unwrap_or_else(|| {
                group_index
                    .and_then(|index| config.file_groups.get(index))
                    .map(|fg| fg.exts.join(","))
                    .unwrap_or_default()
            });
            // 校验错误回显（normalize_exts 后端校验失败经 mt_status）
            let exts_status: View = match &self.mt_status {
                Some((text, is_error)) if *is_error => selected_action_view::hint_bar(text),
                _ => View::empty(),
            };
            match (group_index, exts_status) {
                (Some(index), status) => View::fragment((
                    selected_action_view::exts_editor(
                        index,
                        display_text,
                        self.exts_edit.clone(),
                        context.callback(move |(fg_index, value): (usize, String)| {
                            Message::SaExtsEditValue(fg_index, value)
                        }),
                    ),
                    status,
                )),
                (None, status) => status,
            }
        } else {
            View::empty()
        };

        // 详情面板
        let detail: View = if let Some(mapping) = mapping {
            let covering = sa::covering(&self.catalog, match_type, &mapping.match_value);
            let mut rows: Vec<(String, View)> = Vec::new();
            let entry_count = mapping.entries.len();
            for (index, entry) in mapping.entries.iter().enumerate() {
                let behavior = entry.behavior.clone();
                // 行为切换下拉：覆盖行为全集；当前行为不在覆盖集（脏值）时追加兜底项
                let mut switch_items: Vec<String> = covering
                    .iter()
                    .map(|pack| self.catalog.label_for(&pack.id))
                    .collect();
                let current_in_covering = covering.iter().any(|pack| pack.id == behavior);
                let switch_selected = if current_in_covering {
                    covering.iter().position(|pack| pack.id == behavior)
                } else {
                    switch_items.push(self.catalog.label_for(&behavior));
                    Some(switch_items.len() - 1)
                };
                rows.push((
                    format!("entry-{index}"),
                    // 旧行编辑器套 `rowEditor` 子卡（Ivory 面 + 圆角 4 + Padding 10）
                    selected_action_view::row_editor(selected_action_view::entry_row(
                        index,
                        switch_items,
                        switch_selected,
                        &entry.action_value,
                        &entry.working_dir,
                        self.catalog.is_no_value(&behavior),
                        index > 0,
                        index + 1 < entry_count,
                        context.callback(move |pick: Option<usize>| match pick {
                            Some(pick) => Message::SaEntrySwitch {
                                match_type,
                                index,
                                pick,
                            },
                            None => Message::Noop,
                        }),
                        context.callback(move |value: String| Message::SaEntryValue {
                            match_type,
                            index,
                            value,
                        }),
                        context.callback(move |value: String| Message::SaEntryWorkingDir {
                            match_type,
                            index,
                            value,
                        }),
                        context.message(Message::SaEntryMove {
                            match_type,
                            index,
                            delta: -1,
                        }),
                        context.message(Message::SaEntryMove {
                            match_type,
                            index,
                            delta: 1,
                        }),
                        context.message(Message::SaRemoveEntry { match_type, index }),
                    )),
                ));
            }

            // 「添加行为」：自动选首个未用覆盖行为（pick 为 None 时），禁用原因 1107/1119
            let picked = self.sa_picked(match_type, covering.len());
            let first_unused = covering.iter().position(|pack| {
                !mapping
                    .entries
                    .iter()
                    .any(|entry| entry.behavior == pack.id)
            });
            let effective_pick = picked.or(first_unused);
            let full = mapping.entries.len() >= 9;
            let exhausted = first_unused.is_none();
            let hint = if full {
                Some(i18n::t("1107"))
            } else if exhausted {
                Some(i18n::t("1119"))
            } else {
                None
            };
            let can_add = !full && !exhausted;
            rows.push((
                "add".to_string(),
                selected_action_view::add_behavior_row(
                    self.sa_covering_labels(match_type, &mapping.match_value),
                    effective_pick,
                    can_add,
                    hint,
                    context.callback(move |pick: Option<usize>| Message::SaPickBehavior {
                        match_type,
                        pick,
                    }),
                    context.message(Message::SaAddBehavior { match_type }),
                ),
            ));

            StackPanel::new()
                .spacing(8.0)
                .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                .keyed_children(rows)
        } else {
            let match_value = sa::transient_match_value(config, match_type, &sel_id);
            let covering = sa::covering(&self.catalog, match_type, &match_value);
            let picked = self.sa_picked(match_type, covering.len());
            let can_add = picked.is_some();

            StackPanel::new()
                .spacing(8.0)
                .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                .children((
                    selected_action_view::pending_hint(sa::has_dedicated_behavior_for(
                        &self.catalog,
                        match_type,
                        &match_value,
                    )),
                    selected_action_view::add_behavior_row(
                        self.sa_covering_labels(match_type, &match_value),
                        picked,
                        can_add,
                        None,
                        context.callback(move |pick: Option<usize>| Message::SaPickBehavior {
                            match_type,
                            pick,
                        }),
                        context.message(Message::SaAddBehavior { match_type }),
                    ),
                ))
        };

        // 卡内「＋ 新建匹配类型」（2553）：按卡种类预置 text/fileExt 草稿
        let new_type: View =
            selected_action_view::new_type_link(context.message(Message::SaNewType {
                kind: if match_type == MATCH_TEXT_TYPE {
                    "text"
                } else {
                    "fileExt"
                },
            }));
        let mut card_children: Vec<(usize, View)> = vec![
            (0, header),
            (1, toggles_area),
            (2, exts_editor),
            (3, new_type),
            (4, detail),
        ];
        // 页内状态条（▶ 执行失败等）：仅渲染在**当前点亮**的卡上
        if is_selected_type && let Some((text, is_error)) = &self.sa_status {
            card_children.push((
                card_children.len(),
                TextBlock::new()
                    .text(text.clone())
                    .font_size(theme::FONT_CAPTION)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(if *is_error {
                        theme::solid(theme::ERROR_CRIMSON)
                    } else {
                        theme::accent_solid()
                    })
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            ));
        }

        selected_action_view::type_card(
            StackPanel::new().spacing(8.0).keyed_children(card_children),
        )
    }

    /// 手风琴分区卡包装：展开时才构建内容（一次只展开一张，复刻旧版）。
    pub(super) fn section(
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
    pub(super) fn settings_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let schemes: Vec<&Keymap> = config.keymaps.iter().filter(|km| km.id > 4).collect();

        // 左列：快捷键方案（915）——行内直接编辑名称/触发键/开关
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

        // 左列卡：旧 `Border.leftPanel`（Ivory 面 + 淡冷边 2px + 圆角 14 + Padding 16）；
        // 标题 915 旧 16 SemiBold + 底距 12
        let left: View = Border::new()
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
            );

        // 右列：分区栈
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

        // 505 其他设置：506 开机自启（即时生效）/ 901 隐藏矩阵 / 781 语言
        sections.push((
            sections.len(),
            self.section(context, "other", "505", |this, context| {
                let languages = vec!["中文".to_string(), "English".to_string()];
                let language_index = match this
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
                        this.config
                            .as_ref()
                            .map(|c| c.options.startup)
                            .unwrap_or(false),
                        context.callback(|value: bool| Message::StartupToggle(value)),
                    ),
                    settings_view::check_row(
                        i18n::t("902"),
                        this.config
                            .as_ref()
                            .map(|c| c.options.hide_matrix)
                            .unwrap_or(false),
                        context.callback(|value: bool| Message::Opt(OptEdit::HideMatrix(value))),
                    ),
                    settings_view::toggle_row(
                        i18n::t("2593"),
                        this.acrylic,
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
                ))
            }),
        ));

        // 601 编辑程序分组（哨兵 Exclude/Global 不进编辑器）
        sections.push((
            sections.len(),
            self.section(context, "groups", "601", |this, context| {
                let groups: Vec<&crate::models::WindowGroup> = this
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
                    let name_cb = context.callback(move |value: String| {
                        Message::Opt(OptEdit::GroupName(row, value))
                    });
                    let value_cb = context.callback(move |value: String| {
                        Message::Opt(OptEdit::GroupValue(row, value))
                    });
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
            }),
        ));

        // 1116 自定义热键（keymap id=1；「功能」列点击打开动作编辑对话框，
        // 复刻旧 SettingsPageView「功能列点击弹 ActionEditorWindow」交互）
        sections.push((
            sections.len(),
            self.section(context, "customhotkeys", "1116", |this, context| {
                let rows: Vec<(String, String)> = this
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
                let mut list: Vec<(usize, View)> =
                    vec![(usize::MAX, settings_view::hotkey_header())];
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
            }),
        ));

        // 701 修改鼠标参数（9 字段；字符串型数值原样透传，与旧版自由文本框一致）
        sections.push((
            sections.len(),
            self.section(context, "mouse", "701", |this, context| {
                let mouse = this
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
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseFastSingle(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("706"),
                        &mouse.fast_repeat,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseFastRepeat(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("707"),
                        &mouse.slow_single,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseSlowSingle(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("708"),
                        &mouse.slow_repeat,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseSlowRepeat(value)))),
                    ),
                    settings_view::text_field(
                        i18n::t("709"),
                        &mouse.tip_symbol,
                        context
                            .callback(text(|value| Message::Opt(OptEdit::MouseTipSymbol(value)))),
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
            }),
        ));

        // 712 滚轮相关参数（3 字段）
        sections.push((
            sections.len(),
            self.section(context, "scroll", "712", |this, context| {
                let scroll = this
                    .config
                    .as_ref()
                    .map(|config| config.options.scroll.clone())
                    .unwrap_or_default();
                StackPanel::new().children((
                    settings_view::text_field_wide_label(
                        i18n::t("713"),
                        &scroll.delay1,
                        context
                            .callback(|value: String| Message::Opt(OptEdit::ScrollDelay1(value))),
                    ),
                    settings_view::text_field_wide_label(
                        i18n::t("714"),
                        &scroll.delay2,
                        context
                            .callback(|value: String| Message::Opt(OptEdit::ScrollDelay2(value))),
                    ),
                    settings_view::text_field_wide_label(
                        i18n::t("715"),
                        &scroll.once_line_count,
                        context
                            .callback(|value: String| Message::Opt(OptEdit::ScrollOnceLine(value))),
                    ),
                ))
            }),
        ));

        // 721 修改键盘布局：多行文本 + 四个预设按钮
        sections.push((
            sections.len(),
            self.section(context, "layout", "721", |this, context| {
                let layout = this
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
                        .on_text_changed(context.callback(|value: String| {
                            Message::Opt(OptEdit::KeyboardLayoutSet(value))
                        })),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("0"))))
                                .content(TextBlock::new().text(i18n::t("723"))),
                            Button::new()
                                .on_click(
                                    context.message(Message::Opt(OptEdit::LayoutPreset("74"))),
                                )
                                .content(TextBlock::new().text(i18n::t("724"))),
                            Button::new()
                                .on_click(
                                    context.message(Message::Opt(OptEdit::LayoutPreset("104"))),
                                )
                                .content(TextBlock::new().text(i18n::t("725"))),
                            Button::new()
                                .on_click(context.message(Message::Opt(OptEdit::LayoutPreset("1"))))
                                .content(TextBlock::new().text(i18n::t("726"))),
                        )),
                ))
            }),
        ));

        // 761 设置触发延时：方案下拉 + 毫秒数（提示 763）
        sections.push((
            sections.len(),
            self.section(context, "delay", "761", |this, context| {
                let schemes: Vec<String> = this
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
                let index = this.delay_scheme.min(schemes.len().saturating_sub(1));
                let delay = this
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
                        context.callback(|value: Option<usize>| {
                            Message::DelayScheme(value.unwrap_or(0))
                        }),
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
            }),
        ));

        // 741 命令框皮肤（18 字段；748 透明度共用文案）
        sections.push((
            sections.len(),
            self.section(context, "skin", "741", |this, context| {
                let mut rows: Vec<(usize, View)> = Vec::new();
                for (index, field) in settings::SKIN_FIELDS.iter().enumerate() {
                    let value = this
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
                            context.callback(move |value: String| {
                                Message::Opt(OptEdit::Skin(index, value))
                            }),
                        ),
                    ));
                }
                StackPanel::new().keyed_children(rows)
            }),
        ));

        // 2503 命令框字体：路径 + 浏览 + 字重 + 恢复默认
        sections.push((
            sections.len(),
            self.section(context, "font", "2503", |this, context| {
                let (source, weight_index) = this
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
                    settings_view::text_field(
                        i18n::t("2504"),
                        &source,
                        context.callback(|value: String| Message::Opt(OptEdit::FontSource(value))),
                    ),
                    settings_view::button_row(
                        "",
                        i18n::t("2582"),
                        context.message(Message::FontBrowse),
                    ),
                    settings_view::combo_row(
                        i18n::t("2508"),
                        &weights,
                        weight_index,
                        context.callback(|value: Option<usize>| {
                            Message::Opt(OptEdit::FontWeight(value.unwrap_or(2)))
                        }),
                    ),
                    settings_view::button_row(
                        "",
                        i18n::t("2507"),
                        context.message(Message::Opt(OptEdit::FontReset)),
                    ),
                ))
            }),
        ));

        // 907 编辑路径变量：行编辑 + 新增（933）
        sections.push((
            sections.len(),
            self.section(context, "pathvars", "907", |this, context| {
                let variables = this
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
            }),
        ));

        // 右列：旧 `StackPanel Width="460" Spacing="16" Margin="24,0,24,24"`（卡间距由
        // section_card 自带底距 16 承担）；整页容器 = 旧 `Margin="24,20,24,28"`
        let right: View = ScrollViewer::new().content(
            StackPanel::new()
                .width(460.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .margin(Thickness::new(24.0, 0.0, 24.0, 24.0))
                .keyed_children(sections),
        );

        Grid::new()
            .margin(Thickness::new(24.0, 20.0, 24.0, 28.0))
            .columns([GridLength::Pixel(560.0), GridLength::STAR])
            .children((left, Border::new().grid_column(1).content(right)))
    }

    /// 插件页：页头入口 + 分区说明 + 统一卡片列表 + 三态。
    ///
    /// 卡片由「当前配置 + 目录快照」**每次渲染即时派生** ⇒ 开关状态天然与配置同步
    /// （无需额外的双向同步标记，旧版的 `_syncingFromConfig` 因此省去）。
    pub(super) fn plugins_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let english = matches!(i18n::language(), i18n::Lang::En);
        let catalog = self.plugin_catalog.clone().unwrap_or_default();
        let cards = plugins::build_cards(config, &catalog);

        let mut rows: Vec<(usize, View)> = Vec::new();
        rows.push((
            rows.len(),
            plugins_view::page_header(
                context.message(Message::PluginsMarket),
                context.message(Message::PluginImport),
            ),
        ));

        if let Some(status) = &self.plugin_status {
            rows.push((rows.len(), plugins_view::status_banner(status)));
        }

        // 统一插件列表（旧版顺序：卡列表在前，加载/告警/空态随后）
        for card in &cards {
            let toggle_id = card.id.clone();
            let is_builtin = card.is_builtin;
            let delete_id = card.id.clone();
            let configure_id = card.id.clone();
            rows.push((
                rows.len(),
                plugins_view::plugin_card(
                    card,
                    english,
                    context.callback(move |enabled: bool| Message::PluginToggle {
                        id: toggle_id.clone(),
                        is_builtin,
                        enabled,
                    }),
                    context.message(Message::PluginDelete(delete_id)),
                    context.callback(move |_info: PointerEventInfo| {
                        Message::PluginConfigure(configure_id.clone())
                    }),
                ),
            ));
        }

        if self.plugins_loading {
            rows.push((rows.len(), plugins_view::loading()));
        } else if let Some(error) = &self.plugins_error {
            rows.push((
                rows.len(),
                plugins_view::load_error(error, None, context.message(Message::PluginsReload)),
            ));
        } else if let Some(error) = &self.plugins_action_error {
            // 一次性操作失败：纯文字横幅（无重试按钮，重试语义只属于目录加载）
            rows.push((rows.len(), plugins_view::action_error(error)));
        } else if plugins::show_empty_state(false, None, &cards) {
            rows.push((rows.len(), plugins_view::empty_state()));
        }

        // 页尾：运行时边界说明 + 配置引导（旧版在列表之后）
        rows.push((rows.len(), plugins_view::footer_notes()));

        // 旧 `StackPanel Margin="36,32,36,40" Spacing="16" MaxWidth="820"`（左对齐）
        ScrollViewer::new().content(
            StackPanel::new()
                .spacing(16.0)
                .max_width(820.0)
                .horizontal_alignment(HorizontalAlignment::Left)
                .margin(Thickness::new(36.0, 32.0, 36.0, 40.0))
                .keyed_children(rows),
        )
    }

    /// 动作编辑面板：两级下拉 + 按类型分发的编辑器。
    pub(super) fn action_editor_panel(&self, context: &mut ViewContext<Self>) -> View {
        let Some(config) = self.config.as_ref() else {
            return action_editor_view::frame(action_editor_view::hint("配置未加载"));
        };

        // 窗口分组下拉（复刻 `windowGroups.filter(id >= 0)`）
        let groups = action_editor::window_group_options(config);
        let group_ids: Vec<i32> = groups.iter().map(|group| group.id).collect();
        let group_index = group_ids.iter().position(|id| *id == self.window_group_id);
        let group_combo = action_editor_view::combo(
            groups.iter().map(|group| group.name.clone()).collect(),
            group_index,
            true,
            context.callback({
                let ids = group_ids.clone();
                move |index: Option<usize>| match index.and_then(|i| ids.get(i).copied()) {
                    Some(id) => Message::SelectWindowGroup(id),
                    None => Message::Noop,
                }
            }),
        );

        // 动作类型下拉（未选键时禁用；缩写语境隐藏 4/5）
        let is_abbr = self.current_keymap().map(keymap::is_abbr).unwrap_or(false);
        let types = action_editor::type_options(is_abbr);
        let type_ids: Vec<i32> = types.iter().map(|option| option.id).collect();
        let has_hotkey = self.selected_hotkey.is_some();
        let action = self.current_action();
        let type_index = match action {
            Some(action) => type_ids.iter().position(|id| *id == action.type_id),
            None => Some(0),
        };
        let type_combo = action_editor_view::combo(
            types.iter().map(|option| option.label()).collect(),
            type_index,
            has_hotkey,
            context.callback({
                let ids = type_ids.clone();
                move |index: Option<usize>| match index.and_then(|i| ids.get(i).copied()) {
                    Some(id) => Message::SelectActionType(id),
                    None => Message::Noop,
                }
            }),
        );

        // 编辑器主体（按 typeId 分发；复刻 `RebuildEditor`）
        let body: View = match action {
            None => action_editor_view::hint(if has_hotkey {
                "该键在当前窗口分组下尚无动作，请选择动作类型"
            } else {
                // 措辞需同时适配「模式页键盘网格」与「缩写页 chips」两种宿主
                "请先从上方选中一个键或缩写条目"
            }),
            Some(action) => match action.type_id {
                0 => action_editor_view::hint("未配置"),
                1 => self.editor_activate_or_run(action, context),
                2 | 3 | 4 | 7 | 9 => self.editor_radio_group(action, context),
                5 => self.editor_remap(action, context),
                6 => self.editor_send_keys(action, context),
                8 => self.editor_ahk_code(action, context),
                _ => action_editor_view::hint("未知动作类型"),
            },
        };

        // 顶部两个下拉**无标签**（忠实复刻旧面板：靠选项内容自解释）
        let header: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(20.0)
            .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
            .children((group_combo, type_combo));

        let rows: Vec<(usize, View)> =
            vec![(0, header), (1, action_editor_view::divider()), (2, body)];

        action_editor_view::frame(StackPanel::new().spacing(0.0).keyed_children(rows))
    }

    /// 类型 1：启动程序或激活窗口。
    pub(super) fn editor_activate_or_run(
        &self,
        action: &Action,
        context: &mut ViewContext<Self>,
    ) -> View {
        let win_title = action_editor_view::text_box(
            &action.win_title,
            false,
            context.callback(|value: String| Message::EditField(ActionField::WinTitle(value))),
        );
        let args = action_editor_view::text_box(
            &action.args,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Args(value))),
        );
        let working_dir = action_editor_view::text_box(
            &action.working_dir,
            false,
            context.callback(|value: String| Message::EditField(ActionField::WorkingDir(value))),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Comment(value))),
        );

        // 目标：文本框 + 快捷方式下拉（选中即填入目标，复刻旧版的 shortcuts 下拉）
        let target = action_editor_view::text_box(
            &action.target,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Target(value))),
        );
        let shortcuts: View = if self.shortcuts.is_empty() {
            View::empty()
        } else {
            let paths = self.shortcuts.clone();
            action_editor_view::combo(
                paths.clone(),
                None,
                true,
                context.callback(move |index: Option<usize>| {
                    match index.and_then(|i| paths.get(i).cloned()) {
                        Some(path) => Message::EditField(ActionField::Target(path)),
                        None => Message::Noop,
                    }
                }),
            )
        };

        let error: View = match action_editor::evaluate_win_title_error(&action.win_title) {
            Some(message) => action_editor_view::field_error(message),
            None => View::empty(),
        };
        let hint: View = TextBlock::new()
            .text(i18n::t("301hint"))
            .font_size(theme::FONT_CAPTION)
            .foreground(theme::stone_gray())
            .text_wrapping(TextWrapping::Wrap)
            .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
            .into();
        let spy: View = Button::new()
            .on_click(context.message(Message::WindowSpy))
            .content(i18n::t("309"));
        let target_rows: Vec<(usize, View)> = vec![(0, target), (1, shortcuts)];

        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("301"), win_title)),
            (1, hint),
            (2, error),
            (
                3,
                action_editor_view::field(
                    i18n::t("302"),
                    StackPanel::new().spacing(6.0).keyed_children(target_rows),
                ),
            ),
            (4, action_editor_view::field(i18n::t("303"), args)),
            (5, action_editor_view::field(i18n::t("304"), working_dir)),
            (6, action_editor_view::field(i18n::t("305"), comment)),
            (
                7,
                action_editor_view::toggle(
                    i18n::t("306"),
                    action.run_as_admin,
                    context
                        .callback(|value: bool| Message::EditField(ActionField::RunAsAdmin(value))),
                ),
            ),
            (
                8,
                action_editor_view::toggle(
                    i18n::t("307"),
                    action.run_in_background,
                    context.callback(|value: bool| {
                        Message::EditField(ActionField::RunInBackground(value))
                    }),
                ),
            ),
            (
                9,
                action_editor_view::toggle(
                    i18n::t("308"),
                    action.detect_hidden_window,
                    context.callback(|value: bool| {
                        Message::EditField(ActionField::DetectHiddenWindow(value))
                    }),
                ),
            ),
            (10, spy),
        ];

        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 类型 2/3/4/7/9：枚举单选（两两一行）。
    ///
    /// P7b：type 9 追加插件动作动态组（目录聚合 `provides.actions[]`；内置快路径
    /// 1-8 红线不动）。目录未加载或无声明 ⇒ 不渲染插件动作区。
    pub(super) fn editor_radio_group(
        &self,
        action: &Action,
        context: &mut ViewContext<Self>,
    ) -> View {
        let is_abbr = self.current_keymap().map(keymap::is_abbr).unwrap_or(false);
        let rows = action_editor::radio_rows(action.type_id, is_abbr);
        let group_name = format!("kf-radio-{}-{}", action.type_id, self.window_group_id);
        let mut sections: Vec<(usize, View)> = vec![(
            0,
            action_editor_view::radio_rows(&rows, &group_name, action.value_id, |item| {
                context.callback(move |checked: bool| {
                    if checked {
                        Message::SelectRadio {
                            value_id: item.value_id,
                            label_key: item.label_key,
                        }
                    } else {
                        Message::Noop
                    }
                })
            }),
        )];
        if action.type_id == 9 {
            let groups = action_editor::plugin_action_groups(self.plugin_catalog.as_ref());
            if !groups.is_empty() {
                sections.push((
                    1,
                    action_editor_view::plugin_action_rows(
                        &groups,
                        &group_name,
                        &action.action_id,
                        |item| {
                            let action_id = item.full_id.clone();
                            context.callback(move |checked: bool| {
                                if checked {
                                    Message::SelectPluginAction {
                                        action_id: action_id.clone(),
                                    }
                                } else {
                                    Message::Noop
                                }
                            })
                        },
                    ),
                ));
            }
        }
        StackPanel::new().spacing(4.0).keyed_children(sections)
    }

    /// 类型 5：重映射按键。
    pub(super) fn editor_remap(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let single_press = self.selected_hotkey.as_deref() == Some("singlePress");
        let value = action_editor_view::text_box(
            &action.remap_to_key,
            false,
            context.callback(|v: String| Message::EditField(ActionField::RemapToKey(v))),
        );
        let candidates = action_editor::REMAP_ITEMS.to_vec();
        let picker = action_editor_view::combo(
            candidates.iter().map(|key| (*key).to_string()).collect(),
            None,
            !single_press,
            context.callback({
                let keys: Vec<String> = candidates.iter().map(|key| (*key).to_string()).collect();
                move |index: Option<usize>| match index.and_then(|i| keys.get(i).cloned()) {
                    Some(key) => Message::EditField(ActionField::RemapToKey(key)),
                    None => Message::Noop,
                }
            }),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|v: String| Message::EditField(ActionField::Comment(v))),
        );

        let mut rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("401"), value)),
            (1, picker),
        ];
        if single_press {
            rows.push((2, action_editor_view::field_error(i18n::t("954"))));
        }
        rows.push((3, action_editor_view::field(i18n::t("305"), comment)));
        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 类型 6：输入按键或文本。
    pub(super) fn editor_send_keys(
        &self,
        action: &Action,
        context: &mut ViewContext<Self>,
    ) -> View {
        let keys = action_editor_view::text_box(
            &action.keys_to_send,
            true,
            context.callback(|v: String| Message::EditField(ActionField::KeysToSend(v))),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|v: String| Message::EditField(ActionField::Comment(v))),
        );
        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("402"), keys)),
            (1, action_editor_view::field(i18n::t("305"), comment)),
        ];
        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 类型 8：自定义函数。
    pub(super) fn editor_ahk_code(&self, action: &Action, context: &mut ViewContext<Self>) -> View {
        let code = action_editor_view::text_box(
            &action.ahk_code,
            true,
            context.callback(|v: String| Message::EditField(ActionField::AhkCode(v))),
        );
        let comment = action_editor_view::text_box(
            &action.comment,
            false,
            context.callback(|v: String| Message::EditField(ActionField::Comment(v))),
        );

        // 示例下拉：选中即写入代码框（复刻 `BuiltinFunction.vue` 的 items）
        let examples: Vec<String> = action_editor::AHK_EXAMPLES
            .iter()
            .map(|item| (*item).to_string())
            .collect();
        let example_picker = action_editor_view::combo(
            examples.clone(),
            None,
            true,
            context.callback(move |index: Option<usize>| {
                match index.and_then(|i| examples.get(i).cloned()) {
                    Some(code) => Message::EditField(ActionField::AhkCode(code)),
                    None => Message::Noop,
                }
            }),
        );
        let tips: View = StackPanel::new()
            .spacing(2.0)
            .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
            .children((
                TextBlock::new()
                    .text(i18n::t("955"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(i18n::t("956"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
            ));

        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("403"), code)),
            (1, example_picker),
            (2, action_editor_view::field(i18n::t("305"), comment)),
            (3, tips),
        ];
        StackPanel::new().spacing(0.0).keyed_children(rows)
    }

    /// 键位/缩写页右侧备注汇总列（可折叠）。
    ///
    /// 展开时**定宽 340**：默认 1200 窗宽下左列（STAR）仍能完整容纳键盘网格
    /// （最宽行 ~700px，旧 `[STAR, STAR]` 半窗 ~564px 会横向裁切键格）；
    /// 折叠时窄轨按钮，左列近乎全宽。点按钮往复切换。
    pub(super) fn comments_column(
        &self,
        context: &mut ViewContext<Self>,
        entries: &[keymap::CommentEntry],
    ) -> View {
        if self.comments_collapsed {
            return Border::new()
                .grid_column(1)
                .margin(Thickness::new(12.0, 20.0, 8.0, 28.0))
                .vertical_alignment(VerticalAlignment::Top)
                .content(
                    Button::new()
                        .on_click(context.message(Message::ToggleComments))
                        .content(TextBlock::new().text("« 备注")),
                );
        }
        let comments: View = if entries.is_empty() {
            keymap_view::comment_empty_hint()
        } else {
            keymap_view::comment_summary(entries)
        };
        // ⚠️ 必须用 Grid 行约束而非 StackPanel：StackPanel 给子级无限高度，
        // ScrollViewer 拿不到边界 ⇒ 不滚动、列表底部溢出窗口被裁（2026-09-29 实测）。
        Border::new()
            .grid_column(1)
            .width(300.0)
            .margin(Thickness::new(20.0, 20.0, 4.0, 28.0))
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::STAR])
                    .children((
                        Border::new()
                            .grid_row(0)
                            .margin(Thickness::new(0.0, 0.0, 0.0, 4.0))
                            .content(
                                Button::new()
                                    .on_click(context.message(Message::ToggleComments))
                                    .horizontal_alignment(HorizontalAlignment::Right)
                                    // 两个状态（收起 » / « 备注）同用默认字号与前景色，
                                    // 保证点击前后按钮尺寸一致（用户定版 2026-10-03：
                                    // 此前收起态 12 号小字，比备注态小一圈）
                                    .content(TextBlock::new().text("收起 »")),
                            ),
                        Border::new().grid_row(1).content(comments),
                    )),
            )
    }

    /// 键位图页：页头 + 键盘网格 + 动作编辑面板 + 右侧备注汇总。
    pub(super) fn keymap_page(&self, context: &mut ViewContext<Self>, keymap_id: i32) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let Some(keymap) = config.keymaps.iter().find(|km| km.id == keymap_id) else {
            return TextBlock::new()
                .text(format!("未找到 keymap id={keymap_id}"))
                .foreground(theme::stone_gray())
                .into();
        };

        let rows = keymap::build_rows(&config.options.keyboard_layout, &keymap.hotkey);
        let font_size = keymap::key_font_size(keymap::small_font(&rows));
        let disabled = keymap::disabled_keys(config);
        let states = keymap_view::compute_states(
            &rows,
            keymap,
            &disabled,
            self.selected_hotkey.as_deref(),
            self.window_group_id,
        );

        let grid = keymap_view::keyboard_grid(&rows, &states, font_size, |hotkey| {
            context.message(Message::SelectKey(hotkey))
        });

        // 左列三段式：页头（Auto）/ 键盘网格（**Auto = 自然高度**）/ 动作面板（STAR）。
        // ⚠️ `View` 不实现 `LayoutControl` ⇒ `grid_row` 只能设在未收尾的 builder 上；
        //    已构建的 `View` 用 `Border` 包裹后再定位。
        //
        // ⚠️ 2026-10-01 修「点已绑定键后动作面板铺满整页、键盘网格被挤没」：
        //    旧结构 = 网格行 STAR + 面板行 **Auto**。Grid 的 Auto 行按**无限高度**测量子级 ⇒
        //    面板内容要多少给多少（实测单选表 ≈900 DIP），网格行被压到 0 高度；面板底部还会
        //    溢出窗口被裁，且**滚不动**（ScrollViewer 自认拿到全额高度，无滚动区间）。
        //    新结构 = **网格行 Auto**（自然高度，`max_height` 封顶防超大自定义布局）+
        //    **面板行 STAR**：① 网格永远拿满自己的自然高度 ⇒ 不被面板挤压、整张可见；
        //    ② 面板拿到「剩余高度」这个**有界**视口 ⇒ 内容再高也只滚不溢出；
        //    ③ 面板内容不足时**贴底**（`VerticalAlignment::Bottom`）⇒ 观感与旧版一致
        //       （小卡片仍在页面底部），内容一多就填满该区并滚轮浏览。
        //    两者都是纯**布局约束**：两个滚动区各自独立（指针落在哪个区就滚哪个），
        //    网格的显示与交互不受任何影响。
        let left: View = Border::new().grid_column(0).content(
            Grid::new()
                .rows([GridLength::Auto, GridLength::Auto, GridLength::STAR])
                .margin(Thickness::new(24.0, 20.0, 12.0, 28.0))
                .children((
                    Border::new().grid_row(0).content(keymap_view::page_header(
                        &keymap::header_title(keymap),
                        keymap::parent_info(keymap, config).as_deref(),
                    )),
                    Border::new().grid_row(1).content(
                        // 横向兜底：低逻辑宽（DPI 缩放/备注栏展开）下底行自然宽可能
                        // 超出可用宽，SinglePress 等行尾键会被裁 —— 允许横向滚动保底。
                        ScrollViewer::new()
                            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
                            .max_height(keymap_view::GRID_MAX_HEIGHT)
                            .content(grid),
                    ),
                    Border::new()
                        .grid_row(2)
                        .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                        .content(
                            ScrollViewer::new().content(
                                Border::new()
                                    .vertical_alignment(VerticalAlignment::Bottom)
                                    .content(self.action_editor_panel(context)),
                            ),
                        ),
                )),
        );

        let entries = keymap::build_comment_entries(keymap, config);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((left, self.comments_column(context, &entries)))
    }

    /// 缩写页（id 2/3）：页头 + chips 网格 + 命令框 + 动作编辑面板 + 右侧备注汇总。
    pub(super) fn abbr_page(&self, context: &mut ViewContext<Self>, keymap_id: i32) -> View {
        let Some(config) = self.config.as_ref() else {
            return TextBlock::new().text("配置未加载").into();
        };
        let Some(keymap) = config.keymaps.iter().find(|km| km.id == keymap_id) else {
            return TextBlock::new()
                .text(format!("未找到 keymap id={keymap_id}"))
                .foreground(theme::stone_gray())
                .into();
        };

        let disabled = keymap::disabled_keys(config);
        let chips = abbr::build_chips(keymap, &disabled, self.selected_hotkey.as_deref());
        let chips_area: View = if chips.is_empty() {
            abbr_view::empty_hint("（暂无缩写条目：在下方命令框输入内容并回车即可新增）")
        } else {
            // 统一格宽：依最长标签自适应（`VariableSizedWrapGrid` 的硬性要求）
            let item_width = abbr::chip_item_width(chips.iter().map(|chip| chip.label.as_str()));
            abbr_view::chip_grid(&chips, item_width, |chip| {
                context.message(Message::SelectKey(chip.hotkey))
            })
        };
        let command: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .keyed_children(vec![
                (
                    0usize,
                    abbr_view::command_box(
                        &self.cmd_text,
                        &i18n::t("406"),
                        context.callback(|value: String| Message::CmdText(value)),
                    ),
                ),
                (
                    1usize,
                    abbr_view::run_button(i18n::t("920"), context.message(Message::RunCmd)),
                ),
            ]);

        let header: View = keymap_view::page_header(
            &keymap::header_title(keymap),
            keymap::parent_info(keymap, config).as_deref(),
        );

        // 左列四段式：页头 / chips / 命令框 / 动作面板（STAR 行 ⇒ 面板有界可滚）
        let left: View = Border::new().grid_column(0).content(
            Grid::new()
                .rows([
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::STAR,
                ])
                .margin(Thickness::new(24.0, 20.0, 12.0, 28.0))
                .children((
                    Border::new().grid_row(0).content(header),
                    Border::new().grid_row(1).content(chips_area),
                    Border::new()
                        .grid_row(2)
                        .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                        .content(command),
                    Border::new()
                        .grid_row(3)
                        .margin(Thickness::new(0.0, 18.0, 0.0, 0.0))
                        .content(ScrollViewer::new().content(self.action_editor_panel(context))),
                )),
        );

        // 缩写页备注用 `format_space` 口径（原样键 + 尾部空格可见）
        let entries = abbr::build_comment_entries(keymap, config);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((left, self.comments_column(context, &entries)))
    }

    /// 指南页：`config.overviewDocMd` 优先，为空时已在后台拉取 `/config_doc.md`。
    ///
    /// 底部有「编辑指南」入口（复刻旧 `EditZoneHint` 虚线编辑区 → `OverviewEditWindow`）。
    pub(super) fn guide_view(&self, context: &mut ViewContext<Self>) -> View {
        let Some(port) = self.port else {
            return TextBlock::new().text("后端未连接").into();
        };

        let body: View = if self.doc_md.trim().is_empty() {
            // 文档不可达空态：内置快速上手引导（复刻旧 `HomePageView.axaml:42-56` 的
            // 932 标题 + 934-938 文案，不再是一行硬编码中文）
            ScrollViewer::new().content(
                StackPanel::new()
                    .spacing(10.0)
                    .max_width(720.0)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .margin(Thickness::new(28.0, 20.0, 28.0, 28.0))
                    .children((
                        TextBlock::new()
                            .text(self.current_title())
                            .font_size(28.0)
                            .font_weight(FontWeight::BOLD)
                            .foreground(theme::near_black()),
                        TextBlock::new()
                            .text(i18n::t("932"))
                            .font_size(theme::FONT_CARD_TITLE)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .foreground(theme::near_black()),
                        TextBlock::new()
                            .text(i18n::t("934"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("935"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("936"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("937"))
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(i18n::t("938"))
                            .foreground(theme::stone_gray())
                            .text_wrapping(TextWrapping::Wrap),
                        Self::guide_edit_entry(context),
                    )),
            )
        } else {
            let blocks = markdown::parse(&self.doc_md);
            let rendered = markdown_view::render(&blocks, port);
            let content: View = StackPanel::new()
                .spacing(10.0)
                .margin(Thickness::new(28.0, 20.0, 28.0, 28.0))
                .children((
                    TextBlock::new()
                        .text(self.current_title())
                        .font_size(28.0)
                        .font_weight(FontWeight::BOLD)
                        .foreground(theme::near_black()),
                    rendered,
                    // 页脚来源说明（旧 `HomePageView.axaml:33-34` 的 931，WarmSilver 12px）
                    TextBlock::new()
                        .text(i18n::t("931"))
                        .font_size(theme::FONT_CAPTION)
                        .foreground(theme::stone_gray())
                        .text_wrapping(TextWrapping::Wrap),
                    Self::guide_edit_entry(context),
                ));
            // 文档较长 ⇒ 纵向滚动（Fluent：内容区可滚动，页面不整体滚动）
            ScrollViewer::new().content(content)
        };

        body
    }

    pub(super) fn placeholder_page(&self, hint: &str, title: &str) -> View {
        let keymap_count = self
            .config
            .as_ref()
            .map(|config| config.keymaps.len())
            .unwrap_or(0);

        StackPanel::new()
            .spacing(10.0)
            .margin(theme::pad_lg())
            .children((
                TextBlock::new()
                    .text(title.to_string())
                    .font_size(28.0)
                    .font_weight(FontWeight::BOLD)
                    .foreground(theme::near_black()),
                TextBlock::new()
                    .text(hint.to_string())
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(format!("后端已连接 · keymap {keymap_count} 个"))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::SLATE_GRAY)),
                Border::new()
                    .padding(theme::pad_md())
                    .background(theme::card_background())
                    .border_brush(theme::card_stroke())
                    .border_thickness(theme::hairline())
                    .corner_radius(theme::radius_md())
                    .content(
                        TextBlock::new()
                            .text("Phase 3 进行中：本页内容待迁移。")
                            .opacity(0.75),
                    ),
            ))
    }
}
