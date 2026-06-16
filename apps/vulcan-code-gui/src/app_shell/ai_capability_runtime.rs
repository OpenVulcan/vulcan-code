//! AI capability picker and vendor editor runtime helpers for the main workbench.
//! 主工作台的 AI 能力选择器与供应商编辑器运行态辅助逻辑。

use super::*;

impl VulcanWorkbench {
    /// Opens the quick AI capability panel with a fresh editable baseline.
    /// 使用最新可编辑基线打开快捷 AI 能力面板。
    pub(super) fn open_workbench_ai_capability_panel(&mut self) -> Task<Message> {
        self.load_ai_tool_configuration_for_session(self.selected_session);
        self.workbench_ai_capability_panel_baseline =
            Some(self.workbench_ai_capability_panel_draft_snapshot());
        self.workbench_ai_capability_panel_pending_save = None;
        self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Idle;
        self.workbench_ai_capability_panel_open = true;
        Task::none()
    }

    /// Captures the currently persistable quick AI capability panel draft.
    /// 捕获当前可持久化的快捷 AI 能力面板草稿。
    fn workbench_ai_capability_panel_draft_snapshot(&self) -> AiCapabilityPanelDraftSnapshot {
        let disabled_capability_kinds = self.effective_workbench_disabled_ai_capability_kinds();
        let mut configuration = self.ai_tool_configuration.clone();
        configuration.disabled_capability_kinds = disabled_capability_kinds
            .iter()
            .copied()
            .filter(|kind| kind.supports_workbench_quick_toggle())
            .collect();
        configuration.normalize();
        AiCapabilityPanelDraftSnapshot {
            configuration,
            disabled_capability_kinds,
            web_search_override: self.workbench_web_search_override,
            web_search_provider_id: self.workbench_web_search_provider_id.clone(),
        }
    }

    /// Returns whether the quick AI capability panel draft differs from its open-time baseline.
    /// 返回快捷 AI 能力面板草稿是否不同于打开时基线。
    pub(super) fn workbench_ai_capability_panel_has_unsaved_changes(&self) -> bool {
        self.workbench_ai_capability_panel_baseline
            .as_ref()
            .is_some_and(|baseline| {
                baseline != &self.workbench_ai_capability_panel_draft_snapshot()
            })
    }

    /// Marks the quick AI capability panel draft as changed by a local user edit.
    /// 将快捷 AI 能力面板草稿标记为已被本地用户编辑。
    pub(super) fn mark_workbench_ai_capability_panel_draft_changed(
        &mut self,
        notice: impl Into<String>,
    ) {
        self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Idle;
        self.ai_tool_notice = notice.into();
        self.status_message = self.ai_tool_notice.clone();
    }

    /// Requests closing the quick AI capability panel, asking about dirty drafts when needed.
    /// 请求关闭快捷 AI 能力面板，并在存在未保存草稿时询问用户。
    pub(super) fn request_close_workbench_ai_capability_panel(&mut self) -> Task<Message> {
        if self.workbench_ai_capability_panel_save_state == AiCapabilityPanelSaveState::Saving {
            return Task::none();
        }
        if !self.workbench_ai_capability_panel_has_unsaved_changes() {
            self.workbench_ai_capability_panel_open = false;
            self.workbench_ai_capability_panel_baseline = None;
            self.workbench_ai_capability_panel_pending_save = None;
            self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Idle;
            return Task::none();
        }
        self.show_workbench_choice_modal(
            "保存 AI 能力配置变更？",
            "当前快捷面板里有未保存的能力启用状态或顺序调整。保存后会写入当前会话的独立配置；不保存会放弃这些草稿变更。",
            "保存",
            "不保存",
            WorkbenchNoticeConfirmAction::SaveWorkbenchAiCapabilityPanel,
            WorkbenchNoticeConfirmAction::DiscardWorkbenchAiCapabilityPanel,
        );
        Task::none()
    }

    /// Discards the quick AI capability panel draft and closes the panel.
    /// 放弃快捷 AI 能力面板草稿并关闭面板。
    pub(super) fn discard_workbench_ai_capability_panel_changes(&mut self) {
        if let Some(baseline) = self.workbench_ai_capability_panel_baseline.take() {
            self.ai_tool_configuration = baseline.configuration;
            self.workbench_disabled_ai_capability_kinds = baseline.disabled_capability_kinds;
            self.workbench_web_search_override = baseline.web_search_override;
            self.workbench_web_search_provider_id = baseline.web_search_provider_id;
            self.reconcile_workbench_ai_capability_runtime_state();
        }
        self.workbench_ai_capability_panel_open = false;
        self.workbench_ai_capability_panel_pending_save = None;
        self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Idle;
        self.ai_tool_notice = String::from("已放弃快捷面板中的未保存 AI 能力配置变更。");
        self.status_message = self.ai_tool_notice.clone();
    }

