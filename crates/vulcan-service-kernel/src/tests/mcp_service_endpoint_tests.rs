use super::*;
use serde_json::{Value, json};
use vulcan_provider_config::provider_config::{
    McpProjectScope, McpServiceClient, McpSessionGrant, PersistedMcpServiceConfiguration,
};

// READER_TOKEN signs in the read-only client limited to `project-a`.
// READER_TOKEN 用于登录只读且限于 `project-a` 的客户端。
const READER_TOKEN: &str = "vmcp_kernel_reader";

/// Exercises per-library read and write grants through the MCP endpoint and SQLite store.
/// 通过 MCP 端点与 SQLite 仓库检验逐库读写授权。
#[tokio::test(flavor = "current_thread")]
async fn external_knowledge_tools_enforce_each_library_grant() {
    use vulcan_workspace_store::data_plane::prepare_application_schema_for_test;
    use vulcan_workspace_store::knowledge_base::{
        KnowledgeBaseRuntime, KnowledgeIndexMode, KnowledgeLibrarySettings,
    };
    use vulcan_workspace_store::{local_database_runtime::LogicalDatabase, local_sql};

    // Keep all storage and file side effects inside the test-owned controller.
    // 将所有存储与文件副作用限制在测试所拥有的控制器内。
    let root = TestHostRuntimeRoot::new(SessionId::new_v4());
    let controller = root.attached_host_runtime_handle().await;
    prepare_application_schema_for_test(&controller)
        .await
        .expect("private schema");
    local_sql::execute_script_async(
        &controller.local,
        LogicalDatabase::AppState,
        "INSERT INTO projects(id,name,root_path,created_at,updated_at) VALUES('project-a','Test','test','','');",
        vec![],
    )
    .await
    .expect("test project");
    let database = KnowledgeBaseRuntime::new(controller.clone());
    let settings = KnowledgeLibrarySettings {
        index_mode: KnowledgeIndexMode::Browse,
        embedding: None,
    };
    let granted = database
        .create_library(
            "Granted",
            "Fixture reference",
            &["project-a".into()],
            &settings,
        )
        .await
        .expect("granted library");
    let hidden = database
        .create_library(
            "Hidden",
            "Fixture reference",
            &["project-a".into()],
            &settings,
        )
        .await
        .expect("hidden library");
    let state = mcp_test_state();
    {
        let mut guard = state.lock().expect("state");
        guard.controller_runtime = Some(controller);
        guard
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .task_projects
            .push(vulcan_workbench_model::TaskProjectSummary::new(
                "project-a",
                "Test",
                root.data_root().to_string_lossy().into_owned(),
                false,
                0,
                None,
                None,
                Vec::new(),
            ));
        let client = &mut guard
            .agent_runtime
            .as_mut()
            .unwrap()
            .provider_configuration
            .mcp_service
            .clients[0];
        client.tool_ids = Some(vec![
            "knowledge_list".into(),
            "knowledge_read".into(),
            "knowledge_write".into(),
        ]);
        client.knowledge_libraries.insert(granted.id.clone(), false);
    }
    let server =
        crate::mcp_service::install_mcp_server(ServiceEventEmitter::new(Vec::new()), &state)
            .expect("install MCP server");
    let (_, listed) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"knowledge_list","arguments":{"project_id":"project-a"},
        }),
    )
    .await;
    let visible = structured(&listed).to_string();
    assert!(visible.contains(&granted.id));
    assert!(!visible.contains(&hidden.id));

    let (_, denied_read) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"knowledge_list","arguments":{"project_id":"project-a","libraryId":hidden.id},
        }),
    )
    .await;
    assert_eq!(denied_read["result"]["isError"], true);

    let write = json!({
        "name":"knowledge_write", "arguments":{
            "project_id":"project-a","libraryId":granted.id,"action":"create",
            "path":"note.md","title":"Note","summary":"Fixture","body":"mcp write fixture",
            "provenance":[{"kind":"session","ref":"fixture","observedAtMs":1}],
        },
    });
    let (_, denied_write) = post(&server, READER_TOKEN, "tools/call", write.clone()).await;
    assert_eq!(denied_write["result"]["isError"], true);
    state
        .lock()
        .unwrap()
        .agent_runtime
        .as_mut()
        .unwrap()
        .provider_configuration
        .mcp_service
        .clients[0]
        .knowledge_libraries
        .insert(granted.id.clone(), true);
    let (_, written) = post(&server, READER_TOKEN, "tools/call", write).await;
    assert_ne!(written["result"]["isError"], true, "{written}");
    let (_, read) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"knowledge_read", "arguments":{
                "project_id":"project-a","libraryId":granted.id,"path":"note.md",
            },
        }),
    )
    .await;
    assert!(structured(&read).to_string().contains("mcp write fixture"));
    let changes = database
        .list_changes(&granted.id, None, 10)
        .await
        .expect("persisted changes");
    let actor = &changes
        .iter()
        .find(|change| change.operation == "create")
        .expect("write history")
        .actor;
    assert!(actor.session_id.is_none());
    assert!(
        actor
            .tool_call_id
            .as_deref()
            .is_some_and(|id| id.starts_with("mcp:reader:"))
    );
}

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
            tool_ids: None,
            tool_project_ids: Vec::new(),
            knowledge_libraries: Default::default(),
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
            session_sort_mode: Default::default(),
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
                created_at: String::from("2026-10-22T00:00:00Z"),
                last_request_at: String::from("2026-10-22T00:00:00Z"),
                manual_position: None,
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

