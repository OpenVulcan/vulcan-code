//! Protocol, authorization, waiting, and media-job behaviour against an in-memory backend.
//! 基于内存后端验证协议、授权、等待与媒体任务行为。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use vulcan_provider_config::provider_config::{
    McpMediaTargetGrant, McpProjectScope, McpServiceClient, McpSessionGrant,
    PersistedMcpServiceConfiguration,
};

use crate::{
    MCP_SUPPORTED_PROTOCOL_VERSIONS, McpChatModel, McpMediaFile, McpMediaModel, McpMediaOutcome,
    McpMediaRun, McpPendingQuestion, McpProject, McpQuestionPrompt, McpServer, McpServiceBackend,
    McpSessionRow, McpSessionStatus, McpSubmission, McpSubmissionAck, McpTranscriptEntry,
    McpTranscriptKind, McpTranscriptPage, issue_client_token, token_sha256_hex,
};

/// Token of the full-access test client.
const FULL_TOKEN: &str = "vmcp_full";
/// Token of the read-only test client limited to project `p1`.
const READER_TOKEN: &str = "vmcp_reader";

/// One simulated session.
#[derive(Clone)]
struct FakeSession {
    row: McpSessionRow,
    questions: Vec<McpPendingQuestion>,
    entries: Vec<McpTranscriptEntry>,
    /// Status checks left before a running turn finishes with its reply.
    remaining_polls: usize,
}

/// In-memory backend.
struct FakeBackend {
    configuration: Mutex<PersistedMcpServiceConfiguration>,
    sessions: Mutex<Vec<FakeSession>>,
    submissions: Mutex<Vec<McpSubmission>>,
    media_delay: Duration,
    cancelled_owners: Mutex<Vec<String>>,
}

impl FakeBackend {
    fn new(media_delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            configuration: Mutex::new(PersistedMcpServiceConfiguration {
                enabled: true,
                clients: vec![
                    client("full", FULL_TOKEN, full_grant(), vec![media_grant()]),
                    client(
                        "reader",
                        READER_TOKEN,
                        McpSessionGrant {
                            read: true,
                            projects: McpProjectScope::Selected {
                                project_ids: vec![String::from("p1")],
                            },
                            ..McpSessionGrant::default()
                        },
                        Vec::new(),
                    ),
                ],
            }),
            sessions: Mutex::new(vec![
                session("s1", "p1", Vec::new()),
                session(
                    "s2",
                    "p2",
                    vec![McpPendingQuestion {
                        request_id: String::from("approval-1"),
                        approval_tool: Some(String::from("shell_command")),
                        prompts: vec![prompt()],
                    }],
                ),
            ]),
            submissions: Mutex::new(Vec::new()),
            media_delay,
            cancelled_owners: Mutex::new(Vec::new()),
        })
    }
}

fn full_grant() -> McpSessionGrant {
    McpSessionGrant {
        read: true,
        send: true,
        create: true,
        stop: true,
        answer_questions: true,
        projects: McpProjectScope::All,
    }
}

fn media_grant() -> McpMediaTargetGrant {
    McpMediaTargetGrant {
        connection_id: String::from("acct"),
        target_id: String::from("image-target"),
    }
}

fn client(
    id: &str,
    token: &str,
    sessions: McpSessionGrant,
    media_targets: Vec<McpMediaTargetGrant>,
) -> McpServiceClient {
    McpServiceClient {
        id: id.to_string(),
        name: id.to_string(),
        token_sha256: token_sha256_hex(token),
        token_hint: String::from("test"),
        created_at_ms: 1,
        enabled: true,
        sessions,
        media_targets,
    }
}

fn prompt() -> McpQuestionPrompt {
    McpQuestionPrompt {
        header: String::from("Pick"),
        question: String::from("Which?"),
        options: Vec::new(),
        multiple: false,
    }
}

fn session(id: &str, project_id: &str, questions: Vec<McpPendingQuestion>) -> FakeSession {
    FakeSession {
        row: McpSessionRow {
            id: id.to_string(),
            title: id.to_string(),
            project_id: Some(project_id.to_string()),
            run_phase: String::from("Idle"),
            running: false,
            attention: None,
            updated_at: String::new(),
            unread: false,
            dedicated: false,
        },
        questions,
        entries: vec![
            entry(1, McpTranscriptKind::User, "old question"),
            entry(2, McpTranscriptKind::Assistant, "old answer"),
        ],
        remaining_polls: 0,
    }
}

fn entry(seq: u64, kind: McpTranscriptKind, text: &str) -> McpTranscriptEntry {
    McpTranscriptEntry {
        seq,
        turn: None,
        kind,
        status: None,
        tool_name: None,
        text: text.to_string(),
        updated_at_ms: 0,
    }
}