    /// Saves the quick AI capability panel draft and closes the panel after persistence succeeds.
    /// 保存快捷 AI 能力面板草稿，并在持久化成功后关闭面板。
    pub(super) fn save_workbench_ai_capability_panel_changes(&mut self) -> Task<Message> {
        if self.workbench_ai_capability_panel_save_state == AiCapabilityPanelSaveState::Saving {
            return Task::none();
        }
        if !self.workbench_ai_capability_panel_has_unsaved_changes() {
            self.workbench_ai_capability_panel_open = false;
            self.workbench_ai_capability_panel_baseline = None;
            self.workbench_ai_capability_panel_pending_save = None;
            self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Idle;
            return Task::none();
        }
        match self.save_ai_tool_configuration() {
            Ok(()) => {
                self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Saving;
                self.ai_tool_notice = String::from("正在保存当前会话的 AI 能力配置。");
                self.status_message = self.ai_tool_notice.clone();
                if self.selected_session != SessionId::nil() {
                    let request_id = format!(
                        "ai-capability-panel:{}:{}",
                        self.selected_session,
                        current_unix_millis_text()
                    );
                    self.workbench_ai_capability_panel_pending_save = self
                        .locally_updated_ai_tool_configuration_sessions
                        .get(&self.selected_session)
                        .map(|guard| AiCapabilityPanelPendingSave {
                            session_id: self.selected_session,
                            request_id: request_id.clone(),
                            hash: guard.hash.clone(),
                        });
                    self.persist_single_session_addon_state_task_with_request_id(
                        self.selected_session,
                        vec![SessionAddonNamespace::AiCapabilities],
                        Some(request_id),
                    )
                } else {
                    self.workbench_ai_capability_panel_save_state =
                        AiCapabilityPanelSaveState::Saved;
                    self.workbench_ai_capability_panel_baseline = None;
                    self.workbench_ai_capability_panel_pending_save = None;
                    self.workbench_ai_capability_panel_open = false;
                    Task::none()
                }
            }
            Err(error) => {
                self.workbench_ai_capability_panel_pending_save = None;
                self.workbench_ai_capability_panel_save_state = AiCapabilityPanelSaveState::Idle;
                self.ai_tool_notice = error.clone();
                self.status_message = error;
                Task::none()
            }
        }
    }

