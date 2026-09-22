//! Regression tests for the adapter entry points.
//! 适配器入口边界的回归测试。

use super::*;
use crate::agent_runtime::provider_runtime_context::{
    runtime_context_host_event_candidates, synchronize_runtime_host_events,
};
use serde_json::Value;

/// Verifies a catalog maximum does not become the default request output budget.
/// 验证目录最大值不会变成默认请求输出预算。
#[test]
fn public_output_limit_uses_default_below_catalog_maximum() {
    assert_eq!(
        resolved_public_output_token_limit(None, Some(393_216)),
        DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT
    );
}

/// Verifies user narrowing remains authoritative and cannot exceed the catalog maximum.
/// 验证用户收窄值保持权威且不能超过目录最大值。
#[test]
fn public_output_limit_honors_user_narrowing_and_catalog_cap() {
    assert_eq!(
        resolved_public_output_token_limit(Some(16_000), Some(393_216)),
        16_000
    );
    assert_eq!(
        resolved_public_output_token_limit(Some(500_000), Some(393_216)),
        393_216
    );
}

/// Verifies public failures preserve every serialized execution field instead of delimiter-splitting text.
/// 验证公共失败会保留全部序列化执行字段，而不是通过分隔符拆分文本。
#[test]
fn public_execution_failure_message_is_json() {
    let error = vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
        subcategory: Some(String::from("upstream")),
        stage: String::from("stream"),
        http_status: Some(503),
        transport: Some(vulcan_ai_contract::AiTransportKind::Http),
        websocket_close_code: None,
        upstream_code: Some(String::from("server_is_overloaded")),
        upstream_type: Some(String::from("server_error")),
        upstream_parameter: Some(String::from("model")),
        upstream_message: Some(String::from("upstream detail | key=value")),
        request_id: Some(String::from("req-123")),
        provider_id: Some(String::from("custom:provider")),
        entry_id: Some(String::from("entry-1")),
        model_id: Some(String::from("model-1")),
        message: String::from("Our servers are currently overloaded. Please try again later."),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: false,
        retry_after: None,
    };

    let serialized = public_execution_failure_message(&error);
    let value: Value = serde_json::from_str(&serialized).expect("public failure must be JSON");
    assert_eq!(value["upstream_code"], "server_is_overloaded");
    assert_eq!(value["upstream_message"], "upstream detail | key=value");
    assert_eq!(value["request_id"], "req-123");
    assert_eq!(value["message"], error.message);
}

/// Builds the normalized 401 an adapter sees before any credential recovery has run.
/// 构建适配器在凭据恢复运行之前看到的归一化 401。
fn rejected_unauthorized_failure() -> vulcan_ai_contract::AiExecutionError {
    vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Authentication,
        subcategory: Some(String::from("invalid_api_key")),
        stage: String::from("response_headers"),
        http_status: Some(401),
        transport: Some(vulcan_ai_contract::AiTransportKind::Http),
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: Some(String::from("openai")),
        entry_id: Some(String::from("api")),
        model_id: Some(String::from("gpt")),
        message: String::from("upstream rejected the credential"),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: false,
        retry_after: None,
    }
}

/// Verifies the host decision consumes the adapter's real authentication-recovery marker.
/// 验证宿主判定消费的是适配器真实产生的认证恢复标记。
///
/// # Notes
/// 说明
/// The runtime-level sibling test builds its own copy of the marker, so it would keep passing even if
/// the adapter stopped producing the subcategory the host keys on. This test instead runs the
/// adapter's own annotation and its own Core-error projection before asking the host decision, so the
/// marker value, its retryability and the host condition stay connected by construction.
/// 运行时层的同类测试自行复刻标记，因此即使适配器不再产生宿主所依赖的子类别它也仍会通过。本测试
/// 先执行适配器自身的标注与其自身的 Core 错误投影，再交给宿主判定，使标记取值、可重试性与宿主
/// 条件天然保持连接。
#[test]
fn host_retry_decision_accepts_the_adapters_real_credential_recovery_marker() {
    use crate::agent_runtime::provider_transport_stream_runtime::refreshed_oauth_credential_retry_decision;

    // unrecovered starts exactly as the adapter's normalized 401 arrives from wire decoding.
    // unrecovered 的初值正是适配器从线路解码拿到归一化 401 时的状态。
    let unrecovered = public_execution_failure("openai", rejected_unauthorized_failure());
    assert!(
        refreshed_oauth_credential_retry_decision(&unrecovered, 0, false).is_none(),
        "an unrecovered 401 must not authorize a credential replay"
    );
    let mut recovered = rejected_unauthorized_failure();
    streaming::mark_credential_refreshed_after_unauthorized(&mut recovered);
    // failure is the adapter's real Core-error projection of that annotated error.
    // failure 是该已标注错误经适配器真实 Core 错误投影的结果。
    let failure = public_execution_failure("openai", recovered);
    let decision = refreshed_oauth_credential_retry_decision(&failure, 0, false).expect(
        "the adapter's real authentication-recovery marker must authorize exactly one replay",
    );
    assert_eq!(decision.retry_number, 1);
    assert_eq!(decision.max_retries, 1);
    assert_eq!(decision.delay, std::time::Duration::ZERO);
}

/// Verifies typed TTL expiration is encoded as a User host message for non-V1 transport.
/// 验证强类型 TTL 失效事件在非 V1 传输中编码为 User 宿主消息。
#[test]
fn non_v1_expired_context_is_a_user_event() {
    let injection = vulcan_core::types::ProviderContextInjection {
        expired_events: vec![vulcan_core::types::ProviderContextExpiredEvent {
            event_id: String::from("context-expired:event-1"),
            source_id: String::from("context_injection:profile:hash-1"),
            occurrence_id: String::from("context_injection:profile:hash-1:occurrence:1"),
            tag: String::from("profile"),
            content_hash: String::from("hash-1"),
            source: String::from("context_injection"),
            target: String::from("model"),
            scope: String::from("turn"),
            turn_id: String::from("turn-1"),
            body: String::from("Context injection expired after TTL."),
        }],
        ..Default::default()
    };
    let mut snapshot = vulcan_core::types::ConversationSnapshot::default();
    let candidates = runtime_context_host_event_candidates(&injection);
    synchronize_runtime_host_events(&mut snapshot, &candidates, None)
        .expect("typed expiry should synchronize");

    assert_eq!(snapshot.messages.len(), 1);
    let vulcan_core::types::ConversationPayload::UserText { text, .. } =
        &snapshot.messages[0].payload
    else {
        panic!("expiry event should be a User message");
    };
    assert!(text.contains("<vulcan_context_expired"));
    assert!(text.contains("event_id=\"context-expired:event-1\""));
}
