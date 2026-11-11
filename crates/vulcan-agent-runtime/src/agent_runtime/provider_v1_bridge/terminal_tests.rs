//! Actual domain acceptance of transport failures and cancellation projections.
//! 传输失败与取消投影的真实领域接纳验证。

use super::*;
use vulcan_session_core::v1::{
    ProviderRequestBudgetEvidence, ProviderRequestContextBreakdown, ProviderRequestTextMeasurement,
    context::LockedBaselineProjection,
    document::{ApiCompletion, ClientInputSubmission, PrepareAndStartApiInput, SessionDocument},
    event::{EventBody, ProviderProjectionProof},
    ids::Seq,
    projection::provider_visible_fingerprint,
};

/// Returns a controlled error for `category`, `transport` and `dispatch_outcome`, without secrets.
/// 为 `category`、`transport` 和 `dispatch_outcome` 返回不含秘密的受控错误。
fn failure(
    category: AiErrorCategory,
    transport: Option<AiTransportKind>,
    dispatch_outcome: DispatchOutcome,
) -> AiExecutionError {
    AiExecutionError {
        category,
        transport,
        dispatch_outcome,
        subcategory: None,
        stage: "transport".into(),
        http_status: None,
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: None,
        entry_id: None,
        model_id: None,
        message: "Controlled transport failure".into(),
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: false,
        retry_after: None,
    }
}

/// Returns one physical request identity accepted by the domain fixture below.
/// 返回下方领域夹具接纳的一次物理请求身份。
fn request() -> ProviderV1RequestFacts {
    ProviderV1RequestFacts {
        provider_call_id: "terminal-contract".into(),
        purpose: ApiPurposeKind::MainGeneration,
        continuation_action_id: None,
        request: ApiRequestSnapshot::new("provider", "model"),
        prepared_at_ms: 2,
    }
}

/// Finishes a real admitted API with `terminal` and asserts its terminal event is retained.
/// 使用 `terminal` 完成真实准入的 API，并断言其终态事件保留。
/// Return the completed document for usage replay and accounting assertions.
/// 返回已完成的文档，供用量重放和记账断言使用。
fn assert_domain_accepts(terminal: ProviderV1ApiTerminalFacts) -> SessionDocument {
    // The immutable projection and budget evidence are explicit fixture inputs, not production fallbacks.
    // 不可变投影和预算证据是明确的夹具输入，不是生产回退。
    let mut document = SessionDocument::new("terminal-fixture", "project", "Terminal", 1);
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("terminal-input").unwrap(),
                "Test transport closure",
                vec![],
            ),
            1,
        )
        .unwrap();
    let through_seq = document.events().last().unwrap().seq;
    let api = document
        .prepare_and_start_api(PrepareAndStartApiInput {
            purpose: terminal.request_facts.purpose,
            request: terminal.request_facts.request.clone(),
            projection: ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .unwrap(),
            },
            request_budget_evidence: ProviderRequestBudgetEvidence {
                request_assembly_fingerprint: format!("sha256:{}", "0".repeat(64)),
                model_route_fingerprint: format!("sha256:{}", "1".repeat(64)),
                toolset_fingerprint: format!("sha256:{}", "2".repeat(64)),
                system_prompt_fingerprint: format!("sha256:{}", "3".repeat(64)),
                request_text_measurement: ProviderRequestTextMeasurement {
                    character_count: 0,
                    utf8_byte_count: 0,
                    structural_units: 0,
                    history_message_count: 0,
                    prompt_in_history: false,
                    history_fingerprint: format!("sha256:{}", "4".repeat(64)),
                    prefix_invariant_fingerprint: format!("sha256:{}", "5".repeat(64)),
                    context_breakdown: ProviderRequestContextBreakdown::default(),
                },
            },
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms: 2,
        })
        .unwrap()
        .into_value()
        .api_call_id;
    document
        .complete_api_group(
            api,
            ApiCompletion {
                request: terminal.request_facts.request,
                usage: terminal.usage,
                status: terminal.status,
                end_reason: terminal.end_reason,
                error: terminal.error,
                continuation_eligibility: terminal.continuation_eligibility,
                timing: terminal.timing,
                provider_source: None,
                completed_at_ms: terminal.completed_at_ms,
            },
        )
        .expect("the real domain must accept the producer's terminal classification");
    assert!(document.active_api().is_none());
    assert!(document.events().iter().any(|event| matches!(&event.event, EventBody::ApiStats(stats) if stats.api_call_id == api && stats.status == terminal.status)));
    document
}