/// Exercises authorization, native file handling, and single-call Shell on the external endpoint.
/// 在对外端点验证授权、原生文件处理和单次 Shell 调用。
#[tokio::test(flavor = "current_thread")]
async fn external_workspace_tools_follow_individual_grants() {
    // Each synthetic project has a real but isolated directory.
    // 每个合成项目使用真实且独立的目录。
    let workspace = tempfile::tempdir().expect("workspace");
    let allowed = workspace.path().join("allowed");
    let denied = workspace.path().join("denied");
    for root in [&allowed, &denied] {
        std::fs::create_dir(root).expect("project root");
        std::fs::write(root.join("note.txt"), "fixture marker").expect("source file");
    }
    let state = mcp_test_state();
    {
        let mut guard = state.lock().expect("state");
        for (id, root) in [("project-a", &allowed), ("project-b", &denied)] {
            guard
                .workspace_snapshot
                .as_mut()
                .unwrap()
                .task_projects
                .push(vulcan_workbench_model::TaskProjectSummary::new(
                    id,
                    id,
                    root.to_string_lossy().into_owned(),
                    false,
                    0,
                    None,
                    None,
                    Vec::new(),
                ));
        }
        let client = &mut guard
            .agent_runtime
            .as_mut()
            .unwrap()
            .provider_configuration
            .mcp_service
            .clients[0];
        client.tool_ids = Some(vec![
            "file".into(),
            "file_read".into(),
            "file_write".into(),
            "shell_command".into(),
        ]);
        client.tool_project_ids = vec!["project-a".into()];
    }
    let server =
        crate::mcp_service::install_mcp_server(ServiceEventEmitter::new(Vec::new()), &state)
            .expect("install MCP server");

    let (_, listed) = post(&server, READER_TOKEN, "tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().expect("tool list");
    let shell = tools
        .iter()
        .find(|tool| tool["name"] == "shell_command")
        .expect("selected Shell tool");
    assert!(
        shell["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("cwd"))
    );
    assert!(
        shell["inputSchema"]["properties"]
            .get("background")
            .is_none()
    );
    assert!(!tools.iter().any(|tool| matches!(
        tool["name"].as_str(),
        Some("shell_job" | "terminal_session" | "list_sessions" | "file_delete")
    )));

    let (_, read) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"file_read", "arguments":{"project_id":"project-a","path":"note.txt"},
        }),
    )
    .await;
    assert!(structured(&read).to_string().contains("fixture marker"));
    for (name, arguments) in [
        (
            "file_read",
            json!({"project_id":"project-b","path":"note.txt"}),
        ),
        (
            "file_read",
            json!({"cwd":denied.to_string_lossy(),"path":"note.txt"}),
        ),
        (
            "file_read",
            json!({"project_id":"project-a","path": denied.join("note.txt").to_string_lossy()}),
        ),
        (
            "file",
            json!({"project_id":"project-a","path":"note.txt","action":"delete"}),
        ),
        (
            "file_delete",
            json!({"project_id":"project-a","path":"note.txt"}),
        ),
        (
            "shell_command",
            json!({"command":"echo fixture","title":"Fixture","cwd":denied.to_string_lossy()}),
        ),
        (
            "shell_command",
            json!({"command":"echo fixture","title":"Fixture","cwd":allowed.to_string_lossy(),"background":true}),
        ),
    ] {
        let (_, rejected) = post(
            &server,
            READER_TOKEN,
            "tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await;
        assert_eq!(rejected["result"]["isError"], true, "{name} must fail");
    }

    let (_, ran) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"shell_command",
        "arguments":{"command":"echo mcp-shell-fixture","title":"Fixture","cwd":allowed.to_string_lossy(),"timeout_ms":10000},
    })).await;
    let result = structured(&ran);
    assert_eq!(result["status"], "completed");
    assert!(
        result["stdout"]
            .as_str()
            .unwrap()
            .contains("mcp-shell-fixture")
    );
    assert!(result.get("job_id").is_none());

    // Removing the optional restriction allows standalone calls even without a project registry.
    // 清除可选限制后，独立调用不依赖项目登记表。
    {
        let mut guard = state.lock().unwrap();
        guard.workspace_snapshot = None;
        guard
            .agent_runtime
            .as_mut()
            .unwrap()
            .provider_configuration
            .mcp_service
            .clients[0]
            .tool_project_ids
            .clear();
    }
    let (_, listed) = post(&server, READER_TOKEN, "tools/list", json!({})).await;
    let listed = listed["result"]["tools"]
        .as_array()
        .expect("unrestricted tools");
    let file_read = listed
        .iter()
        .find(|tool| tool["name"] == "file_read")
        .expect("file_read is available without a registered project");
    assert_eq!(
        file_read["inputSchema"]["properties"]["cwd"]["type"],
        "string"
    );
    let raw_read =
        json!({"name":"file_read", "arguments":{"cwd":denied.to_string_lossy(),"path":"note.txt"}});
    let (_, read) = post(&server, READER_TOKEN, "tools/call", raw_read.clone()).await;
    assert!(structured(&read).to_string().contains("fixture marker"));
    let (_, ran) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"shell_command",
        "arguments":{"command":"echo unrestricted-fixture","title":"Fixture","cwd":denied.to_string_lossy(),"timeout_ms":10000},
    })).await;
    assert_eq!(structured(&ran)["status"], "completed");
    assert!(
        structured(&ran)["stdout"]
            .as_str()
            .unwrap()
            .contains("unrestricted-fixture")
    );

    // A caller-selected directory does not bypass the Git metadata write protection.
    // 调用方提供目录也不能绕过 Git 元数据写入保护。
    let git_data = denied.join(".git");
    std::fs::create_dir(&git_data).expect("metadata directory");
    let (_, protected) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"file_write","arguments":{"cwd":git_data.to_string_lossy(),"path":"config","content":"fixture"},
    })).await;
    assert_eq!(protected["result"]["isError"], true);
    assert!(!git_data.join("config").exists());

    // Revocation affects a direct call without requiring a new token or tools/list.
    // 撤销直接影响下一次调用，无需刷新令牌或重新获取工具目录。
    state
        .lock()
        .unwrap()
        .agent_runtime
        .as_mut()
        .unwrap()
        .provider_configuration
        .mcp_service
        .clients[0]
        .tool_ids = Some(vec![]);
    let (_, revoked) = post(&server, READER_TOKEN, "tools/call", raw_read).await;
    assert_eq!(revoked["result"]["isError"], true);
}
