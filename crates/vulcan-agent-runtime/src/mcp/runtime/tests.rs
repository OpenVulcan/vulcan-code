use super::{
    McpRuntimeAccess, McpRuntimeDiscoveryReport, McpRuntimeEventProjection,
    McpRuntimeEventProjectionKind, McpRuntimeOverview, McpRuntimeOwnedPartitionKey,
    McpRuntimeServerDiagnostic, McpRuntimeServerSnapshot, McpRuntimeServerStatus,
    McpRuntimeSnapshot, McpRuntimeToolAllowlist, McpRuntimeToolDescriptor,
};
use crate::mcp::config::{McpServerConfig, McpServerSource, McpServerTransportConfig};
use crate::model::SessionId;
use std::collections::{BTreeMap, BTreeSet};

/// Builds a minimal server snapshot used as the struct-update base in tests.
/// 构建测试中用于结构体更新语法的最小服务器快照。
fn empty_server_snapshot() -> McpRuntimeServerSnapshot {
    McpRuntimeServerSnapshot {
        inspection_generation: 0,
        name: String::new(),
        enabled: false,
        transport_label: String::new(),
        endpoint_summary: String::new(),
        status: McpRuntimeServerStatus::Disabled,
        last_error: None,
        recent_logs: Vec::new(),
        tools: Vec::new(),
        cached_tools: Vec::new(),
        prompt_count: 0,
        resource_count: 0,
    }
}

