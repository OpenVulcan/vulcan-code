//! Workspace-store regression tests extracted from the storage implementation module.
//! 从存储实现模块中抽取的工作区存储回归测试。

use super::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionEventKind,
    append_segmented_session_message_update, create_segmented_session_in_location,
    ensure_runtime_log_from_segmented_context, read_segmented_session_event_window, read_state,
};
use super::session_addon_store::{read_session_addon_state, write_session_addon_state};
use super::*;
use crate::conversation_protocol::SessionProviderContinuationState;
use crate::model::{SessionAddonNamespace, SessionAddonState, SessionRuntimeControls};
use vulcan_context::{SessionConversationRecord, SessionMessageKind};
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
    SessionLifecycleBeginRequest, SessionLifecycleFinishRequest, SessionLifecycleHook,
    SessionLifecycleRunner, SessionTurnTerminalStatus,
};
use vulcan_session_core::SessionNodeId;

/// No-op session lifecycle hook adapter used by workspace-store session lifecycle runner tests.
/// workspace-store session lifecycle runner 测试使用的空 hook 适配器。
struct NoopSessionLifecycleHook;

impl SessionLifecycleHook for NoopSessionLifecycleHook {
    /// Ignores appended session boundary events.
    /// 忽略已追加的 会话边界事件。
    fn after_event_appended(&self, _event: &SessionBoundaryEvent) -> Result<(), String> {
        Ok(())
    }
}

/// Verifies complete provider traces are published only inside the exact owning session temp directory.
/// 验证完整 Provider 追踪只会发布到精确所属会话的临时目录中。
#[test]
fn provider_request_trace_is_persisted_in_owning_session_temp_directory() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-provider-request-trace-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7020);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("provider-trace-project"),
            created_date: String::from("2026-08-13"),
            created_at: String::from("100"),
            project_id: Some(String::from("provider-trace-project")),
            title: Some(String::from("provider trace")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create provider trace session");
    let trace = serde_json::json!({
        "schema_version": "vulcan.provider-request-trace.v1",
        "final_wire_request": {"body": {"model": "test-model"}},
    });

    let trace_path = persist_provider_request_trace(
        &root,
        session_id,
        "100-000001-00000000-0000-0000-0000-000000000001",
        &trace,
    )
    .expect("persist provider request trace");

    assert_eq!(
        trace_path,
        session_dir
            .join("temp")
            .join(PROVIDER_REQUEST_TRACE_DIRECTORY)
            .join("100-000001-00000000-0000-0000-0000-000000000001.json")
    );
    let restored = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(&trace_path).expect("read provider request trace"),
    )
    .expect("decode provider request trace");
    assert_eq!(restored, trace);
    assert!(
        persist_provider_request_trace(&root, session_id, "../escape", &trace).is_err(),
        "path-like trace identities must be rejected"
    );

    std::fs::remove_dir_all(&root).expect("remove provider trace test root");
}

/// Verifies configured-inference traces are atomically isolated by their authenticated consumer profile.
/// 验证已配置推理追踪会按已认证消费者配置档原子隔离。
#[test]
fn configured_inference_provider_request_trace_is_consumer_scoped() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-configured-inference-trace-test-{}",
        uuid::Uuid::new_v4()
    ));
    let trace = serde_json::json!({
        "schema_version": "vulcan.configured-inference-provider-request-trace.v1",
        "api_request": {"body": {"reasoning": {"effort": "none"}}},
    });

    let trace_path = persist_configured_inference_provider_request_trace(
        &root,
        "vmm",
        "100-000001-00000000-0000-0000-0000-000000000001",
        &trace,
    )
    .expect("persist configured-inference provider request trace");

    assert_eq!(
        trace_path,
        root.join("diagnostics")
            .join(CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_DIRECTORY)
            .join("vmm")
            .join("100-000001-00000000-0000-0000-0000-000000000001.json")
    );
    let restored = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(&trace_path).expect("read configured-inference trace"),
    )
    .expect("decode configured-inference trace");
    assert_eq!(restored, trace);
    assert!(
        persist_configured_inference_provider_request_trace(
            &root,
            "../escape",
            "valid-stem",
            &trace,
        )
        .is_err(),
        "path-like consumer profiles must be rejected"
    );

    std::fs::remove_dir_all(&root).expect("remove configured-inference trace test root");
}

/// Verifies freshly loaded runtime state drops stale process-scoped precheck latches.
/// 验证刚加载的运行态会丢弃陈旧的进程级预检查闩锁。
#[test]
fn loaded_runtime_state_drops_process_scoped_precheck_latch() {
    let mut runtime_state = SessionRuntimeStateEntry {
        active_model_id: Some(String::from("model-alpha")),
        context_start_message_id: Some(String::from("message-1")),
        active_precheck_turn_id: Some(String::from("turn-stale")),
        updated_at: String::from("100"),
        ..SessionRuntimeStateEntry::default()
    };

    let changed = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
        .expect("runtime state sanitization should succeed");

    assert!(changed);
    assert_eq!(
        runtime_state.active_model_id.as_deref(),
        Some("model-alpha")
    );
    assert_eq!(
        runtime_state.context_start_message_id.as_deref(),
        Some("message-1")
    );
    assert!(runtime_state.active_precheck_turn_id.is_none());
}

