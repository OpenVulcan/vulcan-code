//! Host runtime foundation assembled from provider, tool, skill, and MCP configuration.
//! 从供应商、工具、技能与 MCP 配置组装出的宿主运行时基础架构。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeToolPolicyDefinition,
};
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime_stream_bootstrap_core::after_precheck_trace;
use crate::agent_runtime::task_runtime::TaskRuntime;
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
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, KNOWLEDGE_BROWSE_TOOL_ID, KNOWLEDGE_READ_TOOL_ID,
    KNOWLEDGE_SEARCH_TOOL_ID, RUNTIME_CONFIG_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use crate::tooling::{CommandSessionRuntime, NativeToolCatalog, build_native_tool_catalog};
use luaskills::RuntimeEntryDescriptor;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vulcan_luaskills_prompt::{
    LuaSkillPromptEntry, render_luaskill_argument_summary, render_luaskill_schema_markdown,
};
use vulcan_plugin_runtime::{
    VulcanPluginCatalogSnapshot, VulcanPluginRuntimeManager, VulcanPluginToolDescriptor,
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
    /// Registered Vulcan Plugin tool count.
    /// 已注册 Vulcan 插件工具数量。
    pub vulcan_plugin_tool_count: usize,
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
}

/// In-memory host runtime foundation built from the shared Vulcan configuration file.
/// 根据共享 Vulcan 配置文件构建的内存态宿主运行时基础架构。
pub struct RuntimeFoundation {
    /// Extension revision captured atomically for this foundation.
    /// 当前基础架构原子捕获的扩展修订号。
    extension_revision: u64,
    /// Runtime-facing aggregate configuration.
    /// 面向运行时的聚合配置。
    config: AgentRuntimeConfig,
    /// Registered native tool catalog.
    /// 已注册的原生工具目录。
    native_tools: NativeToolCatalog,
    /// Long-lived generic Vulcan Plugin runtime shared across foundation refreshes.
    /// 跨基础架构刷新共享的长生命周期通用 Vulcan 插件运行时。
    vulcan_plugin_runtime: Option<VulcanPluginRuntimeManager>,
    /// Immutable Vulcan Plugin catalog revision captured for this foundation.
    /// 当前基础架构捕获的不可变 Vulcan 插件目录修订版。
    vulcan_plugin_catalog: Option<Arc<VulcanPluginCatalogSnapshot>>,
    /// Immutable system-and-user prompt-skill catalog captured before the interaction path.
    /// 在交互路径之前捕获的不可变系统与用户提示词技能目录。
    prompt_skill_catalog: Arc<vulcan_prompt_skills::PromptSkillCatalog>,
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
    /// Process-local public AI registries, catalogs, clients, and secure credential boundary.
    /// 进程内公共 AI 注册表、目录、客户端与安全凭据边界。
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
}

