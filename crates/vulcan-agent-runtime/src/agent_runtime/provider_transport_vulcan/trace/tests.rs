//! Regression tests for the trace boundary.
//! trace边界的回归测试。

use super::*;

/// Verifies trace headers preserve ordinary protocol fields while removing access material.
/// 验证追踪请求头保留普通协议字段，同时移除访问凭据材料。
#[test]
fn provider_request_trace_headers_redact_access_material() {
    let headers = BTreeMap::from([
        (
            String::from("content-type"),
            String::from("application/json"),
        ),
        (String::from("Authorization"), String::from("Bearer secret")),
        (
            String::from("x-session-token"),
            String::from("secret-token"),
        ),
    ]);

    let traced = provider_request_trace_headers(&headers);

    assert_eq!(
        traced.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        traced.get("Authorization").map(String::as_str),
        Some("<redacted>")
    );
    assert_eq!(
        traced.get("x-session-token").map(String::as_str),
        Some("<redacted>")
    );
}

/// Verifies host-side failures fill the trace stage only when no lower transport failure exists.
/// 验证仅当不存在更底层传输失败时，宿主侧失败才会补充追踪阶段。
#[test]
fn provider_api_trace_failure_selection_preserves_the_earliest_boundary() {
    let processing_failure = ProviderApiTraceProcessingFailure {
        stage: String::from("tool_request_coordinator"),
        message: String::from("tool stream ordering failed"),
    };

    let (stage, message, local_processing_failed) =
        select_provider_api_trace_failure(None, None, Some(&processing_failure));
    assert_eq!(stage.as_deref(), Some("tool_request_coordinator"));
    assert_eq!(message.as_deref(), Some("tool stream ordering failed"));
    assert!(local_processing_failed);

    let (stage, message, local_processing_failed) = select_provider_api_trace_failure(
        Some("protocol_decoder"),
        Some("invalid frame"),
        Some(&processing_failure),
    );
    assert_eq!(stage.as_deref(), Some("protocol_decoder"));
    assert_eq!(message.as_deref(), Some("invalid frame"));
    assert!(!local_processing_failed);
}

/// Verifies tool construction events are attributed to the request coordinator trace stage.
/// 验证工具构造事件会归属到工具请求协调器追踪阶段。
#[test]
fn provider_api_trace_classifies_tool_request_coordinator_events() {
    let event = AiStreamEvent::ToolCallStarted {
        item_id: String::from("item-1"),
        call_id: String::from("call-1"),
        tool_name: String::from("file_edit"),
        index: 0,
    };

    assert_eq!(
        provider_trace_processing_stage_for_event(&event),
        "tool_request_coordinator"
    );
    assert_eq!(
        provider_trace_processing_stage_for_event(&AiStreamEvent::ResponseCompleted),
        "response_projection"
    );
}

/// Verifies the detached trace writer receives one controlled host-side processing failure.
/// 验证独立追踪写入器会收到一项受控的宿主侧处理失败。
#[tokio::test]
async fn provider_api_trace_processing_failure_sender_publishes_once() {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut publisher = ProviderApiTraceProcessingFailureSender::new(Some(sender));
    let error = provider_failure("provider", "tool stream ordering failed");

    publisher.observe_failure("tool_request_coordinator", &error);
    publisher.observe_failure("response_projection", &error);

    let failure = receiver
        .await
        .expect("the first processing failure should be published");
    assert_eq!(failure.stage, "tool_request_coordinator");
    assert!(failure.message.contains("tool stream ordering failed"));
}

