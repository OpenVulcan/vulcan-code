//! Provider, MCP, and model-selection support methods extracted from the main workbench module.
//! 从主工作台模块中抽取的供应商、MCP 与模型选择支撑方法。

use super::helpers::*;
use super::*;
use crate::ai_tool_configuration;
use crate::context_budget::{ContextCompactionPolicy, build_context_budget_fingerprint};
use crate::provider_capabilities::{
    ResolvedModelCapabilities, apply_ai_tool_runtime_configuration, resolve_model_capabilities,
};
use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
};

impl VulcanWorkbench {
    /// Resolves the remembered initial workbench reasoning preference from persisted configuration and falls back to the shared medium preset when no prior selection exists.
    /// 从持久化配置解析已记忆的初始工作台思考偏好；当不存在历史选择时回退到共享的中等预设。
    pub(super) fn resolve_initial_workbench_reasoning_effort(
        config: &ProviderConfigurationFile,
    ) -> WorkbenchReasoningEffort {
        config
            .workbench_reasoning_effort
            .unwrap_or(WorkbenchReasoningEffort::Medium)
    }

    /// Resolves the initial provider picker choice using the persisted configuration and supported catalog order.
    /// 使用持久化配置与受支持目录顺序解析初始供应商选择项。
    pub(super) fn resolve_initial_provider_choice(
        config: &ProviderConfigurationFile,
        choices: &[ProviderChoice],
    ) -> ProviderChoice {
        let preferred = config
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, _)| provider_id.to_string())
            .or_else(|| config.provider.keys().next().cloned());

        preferred
            .and_then(|provider_id| choices.iter().find(|item| item.id == provider_id).cloned())
            .or_else(|| choices.first().cloned())
            .unwrap_or_else(|| ProviderChoice {
                id: String::from("openai"),
                name: String::from("OpenAI"),
            })
    }

    /// Resolves the initial MCP server identifier using the persisted configuration order.
    /// 使用持久化配置顺序解析初始 MCP 服务器标识。
    pub(super) fn resolve_initial_mcp_server_id(
        config: &ProviderConfigurationFile,
    ) -> Option<String> {
        config.mcp.servers.keys().next().cloned()
    }

    /// Builds the configured agent choices rendered in the runtime settings card.
    /// 构建渲染在运行时设置卡片中的已配置代理选择项。
    pub(super) fn configured_agent_choices(&self) -> Vec<AgentChoice> {
        vec![
            AgentChoice {
                id: String::from(SYSTEM_DEFAULT_AGENT_CHOICE_ID),
                label: String::from("系统默认（按模型）"),
            },
            AgentChoice {
                id: String::from(PLAN_AGENT_ID),
                label: String::from("方案代理"),
            },
            AgentChoice {
                id: String::from(EXPLORE_AGENT_ID),
                label: String::from("探索代理"),
            },
        ]
        .into_iter()
        .chain(
            self.agent_profile_catalog
                .agents
                .values()
                .filter(|profile| !agent_profile::is_reserved_agent_identifier(&profile.id))
                .map(|profile| AgentChoice {
                    id: profile.id.clone(),
                    label: profile.name.clone(),
                }),
        )
        .collect()
    }

    /// Returns the currently active agent choice rendered in the runtime settings card.
    /// 返回渲染在运行时设置卡片中的当前激活代理选择项。
    pub(super) fn active_agent_choice(&self) -> Option<AgentChoice> {
        let active_agent_id = self
            .agent_profile_catalog
            .state
            .selected_agent_id
            .as_deref()
            .unwrap_or(SYSTEM_DEFAULT_AGENT_CHOICE_ID);
        let choices = self.configured_agent_choices();

        choices
            .iter()
            .find(|choice| choice.id == active_agent_id)
            .cloned()
            .or_else(|| choices.first().cloned())
    }

    /// Builds the configured MCP server choices rendered in the MCP settings card.
    /// 构建渲染在 MCP 设置卡片中的已配置 MCP 服务器选择项。
    pub(super) fn configured_mcp_server_choices(&self) -> Vec<McpServerChoice> {
        self.provider_configuration
            .mcp
            .servers
            .iter()
            .map(|(id, entry)| McpServerChoice {
                id: id.clone(),
                label: format!(
                    "{} · {}",
                    entry.name,
                    if entry.enabled { "启用" } else { "禁用" }
                ),
            })
            .collect()
    }

    /// Returns the currently selected MCP server choice rendered in the MCP settings card.
    /// 返回渲染在 MCP 设置卡片中的当前选中 MCP 服务器选择项。
    pub(super) fn selected_mcp_server_choice(&self) -> Option<McpServerChoice> {
        let selected_server_id = self.selected_mcp_server_id.as_deref()?;

        self.configured_mcp_server_choices()
            .into_iter()
            .find(|choice| choice.id == selected_server_id)
    }

    /// Reloads the editable MCP server form from the currently selected persisted entry.
    /// 根据当前选中的持久化条目重新载入可编辑 MCP 服务器表单。
    pub(super) fn reload_selected_mcp_server_form(&mut self) {
        self.mcp_server_form = self
            .selected_mcp_server_id
            .as_deref()
            .and_then(|server_id| self.provider_configuration.mcp.servers.get(server_id))
            .map(McpServerFormState::from_entry)
            .unwrap_or_else(|| McpServerFormState {
                enabled: true,
                ..McpServerFormState::default()
            });
    }

    /// Reloads the editable provider form from the persisted configuration entry of the selected provider.
    /// 从当前选中供应商的持久化配置条目重新载入可编辑表单。
    pub(super) fn reload_selected_provider_form(&mut self) {
        self.provider_form = self
            .provider_configuration
            .provider
            .get(&self.selected_provider.id)
            .map(|entry| ProviderFormState::from_options(&entry.options))
            .unwrap_or_default();
    }

    /// Returns the currently selected provider definition, if it is supported.
    /// 返回当前选中且受支持的供应商定义（如果存在）。
    pub(super) fn selected_provider_definition(
        &self,
    ) -> Option<&'static provider_config::SupportedProviderDefinition> {
        provider_config::supported_provider_by_id(&self.selected_provider.id)
    }

    /// Returns the configured provider list rendered in the provider settings card.
    /// 返回渲染在供应商设置卡片中的已配置供应商列表。
    pub(super) fn configured_provider_choices(&self) -> Vec<ProviderChoice> {
        provider_config::supported_provider_choices()
            .into_iter()
            .filter(|choice| {
                self.provider_configuration
                    .provider
                    .contains_key(&choice.id)
            })
            .collect()
    }

    /// Returns the effective model list for the currently selected provider.
    /// 返回当前选中供应商的生效模型列表。
    pub(super) fn current_provider_model_choices(&self) -> Vec<ModelChoice> {
        provider_config::effective_model_choices(
            &self.selected_provider.id,
            &self.provider_configuration,
        )
    }

    /// Returns the globally configured workbench model choices across every currently configured provider.
    /// 返回横跨全部当前已配置供应商的全局工作台模型选项集合。
    pub(super) fn configured_workbench_model_choices(&self) -> Vec<WorkbenchModelChoice> {
        let mut choices = provider_config::supported_provider_choices()
            .into_iter()
            .filter(|provider_choice| {
                self.provider_configuration
                    .provider
                    .contains_key(&provider_choice.id)
            })
            .flat_map(|provider_choice| {
                let provider_id = provider_choice.id.clone();
                let provider_label = provider_choice.name.clone();

                provider_config::effective_model_choices(&provider_id, &self.provider_configuration)
                    .into_iter()
                    .map(move |model_choice| WorkbenchModelChoice {
                        provider_id: provider_id.clone(),
                        provider_label: provider_label.clone(),
                        model_id: model_choice.id,
                        model_name: model_choice.name,
                    })
            })
            .collect::<Vec<_>>();

        choices.sort_by(|left, right| {
            left.model_name
                .cmp(&right.model_name)
                .then_with(|| left.provider_label.cmp(&right.provider_label))
                .then_with(|| left.model_id.cmp(&right.model_id))
        });
        choices
    }

    /// Returns the currently active workbench model choice when it still exists in the configured global model catalog.
    /// 返回当前激活且仍存在于已配置全局模型目录中的工作台模型选项。
    pub(super) fn active_workbench_model_choice(&self) -> Option<WorkbenchModelChoice> {
        let (active_provider_id, active_model_id) = self
            .provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)?;

        self.configured_workbench_model_choices()
            .into_iter()
            .find(|choice| {
                choice.provider_id == active_provider_id && choice.model_id == active_model_id
            })
    }

    /// Resolves the dynamic reasoning selector view for the currently active workbench model and the remembered user preference.
    /// 根据当前激活的工作台模型与已记忆用户偏好解析动态思考选择器视图。
    pub(super) fn active_workbench_reasoning_view(
        &self,
    ) -> Option<provider_config::ReasoningSelectionView> {
        let active_choice = self.active_workbench_model_choice()?;
        let definition = provider_config::supported_provider_by_id(&active_choice.provider_id)?;
        let active_model = provider_config::effective_model_choices(
            &active_choice.provider_id,
            &self.provider_configuration,
        )
        .into_iter()
        .find(|model| model.id == active_choice.model_id);

        Some(definition.reasoning_selection_view(
            active_model.as_ref(),
            self.workbench_reasoning_effort_preference,
        ))
    }

    /// Reconciles the remembered workbench reasoning preference against the active model capability set and updates the effective effort.
    /// 将已记忆的工作台思考偏好与当前激活模型能力集合对齐，并更新生效档位。
    pub(super) fn reconcile_workbench_reasoning_effort_for_active_model(
        &mut self,
    ) -> Option<String> {
        let Some(view) = self.active_workbench_reasoning_view() else {
            self.workbench_reasoning_effort = self.workbench_reasoning_effort_preference;
            return None;
        };

        self.workbench_reasoning_effort = view.selected_effort;
        view.note
    }

    /// Resolves the full model entry for one concrete workbench model choice.
    /// 根据指定工作台模型选项解析完整模型条目。
    pub(super) fn resolve_workbench_model_entry(
        &self,
        choice: &WorkbenchModelChoice,
    ) -> Option<provider_config::ModelChoice> {
        provider_config::effective_model_choices(&choice.provider_id, &self.provider_configuration)
            .into_iter()
            .find(|model| model.id == choice.model_id)
    }

    /// Resolves the capability snapshot for one concrete workbench model choice.
    /// 根据指定工作台模型选项解析能力快照。
    fn resolve_workbench_model_capabilities(
        &self,
        choice: &WorkbenchModelChoice,
    ) -> Option<ResolvedModelCapabilities> {
        let definition = provider_config::supported_provider_by_id(&choice.provider_id)?;
        let model = self.resolve_workbench_model_entry(choice);
        let mut capabilities = resolve_model_capabilities(definition, model.as_ref());
        apply_ai_tool_runtime_configuration(&mut capabilities, &self.ai_tool_configuration);
        Some(capabilities)
    }

    /// Resolves the provider-declared base capability snapshot for the currently active workbench model without applying user routing overrides.
    /// 在不应用用户路由覆盖的前提下解析当前激活工作台模型的供应商声明基础能力快照。
    fn active_workbench_base_model_capabilities(&self) -> Option<ResolvedModelCapabilities> {
        let active_choice = self.active_workbench_model_choice()?;
        let definition = provider_config::supported_provider_by_id(&active_choice.provider_id)?;
        let model = self.resolve_workbench_model_entry(&active_choice);
        Some(resolve_model_capabilities(definition, model.as_ref()))
    }

    /// Returns whether the currently active workbench model declares one native web-search capability regardless of the current user routing preference.
    /// 返回当前激活工作台模型是否声明了原生网页搜索能力，并且不受当前用户路由偏好的影响。
    fn active_workbench_has_native_web_search(&self) -> bool {
        self.active_workbench_base_model_capabilities()
            .is_some_and(|capabilities| {
                capabilities.ai_tools.iter().any(|tool| {
                    tool.kind == crate::provider_config::AiToolKind::WebSearch
                        && tool.native_provider_tool
                        && tool.policy != crate::provider_config::AiToolPolicy::Disabled
                })
            })
    }

    /// Reconciles the visible temperature controls against the currently active workbench model capability set.
    /// 将当前可见温度控制与激活工作台模型的能力集合对齐。
    pub(super) fn reconcile_workbench_temperature_for_active_model(&mut self) -> Option<String> {
        let active_choice = self.active_workbench_model_choice()?;
        let capabilities = self.resolve_workbench_model_capabilities(&active_choice)?;

        if capabilities.supports_temperature {
            return None;
        }

        if self.temperature_enabled {
            self.temperature_enabled = false;
            return Some(format!(
                "当前模型 `{}` 未声明显式温度能力，已自动关闭温度覆盖。",
                active_choice.model_name
            ));
        }

        None
    }

    /// Invalidates the selected session context anchor when the newly chosen model would no longer share the same budget fingerprint.
    /// 当新选择模型不再共享相同预算指纹时，使当前选中会话的上下文锚点失效。
    pub(super) fn reconcile_selected_session_context_anchor_after_model_switch(
        &mut self,
        choice: &WorkbenchModelChoice,
    ) -> Option<String> {
        let session_id = self.selected_session().map(|session| session.id)?;
        let Some(previous_runtime_state) = self.session_runtime_state(session_id).cloned() else {
            return None;
        };
        let Some(previous_provider_id) = previous_runtime_state.active_provider_id.as_deref()
        else {
            return None;
        };
        let Some(previous_model_id) = previous_runtime_state.active_model_id.as_deref() else {
            return None;
        };
        if previous_provider_id == choice.provider_id && previous_model_id == choice.model_id {
            return None;
        }

        let previous_choice = WorkbenchModelChoice {
            provider_id: previous_provider_id.to_string(),
            provider_label: provider_config::provider_summary(previous_provider_id),
            model_id: previous_model_id.to_string(),
            model_name: provider_config::model_summary(
                previous_provider_id,
                previous_model_id,
                &self.provider_configuration,
            ),
        };
        let Some(previous_capabilities) =
            self.resolve_workbench_model_capabilities(&previous_choice)
        else {
            return None;
        };
        let Some(next_capabilities) = self.resolve_workbench_model_capabilities(choice) else {
            return None;
        };
        let previous_api_family = provider_config::supported_provider_by_id(previous_provider_id)
            .map(|definition| {
            definition.effective_api_family_for_model(Some(previous_model_id))
        })?;
        let next_api_family = provider_config::supported_provider_by_id(&choice.provider_id)
            .map(|definition| definition.effective_api_family_for_model(Some(&choice.model_id)))?;
        let visible_tool_count = self
            .last_execution_preparation
            .as_ref()
            .filter(|preparation| preparation.session_id == session_id)
            .map(|preparation| preparation.tool_descriptors.len())
            .unwrap_or(0usize);
        let compaction_policy = ContextCompactionPolicy::default();
        let previous_fingerprint = build_context_budget_fingerprint(
            previous_provider_id,
            previous_model_id,
            previous_api_family,
            &previous_capabilities,
            self.workbench_reasoning_effort,
            visible_tool_count,
            Some(compaction_policy),
        );
        let next_fingerprint = build_context_budget_fingerprint(
            &choice.provider_id,
            &choice.model_id,
            next_api_family,
            &next_capabilities,
            self.workbench_reasoning_effort,
            visible_tool_count,
            Some(compaction_policy),
        );
        if previous_fingerprint == next_fingerprint {
            return None;
        }

        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state.last_compaction_id = None;
            runtime_state.context_start_message_id = None;
            runtime_state.updated_at = current_unix_millis_text();
        }
        self.last_execution_preparation = None;
        self.sync_context_usage_from_preparation(None);
        self.replace_session_last_api_request_preview(
            session_id,
            String::from(
                "# 最后一次真实 API 请求快照\n\n模型已切换，上一份请求快照与预算结果已失效。",
            ),
        );

        Some(format!(
            "会话 `{}` 的旧上下文锚点已因模型预算变化失效，后续将按新模型预算重新压缩。",
            choice.model_name
        ))
    }

    /// Persists the remembered workbench reasoning preference back into the shared provider configuration file.
    /// 将已记忆的工作台思考偏好持久化回共享供应商配置文件。
    pub(super) fn persist_workbench_reasoning_effort_preference(&mut self) -> Result<(), String> {
        self.provider_configuration.workbench_reasoning_effort =
            Some(self.workbench_reasoning_effort_preference);
        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Returns the currently visible workbench model choices after applying the in-modal filter text.
    /// 返回应用弹窗内过滤文本后当前可见的工作台模型选项集合。
    pub(super) fn filtered_workbench_model_choices(&self) -> Vec<WorkbenchModelChoice> {
        let filter = self.workbench_model_picker_filter.trim().to_lowercase();

        if filter.is_empty() {
            return self.configured_workbench_model_choices();
        }

        self.configured_workbench_model_choices()
            .into_iter()
            .filter(|choice| {
                choice.model_name.to_lowercase().contains(&filter)
                    || choice.provider_label.to_lowercase().contains(&filter)
                    || choice.model_id.to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// Returns the preferred highlighted index when the model-picker modal opens with a fresh filter.
    /// 返回模型选择弹窗以全新过滤状态打开时优先高亮的索引。
    pub(super) fn preferred_workbench_model_picker_index(&self) -> Option<usize> {
        let filtered_choices = self.filtered_workbench_model_choices();

        if filtered_choices.is_empty() {
            return None;
        }

        let active_choice = self.active_workbench_model_choice();

        active_choice
            .and_then(|choice| {
                filtered_choices
                    .iter()
                    .position(|candidate| candidate == &choice)
            })
            .or(Some(0))
    }

    /// Opens the in-app workbench model-picker modal and focuses its filter field.
    /// 打开工作台内嵌模型选择弹窗并聚焦其过滤输入框。
    pub(super) fn open_workbench_model_picker(&mut self) -> Task<Message> {
        self.workbench_model_picker_open = true;
        self.workbench_model_picker_filter.clear();
        self.workbench_model_picker_highlighted_index =
            self.preferred_workbench_model_picker_index();

        operation::focus(WORKBENCH_MODEL_PICKER_FILTER_INPUT_ID)
    }

    /// Keeps the highlighted index inside the visible bounds of the filtered model-picker result list.
    /// 使模型选择弹窗的高亮索引始终保持在过滤结果列表的可见边界内。
    pub(super) fn clamp_workbench_model_picker_highlight(&mut self) {
        let filtered_len = self.filtered_workbench_model_choices().len();

        self.workbench_model_picker_highlighted_index = if filtered_len == 0 {
            None
        } else {
            Some(
                self.workbench_model_picker_highlighted_index
                    .unwrap_or(0)
                    .min(filtered_len.saturating_sub(1)),
            )
        };
    }

    /// Resolves the currently highlighted model choice from the filtered model-picker result list.
    /// 从过滤后的模型选择结果列表中解析当前高亮的模型选项。
    pub(super) fn highlighted_workbench_model_choice(&self) -> Option<WorkbenchModelChoice> {
        let index = self.workbench_model_picker_highlighted_index?;

        self.filtered_workbench_model_choices().get(index).cloned()
    }

    /// Returns the full list of workbench-scoped web-search override choices shown beside the composer.
    /// 返回显示在提示词编辑器旁侧的完整工作台网页搜索覆盖选项列表。
    pub(super) fn configured_workbench_web_search_choices(&self) -> Vec<WorkbenchWebSearchChoice> {
        let native_available = self.active_workbench_has_native_web_search();
        let provider_targets = self
            .ai_tool_configuration
            .usable_targets_for_kind(crate::provider_config::AiToolKind::WebSearch);
        let provider_chain_available = !provider_targets.is_empty();
        let mut choices = Vec::new();

        if native_available && provider_chain_available {
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::FollowGlobal,
                provider_id: None,
                label: String::from("智能搜索"),
                detail: String::from("跟随全局 web-search 设置"),
            });
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::NativeOnly,
                provider_id: None,
                label: String::from("原生搜索"),
                detail: String::from("仅声明模型原生搜索"),
            });
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::ThirdPartyOnly,
                provider_id: None,
                label: String::from("自定义搜索链"),
                detail: String::from("仅使用第三方搜索链"),
            });
        } else if native_available {
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::NativeOnly,
                provider_id: None,
                label: String::from("原生搜索"),
                detail: String::from("当前模型仅支持原生搜索"),
            });
            return choices;
        } else if provider_chain_available {
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::FollowGlobal,
                provider_id: None,
                label: String::from("智能搜索"),
                detail: String::from("跟随全局 web-search 设置"),
            });
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::ThirdPartyOnly,
                provider_id: None,
                label: String::from("自定义搜索链"),
                detail: String::from("仅使用第三方搜索链"),
            });
        } else {
            choices.push(WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::Disabled,
                provider_id: None,
                label: String::from("关闭搜索"),
                detail: String::from("当前模型与扩展都未提供统一搜索能力"),
            });
            return choices;
        }

        let provider_choices = provider_targets
            .iter()
            .map(|target| WorkbenchWebSearchChoice {
                override_mode: PendingPromptWebSearchOverride::SpecificProvider,
                provider_id: Some(target.clone()),
                label: format!("指定：{}", target),
                detail: String::from("显式目标"),
            })
            .collect::<Vec<_>>();
        choices.extend(provider_choices);
        choices.push(WorkbenchWebSearchChoice {
            override_mode: PendingPromptWebSearchOverride::Disabled,
            provider_id: None,
            label: String::from("关闭搜索"),
            detail: String::from("不启用统一 web-search"),
        });
        choices
    }

    /// Resolves the currently active workbench web-search override choice, falling back to the global-default entry when needed.
    /// 解析当前激活的工作台网页搜索覆盖选项；必要时回退到全局默认条目。
    pub(super) fn active_workbench_web_search_choice(&self) -> Option<WorkbenchWebSearchChoice> {
        let configured_choices = self.configured_workbench_web_search_choices();
        configured_choices
            .iter()
            .find(|choice| {
                choice.override_mode == self.workbench_web_search_override
                    && choice.provider_id == self.workbench_web_search_provider_id
            })
            .cloned()
            .or_else(|| {
                configured_choices
                    .iter()
                    .find(|choice| {
                        choice.override_mode == PendingPromptWebSearchOverride::FollowGlobal
                    })
                    .cloned()
                    .or_else(|| {
                        configured_choices
                            .iter()
                            .find(|choice| {
                                choice.override_mode == PendingPromptWebSearchOverride::NativeOnly
                            })
                            .cloned()
                    })
                    .or_else(|| {
                        configured_choices
                            .iter()
                            .find(|choice| {
                                choice.override_mode
                                    == PendingPromptWebSearchOverride::ThirdPartyOnly
                            })
                            .cloned()
                    })
                    .or_else(|| configured_choices.into_iter().next())
            })
    }

    /// Builds one effective AI-tool configuration snapshot after applying the current workbench web-search override.
    /// 在应用当前工作台网页搜索覆盖后构建一份生效 AI 工具配置快照。
    pub(super) fn effective_ai_tool_configuration_for_runtime_options(
        &self,
        runtime_options: &PendingPromptRuntimeOptions,
    ) -> PersistedAiToolConfiguration {
        let mut configuration = self.ai_tool_configuration.clone();
        let native_available = self.active_workbench_has_native_web_search();
        let provider_targets =
            configuration.usable_targets_for_kind(crate::provider_config::AiToolKind::WebSearch);
        let provider_chain_available = !provider_targets.is_empty();
        let specific_target = runtime_options
            .web_search_provider_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let normalized_override = match runtime_options.web_search_override {
            PendingPromptWebSearchOverride::SpecificProvider => {
                if specific_target.as_ref().is_some_and(|target| {
                    provider_targets
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(target))
                }) {
                    PendingPromptWebSearchOverride::SpecificProvider
                } else if provider_chain_available {
                    PendingPromptWebSearchOverride::ThirdPartyOnly
                } else if native_available {
                    PendingPromptWebSearchOverride::NativeOnly
                } else {
                    PendingPromptWebSearchOverride::Disabled
                }
            }
            PendingPromptWebSearchOverride::NativeOnly if !native_available => {
                if provider_chain_available {
                    PendingPromptWebSearchOverride::ThirdPartyOnly
                } else {
                    PendingPromptWebSearchOverride::Disabled
                }
            }
            PendingPromptWebSearchOverride::ThirdPartyOnly if !provider_chain_available => {
                if native_available {
                    PendingPromptWebSearchOverride::NativeOnly
                } else {
                    PendingPromptWebSearchOverride::Disabled
                }
            }
            override_mode => override_mode,
        };

        match normalized_override {
            PendingPromptWebSearchOverride::FollowGlobal => {}
            PendingPromptWebSearchOverride::NativeOnly => {
                configuration.web_search.mode = WebSearchRoutingMode::NativeOnly;
            }
            PendingPromptWebSearchOverride::ThirdPartyOnly => {
                configuration.web_search.mode = WebSearchRoutingMode::ThirdPartyOnly;
            }
            PendingPromptWebSearchOverride::Disabled => {
                configuration.web_search.mode = WebSearchRoutingMode::Disabled;
            }
            PendingPromptWebSearchOverride::SpecificProvider => {
                configuration.web_search.mode = WebSearchRoutingMode::ThirdPartyOnly;
                if let Some(target) = specific_target.as_deref() {
                    configuration.web_search.target_order = vec![target.to_string()];
                } else {
                    configuration.web_search.target_order.clear();
                }
            }
        }

        configuration
    }

    /// Captures the currently selected workbench provider/model pair for a queued prompt.
    /// 为排队提示词捕获当前选中的工作台供应商与模型组合。
    pub(super) fn current_pending_prompt_model_reference(
        &self,
    ) -> Option<PendingPromptModelReference> {
        if let Some(choice) = self.active_workbench_model_choice() {
            return Some(PendingPromptModelReference {
                provider_id: choice.provider_id,
                model_id: choice.model_id,
                provider_label: choice.provider_label,
                model_name: choice.model_name,
            });
        }

        let (provider_id, model_id) = self
            .provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)?;
        Some(PendingPromptModelReference {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            provider_label: provider_config::provider_summary(provider_id),
            model_name: provider_config::model_summary(
                provider_id,
                model_id,
                &self.provider_configuration,
            ),
        })
    }

    /// Captures the currently selected workbench runtime controls for a queued prompt.
    /// 为排队提示词捕获当前选中的工作台运行控制参数。
    pub(super) fn current_pending_prompt_runtime_options(&self) -> PendingPromptRuntimeOptions {
        PendingPromptRuntimeOptions {
            reasoning_effort: self.workbench_reasoning_effort,
            temperature: self.temperature_enabled.then_some(self.temperature),
            web_search_override: self.workbench_web_search_override,
            web_search_provider_id: self.workbench_web_search_provider_id.clone(),
        }
    }

    /// Resolves the capability snapshot for a queued prompt's locked provider/model identity.
    /// 根据排队提示词锁定的供应商与模型身份解析能力快照。
    pub(super) fn resolve_pending_prompt_model_capabilities(
        &self,
        model_reference: &PendingPromptModelReference,
    ) -> Option<ResolvedModelCapabilities> {
        let definition = provider_config::supported_provider_by_id(&model_reference.provider_id)?;
        let model = provider_config::effective_model_choices(
            &model_reference.provider_id,
            &self.provider_configuration,
        )
        .into_iter()
        .find(|model| model.id == model_reference.model_id);
        Some(resolve_model_capabilities(definition, model.as_ref()))
    }

    /// Clones the provider configuration and overrides only the active model for one queued prompt run.
    /// 克隆供应商配置，并仅为单次排队提示词运行覆盖激活模型。
    pub(super) fn provider_configuration_for_prompt_entry(
        &self,
        prompt_entry: &PendingSessionPromptEntry,
    ) -> ProviderConfigurationFile {
        let mut config = self.provider_configuration.clone();
        if let Some(model_reference) = prompt_entry.model_reference.as_ref() {
            config.model = Some(format!(
                "{}/{}",
                model_reference.provider_id, model_reference.model_id
            ));
        }
        config
    }

    /// Requests one workbench model switch, prompting first when staged attachments would be dropped by the target model.
    /// 请求一次工作台模型切换；当目标模型会丢弃已暂存附件时先弹窗确认。
    pub(super) fn request_workbench_model_choice(
        &mut self,
        choice: WorkbenchModelChoice,
    ) -> Task<Message> {
        if self.active_workbench_model_choice().as_ref() == Some(&choice) {
            return self.apply_workbench_model_choice(choice);
        }

        if let Some((unsupported_kinds, unsupported_count)) =
            self.unsupported_composer_attachments_for_model_choice(&choice)
        {
            let unsupported_labels = Self::format_attachment_kind_labels(&unsupported_kinds);
            let body = format!(
                "目标模型 `{}` 未声明支持当前输入框中的 {unsupported_labels} 附件。\n\n是否切换模型？点击「切换并移除」会自动移除 {unsupported_count} 个不受支持附件；点击「取消」将保持当前模型和附件不变。",
                choice.model_name
            );
            self.show_workbench_confirmation_modal(
                "目标模型不支持当前附件",
                body,
                "切换并移除",
                "取消",
                WorkbenchNoticeConfirmAction::ApplyModelSwitchRemovingUnsupportedAttachments {
                    choice,
                },
            );
            return Task::none();
        }

        self.apply_workbench_model_choice(choice)
    }

    /// Applies a previously confirmed model switch after removing staged attachments unsupported by the target model.
    /// 移除目标模型不支持的已暂存附件后，应用一次已经确认的模型切换。
    pub(super) fn apply_confirmed_workbench_model_choice_after_attachment_cleanup(
        &mut self,
        choice: WorkbenchModelChoice,
    ) -> Task<Message> {
        let task = self.apply_workbench_model_choice(choice.clone());
        if self.active_workbench_model_choice().as_ref() != Some(&choice) {
            return task;
        }

        let (removed_count, removed_kinds) =
            self.remove_unsupported_composer_attachments_for_model_choice(&choice);
        if removed_count > 0 {
            let removed_labels = Self::format_attachment_kind_labels(&removed_kinds);
            self.status_message.push_str(&format!(
                " 已移除 {removed_count} 个不受支持的 {removed_labels} 附件。"
            ));
        }

        task
    }

    /// Returns staged attachment kinds that the target workbench model does not declare as supported.
    /// 返回目标工作台模型未声明支持的已暂存附件类型。
    fn unsupported_composer_attachments_for_model_choice(
        &self,
        choice: &WorkbenchModelChoice,
    ) -> Option<(Vec<ConversationAttachmentKind>, usize)> {
        let capabilities = self.resolve_workbench_model_capabilities(choice)?;
        let mut unsupported_kinds = Vec::new();
        let mut unsupported_count = 0usize;

        for attachment in &self.composer_attachments {
            if capabilities.allows_staged_attachment_kind(attachment.kind.clone()) {
                continue;
            }
            unsupported_count += 1;
            if !unsupported_kinds.contains(&attachment.kind) {
                unsupported_kinds.push(attachment.kind.clone());
            }
        }

        (unsupported_count > 0).then_some((unsupported_kinds, unsupported_count))
    }

    /// Removes staged composer attachments that the target workbench model does not declare as supported.
    /// 移除目标工作台模型未声明支持的已暂存提示词附件。
    fn remove_unsupported_composer_attachments_for_model_choice(
        &mut self,
        choice: &WorkbenchModelChoice,
    ) -> (usize, Vec<ConversationAttachmentKind>) {
        let Some(capabilities) = self.resolve_workbench_model_capabilities(choice) else {
            return (0, Vec::new());
        };
        let mut removed_count = 0usize;
        let mut removed_kinds = Vec::new();
        let preview_path = self
            .workbench_image_preview_modal
            .as_ref()
            .map(|modal| modal.local_path.clone());

        self.push_composer_undo_snapshot();
        let mut removed_attachments = Vec::new();
        self.composer_attachments.retain(|attachment| {
            if capabilities.allows_staged_attachment_kind(attachment.kind.clone()) {
                return true;
            }
            removed_count += 1;
            if !removed_kinds.contains(&attachment.kind) {
                removed_kinds.push(attachment.kind.clone());
            }
            removed_attachments.push(attachment.clone());
            false
        });

        if removed_count == 0 {
            if let Some(snapshot) = self.composer_undo_history.pop() {
                if let (Some(scope), Some(workspace_path)) = (
                    snapshot.attachment_ref_scope.as_deref(),
                    snapshot.attachment_workspace_path.as_deref(),
                ) {
                    self.release_staged_attachment_refs(
                        workspace_path,
                        scope,
                        &snapshot.attachments,
                        "输入框撤销",
                    );
                }
            }
        } else {
            let workspace_path = self.selected_attachment_workspace_path();
            let composer_scope = self.composer_attachment_ref_scope();
            self.release_staged_attachment_refs(
                &workspace_path,
                &composer_scope,
                &removed_attachments,
                "输入框",
            );
        }

        if preview_path.is_some_and(|path| {
            !self
                .composer_attachments
                .iter()
                .any(|attachment| attachment.local_path == path)
        }) {
            self.workbench_image_preview_modal = None;
        }

        (removed_count, removed_kinds)
    }

    /// Formats attachment-kind labels into a compact slash-separated phrase for confirmation copy.
    /// 将附件类型标签格式化为用于确认文案的紧凑斜杠分隔短语。
    fn format_attachment_kind_labels(kinds: &[ConversationAttachmentKind]) -> String {
        if kinds.is_empty() {
            return String::from("附件");
        }

        kinds
            .iter()
            .map(ConversationAttachmentKind::display_label)
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Applies one chosen workbench model to the persisted global runtime selection and closes the picker.
    /// 将某个工作台模型应用到持久化全局运行时选择并关闭选择弹窗。
    pub(super) fn apply_workbench_model_choice(
        &mut self,
        choice: WorkbenchModelChoice,
    ) -> Task<Message> {
        let mut next_configuration = self.provider_configuration.clone();
        next_configuration.model = Some(format!("{}/{}", choice.provider_id, choice.model_id));
        if let Err(error) = provider_config::save_configuration_file(&next_configuration) {
            self.provider_notice = error.clone();
            self.status_message = error;
            return Task::none();
        }

        self.provider_configuration = next_configuration;
        if let Some(provider_choice) = provider_config::supported_provider_choices()
            .into_iter()
            .find(|provider_choice| provider_choice.id == choice.provider_id)
        {
            self.selected_provider = provider_choice;
            self.reload_selected_provider_form();
        }

        self.refresh_runtime_foundation();
        let reasoning_note = self.reconcile_workbench_reasoning_effort_for_active_model();
        let temperature_note = self.reconcile_workbench_temperature_for_active_model();
        let compaction_note =
            self.reconcile_selected_session_context_anchor_after_model_switch(&choice);
        self.workbench_model_picker_open = false;
        self.workbench_model_picker_filter.clear();
        self.workbench_model_picker_highlighted_index = None;
        let mut notes = Vec::new();
        if let Some(note) = reasoning_note {
            notes.push(note);
        }
        if let Some(note) = temperature_note {
            notes.push(note);
        }
        if let Some(note) = compaction_note {
            notes.push(note);
        }
        self.status_message = if notes.is_empty() {
            format!(
                "当前模型已切换为 `{}`（{}）。",
                choice.model_name, choice.provider_label
            )
        } else {
            format!(
                "当前模型已切换为 `{}`（{}）。{}",
                choice.model_name,
                choice.provider_label,
                notes.join(" ")
            )
        };
        Task::none()
    }

    /// Returns the resolved endpoint summary for the currently selected provider form state.
    /// 返回当前选中供应商表单状态对应的解析后接口地址摘要。
    pub(super) fn selected_provider_endpoint_summary(&self) -> String {
        self.selected_provider_definition()
            .and_then(|definition| definition.resolved_base_url(&self.provider_form.to_options()))
            .unwrap_or_else(|| String::from("由运行时自动解析"))
    }

    /// Returns the model-source summary computed from the current provider definition and cache state.
    /// 返回根据当前供应商定义与缓存状态计算出的模型来源摘要。
    pub(super) fn selected_provider_model_source_summary(&self) -> String {
        let models = self.current_provider_model_choices();

        self.selected_provider_definition()
            .map(|definition| {
                if definition.supports_remote_model_refresh() {
                    let has_cached_models = self
                        .provider_configuration
                        .provider
                        .get(&self.selected_provider.id)
                        .is_some_and(|entry| !entry.models.is_empty());

                    if has_cached_models {
                        format!("已缓存 {} 个模型", models.len())
                    } else {
                        format!("可远端拉取，当前内置 {} 个模型", models.len())
                    }
                } else {
                    format!("使用内置目录，共 {} 个模型", models.len())
                }
            })
            .unwrap_or_else(|| String::from("未找到供应商定义"))
    }

    /// Returns the currently active model choice when it belongs to the selected provider.
    /// 当当前激活模型属于选中供应商时，返回对应的模型选项。
    pub(super) fn active_model_choice_for_selected_provider(&self) -> Option<ModelChoice> {
        let (provider_id, model_id) = self
            .provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)?;

        if provider_id != self.selected_provider.id {
            return None;
        }

        self.current_provider_model_choices()
            .into_iter()
            .find(|model| model.id == model_id)
    }

    /// Returns the summary text rendered for one configured provider entry.
    /// 返回某个已配置供应商条目渲染使用的摘要文本。
    pub(super) fn configured_provider_model_summary(&self, provider_id: &str) -> String {
        self.provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .filter(|(active_provider_id, _)| *active_provider_id == provider_id)
            .map(|(_, model_id)| {
                format!(
                    "当前：{}",
                    provider_config::model_summary(
                        provider_id,
                        model_id,
                        &self.provider_configuration,
                    )
                )
            })
            .unwrap_or_else(|| String::from("未设为当前模型"))
    }

    /// Persists the selected provider form back into the Vulcan configuration file.
    /// 将当前选中供应商表单持久化回 Vulcan 配置文件。
    pub(super) fn save_selected_provider_configuration(&mut self) -> Result<(), String> {
        let definition = self
            .selected_provider_definition()
            .ok_or_else(|| String::from("当前供应商未找到可用定义。"))?;

        for field in definition.input_fields() {
            let value = self.provider_form.get(field).trim();
            if value.is_empty() {
                return Err(format!("请先填写 `{}`。", definition.field_label(field)));
            }
        }

        let entry = self
            .provider_configuration
            .provider
            .entry(definition.id.clone())
            .or_insert_with(PersistedProviderEntry::default);
        entry.name = Some(definition.name.clone());
        entry.options = self.provider_form.to_options();

        if entry.models.is_empty() {
            entry.models = provider_config::persist_remote_models(&definition.models);
        }

        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Persists standalone AI capability configuration and refreshes the runtime foundation.
    /// 持久化独立 AI 能力配置并刷新运行时基础架构。
    pub(super) fn save_ai_tool_configuration(&mut self) -> Result<(), String> {
        ai_tool_configuration::save_ai_tool_configuration_file(&self.ai_tool_configuration)?;
        if !self.provider_configuration.ai_tools.is_default() {
            self.provider_configuration.ai_tools = PersistedAiToolConfiguration::default();
            provider_config::save_configuration_file(&self.provider_configuration)?;
        }
        self.refresh_runtime_foundation();
        Ok(())
    }

    /// Adds one shared AI capability vendor instance when it does not already exist.
    /// 当共享 AI 能力供应商实例尚不存在时新增该实例。
    pub(super) fn add_ai_capability_vendor(&mut self, vendor_id: &str) -> bool {
        let normalized_vendor_id = vendor_id.trim().to_ascii_lowercase();
        if normalized_vendor_id.is_empty() {
            return false;
        }
        if self
            .ai_tool_configuration
            .vendors
            .iter()
            .any(|vendor| vendor.vendor_id == normalized_vendor_id)
        {
            return false;
        }
        self.ai_tool_configuration
            .vendors
            .push(PersistedAiCapabilityVendorEntry::new(normalized_vendor_id));
        self.ai_tool_configuration.normalize();
        true
    }

    /// Removes one shared AI capability vendor instance by identifier.
    /// 根据标识移除一条共享 AI 能力供应商实例。
    pub(super) fn remove_ai_capability_vendor(&mut self, vendor_id: &str) -> bool {
        let before = self.ai_tool_configuration.vendors.len();
        self.ai_tool_configuration
            .vendors
            .retain(|vendor| vendor.vendor_id != vendor_id.trim().to_ascii_lowercase());
        let changed = before != self.ai_tool_configuration.vendors.len();
        if changed {
            self.ai_tool_configuration.normalize();
        }
        changed
    }

    /// Updates one shared AI capability vendor instance in-place when it exists.
    /// 当共享 AI 能力供应商实例存在时原地更新该实例。
    pub(super) fn update_ai_capability_vendor<F>(&mut self, vendor_id: &str, updater: F) -> bool
    where
        F: FnOnce(&mut PersistedAiCapabilityVendorEntry),
    {
        let Some(vendor) = self
            .ai_tool_configuration
            .vendors
            .iter_mut()
            .find(|vendor| vendor.vendor_id == vendor_id.trim().to_ascii_lowercase())
        else {
            return false;
        };
        updater(vendor);
        self.ai_tool_configuration.normalize();
        true
    }

    /// Replaces all enabled targets of one semantic capability beneath the selected vendor instance using the provided temporary picker selections.
    /// 使用提供的临时选择器结果替换选中供应商实例下某个语义能力的全部启用目标。
    pub(super) fn replace_ai_capability_vendor_targets_for_kind(
        &mut self,
        vendor_id: &str,
        capability_kind: AiToolKind,
        selected_targets: Vec<(String, String)>,
    ) -> bool {
        let Some(vendor) = self
            .ai_tool_configuration
            .vendors
            .iter_mut()
            .find(|vendor| vendor.vendor_id == vendor_id.trim().to_ascii_lowercase())
        else {
            return false;
        };

        let supported_choices =
            provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                .into_iter()
                .filter(|choice| choice.capability_kind == capability_kind)
                .collect::<Vec<_>>();
        if supported_choices.is_empty() {
            return false;
        }

        vendor
            .targets
            .retain(|binding| binding.capability_kind != capability_kind);

        for (target_alias, model_id) in selected_targets {
            let Some(choice) = supported_choices.iter().find(|choice| {
                choice
                    .target_alias
                    .eq_ignore_ascii_case(target_alias.as_str())
            }) else {
                continue;
            };
            let resolved_model_id = if model_id.trim().is_empty() {
                choice.default_model_id.clone()
            } else {
                model_id.trim().to_string()
            };
            vendor.targets.push(PersistedAiCapabilityTargetBinding::new(
                capability_kind,
                choice.target_alias.clone(),
                resolved_model_id,
            ));
        }

        self.ai_tool_configuration.normalize();
        true
    }

    /// Removes one enabled semantic capability target from both the vendor binding list and the persisted route order.
    /// 从供应商绑定列表与持久化路由顺序中同时移除单条已启用语义能力目标。
    pub(super) fn remove_ai_capability_route_target(
        &mut self,
        capability_kind: AiToolKind,
        target_selector: &str,
    ) -> bool {
        let normalized_selector = target_selector.trim();
        let Some((vendor_id, target_alias)) = normalized_selector.split_once('/') else {
            return false;
        };
        let Some(vendor) = self
            .ai_tool_configuration
            .vendors
            .iter_mut()
            .find(|vendor| vendor.vendor_id == vendor_id.trim().to_ascii_lowercase())
        else {
            return false;
        };
        let before_len = vendor.targets.len();
        vendor.targets.retain(|binding| {
            !(binding.capability_kind == capability_kind
                && binding
                    .target_alias()
                    .eq_ignore_ascii_case(target_alias.trim()))
        });
        let changed = before_len != vendor.targets.len();
        if changed {
            self.ai_tool_configuration.normalize();
        }
        changed
    }

    /// Persists the currently edited MCP server form back into the shared Vulcan configuration file.
    /// 将当前编辑中的 MCP 服务器表单持久化回共享 Vulcan 配置文件。
    pub(super) fn save_selected_mcp_server_configuration(&mut self) -> Result<String, String> {
        let entry = self.mcp_server_form.to_entry()?;
        let current_server_id = self.selected_mcp_server_id.clone();
        let next_server_id = current_server_id
            .clone()
            .unwrap_or_else(|| self.generate_mcp_server_id(&entry.name));

        self.provider_configuration
            .mcp
            .servers
            .insert(next_server_id.clone(), entry);
        provider_config::save_configuration_file(&self.provider_configuration)?;

        Ok(next_server_id)
    }

    /// Removes the currently selected MCP server entry from the shared Vulcan configuration file.
    /// 从共享 Vulcan 配置文件中移除当前选中的 MCP 服务器条目。
    pub(super) fn remove_selected_mcp_server_configuration(&mut self) -> Result<String, String> {
        let Some(server_id) = self.selected_mcp_server_id.clone() else {
            return Err(String::from("当前没有可移除的 MCP 服务器。"));
        };
        let Some(removed_entry) = self.provider_configuration.mcp.servers.remove(&server_id) else {
            return Err(String::from("当前选中的 MCP 服务器不存在。"));
        };

        provider_config::save_configuration_file(&self.provider_configuration)?;
        self.selected_mcp_server_id =
            Self::resolve_initial_mcp_server_id(&self.provider_configuration);

        Ok(removed_entry.name)
    }

    /// Builds a stable MCP server identifier from the current form name while preserving uniqueness.
    /// 根据当前表单名称构建稳定且唯一的 MCP 服务器标识。
    pub(super) fn generate_mcp_server_id(&self, name: &str) -> String {
        let mut base = slugify_identifier(name);
        if base.is_empty() {
            base = String::from("mcp-server");
        }

        if !self.provider_configuration.mcp.servers.contains_key(&base) {
            return base;
        }

        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base}-{suffix}");
            if !self
                .provider_configuration
                .mcp
                .servers
                .contains_key(&candidate)
            {
                return candidate;
            }
            suffix = suffix.saturating_add(1);
        }
    }

    /// Returns the active provider summary used by the workbench header and activity log.
    /// 返回工作台头部与活动日志使用的当前供应商摘要。
    pub(super) fn active_provider_summary(&self) -> String {
        self.provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, _)| provider_config::provider_summary(provider_id))
            .unwrap_or_else(|| String::from("未配置供应商"))
    }

    /// Returns the active model summary used by the workbench header and activity log.
    /// 返回工作台头部与活动日志使用的当前模型摘要。
    pub(super) fn active_model_summary(&self) -> String {
        self.provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, model_id)| {
                provider_config::model_summary(provider_id, model_id, &self.provider_configuration)
            })
            .unwrap_or_else(|| String::from("未配置模型"))
    }

    /// Rebuilds the compose runtime foundation from the current shared configuration state.
    /// 根据当前共享配置状态重建 compose 运行时基础架构。
    pub(super) fn refresh_runtime_foundation(&mut self) {
        let task_runtime = self.runtime_foundation.task_runtime();
        self.runtime_foundation =
            RuntimeFoundation::build_from_optional_mcp_access_with_task_runtime(
                &self.provider_configuration,
                &self.agent_profile_catalog,
                self.luaskills_runtime.clone(),
                self.mcp_runtime.clone(),
                task_runtime,
            );
        self.last_execution_preparation = None;
        self.last_mcp_discovery_rows.clear();
        self.last_mcp_discovery_errors.clear();
    }

    /// Restarts the long-lived MCP runtime after configuration changes and rebuilds the runtime foundation immediately.
    /// 在配置变更后重启长期 MCP 运行时，并立即重建运行时基础架构。
    pub(super) fn restart_mcp_runtime_task(&mut self) -> Task<Message> {
        if let Some(runtime_access) = self.mcp_runtime.take() {
            runtime_access.shutdown();
        }
        self.refresh_runtime_foundation();
        Task::perform(
            bootstrap_mcp_runtime_access(self.provider_configuration.clone()),
            Message::McpRuntimeBootstrapped,
        )
    }

    /// Returns the currently selected persisted MCP server entry, if any.
    /// 返回当前选中的持久化 MCP 服务器条目（如果存在）。
    pub(super) fn selected_mcp_server_entry(
        &self,
    ) -> Option<&crate::provider_config::PersistedMcpServerEntry> {
        let server_id = self.selected_mcp_server_id.as_deref()?;
        self.provider_configuration.mcp.servers.get(server_id)
    }

    /// Returns the configured OAuth redirect URI for the currently selected MCP server, if any.
    /// 返回当前选中 MCP 服务器已配置的 OAuth 回调 URI（如果存在）。
    pub(super) fn selected_mcp_server_oauth_redirect_uri(&self) -> Option<String> {
        self.selected_mcp_server_entry()
            .and_then(|entry| entry.oauth.as_ref())
            .and_then(|oauth| oauth.redirect_uri.clone())
    }

    /// Loads the latest OAuth status summary for the currently selected MCP server through the live runtime when possible.
    /// 在可能时通过实时运行时载入当前选中 MCP 服务器的最新 OAuth 状态摘要。
    pub(super) fn load_selected_mcp_server_oauth_status_task(&self) -> Task<Message> {
        let Some(server_id) = self.selected_mcp_server_id.clone() else {
            return Task::none();
        };
        let runtime = self.mcp_runtime.clone();
        let status_server_id = server_id.clone();

        Task::perform(
            async move {
                match runtime {
                    Some(runtime_access) => {
                        runtime_access
                            .server_oauth_status_label(&status_server_id)
                            .await
                    }
                    None => Ok(String::from("运行时未就绪")),
                }
            },
            move |result| Message::SelectedMcpServerOAuthStatusLoaded(server_id.clone(), result),
        )
    }

    /// Starts the host-driven OAuth flow for the currently selected MCP server.
    /// 为当前选中的 MCP 服务器启动宿主驱动的 OAuth 流程。
    pub(super) fn start_selected_mcp_server_oauth_task(&self) -> Task<Message> {
        let Some(server_id) = self.selected_mcp_server_id.clone() else {
            return Task::none();
        };
        let Some(runtime_access) = self.mcp_runtime.clone() else {
            return Task::perform(
                async {
                    Err(String::from(
                        "MCP 运行时尚未就绪，暂时无法启动 OAuth 授权。",
                    ))
                },
                move |result| {
                    Message::SelectedMcpServerOAuthChallengeReady(server_id.clone(), result)
                },
            );
        };
        let auth_server_id = server_id.clone();

        Task::perform(
            async move {
                runtime_access
                    .start_server_oauth(&auth_server_id, "Vulcan Code")
                    .await
                    .map(|challenge| {
                        (
                            challenge.authorization_url().to_string(),
                            challenge.oauth_state().to_string(),
                        )
                    })
            },
            move |result| Message::SelectedMcpServerOAuthChallengeReady(server_id.clone(), result),
        )
    }

    /// Finishes the host-driven OAuth flow for the currently selected MCP server from one resolved authorization code and state.
    /// 根据一组已解析的授权码与 state，为当前选中的 MCP 服务器完成宿主驱动 OAuth 流程。
    pub(super) fn finish_selected_mcp_server_oauth_task(
        &self,
        authorization_code: String,
        oauth_state: String,
    ) -> Task<Message> {
        let Some(server_id) = self.selected_mcp_server_id.clone() else {
            return Task::none();
        };
        let Some(runtime_access) = self.mcp_runtime.clone() else {
            return Task::perform(
                async {
                    Err(String::from(
                        "MCP 运行时尚未就绪，暂时无法完成 OAuth 授权。",
                    ))
                },
                move |result| Message::SelectedMcpServerOAuthFinished(server_id.clone(), result),
            );
        };
        let auth_server_id = server_id.clone();

        Task::perform(
            async move {
                runtime_access
                    .finish_server_oauth(&auth_server_id, &authorization_code, &oauth_state)
                    .await
                    .map(|status| status.label().to_string())
            },
            move |result| Message::SelectedMcpServerOAuthFinished(server_id.clone(), result),
        )
    }

    /// Removes stored OAuth credentials for the currently selected MCP server.
    /// 为当前选中的 MCP 服务器移除已存储 OAuth 凭据。
    pub(super) fn remove_selected_mcp_server_oauth_task(&self) -> Task<Message> {
        let Some(server_id) = self.selected_mcp_server_id.clone() else {
            return Task::none();
        };
        let Some(runtime_access) = self.mcp_runtime.clone() else {
            return Task::perform(
                async {
                    Err(String::from(
                        "MCP 运行时尚未就绪，暂时无法清理 OAuth 凭据。",
                    ))
                },
                move |result| {
                    Message::SelectedMcpServerOAuthAuthorizationRemoved(server_id.clone(), result)
                },
            );
        };
        let auth_server_id = server_id.clone();

        Task::perform(
            async move { runtime_access.remove_server_oauth(&auth_server_id).await },
            move |result| {
                Message::SelectedMcpServerOAuthAuthorizationRemoved(server_id.clone(), result)
            },
        )
    }
}