fn next_seq(session: &FakeSession) -> u64 {
    session.entries.last().map_or(1, |entry| entry.seq + 1)
}

#[async_trait]
impl McpServiceBackend for FakeBackend {
    fn configuration(&self) -> Result<PersistedMcpServiceConfiguration, String> {
        Ok(self.configuration.lock().unwrap().clone())
    }

    async fn projects(&self) -> Result<Vec<McpProject>, String> {
        Ok(["p1", "p2"]
            .iter()
            .map(|id| McpProject {
                id: id.to_string(),
                name: id.to_string(),
                path: format!("/work/{id}"),
            })
            .collect())
    }

    async fn sessions(&self) -> Result<Vec<McpSessionRow>, String> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .map(|session| session.row.clone())
            .collect())
    }

    async fn session_status(&self, session_id: &str) -> Result<Option<McpSessionStatus>, String> {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(session) = sessions
            .iter_mut()
            .find(|session| session.row.id == session_id)
        else {
            return Ok(None);
        };
        if session.row.running {
            if session.remaining_polls == 0 {
                let seq = next_seq(session);
                session
                    .entries
                    .push(entry(seq, McpTranscriptKind::Tool, ""));
                session
                    .entries
                    .push(entry(seq + 1, McpTranscriptKind::Assistant, "new answer"));
                session.row.running = false;
                session.row.run_phase = String::from("Ready");
            } else {
                session.remaining_polls -= 1;
            }
        }
        Ok(Some(McpSessionStatus {
            row: session.row.clone(),
            questions: session.questions.clone(),
            queued_prompts: 0,
            next_seq: next_seq(session),
        }))
    }

    async fn read_transcript(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: usize,
        _include_tool_output: bool,
    ) -> Result<McpTranscriptPage, String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .iter()
            .find(|session| session.row.id == session_id)
            .ok_or("missing")?;
        let entries = session
            .entries
            .iter()
            .filter(|entry| entry.seq > after_seq)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        Ok(McpTranscriptPage {
            last_scanned_seq: entries.last().map(|entry| entry.seq),
            entries,
            next_seq: next_seq(session),
        })
    }

    async fn submit(&self, submission: McpSubmission) -> Result<McpSubmissionAck, String> {
        let mut sessions = self.sessions.lock().unwrap();
        let session_id = match submission.session_id.clone() {
            Some(session_id) => session_id,
            None => {
                let mut created =
                    session("new", submission.project_id.as_deref().unwrap(), Vec::new());
                created.entries.clear();
                sessions.push(created);
                String::from("new")
            }
        };
        let session = sessions
            .iter_mut()
            .find(|session| session.row.id == session_id)
            .ok_or("missing")?;
        let seq = next_seq(session);
        session
            .entries
            .push(entry(seq, McpTranscriptKind::User, &submission.prompt));
        session.row.running = true;
        session.row.run_phase = String::from("Thinking");
        session.remaining_polls = 3;
        self.submissions.lock().unwrap().push(submission);
        Ok(McpSubmissionAck {
            session_id,
            queued: false,
        })
    }

    async fn stop(&self, _session_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn answer_question(
        &self,
        _request_id: &str,
        _answers: Option<Vec<Vec<String>>>,
    ) -> Result<bool, String> {
        Ok(true)
    }

    async fn chat_models(&self) -> Result<Vec<McpChatModel>, String> {
        Ok(["a", "b"]
            .iter()
            .map(|account| McpChatModel {
                provider_id: String::from("openai"),
                model_id: String::from("gpt/5"),
                connection_id: account.to_string(),
                model_name: String::from("GPT 5"),
                provider_name: String::from("OpenAI"),
                account_name: account.to_string(),
                preferred_account: *account == "b",
            })
            .collect())
    }

    async fn media_models(
        &self,
        grants: &[McpMediaTargetGrant],
    ) -> Result<Vec<McpMediaModel>, String> {
        Ok(grants
            .iter()
            .map(|grant| McpMediaModel {
                model: String::from("vendor/image-1"),
                capability: String::from("image_generate"),
                display_name: String::from("Image 1"),
                connection_id: grant.connection_id.clone(),
                target_id: grant.target_id.clone(),
                parameters_markdown: String::from("## args"),
            })
            .collect())
    }

    async fn run_media(&self, run: McpMediaRun) -> Result<McpMediaOutcome, String> {
        tokio::time::sleep(self.media_delay).await;
        Ok(McpMediaOutcome {
            files: vec![McpMediaFile {
                path: std::path::PathBuf::from(format!("/media/{}.png", run.job_id)),
                kind: String::from("image"),
                mime_type: String::from("image/png"),
                byte_len: 10,
            }],
            text: None,
        })
    }

    fn cancel_media(&self, owner_id: &str) -> Result<(), String> {
        self.cancelled_owners
            .lock()
            .unwrap()
            .push(owner_id.to_string());
        Ok(())
    }
}

