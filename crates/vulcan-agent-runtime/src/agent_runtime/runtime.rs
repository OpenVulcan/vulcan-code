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
use crate::tooling::{CommandSessionRuntime, NativeToolCatalog, build_native_tool_catalog};
use luaskills::RuntimeEntryDescriptor;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vulcan_luaskills_prompt::{
    LuaSkillPromptEntry, render_luaskill_argument_summary, render_luaskill_schema_markdown,
};
use vulcan_session_core::{PromptSkillSnapshot, ProviderModelSnapshot, ToolExposureSnapshot};
use vulcan_task_session::SubagentProfile;

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
    /// Shared structured-question runtime reused across turns so ask tool calls can survive runtime refreshes while still waiting for GUI input.
    /// 在多轮执行之间复用的共享结构化问题运行时，使 ask 工具在等待图形界面输入期间也能穿越运行时刷新继续存活。
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
        Self::build_with_task_runtime(
            config,
            agent_profile_catalog,
            luaskills_runtime,
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
        mcp_runtime_access: McpRuntimeAccess,
        task_runtime: Arc<TaskRuntime>,
        question_runtime: Arc<QuestionRuntime>,
        command_session_runtime: Arc<CommandSessionRuntime>,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
    ) -> Self {
        let runtime_config = AgentRuntimeConfig::from_sources(config, agent_profile_catalog);
        let native_tools = build_native_tool_catalog(
            &runtime_config,
            config.default_command_shell.as_deref(),
            Arc::clone(&command_session_runtime),
            config.vmm.clone(),
        );
        let mut warnings = Vec::new();
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
        let subagent_profiles = build_task_runtime_subagent_profiles(
            &runtime_config,
            &native_tools,
            &mcp_runtime_access,
            &mut warnings,
        );
        if let Err(error) = task_runtime.configure_subagent_profiles(subagent_profiles) {
            warnings.push(format!("同步子代理 profile 到 TaskRuntime 失败：{error}"));
        }

        Self {
            config: runtime_config,
            native_tools,
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
                    "{} · {} · 原生 {} · MCP 服务器 {} · MCP 工具 {}",
                    policy.id,
                    policy.name,
                    policy.native_tools.len(),
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

/// Builds TaskRuntime subagent profiles from the already-resolved runtime foundation configuration.
/// 从已解析的运行时基础架构配置构建 TaskRuntime 子代理配置快照。
fn build_task_runtime_subagent_profiles(
    runtime_config: &AgentRuntimeConfig,
    native_tools: &NativeToolCatalog,
    mcp_runtime_access: &McpRuntimeAccess,
    warnings: &mut Vec<String>,
) -> Vec<SubagentProfile> {
    let available_native_tool_ids = native_tools
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id.clone())
        .collect::<BTreeSet<_>>();
    runtime_config
        .agents
        .iter()
        .map(|definition| {
            runtime_agent_definition_to_subagent_profile(
                definition,
                runtime_config,
                &available_native_tool_ids,
                mcp_runtime_access,
                warnings,
            )
        })
        .collect()
}

/// Converts one runtime agent definition into the TaskSession resolver's profile model.
/// 将单个运行时代理定义转换为 TaskSession 解析器使用的配置模型。
fn runtime_agent_definition_to_subagent_profile(
    definition: &RuntimeAgentDefinition,
    runtime_config: &AgentRuntimeConfig,
    available_native_tool_ids: &BTreeSet<String>,
    mcp_runtime_access: &McpRuntimeAccess,
    warnings: &mut Vec<String>,
) -> SubagentProfile {
    let mut profile = SubagentProfile::new(definition.id.clone(), definition.name.clone());
    if !definition.description.trim().is_empty() {
        profile.description = Some(definition.description.clone());
    }
    if !definition.system_prompt.trim().is_empty() {
        profile.base_system_prompt = Some(definition.system_prompt.clone());
    }
    profile.provider_model = provider_model_snapshot_from_runtime_agent(definition);
    profile.tool_exposure = tool_exposure_snapshot_from_runtime_agent(
        definition,
        runtime_config,
        available_native_tool_ids,
        mcp_runtime_access,
        warnings,
    );
    profile.prompt_skills = PromptSkillSnapshot {
        enabled: definition.skill_ids.clone(),
        disabled: Vec::new(),
    };
    profile.runtime_policy = definition.runtime_policy;
    profile.extra = runtime_agent_extra_payload(definition);
    profile
}

/// Builds a provider/model snapshot from one runtime agent definition.
/// 从单个运行时代理定义构建供应商与模型快照。
fn provider_model_snapshot_from_runtime_agent(
    definition: &RuntimeAgentDefinition,
) -> ProviderModelSnapshot {
    let mut snapshot = ProviderModelSnapshot::default();
    let Some(model) = definition
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return snapshot;
    };
    if let Some((provider_id, model_id)) = model.split_once('/') {
        snapshot.provider_id = Some(provider_id.trim().to_string());
        snapshot.model_id = Some(model_id.trim().to_string());
    } else {
        snapshot.model_id = Some(model.to_string());
    }
    snapshot
}

