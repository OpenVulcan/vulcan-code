//! Host runtime foundation assembled from provider, tool, skill, and MCP configuration.
//! 从供应商、工具、技能与 MCP 配置组装出的宿主运行时基础架构。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeSkillKind,
    RuntimeToolPolicyDefinition,
};
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::luaskills_runtime::{
    LuaSkillsRuntimeHandle, list_luaskills_tool_entries, luaskills_entry_args_schema,
    render_luaskills_entry_help_summary,
};
use crate::mcp::{
    McpRuntimeAccess, McpRuntimeOverview, McpRuntimeToolAllowlist, McpRuntimeToolBinding,
    configured_mcp_runtime_access,
};
use crate::provider_config::{PersistedLuaSkillsConfiguration, ProviderConfigurationFile};
use crate::tooling::native::{
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use crate::tooling::{CommandSessionRuntime, NativeToolCatalog, build_native_tool_catalog};
use luaskills::RuntimeEntryDescriptor;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vulcan_luaskills_prompt::{
    LuaSkillPromptEntry, render_luaskill_argument_summary, render_luaskill_schema_markdown,
};
use vulcan_plugin_runtime::{
    SystemPluginCatalogSnapshot, SystemPluginRuntimeManager, SystemPluginToolDescriptor,
};

/// High-level runtime summary exposed to UI placeholder surfaces.
/// 暴露给 UI 占位面板的高层运行时摘要。
#[derive(Debug, Clone)]
pub struct RuntimeFoundationSummary {
    /// Active agent identifier.
    /// 激活代理标识。
    pub active_agent_id: String,
    /// Active agent display name.
    /// 激活代理展示名称。
    pub active_agent_name: String,
    /// Active model reference.
    /// 激活模型引用。
    pub active_model: String,
    /// Declared agent count.
    /// 已声明代理数量。
    pub configured_agent_count: usize,
    /// Built agent count.
    /// 已构建代理数量。
    pub built_agent_count: usize,
    /// Built coordinator count.
    /// 已构建协调代理数量。
    pub coordinator_count: usize,
    /// Registered skill count.
    /// 已注册技能数量。
    pub skill_count: usize,
    /// Registered native tool count.
    /// 已注册原生工具数量。
    pub native_tool_count: usize,
    /// Registered System Plugin tool count.
    /// 已注册系统插件工具数量。
    pub system_plugin_tool_count: usize,
    /// Prepared delegate tool count.
    /// 已准备好的委派工具数量。
    pub delegate_tool_count: usize,
    /// Configured MCP server count.
    /// 已配置的 MCP 服务器数量。
    pub configured_mcp_server_count: usize,
    /// Enabled MCP server count.
    /// 已启用的 MCP 服务器数量。
    pub enabled_mcp_server_count: usize,
    /// Build warnings accumulated while materialising the foundation.
    /// 在构建基础架构时累计的警告集合。
    pub warnings: Vec<String>,
    /// Token budget runtime initialization status.
    /// Token 预算运行时初始化状态。
    pub token_budget_status: String,
}

/// In-memory host runtime foundation built from the shared Vulcan configuration file.
/// 根据共享 Vulcan 配置文件构建的内存态宿主运行时基础架构。
pub struct RuntimeFoundation {
    /// Runtime-facing aggregate configuration.
    /// 面向运行时的聚合配置。
    config: AgentRuntimeConfig,
    /// Registered native tool catalog.
    /// 已注册的原生工具目录。
    native_tools: NativeToolCatalog,
    /// Long-lived generic System Plugin runtime shared across foundation refreshes.
    /// 跨基础架构刷新共享的长生命周期通用系统插件运行时。
    system_plugin_runtime: Option<SystemPluginRuntimeManager>,
    /// Immutable System Plugin catalog revision captured for this foundation.
    /// 当前基础架构捕获的不可变系统插件目录修订版。
    system_plugin_catalog: Option<Arc<SystemPluginCatalogSnapshot>>,
    /// LuaSkills-backed runtime tool descriptors exposed to the host-controlled provider execution flows.
    /// 暴露给宿主控制 Provider 执行链的 LuaSkills 运行时工具描述集合。
    luaskills_tools: Vec<RuntimeEntryDescriptor>,
    /// LuaSkills-backed runtime tool descriptors allowed through the host-side dispatcher.
    /// 允许通过宿主侧调度器调用的 LuaSkills 运行时工具描述集合。
    luaskills_dispatch_tools: Vec<RuntimeEntryDescriptor>,
    /// Prompt entries for LuaSkills allowed through implicit host dispatch.
    /// 允许通过隐式宿主调度访问的 LuaSkills 提示词入口集合。
    implicit_luaskill_prompt_entries: Vec<LuaSkillPromptEntry>,
    /// Unified MCP runtime access surface that hides live-versus-configured source selection.
    /// 隐藏实时与配置来源选择细节的统一 MCP 运行时访问面。
    mcp_runtime_access: McpRuntimeAccess,
    /// Build warnings collected while materialising the foundation.
    /// 在组装基础架构时收集到的构建警告。
    warnings: Vec<String>,
    /// Shared resumable task runtime reused across turns and child task chains.
    /// 在多轮执行与子任务链之间复用的共享可恢复任务运行时。
    task_runtime: Arc<TaskRuntime>,
    /// Shared structured-question runtime reused across turns so ask tool calls can survive runtime refreshes while still waiting for WebUI input.
    /// 在多轮执行之间复用的共享结构化问题运行时，使 ask 工具在等待WebUI输入期间也能穿越运行时刷新继续存活。
    question_runtime: Arc<QuestionRuntime>,
    /// Shared command-session runtime reused across turns so long-lived shell sessions can survive execution refreshes.
    /// 在多轮执行之间复用的共享命令会话运行时，使长生命周期 shell 会话能够穿越执行刷新继续存活。
    command_session_runtime: Arc<CommandSessionRuntime>,
    /// Shared token budget runtime reused by context budget hot paths.
    /// 被上下文预算热路径复用的共享 Token 预算运行时。
    token_budget_runtime: Arc<TokenBudgetRuntime>,
}

impl RuntimeFoundation {
    /// Builds the host runtime foundation directly from shared configuration plus one optional stable MCP access surface.
    /// 直接根据共享配置与一个可选稳定 MCP 访问面构建宿主运行时基础架构。
    pub fn build_from_optional_mcp_access(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        mcp_runtime_access: Option<McpRuntimeAccess>,
    ) -> Self {
        Self::build(
            config,
            agent_profile_catalog,
            luaskills_runtime,
            mcp_runtime_access.unwrap_or_else(|| configured_mcp_runtime_access(config)),
        )
    }

    /// Builds the host runtime foundation while reusing existing shared task and command-session runtimes so child tasks and shell sessions stay controllable across runtime refreshes.
    /// 构建宿主运行时基础架构时复用既有共享任务运行时与命令会话运行时，确保子任务与 shell 会话在运行时刷新后仍可被控制。
    pub fn build_from_optional_mcp_access_with_runtimes(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        mcp_runtime_access: Option<McpRuntimeAccess>,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
    ) -> Self {
        Self::build_from_optional_mcp_access_with_runtimes_and_plugins(
            config,
            agent_profile_catalog,
            luaskills_runtime,
            None,
            mcp_runtime_access,
            task_runtime,
            question_runtime,
            command_session_runtime,
            token_budget_runtime,
        )
    }

    /// Builds the host runtime foundation with independent LuaSkills and System Plugin runtimes.
    /// 使用相互独立的 LuaSkills 与系统插件运行时构建宿主运行时基础架构。
    pub fn build_from_optional_mcp_access_with_runtimes_and_plugins(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        system_plugin_runtime: Option<SystemPluginRuntimeManager>,
        mcp_runtime_access: Option<McpRuntimeAccess>,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
    ) -> Self {
        Self::build_with_task_runtime(
            config,
            agent_profile_catalog,
            luaskills_runtime,
            system_plugin_runtime,
            mcp_runtime_access.unwrap_or_else(|| configured_mcp_runtime_access(config)),
            task_runtime,
            question_runtime,
            command_session_runtime,
            token_budget_runtime,
        )
    }

    /// Builds the host runtime foundation from the shared provider configuration file.
    /// 根据共享供应商配置文件构建宿主运行时基础架构。
    pub fn build(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        mcp_runtime_access: McpRuntimeAccess,
    ) -> Self {
        Self::build_with_task_runtime(
            config,
            agent_profile_catalog,
            luaskills_runtime,
            None,
            mcp_runtime_access,
            Arc::new(TaskRuntime::new()),
            Arc::new(QuestionRuntime::new()),
            Arc::new(CommandSessionRuntime::new()),
            Arc::new(TokenBudgetRuntime::pending()),
        )
    }

    /// Builds the host runtime foundation with an explicitly provided shared task runtime instance.
    /// 使用显式提供的共享任务运行时实例构建宿主运行时基础架构。
    fn build_with_task_runtime(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        system_plugin_runtime: Option<SystemPluginRuntimeManager>,
        mcp_runtime_access: McpRuntimeAccess,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
    ) -> Self {
        let mut runtime_config = AgentRuntimeConfig::from_sources(config, agent_profile_catalog);
        let mut subagent_catalog = vulcan_task_session::load_subagent_catalog_from_roots(
            vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
            crate::data_plane::application_data_root().join("agents"),
        );
        let native_tools = build_native_tool_catalog(
            &runtime_config,
            config.default_command_shell.as_deref(),
            Arc::clone(&command_session_runtime),
            config.vmm.clone(),
            system_plugin_runtime.clone(),
        );
        let mut warnings = subagent_catalog
            .errors
            .iter()
            .map(|error| {
                format!(
                    "加载子代理定义 `{}` 失败：{}",
                    error.path.display(),
                    error.message
                )
            })
            .collect::<Vec<_>>();
        // Capture the manager's already-published immutable generation; service lifecycle owns filesystem refreshes.
        // 捕获管理器已经发布的不可变代次；文件系统刷新由服务生命周期负责。
        let system_plugin_catalog = system_plugin_runtime
            .as_ref()
            .map(SystemPluginRuntimeManager::snapshot);
        if let Some(catalog) = system_plugin_catalog.as_ref()
            && let Err(error) =
                subagent_catalog.merge_plugin_definitions(catalog.enabled_agents.values().cloned())
        {
            warnings.push(format!("合并插件智能体目录失败：{error}"));
        }
        if let Err(error) = task_runtime.replace_subagent_catalog(subagent_catalog) {
            warnings.push(error);
        }
        if let Some(catalog) = system_plugin_catalog.as_ref() {
            let plugin_tool_ids = catalog
                .plugins
                .values()
                .flat_map(|plugin| plugin.tools.iter().map(|tool| tool.id.clone()))
                .collect::<BTreeSet<_>>();
            migrate_legacy_plugin_tool_sources(&mut runtime_config, &plugin_tool_ids);
        }
        let (luaskills_tools, luaskills_dispatch_tools, implicit_luaskill_prompt_entries) =
            match luaskills_runtime.as_ref() {
                Some(handle) => match list_luaskills_tool_entries(handle) {
                    Ok(entries) => {
                        let explicit_entries = filter_luaskills_entries_by_explicit_registration(
                            entries.clone(),
                            &config.luaskills,
                        );
                        let dispatch_entries =
                            filter_luaskills_entries_by_host_dispatch(entries, &config.luaskills);
                        let prompt_entries =
                            build_luaskill_prompt_entries(handle, &dispatch_entries, &mut warnings);
                        (explicit_entries, dispatch_entries, prompt_entries)
                    }
                    Err(error) => {
                        warnings.push(format!("加载 LuaSkills 工具表失败：{error}"));
                        (Vec::new(), Vec::new(), Vec::new())
                    }
                },
                None => (Vec::new(), Vec::new(), Vec::new()),
            };
        Self {
            config: runtime_config,
            native_tools,
            system_plugin_runtime,
            system_plugin_catalog,
            luaskills_tools,
            luaskills_dispatch_tools,
            implicit_luaskill_prompt_entries,
            mcp_runtime_access,
            warnings,
            task_runtime,
            question_runtime,
            command_session_runtime,
            token_budget_runtime,
        }
    }

    /// Returns the current runtime summary for UI placeholder surfaces and diagnostics.
    /// 返回供 UI 占位面板与诊断区域使用的当前运行时摘要。
    pub fn summary(&self) -> RuntimeFoundationSummary {
        let active_agent = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == self.config.active_agent_id)
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| String::from("未解析到激活代理"));

        let coordinator_count = self
            .config
            .agents
            .iter()
            .filter(|definition| definition.kind == RuntimeAgentKind::Coordinator)
            .count();
        let mcp_overview = self.mcp_runtime_access.overview();

        RuntimeFoundationSummary {
            active_agent_id: self.config.active_agent_id.clone(),
            active_agent_name: active_agent,
            active_model: self
                .config
                .active_model
                .clone()
                .unwrap_or_else(|| String::from("未配置模型")),
            configured_agent_count: self.config.agents.len(),
            built_agent_count: self.config.agents.len(),
            coordinator_count,
            skill_count: self.config.skills.len(),
            native_tool_count: self.native_tools.descriptors.len(),
            system_plugin_tool_count: self
                .system_plugin_catalog
                .as_ref()
                .map_or(0, |catalog| catalog.enabled_tools.len()),
            delegate_tool_count: self
                .config
                .agents
                .iter()
                .map(|definition| definition.delegates.len())
                .sum(),
            configured_mcp_server_count: mcp_overview.summary_rows().len(),
            enabled_mcp_server_count: mcp_overview.active_summary_rows().len(),
            warnings: self.warnings.clone(),
            token_budget_status: self.token_budget_runtime.status_row(),
        }
    }

    /// Returns lightweight agent rows used by placeholder UI surfaces.
    /// 返回供占位 UI 表面使用的轻量代理行文本。
    pub fn agent_rows(&self) -> Vec<String> {
        self.config
            .agents
            .iter()
            .map(|definition| {
                format!(
                    "{} · {} · {} 技能 · {} 委派 · {}",
                    definition.name,
                    definition.kind.label(),
                    definition.skill_ids.len(),
                    definition.delegates.len(),
                    definition
                        .tool_policy
                        .clone()
                        .unwrap_or_else(|| String::from("无策略"))
                )
            })
            .collect()
    }

    /// Returns lightweight skill rows filtered by skill family.
    /// 返回按技能类型过滤后的轻量技能行文本。
    pub fn skill_rows(&self, kind: RuntimeSkillKind) -> Vec<String> {
        self.config
            .skills
            .iter()
            .filter(|definition| definition.kind == kind)
            .map(|definition| format!("{} · {}", definition.name, definition.id))
            .collect()
    }

    /// Returns lightweight native-tool rows for UI placeholder surfaces.
    /// 返回供 UI 占位面板使用的轻量原生工具行文本。
    pub fn native_tool_rows(&self) -> Vec<String> {
        self.native_tools
            .descriptors
            .iter()
            .map(|descriptor| format!("{} · {}", descriptor.name, descriptor.description))
            .collect()
    }

    /// Returns the shared native tool catalog used by the current runtime foundation.
    /// 返回当前运行时基础架构使用的共享原生工具目录。
    pub fn native_tool_catalog(&self) -> &NativeToolCatalog {
        &self.native_tools
    }

    /// Returns the long-lived generic System Plugin runtime when LuaSkills is available.
    /// 在 LuaSkills 可用时返回长生命周期通用系统插件运行时。
    pub fn system_plugin_runtime(&self) -> Option<SystemPluginRuntimeManager> {
        self.system_plugin_runtime.clone()
    }

    /// Returns the immutable System Plugin catalog captured by this foundation.
    /// 返回当前基础架构捕获的不可变系统插件目录。
    pub fn system_plugin_catalog(&self) -> Option<Arc<SystemPluginCatalogSnapshot>> {
        self.system_plugin_catalog.clone()
    }

    /// Returns the shared resumable task runtime used by the current foundation.
    /// 返回当前基础架构使用的共享可恢复任务运行时。
    pub fn task_runtime(&self) -> Arc<TaskRuntime> {
        Arc::clone(&self.task_runtime)
    }

    /// Returns the shared structured-question runtime used by the current foundation.
    /// 返回当前基础架构使用的共享结构化问题运行时。
    pub fn question_runtime(&self) -> Arc<QuestionRuntime> {
        Arc::clone(&self.question_runtime)
    }

    /// Returns the shared command-session runtime used by the current foundation.
    /// 返回当前基础架构使用的共享命令会话运行时。
    pub fn command_session_runtime(&self) -> Arc<CommandSessionRuntime> {
        Arc::clone(&self.command_session_runtime)
    }

    /// Returns the shared token budget runtime used by the current foundation.
    /// 返回当前基础架构使用的共享 Token 预算运行时。
    pub fn token_budget_runtime(&self) -> Arc<TokenBudgetRuntime> {
        Arc::clone(&self.token_budget_runtime)
    }

    /// Returns lightweight tool-policy rows for UI placeholder surfaces.
    /// 返回供 UI 占位面板使用的轻量工具策略行文本。
    pub fn tool_policy_rows(&self) -> Vec<String> {
        self.config
            .tool_policies
            .values()
            .map(|policy| {
                format!(
                    "{} · {} · 原生 {} · 系统插件 {} · MCP 服务器 {} · MCP 工具 {}",
                    policy.id,
                    policy.name,
                    policy.native_tools.len(),
                    policy.plugin_tools.len(),
                    policy.mcp_servers.len(),
                    policy.mcp_tools.len()
                )
            })
            .collect()
    }

    /// Returns the runtime definition of the currently active agent.
    /// 返回当前激活代理的运行时定义。
    pub fn active_agent_definition(&self) -> Option<&RuntimeAgentDefinition> {
        self.agent_definition(&self.config.active_agent_id)
    }

    /// Returns the runtime definition for one agent identifier.
    /// 返回指定代理标识对应的运行时定义。
    pub fn agent_definition(&self, agent_id: &str) -> Option<&RuntimeAgentDefinition> {
        self.config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
    }

    /// Returns whether one agent identifier has already been built into the runtime foundation.
    /// 返回某个代理标识当前是否已经被构建进运行时基础架构。
    pub fn has_built_agent(&self, agent_id: &str) -> bool {
        self.config
            .agents
            .iter()
            .any(|definition| definition.id == agent_id)
    }

    /// Returns human-readable skill names bound to one agent identifier.
    /// 返回绑定到某个代理标识上的人类可读技能名称集合。
    pub fn skill_names_for_agent(&self, agent_id: &str) -> Vec<String> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };

        definition
            .skill_ids
            .iter()
            .filter_map(|skill_id| {
                self.config
                    .skills
                    .iter()
                    .find(|skill| skill.id == *skill_id)
                    .map(|skill| skill.name.clone())
            })
            .collect()
    }

    /// Resolves the native tool identifiers currently visible to one agent.
    /// 解析当前对某个代理可见的原生工具标识集合。
    pub fn resolved_native_tools_for_agent(&self, agent_id: &str) -> Vec<String> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };
        let available_native_tool_ids = self
            .native_tools
            .descriptors
            .iter()
            .map(|descriptor| descriptor.id.clone())
            .collect::<BTreeSet<_>>();
        let mut warnings = Vec::new();

        resolve_agent_native_tools(
            definition,
            &self.config.tool_policies,
            &available_native_tool_ids,
            &mut warnings,
        )
    }

    /// Resolves complete System Plugin tool groups currently visible to one agent.
    /// 解析当前对某个代理完整可见的系统插件工具组。
    pub fn resolved_system_plugin_tools_for_agent(
        &self,
        agent_id: &str,
    ) -> Vec<SystemPluginToolDescriptor> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };
        let Some(catalog) = self.system_plugin_catalog.as_ref() else {
            return Vec::new();
        };
        let available_tool_ids = catalog
            .enabled_tools
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let selected_tool_ids = resolve_agent_system_plugin_tool_ids(
            definition,
            &self.config.tool_policies,
            &available_tool_ids,
        )
        .into_iter()
        .collect::<BTreeSet<_>>();
        let mut groups = BTreeMap::<(String, String), Vec<SystemPluginToolDescriptor>>::new();
        for tool in catalog.enabled_tools.values() {
            groups
                .entry((tool.plugin_id.clone(), tool.group.clone()))
                .or_default()
                .push(tool.clone());
        }
        groups
            .into_values()
            .filter(|group| {
                group
                    .iter()
                    .all(|tool| selected_tool_ids.contains(&tool.id))
            })
            .flatten()
            .collect()
    }

    /// Resolves the LuaSkills runtime tools currently visible to one agent.
    /// 解析当前对某个代理可见的 LuaSkills 运行时工具集合。
    pub fn resolved_luaskills_tools_for_agent(
        &self,
        agent_id: &str,
    ) -> Vec<RuntimeEntryDescriptor> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };

        let _ = definition;
        self.luaskills_tools.clone()
    }

    /// Resolves the LuaSkills runtime tools callable through the host-side dispatcher for one agent.
    /// 解析当前允许某个代理通过宿主侧调度器调用的 LuaSkills 运行时工具集合。
    pub fn resolved_luaskills_dispatch_tools_for_agent(
        &self,
        agent_id: &str,
    ) -> Vec<RuntimeEntryDescriptor> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };

        let _ = definition;
        self.luaskills_dispatch_tools.clone()
    }

    /// Resolves prompt entries for LuaSkills callable through the host-side dispatcher.
    /// 解析可通过宿主侧调度器调用的 LuaSkills 提示词入口。
    pub fn resolved_luaskills_prompt_entries_for_agent(
        &self,
        agent_id: &str,
    ) -> Vec<LuaSkillPromptEntry> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };

        let _ = definition;
        self.implicit_luaskill_prompt_entries.clone()
    }

    /// Resolves the MCP runtime tools currently visible to one agent.
    /// 解析当前对某个代理可见的 MCP 运行时工具集合。
    pub fn resolved_mcp_tools_for_agent(&self, agent_id: &str) -> Vec<McpRuntimeToolBinding> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };

        let policy = definition
            .tool_policy
            .as_ref()
            .and_then(|policy_id| self.config.tool_policies.get(policy_id));
        let tool_allowlist = resolve_agent_mcp_tool_allowlist(definition, policy);

        self.mcp_runtime_access
            .available_tool_bindings_filtered(&tool_allowlist)
    }

    /// Returns one stable MCP runtime overview that narrows summary and discovery reads to one access surface.
    /// 返回一份将摘要与发现读取收敛到单一访问面的稳定 MCP 运行时概览。
    pub fn mcp_overview(&self) -> McpRuntimeOverview {
        self.mcp_runtime_access.overview()
    }
}

