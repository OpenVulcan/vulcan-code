//! Regression coverage for usage extraction and response-start ordering.
//! 用量抽取与响应开始排序的回归覆盖。

use super::*;
use serde_json::json;
use vulcan_ai_contract::AiStreamStateMachine;
use vulcan_ai_contract::{UsageValue, UsageValueSource};
use vulcan_llm_core::UsageField;

/// Verifies raw usage is retained but cannot precede the normalized response start.
/// 验证原始用量会被保留但不能先于规范化响应开始事件。
#[test]
fn profiled_usage_gate_releases_usage_after_response_start() {
    let mut gate = ProfiledUsageEventGate::default();
    let usage_event = AiStreamEvent::UsageUpdated {
        usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
    };
    assert!(
        gate.stage(vec![usage_event])
            .expect("one pending usage event should remain within the bound")
            .is_empty()
    );

    let start = AiStreamEvent::ResponseStarted {
        execution_id: String::from("execution"),
    };
    let mut state = AiStreamStateMachine::default();
    state.apply(&start).expect("start should be valid");
    let released = gate.observe(&start);
    assert_eq!(released.len(), 1);
    state
        .apply(&released[0])
        .expect("released usage should follow response start");
    gate.ensure_released()
        .expect("all pending usage should be released");
}

/// Verifies a transport cannot silently discard usage when normalization never starts.
/// 验证规范化从未开始时传输不能静默丢弃用量。
#[test]
fn profiled_usage_gate_rejects_missing_response_start() {
    let mut gate = ProfiledUsageEventGate::default();
    let usage_event = AiStreamEvent::UsageUpdated {
        usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
    };
    assert!(
        gate.stage(vec![usage_event])
            .expect("one pending usage event should remain within the bound")
            .is_empty()
    );
    assert!(gate.ensure_released().is_err());
}

/// Verifies pre-start usage staging fails closed at its explicit event ceiling.
/// 验证响应开始前的用量暂存会在显式事件上限处封闭失败。
#[test]
fn profiled_usage_gate_rejects_unbounded_pre_start_events() {
    // gate starts before normalization and therefore retains extracted usage temporarily.
    // gate 在规范化前启动，因此会临时保留提取的用量。
    let mut gate = ProfiledUsageEventGate::default();
    // pending_events fills the complete reviewed staging capacity in one bounded batch.
    // pending_events 在一个有界批次中填满全部已审核暂存容量。
    let pending_events = (0..PROFILED_USAGE_GATE_MAX_PENDING_EVENTS)
        .map(|_| AiStreamEvent::UsageUpdated {
            usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
        })
        .collect::<Vec<_>>();
    assert!(
        gate.stage(pending_events)
            .expect("the exact pending usage ceiling should be accepted")
            .is_empty()
    );
    // overflow is the first event that would make the retained vector unbounded.
    // overflow 是首个会使保留向量越过上限的事件。
    let overflow = AiStreamEvent::UsageUpdated {
        usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
    };

    // error proves the gate rejects overflow instead of dropping evidence or growing memory.
    // error 证明门控会拒绝溢出，而不是丢弃证据或继续增长内存。
    let error = gate
        .stage(vec![overflow])
        .expect_err("usage staging beyond the ceiling must fail closed");
    assert!(error.message.contains("256-event limit"));
}

/// Verifies failed-response profiles extract exact usage before error formatting discards structure.
/// 验证失败响应 Profile 会在错误格式化丢失结构前提取精确用量。
#[test]
fn error_json_body_profile_extracts_billable_usage() {
    let profiles = vec![UsageExtractionProfile {
        profile_id: String::from("test.error-usage.v1"),
        revision: 1,
        record_kind: UsageRecordKind::ErrorJsonBody,
        update_kind: vulcan_ai_contract::UsageUpdateKind::Final,
        event_type_pointer: None,
        event_type_value: None,
        required_pointer: Some(String::from("/error/usage")),
        bindings: vec![vulcan_llm_core::UsageFieldBinding {
            field: vulcan_llm_core::UsageField::InputTokens,
            pointers: vec![String::from("/error/usage/input_tokens")],
            encoding: vulcan_llm_core::UsageValueEncoding::UnsignedInteger,
        }],
        derivations: Vec::new(),
    }];
    let mut accumulator = UsageAccumulator::default();

    let events = extract_profiled_usage_events(
        &profiles,
        UsageRecordKind::ErrorJsonBody,
        r#"{"error":{"usage":{"input_tokens":17}}}"#,
        &mut accumulator,
    )
    .expect("registered error JSON should extract");

    let AiStreamEvent::UsageUpdated { usage } = &events[0] else {
        panic!("error usage extraction should emit one usage event");
    };
    assert_eq!(
        usage.input_tokens,
        vulcan_ai_contract::UsageValue::Known {
            value: 17,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        }
    );
}

/// Verifies unregistered error records leave plain-text provider errors unchanged.
/// 验证未注册错误记录不会改变纯文本 Provider 错误。
#[test]
fn unregistered_error_usage_kind_does_not_parse_plain_text() {
    let profiles = vec![UsageExtractionProfile {
        profile_id: String::from("test.success-usage.v1"),
        revision: 1,
        record_kind: UsageRecordKind::JsonBody,
        update_kind: vulcan_ai_contract::UsageUpdateKind::Final,
        event_type_pointer: None,
        event_type_value: None,
        required_pointer: None,
        bindings: Vec::new(),
        derivations: Vec::new(),
    }];
    let mut accumulator = UsageAccumulator::default();

    assert!(
        extract_profiled_usage_events(
            &profiles,
            UsageRecordKind::ErrorJsonBody,
            "gateway timeout",
            &mut accumulator,
        )
        .expect("unregistered record kinds should be ignored before JSON parsing")
        .is_empty()
    );
}

