//! `app` 的 state：状态访问器与副作用动作（保存/节流/目录重载/选项应用等）。
//!
//! 自原 `app.rs` 的 `impl Shell` 拆分；纯代码搬移，行为不变。

use std::time::Duration;

use super::*;

impl Shell {
    /// 当前卡「添加行为」下拉的有效选中（越界折叠为 `None`）。
    pub(super) fn sa_selected(&self, match_type: &str, covering_len: usize) -> Option<usize> {
        let stored = match match_type {
            MATCH_TEXT_TYPE => self.sa_text_selected,
            _ => self.sa_file_selected,
        };
        stored.filter(|selected| *selected < covering_len)
    }

    /// 覆盖行为的显示名列表（下拉源）。
    pub(super) fn sa_covering_labels(&self, match_type: &str, match_value: &str) -> Vec<String> {
        sa::covering(&self.catalog, match_type, match_value)
            .iter()
            .map(|pack| self.catalog.label_for(&pack.id))
            .collect()
    }

    /// 某卡当前点亮的 toggle id。
    pub(super) fn sa_selected_id(&self, match_type: &str) -> Option<String> {
        match match_type {
            MATCH_TEXT_TYPE => self.sa_text_sel.clone(),
            _ => self.sa_file_sel.clone(),
        }
    }

    /// 删除后清空该卡选中与下拉选择（视图层下次渲染按恢复规则重选）。
    pub(super) fn reset_sa_selection(&mut self, match_type: &str) {
        match match_type {
            MATCH_TEXT_TYPE => {
                self.sa_text_sel = None;
                self.sa_text_selected = None;
            }
            _ => {
                self.sa_file_sel = None;
                self.sa_file_selected = None;
            }
        }
    }

    /// 配置落盘的**唯一入口**（保存策略，2026-10-02 用户定版）：
    /// 一切编辑只改内存（config / 行为目录 / 草稿）并入队
    /// [`save_pipeline::PendingChange`]，只有页脚「保存配置」按钮 / Ctrl+S（同走
    /// `Message::Save`）才经 [`save_pipeline::flush_and_save`] 按序提交全部暂存变更。
    /// ⚠️ 新增交互**禁止**在编辑处理器里调用本方法做自动保存
    /// （1 秒节流防连点，复刻 `SaveCommand` 的 useThrottleFn）。
    pub(super) fn save_now(&mut self, context: &ComponentContext<Self>) {
        let (Some(config), Some(port)) = (self.config.clone(), self.port) else {
            return;
        };
        if let Some(last) = self.last_save
            && last.elapsed() < Duration::from_secs(1)
        {
            return;
        }
        self.last_save = Some(Instant::now());

        // 选项页皮肤字段校验（颜色 #RRGGBB / 数值 >= 0；错误文案用标签而非 JSON 键）
        for field in settings::SKIN_FIELDS {
            let value = settings::skin_get(&config.options.command_input_skin, field.key)
                .unwrap_or_default();
            if let Some(reason) = settings::validate_skin_field(&field, value) {
                self.settings_notice = Some(format!(
                    "{} ({}): {}",
                    i18n::t("741"),
                    i18n::t(field.label_key),
                    reason
                ));
                return;
            }
        }

        self.notice = None;
        self.notice_error = false;
        let queue = self.pending.take();
        let data_root = self.data_root.clone();
        let _ = context.spawn_background(move |_token| {
            Message::SaveFinished(save_pipeline::flush_and_save(
                port,
                data_root.as_deref(),
                queue,
                &config,
            ))
        });
    }

    /// 匹配类型草稿整体替换（选中/新建切换时清状态）。
    pub(super) fn mt_pick_set(&mut self, draft: Option<match_types_edit::MatchTypeDraft>) {
        self.mt_draft = draft;
        self.mt_status = None;
        self.mt_test_result = None;
        self.mt_test.clear();
    }

    /// 就地编辑匹配类型草稿。
    pub(super) fn mt_edit_draft(
        &mut self,
        apply: impl FnOnce(&mut match_types_edit::MatchTypeDraft),
    ) {
        if let Some(draft) = self.mt_draft.as_mut() {
            apply(draft);
        }
    }

    /// 就地编辑行为草稿。
    pub(super) fn bh_edit_draft(&mut self, apply: impl FnOnce(&mut behaviors_edit::BehaviorDraft)) {
        if let Some(draft) = self.bh_draft.as_mut() {
            apply(draft);
        }
    }

