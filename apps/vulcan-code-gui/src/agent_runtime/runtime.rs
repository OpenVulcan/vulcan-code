//! Compose runtime foundation assembled from provider, tool, skill, and MCP configuration.
//! 从供应商、工具、技能与 MCP 配置组装出的 compose 运行时基础架构。

use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeSkillKind,
    RuntimeToolPolicyDefinition,
};
use crate::mcp::McpServerRegistry;
use crate::provider_config::ProviderConfigurationFile;
use crate::skills::{BuiltinSkillCatalog, build_builtin_skill_catalog};
use crate::tooling::{NativeToolCatalog, build_native_tool_catalog};
use rig_compose::agent::Agent;
use rig_compose::coordinator::{CoordinatorAgent, RoutingRule};
use rig_compose::delegate::{DelegateRegistry, DelegateTool, InProcessAgentDelegate};
use rig_compose::{GenericAgent, ToolRegistry};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
}

/// In-memory compose runtime foundation built from the shared Vulcan configuration file.
/// 根据共享 Vulcan 配置文件构建的内存态 compose 运行时基础架构。
pub struct RuntimeFoundation {
    /// Runtime-facing aggregate configuration.
    /// 面向运行时的聚合配置。
    config: AgentRuntimeConfig,
    /// Registered native tool catalog.
    /// 已注册的原生工具目录。
    native_tools: NativeToolCatalog,
    /// Registered built-in skill catalog.
    /// 已注册的内建技能目录。
    skills: BuiltinSkillCatalog,
    /// MCP server registry.
    /// MCP 服务器注册表。
    mcp_registry: McpServerRegistry,
    /// Built agents keyed by their stable agent id.
    /// 以稳定代理标识为键保存的已构建代理集合。
    agents: BTreeMap<String, Arc<dyn Agent>>,
    /// Delegate registry holding in-process child-agent executors.
    /// 保存进程内子代理执行器的委派注册表。
    delegate_registry: DelegateRegistry,
    /// Registry of delegate tools prepared for later model-driven delegation.
    /// 为后续模型驱动委派准备的委派工具注册表。
    delegate_tools: ToolRegistry,
    /// Coordinator agent identifiers.
    /// 协调代理标识集合。
    coordinator_agent_ids: BTreeSet<String>,
    /// Build warnings collected while materialising the foundation.
    /// 在组装基础架构时收集到的构建警告。
    warnings: Vec<String>,
}

