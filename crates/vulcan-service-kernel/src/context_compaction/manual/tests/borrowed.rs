//! Real storage and physical API admission for manual summaries borrowing stopped business.
//! 借用停止业务的手动摘要真实存储及物理 API 准入。

use super::*;
use crate::agent_stream::provider_v1_sink::{
    ProviderV1CompletedAssistantText, ProviderV1FinishedDurability,
};
use vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecutionProfile;
use vulcan_context_compaction::{CompactionResult, CompactionTailSelectionReport};
use vulcan_session_core::v1::{
    ApiCallId, ApiCompletion, CollaborationOperationId, CollaborationRunControl,
    ContinuationRequest, PrepareAndStartApiInput, SetRunControlInput,
    context::LockedBaselineProjection,
    event::{
        ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
        AssistantSemanticStatus, ContinuationEligibility, ProviderProjectionProof,
    },
    projection::provider_visible_fingerprint,
};

fn start_api(handle: &SessionHandle, purpose: ApiPurposeKind) -> ApiCallId {
    let request = handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose,
                request: ApiRequestSnapshot::new("provider", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: document.events().first().unwrap().seq,
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: crate::current_v1_unix_millis().unwrap(),
            }
        })
        .unwrap();
    handle
        .prepare_and_start_api_durable(request)
        .unwrap()
        .value
        .api_call_id
}

async fn borrowed_fixture() -> (
    TestHostRuntimeRoot,
    SharedServiceBridgeState,
    Arc<SessionHandle>,
    ContextCompactionCommitMode,
) {
    let (root, state, handle, task_id) = fixture().await;
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("borrowed-service-work").unwrap(),
                "Keep unfinished work",
                vec![],
            ),
            4,
        )
        .unwrap();
    start_api(&handle, ApiPurposeKind::MainGeneration);
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| document.interrupt_active_api(crate::current_v1_unix_millis().unwrap()),
        )
        .unwrap();
    handle
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("borrowed-service-stopped").unwrap(),
            state: CollaborationRunControl::MainAndSubagents,
            changed_at_ms: crate::current_v1_unix_millis().unwrap(),
        })
        .unwrap();
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    let mode = begin(&state, handle.session_id(), task_id, source)
        .await
        .unwrap();
    assert!(matches!(
        mode,
        ContextCompactionCommitMode::FinishBorrowed { .. }
    ));
    (root, state, handle, mode)
}

fn completed_summary(handle: &SessionHandle) -> WebuiContextCompactionExecution {
    let api = start_api(handle, ApiPurposeKind::Compaction);
    let summary = "Retained unfinished task and decisions";
    let seq = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.append_assistant_text(
                    api,
                    summary,
                    AssistantSemanticStatus::Completed,
                    crate::current_v1_unix_millis().unwrap(),
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    let terminal = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api,
                    ApiCompletion {
                        request: ApiRequestSnapshot::new("provider", "model"),
                        usage: Default::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: crate::current_v1_unix_millis().unwrap(),
                    },
                )
            },
        )
        .unwrap();
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: vec![],
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: "borrowed-summary".into(),
        created_at: "1".into(),
        policy,
        tail_selection: CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count: 0,
            retained_message_ids: vec![],
            retained_token_estimate: 0,
            previewed_tool_output_count: 0,
            omitted_tool_output_token_estimate: 0,
            newest_segment_exceeds_nominal_budget: false,
        },
    };
    let mut durability = ProviderV1FinishedDurability::default();
    durability.api_terminal = Some(terminal);
    durability.completed_assistant_text = Some(ProviderV1CompletedAssistantText {
        api_call_id: api,
        seq,
        text: summary.into(),
    });
    durability.locked_baseline = Some(LockedBaselineProjection::empty());
    WebuiContextCompactionExecution {
        execution: PromptCompactionExecution {
            compaction_result: CompactionResult::from_model_summary(&plan, summary),
            fallback_used: false,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            provider_id: None,
            model_id: None,
            execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
            input_prune_report: None,
        },
        retained_from_seq: Seq::try_new(1).unwrap(),
        durability,
    }
}

#[tokio::test]
async fn borrowed_manual_success_preserves_business_and_queued_input() {
    let (root, state, handle, mode) = borrowed_fixture().await;
    let business = handle
        .read(|document| document.active_turn().unwrap().clone())
        .unwrap();
    let execution = completed_summary(&handle);
    let queued = handle
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queued-during-manual").unwrap(),
                "After compaction",
                vec![],
            ),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap()
        .value;
    let result = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(vec![]),
        &state,
        &crate::tests::test_support::test_agent_runtime_bridge(),
        handle.session_id(),
        SessionRunPhase::Stopped,
        "borrowed-success",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        mode,
        execution,
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        ContextCompactionFinishOutcome::Committed { .. }
    ));
    handle
        .read(|document| {
            assert_eq!(document.active_turn().unwrap().turn_id, business.turn_id);
            assert!(!document.active_turn().unwrap().closed);
            assert!(document.active_api().is_none());
            assert!(document.runtime_lock.current_compaction_id.is_some());
            assert!(document.metadata.context_maintenance.is_none());
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainAndSubagents
            );
            assert!(document.open_turn_awaits_continue());
            assert!(
                document
                    .metadata
                    .input_state
                    .queued_inputs
                    .iter()
                    .any(|input| input.input_id == queued.input_id)
            );
        })
        .unwrap();
    assert!(
        !state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&handle.session_id())
    );
    assert!(!crate::webui_session_kernel_has_active_turn(
        &state.lock().unwrap(),
        handle.session_id()
    ));
    let loaded =
        vulcan_workspace_store::workspace_store::load_session_v1_full(&root.v1_storage_locator())
            .unwrap();
    assert!(loaded.document.open_turn_awaits_continue());
    assert!(!loaded.document.active_turn().unwrap().closed);
}

