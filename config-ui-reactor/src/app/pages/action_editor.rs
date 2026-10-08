//! `app::pages::action_editor` —— 动作编辑器（键位图/缩写页共用的内联面板 + 6 个字段编辑器）。
//!
//! 2026-10-08 自 `app/views.rs` 的 `impl Shell` 逐字搬移（模块化审查 #4）；
//! 可见性 `pub(super)` -> `pub(in crate::app)`，方法体未改。

use super::super::*;

impl Shell {
    /// 动作编辑面板：两级下拉 + 按类型分发的编辑器。
    pub(in crate::app) fn action_editor_panel(&self, context: &mut ViewContext<Self>) -> View {
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
    pub(in crate::app) fn editor_activate_or_run(
        &self,
        action: &Action,
        context: &mut ViewContext<Self>,
    ) -> View {
        let win_title_box = action_editor_view::text_box(
            &action.win_title,
            false,
            context.callback(|value: String| Message::EditField(ActionField::WinTitle(value))),
        );
        // 301 行 = 文本框 + 准星拾取按钮 (旧版 WindowPickButton 的回归, 用户报障
        // 2026-10-05): 会话进行中按钮置灰; 结果经 Message::WindowPicked 写回同一字段。
        let win_title: View = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                Border::new().grid_column(0).content(win_title_box),
                Border::new()
                    .grid_column(1)
                    .margin(Thickness::new(6.0, 0.0, 0.0, 0.0))
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(action_editor_view::pick_button(
                        context.message(Message::PickWindow),
                        !self.picking,
                    )),
            ));
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

        // 目标：纯文本框（2026-10-05 用户定版移除快捷方式下拉 —— 下拉恒空、价值低）
        let target = action_editor_view::text_box(
            &action.target,
            false,
            context.callback(|value: String| Message::EditField(ActionField::Target(value))),
        );

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
        let rows: Vec<(usize, View)> = vec![
            (0, action_editor_view::field(i18n::t("301"), win_title)),
            (1, hint),
            (2, error),
            (3, action_editor_view::field(i18n::t("302"), target)),
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
    pub(in crate::app) fn editor_radio_group(
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
    pub(in crate::app) fn editor_remap(
        &self,
        action: &Action,
        context: &mut ViewContext<Self>,
    ) -> View {
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
    pub(in crate::app) fn editor_send_keys(
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
    pub(in crate::app) fn editor_ahk_code(
        &self,
        action: &Action,
        context: &mut ViewContext<Self>,
    ) -> View {
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
    pub(in crate::app) fn comments_column(
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
}