/// Builds a tool exposure snapshot from one runtime agent's resolved tool policy.
/// 根据单个运行时代理已解析工具策略构建工具可见性快照。
fn tool_exposure_snapshot_from_runtime_agent(
    definition: &RuntimeAgentDefinition,
    runtime_config: &AgentRuntimeConfig,
    available_native_tool_ids: &BTreeSet<String>,
    mcp_runtime_access: &McpRuntimeAccess,
    warnings: &mut Vec<String>,
) -> ToolExposureSnapshot {
    let policy = definition
        .tool_policy
        .as_ref()
        .and_then(|policy_id| runtime_config.tool_policies.get(policy_id));
    let mut allowed_tools = resolve_agent_native_tools(
        definition,
        &runtime_config.tool_policies,
        available_native_tool_ids,
        warnings,
    );
    let mcp_allowlist = resolve_agent_mcp_tool_allowlist(definition, policy);
    allowed_tools.extend(
        mcp_runtime_access
            .available_tool_bindings_filtered(&mcp_allowlist)
            .into_iter()
            .map(|binding| binding.id().to_string()),
    );
    dedupe_string_list(&mut allowed_tools);
    ToolExposureSnapshot {
        allowed_tools,
        disabled_tools: Vec::new(),
        allowed_ai_capabilities: Vec::new(),
        disabled_ai_capabilities: Vec::new(),
        luaskills_overrides: Value::Null,
        mcp_server_overrides: Value::Null,
    }
}

/// Builds an extra payload containing runtime scalar overrides declared by an agent.
/// 构建包含代理声明的运行时标量覆盖的额外载荷。
fn runtime_agent_extra_payload(definition: &RuntimeAgentDefinition) -> Value {
    let mut payload = serde_json::Map::new();
    if let Some(temperature) = definition.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }
    if let Some(max_turns) = definition.max_turns {
        payload.insert(String::from("max_turns"), json!(max_turns));
    }
    if payload.is_empty() {
        Value::Null
    } else {
        Value::Object(payload)
    }
}