/// Verifies restart sanitization turns stale guided rows back into user-controlled ready rows.
/// 验证重启清理会把陈旧引导行恢复为用户可控的就绪行。
#[test]
fn loaded_runtime_state_restores_process_scoped_guided_prompt_latches() {
    let panel_entry = crate::model::PendingPromptPanelEntry {
        id: String::from("panel-guided"),
        prompt: String::from("continue"),
        hidden_model_guidance: None,
        preview: String::from("continue"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: crate::model::PendingPromptRuntimeOptions::default(),
        state: crate::model::PendingPromptPanelEntryState::Inserted,
        created_at: String::from("100"),
    };
    let linked_internal_entry = crate::model::PendingSessionPromptEntry {
        id: String::from("internal-guided"),
        prompt: String::from("continue"),
        hidden_model_guidance: None,
        preview: String::from("continue"),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: crate::model::PendingPromptRuntimeOptions::default(),
        source: crate::model::PendingSessionPromptSource::UserInterrupt,
        task_id: None,
        panel_entry_id: Some(String::from("panel-guided")),
        created_at: String::from("101"),
    };
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.ui_state.pending_prompt_panel_queue = vec![panel_entry];
    runtime_state.ui_state.pending_prompt_queue = vec![linked_internal_entry];

    let changed = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
        .expect("runtime state guided-prompt sanitization should succeed");

    assert!(changed);
    assert!(runtime_state.ui_state.pending_prompt_queue.is_empty());
    assert_eq!(
        runtime_state.ui_state.pending_prompt_panel_queue[0].state,
        crate::model::PendingPromptPanelEntryState::Ready
    );
}

/// Verifies legacy top-level usage becomes audit-only ledger evidence and is not written again.
/// 验证旧版顶层用量会转为账本仅审计证据且不会再次写出。
#[test]
fn loaded_runtime_state_migrates_top_level_usage_into_ledger_audit_evidence() {
    let last_usage = ProviderUsageSnapshot {
        input_tokens: Some(80),
        output_tokens: Some(20),
        total_tokens: Some(100),
        ..ProviderUsageSnapshot::default()
    };
    let successful_usage = ProviderUsageSnapshot {
        input_tokens: Some(70),
        output_tokens: Some(10),
        total_tokens: Some(80),
        ..ProviderUsageSnapshot::default()
    };
    let mut runtime_state = SessionRuntimeStateEntry {
        last_provider_usage: Some(last_usage.clone()),
        last_successful_provider_usage: Some(successful_usage.clone()),
        ..SessionRuntimeStateEntry::default()
    };

    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("legacy usage migration should succeed")
    );
    assert!(runtime_state.last_provider_usage.is_none());
    assert!(runtime_state.last_successful_provider_usage.is_none());
    let legacy = runtime_state
        .provider_call_ledger
        .legacy_import
        .as_ref()
        .expect("legacy audit evidence should be retained");
    assert_eq!(legacy.last_usage.as_ref(), Some(&last_usage));
    assert_eq!(
        legacy.last_successful_usage.as_ref(),
        Some(&successful_usage)
    );
    let serialized = serde_json::to_value(&runtime_state).expect("runtime state should serialize");
    let serialized = serialized
        .as_object()
        .expect("runtime state should serialize as an object");
    assert!(!serialized.contains_key("last_provider_usage"));
    assert!(!serialized.contains_key("last_successful_provider_usage"));
    assert!(
        !sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("completed migration should be idempotent")
    );
}