impl RuntimeFoundation {
    /// Returns the immutable extension revision captured for this runtime foundation.
    /// 返回当前运行时基础架构捕获的不可变扩展修订号。
    pub const fn extension_revision(&self) -> u64 {
        self.extension_revision
    }

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
        )
    }

    /// Builds the host runtime foundation with independent LuaSkills and Vulcan Plugin runtimes.
    /// 使用相互独立的 LuaSkills 与 Vulcan 插件运行时构建宿主运行时基础架构。
    pub fn build_from_optional_mcp_access_with_runtimes_and_plugins(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        vulcan_plugin_runtime: Option<VulcanPluginRuntimeManager>,
        mcp_runtime_access: Option<McpRuntimeAccess>,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
    ) -> Self {
        Self::build_with_task_runtime(
            config,
            agent_profile_catalog,
            luaskills_runtime,
            vulcan_plugin_runtime,
            None,
            None,
            0,
            mcp_runtime_access.unwrap_or_else(|| configured_mcp_runtime_access(config)),
            task_runtime,
            question_runtime,
            command_session_runtime,
        )
    }

    /// Builds the host runtime foundation from one atomically captured extension generation.
    /// 根据一个原子捕获的扩展代次构建宿主运行时基础架构。
    pub fn build_from_extension_generation_with_runtimes_and_plugins(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        vulcan_plugin_runtime: Option<VulcanPluginRuntimeManager>,
        extension_generation: Arc<ExtensionGeneration>,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
    ) -> Self {
        Self::build_with_task_runtime(
            config,
            agent_profile_catalog,
            luaskills_runtime,
            vulcan_plugin_runtime,
            extension_generation.plugin_catalog(),
            extension_generation.prompt_skill_catalog(),
            extension_generation.revision(),
            extension_generation.mcp_runtime(),
            task_runtime,
            question_runtime,
            command_session_runtime,
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
            None,
            None,
            0,
            mcp_runtime_access,
            Arc::new(TaskRuntime::new()),
            Arc::new(QuestionRuntime::new()),
            Arc::new(CommandSessionRuntime::new()),
        )
    }

    /// Builds the host runtime foundation with an explicitly provided shared task runtime instance.
    /// 使用显式提供的共享任务运行时实例构建宿主运行时基础架构。
    fn build_with_task_runtime(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        vulcan_plugin_runtime: Option<VulcanPluginRuntimeManager>,
        vulcan_plugin_catalog: Option<Arc<VulcanPluginCatalogSnapshot>>,
        prompt_skill_catalog: Option<Arc<vulcan_prompt_skills::PromptSkillCatalog>>,
        extension_revision: u64,
        mcp_runtime_access: McpRuntimeAccess,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
    ) -> Self {
        after_precheck_trace(
            "after_precheck_24a2_foundation_components_enter",
            format!("extension_revision={extension_revision}"),
        );
        let runtime_config = AgentRuntimeConfig::from_sources(config, agent_profile_catalog);
        after_precheck_trace(
            "after_precheck_24a3_runtime_config_ready",
            format!("extension_revision={extension_revision}"),
        );
        // taskRuntime already owns the catalog generation published by service startup or an explicit settings refresh.
        // taskRuntime 已持有服务启动或显式设置刷新发布的目录代次。
        after_precheck_trace(
            "after_precheck_24a4_subagent_catalog_reused",
            format!("extension_revision={extension_revision}"),
        );
        let native_tools = build_native_tool_catalog(
            &runtime_config,
            config.default_command_shell.as_deref(),
            Arc::clone(&command_session_runtime),
            config.vmm.clone(),
            vulcan_plugin_runtime.clone(),
            luaskills_runtime.clone(),
        );
        after_precheck_trace(
            "after_precheck_24a5_native_tool_catalog_ready",
            format!("extension_revision={extension_revision}"),
        );
        let mut warnings = Vec::new();
        // Capture the manager's already-published immutable generation; service lifecycle owns filesystem refreshes.
        // 捕获管理器已经发布的不可变代次；文件系统刷新由服务生命周期负责。
        let vulcan_plugin_catalog = vulcan_plugin_catalog.or_else(|| {
            vulcan_plugin_runtime
                .as_ref()
                .map(VulcanPluginRuntimeManager::snapshot)
        });
        // promptSkillCatalog is loaded only by standalone foundation construction; service
        // executions always reuse the catalog published by startup or an explicit watcher refresh.
        // promptSkillCatalog 仅由独立基础架构构建加载；服务执行始终复用启动或显式监视刷新发布的目录。
        let prompt_skill_catalog = prompt_skill_catalog.unwrap_or_else(|| {
            let prompt_skill_config =
                crate::skills::prompt::global_config_from_persisted(&config.prompt_skills);
            Arc::new(vulcan_prompt_skills::load_prompt_skills(
                &prompt_skill_config,
            ))
        });
        after_precheck_trace(
            "after_precheck_24a6_plugin_catalog_ready",
            format!(
                "extension_revision={} installed={}",
                extension_revision,
                vulcan_plugin_catalog.is_some()
            ),
        );
        after_precheck_trace(
            "after_precheck_24a7_subagent_catalog_retained",
            format!("extension_revision={extension_revision}"),
        );
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
        after_precheck_trace(
            "after_precheck_24a8_luaskills_catalog_ready",
            format!(
                "extension_revision={} explicit_tools={} dispatch_tools={} prompt_entries={}",
                extension_revision,
                luaskills_tools.len(),
                luaskills_dispatch_tools.len(),
                implicit_luaskill_prompt_entries.len()
            ),
        );
        Self {
            extension_revision,
            config: runtime_config,
            native_tools,
            vulcan_plugin_runtime,
            vulcan_plugin_catalog,
            prompt_skill_catalog,
            luaskills_tools,
            luaskills_dispatch_tools,
            implicit_luaskill_prompt_entries,
            mcp_runtime_access,
            warnings,
            task_runtime,
            question_runtime,
            command_session_runtime,
            public_ai_runtime: None,
        }
    }

    /// Installs the process-local public AI runtime before execution preparation.
    /// 在执行准备前安装进程内公共 AI 运行时。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` is the service-owned state shared by metadata and execution flows.
    /// `runtime` 是由服务持有并在元数据与执行流程间共享的状态。
    pub(crate) fn install_public_ai_runtime(
        &mut self,
        runtime: Arc<vulcan_provider_adapters::PublicAiRuntime>,
    ) {
        self.public_ai_runtime = Some(runtime);
    }

    /// Installs `runtime` through the normal foundation boundary for host integration tests; returns nothing.
    /// 为宿主集成测试通过普通基础边界安装 `runtime`；无返回值。
    #[cfg(feature = "test-support")]
    pub fn install_public_ai_runtime_for_testing(
        &mut self,
        runtime: Arc<vulcan_provider_adapters::PublicAiRuntime>,
    ) {
        self.install_public_ai_runtime(runtime);
    }

    /// Returns the installed public AI runtime.
    /// 返回已安装的公共 AI 运行时。
    ///
    /// # Returns
    /// 返回值
    /// The exact shared runtime when the service installed it.
    /// 返回服务已安装的精确共享运行时。
    pub(crate) fn public_ai_runtime(
        &self,
    ) -> Option<Arc<vulcan_provider_adapters::PublicAiRuntime>> {
        self.public_ai_runtime.clone()
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
            vulcan_plugin_tool_count: self
                .vulcan_plugin_catalog
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
        }
    }

    /// Returns the shared native tool catalog used by the current runtime foundation.
    /// 返回当前运行时基础架构使用的共享原生工具目录。
    pub fn native_tool_catalog(&self) -> &NativeToolCatalog {
        &self.native_tools
    }

    /// Returns the long-lived generic Vulcan Plugin runtime when LuaSkills is available.
    /// 在 LuaSkills 可用时返回长生命周期通用 Vulcan 插件运行时。
    pub fn vulcan_plugin_runtime(&self) -> Option<VulcanPluginRuntimeManager> {
        self.vulcan_plugin_runtime.clone()
    }

    /// Returns the immutable Vulcan Plugin catalog captured by this foundation.
    /// 返回当前基础架构捕获的不可变 Vulcan 插件目录。
    pub fn vulcan_plugin_catalog(&self) -> Option<Arc<VulcanPluginCatalogSnapshot>> {
        self.vulcan_plugin_catalog.clone()
    }

    /// Returns the immutable system-and-user prompt-skill catalog captured by this foundation.
    /// 返回当前基础架构捕获的不可变系统与用户提示词技能目录。
    ///
    /// # Returns
    /// 返回值
    /// The shared catalog generation without filesystem discovery.
    /// 返回不执行文件系统发现的共享目录代次。
    pub fn prompt_skill_catalog(&self) -> Arc<vulcan_prompt_skills::PromptSkillCatalog> {
        Arc::clone(&self.prompt_skill_catalog)
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

    /// Resolves complete Vulcan Plugin tool groups currently visible to one agent.
    /// 解析当前对某个代理完整可见的 Vulcan 插件工具组。
    pub fn resolved_vulcan_plugin_tools_for_agent(
        &self,
        agent_id: &str,
    ) -> Vec<VulcanPluginToolDescriptor> {
        let Some(definition) = self
            .config
            .agents
            .iter()
            .find(|definition| definition.id == agent_id)
        else {
            return Vec::new();
        };
        let Some(catalog) = self.vulcan_plugin_catalog.as_ref() else {
            return Vec::new();
        };
        let available_tool_ids = catalog
            .enabled_tools
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let selected_tool_ids = resolve_agent_vulcan_plugin_tool_ids(
            definition,
            &self.config.tool_policies,
            &available_tool_ids,
        )
        .into_iter()
        .collect::<BTreeSet<_>>();
        let mut groups = BTreeMap::<(String, String), Vec<VulcanPluginToolDescriptor>>::new();
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

/// Resolves the Vulcan Plugin tool whitelist for one agent definition.
/// 为单个代理定义解析 Vulcan 插件工具白名单。
fn resolve_agent_vulcan_plugin_tool_ids(
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
        KNOWLEDGE_SEARCH_TOOL_ID,
        KNOWLEDGE_BROWSE_TOOL_ID,
        KNOWLEDGE_READ_TOOL_ID,
        RUNTIME_CONFIG_TOOL_ID,
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