/// Verifies diagnostic draining consumes events after the host has already rejected a turn.
/// 验证诊断排空会在宿主已拒绝回合后继续消费剩余事件。
#[tokio::test]
async fn provider_trace_drain_consumes_the_remaining_stream() {
    // tailObserved reports that the detached diagnostic consumer reached the simulated body tail.
    // tailObserved 用于报告分离式诊断消费者已经到达模拟正文尾部。
    let (tail_observed_sender, tail_observed_receiver) = tokio::sync::oneshot::channel();
    // remainingStream models normalized events still backed by unread Provider response bytes.
    // remainingStream 模拟仍由未读取 Provider 响应字节支撑的规范化事件。
    let remaining_stream: ProviderExecutionStream = Box::pin(async_stream::stream! {
        yield Ok(AiStreamEvent::ResponseStarted {
            execution_id: String::from("trace-drain-response"),
        });
        let _ = tail_observed_sender.send(());
        yield Ok(AiStreamEvent::ResponseCompleted);
    });

    drain_provider_stream_for_trace(remaining_stream);

    // observedTail fails if the detached drain stops after the first remaining event.
    // observedTail 在分离式排空于首个剩余事件后停止时失败。
    tokio::time::timeout(std::time::Duration::from_secs(1), tail_observed_receiver)
        .await
        .expect("trace drain should reach the response tail")
        .expect("trace drain should keep the tail observer alive");
}

// The native compaction dispatch owns no controller adapter, so its artifact reuse is exercised
// through the same session trace entry points it calls, against one real loopback compact attempt.
// 原生压缩分派没有控制器适配器，因此其制品复用通过它调用的同一批会话追踪入口、对一次真实回环
// 压缩尝试进行验证。

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, PromptCachePolicy,
};
use vulcan_provider_adapters::{
    CredentialConnection, CredentialExecutionSecret, CredentialSecretEnvelope,
    CredentialSecretMaterial, OAuthTokenSet, ProviderExecutionClient, ResponsesCompactEvent,
    SecondaryRouteSelection,
};
use vulcan_workspace_store::workspace_store::{PROVIDER_API_TRACE_LIMIT, REDACTED_PROVIDER_SECRET};

/// Bundled provider whose Codex subscription entry owns the compaction endpoint.
/// 拥有压缩端点的 Codex 订阅入口所属的内置供应商。
const COMPACTION_TRACE_PROVIDER_ID: &str = "openai";
/// Bundled service entry declaring the Codex Responses conversation route.
/// 声明 Codex Responses 会话路由的内置服务入口。
const COMPACTION_TRACE_SERVICE_ENTRY_ID: &str = "codex-subscription";
/// Exact model identity used by the compaction trace fixture.
/// 压缩追踪夹具使用的精确模型身份。
const COMPACTION_TRACE_MODEL_ID: &str = "gpt-5.6-sol";
/// Synthetic credential value that must never appear in a persisted artifact.
/// 绝不允许出现在持久化制品中的合成凭据值。
const COMPACTION_TRACE_SECRET: &str = "compaction-trace-access-token";

/// One artifact pair read back from the durable trace ring.
/// 从可靠追踪环中读回的一对制品。
struct PublishedProviderApiTrace {
    /// Parsed input artifact.
    /// 已解析的输入制品。
    input: Value,
    /// Parsed output artifact.
    /// 已解析的输出制品。
    output: Value,
    /// Exact input artifact bytes as persisted.
    /// 实际持久化的输入制品字节。
    input_raw: String,
    /// Exact output artifact bytes as persisted.
    /// 实际持久化的输出制品字节。
    output_raw: String,
}

