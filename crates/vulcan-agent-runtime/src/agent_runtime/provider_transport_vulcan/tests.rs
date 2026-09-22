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