#[test]
fn active_summary_rows_only_keep_connecting_and_connected_servers() {
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([
            (
                String::from("connected"),
                McpRuntimeServerSnapshot {
                    name: String::from("Connected Server"),
                    enabled: true,
                    transport_label: String::from("http"),
                    endpoint_summary: String::from("https://connected.test/mcp"),
                    status: McpRuntimeServerStatus::Connected,
                    last_error: None,
                    tools: Vec::new(),
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("connecting"),
                McpRuntimeServerSnapshot {
                    name: String::from("Connecting Server"),
                    enabled: true,
                    transport_label: String::from("stdio"),
                    endpoint_summary: String::from("connecting-mcp"),
                    status: McpRuntimeServerStatus::Connecting,
                    last_error: None,
                    tools: Vec::new(),
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("disconnected"),
                McpRuntimeServerSnapshot {
                    name: String::from("Disconnected Server"),
                    enabled: true,
                    transport_label: String::from("sse"),
                    endpoint_summary: String::from("https://disconnected.test/sse"),
                    status: McpRuntimeServerStatus::Disconnected,
                    last_error: Some(String::from("offline")),
                    tools: Vec::new(),
                    ..empty_server_snapshot()
                },
            ),
        ]),
    };

    assert_eq!(
        snapshot.active_summary_rows(),
        vec![
            String::from("Online · Connected Server · http · https://connected.test/mcp"),
            String::from("Connecting · Connecting Server · stdio · connecting-mcp"),
        ]
    );
}

#[test]
fn enabled_server_diagnostics_only_keep_enabled_servers() {
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([
            (
                String::from("enabled"),
                McpRuntimeServerSnapshot {
                    name: String::from("Enabled Server"),
                    enabled: true,
                    transport_label: String::from("stdio"),
                    endpoint_summary: String::from("enabled-mcp"),
                    status: McpRuntimeServerStatus::Connected,
                    last_error: None,
                    tools: vec![],
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("disabled"),
                McpRuntimeServerSnapshot {
                    name: String::from("Disabled Server"),
                    enabled: false,
                    transport_label: String::from("http"),
                    endpoint_summary: String::from("https://disabled.test/mcp"),
                    status: McpRuntimeServerStatus::Disabled,
                    last_error: None,
                    tools: vec![],
                    ..empty_server_snapshot()
                },
            ),
        ]),
    };

    assert_eq!(
        snapshot.enabled_server_diagnostics(),
        vec![McpRuntimeServerDiagnostic {
            name: String::from("Enabled Server"),
            status: McpRuntimeServerStatus::Connected,
            tool_count: 0,
            prompt_count: 0,
            resource_count: 0,
            last_error: None,
        }]
    );
}

#[test]
fn runtime_server_diagnostic_exposes_stable_summary_and_warning_text() {
    let diagnostic = McpRuntimeServerDiagnostic {
        name: String::from("Enabled Server"),
        status: McpRuntimeServerStatus::Disconnected,
        tool_count: 2,
        prompt_count: 0,
        resource_count: 0,
        last_error: Some(String::from("offline")),
    };

    assert_eq!(
        diagnostic.summary_text(),
        String::from(
            "Server `Enabled Server` is `Offline` with 2 tools, 0 prompts and 0 resources registered."
        )
    );
    assert_eq!(
        diagnostic.warning_text(),
        Some(String::from(
            "Server `Enabled Server` is `Offline` with 2 tools, 0 prompts and 0 resources registered. last error: offline"
        ))
    );
    assert_eq!(
        diagnostic.runtime_event_title(),
        String::from("MCP server offline · Enabled Server")
    );
}

/// Verifies a refresh warning preserves the connected title while remaining a warning projection.
/// 验证刷新警告保留在线标题，同时仍投影为警告级别。
#[test]
fn connected_server_refresh_warning_keeps_online_event_title() {
    // diagnostic represents a usable tool connection with a failed optional-capability refresh.
    // diagnostic 表示工具连接可用但可选能力刷新失败的服务器。
    let diagnostic = McpRuntimeServerDiagnostic {
        name: String::from("Connected Server"),
        status: McpRuntimeServerStatus::Connected,
        tool_count: 2,
        prompt_count: 1,
        resource_count: 3,
        last_error: Some(String::from("prompt refresh failed")),
    };
    // projection is the exact upper-layer event derived from connected status plus warning state.
    // projection 是由连接状态与警告状态共同派生的精确上层事件。
    let projection = diagnostic.to_runtime_event_projection();

    assert_eq!(
        diagnostic.runtime_event_title(),
        String::from("MCP server online · Connected Server")
    );
    assert_eq!(projection.kind, McpRuntimeEventProjectionKind::Warning);
    assert_eq!(
        projection.title,
        String::from("MCP server online · Connected Server")
    );
}

#[test]
fn enabled_server_discovery_report_preserves_rows_warnings_and_event_projections() {
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([
            (
                String::from("healthy"),
                McpRuntimeServerSnapshot {
                    name: String::from("Healthy Server"),
                    enabled: true,
                    transport_label: String::from("http"),
                    endpoint_summary: String::from("https://healthy.test/mcp"),
                    status: McpRuntimeServerStatus::Connected,
                    last_error: None,
                    tools: vec![],
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("offline"),
                McpRuntimeServerSnapshot {
                    name: String::from("Offline Server"),
                    enabled: true,
                    transport_label: String::from("stdio"),
                    endpoint_summary: String::from("offline-mcp"),
                    status: McpRuntimeServerStatus::Disconnected,
                    last_error: Some(String::from("dial tcp timeout")),
                    tools: vec![],
                    ..empty_server_snapshot()
                },
            ),
        ]),
    };

    let report = snapshot.enabled_server_discovery_report();

    assert_eq!(
        report.discovery_rows(),
        &[
            String::from(
                "Server `Healthy Server` is `Online` with 0 tools, 0 prompts and 0 resources registered."
            ),
            String::from(
                "Server `Offline Server` is `Offline` with 0 tools, 0 prompts and 0 resources registered."
            ),
        ]
    );
    assert_eq!(
        report.discovery_errors(),
        &[String::from(
            "Server `Offline Server` is `Offline` with 0 tools, 0 prompts and 0 resources registered. last error: dial tcp timeout"
        )]
    );
    assert_eq!(
        report.discovery_warnings(),
        &[String::from(
            "Server `Offline Server` is `Offline` with 0 tools, 0 prompts and 0 resources registered. last error: dial tcp timeout"
        )]
    );
    assert_eq!(
        report.event_projections(),
        &[
            McpRuntimeEventProjection {
                kind: McpRuntimeEventProjectionKind::Informational,
                title: String::from("MCP server online · Healthy Server"),
                body: String::from(
                    "Server `Healthy Server` is `Online` with 0 tools, 0 prompts and 0 resources registered."
                ),
            },
            McpRuntimeEventProjection {
                kind: McpRuntimeEventProjectionKind::Warning,
                title: String::from("MCP server offline · Offline Server"),
                body: String::from(
                    "Server `Offline Server` is `Offline` with 0 tools, 0 prompts and 0 resources registered. last error: dial tcp timeout"
                ),
            },
        ]
    );
}

#[test]
fn overview_groups_summary_rows_active_rows_and_discovery_report() {
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([
            (
                String::from("healthy"),
                McpRuntimeServerSnapshot {
                    name: String::from("Healthy Server"),
                    enabled: true,
                    transport_label: String::from("http"),
                    endpoint_summary: String::from("https://healthy.test/mcp"),
                    status: McpRuntimeServerStatus::Connected,
                    last_error: None,
                    tools: vec![],
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("offline"),
                McpRuntimeServerSnapshot {
                    name: String::from("Offline Server"),
                    enabled: true,
                    transport_label: String::from("stdio"),
                    endpoint_summary: String::from("offline-mcp"),
                    status: McpRuntimeServerStatus::Disconnected,
                    last_error: Some(String::from("dial tcp timeout")),
                    tools: vec![],
                    ..empty_server_snapshot()
                },
            ),
        ]),
    };

    let overview = snapshot.overview();

    assert_eq!(
        overview.summary_rows(),
        &[
            String::from("Online · Healthy Server · http · https://healthy.test/mcp"),
            String::from("Offline · Offline Server · stdio · offline-mcp · dial tcp timeout"),
        ]
    );
    assert_eq!(
        overview.active_summary_rows(),
        &[String::from(
            "Online · Healthy Server · http · https://healthy.test/mcp"
        )]
    );
    assert_eq!(
        overview.discovery_report().discovery_errors(),
        &[String::from(
            "Server `Offline Server` is `Offline` with 0 tools, 0 prompts and 0 resources registered. last error: dial tcp timeout"
        )]
    );
}

#[test]
fn tool_allowlist_preserves_optional_server_and_tool_sets() {
    let allowlist = McpRuntimeToolAllowlist::new(
        Some(BTreeSet::from([String::from("server-a")])),
        Some(BTreeSet::from([String::from("server-a/read_file")])),
    );

    assert_eq!(
        allowlist.allowed_server_ids(),
        Some(&BTreeSet::from([String::from("server-a")]))
    );
    assert_eq!(
        allowlist.allowed_tool_names(),
        Some(&BTreeSet::from([String::from("server-a/read_file")]))
    );
}

#[test]
fn host_tool_view_preserves_host_visible_descriptor_fields() {
    let descriptor = McpRuntimeToolDescriptor::new(
        String::from("server-a"),
        String::from("read_file"),
        String::from("Read one file"),
        serde_json::json!({"type":"object"}),
    );
    let view = descriptor.host_tool_view(String::from("mcp_server_a_read_file"));

    assert_eq!(view.id(), "mcp_server_a_read_file");
    assert_eq!(view.name(), "mcp_server_a_read_file");
    assert_eq!(view.description(), "Read one file");
    assert_eq!(view.args_schema(), &serde_json::json!({"type":"object"}));
    assert_eq!(view.origin_tool_id(), "server-a/read_file");
}

#[test]
fn filtered_tool_views_apply_server_and_tool_allowlists() {
    let tool_a = McpRuntimeToolDescriptor::new(
        String::from("server-a"),
        String::from("read_file"),
        String::from("Read one file"),
        serde_json::json!({}),
    );
    let tool_b = McpRuntimeToolDescriptor::new(
        String::from("server-b"),
        String::from("edit_file"),
        String::from("Edit one file"),
        serde_json::json!({}),
    );
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([
            (
                String::from("server-a"),
                McpRuntimeServerSnapshot {
                    name: String::from("Server A"),
                    enabled: true,
                    transport_label: String::from("stdio"),
                    endpoint_summary: String::from("server-a"),
                    status: McpRuntimeServerStatus::Connected,
                    last_error: None,
                    tools: vec![tool_a.clone()],
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("server-b"),
                McpRuntimeServerSnapshot {
                    name: String::from("Server B"),
                    enabled: true,
                    transport_label: String::from("http"),
                    endpoint_summary: String::from("server-b"),
                    status: McpRuntimeServerStatus::Connected,
                    last_error: None,
                    tools: vec![tool_b.clone()],
                    ..empty_server_snapshot()
                },
            ),
        ]),
    };

    assert_eq!(
        snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::default()),
        vec![
            tool_a.host_tool_view(String::from("mcp_server_a_read_file")),
            tool_b.host_tool_view(String::from("mcp_server_b_edit_file"))
        ]
    );
    assert_eq!(
        snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::new(
            Some(BTreeSet::from([String::from("server-b")])),
            None,
        )),
        vec![tool_b.host_tool_view(String::from("mcp_server_b_edit_file"))]
    );
    assert_eq!(
        snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::new(
            None,
            Some(BTreeSet::from([String::from("server-a/read_file")])),
        )),
        vec![tool_a.host_tool_view(String::from("mcp_server_a_read_file"))]
    );
}

