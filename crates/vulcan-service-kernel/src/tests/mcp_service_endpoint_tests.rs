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
    let storage = tempfile::tempdir().expect("isolated MCP storage");
    let server = crate::mcp_service::install_mcp_server_in(
        ServiceEventEmitter::new(Vec::new()),
        &state,
        storage.path().to_path_buf(),
    )
    .expect("install MCP server");
    let (_, sid) = new_context(&server, root.data_root()).await;
    let (_, listed) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"knowledge_list","arguments":{"sid":sid},
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
            "name":"knowledge_list","arguments":{"sid":sid,"libraryId":hidden.id},
        }),
    )
    .await;
    assert_eq!(denied_read["result"]["isError"], true);

    let write = json!({
        "name":"knowledge_write", "arguments":{
            "sid":sid,"libraryId":granted.id,"action":"create",
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
                "sid":sid,"libraryId":granted.id,"path":"note.md",
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
            record_calls: false,
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
                tool_request_started_at_ms: None,
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

/// Creates a live MCP project and an annotated session for this test client.
/// 为此测试客户端创建活跃 MCP 项目及带备注的会话。
async fn new_context(
    server: &Arc<vulcan_mcp_service::McpServer>,
    path: &std::path::Path,
) -> (String, String) {
    let (_, project) = post(server, READER_TOKEN, "tools/call", json!({
        "name":"vc_mcp_project","arguments":{"type":"new","name":"Test","path":path,"note":"Project fixture"}
    })).await;
    let project_id = structured(&project)["project_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, session) = post(server, READER_TOKEN, "tools/call", json!({
        "name":"vc_mcp_session","arguments":{"type":"new","project_id":project_id,"note":"Session fixture"}
    })).await;
    let sid = structured(&session)["sid"].as_str().unwrap().to_owned();
    assert_eq!(structured(&session)["note"], "Session fixture");
    assert!(structured(&session).get("uuid").is_none());
    (project_id, sid)
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
        let mut other = client.clone();
        other.id = "other".into();
        other.token_sha256 = vulcan_mcp_service::token_sha256_hex("vmcp_kernel_other");
        guard
            .agent_runtime
            .as_mut()
            .unwrap()
            .provider_configuration
            .mcp_service
            .clients
            .push(other);
    }
    let storage = tempfile::tempdir().expect("isolated MCP storage");
    let server = crate::mcp_service::install_mcp_server_in(
        ServiceEventEmitter::new(Vec::new()),
        &state,
        storage.path().to_path_buf(),
    )
    .expect("install MCP server");
    let (_, scoped_sid) = new_context(&server, &allowed).await;
    let (_, foreign) = post(
        &server,
        "vmcp_kernel_other",
        "tools/call",
        json!({
            "name":"file_read","arguments":{"sid":scoped_sid,"path":"note.txt"}
        }),
    )
    .await;
    assert_eq!(foreign["result"]["isError"], true);
    let (_, denied_project) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"vc_mcp_project", "arguments":{"type":"new","name":"Denied","path":denied}
        }),
    )
    .await;
    assert_eq!(denied_project["result"]["isError"], true);

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
            .contains(&json!("sid"))
    );
    assert!(shell["inputSchema"]["properties"].get("cwd").is_none());
    assert!(!tools.iter().any(|tool| matches!(
        tool["name"].as_str(),
        Some("shell_job" | "terminal_session" | "list_sessions" | "file_delete")
    )));

    let (_, read) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"file_read", "arguments":{"sid":scoped_sid,"path":"note.txt"},
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
            json!({"sid":scoped_sid,"path": denied.join("note.txt").to_string_lossy()}),
        ),
        (
            "file",
            json!({"sid":scoped_sid,"path":"note.txt","action":"delete"}),
        ),
        ("file_delete", json!({"sid":scoped_sid,"path":"note.txt"})),
        (
            "shell_command",
            json!({"sid":scoped_sid,"command":"echo fixture","title":"Fixture","cwd":denied.to_string_lossy()}),
        ),
        (
            "shell_command",
            json!({"sid":scoped_sid,"command":"echo fixture","title":"Fixture","background":true}),
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
        "arguments":{"sid":scoped_sid,"command":"echo mcp-shell-fixture","title":"Fixture","timeout_ms":10000},
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
    assert!(!storage.path().join("calls.sqlite3").exists());

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
    let (project_id, sid) = new_context(&server, &denied).await;
    let (_, listed) = post(&server, READER_TOKEN, "tools/list", json!({})).await;
    let listed = listed["result"]["tools"]
        .as_array()
        .expect("unrestricted tools");
    let file_read = listed
        .iter()
        .find(|tool| tool["name"] == "file_read")
        .expect("file_read is available without a registered project");
    assert_eq!(
        file_read["inputSchema"]["properties"]["sid"]["type"],
        "string"
    );
    let raw_read = json!({"name":"file_read", "arguments":{"sid":sid,"path":"note.txt"}});
    let (_, read) = post(&server, READER_TOKEN, "tools/call", raw_read.clone()).await;
    assert!(structured(&read).to_string().contains("fixture marker"));
    let (_, ran) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"shell_command",
        "arguments":{"sid":sid,"command":"echo unrestricted-fixture","title":"Fixture","timeout_ms":10000},
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
    let (_, git_sid) = new_context(&server, &git_data).await;
    let (_, protected) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"file_write","arguments":{"sid":git_sid,"path":"config","content":"fixture"},
        }),
    )
    .await;
    assert_eq!(protected["result"]["isError"], true);
    assert!(!git_data.join("config").exists());

    // Background jobs and audit records share the session but keep the UUID private.
    // 后台任务和审计记录共用会话，同时保持 UUID 私有。
    {
        let mut guard = state.lock().unwrap();
        let client = &mut guard
            .agent_runtime
            .as_mut()
            .unwrap()
            .provider_configuration
            .mcp_service
            .clients[0];
        client.record_calls = true;
        client
            .tool_ids
            .as_mut()
            .unwrap()
            .extend(["shell_job".into(), "terminal_session".into()]);
    }
    let (_, timed_out) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name": "shell_command",
            "arguments": {
                "sid": sid,
                "command": if cfg!(windows) {
                    "powershell.exe -NoProfile -Command \"Start-Sleep -Seconds 60\""
                } else {
                    "sleep 60"
                },
                "title": "Command timeout fixture",
                "timeout_ms": 100
            }
        }),
    )
    .await;
    assert_eq!(structured(&timed_out)["status"], "timed_out");

    // Drop an in-flight request after its command starts, then reuse the same client and project.
    // 在命令开始后取消进行中的请求，再用同一客户端查询原项目。
    let started_marker = denied.join("mcp-shell-started.txt");
    let in_flight = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name": "shell_command",
            "arguments": {
                "sid": sid,
                "command": if cfg!(windows) {
                    "powershell.exe -NoProfile -Command 'Set-Content -LiteralPath mcp-shell-started.txt -Value $PID; Start-Sleep -Seconds 60'"
                } else {
                    "echo $$ > mcp-shell-started.txt; sleep 60"
                },
                "shell": if cfg!(windows) { "powershell" } else { "sh" },
                "title": "Disconnected command fixture",
                "timeout_ms": 1_500_000
            }
        }),
    );
    let descendant_pid = tokio::select! {
        response = in_flight => panic!("foreground command returned before disconnection: {response:?}"),
        ready = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Ok(marker) = std::fs::read_to_string(&started_marker)
                    && let Ok(pid) = marker.trim().parse::<u32>()
                {
                    break pid;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }) => ready.expect("foreground command should start"),
    };
    let (_, projects) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        post(
            &server,
            READER_TOKEN,
            "tools/call",
            json!({"name":"vc_mcp_project","arguments":{"type":"list"}}),
        ),
    )
    .await
    .expect("project listing should respond after a foreground request disconnects");
    assert!(
        structured(&projects)["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["project_id"] == project_id)
    );

    // On Windows, observe the descendant rather than the direct process reaped by Tokio.
    // 在 Windows 上检查后代进程，避开 Tokio 已回收的直接子进程。
    if cfg!(windows) {
        let pid_filter = format!("PID eq {descendant_pid}");
        let quoted_pid = format!("\"{descendant_pid}\"");
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let output = vulcan_host_tools::process_creation::new_background_tokio_command(
                    "tasklist.exe",
                )
                .args(["/FI", &pid_filter, "/FO", "CSV", "/NH"])
                .output()
                .await
                .expect("list fixture descendant");
                assert!(output.status.success());
                if !String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.split(',').nth(1) == Some(quoted_pid.as_str()))
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("disconnect should terminate the command descendant");
    }

    let (_, started) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"shell_command","arguments":{"sid":sid,"command":"echo audited-fixture","title":"Fixture","background":true,"timeout_ms":1_500_000}
    })).await;
    assert_eq!(structured(&started)["status"], "running");
    let job_id = structured(&started)["job_id"].as_str().unwrap();
    let (_, complete) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"shell_job","arguments":{"sid":sid,"action":"wait","job_id":job_id,"wait_seconds":5}
    })).await;
    assert_eq!(structured(&complete)["status"], "completed");
    assert!(
        structured(&complete)["stdout"]
            .as_str()
            .unwrap()
            .contains("audited-fixture")
    );

    // Start a real interactive terminal while another MCP session is active for the same client.
    // 在同一客户端还拥有另一个活跃 MCP 会话时启动真实交互终端。
    let command = if cfg!(windows) { "cmd.exe /Q /K" } else { "sh" };
    let (_, terminal) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"terminal_session","arguments":{"sid":sid,"action":"start","name":"MCP fixture","command":command}
    })).await;
    let terminal = structured(&terminal);
    let terminal_id = terminal["terminal_session_id"]
        .as_str()
        .expect("structured terminal identifier");
    assert_eq!(
        std::path::Path::new(terminal["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        denied.canonicalize().unwrap()
    );
    let (_, detached) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"terminal_session","arguments":{"sid":scoped_sid,"action":"snapshot","terminal_session_id":terminal_id}
    })).await;
    assert_eq!(detached["result"]["isError"], true);
    let (_, owned) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"terminal_session","arguments":{"sid":sid,"action":"list"}
        }),
    )
    .await;
    assert!(
        structured(&owned)["terminal_sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["terminal_session_id"] == terminal_id)
    );
    let (_, removed) = post(
        &server,
        READER_TOKEN,
        "tools/call",
        json!({
            "name":"vc_mcp_project","arguments":{"type":"remove","project_id":project_id}
        }),
    )
    .await;
    assert_eq!(structured(&removed)["removed"], true);
    let (_, retired) = post(&server, READER_TOKEN, "tools/call", raw_read.clone()).await;
    assert_eq!(retired["result"]["isError"], true);
    let (_, retired_terminal) = post(&server, READER_TOKEN, "tools/call", json!({
        "name":"terminal_session","arguments":{"sid":sid,"action":"snapshot","terminal_session_id":terminal_id}
    })).await;
    assert_eq!(retired_terminal["result"]["isError"], true);
    let history = rusqlite::Connection::open(storage.path().join("calls.sqlite3"))
        .expect("separate history database");
    let (stored_project, stored_result): (String, String) = history
        .query_row(
            "SELECT project_id,result FROM calls WHERE sid=?1 AND tool='shell_job'",
            [&sid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("job result by public SID");
    assert_eq!(stored_project, project_id);
    assert!(stored_result.contains("audited-fixture"));

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