/// Builds one exact Codex Responses route from the bundled manifest, redirected to `base_url`.
/// 从内置清单构建一条精确 Codex Responses 路由，并重定向到 `base_url`。
fn compaction_trace_route(base_url: &str) -> ProviderExecutionRoute {
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text =
        std::fs::read_to_string(directory.join(format!("{COMPACTION_TRACE_PROVIDER_ID}.json")))
            .expect("the bundled openai manifest must exist");
    let manifest = vulcan_provider_adapters::provider_manifest::compile_manifest_text(
        text.as_str(),
        vulcan_provider_adapters::provider_manifest::ManifestOwnership::System,
    )
    .expect("the bundled openai manifest must compile");
    let provider = manifest.provider().clone();
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == COMPACTION_TRACE_SERVICE_ENTRY_ID)
        .expect("the bundled codex subscription entry must exist")
        .clone();
    let credential_profile = entry
        .credential_profiles
        .first()
        .expect("the codex subscription entry must declare one credential profile")
        .clone();
    let connection = CredentialConnection {
        id: String::from("compaction-trace-connection"),
        provider_id: provider.id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: credential_profile.id.clone(),
        credential_mode: credential_profile.credential_mode,
        credential_id: String::from("compaction-trace-credential"),
        display_name: String::from("Compaction trace fixture"),
        secret_reference: String::from("compaction-trace-secret-reference"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    let mut route = ProviderExecutionRoute::resolve(
        provider,
        entry,
        connection,
        SecondaryRouteSelection::Automatic,
        vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID,
    )
    .expect("the codex subscription route must resolve");
    route.upload_channels = manifest.upload_channels;
    route.protocol_route.base_url_override = Some(base_url.to_string());
    route
}

/// Builds one real execution client whose transport never leaves the loopback endpoint.
/// 构建一个真实执行客户端，其传输绝不离开回环端点。
fn compaction_trace_client() -> ProviderExecutionClient {
    let protocols = std::sync::Arc::new(
        vulcan_llm_core::LlmProtocolRuntime::first_batch()
            .expect("the first-batch protocol runtime must build"),
    );
    ProviderExecutionClient::new(
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("the loopback fixture client must build"),
        protocols,
    )
}

/// Builds synthetic OAuth material that requires no credential store or refresh endpoint.
/// 构建不访问凭据仓库或刷新端点的合成 OAuth 材料。
fn compaction_trace_secret() -> CredentialExecutionSecret {
    CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenAiCodexOAuth {
        tokens: OAuthTokenSet {
            access_token: String::from(COMPACTION_TRACE_SECRET),
            refresh_token: String::from("compaction-trace-refresh-token"),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: Default::default(),
            account_subject: None,
            email: None,
        },
        id_token: None,
        account_id: Some(String::from("compaction-trace-account")),
    })
    .expect("the fixture secret envelope must validate")
    .execution_secret()
    .expect("the fixture secret must project for execution")
}

/// Builds one minimal provider-neutral compaction input.
/// 构建一份最小供应商无关压缩输入。
fn compaction_trace_request() -> AiExecutionRequest {
    AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: String::from(COMPACTION_TRACE_MODEL_ID),
        cache_scope_id: Some(String::from("compaction-trace-cache")),
        continuation: None,
        messages: vec![AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: String::from("compact the ordinary conversation history"),
            }],
            provider_context_items: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::ProviderDefault,
        reasoning_effort: None,
        reasoning_output: Default::default(),
        model_parameters: BTreeMap::new(),
        resources: Vec::new(),
        output_token_limit: Some(512),
    }
}

/// Reads one bounded loopback HTTP request and returns its exact UTF-8 body.
/// 读取一项有界回环 HTTP 请求并返回其精确 UTF-8 正文。
async fn read_compact_loopback_body(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4_096];
    let (body_start, body_length) = loop {
        let read = socket
            .read(&mut chunk)
            .await
            .expect("the compact request must read");
        assert!(read > 0, "compact request headers must not end early");
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..position]).to_ascii_lowercase();
            let length = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.trim() == "content-length")
                .map(|(_, value)| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("content length must be numeric")
                })
                .expect("the compact request must declare its content length");
            break (position + 4, length);
        }
        assert!(
            bytes.len() <= 128 * 1_024,
            "compact request must stay bounded"
        );
    };
    while bytes.len() < body_start + body_length {
        let read = socket
            .read(&mut chunk)
            .await
            .expect("the compact request body must read");
        assert!(read > 0, "compact request body must not end early");
        bytes.extend_from_slice(&chunk[..read]);
    }
    String::from_utf8(bytes[body_start..body_start + body_length].to_vec())
        .expect("the compact request body must be UTF-8")
}

