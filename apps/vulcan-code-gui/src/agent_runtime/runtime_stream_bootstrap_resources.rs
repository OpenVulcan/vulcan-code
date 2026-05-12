//! Runtime stream bootstrap resource resolution for tools, MCP discovery, and warning aggregation.
//! 面向工具、MCP 发现与告警汇总的运行时流 bootstrap 资源解析模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::mcp_runtime_projection::collect_enabled_mcp_runtime_events;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use std::path::PathBuf;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Resolved runtime bootstrap resources collected before provider-facing core state is derived.
/// 在推导面向 Provider 的核心状态之前收集完成的运行时 bootstrap 资源集合。
pub(crate) struct RuntimeBootstrapResources {
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub active_agent: Option<RuntimeAgentDefinition>,
    /// Host-owned unified tool manager used by the transport layer.
    /// 供传输层使用的宿主自有统一工具管理器。
    pub tools_manager: VulcanToolsManager,
    /// Final resolved tool definitions exposed to the provider for this turn.
    /// 当前回合向 Provider 暴露的最终已解析工具定义集合。
    pub resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    /// Discovery events projected from enabled MCP runtime servers.
    /// 从已启用 MCP 运行时服务器投影得到的发现事件集合。
    pub discovery_events: Vec<RuntimeEvent>,
    /// Human-readable MCP discovery rows captured for the final execution report.
    /// 为最终执行报告捕获的人类可读 MCP 发现摘要行集合。
    pub discovery_rows: Vec<String>,
    /// MCP discovery errors captured for the final execution report.
    /// 为最终执行报告捕获的 MCP 发现错误集合。
    pub discovery_errors: Vec<String>,
    /// Mutable warning list accumulated before provider streaming begins.
    /// 在 Provider 流式执行开始前累计得到的可变警告列表。
    pub warnings: Vec<String>,
}

/// Resolves agent-scoped tools and MCP discovery outputs before provider-facing bootstrap state is derived.
/// 在推导面向 Provider 的 bootstrap 状态之前，先解析代理作用域工具与 MCP 发现结果。
pub(crate) fn resolve_runtime_bootstrap_resources(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    preparation: &ExecutionPreparation,
) -> RuntimeBootstrapResources {
    let active_agent = foundation.active_agent_definition().cloned();
    let active_agent_tool_id = active_agent
        .as_ref()
        .map(|agent| agent.id.clone())
        .unwrap_or_else(|| preparation.agent_id.clone());
    let tools_manager = build_vulcan_tools_manager(
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        mcp_runtime.clone(),
        (!preparation.workspace_path.is_empty())
            .then(|| PathBuf::from(preparation.workspace_path.clone())),
        foundation.resolved_native_tools_for_agent(&active_agent_tool_id),
        foundation.resolved_luaskills_tools_for_agent(&active_agent_tool_id),
        foundation.resolved_mcp_tools_for_agent(&active_agent_tool_id),
    );
    let resolved_tool_definitions = tools_manager
        .resolve_for_execution_mode(&preparation.execution_mode)
        .definitions;
    let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
        collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
    let mut warnings = preparation.warnings.clone();
    warnings.extend(discovery_warnings);

    RuntimeBootstrapResources {
        active_agent,
        tools_manager,
        resolved_tool_definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        warnings,
    }
}