/// Resolves the native tool whitelist for one agent definition.
/// 为单个代理定义解析原生工具白名单。
fn resolve_agent_native_tools(
    definition: &RuntimeAgentDefinition,
    tool_policies: &BTreeMap<String, RuntimeToolPolicyDefinition>,
    available_native_tool_ids: &BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let mut names = if !definition.native_tools.is_empty() {
        definition.native_tools.clone()
    } else if let Some(policy_id) = &definition.tool_policy {
        if let Some(policy) = tool_policies.get(policy_id) {
            if policy.allow_all_native {
                available_native_tool_ids.iter().cloned().collect()
            } else {
                policy.native_tools.clone()
            }
        } else {
            warnings.push(format!(
                "代理 `{}` 引用了不存在的工具策略 `{}`，因此不会暴露该策略下的受限原生工具，仅保留通用原生工具。",
                definition.name, policy_id
            ));
            Vec::new()
        }
    } else {
        available_native_tool_ids.iter().cloned().collect()
    };

    names.retain(|name| {
        let is_available = available_native_tool_ids.contains(name);
        if !is_available {
            warnings.push(format!(
                "代理 `{}` 引用了未注册的原生工具 `{}`。",
                definition.name, name
            ));
        }
        is_available
    });
    append_universal_native_tools(&mut names, available_native_tool_ids);
    names
}

