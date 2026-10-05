//! Management status selection for global and conversation-scoped plugin MCP servers.
//! 全局与会话级插件 MCP 服务器的管理状态选择。

use vulcan_agent_runtime::mcp::{McpRuntimeServerState, McpRuntimeServerStatus};
use vulcan_plugin_runtime::{VulcanPluginMcpScopeManifest, VulcanPluginRuntimeState};

#[cfg(test)]
mod tests;

/// Resolves a missing instance from the declared scope and plugin lifecycle.
/// 根据声明范围与插件生命周期解析无实例状态。
/// `runtime` is the plugin state and `scope` is the manifest scope; returns a stable UI status key.
/// `runtime` 为插件状态，`scope` 为清单范围；返回稳定界面状态键。
pub(super) fn inactive_plugin_mcp_status(
    runtime: VulcanPluginRuntimeState,
    scope: VulcanPluginMcpScopeManifest,
) -> &'static str {
    match runtime {
        VulcanPluginRuntimeState::Disabled
        | VulcanPluginRuntimeState::DisabledWithCleanupWarning => "disabled",
        VulcanPluginRuntimeState::Enabling => "preparing",
        VulcanPluginRuntimeState::Draining => "draining",
        VulcanPluginRuntimeState::Enabled => match scope {
            VulcanPluginMcpScopeManifest::Conversation => "onDemand",
            VulcanPluginMcpScopeManifest::Global => "stopped",
        },
        VulcanPluginRuntimeState::UnsupportedPlatform | VulcanPluginRuntimeState::Unavailable => {
            "failed"
        }
    }
}

/// Selects the least healthy instance of this exact server generation for the compact management row.
/// 为紧凑管理行选择此精确服务器代次中健康状况最差的实例。
/// `states` contains observations, `id` and `generation` select the manifest; returns one borrowed snapshot.
/// `states` 包含观测，`id` 与 `generation` 选择清单；返回一个借用快照。
/// Counts, errors and logs remain from the same instance, never multiplied by the number of sessions.
/// 数量、错误和日志始终来自同一实例，绝不按会话数量倍增。
pub(super) fn select_plugin_mcp_state<'a>(
    states: &'a [McpRuntimeServerState],
    id: &str,
    generation: u64,
) -> Option<&'a McpRuntimeServerState> {
    states
        .iter()
        .filter(|state| state.id == id && state.generation == Some(generation))
        .max_by_key(|state| match state.status {
            // A healthy session must not conceal another session's failure or pending cleanup.
            // 健康会话不得掩盖其他会话的失败或尚未完成的清理。
            McpRuntimeServerStatus::Failed => 7,
            McpRuntimeServerStatus::Disconnected => 6,
            McpRuntimeServerStatus::Connected if state.last_error.is_some() => 5,
            McpRuntimeServerStatus::Stopping => 4,
            McpRuntimeServerStatus::Connecting => 3,
            McpRuntimeServerStatus::Connected => 2,
            McpRuntimeServerStatus::Disabled => 1,
            McpRuntimeServerStatus::Stopped => 0,
        })
}