    /// 重拉行为目录（保存/删除/创建专属行为后）。
    pub(super) fn reload_catalog(&mut self, context: &ComponentContext<Self>) {
        let Some(port) = self.port else {
            return;
        };
        let _ = context.spawn_background(move |_token| {
            let api = crate::services::transport::new_settings_api(port);
            let response = api.get_behaviors();
            match response.value {
                Some(value) => Message::BehaviorsLoaded(Ok(Box::new(sa::SaCatalog {
                    builtin: value.builtin,
                    user: value.user,
                }))),
                None => Message::BehaviorsLoaded(Err(response
                    .error_message
                    .unwrap_or_else(|| format!("HTTP {}", response.status)))),
            }
        });
    }

    /// 「添加行为」：把下拉选中的行为追加到当前类型的映射（未配置类型同时创建映射）。
    pub(super) fn add_behavior(&mut self, match_type: &'static str) {
        let Some(id) = self.sa_selected_id(match_type) else {
            return;
        };
        if id.is_empty() {
            return;
        }
        let selected = match match_type {
            MATCH_TEXT_TYPE => self.sa_text_selected,
            _ => self.sa_file_selected,
        };
        // 1) matchValue（不可变借用阶段）
        let match_value = match self.config.as_ref() {
            Some(config) => sa::find_mapping_for_type(config, match_type, &id)
                .map(|mapping| mapping.match_value.clone())
                .unwrap_or_else(|| sa::transient_match_value(config, match_type, &id)),
            None => return,
        };

        // 2) 目录推导（catalog 不可变借用）；未显式选择时自动取**首个未用**覆盖行为
        //    （复刻 `AddEntry` 的 CanAddEntry 自动挑选）
        let covering = sa::covering(&self.catalog, match_type, &match_value);
        let selected = match selected {
            Some(selected) => Some(selected),
            None => self
                .config
                .as_ref()
                .and_then(|config| {
                    sa::find_mapping_for_type(config, match_type, &id).map(|mapping| {
                        covering.iter().position(|pack| {
                            !mapping
                                .entries
                                .iter()
                                .any(|entry| entry.behavior == pack.id)
                        })
                    })
                })
                .unwrap_or(None),
        };
        let Some(selected) = selected else {
            return;
        };
        let Some(pack) = covering.get(selected) else {
            return;
        };
        let behavior = pack.id.clone();
        let action_value = if self.catalog.is_no_value(&behavior) {
            String::new()
        } else {
            self.catalog.default_template_for(&behavior)
        };

        // 3) 写入（可变借用；transient → 真实 mapping 转正）
        let Some(config) = self.config.as_mut() else {
            return;
        };
        if let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id) {
            if mapping.entries.len() >= 9 {
                return; // 每条规则最多 9 个行为（旧版 1107 语义）
            }
            if mapping
                .entries
                .iter()
                .any(|entry| entry.behavior == behavior)
            {
                return; // 已添加过（旧版 1119 语义）
            }
            mapping.entries.push(SelectedEntry {
                behavior,
                action_value,
                ..Default::default()
            });
        } else {
            config
                .selected_action
                .mappings
                .push(crate::models::SelectedMapping {
                    match_type: match_type.to_string(),
                    match_value,
                    entries: vec![SelectedEntry {
                        behavior,
                        action_value,
                        ..Default::default()
                    }],
                });
        }
    }

    // ---------------------------------------------------------- 动作编辑（键位图系共享）

    /// 当前页面对应的 keymap id（仅键位图系页面有）。
    ///
    /// 自定义热键动作编辑对话框打开时**覆盖**为 id=1（复刻 `ActionEditorWindow`
    /// 把动作编辑面板指向 keymap 1 的宿主逻辑）。
    pub(super) fn current_keymap_id(&self) -> Option<i32> {
        if self.hotkey_editor_row.is_some() {
            return Some(1);
        }
        match self.nav.get(self.page_index)?.kind {
            PageKind::Keymap(id) | PageKind::Abbr(id) => Some(id),
            _ => None,
        }
    }

    /// 当前页面对应的 keymap。
    pub(super) fn current_keymap(&self) -> Option<&Keymap> {
        let id = self.current_keymap_id()?;
        self.config
            .as_ref()?
            .keymaps
            .iter()
            .find(|keymap| keymap.id == id)
    }

    /// 当前编辑的动作（只读）。
    pub(super) fn current_action(&self) -> Option<&Action> {
        let keymap = self.current_keymap()?;
        keymap::find_action(
            keymap,
            self.selected_hotkey.as_deref()?,
            self.window_group_id,
        )
    }

    /// 当前编辑的动作（可变；不存在则惰性初始化，复刻 `_getAction`）。
    pub(super) fn current_action_mut(&mut self) -> Option<&mut Action> {
        let keymap_id = self.current_keymap_id()?;
        let hotkey = self.selected_hotkey.clone()?;
        let group = self.window_group_id;
        let config = self.config.as_mut()?;
        keymap::ensure_action(config, keymap_id, &hotkey, group)
    }

    /// 对当前 keymap 做可变访问（无配置 / 无该 keymap 时静默跳过）。
    pub(super) fn with_current_keymap<F: FnOnce(&mut Keymap)>(&mut self, apply: F) {
        let Some(id) = self.current_keymap_id() else {
            return;
        };
        let Some(config) = self.config.as_mut() else {
            return;
        };
        if let Some(keymap) = config.keymaps.iter_mut().find(|km| km.id == id) {
            apply(keymap);
        }
    }

    // ---------------------------------------------------------- 插件页

    /// 后台拉取市场目录 + 本地已装集合（复刻 `PluginMarketViewModel.LoadAsync`）。
    pub(super) fn reload_market(&mut self, context: &ComponentContext<Self>) {
        let Some(port) = self.port else {
            return;
        };
        self.market_loading = true;
        self.market_error = None;
        let _ = context.spawn_background(move |_token| {
            // 已装集合取本地后端目录；目录本体走外部网络（后端不出网）
            let installed: Vec<String> = crate::services::transport::new_settings_api(port)
                .get_plugins()
                .value
                .map(|catalog| {
                    catalog
                        .plugins
                        .into_iter()
                        .map(|plugin| plugin.id)
                        .collect()
                })
                .unwrap_or_default();

            match market::fetch_catalog() {
                Ok(catalog) => {
                    Message::MarketLoaded(Ok(market::build_entries(&catalog, &installed)))
                }
                Err(reason) => Message::MarketLoaded(Err(reason)),
            }
        });
    }

    /// 选项页字段编辑（`Message::Opt` 的落地）。
    ///
    /// 下标语义见 [`OptEdit`] 文档；全部**直接写入内存 config**，随页脚保存链路持久化
    /// （与键位图页/缩写页的编辑模式一致）。
    pub(super) fn apply_opt(&mut self, edit: OptEdit) {
        let Some(config) = self.config.as_mut() else {
            return;
        };
        match edit {
            OptEdit::SchemeName(index, value) => {
                if let Some(keymap) = config.keymaps.iter_mut().filter(|km| km.id > 4).nth(index) {
                    keymap.name = value;
                }
            }
            OptEdit::SchemeHotkey(index, value) => {
                if let Some(keymap) = config.keymaps.iter_mut().filter(|km| km.id > 4).nth(index) {
                    keymap.hotkey = value;
                }
            }
            OptEdit::SchemeEnable(index, value) => {
                let changed = config
                    .keymaps
                    .iter_mut()
                    .filter(|km| km.id > 4)
                    .nth(index)
                    .map(|keymap| {
                        let changed = keymap.enable != value;
                        keymap.enable = value;
                        changed
                    })
                    .unwrap_or(false);
                // 启停改变导航构成 ⇒ 重建导航（与保存后 BuildNav 同语义）
                if changed {
                    let next = config.clone();
                    self.nav = build_nav(&next);
                }
            }
            OptEdit::SchemeAdd => {
                let mut next_id = 5;
                while config.keymaps.iter().any(|km| km.id == next_id) {
                    next_id += 1;
                }
                config.keymaps.push(Keymap {
                    id: next_id,
                    // 旧版新建 = 空名（IsNew），显示时回退触发键
                    name: String::new(),
                    enable: false,
                    ..Default::default()
                });
                self.rebuild_nav();
            }
            OptEdit::SchemeDelete(index) => {
                // 删除自定义方案（id>4 渲染序下标）：导航构成变化 ⇒ 重建导航。
                // 方案对应触发键的绑定随 keymap 整体移除（保存链路持久化）。
                // 已启用方案不可删（用户定版；按钮已灰化，此处兜底防绕过）。
                let deletable = config
                    .keymaps
                    .iter()
                    .filter(|km| km.id > 4)
                    .nth(index)
                    .map(|km| !km.enable)
                    .unwrap_or(false);
                let removed = deletable
                    .then(|| {
                        config
                            .keymaps
                            .iter()
                            .filter(|km| km.id > 4)
                            .nth(index)
                            .map(|km| km.id)
                    })
                    .flatten();
                if let Some(id) = removed {
                    config.keymaps.retain(|km| km.id != id);
                    let next = config.clone();
                    self.nav = build_nav(&next);
                }
            }
            OptEdit::SchemeDelay(index, value) => {
                if let (Some(keymap), Ok(delay)) = (
                    config.keymaps.iter_mut().filter(|km| km.id > 4).nth(index),
                    value.trim().parse::<i32>(),
                ) {
                    keymap.delay = delay;
                }
            }
            OptEdit::HideMatrix(value) => config.options.hide_matrix = value,
            OptEdit::Language(index) => {
                let value = ["zh", "en"][index.min(1)];
                config.options.language = value.to_string();
                i18n::apply_config_language(value);
                self.rebuild_nav();
            }
            OptEdit::CustomHotkey(index, value) => {
                if let Some(keymap) = config.keymaps.iter_mut().find(|km| km.id == 1) {
                    let old = keymap.hotkeys.keys().nth(index).cloned();
                    if let Some(old) = old {
                        keymap::change_hotkey(keymap, &old, &value);
                    }
                }
            }
            OptEdit::CustomHotkeyAdd => {
                // 占位热键：动作为空 ⇒ `clean_for_save` 在保存时整体丢弃，不会生成无效 AHK
                let mut next = 1;
                let placeholder = loop {
                    let candidate = format!("ctrl+alt+shift+f{next}");
                    let taken = config
                        .keymaps
                        .iter()
                        .find(|km| km.id == 1)
                        .map(|km| km.hotkeys.contains_key(&candidate))
                        .unwrap_or(true);
                    if !taken {
                        break candidate;
                    }
                    next += 1;
                };
                let _ = keymap::ensure_action(config, 1, &placeholder, -1);
            }
            OptEdit::CustomHotkeyRemove(row) => {
                if let Some(keymap) = config.keymaps.iter_mut().find(|km| km.id == 1)
                    && let Some(old) = keymap.hotkeys.keys().nth(row).cloned()
                {
                    keymap::remove_hotkey(keymap, &old);
                }
            }
            OptEdit::MouseDelay1(value) => config.options.mouse.delay1 = value,
            OptEdit::MouseDelay2(value) => config.options.mouse.delay2 = value,
            OptEdit::MouseFastSingle(value) => config.options.mouse.fast_single = value,
            OptEdit::MouseFastRepeat(value) => config.options.mouse.fast_repeat = value,
            OptEdit::MouseSlowSingle(value) => config.options.mouse.slow_single = value,
            OptEdit::MouseSlowRepeat(value) => config.options.mouse.slow_repeat = value,
            OptEdit::MouseTipSymbol(value) => config.options.mouse.tip_symbol = value,
            OptEdit::MouseKeepMode(value) => config.options.mouse.keep_mouse_mode = value,
            OptEdit::MouseShowTip(value) => config.options.mouse.show_tip = value,
            OptEdit::ScrollDelay1(value) => config.options.scroll.delay1 = value,
            OptEdit::ScrollDelay2(value) => config.options.scroll.delay2 = value,
            OptEdit::ScrollOnceLine(value) => config.options.scroll.once_line_count = value,
            OptEdit::LayoutPreset(kind) => {
                let current = config.options.keyboard_layout.clone();
                if let Some(layout) = settings::keyboard_layout_preset(kind, &current) {
                    config.options.keyboard_layout = layout;
                }
            }
            OptEdit::KeyboardLayoutSet(value) => config.options.keyboard_layout = value,
            OptEdit::Skin(index, value) => {
                if let Some(field) = settings::SKIN_FIELDS.get(index) {
                    settings::skin_set(&mut config.options.command_input_skin, field.key, &value);
                }
            }
            OptEdit::FontSource(value) => config.options.command_font.source_path = value,
            OptEdit::FontWeight(index) => {
                if let Some(weight) = settings::FONT_WEIGHTS.get(index) {
                    config.options.command_font.weight = (*weight).to_string();
                }
            }
            OptEdit::FontReset => settings::font_reset(config),
            OptEdit::PathVarName(index, value) => {
                if let Some(row) = config.options.path_variables.get_mut(index) {
                    row.name = value;
                }
            }
            OptEdit::PathVarValue(index, value) => {
                if let Some(row) = config.options.path_variables.get_mut(index) {
                    row.value = value;
                }
            }
            OptEdit::PathVarAdd => {
                settings::add_path_variable(config);
            }
            OptEdit::PathVarRemove(index) => {
                settings::remove_path_variable(config, index);
            }
            OptEdit::GroupName(row, value) => {
                if let Some(group) = config
                    .options
                    .window_groups
                    .iter_mut()
                    .filter(|group| group.id > 0)
                    .nth(row)
                {
                    group.name = value;
                }
            }
            OptEdit::GroupValue(row, value) => {
                if let Some(group) = config
                    .options
                    .window_groups
                    .iter_mut()
                    .filter(|group| group.id > 0)
                    .nth(row)
                {
                    group.value = value;
                }
            }
            OptEdit::GroupCondition(row, index) => {
                if let Some(group) = config
                    .options
                    .window_groups
                    .iter_mut()
                    .filter(|group| group.id > 0)
                    .nth(row)
                {
                    group.condition_type = index as i32 + 1;
                }
            }
            OptEdit::GroupAdd => {
                let mut next_id = 1;
                while config
                    .options
                    .window_groups
                    .iter()
                    .any(|group| group.id == next_id)
                {
                    next_id += 1;
                }
                config
                    .options
                    .window_groups
                    .push(crate::models::WindowGroup {
                        id: next_id,
                        name: String::new(),
                        ..Default::default()
                    });
            }
            OptEdit::GroupRemove(row) => {
                let keep: Vec<usize> = config
                    .options
                    .window_groups
                    .iter()
                    .enumerate()
                    .filter(|(_, group)| group.id > 0)
                    .map(|(index, _)| index)
                    .collect();
                if let Some(&index) = keep.get(row) {
                    config.options.window_groups.remove(index);
                }
            }
        }
    }

    // ---------------------------------------------------------- 选项页

    /// 后台拉取插件目录（清除一次性回显；复刻 `PluginsPageViewModel.ReloadAsync`）。
    pub(super) fn reload_plugins(&mut self, context: &ComponentContext<Self>) {
        self.plugin_status = None;
        self.plugins_action_error = None;
        self.refresh_plugins(context);
    }

    /// 后台拉取插件目录（**保留**一次性回显——导入成功横幅不被刷新吞掉）。
    pub(super) fn refresh_plugins(&mut self, context: &ComponentContext<Self>) {
        let Some(port) = self.port else {
            return;
        };
        self.plugins_loading = true;
        self.plugins_error = None;
        let _ = context.spawn_background(move |_token| {
            let api = crate::services::transport::new_settings_api(port);
            let response = api.get_plugins();
            match response.value {
                Some(catalog) => Message::PluginsLoaded(Ok(Box::new(catalog))),
                None => Message::PluginsLoaded(Err(response
                    .error_message
                    .unwrap_or_else(|| format!("HTTP {}", response.status)))),
            }
        });
    }

    /// 成功提示 2 秒后自动清除（后台线程 sleep，对齐旧版 `Task.Delay(2000)`）。
    /// `ClearNotice` 分支对错误态提示无操作，故组件关闭后误派发也无副作用。
    pub(super) fn schedule_notice_clear(&self, context: &ComponentContext<Self>) {
        let _ = context.spawn_background(move |_token| {
            std::thread::sleep(Duration::from_secs(2));
            Message::ClearNotice
        });
    }

    /// 出厂默认使用指南文档：经 glue 的单一真源（本机直读 → 回退后端）加载。
    /// 「恢复默认 / 保存空文本」的回落出口 —— 不得用「清空渲染态」代替（那会让
    /// 指南页落进「暂不可用」降级态，2026-10-05 用户报障）。
    pub(super) fn load_default_doc(&self) -> String {
        let api = self
            .session
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|session| session.api()));
        match api {
            Some(api) => super::glue::load_default_doc(self.data_root.as_deref(), api.as_ref()),
            None => String::new(),
        }
    }

    /// 缩写页命令框执行（复刻 `AbbrPageViewModel.RunCmd`）：
    /// `del <缩写>` 删除 · `rn <新名>` 重命名当前选中 · 其余**添加/切换**到该缩写。
    pub(super) fn run_abbr_command(&mut self) {
        let selected = self.selected_hotkey.clone().unwrap_or_default();
        let input = self.cmd_text.clone();
        let outcome = abbr::parse_command(&input, &selected);

        if outcome.clear_input {
            self.cmd_text.clear();
        }

        match outcome.command {
            abbr::AbbrCommand::None => {}
            abbr::AbbrCommand::Delete { hotkey } => {
                self.with_current_keymap(|keymap| {
                    keymap::remove_hotkey(keymap, &hotkey);
                });
            }
            abbr::AbbrCommand::Rename { from, to } => {
                self.with_current_keymap(|keymap| {
                    keymap::change_hotkey(keymap, &from, &to);
                });
            }
            abbr::AbbrCommand::Select { hotkey } => {
                // 卡片文案「输入 ab 按回车添加/切换到 ab」：目标键**不存在时立即新增**条目
                // （旧版只设选中、条目延后到编辑动作才惰性创建 ⇒ 与文案不符，按文案修正）。
                let group = self.window_group_id;
                if let Some(keymap_id) = self.current_keymap_id()
                    && let Some(config) = self.config.as_mut()
                {
                    let _ = keymap::ensure_action(config, keymap_id, &hotkey, group);
                }
            }
        }

        // `del` 分支把选中置空（C# 的 `SelectedHotkey = ""`）；其余为新的目标键
        self.selected_hotkey = outcome.next_selection.filter(|next| !next.is_empty());
    }

    /// 写入动作字段（复刻各类型编辑器的 setter 与 `isEmpty` 规则）。
    pub(super) fn apply_field(&mut self, field: ActionField) {
        let Some(action) = self.current_action_mut() else {
            return;
        };
        match field {
            ActionField::WinTitle(value) => {
                action.win_title = value;
                action_editor::refresh_empty_activate_or_run(action);
            }
            ActionField::Target(value) => {
                action.target = value;
                action_editor::refresh_empty_activate_or_run(action);
            }
            ActionField::Args(value) => action.args = value,
            ActionField::WorkingDir(value) => action.working_dir = value,
            ActionField::Comment(value) => action.comment = value,
            ActionField::KeysToSend(value) => action_editor::apply_keys_to_send(action, &value),
            ActionField::AhkCode(value) => action_editor::apply_ahk_code(action, &value),
            ActionField::RemapToKey(value) => action_editor::apply_remap(action, &value),
            ActionField::RunAsAdmin(value) => action.run_as_admin = value,
            ActionField::RunInBackground(value) => action.run_in_background = value,
            ActionField::DetectHiddenWindow(value) => action.detect_hidden_window = value,
        }
    }

    /// 复刻 `MaybeRefreshAbbrEnable`：类型 9 与取值 5/6 相互变化时重算缩写/命令 keymap 的启用态。
    pub(super) fn maybe_refresh_abbr_enable(
        &mut self,
        old_type: i32,
        new_type: i32,
        old_value: i32,
        new_value: i32,
    ) {
        let type_involved = old_type == 9 || new_type == 9;
        let value_involved = matches!(old_value, 5 | 6) || matches!(new_value, 5 | 6);
        if !(type_involved && value_involved) {
            return;
        }
        if let Some(config) = self.config.as_mut() {
            store::change_abbr_enable(config);
        }
        self.rebuild_nav();
    }

    /// 重建导航并**保持当前选中项**（启用态变化会增删导航项）。
    pub(super) fn rebuild_nav(&mut self) {
        let Some(config) = self.config.as_ref() else {
            return;
        };
        let nav = build_nav(config);
        let current_tag = self.nav.get(self.page_index).map(|entry| entry.tag.clone());
        self.nav = nav;
        if let Some(tag) = current_tag
            && let Some(index) = self.nav.iter().position(|entry| entry.tag == tag)
        {
            self.page_index = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Shell;

    /// 批 W（2026-10-08）：`Shell::default()`（`cfg_attr(test)` derive，见 app.rs）让
    /// 状态方法脱离窗口直接断言。mt_pick_set 的契约 = 换草稿时**同时清空**对话框内
    /// 状态条与「试一下」结果 —— 换了上下文，旧状态必然失效，残留会串页显示。
    #[test]
    fn mt_pick_set_resets_dialog_state() {
        // clippy `field_reassign_with_default`: 用结构体更新语法而非逐字段赋值。
        let mut shell = Shell {
            mt_status: Some(("旧状态".to_string(), true)),
            mt_test_result: Some(Ok("旧结果".to_string())),
            mt_test: "旧内容".to_string(),
            ..Shell::default()
        };

        shell.mt_pick_set(None);

        assert!(shell.mt_draft.is_none());
        assert!(shell.mt_status.is_none());
        assert!(shell.mt_test_result.is_none());
        assert!(shell.mt_test.is_empty());
    }
}