/// Resolves the System Plugin tool whitelist for one agent definition.
/// 为单个代理定义解析系统插件工具白名单。
fn resolve_agent_system_plugin_tool_ids(
    definition: &RuntimeAgentDefinition,
    tool_policies: &BTreeMap<String, RuntimeToolPolicyDefinition>,
    available_tool_ids: &BTreeSet<String>,
) -> Vec<String> {
    let mut names = if !definition.plugin_tools.is_empty() {
        definition.plugin_tools.clone()
    } else if let Some(policy_id) = &definition.tool_policy {
        match tool_policies.get(policy_id) {
            Some(policy) if policy.allow_all_plugin_tools => {
                available_tool_ids.iter().cloned().collect()
            }
            Some(policy) => policy.plugin_tools.clone(),
            None => Vec::new(),
        }
    } else {
        available_tool_ids.iter().cloned().collect()
    };
    names.retain(|name| available_tool_ids.contains(name));
    names.sort();
    names.dedup();
    names
}

/// Migrates legacy native-tool references into the dedicated System Plugin dimension in memory.
/// 在内存中将旧版原生工具引用迁移到专用系统插件维度。
fn migrate_legacy_plugin_tool_sources(
    runtime_config: &mut AgentRuntimeConfig,
    plugin_tool_ids: &BTreeSet<String>,
) {
    for policy in runtime_config.tool_policies.values_mut() {
        move_matching_tool_ids(
            &mut policy.native_tools,
            &mut policy.plugin_tools,
            plugin_tool_ids,
        );
    }
    for agent in &mut runtime_config.agents {
        move_matching_tool_ids(
            &mut agent.native_tools,
            &mut agent.plugin_tools,
            plugin_tool_ids,
        );
    }
}

