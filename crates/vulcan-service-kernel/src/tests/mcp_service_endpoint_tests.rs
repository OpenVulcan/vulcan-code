use super::*;
use serde_json::{Value, json};
use vulcan_provider_config::provider_config::{
    McpProjectScope, McpServiceClient, McpSessionGrant, PersistedMcpServiceConfiguration,
};

// READER_TOKEN signs in the read-only client limited to `project-a`.
// READER_TOKEN 用于登录只读且限于 `project-a` 的客户端。
const READER_TOKEN: &str = "vmcp_kernel_reader";

/// Builds a live service state with two projects, one session in each, and one read-only MCP
/// client scoped to `project-a`.
/// 构建一个实时服务状态：两个项目、各一个会话，以及一个限于 `project-a` 的只读 MCP 客户端。
fn mcp_test_state() -> SharedServiceBridgeState {
    let mut service_state = test_service_state_with_parent_session(SessionId::new_v4());
    let mut runtime = test_agent_runtime_bridge();
    runtime.provider_configuration.mcp_service = PersistedMcpServiceConfiguration {
        enabled: true,
        clients: vec![McpServiceClient {
            id: String::from("reader"),
            name: String::from("Reader"),
            token_sha256: vulcan_mcp_service::token_sha256_hex(READER_TOKEN),
            token_hint: String::from("ader"),
            created_at_ms: 1,
            enabled: true,
            sessions: McpSessionGrant {
                read: true,
                projects: McpProjectScope::Selected {
                    project_ids: vec![String::from("project-a")],
                },
                ..McpSessionGrant::default()
            },
            media_targets: Vec::new(),
        }],
    };
    service_state.agent_runtime = Some(runtime);
    for (project, session) in [("project-a", "session-a"), ("project-b", "session-b")] {
        service_state.workbench.projects.push(ProjectSummary {
            id: project.to_string(),
            name: project.to_string(),
            path: format!("D:/{project}"),
            pinned: false,
            icon_pack: None,
            icon_key: None,
        });
        service_state
            .workbench
            .conversations
            .push(ConversationSummary {
                id: session.to_string(),
                title: session.to_string(),
                status: String::from("idle"),
                run_phase: String::from("Idle"),
                last_updated_label: String::from("now"),
                last_updated_at: String::from("2026-10-22T00:00:00Z"),
                unread: false,
                attention: None,
                project_id: Some(project.to_string()),
                dedicated: false,
                assistant_name: None,
            });
    }
    Arc::new(Mutex::new(service_state))
}

/// Posts one JSON-RPC request with `token` and returns the HTTP status and parsed body.
/// 以 `token` 发送一条 JSON-RPC 请求，返回 HTTP 状态与解析后的响应体。
async fn post(
    server: &Arc<vulcan_mcp_service::McpServer>,
    token: &str,
    method: &str,
    params: Value,
) -> (u16, Value) {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let response = server
        .handle_post(
            Some(&format!("Bearer {token}")),
            &serde_json::to_vec(&body).expect("request should serialize"),
        )
        .await;
    let value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    (response.status, value)
}

/// Returns the structured content of a successful tool call, failing on tool errors.
/// 返回成功工具调用的结构化内容；工具出错时失败。
fn structured(result: &Value) -> &Value {
    assert_eq!(
        result["result"]["isError"], false,
        "tool call failed: {result}"
    );
    &result["result"]["structuredContent"]
}

// The endpoint runs on the kernel backend: tokens authenticate, tools follow the grants, and
// sessions outside the project scope are invisible.
// 端点运行在内核后端之上：令牌完成认证，工具跟随授权，项目范围外的会话不可见。
#[tokio::test(flavor = "current_thread")]
async fn kernel_backend_serves_only_the_granted_scope() {
    let state = mcp_test_state();
    let server =
        crate::mcp_service::install_mcp_server(ServiceEventEmitter::new(Vec::new()), &state)
            .expect("server should install");

    let (status, _) = post(&server, "vmcp_wrong", "tools/list", json!({})).await;
    assert_eq!(status, 401);

    let (status, listed) = post(&server, READER_TOKEN, "tools/list", json!({})).await;
    assert_eq!(status, 200);
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools should be listed")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"list_sessions"));
    assert!(!names.contains(&"send_message"));
    assert!(!names.contains(&"generate_media"));

    let (_, projects) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({ "name": "list_projects", "arguments": {} }),
    )
    .await;
    let projects = structured(&projects).to_string();
    assert!(projects.contains("project-a"));
    assert!(!projects.contains("project-b"));

    let (_, sessions) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({ "name": "list_sessions", "arguments": {} }),
    )
    .await;
    let sessions = structured(&sessions).to_string();
    assert!(sessions.contains("session-a"));
    assert!(!sessions.contains("session-b"));

    let (_, hidden) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({ "name": "get_session", "arguments": { "session_id": "session-b" } }),
    )
    .await;
    assert_eq!(hidden["result"]["isError"], true);

    let (_, refused) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({ "name": "send_message", "arguments": { "session_id": "session-a", "message": "hi" } }),
    )
    .await;
    assert_eq!(refused["result"]["isError"], true);
}