/// All normalized categories, dispatch outcomes and retry endings must produce valid domain terminal facts.
/// 所有规范类别、分派结果及重试结束组合都必须产生合法领域终态事实。
#[test]
fn failure_and_cancellation_projections_finish_real_domain_api() {
    for category in [
        AiErrorCategory::Authentication,
        AiErrorCategory::PermissionDenied,
        AiErrorCategory::PaymentRequired,
        AiErrorCategory::RateLimited,
        AiErrorCategory::ProviderOverloaded,
        AiErrorCategory::ProviderUnavailable,
        AiErrorCategory::Timeout,
        AiErrorCategory::InvalidRequest,
        AiErrorCategory::ContextLengthExceeded,
        AiErrorCategory::ContentPolicy,
        AiErrorCategory::Cancelled,
        AiErrorCategory::Unclassified,
    ] {
        for transport in [
            None,
            Some(AiTransportKind::Http),
            Some(AiTransportKind::ResponsesWebsocket),
        ] {
            for dispatch in [
                DispatchOutcome::NotDispatched,
                DispatchOutcome::Dispatched,
                DispatchOutcome::Unknown,
            ] {
                for exhausted in [false, true] {
                    let error = failure(category, transport, dispatch);
                    let terminal = ProviderV1ApiTerminalFacts::failed(
                        request(),
                        &CoreProviderUsageSnapshot::default(),
                        ApiTimings::default(),
                        Some(&error),
                        exhausted,
                        provider_failure_continuation_eligibility(Some(&error), exhausted),
                        3,
                    );
                    assert_domain_accepts(terminal);
                }
            }
        }
    }
    assert_domain_accepts(ProviderV1ApiTerminalFacts::cancelled(request(), 3));
    assert_domain_accepts(ProviderV1ApiTerminalFacts::failed(
        request(),
        &CoreProviderUsageSnapshot::default(),
        ApiTimings::default(),
        None,
        false,
        ContinuationEligibility::Forbidden,
        3,
    ));
}

/// Verifies terminal API receipts preserve a provider code or the host's HTTP deadline code.
/// 验证 API 终态回执保留上游错误码或宿主的 HTTP 截止时间代码。
#[test]
fn terminal_receipt_keeps_the_specific_http_error_code_and_endpoint() {
    // Set the host facts separately so upstream precedence is tested on the same projection.
    // 单独设置宿主事实，以便在同一投影上验证上游错误码的优先级。
    let mut error = failure(
        AiErrorCategory::ProviderUnavailable,
        Some(AiTransportKind::Http),
        DispatchOutcome::Unknown,
    );
    error.subcategory = Some("response_headers_timeout".into());
    error.message = "Provider request to `https://provider.invalid/v1/chat/completions` exceeded the response-header timeout.".into();
    error.retryable = true;

    for (upstream, expected, retry_exhausted) in [
        (None, "response_headers_timeout", false),
        (None, "response_headers_timeout", true),
        (
            Some("upstream_specific_error"),
            "upstream_specific_error",
            false,
        ),
    ] {
        error.upstream_code = upstream.map(str::to_owned);
        // Exercise the same domain event that the session error card reads.
        // 验证会话错误卡读取的同一领域事件。
        let document = assert_domain_accepts(ProviderV1ApiTerminalFacts::failed(
            request(),
            &CoreProviderUsageSnapshot::default(),
            ApiTimings::default(),
            Some(&error),
            retry_exhausted,
            provider_failure_continuation_eligibility(Some(&error), retry_exhausted),
            3,
        ));
        let recorded = document
            .events()
            .iter()
            .find_map(|entry| match &entry.event {
                EventBody::ApiStats(stats) => stats.error.as_ref(),
                _ => None,
            })
            .expect("terminal API receipt should carry the diagnostic");
        assert_eq!(recorded.error_code.as_deref(), Some(expected));
        assert!(
            recorded
                .message
                .contains("https://provider.invalid/v1/chat/completions")
        );
    }
}