/// Moves exact matching tool identifiers between two policy dimensions without duplication.
/// 在两个策略维度之间移动精确匹配的工具标识，同时避免重复。
fn move_matching_tool_ids(
    source: &mut Vec<String>,
    destination: &mut Vec<String>,
    matching_ids: &BTreeSet<String>,
) {
    let mut destination_ids = destination.iter().cloned().collect::<BTreeSet<_>>();
    let mut retained = Vec::with_capacity(source.len());
    for tool_id in std::mem::take(source) {
        if matching_ids.contains(&tool_id) {
            destination_ids.insert(tool_id);
        } else {
            retained.push(tool_id);
        }
    }
    *source = retained;
    *destination = destination_ids.into_iter().collect();
}

/// Appends native tools that must remain globally available outside regular policy limits.
/// 追加必须在普通策略限制之外保持全局可用的原生工具。
///
/// # Parameters
/// 参数
/// `names` is the resolved native tool list that will be exposed to one agent.
/// `names` 是即将暴露给单个代理的已解析原生工具列表。
/// `available_native_tool_ids` is the registered native tool id set.
/// `available_native_tool_ids` 是已注册的原生工具标识集合。
fn append_universal_native_tools(
    names: &mut Vec<String>,
    available_native_tool_ids: &BTreeSet<String>,
) {
    for tool_id in [
        TODOLIST_TOOL_ID,
        GET_GOAL_TOOL_ID,
        CREATE_GOAL_TOOL_ID,
        UPDATE_GOAL_TOOL_ID,
    ] {
        if available_native_tool_ids.contains(tool_id) && !names.iter().any(|name| name == tool_id)
        {
            names.push(String::from(tool_id));
        }
    }
}

