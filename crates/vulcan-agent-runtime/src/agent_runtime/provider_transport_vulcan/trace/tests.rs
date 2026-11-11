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

use vulcan_provider_adapters::{CredentialConnection, SecondaryRouteSelection};
use vulcan_workspace_store::workspace_store::PROVIDER_API_TRACE_LIMIT;

/// Bundled provider whose Codex subscription entry backs the trace fixture route.
/// 其 Codex 订阅入口承载追踪夹具路由的内置供应商。
const TRACE_FIXTURE_PROVIDER_ID: &str = "openai";
/// Bundled service entry declaring the Codex Responses conversation route.
/// 声明 Codex Responses 会话路由的内置服务入口。
const TRACE_FIXTURE_SERVICE_ENTRY_ID: &str = "codex-subscription";
/// Exact model identity used by the trace fixture.
/// 追踪夹具使用的精确模型身份。
const TRACE_FIXTURE_MODEL_ID: &str = "gpt-5.6-sol";

/// One artifact pair read back from the durable trace ring.
/// 从可靠追踪环中读回的一对制品。
struct PublishedProviderApiTrace {
    /// Parsed input artifact.
    /// 已解析的输入制品。
    input: Value,
    /// Parsed output artifact.
    /// 已解析的输出制品。
    output: Value,
}

/// Builds one exact Codex Responses route from the bundled manifest, redirected to `base_url`.
/// 从内置清单构建一条精确 Codex Responses 路由，并重定向到 `base_url`。
fn trace_fixture_route(base_url: &str) -> ProviderExecutionRoute {
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join(format!("{TRACE_FIXTURE_PROVIDER_ID}.json")))
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
        .find(|entry| entry.id == TRACE_FIXTURE_SERVICE_ENTRY_ID)
        .expect("the bundled codex subscription entry must exist")
        .clone();
    let credential_profile = entry
        .credential_profiles
        .first()
        .expect("the codex subscription entry must declare one credential profile")
        .clone();
    let connection = CredentialConnection {
        id: String::from("trace-fixture-connection"),
        provider_id: provider.id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: credential_profile.id.clone(),
        credential_mode: credential_profile.credential_mode,
        credential_id: String::from("trace-fixture-credential"),
        display_name: String::from("Trace fixture"),
        secret_reference: String::from("trace-fixture-secret-reference"),
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
    })
}

/// Asserts the envelope every request artifact shares.
/// 断言每份请求制品共享的外层结构。
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

/// Verifies the ordinary request artifact keeps its shared envelope shape.
/// 验证普通请求制品保持共享外层结构。
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
        body: json!({"model": TRACE_FIXTURE_MODEL_ID, "input": []}),
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
        transport_failure_phase: None,
        transport_failure_causes: None,
        transport_failure_timeout_ms: None,
        body_sha256: String::new(),
    };

    persist_provider_api_trace_snapshot(
        ProviderApiTraceRenderIdentity {
            record_all: true,
            record_errors: false,
            provider_id: String::from(TRACE_FIXTURE_PROVIDER_ID),
            session_scope_id: String::from("ordinary-trace-session"),
            model_id: String::from(TRACE_FIXTURE_MODEL_ID),
            route: trace_fixture_route("http://127.0.0.1:1"),
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
    // The send-failure slots are part of the record shape even when no send failure occurred.
    // 即使没有发生发送失败，发送失败槽位也属于记录形状的一部分。
    assert!(
        published.output["outcome"]
            .get("transport_failure_phase")
            .is_some_and(|value| value.is_null())
    );
    assert!(
        published.output["outcome"]
            .get("transport_failure_causes")
            .is_some_and(|value| value.is_null())
    );
    assert!(
        published.output["outcome"]
            .get("transport_failure_timeout_ms")
            .is_some_and(|value| value.is_null())
    );
}