/// Deduplicates one string list while preserving the original order.
/// 在保留原始顺序的同时为字符串列表去重。
fn dedupe_string_list(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
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
                "代理 `{}` 引用了不存在的工具策略 `{}`，因此不会暴露任何原生工具。",
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
    names
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
mod tests {
    use super::{
        build_task_runtime_subagent_profiles, filter_luaskills_entries_by_explicit_registration,
        filter_luaskills_entries_by_host_dispatch, resolve_agent_native_tools,
        runtime_agent_definition_to_subagent_profile,
    };
    use crate::agent_runtime::config::{
        AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeToolPolicyDefinition,
    };
    use crate::mcp::configured_mcp_runtime_access;
    use crate::provider_config::{PersistedLuaSkillsConfiguration, ProviderConfigurationFile};
    use crate::tooling::{NativeToolCatalog, NativeToolDescriptor};
    use luaskills::RuntimeEntryDescriptor;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_vmm::VmmConfigLayer;

    /// Verifies that referencing one missing tool policy blocks native-tool exposure instead of silently falling back to a full-open default.
    /// 验证当代理引用缺失的工具策略时，原生工具暴露会被阻断，而不是静默退回到完全放开的默认行为。
    #[test]
    fn missing_tool_policy_blocks_native_tool_resolution() {
        let definition = RuntimeAgentDefinition {
            id: String::from("agent-a"),
            name: String::from("Agent A"),
            kind: RuntimeAgentKind::Worker,
            description: String::from("worker"),
            system_prompt: String::new(),
            skill_ids: Vec::new(),
            native_tools: Vec::new(),
            mcp_tools: Vec::new(),
            delegates: Vec::new(),
            tool_policy: Some(String::from("missing-policy")),
            routes: Vec::new(),
            fallback_delegate: None,
            model: None,
            temperature: None,
            max_turns: None,
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
        };
        let available_native_tool_ids =
            BTreeSet::from([String::from("shell_command"), String::from("shell_session")]);
        let mut warnings = Vec::new();

        let resolved = resolve_agent_native_tools(
            &definition,
            &BTreeMap::<String, RuntimeToolPolicyDefinition>::new(),
            &available_native_tool_ids,
            &mut warnings,
        );

        assert!(resolved.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("不会暴露任何原生工具"));
    }

    /// Verifies runtime agent definitions become TaskSession profiles without TaskRuntime re-reading private configuration.
    /// 验证运行时代理定义会转换为 TaskSession profile，而不需要 TaskRuntime 重新读取私有配置。
    #[test]
    fn runtime_agent_definition_becomes_subagent_profile_snapshot() {
        let definition = RuntimeAgentDefinition {
            id: String::from("agent-review"),
            name: String::from("Review Agent"),
            kind: RuntimeAgentKind::Worker,
            description: String::from("Review code changes"),
            system_prompt: String::from("Review code carefully."),
            skill_ids: vec![
                String::from("regular.review"),
                String::from("luaskills.bridge"),
            ],
            native_tools: Vec::new(),
            mcp_tools: Vec::new(),
            delegates: Vec::new(),
            tool_policy: Some(String::from("review-tools")),
            routes: Vec::new(),
            fallback_delegate: None,
            model: Some(String::from("deepseek/deepseek-chat")),
            temperature: Some(0.3),
            max_turns: Some(8),
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest {
                hooks_enabled: Some(true),
                vmm_enabled: None,
                memory_recall_enabled: Some(true),
                memory_writeback_enabled: None,
                search_projection_enabled: Some(true),
                database_projection_enabled: Some(true),
            },
        };
        let mut runtime_config = test_runtime_config(vec![definition.clone()]);
        runtime_config.tool_policies.insert(
            String::from("review-tools"),
            RuntimeToolPolicyDefinition {
                id: String::from("review-tools"),
                name: String::from("Review Tools"),
                allow_all_native: false,
                native_tools: vec![
                    String::from("shell_command"),
                    String::from("missing_tool"),
                    String::from("shell_command"),
                ],
                mcp_servers: Vec::new(),
                mcp_tools: Vec::new(),
            },
        );
        let available_native_tool_ids = BTreeSet::from([
            String::from("shell_command"),
            String::from("vulcan_file_list"),
        ]);
        let mcp_runtime_access =
            configured_mcp_runtime_access(&ProviderConfigurationFile::default());
        let mut warnings = Vec::new();

        let profile = runtime_agent_definition_to_subagent_profile(
            &definition,
            &runtime_config,
            &available_native_tool_ids,
            &mcp_runtime_access,
            &mut warnings,
        );

        assert_eq!(profile.id, "agent-review");
        assert_eq!(profile.name, "Review Agent");
        assert_eq!(profile.description.as_deref(), Some("Review code changes"));
        assert_eq!(
            profile.base_system_prompt.as_deref(),
            Some("Review code carefully.")
        );
        assert_eq!(
            profile.provider_model.provider_id.as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            profile.provider_model.model_id.as_deref(),
            Some("deepseek-chat")
        );
        assert_eq!(
            profile.prompt_skills.enabled,
            vec![
                String::from("regular.review"),
                String::from("luaskills.bridge")
            ]
        );
        assert_eq!(
            profile.tool_exposure.allowed_tools,
            vec![String::from("shell_command")]
        );
        let resolved_temperature = profile.extra["temperature"].as_f64().unwrap_or_default();
        assert!((resolved_temperature - 0.3).abs() < 0.000001);
        assert_eq!(profile.extra["max_turns"], json!(8));
        assert_eq!(profile.runtime_policy.hooks_enabled, Some(true));
        assert_eq!(profile.runtime_policy.memory_recall_enabled, Some(true));
        assert_eq!(profile.runtime_policy.search_projection_enabled, Some(true));
        assert_eq!(
            profile.runtime_policy.database_projection_enabled,
            Some(true)
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("missing_tool"));
    }

    /// Verifies the TaskRuntime profile synchronization entry consumes the shared native-tool catalog descriptors.
    /// 验证 TaskRuntime profile 同步入口会消费共享原生工具目录描述符。
    #[test]
    fn build_task_runtime_subagent_profiles_uses_native_catalog_descriptors() {
        let definition = RuntimeAgentDefinition {
            id: String::from("agent-artist"),
            name: String::from("Artist Agent"),
            kind: RuntimeAgentKind::Worker,
            description: String::new(),
            system_prompt: String::new(),
            skill_ids: Vec::new(),
            native_tools: vec![
                String::from("image_generation"),
                String::from("unregistered_tool"),
            ],
            mcp_tools: Vec::new(),
            delegates: Vec::new(),
            tool_policy: None,
            routes: Vec::new(),
            fallback_delegate: None,
            model: Some(String::from("flux-dev")),
            temperature: None,
            max_turns: None,
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
        };
        let runtime_config = test_runtime_config(vec![definition]);
        let native_tools = NativeToolCatalog {
            definitions: BTreeMap::new(),
            descriptors: vec![
                test_native_tool_descriptor("image_generation"),
                test_native_tool_descriptor("music_generation"),
            ],
            vmm_global: VmmConfigLayer::default(),
        };
        let mcp_runtime_access =
            configured_mcp_runtime_access(&ProviderConfigurationFile::default());
        let mut warnings = Vec::new();

        let profiles = build_task_runtime_subagent_profiles(
            &runtime_config,
            &native_tools,
            &mcp_runtime_access,
            &mut warnings,
        );

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "agent-artist");
        assert_eq!(
            profiles[0].tool_exposure.allowed_tools,
            vec![String::from("image_generation")]
        );
        assert_eq!(
            profiles[0].provider_model.model_id.as_deref(),
            Some("flux-dev")
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unregistered_tool"));
    }

    /// Verifies that only explicitly registered LuaSkills tools enter provider-visible schemas while implicit tools remain host-dispatchable later.
    /// 验证只有显式注册的 LuaSkills 工具会进入 Provider 可见 Schema，而隐式工具保留给后续宿主调度。
    #[test]
    fn luaskills_registration_filters_only_explicit_tools() {
        let entries = vec![
            build_test_luaskills_entry("skill-a", "tool-a"),
            build_test_luaskills_entry("skill-a", "tool-b"),
            build_test_luaskills_entry("skill-b", "tool-a"),
        ];
        let mut registration = PersistedLuaSkillsConfiguration::default();
        assert!(registration.set_tool_disabled("skill-a", "tool-a", true));

        let filtered = filter_luaskills_entries_by_explicit_registration(entries, &registration);
        let remaining = filtered
            .iter()
            .map(|entry| entry.canonical_name.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            remaining,
            BTreeSet::from(["skill-a-tool-b", "skill-b-tool-a"])
        );
    }

    /// Verifies only implicit LuaSkills tools are callable through the host-side dispatcher.
    /// 验证只有隐式 LuaSkills 工具能通过宿主侧调度器调用。
    #[test]
    fn luaskills_dispatch_filters_only_implicit_tools() {
        let entries = vec![
            build_test_luaskills_entry("skill-a", "tool-a"),
            build_test_luaskills_entry("skill-a", "tool-b"),
            build_test_luaskills_entry("skill-b", "tool-a"),
        ];
        let mut registration = PersistedLuaSkillsConfiguration::default();
        assert!(registration.set_tool_registration_state(
            "skill-a",
            "tool-a",
            crate::provider_config::PersistedLuaSkillToolRegistrationState::Implicit,
        ));
        assert!(registration.set_tool_registration_state(
            "skill-b",
            "tool-a",
            crate::provider_config::PersistedLuaSkillToolRegistrationState::Disabled,
        ));

        let filtered = filter_luaskills_entries_by_host_dispatch(entries, &registration);
        let remaining = filtered
            .iter()
            .map(|entry| entry.canonical_name.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(remaining, BTreeSet::from(["skill-a-tool-a"]));
    }

    /// Builds one minimal LuaSkills runtime entry descriptor used by the runtime visibility tests.
    /// 构建运行时可见性测试使用的最小 LuaSkills 运行时入口描述符。
    fn build_test_luaskills_entry(skill_id: &str, local_name: &str) -> RuntimeEntryDescriptor {
        RuntimeEntryDescriptor {
            canonical_name: format!("{skill_id}-{local_name}"),
            skill_id: skill_id.to_string(),
            local_name: local_name.to_string(),
            root_name: String::from("ROOT"),
            skill_dir: String::from("skill-dir"),
            description: String::from("entry"),
            parameters: Vec::new(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "string"
                    }
                },
                "required": ["value"]
            }),
        }
    }

    /// Builds one minimal runtime configuration for TaskRuntime profile synchronization tests.
    /// 为 TaskRuntime profile 同步测试构建一份最小运行时配置。
    fn test_runtime_config(agents: Vec<RuntimeAgentDefinition>) -> AgentRuntimeConfig {
        AgentRuntimeConfig {
            active_model: Some(String::from("deepseek/deepseek-chat")),
            active_agent_id: agents
                .first()
                .map(|agent| agent.id.clone())
                .unwrap_or_else(|| String::from("agent-a")),
            agents,
            skills: Vec::new(),
            tool_policies: BTreeMap::new(),
            configured_mcp_server_ids: Vec::new(),
            enabled_mcp_server_ids: Vec::new(),
        }
    }

    /// Builds one native-tool descriptor used by TaskRuntime profile synchronization tests.
    /// 构建 TaskRuntime profile 同步测试使用的原生工具描述符。
    fn test_native_tool_descriptor(tool_id: &str) -> NativeToolDescriptor {
        NativeToolDescriptor {
            id: tool_id.to_string(),
            name: tool_id.to_string(),
            description: String::from("test tool"),
            help_text: None,
            ai_visible: true,
            available_in_plan: false,
        }
    }
}
