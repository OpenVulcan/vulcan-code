//! MCP runtime projection bridge used by the Core Service runtime.
//! Core Service 运行时使用的 MCP 运行时投影桥接层。

use vulcan_agent_runtime::mcp::{McpRuntimeAccess, McpRuntimeServerStatus};

/// Lightweight MCP runtime catalog exposed to the WebUI snapshot.
/// 暴露给 WebUI 快照的轻量 MCP 运行时目录。
#[derive(Debug, Clone)]
pub struct TauriMcpRuntimeCatalog {
    /// Configured server count.
    /// 已配置服务器数量。
    pub configured_server_count: usize,
    /// Enabled server count.
    /// 已启用服务器数量。
    pub enabled_server_count: usize,
    /// Connected server count.
    /// 已连接服务器数量。
    pub connected_server_count: usize,
    /// Discovered tool count.
    /// 已发现工具数量。
    pub tool_count: usize,
    /// Discovered prompt count.
    /// 已发现 prompt 数量。
    pub prompt_count: usize,
    /// Discovered resource count.
    /// 已发现 resource 数量。
    pub resource_count: usize,
    /// Human-readable summary rows.
    /// 人类可读摘要行。
    pub summary_rows: Vec<String>,
    /// Discovered runtime tool summaries.
    /// 已发现的运行时工具摘要。
    pub tools: Vec<TauriMcpRuntimeToolSummary>,
}

/// Lightweight MCP runtime tool summary exposed to the WebUI snapshot.
/// 暴露给 WebUI 快照的轻量 MCP 运行时工具摘要。
#[derive(Debug, Clone)]
pub struct TauriMcpRuntimeToolSummary {
    /// Stable tool selector accepted by runtime exposure policies.
    /// 运行时暴露策略可接受的稳定工具选择器。
    pub id: String,
    /// Owning MCP server identifier.
    /// 所属 MCP 服务器标识。
    pub server_id: String,
    /// Stable remote tool name.
    /// 稳定的远端工具名称。
    pub name: String,
    /// Human-readable remote tool description.
    /// 人类可读的远端工具描述。
    pub description: String,
    /// Original qualified MCP tool identifier.
    /// 原始限定 MCP 工具标识。
    pub origin_tool_id: String,
}

/// Projects one read-only WebUI catalog from the single authoritative MCP runtime access.
/// 从唯一权威的 MCP 运行时访问面投影一份只读 WebUI 目录。
///
/// The service no longer owns a second MCP transport: every count, status, and tool row below is
/// derived from the same extension generation the AI tool loop invokes, so the settings page, the
/// workbench, and the AI tool catalog can never observe different generations.
/// 服务不再持有第二套 MCP 传输：下列每个计数、状态与工具行都派生自 AI 工具循环调用的
/// 同一个扩展代次，因此设置页、工作台与 AI 工具目录绝不会观察到不同代次。
///
/// # Parameters
/// 参数
/// `access` is the exact live MCP runtime access owned by the current agent extension generation.
/// `access` 是当前代理扩展代次持有的精确实时 MCP 运行时访问面。
///
/// # Returns
/// 返回值
/// The catalog snapshot captured under one coherent runtime observation.
/// 返回在同一次一致运行时观测下采集的目录快照。
pub fn mcp_runtime_catalog_from_access(access: &McpRuntimeAccess) -> TauriMcpRuntimeCatalog {
    // Each inspection contains state and tools from the same server snapshot generation.
    // 每份检查结果都包含来自同一服务器快照代次的状态与工具。
    let inspections = access.server_inspections();
    let server_states = inspections
        .iter()
        .map(|inspection| &inspection.state)
        .collect::<Vec<_>>();
    let configured_server_count = server_states.len();
    let enabled_server_count = server_states
        .iter()
        .filter(|state| state.status != McpRuntimeServerStatus::Disabled)
        .count();
    let connected_server_count = server_states
        .iter()
        .filter(|state| state.status == McpRuntimeServerStatus::Connected)
        .count();
    let tool_count = server_states.iter().map(|state| state.tool_count).sum();
    let prompt_count = server_states
        .iter()
        .map(|state| state.prompt_count)
        .sum();
    let resource_count = server_states
        .iter()
        .map(|state| state.resource_count)
        .sum();
    let summary_rows = server_states
        .iter()
        .map(|state| {
            let base = format!(
                "{} · {} · {} · {} · tools={} prompts={} resources={}",
                state.status.label(),
                state.name,
                state.transport,
                state.endpoint,
                state.tool_count,
                state.prompt_count,
                state.resource_count
            );
            match state.last_error.as_deref() {
                Some(error) => format!("{base} · {error}"),
                None => base,
            }
        })
        .collect();
    let mut tools = Vec::new();
    for inspection in inspections {
        for tool in &inspection.tools {
            let origin_tool_id = format!("{}/{}", inspection.state.id, tool.name);
            tools.push(TauriMcpRuntimeToolSummary {
                id: origin_tool_id.clone(),
                server_id: inspection.state.id.clone(),
                name: tool.name.clone(),
                description: tool.description.clone(),
                origin_tool_id,
            });
        }
    }

    TauriMcpRuntimeCatalog {
        configured_server_count,
        enabled_server_count,
        connected_server_count,
        tool_count,
        prompt_count,
        resource_count,
        summary_rows,
        tools,
    }
}