/// Resolves the MCP runtime tool allowlist visible to one agent definition.
/// 解析对单个代理定义可见的 MCP 运行时工具白名单。
fn resolve_agent_mcp_tool_allowlist(
    definition: &RuntimeAgentDefinition,
    policy: Option<&RuntimeToolPolicyDefinition>,
) -> McpRuntimeToolAllowlist {
    let allowed_server_ids =
        policy.and_then(|policy| optional_non_empty_string_set(&policy.mcp_servers));
    let allowed_tool_names = if definition.mcp_tools.is_empty() {
        policy.and_then(|policy| optional_non_empty_string_set(&policy.mcp_tools))
    } else {
        optional_non_empty_string_set(&definition.mcp_tools)
    };

    McpRuntimeToolAllowlist::new(allowed_server_ids, allowed_tool_names)
}

/// Converts one string slice into an optional non-empty set.
/// 将一组字符串切片转换为可选的非空集合。
fn optional_non_empty_string_set(values: &[String]) -> Option<BTreeSet<String>> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().cloned().collect())
    }
}

/// Filters LuaSkills runtime entries so only explicitly registered tools enter provider-visible schemas.
/// 过滤 LuaSkills 运行时入口，使只有显式注册工具进入 Provider 可见 Schema。
fn filter_luaskills_entries_by_explicit_registration(
    entries: Vec<RuntimeEntryDescriptor>,
    registration: &PersistedLuaSkillsConfiguration,
) -> Vec<RuntimeEntryDescriptor> {
    entries
        .into_iter()
        .filter(|entry| {
            registration.is_tool_explicitly_registered(&entry.skill_id, &entry.local_name)
        })
        .collect()
}

