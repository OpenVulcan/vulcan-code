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
    let snapshot = runtime.snapshot();

    if snapshot.servers.is_empty() {
        return (runtime_events, rows, errors, warnings);
    }

    for server in snapshot.servers.values().filter(|server| server.enabled) {
        let summary = format!(
            "服务器 `{}` 当前状态为 `{}`，已注册 {} 个工具。",
            server.name,
            server.status.label(),
            server.tools.len()
        );
        rows.push(summary.clone());
        match &server.last_error {
            Some(error) => {
                let combined = format!("{summary} 最近一次错误：{error}");
                warnings.push(combined.clone());
                errors.push(combined.clone());
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    format!("MCP 服务器离线 · {}", server.name),
                    combined,
                ));
            }
            None => {
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::ToolFinished,
                    format!("MCP 服务器在线 · {}", server.name),
                    summary,
                ));
            }
        }
    }

    (runtime_events, rows, errors, warnings)
}