#[test]
fn filtered_tool_views_apply_numbered_suffixes_for_public_name_collisions() {
    let tool_a = McpRuntimeToolDescriptor::new(
        String::from("server-a"),
        String::from("read.file"),
        String::from("Read one file"),
        serde_json::json!({}),
    );
    let tool_b = McpRuntimeToolDescriptor::new(
        String::from("server-a"),
        String::from("read_file"),
        String::from("Read one file safely"),
        serde_json::json!({}),
    );
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([(
            String::from("server-a"),
            McpRuntimeServerSnapshot {
                name: String::from("Server A"),
                enabled: true,
                transport_label: String::from("stdio"),
                endpoint_summary: String::from("server-a"),
                status: McpRuntimeServerStatus::Connected,
                last_error: None,
                tools: vec![tool_a, tool_b],
                ..empty_server_snapshot()
            },
        )]),
    };

    let views = snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::default());
    assert_eq!(views[0].id(), "mcp_server_a_read_file");
    assert_eq!(views[1].id(), "mcp_server_a_read_file_1");
}

/// Verifies reconnecting or disconnected servers can still expose the last successful tool snapshot to upper host layers.
/// 验证正在重连或已经断开的服务器仍可向上层宿主暴露最后一次成功的工具快照。
#[test]
fn filtered_tool_views_keep_cached_tools_while_server_is_offline() {
    let connecting_tool = McpRuntimeToolDescriptor::new(
        String::from("server-a"),
        String::from("read_file"),
        String::from("Read one file"),
        serde_json::json!({}),
    );
    let disconnected_tool = McpRuntimeToolDescriptor::new(
        String::from("server-b"),
        String::from("edit_file"),
        String::from("Edit one file"),
        serde_json::json!({}),
    );
    let disabled_tool = McpRuntimeToolDescriptor::new(
        String::from("server-c"),
        String::from("delete_file"),
        String::from("Delete one file"),
        serde_json::json!({}),
    );
    let snapshot = McpRuntimeSnapshot {
        servers: BTreeMap::from([
            (
                String::from("server-a"),
                McpRuntimeServerSnapshot {
                    name: String::from("Server A"),
                    enabled: true,
                    transport_label: String::from("stdio"),
                    endpoint_summary: String::from("server-a"),
                    status: McpRuntimeServerStatus::Connecting,
                    cached_tools: vec![connecting_tool.clone()],
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("server-b"),
                McpRuntimeServerSnapshot {
                    name: String::from("Server B"),
                    enabled: true,
                    transport_label: String::from("http"),
                    endpoint_summary: String::from("server-b"),
                    status: McpRuntimeServerStatus::Disconnected,
                    cached_tools: vec![disconnected_tool.clone()],
                    ..empty_server_snapshot()
                },
            ),
            (
                String::from("server-c"),
                McpRuntimeServerSnapshot {
                    name: String::from("Server C"),
                    enabled: false,
                    transport_label: String::from("sse"),
                    endpoint_summary: String::from("server-c"),
                    status: McpRuntimeServerStatus::Disabled,
                    cached_tools: vec![disabled_tool],
                    ..empty_server_snapshot()
                },
            ),
        ]),
    };

    assert_eq!(
        snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::default()),
        vec![
            connecting_tool.host_tool_view(String::from("mcp_server_a_read_file")),
            disconnected_tool.host_tool_view(String::from("mcp_server_b_edit_file")),
        ]
    );
}