impl RuntimeFoundation {
    /// Builds the compose runtime foundation from the shared provider configuration file.
    /// 根据共享供应商配置文件构建 compose 运行时基础架构。
    pub fn build(config: &ProviderConfigurationFile) -> Self {
        let runtime_config = AgentRuntimeConfig::from_provider_configuration(config);
        let native_tools = build_native_tool_catalog(&runtime_config);
        let skills = build_builtin_skill_catalog(&runtime_config.skills);
        let mcp_registry = McpServerRegistry::from_provider_configuration(config);
        let mut warnings = Vec::new();
        let mut agents = BTreeMap::new();
        let mut coordinator_agent_ids = BTreeSet::new();

        for definition in runtime_config
            .agents
            .iter()
            .filter(|definition| definition.kind != RuntimeAgentKind::Coordinator)
        {
            if let Some(agent) = build_worker_agent(
                definition,
                &runtime_config.tool_policies,
                &native_tools,
                &skills,
                &mut warnings,
            ) {
                agents.insert(definition.id.clone(), agent);
            }
        }

        let worker_agents = agents.clone();

        for definition in runtime_config
            .agents
            .iter()
            .filter(|definition| definition.kind == RuntimeAgentKind::Coordinator)
        {
            if let Some(agent) = build_coordinator_agent(definition, &worker_agents, &mut warnings)
            {
                coordinator_agent_ids.insert(definition.id.clone());
                agents.insert(definition.id.clone(), agent);
            }
        }

        let delegate_registry = DelegateRegistry::new();
        for (agent_id, agent) in &agents {
            delegate_registry
                .register(agent_id.clone(), InProcessAgentDelegate::arc(agent.clone()));
        }

        let delegate_tools = ToolRegistry::new();
        for (agent_id, definition) in runtime_config
            .agents
            .iter()
            .map(|definition| (&definition.id, definition))
        {
            if let Some(executor) = delegate_registry.get(agent_id) {
                let tool = DelegateTool::new(
                    format!("delegate.{agent_id}"),
                    format!("委派给代理 `{}`。", definition.name),
                    executor,
                );
                delegate_tools.register(Arc::new(tool));
            }
        }

        Self {
            config: runtime_config,
            native_tools,
            skills,
            mcp_registry,
            agents,
            delegate_registry,
            delegate_tools,
            coordinator_agent_ids,
            warnings,
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
            .or_else(|| {
                self.agents
                    .get(&self.config.active_agent_id)
                    .map(|agent| agent.name().to_string())
            })
            .unwrap_or_else(|| String::from("未解析到激活代理"));

        RuntimeFoundationSummary {
            active_agent_id: self.config.active_agent_id.clone(),
            active_agent_name: active_agent,
            active_model: self
                .config
                .active_model
                .clone()
                .unwrap_or_else(|| String::from("未配置模型")),
            configured_agent_count: self.config.agents.len(),
            built_agent_count: self.agents.len(),
            coordinator_count: self.coordinator_agent_ids.len(),
            skill_count: self.skills.descriptors.len(),
            native_tool_count: self.native_tools.descriptors.len(),
            delegate_tool_count: self.delegate_tools.len().max(self.delegate_registry.len()),
            configured_mcp_server_count: self.mcp_registry.configured_count(),
            enabled_mcp_server_count: self.mcp_registry.enabled_count(),
            warnings: self.warnings.clone(),
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
        self.skills
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.kind == kind)
            .map(|descriptor| format!("{} · {}", descriptor.name, descriptor.id))
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

    /// Returns lightweight MCP server rows for UI placeholder surfaces.
    /// 返回供 UI 占位面板使用的轻量 MCP 服务器行文本。
    pub fn mcp_rows(&self) -> Vec<String> {
        self.mcp_registry.summary_rows()
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
        self.config
            .agents
            .iter()
            .find(|definition| definition.id == self.config.active_agent_id)
    }

    /// Returns whether one agent identifier has already been built into the runtime foundation.
    /// 返回某个代理标识当前是否已经被构建进运行时基础架构。
    pub fn has_built_agent(&self, agent_id: &str) -> bool {
        self.agents.contains_key(agent_id)
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

    /// Returns enabled MCP server summary rows for execution diagnostics.
    /// 返回供执行诊断使用的已启用 MCP 服务器摘要行集合。
    pub fn enabled_mcp_server_rows(&self) -> Vec<String> {
        self.mcp_registry
            .summary_rows()
            .into_iter()
            .filter(|row| row.starts_with("启用"))
            .collect()
    }
}

/// Builds one worker or specialist agent from its runtime definition.
/// 根据运行时定义构建单个工作代理或专家代理。
fn build_worker_agent(
    definition: &RuntimeAgentDefinition,
    tool_policies: &BTreeMap<String, RuntimeToolPolicyDefinition>,
    native_tools: &NativeToolCatalog,
    skills: &BuiltinSkillCatalog,
    warnings: &mut Vec<String>,
) -> Option<Arc<dyn Agent>> {
    let available_native_tool_ids = native_tools
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id.clone())
        .collect::<BTreeSet<_>>();
    let allowed_tools = resolve_agent_native_tools(
        definition,
        tool_policies,
        &available_native_tool_ids,
        warnings,
    );

    let builder = GenericAgent::builder(definition.name.clone())
        .with_skills(definition.skill_ids.iter().cloned())
        .with_tools(allowed_tools.iter().cloned());

    match builder.build(&skills.registry, &native_tools.registry) {
        Ok(agent) => Some(Arc::new(agent)),
        Err(error) => {
            warnings.push(format!(
                "代理 `{}` 构建失败，已跳过：{}",
                definition.name, error
            ));
            None
        }
    }
}

/// Builds one coordinator agent from its runtime definition and worker registry.
/// 根据运行时定义与工作代理注册表构建单个协调代理。
fn build_coordinator_agent(
    definition: &RuntimeAgentDefinition,
    worker_agents: &BTreeMap<String, Arc<dyn Agent>>,
    warnings: &mut Vec<String>,
) -> Option<Arc<dyn Agent>> {
    let mut builder = CoordinatorAgent::builder(definition.name.clone());
    let mut registered_specialists = BTreeSet::new();

    for route in &definition.routes {
        let Some(target_agent) = worker_agents.get(&route.delegate) else {
            warnings.push(format!(
                "协调代理 `{}` 引用了不存在的路由目标 `{}`。",
                definition.name, route.delegate
            ));
            continue;
        };

        if registered_specialists.insert(route.delegate.clone()) {
            builder = builder.with_specialist(target_agent.clone());
        }

        builder = builder.route(RoutingRule::new(
            target_agent.name().to_string(),
            [route.signal.clone()],
        ));
    }

    if let Some(fallback) = &definition.fallback_delegate {
        if let Some(target_agent) = worker_agents.get(fallback) {
            if registered_specialists.insert(fallback.clone()) {
                builder = builder.with_specialist(target_agent.clone());
            }
            builder = builder.fallback(target_agent.name().to_string());
        } else {
            warnings.push(format!(
                "协调代理 `{}` 的回退目标 `{}` 不存在。",
                definition.name, fallback
            ));
        }
    }

    Some(Arc::new(builder.build()))
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
                "代理 `{}` 引用了不存在的工具策略 `{}`。",
                definition.name, policy_id
            ));
            available_native_tool_ids.iter().cloned().collect()
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
