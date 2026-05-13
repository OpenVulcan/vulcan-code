//! MCP runtime projection helpers for execution tracing and UI-facing summaries.
//! 面向执行追踪与界面摘要的 MCP 运行时投影辅助模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::mcp::McpRuntimeDiscoveryReport;

/// Converts one stable MCP runtime discovery report into UI-facing runtime events and summary rows.
/// 将一份稳定 MCP 运行时发现报告转换为面向界面的运行时事件与摘要行集合。
pub(crate) fn collect_enabled_mcp_runtime_events(
    report: &McpRuntimeDiscoveryReport,
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