/// Verifies owner partitions reuse one exact key, remain isolated across owners, and reopen after revocation.
/// 验证所有者分区复用同一个精确键、在不同所有者之间隔离，并可在撤销后重新打开。
#[tokio::test]
async fn owned_stdio_partitions_reuse_exact_keys_and_reopen_after_revoke() {
    // access contains no ordinary live partitions, so this test exercises only the opt-in owner registry.
    // access 不包含普通实时分区，因此本测试只验证可选的所有者注册表。
    let access = McpRuntimeAccess::new(
        McpRuntimeOverview::new(Vec::new(), Vec::new(), McpRuntimeDiscoveryReport::default()),
        BTreeMap::new(),
    );
    let owner_a = SessionId::from_u128(101);
    let owner_b = SessionId::from_u128(102);
    let server = McpServerConfig {
        source: McpServerSource::Plugin {
            plugin_id: String::from("fixture"),
            generation: 5,
        },
        id: String::from("fixture:stdio"),
        name: String::from("Fixture Stdio"),
        enabled: false,
        transport: McpServerTransportConfig::Stdio {
            command: String::from("unused"),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            inherit_environment: false,
        },
    };

    let first = access
        .open_owned_stdio_partition("fixture", 5, owner_a, vec![server.clone()])
        .await
        .expect("first owner partition should open");
    // The reuse guard must observe the exact published owner lease, not only the browser slot.
    // 复用闸门必须观察精确发布的所有者租约，而不能只依赖浏览器连接槽状态。
    assert!(
        access
            .has_active_owned_stdio_partition("fixture", 5, owner_a)
            .expect("active owner partition query should validate")
    );
    let reused = access
        .open_owned_stdio_partition("fixture", 5, owner_a, vec![server.clone()])
        .await
        .expect("same owner key should reuse");
    assert_eq!(first.key(), reused.key());

    let other = access
        .open_owned_stdio_partition("fixture", 5, owner_b, vec![server.clone()])
        .await
        .expect("different owner should receive a separate partition");
    assert_ne!(first.key(), other.key());
    assert_eq!(first.owner_session_id(), owner_a);
    assert_eq!(other.owner_session_id(), owner_b);

    first.revoke().expect("revoke first owner partition");
    assert!(
        !access
            .has_active_owned_stdio_partition("fixture", 5, owner_a)
            .expect("active owner partition query should validate after revoke")
    );
    assert!(access.owned_stdio_partition("fixture", 5, owner_a).is_err());
    let reopened = access
        .open_owned_stdio_partition("fixture", 5, owner_a, vec![server])
        .await
        .expect("revoked key should reopen with a new lease");
    assert_eq!(reopened.key(), first.key());
    assert!(
        access
            .has_active_owned_stdio_partition("fixture", 5, owner_a)
            .expect("reopened owner partition should be queryable")
    );

    other.shutdown().expect("shutdown second owner partition");
    reopened
        .shutdown()
        .expect("shutdown reopened owner partition");
}

