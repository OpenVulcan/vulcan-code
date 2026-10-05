//! Idle scope and multi-session failure selection regressions.
//! 空闲范围与多会话失败选择回归。

use super::*;

/// No inputs; an enabled conversation declaration is idle on demand, never a stopped global process.
/// 无输入；启用的会话声明处于按需待命，绝非已停止的全局进程。
#[test]
fn inactive_plugin_mcp_status_respects_scope_and_lifecycle() {
    assert_eq!(
        inactive_plugin_mcp_status(
            VulcanPluginRuntimeState::Enabled,
            VulcanPluginMcpScopeManifest::Conversation
        ),
        "onDemand"
    );
    assert_eq!(
        inactive_plugin_mcp_status(
            VulcanPluginRuntimeState::Enabled,
            VulcanPluginMcpScopeManifest::Global
        ),
        "stopped"
    );
    for (runtime, expected) in [
        (VulcanPluginRuntimeState::Disabled, "disabled"),
        (
            VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            "disabled",
        ),
        (VulcanPluginRuntimeState::Enabling, "preparing"),
        (VulcanPluginRuntimeState::Draining, "draining"),
        (VulcanPluginRuntimeState::UnsupportedPlatform, "failed"),
        (VulcanPluginRuntimeState::Unavailable, "failed"),
    ] {
        assert_eq!(
            inactive_plugin_mcp_status(runtime, VulcanPluginMcpScopeManifest::Conversation),
            expected
        );
    }
}

/// Builds one management observation; `status` selects connectivity and the result has a fixed manifest identity.
/// 构造一个管理观测；`status` 选择连通状态，返回固定清单身份的观测。
fn observation(status: McpRuntimeServerStatus) -> McpRuntimeServerState {
    McpRuntimeServerState {
        id: "vulcan-browser:playwright".into(),
        name: "Playwright".into(),
        source: "plugin".into(),
        plugin_id: Some("vulcan-browser".into()),
        generation: Some(5),
        transport: "stdio".into(),
        endpoint: "playwright-mcp".into(),
        status,
        retry_count: 0,
        protocol_version: None,
        server_product: None,
        server_version: None,
        tool_count: 2,
        prompt_count: 0,
        resource_count: 0,
        last_error: None,
        recent_logs: vec![],
    }
}

/// No inputs; a healthy instance cannot hide an error, and retired generations cannot contaminate the row.
/// 无输入；健康实例不能掩盖错误，退役代次也不能污染当前行。
#[test]
fn plugin_mcp_state_keeps_failures_and_counts_from_one_current_instance() {
    // A previous generation failure and a different server must be excluded by exact identity.
    // 旧代次失败与其他服务器必须按精确身份排除。
    let mut old = observation(McpRuntimeServerStatus::Failed);
    old.generation = Some(4);
    let mut foreign = observation(McpRuntimeServerStatus::Failed);
    foreign.id = "another:server".into();
    let ready = observation(McpRuntimeServerStatus::Connected);
    let mut failed = observation(McpRuntimeServerStatus::Failed);
    failed.last_error = Some("fixture connection failure".into());
    failed.tool_count = 0;
    let mut states = vec![old, foreign, ready.clone(), failed.clone()];
    for _ in 0..2 {
        assert_eq!(
            select_plugin_mcp_state(&states, &ready.id, 5),
            Some(&failed)
        );
        states.reverse();
    }
    assert!(select_plugin_mcp_state(&states, &ready.id, 6).is_none());
    // Duplicate healthy session catalogs remain one server's count rather than being summed.
    // 重复的健康会话目录仍保持单服务器数量，不进行求和。
    assert_eq!(
        select_plugin_mcp_state(&[ready.clone(), ready.clone()], &ready.id, 5),
        Some(&ready)
    );
    for status in [
        McpRuntimeServerStatus::Connecting,
        McpRuntimeServerStatus::Stopping,
        McpRuntimeServerStatus::Disconnected,
    ] {
        let pending = observation(status);
        assert_eq!(
            select_plugin_mcp_state(&[ready.clone(), pending.clone()], &ready.id, 5),
            Some(&pending)
        );
    }
    let mut degraded = ready.clone();
    degraded.last_error = Some("discovery incomplete".into());
    assert_eq!(
        select_plugin_mcp_state(&[degraded.clone(), ready.clone()], &ready.id, 5),
        Some(&degraded)
    );
}