    /// Opens the batch target-picker modal for one semantic AI capability by preloading the current enabled selections from configured vendors.
    /// 通过从已配置供应商中预载当前启用选择，打开某个语义 AI 能力的批量目标选择弹窗。
    pub(super) fn open_ai_capability_target_picker(
        &mut self,
        capability_kind: AiToolKind,
    ) -> Task<Message> {
        let vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .filter(|vendor| {
                provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                    .into_iter()
                    .any(|choice| choice.capability_kind == capability_kind)
            })
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();
        if vendor_ids.is_empty() {
            self.ai_tool_notice = format!(
                "当前还没有可用于“{}”的共享供应商，请先到“能力供应商”页新增并填写凭据。",
                capability_kind.label()
            );
            self.status_message = self.ai_tool_notice.clone();
            self.selected_settings_section = SettingsSection::AiCapabilityVendors;
            self.settings_ai_capability_tree_expanded = true;
            return Task::none();
        }

        let mut selections = BTreeMap::new();
        for vendor_id in &vendor_ids {
            let Some(vendor) = self.ai_tool_configuration.vendor_by_id(vendor_id) else {
                continue;
            };
            let selected_targets = vendor
                .targets
                .iter()
                .filter(|binding| binding.enabled && binding.capability_kind == capability_kind)
                .map(|binding| {
                    (
                        binding.target_alias().to_string(),
                        binding.model_id().to_string(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            selections.insert(vendor_id.clone(), selected_targets);
        }

        self.ai_capability_target_picker = Some(AiCapabilityTargetPickerState {
            capability_kind,
            selected_vendor_id: vendor_ids.first().cloned(),
            selections,
        });
        Task::none()
    }

    /// Updates one staged target checkbox inside the currently visible AI capability picker without mutating persisted configuration yet.
    /// 在当前可见 AI 能力选择器中更新某个暂存目标勾选状态，但暂不修改持久化配置。
    pub(super) fn stage_ai_capability_target_picker_toggle(
        &mut self,
        vendor_id: &str,
        target_alias: &str,
        enabled: bool,
    ) {
        let Some(picker) = self.ai_capability_target_picker.as_mut() else {
            return;
        };
        let choices = provider_config::ai_capability_vendor_target_choices(vendor_id);
        let Some(choice) = choices.into_iter().find(|choice| {
            choice.capability_kind == picker.capability_kind
                && choice.target_alias.eq_ignore_ascii_case(target_alias)
        }) else {
            return;
        };
        if enabled {
            let current_selected_count = picker
                .selections
                .values()
                .map(std::collections::BTreeMap::len)
                .sum::<usize>();
            let already_selected = picker
                .selections
                .get(&vendor_id.to_ascii_lowercase())
                .is_some_and(|targets| targets.contains_key(choice.target_alias.as_str()));
            if !already_selected
                && current_selected_count
                    >= provider_config::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
            {
                self.ai_tool_notice = format!(
                    "“{}”最多只能启用 {} 个接口，请先取消部分已勾选项。",
                    picker.capability_kind.label(),
                    provider_config::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
                );
                self.status_message = self.ai_tool_notice.clone();
                return;
            }
            let vendor_entry = picker
                .selections
                .entry(vendor_id.to_ascii_lowercase())
                .or_default();
            vendor_entry
                .entry(choice.target_alias.clone())
                .or_insert(choice.default_model_id);
        } else {
            if let Some(vendor_entry) = picker.selections.get_mut(&vendor_id.to_ascii_lowercase()) {
                vendor_entry.remove(choice.target_alias.as_str());
            }
        }
    }

    /// Updates one staged request model inside the currently visible AI capability picker.
    /// 更新当前可见 AI 能力选择器中的某个暂存请求模型。
    pub(super) fn stage_ai_capability_target_picker_model(
        &mut self,
        vendor_id: &str,
        target_alias: &str,
        model_id: String,
    ) {
        let Some(picker) = self.ai_capability_target_picker.as_mut() else {
            return;
        };
        let vendor_entry = picker
            .selections
            .entry(vendor_id.to_ascii_lowercase())
            .or_default();
        if let Some(current) = vendor_entry.get_mut(target_alias) {
            *current = model_id;
        }
    }

    /// Applies the staged target selections from the picker modal back into the shared vendor-centric AI capability configuration.
    /// 将选择器弹窗中的暂存目标选择应用回共享供应商中心 AI 能力配置。
    pub(super) fn apply_ai_capability_target_picker_selections(&mut self) -> Task<Message> {
        let Some(picker) = self.ai_capability_target_picker.take() else {
            return Task::none();
        };

        let vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .filter(|vendor| {
                provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                    .into_iter()
                    .any(|choice| choice.capability_kind == picker.capability_kind)
            })
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();

        for vendor_id in vendor_ids {
            let selected_targets = picker
                .selections
                .get(&vendor_id)
                .map(|targets| {
                    targets
                        .iter()
                        .map(|(target_alias, model_id)| (target_alias.clone(), model_id.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.replace_ai_capability_vendor_targets_for_kind(
                &vendor_id,
                picker.capability_kind,
                selected_targets,
            );
        }

        return self.persist_ai_tool_configuration_change(format!(
            "已更新“{}”的启用接口并自动保存。",
            picker.capability_kind.label()
        ));
    }

    /// Opens the shared AI capability vendor editor either for one brand-new vendor or for one already configured vendor instance.
    /// 为全新供应商或已有供应商实例打开共享 AI 能力供应商编辑器。
    pub(super) fn open_ai_capability_vendor_editor(&mut self, vendor_id: &str, is_new: bool) {
        let normalized_vendor_id = vendor_id.trim().to_ascii_lowercase();
        let existing_vendor = self
            .ai_tool_configuration
            .vendor_by_id(&normalized_vendor_id);
        let display_name =
            provider_config::ai_capability_vendor_display_name(&normalized_vendor_id);
        let region_options =
            crate::ai_capability_provider_catalog::ai_capability_vendor_region_options(
                &normalized_vendor_id,
            );
        let connection_mode = existing_vendor
            .map(|vendor| vendor.resolved_connection_mode())
            .unwrap_or(crate::provider_config::AiCapabilityVendorConnectionMode::Local);
        let region = existing_vendor
            .map(|vendor| vendor.region.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| region_options.first().cloned())
            .unwrap_or_default();
        self.ai_capability_vendor_editor = Some(AiCapabilityVendorEditorState {
            vendor_id: normalized_vendor_id,
            display_name,
            api_key: existing_vendor
                .map(|vendor| vendor.api_key.clone())
                .unwrap_or_default(),
            connection_mode,
            base_url: existing_vendor
                .map(|vendor| vendor.base_url.clone())
                .unwrap_or_default(),
            region,
            region_options,
            is_new,
        });
    }

    /// Persists the staged shared AI capability vendor editor back into the vendor list, creating the vendor first when needed.
    /// 将暂存的共享 AI 能力供应商编辑器内容持久化回供应商列表，并在需要时先创建该供应商。
    pub(super) fn save_ai_capability_vendor_editor(&mut self) -> Task<Message> {
        let Some(editor) = self.ai_capability_vendor_editor.take() else {
            return Task::none();
        };
        let definitions =
            crate::ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(
                &editor.vendor_id,
            );
        let supports_api_key_field = definitions.iter().any(|definition| {
            !matches!(
                definition.api_key_requirement,
                crate::ai_capability_provider_catalog::AiCapabilityProviderApiKeyRequirement::None
            )
        });
        let supports_region_field = definitions
            .iter()
            .any(|definition| definition.supports_region_field)
            || !editor.region_options.is_empty();
        if editor.vendor_id.eq_ignore_ascii_case("ollama")
            && matches!(
                editor.connection_mode,
                crate::provider_config::AiCapabilityVendorConnectionMode::Cloud
            )
            && editor.api_key.trim().is_empty()
        {
            self.ai_tool_notice = String::from(
                "Ollama Cloud 模式需要填写 API Key；若要连接本地 Ollama，请切换到“本地”模式。",
            );
            self.status_message = self.ai_tool_notice.clone();
            self.ai_capability_vendor_editor = Some(editor);
            return Task::none();
        }
        if editor.is_new && !self.add_ai_capability_vendor(&editor.vendor_id) {
            self.ai_tool_notice = format!("共享能力供应商 `{}` 已存在。", editor.display_name);
            self.status_message = self.ai_tool_notice.clone();
            return Task::none();
        }
        let vendor_id = editor.vendor_id.clone();
        let display_name = editor.display_name.clone();
        self.update_ai_capability_vendor(&vendor_id, |vendor| {
            vendor.name = provider_config::ai_capability_vendor_display_name(&vendor_id);
            vendor.enabled = true;
            vendor.api_key = if supports_api_key_field {
                editor.api_key.trim().to_string()
            } else {
                String::new()
            };
            vendor.connection_mode = if vendor_id.eq_ignore_ascii_case("ollama") {
                Some(editor.connection_mode)
            } else {
                None
            };
            vendor.base_url = if vendor_id.eq_ignore_ascii_case("ollama")
                && matches!(
                    editor.connection_mode,
                    crate::provider_config::AiCapabilityVendorConnectionMode::Local
                ) {
                editor.base_url.trim().to_string()
            } else {
                String::new()
            };
            vendor.region = if supports_region_field {
                editor.region.trim().to_string()
            } else {
                String::new()
            };
        });
        let success_notice = if editor.is_new {
            format!("已新增共享能力供应商 `{display_name}` 并自动保存。")
        } else {
            format!("已更新共享能力供应商 `{display_name}` 并自动保存。")
        };
        self.persist_ai_tool_configuration_change(success_notice)
    }

    /// Persists the current standalone AI capability configuration immediately after one successful in-memory mutation and updates the shared status notices.
    /// 在一次成功的内存配置变更之后立即持久化当前独立 AI 能力配置，并更新共享状态提示。
    pub(super) fn persist_ai_tool_configuration_change(
        &mut self,
        success_notice: String,
    ) -> Task<Message> {
        match self.save_ai_tool_configuration() {
            Ok(()) => {
                self.ai_tool_notice = success_notice;
                self.status_message = self.ai_tool_notice.clone();
                if self.selected_session != SessionId::nil() {
                    return self.persist_single_session_addon_state_task(
                        self.selected_session,
                        vec![SessionAddonNamespace::AiCapabilities],
                    );
                }
            }
            Err(error) => {
                self.ai_tool_notice = error.clone();
                self.status_message = error;
            }
        }
        Task::none()
    }
}
