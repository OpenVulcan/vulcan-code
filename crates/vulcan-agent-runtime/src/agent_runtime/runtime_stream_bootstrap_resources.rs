//! Runtime stream bootstrap resource collection for prepared tools, MCP discovery, and warning aggregation.
//! 面向已准备工具、MCP 发现与告警汇总的运行时流 bootstrap 资源收集模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::execution_preparation::PreparedToolSnapshot;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use std::sync::Arc;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Resolved runtime bootstrap resources collected before provider-facing core state is derived.
/// 在推导面向 Provider 的核心状态之前收集完成的运行时 bootstrap 资源集合。
pub(crate) struct RuntimeBootstrapResources {
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub active_agent: Option<RuntimeAgentDefinition>,
    /// Shared resumable task runtime reused by the current execution turn.
    /// 被当前执行回合复用的共享可恢复任务运行时。
    pub task_runtime: Arc<TaskRuntime>,
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

/// Collects prepared tool snapshots and MCP discovery outputs before provider-facing bootstrap state is derived.
/// 在推导面向 Provider 的 bootstrap 状态之前，收集已准备工具快照与 MCP 发现结果。
pub(crate) fn resolve_runtime_bootstrap_resources(
    foundation: &RuntimeFoundation,
    _luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    _request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
    tool_snapshot: PreparedToolSnapshot,
) -> RuntimeBootstrapResources {
    let active_agent = foundation
        .agent_definition(&preparation.agent_id)
        .or_else(|| foundation.active_agent_definition())
        .cloned();
    let task_runtime = foundation.task_runtime();
    let tools_manager = tool_snapshot.tools_manager;
    let resolved_tool_definitions = tool_snapshot.resolved_tool_definitions;
    let mcp_overview = foundation.mcp_overview();
    let discovery_report = mcp_overview.discovery_report();
    let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
        project_mcp_discovery_report_to_runtime_events(discovery_report);
    let mut warnings = preparation.warnings.clone();
    warnings.extend(discovery_warnings);

    RuntimeBootstrapResources {
        active_agent,
        task_runtime,
        tools_manager,
        resolved_tool_definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        warnings,
    }
}

/// Projects one stable MCP discovery report into runtime events, discovery rows, discovery errors, and discovery warnings.
/// 将一份稳定 MCP 发现报告投影为运行时事件、发现摘要行、发现错误和发现告警集合。
fn project_mcp_discovery_report_to_runtime_events(
    report: &crate::mcp::McpRuntimeDiscoveryReport,
) -> (Vec<RuntimeEvent>, Vec<String>, Vec<String>, Vec<String>) {
    let mut runtime_events = Vec::new();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if report.is_empty() {
        return (runtime_events, rows, errors, warnings);
    }

    rows.extend(report.discovery_rows().iter().cloned());
    errors.extend(report.discovery_errors().iter().cloned());
    warnings.extend(report.discovery_warnings().iter().cloned());

    for projection in report.event_projections() {
        let event_kind = if projection.is_warning() {
            RuntimeEventKind::Failed
        } else {
            RuntimeEventKind::ToolFinished
        };
        runtime_events.push(RuntimeEvent::new(
            event_kind,
            projection.title().to_string(),
            projection.body().to_string(),
        ));
    }

    (runtime_events, rows, errors, warnings)
}