/// Verifies restart recovery invalidates a previously safe retry reference when Provider dispatch may have occurred.
/// 验证 Provider 分派可能已经发生时，重启恢复会使此前安全的重试引用失效。
#[test]
fn loaded_runtime_state_invalidates_retry_when_provider_dispatch_outcome_is_unknown() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("user-tail"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: String::from("继续处理"),
            metadata: None,
            created_at: String::from("100"),
        });
    assert!(runtime_state.provider_call_ledger.observe_started(
        vulcan_workbench_model::SessionProviderCallStarted {
            provider_call_id: String::from("call-restart"),
            identity: vulcan_workbench_model::SessionProviderCallIdentity {
                logical_request_id: String::from("execution-restart"),
                execution_id: String::from("execution-restart"),
                turn_id: Some(String::from("turn-restart")),
                attempt_index: 0,
                retry_number: 0,
                owner_kind: String::from("main_agent"),
                owner_id: String::from("session-restart"),
                owner_name_snapshot: String::from("主代理"),
                operation: String::from("conversation"),
                provider_id: String::from("provider"),
                provider_name_snapshot: String::from("Provider"),
                model_id: String::from("model"),
                model_name_snapshot: String::from("Model"),
                connection_id: String::from("connection"),
                service_entry_id: String::from("public-api"),
                protocol_profile_id: String::from("responses"),
                manifest_revision: 1,
                manifest_generation: 1,
                binding_source: String::from("manifest"),
                binding_rule: None,
                route_revision: String::from("route-1"),
                request_profiles: Vec::new(),
                response_profiles: Vec::new(),
            },
            prepared_at_ms: 100,
            request_assembly_fingerprint: None,
            context_generation_id: None,
            context_start_record_id: Some(String::from("user-tail")),
            model_route_fingerprint: None,
            toolset_fingerprint: None,
            system_prompt_fingerprint: None,
            request_text_measurement: None,
            budget_accounting: None,
        }
    ));
    runtime_state.execution_retry_snapshot =
        Some(vulcan_workbench_model::ExecutionRetrySnapshotRef {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 1,
            state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
            content_sha256: "0".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("execution-restart"),
            pending_action_label: String::from("dispatch_provider_request"),
            provider_route: Some(vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
                schema_version: 1,
                provider_id: String::from("provider"),
                connection_id: String::from("connection"),
                model_id: String::from("model"),
                protocol_profile_id: String::from("responses"),
                manifest_revision: 1,
                manifest_generation: 1,
                catalog_revision: String::from("catalog-1"),
                route_revision: String::from("route-1"),
            }),
            preparation_route_selection: None,
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: None,
            error_summary: None,
            updated_at: String::from("100"),
        });
    // previouslyBlockedRuntimeState proves restart recovery preserves an earlier, more specific safety reason.
    // previouslyBlockedRuntimeState 证明重启恢复会保留更早且更具体的安全原因。
    let mut previously_blocked_runtime_state = runtime_state.clone();
    // previouslyBlockedReference models a tool-side-effect boundary that is already more specific than dispatch uncertainty.
    // previouslyBlockedReference 模拟已经比“分派未知”更具体的工具副作用边界。
    let previously_blocked_reference = previously_blocked_runtime_state
        .execution_retry_snapshot
        .as_mut()
        .expect("previously blocked retry reference should exist");
    previously_blocked_reference.retryable = false;
    previously_blocked_reference.requires_manual_confirmation = true;
    previously_blocked_reference.blocked_reason = Some(String::from("tool_side_effect_in_flight"));
    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut previously_blocked_runtime_state)
            .expect("previously blocked restart reconciliation should succeed")
    );
    assert_eq!(
        previously_blocked_runtime_state
            .execution_retry_snapshot
            .as_ref()
            .and_then(|reference| reference.blocked_reason.as_deref()),
        Some("tool_side_effect_in_flight")
    );
    // emptyReasonRuntimeState proves a blank legacy reason is replaced by the exact restart diagnosis.
    // emptyReasonRuntimeState 证明空白旧原因会被精确的重启诊断替换。
    let mut empty_reason_runtime_state = runtime_state.clone();
    // emptyReasonReference models an unsafe legacy projection that did not retain a usable reason.
    // emptyReasonReference 模拟未保留可用原因的不安全旧投影。
    let empty_reason_reference = empty_reason_runtime_state
        .execution_retry_snapshot
        .as_mut()
        .expect("empty-reason retry reference should exist");
    empty_reason_reference.retryable = false;
    empty_reason_reference.requires_manual_confirmation = true;
    empty_reason_reference.blocked_reason = Some(String::from("   "));
    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut empty_reason_runtime_state)
            .expect("empty-reason restart reconciliation should succeed")
    );
    assert_eq!(
        empty_reason_runtime_state
            .execution_retry_snapshot
            .as_ref()
            .and_then(|reference| reference.blocked_reason.as_deref()),
        Some("provider_dispatch_outcome_unknown")
    );

    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("restart reconciliation should succeed")
    );
    let recovered = &runtime_state.provider_call_ledger.calls["call-restart"];
    assert_eq!(
        recovered.status,
        vulcan_workbench_model::SessionProviderCallStatus::Interrupted
    );
    assert_eq!(
        recovered.error_category.as_deref(),
        Some("process_interrupted")
    );
    // retryReference is the fail-closed projection that explains why replay now requires manual verification.
    // retryReference 是解释重放为何必须转为人工核验的失败关闭投影。
    let retry_reference = runtime_state
        .execution_retry_snapshot
        .as_ref()
        .expect("interrupted retry reference should remain available for diagnostics");
    assert_eq!(
        retry_reference.state,
        vulcan_workbench_model::ExecutionRetrySnapshotState::Invalidated
    );
    assert!(!retry_reference.retryable);
    assert!(retry_reference.requires_manual_confirmation);
    assert_eq!(
        retry_reference.blocked_reason.as_deref(),
        Some("provider_dispatch_outcome_unknown")
    );
    assert!(retry_reference.claimed_attempt_id.is_none());
    assert_eq!(
        retry_reference.error_summary.as_deref(),
        Some(
            "The previous process ended while a Provider request could already have been accepted; verify the upstream outcome before continuing."
        )
    );
    assert!(!retry_reference.replay_safety_proven());
    // An unknown external outcome must never expose Continue, even though the old reference was safe before restart.
    // 外部结果未知时绝不能暴露 Continue，即使旧引用在重启前曾经安全。
    assert!(!runtime_state.provider_call_ledger.continuation.eligible);
    assert_eq!(
        runtime_state
            .provider_call_ledger
            .continuation
            .canonical_tail_kind
            .as_deref(),
        Some("user")
    );
    let recovery_generation = runtime_state.provider_call_ledger.continuation.generation;
    assert!(
        !sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("idempotent restart reconciliation should succeed")
    );
    assert_eq!(
        runtime_state.provider_call_ledger.continuation.generation,
        recovery_generation
    );
}