/// Sends one JSON-RPC request and returns the HTTP status and parsed body.
async fn post(server: &Arc<McpServer>, token: Option<&str>, body: Value) -> (u16, Value) {
    let authorization = token.map(|token| format!("Bearer {token}"));
    let response = server
        .handle_post(authorization.as_deref(), body.to_string().as_bytes())
        .await;
    let value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    (response.status, value)
}

/// Calls one tool and returns its `CallToolResult`.
async fn call(server: &Arc<McpServer>, token: &str, name: &str, arguments: Value) -> Value {
    let (status, body) = post(
        server,
        Some(token),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
    )
    .await;
    assert_eq!(status, 200, "tools/call must answer over HTTP 200: {body}");
    body["result"].clone()
}

fn server(backend: Arc<FakeBackend>) -> Arc<McpServer> {
    Arc::new(McpServer::new(backend, "0.0.0-test"))
}

#[tokio::test]
async fn initialize_negotiates_supported_protocol_versions() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let (_, body) = post(
        &server,
        Some(FULL_TOKEN),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
    )
    .await;
    assert_eq!(body["result"]["protocolVersion"], "2025-03-26");
    assert!(body["result"]["capabilities"]["tools"].is_object());
    let (_, body) = post(
        &server,
        Some(FULL_TOKEN),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }),
    )
    .await;
    assert_eq!(
        body["result"]["protocolVersion"],
        MCP_SUPPORTED_PROTOCOL_VERSIONS[0]
    );
}

#[tokio::test]
async fn authentication_gates_every_request() {
    let backend = FakeBackend::new(Duration::ZERO);
    let server = server(Arc::clone(&backend));
    let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
    assert_eq!(post(&server, None, ping.clone()).await.0, 401);
    assert_eq!(post(&server, Some("vmcp_wrong"), ping.clone()).await.0, 401);
    assert_eq!(post(&server, Some(FULL_TOKEN), ping.clone()).await.0, 200);
    backend.configuration.lock().unwrap().clients[0].enabled = false;
    assert_eq!(post(&server, Some(FULL_TOKEN), ping.clone()).await.0, 401);
    backend.configuration.lock().unwrap().enabled = false;
    assert_eq!(post(&server, Some(READER_TOKEN), ping).await.0, 503);
}

#[tokio::test]
async fn notifications_are_accepted_without_a_body() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let response = server
        .handle_post(
            Some(&format!("Bearer {FULL_TOKEN}")),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
                .to_string()
                .as_bytes(),
        )
        .await;
    assert_eq!(response.status, 202);
    assert!(response.body.is_empty());
}

#[tokio::test]
async fn tool_list_follows_the_client_grant() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let names = |body: Value| {
        body["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let reader = names(post(&server, Some(READER_TOKEN), list.clone()).await.1);
    assert!(reader.contains(&String::from("read_messages")));
    assert!(!reader.contains(&String::from("send_message")));
    assert!(!reader.contains(&String::from("generate_media")));
    let full = names(post(&server, Some(FULL_TOKEN), list).await.1);
    assert!(full.contains(&String::from("send_message")));
    assert!(full.contains(&String::from("generate_media")));
    let refused = call(
        &server,
        READER_TOKEN,
        "send_message",
        json!({ "session_id": "s1", "message": "hi" }),
    )
    .await;
    assert_eq!(refused["isError"], true);
}

#[tokio::test]
async fn project_scope_hides_sessions_outside_the_grant() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let listed = call(&server, READER_TOKEN, "list_sessions", json!({})).await;
    let sessions = listed["structuredContent"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], "s1");
    let hidden = call(
        &server,
        READER_TOKEN,
        "get_session",
        json!({ "session_id": "s2" }),
    )
    .await;
    assert_eq!(hidden["isError"], true);
}

#[tokio::test(start_paused = true)]
async fn send_message_waits_for_the_reply_after_its_own_message() {
    let backend = FakeBackend::new(Duration::ZERO);
    let server = server(Arc::clone(&backend));
    let result = call(
        &server,
        FULL_TOKEN,
        "send_message",
        json!({ "session_id": "s1", "message": "do it" }),
    )
    .await;
    let structured = &result["structuredContent"];
    assert_eq!(structured["status"], "idle", "{result}");
    assert_eq!(structured["reply"], "new answer");
    assert_eq!(structured["cursor"], 5);
    assert!(backend.submissions.lock().unwrap()[0].model.is_none());
}