/// Serves exactly one compact attempt returning `body`; yields the base URL and received body.
/// 为一次压缩尝试提供服务并返回 `body`；产出基址与收到的正文。
async fn serve_one_compact_attempt(
    body: String,
) -> (
    String,
    tokio::sync::oneshot::Receiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the loopback compact endpoint must bind");
    let address = listener
        .local_addr()
        .expect("the loopback endpoint must have an address");
    let (received_sender, received_receiver) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("one compact connection");
        let received = read_compact_loopback_body(&mut socket).await;
        let _ = received_sender.send(received);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len(),
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("the compact response must write");
        let _ = socket.shutdown().await;
    });
    (format!("http://{address}"), received_receiver, task)
}

/// Returns the parsed artifact pair once the detached writer published `sequence`.
/// 独立写入器发布 `sequence` 后返回已解析的制品对。
fn read_published_provider_api_trace(
    root: &std::path::Path,
    sequence: u64,
) -> Option<PublishedProviderApiTrace> {
    let slot = (sequence - 1) % PROVIDER_API_TRACE_LIMIT;
    let slot_dir = root.join(format!("slot-{slot:02}"));
    let pointer: Value =
        serde_json::from_slice(&std::fs::read(slot_dir.join("identity.json")).ok()?)
            .expect("the committed trace pointer must be valid JSON");
    let payload_bank = pointer["payload_bank"]
        .as_u64()
        .expect("the committed trace pointer must carry its payload bank");
    let input_raw =
        std::fs::read_to_string(slot_dir.join(format!("payload-{payload_bank}/input.jsonl")))
            .ok()?;
    let output_raw =
        std::fs::read_to_string(slot_dir.join(format!("payload-{payload_bank}/output.jsonl")))
            .ok()?;
    Some(PublishedProviderApiTrace {
        input: serde_json::from_str(input_raw.as_str()).expect("the input artifact must be JSON"),
        output: serde_json::from_str(output_raw.as_str())
            .expect("the output artifact must be JSON"),
        input_raw,
        output_raw,
    })
}

