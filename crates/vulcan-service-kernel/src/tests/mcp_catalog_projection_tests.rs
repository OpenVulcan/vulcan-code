//! MCP runtime catalog projection regressions.
//! MCP 运行时目录投影回归测试。

use super::*;
use std::collections::BTreeMap;

/// Builds one user-configured stdio MCP server whose launch command cannot exist.
/// 构建一个用户配置、但启动命令必然不存在的 stdio MCP 服务器。
///
/// The service used to own a second MCP transport that connected every configured server on its
/// own. The fixture keeps the same failure surface (one configured, never connected server) while
/// proving the UI catalog is derived from the single Agent runtime instead.
/// 服务过去自有第二套 MCP 传输，会自行连接每个已配置服务器。该夹具保持同样的失败表面
/// （一个已配置、永不连接的服务器），同时证明界面目录派生自唯一的 Agent 运行时。
fn unlaunchable_user_mcp_server() -> vulcan_agent_runtime::mcp::McpServerConfig {
    vulcan_agent_runtime::mcp::McpServerConfig {
        source: vulcan_agent_runtime::mcp::McpServerSource::UserConfigured,
        id: String::from("user-fixture"),
        name: String::from("Fixture MCP Server"),
        enabled: true,
        transport: vulcan_agent_runtime::mcp::McpServerTransportConfig::Stdio {
            command: String::from("vulcan-mcp-fixture-missing-executable"),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            inherit_environment: false,
        },
    }
}

/// Verifies the UI MCP catalog is projected from the single Agent MCP runtime generation.
/// 验证界面 MCP 目录从唯一的 Agent MCP 运行时代次投影。
///
/// The service owns no second transport, so every UI surface reads the same access the AI tool
/// loop invokes, and a new published generation replaces the catalog atomically.
/// 服务不持有第二套传输，因此每个界面表面读取的都是 AI 工具循环调用的同一个访问面，
/// 且新发布的代次原子替换目录。
#[tokio::test]
async fn ui_mcp_catalog_follows_the_single_agent_runtime_generation() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);
    // initial_runtime is the agent runtime whose MCP access owns no user server yet.
    // initial_runtime 是其 MCP 访问面尚无用户服务器的代理运行时。
    let mut bridge = test_agent_runtime_bridge();
    assert_eq!(
        mcp_runtime_catalog_from_state(&state).configured_server_count,
        0,
        "a service state without an agent runtime must project an empty MCP catalog"
    );
    // configured_access owns exactly one user partition for the fixture server.
    // configured_access 为该夹具服务器持有恰好一个用户分区。
    let configured_access =
        vulcan_agent_runtime::mcp::bootstrap_mcp_runtime_access(vec![unlaunchable_user_mcp_server()])
            .await
            .expect("fixture MCP runtime access should bootstrap");
    bridge.extension_generation = Arc::new(ExtensionGeneration::new(
        1,
        None,
        configured_access.clone(),
        None,
    ));
    state.agent_runtime = Some(bridge);

    let projected = mcp_runtime_catalog_from_state(&state);
    assert_eq!(projected.configured_server_count, 1);
    assert_eq!(projected.enabled_server_count, 1);
    assert_eq!(
        projected.connected_server_count, 0,
        "an unlaunchable fixture server must never be reported as connected"
    );
    // projected_equals_runtime proves the UI reads the exact agent runtime access surface.
    // projected_equals_runtime 证明界面读取的正是代理运行时访问面本身。
    let runtime_catalog = state
        .agent_runtime
        .as_ref()
        .map(|agent_runtime| {
            vulcan_service_runtime::mcp_bridge::mcp_runtime_catalog_from_access(
                &agent_runtime.extension_generation.mcp_runtime(),
            )
        })
        .expect("agent runtime should be installed");
    assert_eq!(
        projected.configured_server_count, runtime_catalog.configured_server_count
    );
    assert_eq!(projected.summary_rows, runtime_catalog.summary_rows);
    assert_eq!(projected.tools.len(), runtime_catalog.tools.len());
    assert!(
        projected.summary_rows[0].contains("Fixture MCP Server"),
        "the projected summary row must name the exact configured server"
    );

    // A published replacement generation replaces the catalog without any cached stale view.
    // 已发布的替换代次在无任何过期缓存视图的情况下替换目录。
    {
        let agent_runtime = state
            .agent_runtime
            .as_mut()
            .expect("agent runtime should remain installed");
        let next_generation = Arc::new(ExtensionGeneration::new(
            agent_runtime.extension_generation.revision().saturating_add(1),
            None,
            vulcan_agent_runtime::mcp::configured_mcp_runtime_access(
                &agent_runtime.provider_configuration,
            ),
            None,
        ));
        agent_runtime.extension_generation = next_generation;
    }
    let replaced = mcp_runtime_catalog_from_state(&state);
    assert_eq!(replaced.configured_server_count, 0);
    assert!(replaced.summary_rows.is_empty());
    assert!(replaced.tools.is_empty());

    // Stopping the retired runtime leaves no connection behind for the removed server.
    // 停止已退休的运行时不会为被移除的服务器留下任何连接。
    configured_access.shutdown();
}