/// Verifies restart recovery converts a persisted Continue claim with no dispatched Provider call into a blocked fresh terminal generation when no sidecar is retained.
/// 验证重启恢复在没有保留 sidecar 时，会把已持久化但未派发 Provider 调用的 Continue 领取转换为阻断的新终止代际。
#[test]
fn loaded_runtime_state_recovers_orphaned_continuation_claim_once() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("error-tail"),
            kind: SessionMessageKind::Error,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("Provider 错误"),
            body: String::from("请求尚未派发"),
            metadata: None,
            created_at: String::from("100"),
        });
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("failed-execution")),
                String::from("failed"),
                100,
                String::from("error"),
                true,
            )
    );
    let claimed_generation = runtime_state.provider_call_ledger.continuation.generation;
    let claimed_revision = runtime_state
        .provider_call_ledger
        .continuation
        .expected_ledger_revision;
    runtime_state
        .provider_call_ledger
        .claim_continuation(
            claimed_generation,
            claimed_revision,
            String::from("continue-submission"),
            101,
        )
        .expect("continuation claim should succeed");

    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("orphaned continuation recovery should succeed")
    );
    let recovered = &runtime_state.provider_call_ledger.continuation;
    // The ledger must not advertise a claimable boundary that has no sidecar safety proof.
    // 账本不能宣传没有 sidecar 安全证明的可领取边界。
    assert!(!recovered.eligible);
    assert_eq!(recovered.generation, claimed_generation + 1);
    assert_eq!(
        recovered.terminal_reason.as_deref(),
        Some("continuation_interrupted_after_restart")
    );
    assert_eq!(
        recovered.terminal_execution_id.as_deref(),
        Some("continue-submission")
    );
    assert!(recovered.claimed_at_ms.is_none());
    let recovery_generation = recovered.generation;
    assert!(
        !sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("orphaned continuation recovery should be idempotent")
    );
    assert_eq!(
        runtime_state.provider_call_ledger.continuation.generation,
        recovery_generation
    );
}

/// Verifies restart reconciliation treats assistant-output and empty tails as non-retryable dialogue boundaries.
/// 验证重启收敛会把助手输出尾部与空尾部视为不可重试的对话边界。
#[test]
fn loaded_runtime_state_rejects_assistant_output_and_empty_continuation_tails() {
    // assistantTailState starts with a stale claim whose actual latest canonical record is assistant output.
    // assistantTailState 初始持有一个过期领取，而其真实最新规范记录是助手输出。
    let mut assistant_tail_state = SessionRuntimeStateEntry::default();
    assistant_tail_state.conversation_state.records.extend([
        SessionConversationRecord {
            id: String::from("user-before-assistant"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户"),
            body: String::from("继续处理"),
            metadata: None,
            created_at: String::from("100"),
        },
        SessionConversationRecord {
            id: String::from("assistant-tail"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("AI"),
            body: String::from("处理完成"),
            metadata: None,
            created_at: String::from("101"),
        },
    ]);
    assert!(
        assistant_tail_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("completed-execution")),
                String::from("completed"),
                101,
                String::from("user"),
                true,
            )
    );
    // staleContinuation captures the claim token persisted before the process ended.
    // staleContinuation 捕获进程结束前持久化的领取令牌。
    let stale_continuation = assistant_tail_state
        .provider_call_ledger
        .continuation
        .clone();
    assistant_tail_state
        .provider_call_ledger
        .claim_continuation(
            stale_continuation.generation,
            stale_continuation.expected_ledger_revision,
            String::from("stale-assistant-claim"),
            102,
        )
        .expect("stale assistant-tail claim should be constructed");

    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut assistant_tail_state)
            .expect("assistant-tail restart reconciliation should succeed")
    );
    assert_eq!(
        assistant_tail_state
            .provider_call_ledger
            .continuation
            .canonical_tail_kind
            .as_deref(),
        Some("assistant")
    );
    assert!(
        !assistant_tail_state
            .provider_call_ledger
            .continuation
            .eligible
    );

    // emptyTailState models the same stale claim when no canonical dialogue record exists at all.
    // emptyTailState 模拟完全不存在规范对话记录时的同类过期领取。
    let mut empty_tail_state = SessionRuntimeStateEntry::default();
    assert!(
        empty_tail_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("missing-dialogue-execution")),
                String::from("failed"),
                200,
                String::from("user"),
                true,
            )
    );
    // emptyContinuation captures the exact claim generation and ledger revision before restart.
    // emptyContinuation 捕获重启前精确的领取代际与账本修订号。
    let empty_continuation = empty_tail_state.provider_call_ledger.continuation.clone();
    empty_tail_state
        .provider_call_ledger
        .claim_continuation(
            empty_continuation.generation,
            empty_continuation.expected_ledger_revision,
            String::from("stale-empty-claim"),
            201,
        )
        .expect("stale empty-tail claim should be constructed");

    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut empty_tail_state)
            .expect("empty-tail restart reconciliation should succeed")
    );
    assert_eq!(
        empty_tail_state
            .provider_call_ledger
            .continuation
            .canonical_tail_kind
            .as_deref(),
        Some("empty")
    );
    assert!(!empty_tail_state.provider_call_ledger.continuation.eligible);
}

