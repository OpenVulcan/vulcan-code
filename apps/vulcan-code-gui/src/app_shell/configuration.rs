//! Provider, MCP, and model-selection support methods extracted from the main workbench module.
//! 从主工作台模块中抽取的供应商、MCP 与模型选择支撑方法。

use super::helpers::*;
use super::*;
use crate::AppLocale;
use crate::ai_tool_configuration;
use crate::context_budget::{ContextCompactionPolicy, build_context_budget_fingerprint};
use crate::provider_capabilities::{
    ResolvedModelCapabilities, resolve_effective_model_capabilities, resolve_model_capabilities,
};
use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    normalize_session_transcript_turn_page_size,
};
use crate::tooling::shell_host::{HostCommandShell, HostCommandShellSupport};

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

    /// Resolves the remembered initial GUI locale from persisted configuration and falls back to Simplified Chinese when no prior selection exists.
    /// 从持久化配置解析已记忆的初始图形界面语言；当不存在历史选择时回退到简体中文。
    pub(super) fn resolve_initial_locale(config: &ProviderConfigurationFile) -> AppLocale {
        config
            .ui_locale
            .as_deref()
            .and_then(AppLocale::parse)
            .unwrap_or_default()
    }

    /// Resolves the remembered startup transcript turn-page size from persisted configuration and falls back to the shared default preset when no prior selection exists.
    /// 从持久化配置解析已记忆的启动转录轮次分页大小；当不存在历史选择时回退到共享默认预设。
    pub(super) fn resolve_initial_session_transcript_turn_page_size(
        config: &ProviderConfigurationFile,
    ) -> usize {
        normalize_session_transcript_turn_page_size(config.session_transcript_turn_page_size)
    }

    /// Builds the current host shell-support snapshot from persisted configuration and live host detection.
    /// 根据持久化配置与当前宿主检测结果构建当前 shell 支持快照。
    pub(super) fn command_shell_support(&self) -> HostCommandShellSupport {
        HostCommandShellSupport::from_configured_default_id(
            self.provider_configuration.default_command_shell.as_deref(),
        )
    }

    /// Builds the default-command-shell picker choices rendered in runtime settings.
    /// 构建渲染在运行时设置中的默认命令 shell 下拉选项。
    pub(super) fn command_shell_picker_choices(&self) -> Vec<CommandShellPickerChoice> {
        let shell_support = self.command_shell_support();
        let auto_label = shell_support
            .auto_shell()
            .map(|entry| entry.shell.display_label())
            .unwrap_or_else(|| match self.current_locale() {
                AppLocale::ZhCn => "未检测到可用 shell",
                AppLocale::EnUs => "No shell detected",
            });
        let auto_choice = CommandShellPickerChoice {
            shell: None,
            label: match self.current_locale() {
                AppLocale::ZhCn => format!("系统自动（当前：{auto_label}）"),
                AppLocale::EnUs => format!("System auto (current: {auto_label})"),
            },
        };

        std::iter::once(auto_choice)
            .chain(
                shell_support
                    .supported_shells()
                    .iter()
                    .map(|entry| CommandShellPickerChoice {
                        shell: Some(entry.shell),
                        label: entry.shell.display_label().to_string(),
                    }),
            )
            .collect()
    }

    /// Returns the currently selected default-command-shell picker choice derived from persisted configuration.
    /// 返回根据持久化配置解析出的当前默认命令 shell 下拉选择项。
    pub(super) fn selected_command_shell_picker_choice(&self) -> Option<CommandShellPickerChoice> {
        let selected_shell = self
            .provider_configuration
            .default_command_shell
            .as_deref()
            .and_then(HostCommandShell::parse);

        self.command_shell_picker_choices()
            .into_iter()
            .find(|choice| choice.shell == selected_shell)
            .or_else(|| {
                self.command_shell_picker_choices()
                    .into_iter()
                    .find(|choice| choice.shell.is_none())
            })
    }

    /// Builds the host-shell support summary rendered below the settings picker.
    /// 构建渲染在设置选择器下方的宿主 shell 支持摘要。
    pub(super) fn command_shell_support_summary(&self) -> String {
        let shell_support = self.command_shell_support();
        let supported = if shell_support.supported_shells().is_empty() {
            match self.current_locale() {
                AppLocale::ZhCn => String::from("未检测到可用命令行工具。"),
                AppLocale::EnUs => String::from("No command shells were detected on this host."),
            }
        } else {
            let labels = shell_support
                .supported_shells()
                .iter()
                .map(|entry| entry.shell.display_label())
                .collect::<Vec<_>>();
            match self.current_locale() {
                AppLocale::ZhCn => format!("当前系统支持：{}。", labels.join("、")),
                AppLocale::EnUs => format!("Detected shells: {}.", labels.join(", ")),
            }
        };

        let auto_summary = match shell_support.auto_shell() {
            Some(entry) if shell_support.auto_uses_user_preference() => match self.current_locale()
            {
                AppLocale::ZhCn => {
                    format!(
                        "auto 当前将使用用户指定的 {}。",
                        entry.shell.display_label()
                    )
                }
                AppLocale::EnUs => {
                    format!(
                        "auto currently resolves to the user-selected {}.",
                        entry.shell.display_label()
                    )
                }
            },
            Some(entry) => match self.current_locale() {
                AppLocale::ZhCn => {
                    format!(
                        "auto 当前将按系统优先级使用 {}。",
                        entry.shell.display_label()
                    )
                }
                AppLocale::EnUs => format!(
                    "auto currently resolves to {} by host priority.",
                    entry.shell.display_label()
                ),
            },
            None => match self.current_locale() {
                AppLocale::ZhCn => String::from("auto 当前无法解析到可用 shell。"),
                AppLocale::EnUs => {
                    String::from("auto cannot currently resolve to an available shell.")
                }
            },
        };

        format!("{supported} {auto_summary}")
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

    /// Returns the full catalog model list for the currently selected provider before enabled-model filtering is applied.
    /// 返回当前选中供应商在应用启用模型过滤之前的完整目录模型列表。
    pub(super) fn current_provider_catalog_choices(&self) -> Vec<ModelChoice> {
        provider_config::catalog_model_choices(
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

    /// Returns the currently selected OpenRouter endpoint route for the active workbench model when the model belongs to OpenRouter.
    /// 返回当前激活工作台模型在属于 OpenRouter 时所选中的 OpenRouter 端点路由。
    pub(super) fn active_workbench_openrouter_endpoint_route(
        &self,
    ) -> Option<crate::provider_config::OpenRouterEndpointRouteSelection> {
        let active_choice = self.active_workbench_model_choice()?;
        if active_choice.provider_id != "openrouter" {
            return None;
        }
        provider_config::openrouter_endpoint_route_for_model(
            &self.provider_configuration,
            &active_choice.model_id,
        )
    }

    /// Persists or clears the currently selected OpenRouter endpoint route for the active workbench model.
    /// 为当前激活工作台模型持久化保存或清除所选中的 OpenRouter 端点路由。
    pub(super) fn set_active_workbench_openrouter_endpoint_route(
        &mut self,
        selection: Option<crate::provider_config::OpenRouterEndpointRouteSelection>,
    ) -> Result<(), String> {
        let Some(active_choice) = self.active_workbench_model_choice() else {
            return Err(String::from("当前尚未选中工作台模型，无法设置二级供应商。"));
        };
        if active_choice.provider_id != "openrouter" {
            return Err(String::from(
                "当前模型不属于 OpenRouter，无法设置二级供应商。",
            ));
        }
        provider_config::set_openrouter_endpoint_route_for_model(
            &mut self.provider_configuration,
            &active_choice.model_id,
            selection,
        );
        provider_config::save_configuration_file(&self.provider_configuration)
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
        let runtime_options = self.current_pending_prompt_runtime_options();
        let effective_configuration =
            self.effective_ai_tool_configuration_for_runtime_options(&runtime_options);
        let mut capabilities = resolve_effective_model_capabilities(
            definition,
            model.as_ref(),
            &effective_configuration,
        );
        self.apply_runtime_ai_capability_capability_overrides(&mut capabilities, &runtime_options);
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

    /// Persists the currently selected GUI locale back into the shared provider configuration file.
    /// 将当前选中的图形界面语言持久化回共享供应商配置文件。
    pub(super) fn persist_locale_preference(&mut self) -> Result<(), String> {
        self.provider_configuration.ui_locale =
            Some(self.language_manager.current_locale().as_str().to_string());
        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Persists the currently selected startup transcript turn-page size back into the shared provider configuration file.
    /// 将当前选中的启动转录轮次分页大小持久化回共享 Provider 配置文件。
    pub(super) fn persist_session_transcript_turn_page_size_preference(
        &mut self,
    ) -> Result<(), String> {
        self.provider_configuration
            .session_transcript_turn_page_size = Some(self.session_transcript_turn_page_size);
        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Persists the currently selected default command shell back into shared configuration and refreshes the runtime foundation so tool schemas immediately reflect the new `auto` behavior.
    /// 将当前选中的默认命令 shell 持久化回共享配置，并刷新运行时基础架构，使工具 schema 立刻反映新的 `auto` 行为。
    pub(super) fn persist_default_command_shell_preference(
        &mut self,
        selected_shell: Option<HostCommandShell>,
    ) -> Result<String, String> {
        self.provider_configuration.default_command_shell =
            selected_shell.map(|shell| shell.id().to_string());
        provider_config::save_configuration_file(&self.provider_configuration).map_err(
            |error| match self.current_locale() {
                AppLocale::ZhCn => format!("保存默认命令 shell 配置失败：{error}"),
                AppLocale::EnUs => {
                    format!("Failed to save the default command shell preference: {error}")
                }
            },
        )?;
        self.refresh_runtime_foundation();

        let shell_support = self.command_shell_support();
        let status_message = match selected_shell {
            Some(shell) => match self.current_locale() {
                AppLocale::ZhCn => {
                    format!(
                        "默认命令 shell 已切换为 {}，后续 auto 将优先使用它。",
                        shell.display_label()
                    )
                }
                AppLocale::EnUs => format!(
                    "Default command shell changed to {}; future auto resolution will prefer it.",
                    shell.display_label()
                ),
            },
            None => {
                let auto_label = shell_support
                    .auto_shell()
                    .map(|entry| entry.shell.display_label())
                    .unwrap_or_else(|| match self.current_locale() {
                        AppLocale::ZhCn => "未检测到可用 shell",
                        AppLocale::EnUs => "no detected shell",
                    });
                match self.current_locale() {
                    AppLocale::ZhCn => {
                        format!("默认命令 shell 已切回系统自动检测，当前 auto 为 {auto_label}。")
                    }
                    AppLocale::EnUs => format!(
                        "Default command shell restored to system auto; auto currently resolves to {auto_label}."
                    ),
                }
            }
        };

        Ok(status_message)
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
        self.workbench_reasoning_picker_open = false;
        self.workbench_model_picker_open = true;
        self.workbench_model_picker_filter.clear();
        self.workbench_model_picker_highlighted_index =
            self.preferred_workbench_model_picker_index();
        self.workbench_model_picker_scroll_offset = 0.0;
        self.workbench_model_picker_viewport_height = WORKBENCH_MODEL_PICKER_LIST_HEIGHT;

        let focus_task = operation::focus(WORKBENCH_MODEL_PICKER_FILTER_INPUT_ID);
        let scroll_task = self
            .workbench_model_picker_highlighted_index
            .map(|index| self.scroll_workbench_model_picker_to_index(index))
            .unwrap_or_else(Task::none);

        Task::batch([focus_task, scroll_task])
    }

    /// Opens the OpenRouter secondary-provider picker for the currently active OpenRouter model and asynchronously loads its endpoint catalog.
    /// 为当前激活的 OpenRouter 模型打开二级供应商选择弹窗，并异步加载其端点目录。
    pub(super) fn open_workbench_openrouter_endpoint_picker(&mut self) -> Task<Message> {
        let Some(active_choice) = self.active_workbench_model_choice() else {
            return Task::none();
        };
        if active_choice.provider_id != "openrouter" {
            return Task::none();
        }
        let openrouter_api_key = match self
            .provider_configuration
            .provider
            .get("openrouter")
            .map(|entry| entry.options.resolved_api_key())
            .transpose()
        {
            Ok(api_key) => api_key.and_then(|value| value),
            Err(error) => {
                self.status_message =
                    format!("读取 OpenRouter API Key 失败，无法加载二级供应商目录：{error}");
                return Task::none();
            }
        };

        self.workbench_model_picker_open = false;
        self.workbench_reasoning_picker_open = false;
        self.workbench_openrouter_endpoint_picker = WorkbenchOpenRouterEndpointPickerState {
            open: true,
            model_id: active_choice.model_id.clone(),
            filter: String::new(),
            loading: true,
            error: None,
            choices: Vec::new(),
        };

        Task::perform(
            fetch_openrouter_endpoint_choices_for_model(
                active_choice.model_id.clone(),
                openrouter_api_key,
            ),
            move |result| Message::WorkbenchOpenRouterEndpointsLoaded {
                model_id: active_choice.model_id.clone(),
                result,
            },
        )
    }

    /// Returns the currently visible OpenRouter endpoint choices after applying the in-modal filter text.
    /// 返回应用弹窗内过滤文本后当前可见的 OpenRouter 端点选项集合。
    pub(super) fn filtered_workbench_openrouter_endpoint_choices(
        &self,
    ) -> Vec<WorkbenchOpenRouterEndpointChoice> {
        let filter = self
            .workbench_openrouter_endpoint_picker
            .filter
            .trim()
            .to_lowercase();
        if filter.is_empty() {
            return self.workbench_openrouter_endpoint_picker.choices.clone();
        }
        self.workbench_openrouter_endpoint_picker
            .choices
            .iter()
            .filter(|choice| {
                choice.provider_name.to_lowercase().contains(&filter)
                    || choice.endpoint_name.to_lowercase().contains(&filter)
                    || choice.tag.to_lowercase().contains(&filter)
            })
            .cloned()
            .collect()
    }

    /// Returns the human-readable active runtime configuration summary that should stay pinned while a busy session still uses a locked model snapshot.
    /// 返回当繁忙会话仍在使用锁定模型快照时应保持显示的人类可读运行时配置摘要。
    pub(super) fn selected_session_pending_runtime_config_hint_text(&self) -> Option<String> {
        let session_id = self.selected_session().map(|session| session.id)?;
        if !self.session_has_blocking_system_prompt_work(session_id) {
            return None;
        }
        let runtime_state = self.session_runtime_state(session_id)?;
        let active_provider_id = runtime_state.active_provider_id.as_deref()?;
        let active_model_id = runtime_state.active_model_id.as_deref()?;
        let active_provider_label = provider_config::provider_summary(active_provider_id);
        let active_model_label = provider_config::model_summary(
            active_provider_id,
            active_model_id,
            &self.provider_configuration,
        );
        let panel_route = self.active_workbench_openrouter_endpoint_route();
        let active_route = runtime_state.active_openrouter_endpoint_route.clone();
        let panel_model = self.active_workbench_model_choice();
        let panel_matches_active = panel_model.as_ref().is_some_and(|choice| {
            choice.provider_id == active_provider_id && choice.model_id == active_model_id
        });
        if panel_matches_active && panel_route == active_route {
            return None;
        }

        let mut current_parts = vec![active_model_label];
        if active_provider_id == "openrouter" {
            current_parts.push(
                active_route
                    .map(|route| route.display_label())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| String::from("Auto")),
            );
        } else if !active_provider_label.trim().is_empty() {
            current_parts.push(active_provider_label);
        }

        Some(format!(
            "新配置将在下一轮生效\n当前仍使用：{}",
            current_parts.join(" · ")
        ))
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

    /// Returns the current visible virtualized range for the model-picker result list, including one fixed overscan window.
    /// 返回当前模型选择结果列表的可见虚拟化范围，并额外保留一段固定预加载窗口。
    pub(super) fn visible_workbench_model_picker_range(
        &self,
        total_items: usize,
    ) -> (usize, usize, f32, f32) {
        if total_items == 0 {
            return (0, 0, 0.0, 0.0);
        }

        let row_extent = WORKBENCH_MODEL_PICKER_ROW_HEIGHT + WORKBENCH_MODEL_PICKER_ROW_GAP;
        let total_height = total_items as f32 * WORKBENCH_MODEL_PICKER_ROW_HEIGHT
            + total_items.saturating_sub(1) as f32 * WORKBENCH_MODEL_PICKER_ROW_GAP;
        let viewport_height = self
            .workbench_model_picker_viewport_height
            .max(WORKBENCH_MODEL_PICKER_LIST_HEIGHT);
        let window_top = self.workbench_model_picker_scroll_offset.max(0.0);
        let window_bottom = window_top + viewport_height;
        let mut start = (window_top / row_extent).floor().max(0.0) as usize;
        let mut end = ((window_bottom / row_extent).ceil().max(0.0) as usize).min(total_items);

        start = start.saturating_sub(WORKBENCH_MODEL_PICKER_VIRTUAL_OVERSCAN_ROWS);
        end = end
            .saturating_add(WORKBENCH_MODEL_PICKER_VIRTUAL_OVERSCAN_ROWS)
            .min(total_items);
        if start >= end {
            end = (start + 1).min(total_items);
        }

        let top_spacer_height = start as f32 * row_extent;
        let visible_count = end.saturating_sub(start);
        let visible_height = visible_count as f32 * WORKBENCH_MODEL_PICKER_ROW_HEIGHT
            + visible_count.saturating_sub(1) as f32 * WORKBENCH_MODEL_PICKER_ROW_GAP;
        let bottom_spacer_height = (total_height - top_spacer_height - visible_height).max(0.0);

        (start, end, top_spacer_height, bottom_spacer_height)
    }

    /// Estimates the scroll offset needed to bring one highlighted model-picker row close to the viewport center.
    /// 估算将某个高亮模型选择行带到接近视口中心所需的滚动偏移量。
    pub(super) fn estimated_workbench_model_picker_scroll_offset(&self, index: usize) -> f32 {
        let row_extent = WORKBENCH_MODEL_PICKER_ROW_HEIGHT + WORKBENCH_MODEL_PICKER_ROW_GAP;
        let viewport_height = self
            .workbench_model_picker_viewport_height
            .max(WORKBENCH_MODEL_PICKER_LIST_HEIGHT);
        let row_top = index as f32 * row_extent;
        let centered = row_top - ((viewport_height - WORKBENCH_MODEL_PICKER_ROW_HEIGHT) / 2.0);

        centered.max(0.0)
    }

    /// Builds the scroll task that reveals one highlighted model-picker row inside the virtualized list.
    /// 构建将某个高亮模型选择行滚动到虚拟化列表可见区域的滚动任务。
    pub(super) fn scroll_workbench_model_picker_to_index(&self, index: usize) -> Task<Message> {
        let target_offset = self.estimated_workbench_model_picker_scroll_offset(index);

        operation::scroll_to(
            workbench_model_picker_scrollable_id(),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: target_offset,
            },
        )
    }

    /// Returns whether the requested quick-panel capability section currently has at least one usable execution path.
    /// 返回快捷能力面板中的指定能力分区当前是否至少存在一条可用执行路径。
    pub(super) fn workbench_ai_capability_is_available(
        &self,
        section: super::WorkbenchAiCapabilitySection,
    ) -> bool {
        let Some(capability_kind) = section.capability_kind() else {
            return false;
        };
        self.ai_tool_configuration
            .has_usable_targets_for_kind(capability_kind)
    }

    /// Returns whether the requested quick-panel capability section stays enabled for the current workbench send chain.
    /// 返回快捷能力面板中的指定能力分区当前是否对工作台发送链路保持启用。
    pub(super) fn workbench_ai_capability_is_enabled(
        &self,
        section: super::WorkbenchAiCapabilitySection,
    ) -> bool {
        let Some(capability_kind) = section.capability_kind() else {
            return false;
        };
        self.workbench_ai_capability_is_available(section)
            && !self
                .effective_workbench_disabled_ai_capability_kinds()
                .contains(&capability_kind)
    }

    /// Builds the runtime-disabled AI capability set after force-disabling every semantic capability that currently lacks at least one usable configured target.
    /// 构建运行时关闭的 AI 能力集合，并强制关闭当前缺少可用已配置目标的所有语义能力。
    pub(super) fn effective_workbench_disabled_ai_capability_kinds(
        &self,
    ) -> std::collections::BTreeSet<crate::provider_config::AiToolKind> {
        let mut disabled = self.workbench_disabled_ai_capability_kinds.clone();
        for section in super::WorkbenchAiCapabilitySection::ALL {
            let Some(capability_kind) = section.capability_kind() else {
                continue;
            };
            if !self
                .ai_tool_configuration
                .has_usable_targets_for_kind(capability_kind)
            {
                disabled.insert(capability_kind);
            }
        }
        disabled
    }

    /// Synchronizes the current workbench quick-toggle state back into the persisted standalone AI capability configuration snapshot.
    /// 将当前工作台快捷开关状态同步回独立 AI 能力配置持久化快照。
    fn sync_workbench_ai_capability_runtime_preferences_into_configuration(&mut self) {
        self.ai_tool_configuration.disabled_capability_kinds = self
            .effective_workbench_disabled_ai_capability_kinds()
            .into_iter()
            .filter(|kind| kind.supports_workbench_quick_toggle())
            .collect();
    }

    /// Reconciles the current-message AI capability toggles so every semantic capability without usable configured targets is force-closed until the user explicitly re-enables it later.
    /// 对当前消息级 AI 能力开关执行对齐：凡是缺少可用已配置目标的语义能力都会被强制关闭，后续需由用户显式重新开启。
    pub(super) fn reconcile_workbench_ai_capability_runtime_state(&mut self) {
        for section in super::WorkbenchAiCapabilitySection::ALL {
            let Some(capability_kind) = section.capability_kind() else {
                continue;
            };
            if self
                .ai_tool_configuration
                .has_usable_targets_for_kind(capability_kind)
            {
                continue;
            }
            self.workbench_disabled_ai_capability_kinds
                .insert(capability_kind);
            if capability_kind == crate::provider_config::AiToolKind::WebSearch {
                self.disable_workbench_web_search();
            }
        }
    }

    /// Re-enables unified web-search for the current workbench send chain using the best available non-disabled override mode.
    /// 使用当前最合适的非关闭覆盖模式，为当前工作台发送链路重新启用统一网页搜索。
    pub(super) fn enable_workbench_web_search(&mut self) {
        let native_available = self.active_workbench_has_native_web_search();
        let provider_chain_available = self
            .ai_tool_configuration
            .has_usable_targets_for_kind(crate::provider_config::AiToolKind::WebSearch);
        let (override_mode, provider_id) = if native_available && provider_chain_available {
            (PendingPromptWebSearchOverride::FollowGlobal, None)
        } else if native_available {
            (PendingPromptWebSearchOverride::NativeOnly, None)
        } else if provider_chain_available {
            (PendingPromptWebSearchOverride::ThirdPartyOnly, None)
        } else {
            (PendingPromptWebSearchOverride::Disabled, None)
        };
        self.workbench_web_search_override = override_mode;
        self.workbench_web_search_provider_id = provider_id;
    }

    /// Disables unified web-search for the current workbench send chain without mutating the persisted global route preference.
    /// 在不修改持久化全局路由偏好的前提下，为当前工作台发送链路关闭统一网页搜索。
    pub(super) fn disable_workbench_web_search(&mut self) {
        self.workbench_web_search_override = PendingPromptWebSearchOverride::Disabled;
        self.workbench_web_search_provider_id = None;
    }

    /// Updates one quick-panel semantic capability toggle without mutating the persisted target-enabled preferences.
    /// 更新一条快捷能力分区的语义能力开关，但不修改持久化目标启用偏好。
    pub(super) fn set_workbench_ai_capability_enabled(
        &mut self,
        section: super::WorkbenchAiCapabilitySection,
        enabled: bool,
    ) {
        let Some(capability_kind) = section.capability_kind() else {
            return;
        };
        if !self
            .ai_tool_configuration
            .has_usable_targets_for_kind(capability_kind)
        {
            self.workbench_disabled_ai_capability_kinds
                .insert(capability_kind);
            if capability_kind == crate::provider_config::AiToolKind::WebSearch {
                self.disable_workbench_web_search();
            }
            return;
        }
        if capability_kind == crate::provider_config::AiToolKind::WebSearch {
            if enabled {
                self.enable_workbench_web_search();
            } else {
                self.disable_workbench_web_search();
            }
            return;
        }

        if enabled {
            self.workbench_disabled_ai_capability_kinds
                .remove(&capability_kind);
        } else {
            self.workbench_disabled_ai_capability_kinds
                .insert(capability_kind);
        }
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

        for disabled_kind in &runtime_options.disabled_ai_capability_kinds {
            if *disabled_kind == crate::provider_config::AiToolKind::WebSearch {
                configuration.web_search.mode = WebSearchRoutingMode::Disabled;
                continue;
            }
            for vendor in &mut configuration.vendors {
                for binding in &mut vendor.targets {
                    if binding.capability_kind == *disabled_kind {
                        binding.enabled = false;
                    }
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
                openrouter_endpoint_route: (choice.provider_id == "openrouter")
                    .then(|| {
                        provider_config::openrouter_endpoint_route_for_model(
                            &self.provider_configuration,
                            &choice.model_id,
                        )
                    })
                    .flatten(),
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
            openrouter_endpoint_route: (provider_id == "openrouter")
                .then(|| {
                    provider_config::openrouter_endpoint_route_for_model(
                        &self.provider_configuration,
                        model_id,
                    )
                })
                .flatten(),
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
            disabled_ai_capability_kinds: self.effective_workbench_disabled_ai_capability_kinds(),
        }
    }

    /// Resolves the capability snapshot for a queued prompt's locked provider/model identity.
    /// 根据排队提示词锁定的供应商与模型身份解析能力快照。
    pub(super) fn resolve_pending_prompt_model_capabilities(
        &self,
        model_reference: &PendingPromptModelReference,
        runtime_options: &PendingPromptRuntimeOptions,
    ) -> Option<ResolvedModelCapabilities> {
        let definition = provider_config::supported_provider_by_id(&model_reference.provider_id)?;
        let model = provider_config::effective_model_choices(
            &model_reference.provider_id,
            &self.provider_configuration,
        )
        .into_iter()
        .find(|model| model.id == model_reference.model_id);
        let effective_configuration =
            self.effective_ai_tool_configuration_for_runtime_options(runtime_options);
        let mut capabilities = resolve_effective_model_capabilities(
            definition,
            model.as_ref(),
            &effective_configuration,
        );
        self.apply_runtime_ai_capability_capability_overrides(&mut capabilities, runtime_options);
        Some(capabilities)
    }

    /// Applies current-message AI capability disable toggles to one already resolved capability snapshot.
    /// 将当前消息级 AI 能力关闭开关应用到一份已经解析完成的能力快照。
    pub(super) fn apply_runtime_ai_capability_capability_overrides(
        &self,
        capabilities: &mut ResolvedModelCapabilities,
        runtime_options: &PendingPromptRuntimeOptions,
    ) {
        for disabled_kind in &runtime_options.disabled_ai_capability_kinds {
            capabilities
                .ai_tools
                .retain(|tool| tool.kind != *disabled_kind);
            match disabled_kind {
                crate::provider_config::AiToolKind::UnderstandImage => {
                    capabilities
                        .input_modalities
                        .retain(|modality| modality != "image");
                }
                _ => {}
            }
        }
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
            if model_reference.provider_id == "openrouter" {
                provider_config::set_openrouter_endpoint_route_for_model(
                    &mut config,
                    &model_reference.model_id,
                    model_reference.openrouter_endpoint_route.clone(),
                );
            }
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
        let catalog_models = self.current_provider_catalog_choices();
        let enabled_models = self.current_provider_model_choices();

        self.selected_provider_definition()
            .map(|_| {
                format!(
                    "使用内置目录，共 {} 个模型，当前启用 {} 个",
                    catalog_models.len(),
                    enabled_models.len()
                )
            })
            .unwrap_or_else(|| String::from("未找到供应商定义"))
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

        let is_new_entry = !self
            .provider_configuration
            .provider
            .contains_key(&definition.id);
        let entry = self
            .provider_configuration
            .provider
            .entry(definition.id.clone())
            .or_insert_with(PersistedProviderEntry::default);
        entry.name = Some(definition.name.clone());
        entry.options = self.provider_form.to_options();

        if is_new_entry {
            entry.set_enabled_models(Vec::<String>::new());
        }

        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Opens the blocking provider editor for a brand-new provider configuration draft.
    /// 为一条全新的供应商配置草稿打开阻塞式供应商编辑弹窗。
    pub(super) fn open_provider_editor_for_create(&mut self) {
        if let Some(choice) = provider_config::supported_provider_choices()
            .into_iter()
            .find(|choice| choice.id == self.selected_provider.id)
            .or_else(|| {
                provider_config::supported_provider_choices()
                    .into_iter()
                    .next()
            })
        {
            self.selected_provider = choice;
        }
        self.provider_form = ProviderFormState::default();
        self.provider_editor = Some(ProviderEditorState { is_new: true });
        self.provider_notice = String::from("正在创建新的模型供应商配置。");
    }

    /// Opens the blocking provider editor for one existing configured provider entry.
    /// 为一条已有供应商配置打开阻塞式供应商编辑弹窗。
    pub(super) fn open_provider_editor_for_edit(
        &mut self,
        provider_id: &str,
    ) -> Result<(), String> {
        let Some(choice) = provider_config::supported_provider_choices()
            .into_iter()
            .find(|choice| choice.id == provider_id)
        else {
            return Err(format!("未找到供应商 `{provider_id}` 的目录定义。"));
        };
        self.selected_provider = choice;
        self.reload_selected_provider_form();
        self.provider_editor = Some(ProviderEditorState { is_new: false });
        self.provider_notice = format!(
            "正在编辑供应商 `{}`。",
            provider_config::provider_summary(provider_id)
        );
        Ok(())
    }

    /// Opens the blocking enabled-model picker modal for one configured provider.
    /// 为一条已配置供应商打开阻塞式启用模型配置弹窗。
    pub(super) fn open_provider_model_picker(&mut self, provider_id: &str) -> Result<(), String> {
        if !self
            .provider_configuration
            .provider
            .contains_key(provider_id)
        {
            return Err(format!(
                "供应商 `{}` 尚未保存，无法配置模型。",
                provider_config::provider_summary(provider_id)
            ));
        }

        self.provider_model_picker = Some(ProviderModelPickerState {
            selected_provider_id: provider_id.to_string(),
            filter: String::new(),
            draft_enabled_model_ids_by_provider: self
                .configured_provider_choices()
                .into_iter()
                .map(|choice| {
                    (
                        choice.id.clone(),
                        provider_config::enabled_model_ids(
                            &choice.id,
                            &self.provider_configuration,
                        ),
                    )
                })
                .collect(),
        });
        self.provider_notice = format!(
            "正在配置 `{}` 的启用模型列表。",
            provider_config::provider_summary(provider_id)
        );
        Ok(())
    }

    /// Returns the configured provider choices rendered inside the blocking model-picker modal.
    /// 返回渲染在阻塞式模型配置弹窗中的已配置供应商选择项。
    pub(super) fn provider_model_picker_provider_choices(&self) -> Vec<ProviderChoice> {
        self.configured_provider_choices()
    }

    /// Returns the full filtered catalog shown in the blocking provider model-picker modal.
    /// 返回阻塞式供应商模型配置弹窗中展示的完整过滤后目录列表。
    pub(super) fn filtered_provider_model_picker_catalog(&self) -> Vec<ModelChoice> {
        let Some(state) = self.provider_model_picker.as_ref() else {
            return Vec::new();
        };
        let filter = state.filter.trim().to_ascii_lowercase();

        provider_config::catalog_model_choices(
            &state.selected_provider_id,
            &self.provider_configuration,
        )
        .into_iter()
        .filter(|model| {
            if filter.is_empty() {
                return true;
            }

            model.id.to_ascii_lowercase().contains(&filter)
                || model.name.to_ascii_lowercase().contains(&filter)
                || model
                    .family
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&filter)
        })
        .collect()
    }

    /// Persists the staged enabled-model draft from the blocking provider model-picker modal and reconciles the active workbench model when needed.
    /// 持久化阻塞式供应商模型配置弹窗中的启用模型草稿，并在需要时对齐当前工作台激活模型。
    pub(super) fn save_provider_model_picker(&mut self) -> Result<(), String> {
        let Some(state) = self.provider_model_picker.as_ref() else {
            return Err(String::from("当前没有可保存的模型配置草稿。"));
        };
        for (provider_id, draft_ids) in &state.draft_enabled_model_ids_by_provider {
            let Some(entry) = self.provider_configuration.provider.get_mut(provider_id) else {
                continue;
            };
            entry.set_enabled_models(draft_ids.iter().cloned().collect::<Vec<_>>());
        }

        if let Some((active_provider_id, active_model_id)) = self
            .provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .map(|(provider_id, model_id)| (provider_id.to_string(), model_id.to_string()))
        {
            let next_models = provider_config::effective_model_choices(
                &active_provider_id,
                &self.provider_configuration,
            );
            if !next_models.iter().any(|model| model.id == active_model_id) {
                self.provider_configuration.model = next_models
                    .first()
                    .map(|model| format!("{active_provider_id}/{}", model.id));
            }
            if next_models.is_empty() {
                self.provider_configuration.model = None;
            }
        }

        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Opens the blocking per-model metadata editor for one enabled model entry.
    /// 为一条已启用模型条目打开阻塞式逐模型元数据编辑弹窗。
    pub(super) fn open_provider_model_override_editor(
        &mut self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        let catalog =
            provider_config::catalog_model_choices(provider_id, &self.provider_configuration);
        let Some(model) = catalog.into_iter().find(|model| model.id == model_id) else {
            return Err(format!(
                "未找到供应商 `{}` 下的模型 `{model_id}`。",
                provider_config::provider_summary(provider_id)
            ));
        };
        let active_override = self
            .provider_configuration
            .provider
            .get(provider_id)
            .and_then(|entry| entry.model_overrides.get(model_id))
            .cloned()
            .unwrap_or_default();
        let active_input_modalities = if let Some(modalities) = active_override.input_modalities {
            modalities
        } else if model.input_modalities.is_empty() {
            vec![String::from("text")]
        } else {
            model.input_modalities.clone()
        };
        let reasoning_profile_text = active_override
            .reasoning_profile
            .clone()
            .or_else(|| model.reasoning_profile.clone())
            .map(|profile| serde_json::to_string_pretty(&profile).unwrap_or_default())
            .unwrap_or_default();

        self.provider_model_override_editor = Some(ProviderModelOverrideEditorState {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            model_name: model.name,
            reasoning: active_override
                .reasoning
                .unwrap_or(model.reasoning.unwrap_or(false)),
            attachment: active_override
                .attachment
                .unwrap_or(model.attachment.unwrap_or(false)),
            supports_text: active_input_modalities
                .iter()
                .any(|modality| modality.eq_ignore_ascii_case("text")),
            supports_image: active_input_modalities
                .iter()
                .any(|modality| modality.eq_ignore_ascii_case("image")),
            supports_audio: active_input_modalities
                .iter()
                .any(|modality| modality.eq_ignore_ascii_case("audio")),
            supports_video: active_input_modalities
                .iter()
                .any(|modality| modality.eq_ignore_ascii_case("video")),
            supports_pdf: active_input_modalities
                .iter()
                .any(|modality| modality.eq_ignore_ascii_case("pdf")),
            context_limit_text: active_override
                .context_limit
                .or(model.context_limit)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            input_limit_text: active_override
                .input_limit
                .or(model.input_limit)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            output_limit_text: active_override
                .output_limit
                .or(model.output_limit)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            reasoning_profile_buffer: text_editor::Content::with_text(&reasoning_profile_text),
        });
        self.provider_notice = format!(
            "正在编辑 `{}` 的模型元数据。",
            provider_config::provider_summary(provider_id)
        );
        Ok(())
    }

    /// Persists the staged per-model override metadata back into the provider configuration file.
    /// 将暂存的逐模型覆盖元数据持久化回供应商配置文件。
    pub(super) fn save_provider_model_override_editor(&mut self) -> Result<(), String> {
        let Some(editor) = self.provider_model_override_editor.as_ref() else {
            return Err(String::from("当前没有可保存的模型元数据草稿。"));
        };

        let context_limit = parse_optional_usize_field(&editor.context_limit_text, "上下文长度")?;
        let input_limit = parse_optional_usize_field(&editor.input_limit_text, "输入长度")?;
        let output_limit = parse_optional_usize_field(&editor.output_limit_text, "输出长度")?;
        let reasoning_profile_text = editor.reasoning_profile_buffer.text();
        let reasoning_profile = if reasoning_profile_text.trim().is_empty() {
            None
        } else {
            Some(
                serde_json::from_str(&reasoning_profile_text)
                    .map_err(|error| format!("思考模式 JSON 解析失败：{error}"))?,
            )
        };
        let input_modalities = build_model_override_input_modalities(editor);

        let Some(entry) = self
            .provider_configuration
            .provider
            .get_mut(&editor.provider_id)
        else {
            return Err(String::from("目标供应商配置不存在，无法保存模型元数据。"));
        };

        entry.model_overrides.insert(
            editor.model_id.clone(),
            PersistedModelOverrideEntry {
                reasoning: Some(editor.reasoning),
                attachment: Some(editor.attachment),
                context_limit,
                input_limit,
                output_limit,
                reasoning_profile,
                input_modalities: Some(input_modalities),
                output_modalities: None,
            },
        );

        provider_config::save_configuration_file(&self.provider_configuration)
    }

    /// Persists standalone AI capability configuration and refreshes the runtime foundation.
    /// 持久化独立 AI 能力配置并刷新运行时基础架构。
    pub(super) fn save_ai_tool_configuration(&mut self) -> Result<(), String> {
        self.reconcile_workbench_ai_capability_runtime_state();
        self.sync_workbench_ai_capability_runtime_preferences_into_configuration();
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

    /// Toggles one configured semantic capability target without removing its stored order or model override.
    /// 切换单条已配置语义能力目标的启用状态，同时保留其顺序与模型覆盖配置。
    pub(super) fn set_ai_capability_target_enabled(
        &mut self,
        capability_kind: AiToolKind,
        vendor_id: &str,
        target_alias: &str,
        enabled: bool,
    ) -> Result<bool, String> {
        let normalized_vendor_id = vendor_id.trim().to_ascii_lowercase();
        let normalized_target_alias = target_alias.trim().to_ascii_lowercase();
        if enabled
            && self
                .ai_tool_configuration
                .enabled_target_count_for_kind(capability_kind)
                >= provider_config::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
            && !self
                .ai_tool_configuration
                .vendors
                .iter()
                .filter(|vendor| vendor.vendor_id == normalized_vendor_id)
                .flat_map(|vendor| vendor.targets.iter())
                .any(|binding| {
                    binding.enabled
                        && binding.capability_kind == capability_kind
                        && binding
                            .target_alias()
                            .eq_ignore_ascii_case(&normalized_target_alias)
                })
        {
            return Err(format!(
                "“{}”最多只能启用 {} 条目标，请先取消部分已勾选项。",
                capability_kind.label(),
                provider_config::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
            ));
        }
        let Some(vendor) = self
            .ai_tool_configuration
            .vendors
            .iter_mut()
            .find(|vendor| vendor.vendor_id == normalized_vendor_id)
        else {
            return Ok(false);
        };
        let Some(binding) = vendor.targets.iter_mut().find(|binding| {
            binding.capability_kind == capability_kind
                && binding
                    .target_alias()
                    .eq_ignore_ascii_case(&normalized_target_alias)
        }) else {
            return Ok(false);
        };
        if binding.enabled == enabled {
            return Ok(false);
        }
        binding.enabled = enabled;
        self.ai_tool_configuration.normalize();
        Ok(true)
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

    /// Removes one specified MCP server entry from the shared Vulcan configuration file.
    /// 从共享 Vulcan 配置文件中移除指定的 MCP 服务器条目。
    pub(super) fn remove_mcp_server_configuration_by_id(
        &mut self,
        server_id: &str,
    ) -> Result<String, String> {
        let Some(removed_entry) = self.provider_configuration.mcp.servers.remove(server_id) else {
            return Err(String::from("目标 MCP 服务器不存在。"));
        };

        provider_config::save_configuration_file(&self.provider_configuration)?;
        if self.selected_mcp_server_id.as_deref() == Some(server_id) {
            self.selected_mcp_server_id =
                Self::resolve_initial_mcp_server_id(&self.provider_configuration);
        }

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
        let command_session_runtime = self.runtime_foundation.command_session_runtime();
        self.runtime_foundation = RuntimeFoundation::build_from_optional_mcp_access_with_runtimes(
            &self.provider_configuration,
            &self.agent_profile_catalog,
            self.luaskills_runtime.clone(),
            self.mcp_runtime.clone(),
            task_runtime,
            command_session_runtime,
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

/// Parses one optional positive integer field from an editor text buffer.
/// 从编辑器文本缓冲区解析一个可选正整数数值字段。
fn parse_optional_usize_field(value: &str, field_label: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    trimmed
        .parse::<usize>()
        .map(Some)
        .map_err(|error| format!("{field_label} 解析失败：{error}"))
}

/// Builds the normalized input modality override list from the staged model editor checkbox state.
/// 根据暂存模型编辑器勾选状态构建规范化输入模态覆盖列表。
fn build_model_override_input_modalities(editor: &ProviderModelOverrideEditorState) -> Vec<String> {
    let mut modalities = BTreeSet::new();
    if editor.supports_text {
        modalities.insert(String::from("text"));
    }
    if editor.supports_image {
        modalities.insert(String::from("image"));
    }
    if editor.supports_audio {
        modalities.insert(String::from("audio"));
    }
    if editor.supports_video {
        modalities.insert(String::from("video"));
    }
    if editor.supports_pdf {
        modalities.insert(String::from("pdf"));
    }
    modalities.into_iter().collect()
}
