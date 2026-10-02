//! `app` 的 dialogs：对话框装配方法（窗口弹层）。
//!
//! 自原 `app.rs` 的 `impl Shell` 拆分；纯代码搬移，行为不变。

use super::*;

impl Shell {
    /// 插件声明式设置对话框（`ContentDialog`；表单按 manifest 声明渲染）。
    pub(super) fn plugin_settings_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.ps_open {
            return View::empty();
        }

        let english = matches!(i18n::language(), i18n::Lang::En);
        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some(error) = &self.ps_error {
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(error.clone())
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::solid(theme::ERROR_CRIMSON))
                    .text_wrapping(TextWrapping::Wrap)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 10.0))
                    .into(),
            ));
        }

        if self.ps_loading {
            rows.push((rows.len(), plugins_view::loading()));
        } else if self.ps_rows.is_empty() {
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(i18n::t("2586"))
                    .foreground(theme::stone_gray())
                    .into(),
            ));
        }

        for (index, (setting, value)) in self.ps_rows.iter().enumerate() {
            rows.push((
                rows.len(),
                plugins_view::setting_row(
                    setting,
                    value,
                    english,
                    context.callback(move |value: String| Message::PsValue(index, value)),
                    // 开关复用**同一条** `PsValue` 字符串通道（"true"/"false"）——
                    // `bool` 在协议里就是字符串承载，没必要为一个控件开一条新消息。
                    context.callback(move |on: bool| {
                        Message::PsValue(index, if on { "true" } else { "false" }.to_string())
                    }),
                    context.message(Message::PsPickFile(index)),
                ),
            ));
        }

        // ⚠️ `content()` 收尾 ⇒ 其余 builder 在前
        ContentDialog::new()
            .title(self.ps_title.clone())
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| Message::PsClosed(result)))
            .content(
                ScrollViewer::new()
                    .max_height(430.0)
                    .min_width(480.0)
                    .content(StackPanel::new().spacing(0.0).keyed_children(rows)),
            )
    }

    /// 「删除映射」确认框（复刻 `ConfirmAsync`：1109 正文含规则名，确认才删除并保存）。
    pub(super) fn sa_delete_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.sa_delete_confirm {
            return View::empty();
        }
        let id = self
            .sa_selected_id(MATCH_TEXT_TYPE)
            .or_else(|| self.sa_selected_id(MATCH_FILE_EXT))
            .unwrap_or_default();
        let label = self
            .config
            .as_ref()
            .and_then(|config| {
                sa::build_toggles(config, MATCH_TEXT_TYPE)
                    .into_iter()
                    .chain(sa::build_toggles(config, MATCH_FILE_EXT))
                    .find(|toggle| toggle.id == id)
                    .map(|toggle| toggle.label)
            })
            .unwrap_or_else(|| id.clone());

        ContentDialog::new()
            .title(i18n::t("967"))
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| {
                if result == ContentDialogResult::Primary {
                    Message::SaDeleteConfirmed
                } else {
                    Message::SaDeleteCancelled
                }
            }))
            .content(
                TextBlock::new()
                    .text(i18n::t_fmt("1109", &[&label]))
                    .text_wrapping(TextWrapping::Wrap)
                    .min_width(360.0),
            )
    }

    /// 「添加映射」弹窗（复刻 `AddMappingVm` + `BehaviorPickVm`）：
    /// 类型下拉（文件分组 → 内置特征 → 自定义）+ 条件值回显 + 行为勾选（勾选序 = 菜单序）。
    pub(super) fn sa_add_dialog(&self, context: &mut ViewContext<Self>) -> View {
        let Some(draft) = self.sa_add.as_ref() else {
            return View::empty();
        };
        let Some(config) = self.config.as_ref() else {
            return View::empty();
        };

        let options = sa::add_type_options(config);
        let labels: Vec<String> = options.iter().map(|option| option.label.clone()).collect();

        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some(error) = &draft.error {
            rows.push((rows.len(), plugins_view::action_error(error)));
        }

        // 类型下拉
        let type_combo: View = ComboBox::new()
            .min_width(320.0)
            .placeholder_text(i18n::t("1032"))
            .items_source(labels)
            .selected_index(draft.type_pick)
            .on_selection_changed(context.callback(|pick: Option<usize>| Message::SaAddType(pick)))
            .into();
        rows.push((rows.len(), type_combo));

        // 选中类型 → 条件值回显 + 行为勾选列表
        if let Some(pick) = draft.type_pick
            && let Some(option) = options.get(pick)
        {
            let (match_type, match_value) = sa::add_target(config, &option.id);
            let condition_text = if match_type == sa::MATCH_TEXT_TYPE {
                option.label.clone()
            } else {
                match_value.clone()
            };
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(format!("{}: {}", i18n::t("1005"), condition_text))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            ));

            let covering = sa::covering(&self.catalog, &match_type, &match_value);
            for (index, pack) in covering.iter().enumerate() {
                let checked = draft.checked.contains(&pack.id);
                let exhausted = draft.checked.len() >= 9 && !checked;
                rows.push((
                    rows.len(),
                    CheckBox::new()
                        .is_checked(checked)
                        .is_enabled(!exhausted)
                        .on_is_checked_changed(
                            context.callback(move |value: bool| Message::SaAddToggle(index, value)),
                        )
                        .content(TextBlock::new().text(self.catalog.label_for(&pack.id))),
                ));
            }
            if covering.is_empty() {
                rows.push((
                    rows.len(),
                    TextBlock::new()
                        .text(i18n::t("2517"))
                        .font_size(theme::FONT_CAPTION)
                        .foreground(theme::stone_gray())
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                ));
            }
        }

        ContentDialog::new()
            .title(i18n::t("1105"))
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| {
                if result == ContentDialogResult::Primary {
                    Message::SaAddConfirm
                } else {
                    Message::SaAddCancel
                }
            }))
            .content(
                ScrollViewer::new()
                    .max_height(430.0)
                    .min_width(480.0)
                    .content(StackPanel::new().spacing(8.0).keyed_children(rows)),
            )
    }

    /// 「管理匹配类型」对话框（复刻 `MatchTypesDialogWindow` + `MatchTypesDialogViewModel`）：
    /// 类型列表 + 内联表单（名称/英文名/kind 胶囊/规则行/后缀串）+「试一下」+ 双保存路径。
    /// `ContentDialog`：primary = 仅保存类型（2565），secondary = 保存并创建专属行为（2529）。
    pub(super) fn sa_match_types_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.mt_dialog {
            return View::empty();
        }
        let Some(config) = self.config.as_ref() else {
            return View::empty();
        };
        let Some(draft) = self.mt_draft.as_ref() else {
            return View::empty();
        };

        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some((text, is_error)) = &self.mt_status {
            rows.push((rows.len(), plugins_view::action_error(text)));
            let _ = is_error; // 对话框内统一红字渲染（views 侧才按 is_error 分流）
        }

        // 类型列表（既有自定义类型）+ 新建（405）
        let type_labels: Vec<String> = config
            .match_types
            .iter()
            .map(|mt| {
                if mt.label.is_empty() {
                    mt.id.clone()
                } else {
                    mt.label.clone()
                }
            })
            .collect();
        let pick_index = (draft.index != match_types_edit::NEW_INDEX).then_some(draft.index);
        rows.push((
            rows.len(),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .children((
                    {
                        let combo: View = ComboBox::new()
                            .min_width(260.0)
                            .placeholder_text(i18n::t("2519"))
                            .items_source(type_labels)
                            .selected_index(pick_index)
                            .on_selection_changed(
                                context
                                    .callback(|pick: Option<usize>| Message::MatchTypesPick(pick)),
                            )
                            .into();
                        combo
                    },
                    Button::new()
                        .grid_column(1)
                        .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                        .on_click(context.message(Message::MtNew))
                        .content(TextBlock::new().text(i18n::t("405"))),
                )),
        ));

        // 表单
        rows.push((
            rows.len(),
            settings_view::text_field(
                i18n::t("2523"),
                &draft.label,
                context.callback(|value: String| Message::MtLabel(value)),
            ),
        ));
        rows.push((
            rows.len(),
            settings_view::text_field(
                i18n::t("2567"),
                &draft.label_en,
                context.callback(|value: String| Message::MtLabelEn(value)),
            ),
        ));

        // kind：草稿态可切换（胶囊下拉），编辑态锁定（kind 决定引用语义）
        if draft.index == match_types_edit::NEW_INDEX {
            rows.push((
                rows.len(),
                settings_view::combo_row(
                    i18n::t("2556"),
                    &[i18n::t("2556"), i18n::t("2551")],
                    usize::from(draft.kind == "fileExt"),
                    context.callback(|pick: Option<usize>| Message::MtKind(pick.unwrap_or(0))),
                ),
            ));
        } else {
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(format!(
                        "{}: {}",
                        i18n::t("1011"),
                        if draft.kind == "fileExt" {
                            i18n::t("2551")
                        } else {
                            i18n::t("2556")
                        }
                    ))
                    .font_size(theme::FONT_CAPTION)
                    .foreground(theme::stone_gray())
                    .into(),
            ));
        }

        if draft.kind == "fileExt" {
            rows.push((
                rows.len(),
                settings_view::text_field(
                    i18n::t("2528"),
                    &draft.exts,
                    context.callback(|value: String| Message::MtExts(value)),
                ),
            ));
            rows.push((rows.len(), settings_view::hint_row(i18n::t("2561"))));
        } else {
            // 规则行：算子下拉（2512 equals / 2513 prefix / 2514 suffix / 2515 contains）+ 值 + ✕
            let ops = [
                i18n::t("2512"),
                i18n::t("2513"),
                i18n::t("2514"),
                i18n::t("2515"),
            ];
            for (rule, (op, value)) in draft.rules.iter().enumerate() {
                let op_index = ["equals", "prefix", "suffix", "contains"]
                    .iter()
                    .position(|candidate| candidate == op)
                    .unwrap_or(3);
                rows.push((
                    rows.len(),
                    Grid::new()
                        .columns([GridLength::Pixel(120.0), GridLength::STAR, GridLength::Auto])
                        .children((
                            {
                                let combo: View = ComboBox::new()
                                    .items_source(ops.to_vec())
                                    .selected_index(op_index)
                                    .on_selection_changed(context.callback(
                                        move |pick: Option<usize>| {
                                            Message::MtRuleOp(rule, pick.unwrap_or(3))
                                        },
                                    ))
                                    .into();
                                combo
                            },
                            Border::new()
                                .grid_column(1)
                                .margin(Thickness::new(8.0, 0.0, 8.0, 0.0))
                                .content(
                                    TextBox::new()
                                        .text(value.clone())
                                        .min_width(200.0)
                                        .on_text_changed(context.callback(move |value: String| {
                                            Message::MtRuleValue(rule, value)
                                        })),
                                ),
                            Border::new().grid_column(2).content(crate::ui::icon_button(
                                "✕",
                                15.0,
                                theme::ERROR_CRIMSON,
                                draft.rules.len() > 1,
                                context.message(Message::MtRuleRemove(rule)),
                            )),
                        )),
                ));
            }
            rows.push((
                rows.len(),
                Button::new()
                    .on_click(context.message(Message::MtRuleAdd))
                    .content(TextBlock::new().text(i18n::t("405"))),
            ));
        }

        // 「试一下」（2560）：示例内容 + 提交 + 结果
        rows.push((
            rows.len(),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .children((
                    TextBox::new()
                        .grid_column(0)
                        .text(self.mt_test.clone())
                        .placeholder_text(i18n::t("2557"))
                        .on_text_changed(context.callback(|value: String| Message::MtTest(value))),
                    Button::new()
                        .grid_column(1)
                        .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                        .on_click(context.message(Message::MtTestRun))
                        .content(TextBlock::new().text(i18n::t("2560"))),
                )),
        ));
        if let Some(result) = &self.mt_test_result {
            let text = match result {
                Ok(preview) if preview.is_empty() => {
                    format!("{}（{}）", i18n::t("2559"), i18n::t("920"))
                }
                Ok(preview) => format!("{}: {preview}", i18n::t("2558")),
                Err(reason) => reason.clone(),
            };
            rows.push((
                rows.len(),
                TextBlock::new()
                    .text(text)
                    .font_size(theme::FONT_CAPTION)
                    .foreground(match result {
                        Ok(_) => theme::solid(theme::MUTED_GREEN),
                        Err(_) => theme::solid(theme::ERROR_CRIMSON),
                    })
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            ));
        }

        ContentDialog::new()
            .title(i18n::t("2519"))
            .primary_button_text(i18n::t("2565"))
            .secondary_button_text(i18n::t("2529"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(
                context.callback(|result: ContentDialogResult| match result {
                    ContentDialogResult::Primary => Message::MtSave(false),
                    ContentDialogResult::Secondary => Message::MtSave(true),
                    _ => Message::MatchTypesClose,
                }),
            )
            .content(
                // 125% DPI 下 ContentDialog 内容区仅 ~496 DIP：min_width 520 会把
                // 右侧（规则行 ✕ / 测试匹配钮）推出弹窗被裁 —— 收窄到 480 并开
                // 横向滚动兜底（2026-10-02 用户截图实锤）。
                ScrollViewer::new()
                    .max_height(460.0)
                    .min_width(480.0)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
                    .content(StackPanel::new().spacing(8.0).keyed_children(rows)),
            )
    }

    /// 「管理行为」对话框（复刻 `BehaviorLibraryWindow` 的主从编辑）：
    /// 目录下拉（内置 ★ 标注）+ 新建 + 表单（ID/名称/描述/前提行/基础动作/模板/工作目录）
    /// + 删除。内置包只读（1103_only）。
    ///
    /// 🔴 「立即生效」按钮已移除（2026-10-02 用户定版）：行为变更经页脚
    /// 「保存配置」重启引擎后生效，不再提供绕过保存链路的即时应用入口。
    pub(super) fn sa_behaviors_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.bh_dialog {
            return View::empty();
        }

        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some((text, is_error)) = &self.bh_status {
            rows.push((rows.len(), plugins_view::action_error(text)));
            let _ = is_error; // 对话框内统一红字渲染（views 侧才按 is_error 分流）
        }

        // 目录下拉 + 新建
        let labels: Vec<String> = self
            .catalog
            .packs()
            .map(|pack| {
                let source = if pack.source.as_deref() == Some("builtin") {
                    i18n::t("1092")
                } else {
                    i18n::t("1093")
                };
                format!("({source}) {}", self.catalog.label_for(&pack.id))
            })
            .collect();
        rows.push((
            rows.len(),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .children((
                    {
                        let combo: View = ComboBox::new()
                            .min_width(280.0)
                            .placeholder_text(i18n::t("1083"))
                            .items_source(labels)
                            .selected_index(self.bh_pick)
                            .on_selection_changed(
                                context.callback(|pick: Option<usize>| Message::BhPick(pick)),
                            )
                            .into();
                        combo
                    },
                    Button::new()
                        .grid_column(1)
                        .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                        .on_click(context.message(Message::BhNew))
                        .content(TextBlock::new().text(i18n::t("405"))),
                )),
        ));

        // 表单（草稿在位时渲染；内置包可看不可存）
        if let Some(draft) = self.bh_draft.as_ref() {
            let is_new = draft.index == behaviors_edit::NEW_INDEX;
            let is_builtin = !is_new && self.catalog.builtin.iter().any(|pack| pack.id == draft.id);
            if is_builtin {
                rows.push((rows.len(), settings_view::hint_row(i18n::t("1103_only"))));
            }

            rows.push((
                rows.len(),
                settings_view::text_field(
                    "ID",
                    &draft.id,
                    context.callback(|value: String| Message::BhId(value)),
                ),
            ));
            rows.push((
                rows.len(),
                settings_view::text_field(
                    i18n::t("2568"),
                    &draft.name,
                    context.callback(|value: String| Message::BhName(value)),
                ),
            ));
            rows.push((
                rows.len(),
                settings_view::text_field(
                    i18n::t("2523"),
                    &draft.description,
                    context.callback(|value: String| Message::BhDescription(value)),
                ),
            ));

            // 前提行：类型（1032 文本特征 / 1031 文件后缀）+ 值 + ✕；底部追加
            for (row, applies) in draft.applies.iter().enumerate() {
                let kind_labels = [i18n::t("1032"), i18n::t("1031")];
                rows.push((
                    rows.len(),
                    Grid::new()
                        .columns([GridLength::Pixel(140.0), GridLength::STAR, GridLength::Auto])
                        .children((
                            {
                                let combo: View = ComboBox::new()
                                    .items_source(kind_labels.to_vec())
                                    .selected_index(usize::from(applies.kind == "fileExt"))
                                    .on_selection_changed(context.callback(
                                        move |pick: Option<usize>| {
                                            Message::BhAppliesKind(row, pick.unwrap_or(0))
                                        },
                                    ))
                                    .into();
                                combo
                            },
                            Border::new()
                                .grid_column(1)
                                .margin(Thickness::new(8.0, 0.0, 8.0, 0.0))
                                .content(
                                    TextBox::new().text(applies.value.clone()).on_text_changed(
                                        context.callback(move |value: String| {
                                            Message::BhAppliesValue(row, value)
                                        }),
                                    ),
                                ),
                            Border::new().grid_column(2).content(crate::ui::icon_button(
                                "✕",
                                15.0,
                                theme::ERROR_CRIMSON,
                                draft.applies.len() > 1,
                                context.message(Message::BhAppliesRemove(row)),
                            )),
                        )),
                ));
            }
            rows.push((
                rows.len(),
                Button::new()
                    .on_click(context.message(Message::BhAppliesAdd))
                    .content(TextBlock::new().text(i18n::t("405"))),
            ));

            // 基础动作 + 模板 + 工作目录
            let base_options = behaviors_edit::base_action_options(&self.catalog);
            let base_index = base_options
                .iter()
                .position(|action| *action == draft.base_action);
            rows.push((
                rows.len(),
                settings_view::combo_row(
                    i18n::t("1011"),
                    &base_options,
                    base_index.unwrap_or(0),
                    context
                        .callback(|pick: Option<usize>| Message::BhBaseAction(pick.unwrap_or(0))),
                ),
            ));
            rows.push((
                rows.len(),
                settings_view::text_field(
                    i18n::t("2532"),
                    &draft.template,
                    context.callback(|value: String| Message::BhTemplate(value)),
                ),
            ));
            rows.push((
                rows.len(),
                settings_view::text_field(
                    i18n::t("2533"),
                    &draft.working_dir,
                    context.callback(|value: String| Message::BhWorkingDir(value)),
                ),
            ));
            if !is_new {
                rows.push((
                    rows.len(),
                    Button::new()
                        .on_click(context.message(Message::BhDelete))
                        .content(
                            TextBlock::new()
                                .text(i18n::t("967"))
                                .foreground(theme::solid(theme::ERROR_CRIMSON)),
                        ),
                ));
            }
        }

        ContentDialog::new()
            .title(i18n::t("1083"))
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| {
                if result == ContentDialogResult::Primary {
                    Message::BhSave
                } else {
                    Message::BehaviorsClose
                }
            }))
            .content(
                ScrollViewer::new()
                    .max_height(460.0)
                    .min_width(520.0)
                    .content(StackPanel::new().spacing(8.0).keyed_children(rows)),
            )
    }

    /// 指南页底部编辑入口（复刻 `EditZoneHint` 虚线编辑区：点击打开总览编辑窗）。
    pub(super) fn guide_edit_entry(context: &mut ViewContext<Self>) -> View {
        Button::new()
            .margin(Thickness::new(0.0, 10.0, 0.0, 0.0))
            .on_click(context.message(Message::GuideEditOpen))
            .content(TextBlock::new().text(i18n::t("2407")))
    }

    /// 「编辑使用指南」对话框（复刻 `OverviewEditWindow`：2406 标题 / 2405 提示 /
    /// 2404 恢复默认 / 保存 = overviewDocMd 落盘）。
    pub(super) fn guide_edit_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.guide_edit_open {
            return View::empty();
        }
        let body: View = StackPanel::new().spacing(10.0).children((
            settings_view::hint_row(i18n::t("2405")),
            TextBox::new()
                .text(self.guide_edit_text.clone())
                .accepts_return(true)
                .min_height(320.0)
                .min_width(560.0)
                .on_text_changed(context.callback(|value: String| Message::GuideEditValue(value))),
            Button::new()
                .on_click(context.message(Message::GuideEditReset))
                .content(TextBlock::new().text(i18n::t("2404"))),
        ));
        ContentDialog::new()
            .title(i18n::t("2406"))
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| {
                if result == ContentDialogResult::Primary {
                    Message::GuideEditSave
                } else {
                    Message::GuideEditClose
                }
            }))
            .content(ScrollViewer::new().max_height(480.0).content(body))
    }

    /// 自定义热键动作编辑对话框（复刻 `ActionEditorWindow`：承载动作编辑面板，
    /// 经 `hotkey_editor_row` 把 `current_keymap_id` 覆盖为 keymap 1）。
    pub(super) fn custom_hotkey_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if self.hotkey_editor_row.is_none() {
            return View::empty();
        }
        ContentDialog::new()
            .title(i18n::t("1117"))
            .primary_button_text(i18n::t("610"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|result: ContentDialogResult| {
                let _ = result;
                Message::CustomHotkeyEditClose
            }))
            .content(
                ScrollViewer::new()
                    .max_height(480.0)
                    .min_width(560.0)
                    .content(self.action_editor_panel(context)),
            )
    }

    /// 插件市场对话框（`ContentDialog`；列表可滚动）。
    ///
    /// 与旧 `PluginMarketWindow` 的差异：旧版是**独立窗口**，新版用 `ContentDialog`
    /// （Fluent 一致的模态交互，且省去第二窗口的生命周期管理）。
    pub(super) fn market_dialog(&self, context: &mut ViewContext<Self>) -> View {
        if !self.market_open {
            return View::empty();
        }

        let english = matches!(i18n::language(), i18n::Lang::En);
        let mut rows: Vec<(usize, View)> = Vec::new();
        if let Some(status) = &self.market_status {
            rows.push((rows.len(), plugins_view::status_banner(status)));
        }

        if self.market_loading {
            rows.push((rows.len(), plugins_view::loading()));
        } else if let Some(error) = &self.market_error {
            rows.push((
                rows.len(),
                plugins_view::load_error(
                    error,
                    Some(&i18n::t("2433")),
                    context.message(Message::MarketReload),
                ),
            ));
        } else if self.market_entries.is_empty() {
            // 市场空态 = 2438 单行（此前误用插件页的 2429+2430 导入引导文案）
            rows.push((rows.len(), plugins_view::market_empty()));
        }

        for entry in &self.market_entries {
            let installing = self.market_installing.as_deref() == Some(entry.id.as_str());
            rows.push((
                rows.len(),
                plugins_view::market_entry(
                    entry,
                    english,
                    installing,
                    context.message(Message::MarketInstall {
                        id: entry.id.clone(),
                        url: entry.url.clone(),
                    }),
                ),
            ));
        }

        ContentDialog::new()
            .title(i18n::t("2428"))
            .close_button_text(i18n::t("611"))
            .is_open(true)
            .on_closed(context.callback(|_result: ContentDialogResult| Message::MarketClosed))
            .content(
                ScrollViewer::new()
                    .max_height(460.0)
                    .min_width(520.0)
                    .content(StackPanel::new().spacing(0.0).keyed_children(rows)),
            )
    }
}
