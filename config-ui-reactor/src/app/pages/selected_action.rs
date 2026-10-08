//! `app::pages::selected_action` —— 选中动作页：页头 + 热键卡 + 文本/文件两张聚合卡（toggle + 详情编辑器）。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。
//!
//! 2026-10-09 结构优化：`sa_type_card` 原为单函数 236 行。现按职责抽出
//! 后缀编辑器（`sa_exts_editor`）与详情面板（`sa_detail_panel` → `sa_detail_for_mapping`
//! / `sa_entry_rows`）；卡内子件组装顺序不变（顺序敏感）。

use super::super::*;

impl Shell {
    /// 选中动作页：页头 + 热键卡 + 文本/文件两张聚合卡（toggle + 详情编辑器）。
    pub(in crate::app) fn selected_action_page(&self, context: &mut ViewContext<Self>) -> View {
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
    pub(in crate::app) fn sa_type_card(
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

        let exts_editor = self.sa_exts_editor(context, config, match_type, &sel_id);
        let detail = self.sa_detail_panel(context, config, match_type, &sel_id, mapping);

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

    /// 文件后缀卡专属：常驻可编辑后缀框（直接展示/编辑当前选中**分组**的后缀串；
    /// 数据源 = `config.file_groups`（chips 即由此渲染，预设分组出厂自带后缀）；
    /// 文本特征卡返回空视图。改动经 800ms 尾随防抖归一写回内存 file_groups（chips 数据源），
    /// 落盘统一走页脚「保存配置」（无自动保存，2026-10-02 定版）。
    /// 仅「group:」chip 可编辑（自定义类型 type: / 孤儿 orphan: 无 exts 数据源）。
    fn sa_exts_editor(
        &self,
        context: &mut ViewContext<Self>,
        config: &crate::models::Config,
        match_type: &'static str,
        sel_id: &str,
    ) -> View {
        if match_type != MATCH_FILE_EXT {
            return View::empty();
        }
        let group_index = sel_id
            .strip_prefix("group:")
            .and_then(|name| config.file_groups.iter().position(|fg| fg.name == name));
        let dirty = match (&self.exts_edit, group_index) {
            (Some((dirty_index, text)), Some(index)) if *dirty_index == index => Some(text.clone()),
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
    }

    /// 详情面板：已配置 → 行编辑器 + 添加行；未配置 → 待配置提示 + 添加行。
    fn sa_detail_panel(
        &self,
        context: &mut ViewContext<Self>,
        config: &crate::models::Config,
        match_type: &'static str,
        sel_id: &str,
        mapping: Option<&crate::models::SelectedMapping>,
    ) -> View {
        let Some(mapping) = mapping else {
            let match_value = sa::transient_match_value(config, match_type, sel_id);
            let covering = sa::covering(&self.catalog, match_type, &match_value);
            let selected = self.sa_selected(match_type, covering.len());
            let can_add = selected.is_some();
            return StackPanel::new()
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
                        selected,
                        can_add,
                        None,
                        context.callback(move |selected: Option<usize>| {
                            Message::SaSelectBehavior {
                                match_type,
                                selected,
                            }
                        }),
                        context.message(Message::SaAddBehavior { match_type }),
                    ),
                ));
        };

        let covering = sa::covering(&self.catalog, match_type, &mapping.match_value);
        let mut rows = self.sa_entry_rows(context, match_type, mapping, &covering);

        // 「添加行为」：自动选首个未用覆盖行为（selected 为 None 时），禁用原因 1107/1119
        let selected = self.sa_selected(match_type, covering.len());
        let first_unused = covering.iter().position(|pack| {
            !mapping
                .entries
                .iter()
                .any(|entry| entry.behavior == pack.id)
        });
        let effective_selected = selected.or(first_unused);
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
                effective_selected,
                can_add,
                hint,
                context.callback(move |selected: Option<usize>| Message::SaSelectBehavior {
                    match_type,
                    selected,
                }),
                context.message(Message::SaAddBehavior { match_type }),
            ),
        ));

        StackPanel::new()
            .spacing(8.0)
            .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
            .keyed_children(rows)
    }

    /// 已配置 mapping 的 entry 行列表（行为切换下拉 + 值/工作目录 + 上移/下移/删除）。
    fn sa_entry_rows(
        &self,
        context: &mut ViewContext<Self>,
        match_type: &'static str,
        mapping: &crate::models::SelectedMapping,
        covering: &[&crate::models::BehaviorPack],
    ) -> Vec<(String, View)> {
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
                    context.callback(move |selected: Option<usize>| match selected {
                        Some(selected) => Message::SaEntrySwitch {
                            match_type,
                            index,
                            selected,
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
        rows
    }
}