#[tokio::test(start_paused = true)]
async fn zero_wait_returns_a_running_status_and_cursor() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let result = call(
        &server,
        FULL_TOKEN,
        "send_message",
        json!({ "session_id": "s1", "message": "do it", "wait_seconds": 0 }),
    )
    .await;
    assert_eq!(result["structuredContent"]["status"], "running");
    assert_eq!(result["structuredContent"]["cursor"], 3);
}

#[tokio::test(start_paused = true)]
async fn model_selection_uses_the_preferred_account_or_the_named_one() {
    let backend = FakeBackend::new(Duration::ZERO);
    let server = server(Arc::clone(&backend));
    call(
        &server,
        FULL_TOKEN,
        "create_session",
        json!({ "project_id": "p1", "message": "start", "model": "openai/gpt/5" }),
    )
    .await;
    call(
        &server,
        FULL_TOKEN,
        "send_message",
        json!({ "session_id": "s1", "message": "go", "model": "openai/gpt/5", "account": "a" }),
    )
    .await;
    let submissions = backend.submissions.lock().unwrap();
    assert_eq!(submissions[0].model.as_ref().unwrap().connection_id, "b");
    assert_eq!(submissions[0].model.as_ref().unwrap().model_id, "gpt/5");
    assert_eq!(submissions[1].model.as_ref().unwrap().connection_id, "a");
}

#[tokio::test]
async fn approvals_cannot_be_answered_and_waits_report_them() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let answered = call(
        &server,
        FULL_TOKEN,
        "answer_question",
        json!({ "session_id": "s2", "request_id": "approval-1", "answers": [["yes"]] }),
    )
    .await;
    assert_eq!(answered["isError"], true);
    let waited = call(
        &server,
        FULL_TOKEN,
        "wait_for_reply",
        json!({ "session_id": "s2" }),
    )
    .await;
    assert_eq!(
        waited["structuredContent"]["status"],
        "waiting_for_approval"
    );
}

#[tokio::test(start_paused = true)]
async fn slow_media_returns_a_job_that_can_be_awaited_or_cancelled() {
    let backend = FakeBackend::new(Duration::from_secs(120));
    let server = server(Arc::clone(&backend));
    let started = call(
        &server,
        FULL_TOKEN,
        "generate_media",
        json!({ "model": "vendor/image-1", "arguments": { "prompt": "cat" }, "wait_seconds": 1 }),
    )
    .await;
    assert_eq!(
        started["structuredContent"]["status"], "running",
        "{started}"
    );
    let job_id = started["structuredContent"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let finished = call(
        &server,
        FULL_TOKEN,
        "get_media_job",
        json!({ "job_id": job_id, "wait_seconds": 600 }),
    )
    .await;
    assert_eq!(finished["structuredContent"]["status"], "succeeded");
    assert_eq!(
        finished["structuredContent"]["files"][0]["mime_type"],
        "image/png"
    );

    let second = call(
        &server,
        FULL_TOKEN,
        "generate_media",
        json!({ "model": "vendor/image-1", "arguments": {}, "wait_seconds": 0 }),
    )
    .await;
    let second_id = second["structuredContent"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let cancelled = call(
        &server,
        FULL_TOKEN,
        "cancel_media_job",
        json!({ "job_id": second_id }),
    )
    .await;
    assert_eq!(cancelled["structuredContent"]["status"], "cancelled");
    assert_eq!(
        backend.cancelled_owners.lock().unwrap().as_slice(),
        [format!("mcp-media:{second_id}")]
    );
    let foreign = call(
        &server,
        READER_TOKEN,
        "get_media_job",
        json!({ "job_id": second_id }),
    )
    .await;
    assert_eq!(foreign["isError"], true);
}

#[tokio::test]
async fn unknown_arguments_are_rejected() {
    let server = server(FakeBackend::new(Duration::ZERO));
    let result = call(
        &server,
        FULL_TOKEN,
        "list_projects",
        json!({ "surprise": 1 }),
    )
    .await;
    assert_eq!(result["isError"], true);
}

#[test]
fn issued_tokens_are_unique_and_hashed() {
    let first = issue_client_token();
    let second = issue_client_token();
    assert_ne!(first.token, second.token);
    assert!(first.token.starts_with("vmcp_"));
    assert_eq!(first.token_sha256, token_sha256_hex(&first.token));
    assert_eq!(first.token_sha256.len(), 64);
    assert!(first.token.ends_with(&first.token_hint));
}