#[tokio::test]
async fn borrowed_manual_failure_and_stale_callback_preserve_business() {
    for after_api in [false, true] {
        let (_root, state, handle, mode) = borrowed_fixture().await;
        let business = handle
            .read(|document| document.active_turn().unwrap().turn_id)
            .unwrap();
        if after_api {
            let api = start_api(&handle, ApiPurposeKind::Compaction);
            handle
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::ApiCompleted,
                    |document| {
                        document.fail_manual_compaction_api(
                            api,
                            "fixture failure",
                            crate::current_v1_unix_millis().unwrap(),
                        )
                    },
                )
                .unwrap();
        }
        fail_webui_context_compaction_execution(
            &ServiceEventEmitter::new(vec![]),
            &state,
            handle.session_id(),
            SessionRunPhase::Stopped,
            "borrowed-failure",
            "",
            MANUAL_CONTEXT_COMPACTION_TRIGGER,
            mode,
            "fixture failure",
        )
        .unwrap();
        handle.read(|document| {
            assert!(document.open_turn_awaits_continue());
            assert_eq!(document.active_turn().unwrap().turn_id, business);
            assert!(document.events().iter().any(|event| event.turn_id == Some(business)
                && matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_compaction_failed")));
        }).unwrap();
        handle
            .continue_turn(
                ContinuationRequest::new(
                    ClientActionId::try_new("continue-after-manual-failure").unwrap(),
                    "",
                    vec![],
                ),
                crate::current_v1_unix_millis().unwrap(),
            )
            .unwrap();
        let before = handle.read(|document| document.memory_revision()).unwrap();
        assert!(
            fail_webui_context_compaction_execution(
                &ServiceEventEmitter::new(vec![]),
                &state,
                handle.session_id(),
                SessionRunPhase::Stopped,
                "old-failure",
                "",
                MANUAL_CONTEXT_COMPACTION_TRIGGER,
                mode,
                "stale callback"
            )
            .is_err()
        );
        assert_eq!(
            handle.read(|document| document.memory_revision()).unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn borrowed_manual_post_commit_failure_keeps_committed_summary() {
    let (_root, state, handle, mode) = borrowed_fixture().await;
    let ContextCompactionCommitMode::FinishBorrowed { owner, .. } = mode else {
        unreachable!()
    };
    let execution = completed_summary(&handle);
    let input = super::super::super::completion::verified_compaction_commit_input(
        &execution.execution,
        &execution.durability,
        execution.retained_from_seq,
    )
    .unwrap();
    let node = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ContextChanged,
            |document| {
                document.record_borrowed_manual_compaction(
                    owner,
                    input,
                    crate::current_v1_unix_millis().unwrap(),
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    fail_webui_context_compaction_execution(
        &ServiceEventEmitter::new(vec![]),
        &state,
        handle.session_id(),
        SessionRunPhase::Stopped,
        "post-commit-failure",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        mode,
        "Fixture VMM publication failure",
    )
    .unwrap();
    handle.read(|document| {
        assert_eq!(document.runtime_lock.current_compaction_id, Some(node));
        assert!(document.open_turn_awaits_continue());
        let api = execution.durability.api_terminal.as_ref().unwrap().value.api_call_id;
        assert!(document.events().iter().any(|event| matches!(&event.event,
            EventBody::ApiStats(stats) if stats.api_call_id == api && stats.status == ApiCallStatus::Completed)));
    }).unwrap();
}

#[tokio::test]
async fn borrowed_manual_restart_interrupts_summary_not_business() {
    let (root, state, handle, _mode) = borrowed_fixture().await;
    let session_id = handle.session_id();
    let business = handle
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    start_api(&handle, ApiPurposeKind::Compaction);
    let worker = state
        .lock()
        .unwrap()
        .active_stream_tasks
        .remove(&session_id)
        .unwrap();
    worker.abort_and_wait().await.unwrap();
    let controller = state.lock().unwrap().controller_runtime.clone().unwrap();
    drop(handle);
    drop(state);
    let mut restarted = test_service_state_with_parent_session(session_id);
    restarted.controller_runtime = Some(controller);
    let restarted = Arc::new(Mutex::new(restarted));
    crate::conversation_drafts_v1::service_session_draft_v1(
        restarted.clone(),
        vulcan_service_api::SessionDraftReadV1Request {
            session_id: session_id.to_string(),
        },
    )
    .await
    .unwrap();
    let loaded =
        vulcan_workspace_store::workspace_store::load_session_v1_full(&root.v1_storage_locator())
            .unwrap();
    assert_eq!(loaded.document.active_turn().unwrap().turn_id, business);
    assert!(loaded.document.open_turn_awaits_continue());
    assert!(loaded.document.active_api().is_none());
    assert!(loaded.document.metadata.context_maintenance.is_none());
    assert!(
        !restarted
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&session_id)
    );
}

#[tokio::test]
async fn manual_admission_rechecks_stream_after_preparation_drain() {
    let (_root, state, handle, task_id) = fixture().await;
    let session_id = handle.session_id();
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    let before = handle.read(|document| document.memory_revision()).unwrap();
    let preparation = handle.acquire_request_preparation_settlement().await;
    let mut admission = Box::pin(begin(&state, session_id, task_id, source));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut admission)
            .await
            .is_err()
    );
    let worker = state
        .lock()
        .unwrap()
        .active_stream_tasks
        .remove(&session_id)
        .unwrap();
    worker.abort_and_wait().await.unwrap();
    drop(preparation);
    assert!(admission.await.is_err());
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        before
    );
}
