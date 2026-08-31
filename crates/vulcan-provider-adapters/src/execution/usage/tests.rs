//! Regression coverage for usage extraction and response-start ordering.
//! 用量抽取与响应开始排序的回归覆盖。

use super::*;
use std::collections::BTreeSet;
use vulcan_ai_contract::AiStreamStateMachine;

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

/// Verifies bundled manifest usage profiles equal the legacy provider functions (plan §15.4).
/// 验证内置清单 usage Profile 与旧版供应商函数等价（方案 §15.4）。
#[test]
fn bundled_usage_profiles_match_legacy_provider_functions() {
    for (provider_id, expected) in [
        (
            "alibaba",
            vulcan_llm_core::alibaba_responses_usage_profiles(),
        ),
        ("deepseek", vulcan_llm_core::deepseek_chat_usage_profiles()),
        (
            "openrouter",
            vulcan_llm_core::openrouter_chat_usage_profiles(),
        ),
    ] {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .unwrap_or_else(|_| panic!("missing bundled manifest `{provider_id}`"));
        let manifest = crate::provider_manifest::compile_manifest_text(
            &text,
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("bundled manifest must compile");
        // manifest_profiles keeps the exact declared order across every route, deduplicated
        // by profile id because multiple routes may reference the same declaration.
        // manifest_profiles 跨全部路由保持精确声明顺序，并按 Profile 标识去重
        // （多条路由可能引用同一声明）。
        let mut seen = BTreeSet::new();
        let manifest_profiles = manifest
            .provider()
            .service_entries
            .iter()
            .flat_map(|entry| entry.protocol_routes.iter())
            .flat_map(|route| route.usage_extraction_profiles.iter())
            .filter(|profile| profile.profile_id.starts_with(provider_id))
            .filter(|profile| seen.insert(profile.profile_id.clone()))
            .collect::<Vec<_>>();
        assert!(
            !manifest_profiles.is_empty(),
            "{provider_id} manifest must declare its usage profiles"
        );
        assert_eq!(
            manifest_profiles.len(),
            expected.len(),
            "{provider_id} profile count must match the legacy function"
        );
        for (actual, legacy) in manifest_profiles.iter().zip(expected.iter()) {
            assert_eq!(
                **actual, *legacy,
                "{provider_id} profile `{}` must equal the legacy declaration",
                actual.profile_id
            );
        }
    }
}
