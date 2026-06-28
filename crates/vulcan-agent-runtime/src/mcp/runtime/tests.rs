use super::{
    McpRuntimeEventProjection, McpRuntimeEventProjectionKind, McpRuntimeServerDiagnostic,
    McpRuntimeServerSnapshot, McpRuntimeServerStatus, McpRuntimeSnapshot, McpRuntimeToolAllowlist,
    McpRuntimeToolDescriptor,
};
use std::collections::{BTreeMap, BTreeSet};

/// Builds a minimal server snapshot used as the struct-update base in tests.
/// 构建测试中用于结构体更新语法的最小服务器快照。
fn empty_server_snapshot() -> McpRuntimeServerSnapshot {
    McpRuntimeServerSnapshot {
        name: String::new(),
        enabled: false,
        transport_label: String::new(),
        endpoint_summary: String::new(),
        status: McpRuntimeServerStatus::Disabled,
        last_error: None,
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
            String::from("在线 · Connected Server · http · https://connected.test/mcp"),
            String::from("连接中 · Connecting Server · stdio · connecting-mcp"),
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
            "服务器 `Enabled Server` 当前状态为 `离线`，已注册 2 个工具、0 个 prompt 与 0 个资源。"
        )
    );
    assert_eq!(
        diagnostic.warning_text(),
        Some(String::from(
            "服务器 `Enabled Server` 当前状态为 `离线`，已注册 2 个工具、0 个 prompt 与 0 个资源。 最近一次错误：offline"
        ))
    );
    assert_eq!(
        diagnostic.runtime_event_title(),
        String::from("MCP 服务器离线 · Enabled Server")
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
                "服务器 `Healthy Server` 当前状态为 `在线`，已注册 0 个工具、0 个 prompt 与 0 个资源。"
            ),
            String::from(
                "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具、0 个 prompt 与 0 个资源。"
            ),
        ]
    );
    assert_eq!(
        report.discovery_errors(),
        &[String::from(
            "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具、0 个 prompt 与 0 个资源。 最近一次错误：dial tcp timeout"
        )]
    );
    assert_eq!(
        report.discovery_warnings(),
        &[String::from(
            "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具、0 个 prompt 与 0 个资源。 最近一次错误：dial tcp timeout"
        )]
    );
    assert_eq!(
        report.event_projections(),
        &[
            McpRuntimeEventProjection {
                kind: McpRuntimeEventProjectionKind::Informational,
                title: String::from("MCP 服务器在线 · Healthy Server"),
                body: String::from(
                    "服务器 `Healthy Server` 当前状态为 `在线`，已注册 0 个工具、0 个 prompt 与 0 个资源。"
                ),
            },
            McpRuntimeEventProjection {
                kind: McpRuntimeEventProjectionKind::Warning,
                title: String::from("MCP 服务器离线 · Offline Server"),
                body: String::from(
                    "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具、0 个 prompt 与 0 个资源。 最近一次错误：dial tcp timeout"
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
            String::from("在线 · Healthy Server · http · https://healthy.test/mcp"),
            String::from("离线 · Offline Server · stdio · offline-mcp · dial tcp timeout"),
        ]
    );
    assert_eq!(
        overview.active_summary_rows(),
        &[String::from(
            "在线 · Healthy Server · http · https://healthy.test/mcp"
        )]
    );
    assert_eq!(
        overview.discovery_report().discovery_errors(),
        &[String::from(
            "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具、0 个 prompt 与 0 个资源。 最近一次错误：dial tcp timeout"
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