/// Awaits the detached trace writer with one bounded deadline.
/// 以一个有界期限等待独立追踪写入器。
async fn wait_for_published_provider_api_trace(
    root: &std::path::Path,
    sequence: u64,
) -> PublishedProviderApiTrace {
    let started_at = std::time::Instant::now();
    loop {
        if let Some(published) = read_published_provider_api_trace(root, sequence) {
            return published;
        }
        assert!(
            started_at.elapsed() < Duration::from_secs(10),
            "the detached trace writer must publish the compaction artifact"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Asserts the envelope every reused request artifact shares with ordinary requests.
/// 断言每份复用请求制品与普通请求共享的外层结构。
fn assert_reused_trace_envelope(input: &Value, output: &Value, api_call_id: ApiCallId) {
    assert_eq!(input["schema_version"], 1);
    assert_eq!(
        input["api_call_id"],
        serde_json::to_value(api_call_id).expect("the API call id must serialize")
    );
    assert!(input["trace_sequence"].is_u64());
    assert!(input["attempt_index"].is_u64());
    assert!(input["captured_at_unix_ms"].is_u64());
    assert!(
        input["execution_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(
        input["session_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(input["model_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(input["route"]["provider"]["id"].as_str().is_some());
    assert_eq!(input["request"]["capture_point"], "transport_dispatch");
    assert_eq!(input["request"]["credential_values_redacted"], true);
    assert_eq!(output["schema_version"], 1);
    assert_eq!(
        output["capture_point"],
        "before_local_response_framing_and_protocol_decode"
    );
    assert!(output["response"]["transport"].as_str().is_some());
}

/// Verifies one real native compaction attempt publishes the reused session trace artifact.
/// 验证一次真实原生压缩尝试发布复用的会话追踪制品。
///
/// Business invariant (S2.2): the session-owned trace mechanism must observe the actual native
/// compaction request and its raw response through the frozen opt-in decision, not a parallel
/// diagnostic platform.
/// 业务不变量（S2.2）：会话所属追踪机制必须通过冻结开关决策观察真实原生压缩请求及其原始响应，
/// 而不是另建诊断平台。
#[tokio::test]
async fn native_compaction_request_publishes_the_reused_provider_api_trace_artifact() {
    let temporary = tempfile::tempdir().expect("temporary trace parent");
    let trace_root = temporary.path().join("temp").join("provider-api-trace");
    let trace_store = ProviderApiTraceStore::new(trace_root.clone(), true);
    let body = serde_json::json!({
        "id": "compaction-trace-result",
        "output": [{"type": "compaction", "encrypted_content": "opaque"}],
        "usage": {"input_tokens": 3},
    })
    .to_string();
    let (base_url, received_body, server) = serve_one_compact_attempt(body).await;
    let route = compaction_trace_route(base_url.as_str());
    let client = compaction_trace_client();
    let api_call_id = ApiCallId::try_new(7).expect("valid API call id");
    let attempt_state = std::sync::Mutex::new(ProviderApiTraceAttemptState::default());

    // capture is exactly what the native compaction dispatch creates from its physical permit.
    // capture 正是原生压缩分派依据其物理许可创建的内容。
    let capture = ProviderApiTraceAttemptCapture::start(
        true,
        &attempt_state,
        Some(trace_store.clone()),
        api_call_id,
        ProviderApiTraceRenderIdentity {
            record_all: true,
            record_errors: false,
            provider_id: String::from(COMPACTION_TRACE_PROVIDER_ID),
            session_scope_id: String::from("compaction-trace-session"),
            model_id: String::from(COMPACTION_TRACE_MODEL_ID),
            route: route.clone(),
        },
        Some(String::from("compaction-trace-anchor")),
    )
    .expect("enabled tracing must reserve one physical attempt");

    let mut stream = client.stream_responses_compact(
        route,
        compaction_trace_request(),
        compaction_trace_secret(),
        Some(capture.request),
        Some(capture.response),
    );
    let mut completed = false;
    while let Some(event) = stream.next().await {
        match event {
            Ok(ResponsesCompactEvent::TransportObserved { http_status, .. }) => {
                assert_eq!(http_status, 200);
            }
            Ok(ResponsesCompactEvent::Completed(_)) => {
                completed = true;
                break;
            }
            Err(failure) => panic!("the compact attempt must not fail: {failure}"),
        }
    }
    assert!(
        completed,
        "the compact attempt must yield one native result"
    );
    // Dropping the completed stream publishes the raw response capture into the artifact.
    // 析构已完成的流会把原始响应捕获发布进制品。
    drop(stream);

    let published = wait_for_published_provider_api_trace(&trace_root, 1).await;
    assert_reused_trace_envelope(&published.input, &published.output, api_call_id);
    assert_eq!(published.input["trace_sequence"], 1);
    assert_eq!(published.input["attempt_index"], 0);
    assert_eq!(published.input["session_id"], "compaction-trace-session");
    assert_eq!(published.input["model_id"], COMPACTION_TRACE_MODEL_ID);
    assert_eq!(
        published.input["turn_anchor_message_id"],
        "compaction-trace-anchor"
    );
    // The artifact identifies the real compact endpoint rather than an ordinary turn.
    // 制品标识真实压缩端点，而不是普通回合。
    assert_eq!(published.input["request"]["transport"], "responses_compact");
    assert_eq!(published.input["request"]["method"], "POST");
    assert_eq!(published.input["request"]["path"], "/responses/compact");
    assert!(
        published.input["request"]["resolved_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:")),
        "the artifact must carry the actually resolved loopback endpoint"
    );
    // The artifact body is the exact JSON the endpoint received, not a reconstruction.
    // 制品正文是端点实际收到的精确 JSON，而不是重建结果。
    let received = received_body
        .await
        .expect("the endpoint must observe the compact request");
    assert_eq!(
        published.input["request"]["body"],
        serde_json::from_str::<Value>(received.as_str()).expect("the received body must be JSON")
    );
    assert_eq!(published.output["capture_state"], "captured");
    assert_eq!(published.output["response"]["http_status"], 200);
    assert_eq!(
        published.output["response"]["transport"],
        "responses_compact"
    );
    assert_eq!(
        published.output["outcome"]["classification"],
        "responses_compact_completed"
    );
    assert_eq!(published.output["outcome"]["complete"], true);
    assert!(
        published.output["response"]["body_base64"]
            .as_str()
            .is_some_and(|body| !body.is_empty()),
        "the artifact must retain the raw response bytes"
    );

    // Access material is replaced by the existing header/key rules; the raw token never persists.
    // 访问凭据按既有请求头/键规则替换；原始 Token 绝不落盘。
    assert_eq!(
        published.input["request"]["headers"]["authorization"],
        Value::String(String::from(REDACTED_PROVIDER_SECRET))
    );
    assert!(
        !published.input_raw.contains(COMPACTION_TRACE_SECRET),
        "the credential must not appear in the persisted input artifact"
    );
    assert!(
        !published.output_raw.contains(COMPACTION_TRACE_SECRET),
        "the credential must not appear in the persisted output artifact"
    );
    server.await.expect("the compact endpoint task must finish");
}

/// Verifies the native compaction trace stays disabled without the frozen opt-in decision.
/// 验证缺少冻结开关决策时原生压缩追踪保持关闭。
///
/// Business invariant (S2.2): the whole artifact is gated by the frozen per-dispatch setting, so a
/// disabled switch must not allocate capture endpoints or touch the session trace path.
/// 业务不变量（S2.2）：整份制品由逐分派冻结设置把关，关闭开关不得分配捕获端点，也不得接触会话
/// 追踪路径。
#[tokio::test]
async fn native_compaction_trace_requires_the_frozen_opt_in_decision() {
    let temporary = tempfile::tempdir().expect("temporary trace parent");
    let trace_root = temporary.path().join("temp").join("provider-api-trace");
    let trace_store = ProviderApiTraceStore::new(trace_root.clone(), true);
    let route = compaction_trace_route("http://127.0.0.1:1");
    let api_call_id = ApiCallId::try_new(11).expect("valid API call ID");
    let attempt_state = std::sync::Mutex::new(ProviderApiTraceAttemptState::default());

    let disabled = ProviderApiTraceAttemptCapture::start(
        false,
        &attempt_state,
        Some(trace_store.clone()),
        api_call_id,
        ProviderApiTraceRenderIdentity {
            record_all: true,
            record_errors: false,
            provider_id: String::from(COMPACTION_TRACE_PROVIDER_ID),
            session_scope_id: String::from("compaction-trace-session"),
            model_id: String::from(COMPACTION_TRACE_MODEL_ID),
            route: route.clone(),
        },
        None,
    );
    assert!(
        disabled.is_none(),
        "a disabled frozen opt-in must not allocate compaction capture endpoints"
    );
    assert!(
        !trace_root.exists(),
        "a disabled frozen opt-in must not touch the session trace path"
    );

    // Control: the identical call reserves exactly one attempt once the frozen decision is enabled.
    // 对照：冻结决策启用后，同一次调用恰好预留一次尝试。
    let enabled = ProviderApiTraceAttemptCapture::start(
        true,
        &attempt_state,
        Some(trace_store),
        api_call_id,
        ProviderApiTraceRenderIdentity {
            record_all: true,
            record_errors: false,
            provider_id: String::from(COMPACTION_TRACE_PROVIDER_ID),
            session_scope_id: String::from("compaction-trace-session"),
            model_id: String::from(COMPACTION_TRACE_MODEL_ID),
            route,
        },
        None,
    );
    assert!(
        enabled.is_some(),
        "the enabled frozen opt-in must reserve one compaction attempt"
    );
    assert!(
        trace_root.join("sequence.json").exists(),
        "the enabled control must durably reserve its sequence"
    );
}

/// Verifies the ordinary reservation still requires the frozen opt-in decision.
/// 验证普通预留仍然需要冻结开关决策。
#[test]
fn ordinary_trace_reservation_requires_the_frozen_opt_in() {
    let temporary = tempfile::tempdir().expect("temporary trace parent");
    let trace_root = temporary.path().join("temp").join("provider-api-trace");
    let trace_store = ProviderApiTraceStore::new(trace_root.clone(), true);
    let attempt_state = std::sync::Mutex::new(ProviderApiTraceAttemptState::default());
    let api_call_id = ApiCallId::try_new(3).expect("valid API call ID");

    let disabled =
        claim_provider_api_trace_attempt(false, &attempt_state, api_call_id, &trace_store)
            .expect("a disabled reservation is not an error");
    assert!(disabled.is_none());
    assert!(!trace_root.exists());

    let enabled = claim_provider_api_trace_attempt(true, &attempt_state, api_call_id, &trace_store)
        .expect("an enabled reservation must succeed")
        .expect("an enabled reservation must reserve one sequence");
    assert_eq!(enabled.trace_sequence, 1);
    assert_eq!(enabled.api_call_id, api_call_id);
    assert_eq!(enabled.attempt_index, 0);
}

/// Verifies the ordinary request artifact shape is unchanged by the compaction reuse.
/// 验证普通请求制品结构未被压缩复用改变。
#[tokio::test]
async fn ordinary_provider_request_trace_shape_is_unchanged() {
    let temporary = tempfile::tempdir().expect("temporary trace parent");
    let trace_root = temporary.path().join("temp").join("provider-api-trace");
    let trace_store = ProviderApiTraceStore::new(trace_root.clone(), true);
    let attempt_state = std::sync::Mutex::new(ProviderApiTraceAttemptState::default());
    let api_call_id = ApiCallId::try_new(5).expect("valid API call ID");
    let reservation =
        claim_provider_api_trace_attempt(true, &attempt_state, api_call_id, &trace_store)
            .expect("the ordinary reservation must succeed")
            .expect("the ordinary reservation must reserve one sequence");

    let captured_request = ProviderWireRequestCapture {
        transport: String::from("http_sse"),
        method: String::from("POST"),
        path: String::from("/responses"),
        resolved_url: String::from("https://example.test/responses"),
        headers: BTreeMap::from([
            (
                String::from("content-type"),
                String::from("application/json"),
            ),
            (String::from("authorization"), String::from("<redacted>")),
        ]),
        body: json!({"model": COMPACTION_TRACE_MODEL_ID, "input": []}),
    };
    let ordinary_response_body = br#"data: {"type":"response.completed"}"#;
    let captured_response = ProviderWireResponseCapture {
        transport: String::from("http_sse"),
        http_status: Some(200),
        content_type: Some(String::from("text/event-stream")),
        content_encoding: None,
        body: ordinary_response_body.to_vec(),
        chunks: Vec::new(),
        frames: Vec::new(),
        received_bytes: u64::try_from(ordinary_response_body.len())
            .expect("the fixture body length must fit u64"),
        body_truncated: false,
        chunk_index_truncated: false,
        frame_index_truncated: false,
        outcome: String::from("response_completed"),
        normalized_tool_call_abort: None,
        failure_stage: None,
        failure_message: None,
        body_sha256: String::new(),
    };

    persist_provider_api_trace_snapshot(
        ProviderApiTraceRenderIdentity {
            record_all: true,
            record_errors: false,
            provider_id: String::from(COMPACTION_TRACE_PROVIDER_ID),
            session_scope_id: String::from("ordinary-trace-session"),
            model_id: String::from(COMPACTION_TRACE_MODEL_ID),
            route: compaction_trace_route("http://127.0.0.1:1"),
        },
        trace_store,
        reservation,
        "ordinary-trace-execution",
        Some("ordinary-trace-anchor"),
        captured_request,
        ProviderApiTraceResponseState::Captured(captured_response),
        None,
    )
    .await
    .expect("the ordinary trace artifact must publish");

    let published = read_published_provider_api_trace(&trace_root, 1)
        .expect("the ordinary artifact must be durable after publication");
    assert_reused_trace_envelope(&published.input, &published.output, api_call_id);
    assert_eq!(published.input["request"]["transport"], "http_sse");
    assert_eq!(published.input["request"]["path"], "/responses");
    assert_eq!(published.input["session_id"], "ordinary-trace-session");
    assert_eq!(
        published.input["turn_anchor_message_id"],
        "ordinary-trace-anchor"
    );
    assert_eq!(
        published.output["outcome"]["classification"],
        "response_completed"
    );
}
