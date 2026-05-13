//! Host runtime foundation assembled from provider, tool, skill, and MCP configuration.
//! 从供应商、工具、技能与 MCP 配置组装出的宿主运行时基础架构。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeSkillKind,
    RuntimeToolPolicyDefinition,
};
use crate::luaskills_runtime::{LuaSkillsRuntimeHandle, list_luaskills_tool_entries};
use crate::mcp::{
    McpRegistrySummary, McpRuntimeDiscoveryReport, McpRuntimeHandle, McpRuntimeOverview,
    McpRuntimeToolAllowlist, McpRuntimeToolBinding, configured_mcp_registry_summary,
};
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::{NativeToolCatalog, build_native_tool_catalog};
use luaskills::RuntimeEntryDescriptor;
use std::collections::{BTreeMap, BTreeSet};

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
    /// Lightweight configured MCP summary retained for counts and fallback rows.
    /// 为计数与回退展示行保留的轻量 MCP 配置摘要。
    mcp_registry_summary: McpRegistrySummary,
    /// Live MCP runtime handle that tracks current transport state and available tools.
    /// 跟踪当前传输状态与可用工具的实时 MCP 运行时句柄。
    mcp_runtime: Option<McpRuntimeHandle>,
    /// Build warnings collected while materialising the foundation.
    /// 在组装基础架构时收集到的构建警告。
    warnings: Vec<String>,
}

impl RuntimeFoundation {
    /// Builds the host runtime foundation from the shared provider configuration file.
    /// 根据共享供应商配置文件构建宿主运行时基础架构。
    pub fn build(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        mcp_runtime: Option<McpRuntimeHandle>,
    ) -> Self {
        let runtime_config = AgentRuntimeConfig::from_sources(config, agent_profile_catalog);
        let native_tools = build_native_tool_catalog(&runtime_config);
        let mcp_registry_summary = configured_mcp_registry_summary(config);
        let mut warnings = Vec::new();
        let luaskills_tools = match luaskills_runtime.as_ref() {
            Some(handle) => match list_luaskills_tool_entries(handle) {
                Ok(entries) => entries,
                Err(error) => {
                    warnings.push(format!("加载 LuaSkills 工具表失败：{error}"));
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        Self {
            config: runtime_config,
            native_tools,
            luaskills_tools,
            mcp_registry_summary,
            mcp_runtime,
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
            .unwrap_or_else(|| String::from("未解析到激活代理"));

        let coordinator_count = self
            .config
            .agents
            .iter()
            .filter(|definition| definition.kind == RuntimeAgentKind::Coordinator)
            .count();

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
            configured_mcp_server_count: self.mcp_registry_summary.configured_count(),
            enabled_mcp_server_count: self.mcp_registry_summary.enabled_count(),
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

    /// Resolves the MCP runtime tools currently visible to one agent.
    /// 解析当前对某个代理可见的 MCP 运行时工具集合。
    pub fn resolved_mcp_tools_for_agent(&self, agent_id: &str) -> Vec<McpRuntimeToolBinding> {
        let Some(mcp_runtime) = self.mcp_runtime.as_ref() else {
            return Vec::new();
        };
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

        mcp_runtime.available_tool_bindings_filtered(&tool_allowlist)
    }

    /// Returns one stable MCP runtime overview that narrows summary and discovery reads to one access surface.
    /// 返回一份将摘要与发现读取收敛到单一访问面的稳定 MCP 运行时概览。
    pub fn mcp_overview(&self) -> McpRuntimeOverview {
        self.mcp_runtime.as_ref().map_or_else(
            || {
                McpRuntimeOverview::new(
                    self.mcp_registry_summary.summary_rows(),
                    self.mcp_registry_summary.enabled_summary_rows(),
                    McpRuntimeDiscoveryReport::default(),
                )
            },
            McpRuntimeHandle::overview,
        )
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