/// Verifies owner partition input rejects foreign sources, non-stdio transports, and nil owners before launch.
/// 验证所有者分区输入会在启动前拒绝外部来源、非 stdio 传输及空所有者。
#[tokio::test]
async fn owned_stdio_partition_validates_source_transport_and_owner() {
    let access = McpRuntimeAccess::new(
        McpRuntimeOverview::new(Vec::new(), Vec::new(), McpRuntimeDiscoveryReport::default()),
        BTreeMap::new(),
    );
    let owner = SessionId::from_u128(103);
    let user_server = McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: String::from("foreign"),
        name: String::from("Foreign"),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: String::from("must-not-launch"),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            inherit_environment: true,
        },
    };
    let source_error = access
        .open_owned_stdio_partition("fixture", 6, owner, vec![user_server])
        .await
        .expect_err("foreign source must fail before launch");
    assert!(source_error.contains("another source"));

    let http_server = McpServerConfig {
        source: McpServerSource::Plugin {
            plugin_id: String::from("fixture"),
            generation: 6,
        },
        id: String::from("fixture:http"),
        name: String::from("Fixture HTTP"),
        enabled: true,
        transport: McpServerTransportConfig::Http {
            url: String::from("https://example.test/mcp"),
            bearer_token: None,
            headers: BTreeMap::new(),
            headers_from_env: BTreeMap::new(),
            oauth: None,
        },
    };
    let transport_error = access
        .open_owned_stdio_partition("fixture", 6, owner, vec![http_server])
        .await
        .expect_err("non-stdio transport must fail before launch");
    assert!(transport_error.contains("stdio servers only"));

    let nil_owner_error = McpRuntimeOwnedPartitionKey::try_new("fixture", 6, SessionId::nil())
        .expect_err("nil owner must be rejected");
    assert!(nil_owner_error.contains("non-nil"));
}