/// Returns the first usage profile one bundled manifest declares for a record kind.
/// 返回单个内置清单为某一记录类型声明的首个用量 Profile。
fn bundled_usage_profile(
    provider_id: &str,
    record_kind: UsageRecordKind,
) -> UsageExtractionProfile {
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
        .unwrap_or_else(|_| panic!("missing bundled manifest `{provider_id}`"));
    let manifest = crate::provider_manifest::compile_manifest_text(
        &text,
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("bundled manifest must compile");
    manifest
        .provider()
        .service_entries
        .iter()
        .flat_map(|entry| entry.protocol_routes.iter())
        .flat_map(|route| route.usage_extraction_profiles.iter())
        .find(|profile| profile.record_kind == record_kind)
        .cloned()
        .unwrap_or_else(|| panic!("`{provider_id}` must declare a {record_kind:?} usage profile"))
}

/// Verifies Alibaba Responses billing partitions remain typed and separate from top-level totals.
/// 验证阿里云 Responses 计费分区保持强类型并与顶层合计分离。
#[test]
fn alibaba_responses_profile_preserves_billing_details() {
    let profile = bundled_usage_profile("alibaba", UsageRecordKind::JsonBody);
    let observation = profile
        .extract(
            UsageRecordKind::JsonBody,
            &json!({
                "id": "resp_alibaba",
                "model": "qwen-plus",
                "usage": {
                    "input_tokens": 52,
                    "output_tokens": 1280,
                    "total_tokens": 1332,
                    "x_details": [{
                        "input_tokens": 52,
                        "output_tokens": 1280,
                        "total_tokens": 1332,
                        "image_tokens": 12,
                        "input_tokens_details": {
                            "text_tokens": 40,
                            "image_tokens": 12
                        },
                        "output_tokens_details": {"reasoning_tokens": 1122},
                        "x_billing_type": "response_api"
                    }]
                }
            }),
        )
        .expect("Alibaba usage should parse")
        .expect("Alibaba usage should match");
    assert_eq!(observation.usage.billing_details.len(), 1);
    let detail = &observation.usage.billing_details[0];
    assert_eq!(detail.billing_type, "response_api");
    assert_eq!(detail.input_tokens, Some(52));
    assert_eq!(detail.output_tokens, Some(1280));
    assert_eq!(detail.total_tokens, Some(1332));
    assert_eq!(detail.text_tokens, Some(40));
    assert_eq!(detail.image_tokens, Some(12));
    assert_eq!(detail.reasoning_tokens, Some(1122));
    assert_eq!(
        observation
            .source_paths
            .get(&UsageField::BillingDetails)
            .map(String::as_str),
        Some("/usage/x_details")
    );

    profile
        .extract(
            UsageRecordKind::JsonBody,
            &json!({
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1,
                    "total_tokens": 2,
                    "x_details": [{
                        "input_tokens": 1,
                        "output_tokens": 1,
                        "total_tokens": 2,
                        "x_billing_type": "unregistered_type"
                    }]
                }
            }),
        )
        .expect_err("unregistered Alibaba billing type must fail");
}

/// Verifies OpenRouter Chat ignores provider-reported cost and retains token facts only.
/// 验证 OpenRouter Chat 忽略供应商上报金额，仅保留 Token 事实。
#[test]
fn openrouter_chat_profile_retains_token_facts_only() {
    let profile = bundled_usage_profile("openrouter", UsageRecordKind::JsonBody);
    let observation = profile
        .extract(
            UsageRecordKind::JsonBody,
            &json!({
                "id": "chatcmpl-openrouter",
                "model": "openai/gpt-5",
                "usage": {
                    "prompt_tokens": 100,
                    "completion_tokens": 20,
                    "total_tokens": 120,
                    "cost": 0.00125
                }
            }),
        )
        .expect("OpenRouter usage should parse")
        .expect("OpenRouter usage should match");
    assert!(matches!(
        &observation.usage.input_tokens,
        UsageValue::Known { value: 100, .. }
    ));
    assert!(matches!(
        &observation.usage.output_tokens,
        UsageValue::Known { value: 20, .. }
    ));
    assert!(matches!(
        &observation.usage.total_tokens,
        UsageValue::Known { value: 120, .. }
    ));
    assert!(
        !observation
            .source_paths
            .values()
            .any(|path| path == "/usage/cost")
    );
}

/// Verifies DeepSeek streaming usage reads cache hits and misses only from their registered paths.
/// 验证 DeepSeek 流式用量只从已注册路径读取缓存命中与未命中。
#[test]
fn deepseek_profile_uses_only_registered_cache_paths() {
    let profile = bundled_usage_profile("deepseek", UsageRecordKind::SseEvent);
    let observation = profile
        .extract(
            UsageRecordKind::SseEvent,
            &json!({
                "id": "response-1",
                "model": "deepseek-chat",
                "usage": {
                    "prompt_tokens": 100,
                    "prompt_cache_hit_tokens": 70,
                    "prompt_cache_miss_tokens": 30,
                    "completion_tokens": 9,
                    "total_tokens": 109
                }
            }),
        )
        .expect("DeepSeek usage should parse")
        .expect("usage record should match");
    assert_eq!(
        observation.usage.cache_read_tokens,
        UsageValue::Known {
            value: 70,
            source: UsageValueSource::Reported,
        }
    );
    assert_eq!(
        observation.source_paths[&UsageField::UncachedInputTokens],
        "/usage/prompt_cache_miss_tokens"
    );
}
