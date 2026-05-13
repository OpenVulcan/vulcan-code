//! MCP runtime projection helpers for execution tracing and UI-facing summaries.
//! 面向执行追踪与界面摘要的 MCP 运行时投影辅助模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::mcp::{McpRuntimeEventProjectionKind, McpRuntimeHandle};

/// Collects enabled MCP runtime state and converts the latest snapshot into UI-facing runtime events.
/// 收集已启用 MCP 运行时状态，并将最新快照转换为面向界面的运行时事件。
pub(crate) fn collect_enabled_mcp_runtime_events(
    mcp_runtime: Option<&McpRuntimeHandle>,
) -> (Vec<RuntimeEvent>, Vec<String>, Vec<String>, Vec<String>) {
    let mut runtime_events = Vec::new();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let Some(runtime) = mcp_runtime else {
        return (runtime_events, rows, errors, warnings);
    };
    let report = runtime.enabled_server_discovery_report();
    if report.event_projections().is_empty() {
        return (runtime_events, rows, errors, warnings);
    }

    rows.extend(report.discovery_rows().iter().cloned());
    errors.extend(report.discovery_errors().iter().cloned());
    warnings.extend(report.discovery_warnings().iter().cloned());

    for projection in report.event_projections() {
        let event_kind = match projection.kind() {
            McpRuntimeEventProjectionKind::Informational => RuntimeEventKind::ToolFinished,
            McpRuntimeEventProjectionKind::Warning => RuntimeEventKind::Failed,
        };
        runtime_events.push(RuntimeEvent::new(
            event_kind,
            projection.title().to_string(),
            projection.body().to_string(),
        ));
    }

    (runtime_events, rows, errors, warnings)
}