/// Verifies an orphaned claimed sidecar cannot publish ledger eligibility before hydration refreezes it.
/// 验证孤立的已领取 sidecar 在 hydration 重冻前不能发布账本资格。
#[test]
fn loaded_runtime_state_keeps_orphaned_claim_ineligible_until_refreeze() {
    // runtimeState owns one exact error tail and a previously claimable terminal boundary.
    // runtimeState 持有一条精确错误尾部与一个此前可领取的终止边界。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("error-tail-with-sidecar"),
            kind: SessionMessageKind::Error,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("执行中断"),
            body: String::from("领取后服务重启"),
            metadata: None,
            created_at: String::from("100"),
        });
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("failed-execution")),
                String::from("failed"),
                100,
                String::from("error"),
                true,
            )
    );
    // continuation freezes the exact generation and revision that the old process claimed.
    // continuation 冻结旧进程领取的精确代际与修订号。
    let continuation = runtime_state.provider_call_ledger.continuation.clone();
    runtime_state
        .provider_call_ledger
        .claim_continuation(
            continuation.generation,
            continuation.expected_ledger_revision,
            String::from("continue-submission"),
            101,
        )
        .expect("continuation claim should succeed");
    runtime_state.execution_retry_snapshot =
        Some(vulcan_workbench_model::ExecutionRetrySnapshotRef {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: continuation.generation,
            state: vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed,
            content_sha256: "0".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("failed-execution"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: None,
            preparation_route_selection: Some(
                vulcan_workbench_model::ExecutionRetryPreparationRouteSelection {
                    active_provider_id: None,
                    active_connection_id: None,
                    active_model_id: None,
                },
            ),
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: Some(String::from("continue-submission")),
            error_summary: None,
            updated_at: String::from("101"),
        });

    assert!(
        sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state)
            .expect("orphaned claimed sidecar reconciliation should succeed")
    );

    assert!(!runtime_state.provider_call_ledger.continuation.eligible);
    assert_eq!(
        runtime_state
            .execution_retry_snapshot
            .as_ref()
            .map(|reference| reference.state),
        Some(vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed)
    );
}

/// Verifies persistence preview snippets keep line joining, fallback compaction, and truncation rules.
/// 验证持久化预览片段保持行拼接、回退压缩与截断规则。
#[test]
fn truncate_preview_lines_preserves_compaction_and_truncation_rules() {
    assert_eq!(
        truncate_preview_lines("  first line  \n\n second line \n third line ", 2, 80),
        "first line / second line"
    );
    assert_eq!(
        truncate_preview_lines(" alpha   beta\n\tgamma ", 0, 80),
        "alpha beta gamma"
    );
    assert_eq!(truncate_preview_lines("abcdef", 1, 4), "abc…");
}

/// Verifies session addon persistence writes only the addon document beside segmented state.
/// 验证会话附属状态持久化只写入分段状态旁的附属文档。
#[test]
fn session_addon_state_write_does_not_rewrite_segmented_state_document() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-session-addon-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7011);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-addon"),
            created_date: String::from("2026-06-16"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-addon")),
            title: Some(String::from("addon persist")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let state_document = state_document_from_runtime_state(
        &session_id.to_string(),
        &SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("old-provider")),
            active_model_id: Some(String::from("old-model")),
            updated_at: String::from("200"),
            ..SessionRuntimeStateEntry::default()
        },
    )
    .expect("state document");
    write_state(&session_dir, &state_document).expect("write state");
    let before_state = std::fs::read_to_string(session_dir.join("state.json")).expect("read state");

    let mut addon_state = SessionAddonState::new(session_id);
    addon_state.set_runtime_controls(
        &SessionRuntimeControls {
            active_agent_id: None,
            active_provider_id: Some(String::from("new-provider")),
            active_connection_id: Some(String::from("new-connection")),
            active_model_id: Some(String::from("new-model")),
            pending_model_selection: None,
            active_reasoning_effort: None,
            active_reasoning_effort_explicit: false,
            active_temperature: None,
            context_compaction_window_node_id: None,
            provider_route_lease: None,
            provider_tool_selections: Default::default(),
            provider_tool_enabled_capabilities: Default::default(),
            provider_extension_target_sets: Default::default(),
            provider_tool_settings_initialized: false,
            provider_tool_selections_overridden: false,
        },
        String::from("300"),
    );
    write_session_addon_state(&session_dir, &addon_state).expect("write addon");

    let after_state = std::fs::read_to_string(session_dir.join("state.json")).expect("read state");
    let restored_addon = read_session_addon_state(&session_dir)
        .expect("read addon")
        .expect("addon exists");

    assert_eq!(before_state, after_state);
    assert!(
        restored_addon
            .namespace(SessionAddonNamespace::RuntimeControls)
            .is_some()
    );
    assert_eq!(
        restored_addon
            .runtime_controls()
            .and_then(|controls| controls.active_provider_id),
        Some(String::from("new-provider"))
    );
}

