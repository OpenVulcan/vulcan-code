//! MCP runtime projection helpers for execution tracing and UI-facing summaries.
//! 面向执行追踪与界面摘要的 MCP 运行时投影辅助模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::mcp::McpRuntimeHandle;

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
    let diagnostics = runtime.enabled_server_diagnostics();
    if diagnostics.is_empty() {
        return (runtime_events, rows, errors, warnings);
    }

    for server in diagnostics {
        let summary = server.summary_text();
        rows.push(summary.clone());
        match server.warning_text() {
            Some(combined) => {
                warnings.push(combined.clone());
                errors.push(combined.clone());
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    server.runtime_event_title(),
                    combined,
                ));
            }
            None => {
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::ToolFinished,
                    server.runtime_event_title(),
                    summary,
                ));
            }
        }
    }

    (runtime_events, rows, errors, warnings)
}
