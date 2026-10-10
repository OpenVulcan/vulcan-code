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
        resolved_public_output_token_limit(Some(393_216)),
        DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT
    );
}

/// Verifies a model maximum below the default caps the request budget.
/// 验证低于默认值的模型最大值会为请求预算封顶。
#[test]
fn public_output_limit_is_capped_by_model_maximum() {
    assert_eq!(resolved_public_output_token_limit(Some(16_000)), 16_000);
    assert_eq!(
        resolved_public_output_token_limit(None),
        DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT
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

/// Verifies an uncovered host event blocks continuation reuse, using the real host-event producer.
/// 验证未被覆盖的宿主事件会阻止延续复用，且使用真实的宿主事件生产者。
///
/// # Notes
/// 说明
/// The reuse guard keys on the producer-owned message prefix. This test builds its host message
/// through the same synchronizer that production uses, so the guard and the producer are tied by
/// construction instead of both repeating the same literal. The uncovered case is asserted first:
/// reusing a continuation across a host event the cursor never covered would drop that event from
/// the provider's view.
/// 复用守卫依赖生产者持有的消息前缀。本测试通过生产同款的同步器构造宿主消息，使守卫与生产者由构造
/// 相连，而不是各自重复同一个字面量。先断言未覆盖的情形：跨越游标从未覆盖的宿主事件复用延续，会让
/// 该事件从供应商视野中消失。
#[test]
fn uncovered_host_event_blocks_continuation_reuse() {
    use vulcan_core::types::{
        ConversationMessage, ConversationPayload, ConversationSnapshot, ProviderContinuation,
    };

    // Route identities are frozen here only to satisfy the continuation ownership check.
    // 这里的路由标识仅用于满足延续归属检查。
    const PROVIDER_ID: &str = "openai";
    const PROTOCOL_PROFILE_ID: &str = "openai-responses.v1";
    const MODEL_ID: &str = "gpt-5";

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
    let mut with_host_event = ConversationSnapshot::default();
    synchronize_runtime_host_events(
        &mut with_host_event,
        &runtime_context_host_event_candidates(&injection),
        None,
    )
    .expect("typed expiry should synchronize");
    let host_message_id = with_host_event.messages[0].id.clone();
    assert!(
        request::is_runtime_host_event_message_id(host_message_id.as_str()),
        "the producer's own message id must be recognized by the reuse guard"
    );
    assert!(
        !request::is_runtime_host_event_message_id("ordinary-user-message"),
        "an ordinary message id must not be treated as a host event"
    );

    let text_message = |id: &str| ConversationMessage {
        id: String::from(id),
        payload: ConversationPayload::UserText {
            text: String::from("ordinary"),
            attachments: Vec::new(),
        },
    };
    let frozen = |last_local_message_id: &str| ProviderContinuation {
        provider_id: Some(String::from(PROVIDER_ID)),
        protocol_profile_id: Some(String::from(PROTOCOL_PROFILE_ID)),
        model_id: Some(String::from(MODEL_ID)),
        last_provider_message_id: Some(String::from("provider-message-1")),
        last_local_message_id: Some(String::from(last_local_message_id)),
        ..Default::default()
    };

    // covered first, host event uncovered after it: the guard must refuse the builder.
    // 先覆盖普通消息、宿主事件留在其后未被覆盖：守卫必须拒绝构建。
    let uncovered = ConversationSnapshot {
        messages: vec![
            text_message("covered-message"),
            text_message("later-message"),
            with_host_event.messages[0].clone(),
        ],
        continuation: frozen("covered-message"),
        ..Default::default()
    };
    assert!(
        request::build_public_continuation(
            &uncovered,
            &vulcan_core::types::ControllerConfig::default(),
            PROVIDER_ID,
            PROTOCOL_PROFILE_ID,
            MODEL_ID,
            0,
        )
        .is_none(),
        "an uncovered host event must block continuation reuse"
    );

    // Remedy: with the host message absent and the rewrite fingerprint taken from the same config,
    // the builder must reach a proven continuation. This is what makes the refusal above attributable
    // to the host-event boundary rather than to a proof that fails for both fixtures.
    // 反证：在宿主消息缺席、且改写指纹取自同一 config 时，构建器必须给出已证明的延续。正因如此，
    // 上面的拒绝才能归因于宿主事件边界，而不是两个夹具都失败的某一项证明。
    let config = vulcan_core::types::ControllerConfig::default();
    let mut proven = uncovered;
    proven.messages = vec![
        text_message("covered-message"),
        text_message("later-message"),
        text_message("ordinary-trailing-message"),
    ];
    proven.continuation = ProviderContinuation {
        context_rewrite_fingerprint: Some(config.provider_rewrite_fingerprint()),
        ..frozen("covered-message")
    };
    assert!(
        request::build_public_continuation(
            &proven,
            &config,
            PROVIDER_ID,
            PROTOCOL_PROFILE_ID,
            MODEL_ID,
            0,
        )
        .is_some(),
        "an ordinary uncovered suffix with a matching fingerprint must reuse the continuation"
    );
}