/// Verifies runtime recovery does not synthesize provider context from segmented display JSONL.
/// 验证运行时恢复不会从分段展示 JSONL 合成 Provider 上下文。
#[test]
fn session_hydration_does_not_rebuild_runtime_log_from_segmented_jsonl() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-session-store-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7010);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-08"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("startup hydrate")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let user_message = SessionMessageEntry {
        id: String::from("message-user"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("User"),
        body: String::from("调用两个子代理"),
        metadata: None,
        created_at: String::from("101"),
    };
    let assistant_message = SessionMessageEntry {
        id: String::from("message-assistant"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: String::from("两个子代理均已完成"),
        metadata: None,
        created_at: String::from("102"),
    };
    append_segmented_session_message_update(
        &session_dir,
        &[user_message.clone(), assistant_message.clone()],
    )
    .expect("append transcript");
    let restored_conversation_state =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("rehydrate runtime log");

    assert!(restored_conversation_state.is_none());

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies records-only runtime state is not upgraded from display rows into provider-visible messages.
/// 验证仅有 records 的运行态不会从展示行升级为 Provider 可见消息。
#[test]
fn records_only_state_session_hydration_does_not_rebuild_provider_messages_from_segmented_jsonl() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-record-only-replay-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7012);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-24"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("record only replay")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let user_message = SessionMessageEntry {
        id: String::from("message-user"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("User"),
        body: String::from("之前的问题"),
        metadata: None,
        created_at: String::from("101"),
    };
    let assistant_message = SessionMessageEntry {
        id: String::from("message-assistant"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: String::from("之前的回答"),
        metadata: None,
        created_at: String::from("102"),
    };
    append_segmented_session_message_update(
        &session_dir,
        &[user_message.clone(), assistant_message.clone()],
    )
    .expect("append transcript");
    let records_only_runtime_state = SessionRuntimeStateEntry {
        active_model_id: Some(String::from("model-alpha")),
        conversation_state: SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("display-only-record"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("AI"),
                body: String::from("仅显示记录"),
                metadata: None,
                created_at: String::from("99"),
            }],
            continuation: Default::default(),
        },
        updated_at: String::from("103"),
        ..SessionRuntimeStateEntry::default()
    };

    let restored_conversation_state =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("rehydrate runtime log");

    assert_eq!(
        records_only_runtime_state.active_model_id.as_deref(),
        Some("model-alpha")
    );
    assert!(restored_conversation_state.is_none());
    assert_eq!(
        records_only_runtime_state.conversation_state.messages.len(),
        0
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records.len(),
        1
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records[0].body,
        "仅显示记录"
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies records-only runtime state is preserved when no segmented dialogue replay exists.
/// 验证没有分段对话可重放时会保留仅有 records 的运行态。
#[test]
fn records_only_state_without_replay_preserves_existing_records() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-record-only-no-replay-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7013);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-24"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("record only no replay")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let records_only_runtime_state = SessionRuntimeStateEntry {
        conversation_state: SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("display-only-record"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("AI"),
                body: String::from("仅显示记录"),
                metadata: None,
                created_at: String::from("99"),
            }],
            continuation: Default::default(),
        },
        updated_at: String::from("103"),
        ..SessionRuntimeStateEntry::default()
    };

    let restored_conversation_state =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("rehydrate runtime log");

    assert!(restored_conversation_state.is_none());

    assert!(
        records_only_runtime_state
            .conversation_state
            .messages
            .is_empty()
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records.len(),
        1
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records[0].body,
        "仅显示记录"
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies segmented context APIs write state and turn-context anchors into the new session directory.
/// 验证分段上下文 API 会把状态与轮次上下文锚点写入新会话目录。
#[test]
fn segmented_context_public_apis_write_state_and_turn_context() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-context-api-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7001);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-07"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("上下文测试")),
            title_source: Some(String::from("manual")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create segmented session");
    let message = SessionMessageEntry {
        id: String::from("assistant-context"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: String::from("当前上下文"),
        metadata: None,
        created_at: String::from("101"),
    };
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&[message])
            .expect("valid replayed conversation identities"),
        updated_at: String::from("101"),
        ..SessionRuntimeStateEntry::default()
    };

    persist_segmented_session_context_current_document(
        &root,
        "project-context",
        session_id,
        &runtime_state,
    )
    .expect("persist segmented current context");
    snapshot_segmented_session_context_before_user_prompt(
        &root,
        "project-context",
        session_id,
        &runtime_state,
    )
    .expect("snapshot segmented context");

    let state = read_state(&session_dir)
        .expect("read segmented state")
        .expect("expected state document");
    assert_eq!(state.session_id, session_id.to_string());
    assert!(!state.current_context.is_null());
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    assert!(
        lines
            .iter()
            .any(|line| line.kind == SegmentedSessionEventKind::TurnContext)
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies session boundary events create and append to the segmented session stream.
/// 验证 会话边界事件会创建并追加到分段会话流。
#[test]
fn segmented_session_boundary_public_api_appends_events() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-session-boundary-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7002);
    let event = SessionBoundaryEvent::new(
        "boundary-1",
        SessionNodeId::from(session_id),
        0,
        SessionBoundaryEventKind::TurnStarted,
        SessionEventScope::Turn,
        SessionEventVisibility::Hidden,
        100,
    )
    .with_turn_id("turn-1")
    .with_payload(serde_json::json!({ "source": "user_followup" }));

    append_segmented_session_boundary_events(
        &root,
        "project-context",
        session_id,
        "会话边界",
        &[event],
    )
    .expect("append session boundary events");

    let session_dir =
        find_existing_segmented_context_session_dir(&root, "project-context", session_id)
            .expect("find session dir")
            .expect("session dir should exist");
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    assert!(lines.iter().any(
        |line| line.kind == SegmentedSessionEventKind::SessionBoundary
            && line.payload["event_id"] == "boundary-1"
    ));

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies session boundary payload sequence is assigned by the segmented store.
/// 验证 会话边界 payload 序号由分段存储层统一分配。
#[test]
fn segmented_session_boundary_payload_sequence_matches_jsonl_sequence() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-session-boundary-seq-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7003);
    let thread_id = SessionNodeId::from(session_id);
    let first_turn_events = vec![
        SessionBoundaryEvent::new(
            "turn-1-started",
            thread_id,
            0,
            SessionBoundaryEventKind::TurnStarted,
            SessionEventScope::Turn,
            SessionEventVisibility::Hidden,
            100,
        )
        .with_turn_id("turn-1"),
        SessionBoundaryEvent::new(
            "turn-1-input",
            thread_id,
            1,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            101,
        )
        .with_turn_id("turn-1"),
    ];
    let second_turn_events = vec![
        SessionBoundaryEvent::new(
            "turn-2-started",
            thread_id,
            0,
            SessionBoundaryEventKind::TurnStarted,
            SessionEventScope::Turn,
            SessionEventVisibility::Hidden,
            200,
        )
        .with_turn_id("turn-2"),
        SessionBoundaryEvent::new(
            "turn-2-input",
            thread_id,
            1,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            201,
        )
        .with_turn_id("turn-2"),
    ];

    append_segmented_session_boundary_events(
        &root,
        "project-context",
        session_id,
        "会话边界序号",
        &first_turn_events,
    )
    .expect("append first session boundary turn");
    append_segmented_session_boundary_events(
        &root,
        "project-context",
        session_id,
        "会话边界序号",
        &second_turn_events,
    )
    .expect("append second session boundary turn");

    let session_dir =
        find_existing_segmented_context_session_dir(&root, "project-context", session_id)
            .expect("find session dir")
            .expect("session dir should exist");
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    let boundary_lines = lines
        .iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .collect::<Vec<_>>();
    assert_eq!(boundary_lines.len(), 4);
    for line in &boundary_lines {
        assert_eq!(line.payload["sequence"], serde_json::json!(line.seq));
        assert_eq!(line.payload["thread_id"], serde_json::json!(thread_id));
    }
    let restored_events =
        load_segmented_session_boundary_events(&root, "project-context", session_id, None, 8)
            .expect("restore session boundary events");
    assert_eq!(restored_events.len(), 4);
    assert_eq!(restored_events[0].thread_id, thread_id);
    assert_eq!(restored_events[0].sequence, boundary_lines[0].seq);
    assert_eq!(restored_events[3].turn_id.as_deref(), Some("turn-2"));

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies the workspace session boundary store adapter preserves host state fields.
/// 验证工作区会话边界存储适配器会保留宿主状态字段。
#[test]
fn workspace_segmented_boundary_store_runs_external_lifecycle_without_overwriting_runtime_state() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-session-store-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7004);
    let thread_id = SessionNodeId::from(session_id);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-07"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("session lifecycle runner")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    write_state(
        &session_dir,
        &super::segmented_session_store::SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: session_id.to_string(),
            updated_at: String::from("101"),
            runtime_state: serde_json::json!({ "phase": "existing-runtime" }),
            current_context: serde_json::json!({ "cursor": 42 }),
            session_boundary_state: serde_json::Value::Null,
        },
    )
    .expect("write existing state");
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        &root,
        "project-context",
        session_id,
        "session lifecycle runner",
    );
    let runner = SessionLifecycleRunner::new(store, NoopSessionLifecycleHook);

    runner
        .begin_turn(SessionLifecycleBeginRequest {
            thread_id,
            turn_id: String::from("turn-1"),
            accepted_events: vec![
                SessionBoundaryEvent::new(
                    "turn-started",
                    thread_id,
                    0,
                    SessionBoundaryEventKind::TurnStarted,
                    SessionEventScope::Turn,
                    SessionEventVisibility::Hidden,
                    200,
                )
                .with_turn_id("turn-1"),
            ],
        })
        .expect("begin external lifecycle");
    runner
        .finish_turn(SessionLifecycleFinishRequest {
            thread_id,
            turn_id: String::from("turn-1"),
            terminal_status: SessionTurnTerminalStatus::Completed,
            final_events: Vec::new(),
            terminal_event: Some(
                SessionBoundaryEvent::new(
                    "turn-completed",
                    thread_id,
                    0,
                    SessionBoundaryEventKind::TurnCompleted,
                    SessionEventScope::Turn,
                    SessionEventVisibility::Hidden,
                    201,
                )
                .with_turn_id("turn-1"),
            ),
        })
        .expect("finish external lifecycle");

    let state = read_state(&session_dir)
        .expect("state should read")
        .expect("state should exist");
    assert_eq!(state.runtime_state["phase"], "existing-runtime");
    assert_eq!(state.current_context["cursor"], 42);
    assert_eq!(state.session_boundary_state["status"], "idle");
    let restored_events =
        load_segmented_session_boundary_events(&root, "project-context", session_id, None, 8)
            .expect("session events should restore");
    assert_eq!(restored_events.len(), 2);
    assert_eq!(restored_events[0].sequence, 1);
    assert_eq!(restored_events[1].sequence, 2);

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies that default transcript windows keep tool details out of the mainline query profile.
/// 验证默认转录窗口会将工具明细排除在主线查询画像之外。
#[test]
fn default_transcript_window_filters_tool_detail_kinds() {
    assert!(persisted_kind_in_default_transcript_window(
        "assistant-output"
    ));
    assert!(persisted_kind_in_default_transcript_window(
        "summary-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-call-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-result-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-detail-output"
    ));
}