/// Filters LuaSkills runtime entries so only implicit tools are reachable through host dispatch.
/// 过滤 LuaSkills 运行时入口，使只有隐式工具能通过宿主调度路径访问。
fn filter_luaskills_entries_by_host_dispatch(
    entries: Vec<RuntimeEntryDescriptor>,
    registration: &PersistedLuaSkillsConfiguration,
) -> Vec<RuntimeEntryDescriptor> {
    entries
        .into_iter()
        .filter(|entry| {
            registration.tool_registration_state(&entry.skill_id, &entry.local_name)
                == crate::provider_config::PersistedLuaSkillToolRegistrationState::Implicit
        })
        .collect()
}

/// Builds cached prompt entries for implicit LuaSkills during runtime-foundation refresh.
/// 在运行时基础架构刷新期间构建隐式 LuaSkills 提示词入口缓存。
fn build_luaskill_prompt_entries(
    runtime: &LuaSkillsRuntimeHandle,
    entries: &[RuntimeEntryDescriptor],
    warnings: &mut Vec<String>,
) -> Vec<LuaSkillPromptEntry> {
    entries
        .iter()
        .map(|entry| {
            let visible_schema = luaskills_entry_args_schema(entry, true, true);
            let argument_summary = render_luaskill_argument_summary(&visible_schema);
            let schema_markdown = render_luaskill_schema_markdown(&visible_schema);
            let mut prompt_entry =
                LuaSkillPromptEntry::new(&entry.canonical_name, &entry.description);
            if let Some(argument_summary) = argument_summary {
                prompt_entry = prompt_entry.with_argument_summary(argument_summary);
            }
            if let Some(schema_markdown) = schema_markdown {
                prompt_entry = prompt_entry.with_schema_markdown(schema_markdown);
            }
            match render_luaskills_entry_help_summary(runtime, entry) {
                Ok(Some(help)) => {
                    prompt_entry = prompt_entry
                        .with_help_summary(Some(help.flow_name), Some(help.description));
                    if !help.content.trim().is_empty() {
                        prompt_entry = prompt_entry.with_help_content(help.content);
                    }
                }
                Ok(None) => {}
                Err(error) => warnings.push(error),
            }
            prompt_entry
        })
        .collect()
}

#[cfg(test)]
mod tests;
