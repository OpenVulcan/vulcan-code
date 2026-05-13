//! Provider, MCP, and model-selection support methods extracted from the main workbench module.
//! 从主工作台模块中抽取的供应商、MCP 与模型选择支撑方法。

use super::helpers::*;
use super::*;

impl VulcanWorkbench {
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
        std::iter::once(AgentChoice {
            id: String::from(SYSTEM_DEFAULT_AGENT_CHOICE_ID),
            label: String::from("系统默认（按模型）"),
        })
        .chain(
            self.agent_profile_catalog
                .agents
                .values()
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

        self.configured_agent_choices()
            .into_iter()
            .find(|choice| choice.id == active_agent_id)
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

    /// Applies one chosen workbench model to the persisted global runtime selection and closes the picker.
    /// 将某个工作台模型应用到持久化全局运行时选择并关闭选择弹窗。
    pub(super) fn apply_workbench_model_choice(
        &mut self,
        choice: WorkbenchModelChoice,
    ) -> Task<Message> {
        self.provider_configuration.model =
            Some(format!("{}/{}", choice.provider_id, choice.model_id));
        if let Some(provider_choice) = provider_config::supported_provider_choices()
            .into_iter()
            .find(|provider_choice| provider_choice.id == choice.provider_id)
        {
            self.selected_provider = provider_choice;
            self.reload_selected_provider_form();
        }

        if let Err(error) = provider_config::save_configuration_file(&self.provider_configuration) {
            self.provider_notice = error.clone();
            self.status_message = error;
            return Task::none();
        }

        self.refresh_runtime_foundation();
        self.workbench_model_picker_open = false;
        self.workbench_model_picker_filter.clear();
        self.workbench_model_picker_highlighted_index = None;
        self.status_message = format!(
            "当前模型已切换为 `{}`（{}）。",
            choice.model_name, choice.provider_label
        );
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
        self.runtime_foundation = RuntimeFoundation::build_from_runtime_handles(
            &self.provider_configuration,
            &self.agent_profile_catalog,
            self.luaskills_runtime.clone(),
            self.mcp_runtime.clone(),
        );
        self.last_execution_preparation = None;
        self.last_mcp_discovery_rows.clear();
        self.last_mcp_discovery_errors.clear();
    }

    /// Restarts the long-lived MCP runtime after configuration changes and rebuilds the runtime foundation immediately.
    /// 在配置变更后重启长期 MCP 运行时，并立即重建运行时基础架构。
    pub(super) fn restart_mcp_runtime_task(&mut self) -> Task<Message> {
        if let Some(handle) = self.mcp_runtime.take() {
            handle.shutdown();
        }
        self.refresh_runtime_foundation();
        Task::perform(
            bootstrap_mcp_runtime(self.provider_configuration.clone()),
            Message::McpRuntimeBootstrapped,
        )
    }
}