/// Verifies that the on-demand tool-detail stream accepts tool rows and hidden detail rows.
/// 验证按需工具明细流接收工具行与隐藏详情行。
#[test]
fn tool_detail_stream_filters_only_tool_detail_kinds() {
    assert!(persisted_kind_in_tool_detail_stream("tool-call-output"));
    assert!(persisted_kind_in_tool_detail_stream("tool-result-output"));
    assert!(persisted_kind_in_tool_detail_stream("tool-detail-output"));
    assert!(!persisted_kind_in_tool_detail_stream("assistant-output"));
    assert!(!persisted_kind_in_tool_detail_stream("summary-output"));
}

/// Verifies that context windows never request negative turn indexes.
/// 验证上下文窗口不会请求负数轮次索引。
#[test]
fn session_history_context_turn_bounds_clamp_lower_edge() {
    assert_eq!(session_history_context_turn_bounds(1, 2), (0, 3));
    assert_eq!(session_history_context_turn_bounds(-5, 2), (0, 2));
}

/// Verifies that context windows keep a stable inclusive turn span around the anchor.
/// 验证上下文窗口会围绕锚点保留稳定的闭区间轮次范围。
#[test]
fn session_history_context_turn_bounds_expand_around_anchor() {
    assert_eq!(session_history_context_turn_bounds(12, 2), (10, 14));
    assert_eq!(session_history_context_turn_bounds(12, -1), (12, 12));
}