/// Raw protocol usage survives public conversion, terminal projection, domain replay and goal accounting.
/// 原始协议用量经过公共转换、终态投影、领域重放与目标记账后保持一致。
#[test]
fn normalized_usage_round_trips_through_terminal_domain_and_lifetime_accounting() {
    use serde_json::json;
    use vulcan_llm_core::{
        UpstreamProtocol, UsageAccumulator, UsageRecordKind, standard_usage_profiles,
    };
    use vulcan_session_core::v1::lifetime_usage::SessionUsageBucket;
    // Include absent input/output, explicit zero and both disjoint and inclusive counters.
    // 覆盖输入或输出缺失、明确零值，以及互斥和包含式计数。
    let cases = [
        (
            UpstreamProtocol::AnthropicMessages,
            UsageRecordKind::JsonBody,
            json!({"usage":{"input_tokens":4,"cache_read_input_tokens":0,"cache_creation_input_tokens":449273,"output_tokens":18628}}),
            Some(449277),
            Some(18628),
            Some(467905),
        ),
        (
            UpstreamProtocol::OpenAiResponses,
            UsageRecordKind::JsonBody,
            json!({"usage":{"input_tokens":10000,"input_tokens_details":{"cached_tokens":2500},"output_tokens":5000,"output_tokens_details":{"reasoning_tokens":2500}}}),
            Some(10000),
            Some(5000),
            Some(12500),
        ),
        (
            UpstreamProtocol::OpenAiChatCompletions,
            UsageRecordKind::JsonBody,
            json!({"usage":{"prompt_tokens":10000,"prompt_tokens_details":{"cached_tokens":2500},"completion_tokens":5000,"completion_tokens_details":{"reasoning_tokens":2500}}}),
            Some(10000),
            Some(5000),
            Some(12500),
        ),
        (
            UpstreamProtocol::GeminiGenerateContent,
            UsageRecordKind::JsonBody,
            json!({"usageMetadata":{"promptTokenCount":10000,"cachedContentTokenCount":2500,"candidatesTokenCount":5000,"thoughtsTokenCount":2500}}),
            Some(10000),
            Some(7500),
            Some(15000),
        ),
        (
            UpstreamProtocol::GoogleCodeAssist,
            UsageRecordKind::JsonBody,
            json!({"response":{"usageMetadata":{"promptTokenCount":10000,"cachedContentTokenCount":2500,"candidatesTokenCount":5000,"thoughtsTokenCount":2500}}}),
            Some(10000),
            Some(7500),
            Some(15000),
        ),
        (
            UpstreamProtocol::GeminiInteractions,
            UsageRecordKind::SseEvent,
            json!({"event_type":"interaction.completed","interaction":{"usage":{"total_input_tokens":10000,"total_cached_tokens":2500,"total_output_tokens":5000,"total_thought_tokens":2500}}}),
            Some(10000),
            Some(7500),
            Some(15000),
        ),
        (
            UpstreamProtocol::OpenAiResponses,
            UsageRecordKind::JsonBody,
            json!({"usage":{"output_tokens":5000,"total_tokens":0}}),
            None,
            Some(5000),
            None,
        ),
        (
            UpstreamProtocol::GeminiGenerateContent,
            UsageRecordKind::JsonBody,
            json!({"usageMetadata":{"promptTokenCount":10000,"thoughtsTokenCount":2500}}),
            Some(10000),
            None,
            None,
        ),
    ];
    for (protocol, kind, record, input, output, budget) in cases {
        // Extraction and the public-to-core bridge are the same producers used by live streaming.
        // 提取及公共到 Core 的桥接使用实时流式处理的同一生产代码。
        let mut accumulator = UsageAccumulator::default();
        for profile in standard_usage_profiles(protocol) {
            if let Some(observation) = profile.extract(kind, &record).unwrap() {
                accumulator.apply(None, observation).unwrap();
            }
        }
        // Convert only the accumulated canonical dimensions across the public/Core boundary.
        // 仅将累计后的统一维度转换到公共与 Core 边界的另一侧。
        let usage = crate::agent_runtime::provider_transport_vulcan::public_usage_to_core(
            accumulator.usage(),
        );
        assert_eq!(usage.input_tokens, input);
        assert_eq!(usage.output_tokens, output);
        // Failed requests can carry billable usage and must preserve that same accounting contract.
        // 失败请求也可能产生计费用量，必须保留同一记账契约。
        let terminal = ProviderV1ApiTerminalFacts::failed(
            request(),
            &usage,
            ApiTimings::default(),
            None,
            false,
            ContinuationEligibility::Forbidden,
            3,
        );
        // Commit terminal facts through the same domain admission used by the runtime.
        // 通过运行时使用的同一领域准入提交终态事实。
        let document = assert_domain_accepts(terminal);
        // Serialize all stored parts, then rebuild derived state through the strict domain loader.
        // 序列化全部持久部件，再经严格领域加载器重建派生状态。
        let serialized = serde_json::to_vec(&(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            document.events().to_vec(),
        ))
        .unwrap();
        // Decode the complete stored document before invoking its strict reopening path.
        // 先解码完整持久文档，再调用其严格重开路径。
        let (metadata, lock, compactions, segments, events) =
            serde_json::from_slice(&serialized).unwrap();
        let reopened =
            SessionDocument::from_stored_parts(metadata, lock, compactions, segments, events)
                .unwrap()
                .0;
        // Locate usage by its event kind rather than a position that may evolve.
        // 按事件类型定位用量，避免依赖会演进的位置。
        let stats = reopened
            .events()
            .iter()
            .find_map(|event| {
                if let EventBody::ApiStats(stats) = &event.event {
                    Some(stats)
                } else {
                    None
                }
            })
            .expect("terminal usage must survive replay");
        // Lifetime and goal accounting consume the exact persisted terminal dimensions.
        // 生命周期与目标记账消费精确持久化的终态维度。
        let bucket = SessionUsageBucket::from_api(&stats.usage, &stats.timing).unwrap();
        assert_eq!(bucket.known_budget(), budget, "{protocol:?}");
        assert_eq!(stats.usage.input_tokens, input.map(|value| value as u64));
        assert_eq!(stats.usage.output_tokens, output.map(|value| value as u64));
        assert_eq!(
            bucket.total_tokens.samples,
            u64::from(usage.total_tokens.is_some() || input.zip(output).is_some())
        );
        assert_eq!(
            bucket.total_tokens.sum,
            usage
                .total_tokens
                .or_else(|| input.zip(output).map(|(i, o)| i + o))
                .unwrap_or(0) as u64
        );
    }
}
