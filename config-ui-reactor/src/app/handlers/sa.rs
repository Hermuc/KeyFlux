//! sa —— `Shell::update` 的 SelectedAction 规则编辑 (选中文本动作) 消息处理臂 (自 app.rs 逐字迁移, 2026-10-07)。
//!
//! 每个变体一个 match 臂, 逻辑与迁移前逐字一致 (含原注释); 迁移只改归属, 不改行为。
//! 由 `update` 统一分发: `Message::*` -> 本文件的 `handle_sa`。

use super::super::*;

impl Shell {
    /// `sa` 域消息处理 (由 `update` 转发)。
    pub(in crate::app) fn handle_sa(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::SaHotkey(text) => {
                if let Some(config) = self.config.as_mut() {
                    // UsedHotkeys 冲突校验（复刻 `SelectedActionPageViewModel`）：
                    // 输入与全部 keymap 的热键/触发键比对（不含选中动作自身旧值）
                    let mut occupied: Vec<String> = config
                        .keymaps
                        .iter()
                        .flat_map(|keymap| {
                            keymap
                                .hotkeys
                                .keys()
                                .cloned()
                                .chain(std::iter::once(keymap.hotkey.clone()))
                        })
                        .collect();
                    occupied.retain(|existing| !existing.is_empty());
                    let previous = config.selected_action.hotkey.clone();
                    let conflict = !text.is_empty()
                        && occupied.iter().any(|existing| {
                            existing.eq_ignore_ascii_case(&text)
                                && !existing.eq_ignore_ascii_case(&previous)
                        });
                    self.sa_hotkey_conflict = conflict;
                    config.selected_action.hotkey = text;
                    self.hotkey_pending_save = true;
                }
            }
            Message::SaEnable(enabled) => {
                // 只改内存态（保存策略：唯一落盘入口 = 页脚「保存配置」；旧版
                // `SaveEnableAsync` 的"开关即保存 + 失败回滚"已随自动保存一起退役）
                if let Some(config) = self.config.as_mut() {
                    config.selected_action.enable = enabled;
                }
            }
            Message::SaSelectToggle { match_type, id } => match match_type {
                MATCH_TEXT_TYPE => self.sa_text_sel = Some(id),
                _ => self.sa_file_sel = Some(id),
            },
            Message::SaDeleteAsk => {
                let id = self
                    .sa_selected_id(MATCH_TEXT_TYPE)
                    .or_else(|| self.sa_selected_id(MATCH_FILE_EXT))
                    .unwrap_or_default();
                if id.is_empty() {
                    return;
                }
                // 仅已配置（存在 mapping）的类型可删；打开确认框（1109，确认才应用，保存配置才落盘）
                let has_mapping = self.config.as_ref().is_some_and(|config| {
                    [MATCH_TEXT_TYPE, MATCH_FILE_EXT].iter().any(|match_type| {
                        sa::find_mapping_for_type(config, match_type, &id).is_some()
                    })
                });
                self.sa_delete_confirm = has_mapping;
            }
            Message::SaDeleteCancelled => self.sa_delete_confirm = false,
            Message::SaDeleteConfirmed => {
                self.sa_delete_confirm = false;
                for match_type in [MATCH_TEXT_TYPE, MATCH_FILE_EXT] {
                    let id = self.sa_selected_id(match_type).unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    if let Some(config) = self.config.as_mut()
                        && let Some(position) =
                            sa::find_mapping_index_for_type(config, match_type, &id)
                    {
                        config.selected_action.mappings.remove(position);
                        self.reset_sa_selection(match_type);
                    }
                }
                // 只删内存映射（确认才应用；落盘统一走页脚「保存配置」）
            }
            Message::SaPlaySample => {
                let Some(port) = self.port else {
                    return;
                };
                // typeId = toggle id（内置特征值 / group:<name> / type:<id>，后端白名单同构）
                let Some(type_id) = self
                    .sa_selected_id(MATCH_TEXT_TYPE)
                    .or_else(|| self.sa_selected_id(MATCH_FILE_EXT))
                    .filter(|id| !id.is_empty())
                else {
                    return;
                };
                let _ = context.spawn_background(move |_token| {
                    let api = crate::services::transport::new_settings_api(port);
                    let response = api.play_selected_action(&type_id);
                    Message::SaPlayDone(if response.success {
                        Ok(())
                    } else {
                        Err(response
                            .error_message
                            .unwrap_or_else(|| format!("HTTP {}", response.status)))
                    })
                });
            }
            Message::SaPickBehavior { match_type, pick } => match match_type {
                MATCH_TEXT_TYPE => self.sa_text_pick = pick,
                _ => self.sa_file_pick = pick,
            },
            Message::SaAddBehavior { match_type } => {
                self.add_behavior(match_type);
            }
            Message::SaRemoveEntry { match_type, index } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                    && mapping.entries.len() > 1
                {
                    // 至少保留一个行为（旧版 1108 语义；空 entries 会被后端 400 拒绝）
                    mapping.entries.remove(index);
                }
            }
            Message::SaEntrySwitch {
                match_type,
                index,
                pick,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                // 覆盖推导（catalog 不可变借用）→ 写回（可变借用）
                let replacement = {
                    let match_value = self.config.as_ref().and_then(|config| {
                        sa::find_mapping_for_type(config, match_type, &id).map(|mapping| {
                            (
                                mapping.match_value.clone(),
                                mapping.entries.get(index).cloned(),
                            )
                        })
                    });
                    match (self.config.as_ref(), match_value) {
                        (_, Some((match_value, Some(current)))) => {
                            let covering = sa::covering(&self.catalog, match_type, &match_value);
                            covering.get(pick).map(|pack| {
                                let behavior = pack.id.clone();
                                SelectedEntry {
                                    action_value: if self.catalog.is_no_value(&behavior) {
                                        String::new()
                                    } else {
                                        // 切换行为 ⇒ 重置为该行为默认模板（复刻 OnBehaviorChanged）
                                        self.catalog.default_template_for(&behavior)
                                    },
                                    ..current
                                }
                            })
                        }
                        _ => None,
                    }
                };
                if let Some(replacement) = replacement
                    && let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                {
                    mapping.entries[index] = replacement;
                }
            }
            Message::SaEntryMove {
                match_type,
                index,
                delta,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                {
                    let target = index as i64 + i64::from(delta);
                    if target >= 0 && (target as usize) < mapping.entries.len() {
                        mapping.entries.swap(index, target as usize);
                    }
                }
            }
            Message::SaEntryValue {
                match_type,
                index,
                value,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                    && let Some(entry) = mapping.entries.get_mut(index)
                {
                    entry.action_value = value;
                }
            }
            Message::SaEntryWorkingDir {
                match_type,
                index,
                value,
            } => {
                let id = self.sa_selected_id(match_type).unwrap_or_default();
                if let Some(config) = self.config.as_mut()
                    && let Some(mapping) = sa::find_mapping_for_type_mut(config, match_type, &id)
                    && let Some(entry) = mapping.entries.get_mut(index)
                {
                    entry.working_dir = value;
                }
            }
            Message::SaPlayDone(result) => {
                self.sa_status = match result {
                    // 成功：引擎侧可见执行，不打扰（旧版 StatusText 仅承载失败）
                    Ok(()) => None,
                    Err(reason) => Some((reason, true)),
                };
            }
            Message::SaAddOpen => {
                self.sa_add = Some(SaAddDraft::default());
            }
            Message::SaHotkeyClear => {
                self.sa_hotkey_conflict = false;
                if let Some(config) = self.config.as_mut() {
                    config.selected_action.hotkey = String::new();
                }
                self.hotkey_pending_save = true;
            }
            Message::SaNewType { kind } => {
                self.mt_dialog = true;
                self.mt_status = None;
                self.mt_test_result = None;
                self.mt_test.clear();
                self.mt_draft = self
                    .config
                    .as_ref()
                    .map(|config| match_types_edit::MatchTypeDraft::new_draft(config, kind));
            }
            Message::SaAddCancel => self.sa_add = None,
            Message::SaAddType(pick) => {
                if let Some(draft) = self.sa_add.as_mut() {
                    draft.type_pick = pick;
                    draft.checked.clear();
                    draft.error = None;
                }
            }
            Message::SaAddToggle(index, checked) => {
                let Some(draft) = self.sa_add.as_mut() else {
                    return;
                };
                let Some(pick) = draft.type_pick else {
                    return;
                };
                let Some(config) = self.config.as_ref() else {
                    return;
                };
                let options = sa::add_type_options(config);
                let Some(option) = options.get(pick) else {
                    return;
                };
                let (match_type, match_value) = sa::add_target(config, &option.id);
                let covering = sa::covering(&self.catalog, &match_type, &match_value);
                let Some(pack) = covering.get(index) else {
                    return;
                };
                let behavior = pack.id.clone();
                if checked {
                    // 勾选序 = 菜单序；9 上限（复刻 BehaviorPickVm）
                    if draft.checked.len() < 9 && !draft.checked.contains(&behavior) {
                        draft.checked.push(behavior);
                    }
                } else {
                    draft.checked.retain(|existing| existing != &behavior);
                }
            }
            Message::SaAddConfirm => {
                let Some(draft) = self.sa_add.as_mut() else {
                    return;
                };
                let Some(pick) = draft.type_pick else {
                    return;
                };
                let Some(config) = self.config.as_ref() else {
                    return;
                };
                let options = sa::add_type_options(config);
                let Some(option) = options.get(pick) else {
                    return;
                };
                if draft.checked.is_empty() {
                    // CanConfirm：至少勾一个行为（1104_any = 「任意」文案即缺位提示）
                    draft.error = Some(i18n::t("1104_any"));
                    return;
                }
                let (match_type, match_value) = sa::add_target(config, &option.id);
                // 重复条件拦截（1115）：同 (matchType, matchValue) 已配置
                if sa::mapping_exists(config, &match_type, &match_value) {
                    draft.error = Some(i18n::t("1115"));
                    return;
                }
                let entries: Vec<SelectedEntry> = draft
                    .checked
                    .iter()
                    .map(|behavior| SelectedEntry {
                        behavior: behavior.clone(),
                        action_value: self.catalog.default_template_for(behavior),
                        ..Default::default()
                    })
                    .collect();
                let Some(config) = self.config.as_mut() else {
                    return;
                };
                config
                    .selected_action
                    .mappings
                    .push(crate::models::SelectedMapping {
                        match_type: match_type.clone(),
                        match_value,
                        entries,
                    });
                self.sa_add = None;
            }
            // ---------------------------------------------------------- 匹配类型管理
            Message::SaExtsEditValue(index, value) => {
                self.exts_edit = Some((index, value));
                // 800ms 尾随**归一**（generation 防抖，复刻行为编辑语义）——只把后缀串
                // 归一写回内存 config（chips 数据源），落盘统一走页脚「保存配置」。
                let generation = self
                    .exts_save_gen
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1;
                let gen_slot = std::sync::Arc::clone(&self.exts_save_gen);
                let _ = context.spawn_background(move |_token| {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    if gen_slot.load(std::sync::atomic::Ordering::SeqCst) == generation {
                        Message::SaExtsEditCommit(generation)
                    } else {
                        Message::Noop
                    }
                });
            }
            Message::SaExtsEditCommit(generation) => {
                // 代际校验：有更新编辑 ⇒ 本代失效
                if self.exts_save_gen.load(std::sync::atomic::Ordering::SeqCst) != generation {
                    return;
                }
                let Some((index, text)) = self.exts_edit.clone() else {
                    return;
                };
                self.exts_edit = None;
                // 直接写 file_groups[index].exts（chips 数据源）+ 归一（内存态；
                // 落盘统一走页脚「保存配置」）
                let mut config_holder = self.config.clone();
                let applied = config_holder.as_mut().and_then(|config| {
                    config.file_groups.get_mut(index).map(|fg| {
                        fg.exts = sa::normalize_exts(&text);
                    })
                });
                if applied.is_some() {
                    self.config = config_holder;
                }
            }
            _ => {}
        }
    }
}