/// Verifies that rebuilding runtime state from recent messages preserves provider continuation.
/// 验证基于最近消息重建运行时状态时会保留 Provider continuation。
#[test]
fn runtime_state_rebuild_from_recent_messages_preserves_continuation() {
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: SessionConversationState {
            continuation: SessionProviderContinuationState {
                provider_id: Some(String::from("openai")),
                protocol_profile_id: Some(String::from("openai-responses.v1")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-1")),
                last_local_message_id: Some(String::from("assistant-1")),
                context_cache_epoch_id: Some(String::from("epoch-1")),
                context_anchor_revision: Some(2),
                context_rewrite_fingerprint: Some(String::from("rewrite-1")),
                updated_at: Some(String::from("123")),
            },
            ..SessionConversationState::default()
        },
        ..SessionRuntimeStateEntry::default()
    };
    let messages = vec![SessionMessageEntry {
        id: String::from("message-1"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户指令"),
        body: String::from("继续。"),
        metadata: None,
        created_at: String::from("1"),
    }];

    let continuation = runtime_state.conversation_state.continuation.clone();
    let mut rebuilt = runtime_state;
    rebuilt.conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    rebuilt.conversation_state.continuation = continuation;

    assert_eq!(rebuilt.conversation_state.records.len(), 1);
    assert_eq!(
        rebuilt
            .conversation_state
            .continuation
            .last_provider_message_id
            .as_deref(),
        Some("resp-1")
    );
    assert_eq!(
        rebuilt
            .conversation_state
            .continuation
            .last_local_message_id
            .as_deref(),
        Some("assistant-1")
    );
}

/// Verifies old ordinary state embedding VMM ownership is rejected instead of silently ignored.
/// 验证嵌入 VMM 所有权的旧普通状态会被拒绝，而不是被静默忽略。
#[test]
fn embedded_historical_vmm_runtime_state_is_rejected() {
    // runtime_state models the removed ordinary-state ownership format.
    // runtime_state 模拟已移除的普通状态所有权格式。
    let runtime_state = serde_json::json!({
        "updated_at": "1",
        "vmm_session_memory": {
            "session_id": "session-a",
            "revision": 2
        }
    });

    let error = reject_embedded_historical_vmm_runtime_state(&runtime_state, "session-a")
        .expect_err("embedded VMM ownership must be rejected");

    assert!(error.contains("unsupported historical embedded VMM runtime state"));
    assert!(error.contains("automatic migration and fallback are disabled"));
}

/// Verifies the removed hidden-user VMM profile group is rejected instead of cleaned at render time.
/// 验证已移除的隐藏用户 VMM 画像分组会被拒绝，而不是在渲染时清理。
#[test]
fn historical_vmm_profile_context_injection_is_rejected() {
    // runtime_state is the decoded ordinary state containing the removed legacy group.
    // runtime_state 是包含已移除旧分组的已解码普通状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.context_injection_state.apply_inserts(
        vec![vulcan_context_injection::InjectionInsert::single(
            vulcan_context_injection::InjectionKind::Persistent,
            HISTORICAL_VMM_PROFILE_CONTEXT_INJECTION_TAG,
            "historical profile",
            "must not load",
            None,
        )],
        "1",
    );

    let error = reject_historical_vmm_profile_context_injection(&runtime_state, "session-a")
        .expect_err("historical profile group must be rejected");

    assert!(error.contains("unsupported historical VMM profile context injection state"));
    assert!(error.contains("automatic cleanup and fallback are disabled"));
}
