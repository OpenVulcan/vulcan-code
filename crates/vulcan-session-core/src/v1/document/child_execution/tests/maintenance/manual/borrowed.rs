//! Borrowed manual compaction preserves the business lifecycle across every terminal path.
//! 借用手动压缩在各终态路径保留业务生命周期。

use super::*;
use crate::v1::collaboration::{
    CollaborationOperationId, CollaborationParticipant, CollaborationRunControl,
    ReserveChildTaskInput, SetRunControlInput,
};

fn open_business(allowed: bool) -> SessionDocument {
    let mut document = SessionDocument::new("borrowed-root", "project", "Manual", 1);
    document
        .submit_root_input_fixture("Retained work", vec![], 2)
        .unwrap();
    let api = document
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("provider", "model"),
            3,
        )
        .unwrap();
    if allowed {
        document.interrupt_active_api(4).unwrap();
    } else {
        complete_summary(&mut document, api, 4);
    }
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("manual-stop").unwrap(),
            state: CollaborationRunControl::MainAndSubagents,
            changed_at_ms: 5,
        })
        .unwrap();
    publish(&mut document);
    document
}

fn complete_summary(document: &mut SessionDocument, api: ApiCallId, stamp: i64) {
    document
        .complete_api_group(
            api,
            ApiCompletion {
                request: ApiRequestSnapshot::new("provider", "model"),
                usage: crate::v1::stats::UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: stamp,
            },
        )
        .unwrap();
}

#[test]
fn borrowed_manual_summary_preserves_business_and_continue_policy() {
    for allowed in [false, true] {
        for terminal in ["success", "api_failure", "before_api", "process_loss"] {
            let mut document = open_business(allowed);
            let original = document.events().to_vec();
            let business = document.active_turn().unwrap().clone();
            let original_hook = document.metadata.business_hook.clone();
            let source = document
                .capture_manual_context_maintenance_source()
                .unwrap();
            let owner = document
                .borrow_manual_context_compaction(&source)
                .unwrap()
                .unwrap();
            let api = if terminal != "before_api" {
                Some(
                    document
                        .start_api(
                            ApiPurposeKind::Compaction,
                            ApiRequestSnapshot::new("provider", "model"),
                            10,
                        )
                        .unwrap(),
                )
            } else {
                None
            };
            match terminal {
                "success" => {
                    let api = api.unwrap();
                    let summary = document
                        .append_assistant_text(
                            api,
                            "Task summary",
                            AssistantSemanticStatus::Completed,
                            11,
                        )
                        .unwrap()
                        .into_value();
                    complete_summary(&mut document, api, 12);
                    let input = CompactionCommitInput::new(
                        api,
                        summary,
                        original.first().unwrap().seq,
                        "Task summary",
                        LockedBaselineProjection::empty(),
                    );
                    document
                        .record_borrowed_manual_compaction(owner, input.clone(), 13)
                        .unwrap();
                    assert!(
                        !document
                            .record_borrowed_manual_compaction(owner, input, 14)
                            .unwrap()
                            .changed()
                    );
                    document.complete_borrowed_manual_compaction(owner).unwrap();
                }
                "api_failure" => {
                    assert!(
                        document
                            .fail_manual_compaction_api(api.unwrap(), "fixture", 11)
                            .unwrap()
                    );
                    assert!(
                        !document
                            .fail_manual_compaction_api(api.unwrap(), "fixture", 12)
                            .unwrap()
                    );
                    document
                        .fail_borrowed_manual_compaction(owner, "fixture", 13)
                        .unwrap();
                    assert!(
                        !document
                            .fail_borrowed_manual_compaction(owner, "fixture", 14)
                            .unwrap()
                    );
                }
                "before_api" => {
                    document
                        .fail_borrowed_manual_compaction(owner, "preparation failed", 10)
                        .unwrap();
                }
                "process_loss" => {
                    publish(&mut document);
                    document = reload(&document);
                    assert!(document.project_persisted_interruptions().unwrap() > 0);
                    assert_eq!(document.project_persisted_interruptions().unwrap(), 0);
                }
                _ => unreachable!(),
            }
            for event in &original {
                assert_eq!(
                    document.events().iter().find(|row| row.seq == event.seq),
                    Some(event)
                );
            }
            assert_eq!(document.active_turn().unwrap(), &business);
            assert_eq!(document.metadata.business_hook, original_hook);
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainAndSubagents
            );
            assert!(document.metadata.context_maintenance.is_none());
            assert!(document.active_api().is_none());
            assert!(!document.events().iter().any(|row| matches!(
                &row.event,
                EventBody::TurnBoundary(_) | EventBody::TurnStats(_)
            )));
            publish(&mut document);
            document = reload(&document);
            assert_eq!(
                document.continue_turn_fixture(20).is_ok(),
                allowed,
                "{terminal}"
            );
        }
    }
}

#[test]
fn borrowed_manual_source_rejects_busy_or_changed_history_and_preserves_draft() {
    let mut document = open_business(true);
    let source = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    document.set_draft("/compact", vec![], 6).unwrap();
    publish(&mut document);
    let owner = document
        .borrow_manual_context_compaction(&source)
        .unwrap()
        .unwrap();
    assert_eq!(document.metadata.input_state.draft.text, "/compact");
    let api = document
        .start_api(
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("provider", "model"),
            10,
        )
        .unwrap();
    publish(&mut document);
    let busy = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    assert!(document.borrow_manual_context_compaction(&busy).is_err());
    document
        .fail_manual_compaction_api(api, "fixture", 11)
        .unwrap();
    document
        .enqueue_input_fixture("later work", vec![], 12)
        .unwrap();
    publish(&mut document);
    let queued = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    assert!(document.borrow_manual_context_compaction(&queued).is_err());
    let before = document.clone();
    assert!(
        document
            .fail_manual_compaction_api(ApiCallId::try_new(99999).unwrap(), "unused", 13)
            .is_ok()
    );
    assert_eq!(document, before);
    document.close_turn(TurnOutcome::Stopped, 1, 14).unwrap();
    document
        .submit_root_input_fixture("replacement", vec![], 15)
        .unwrap();
    let replacement = document.clone();
    assert!(
        document
            .fail_borrowed_manual_compaction(owner, "late failure", 16)
            .is_err()
    );
    assert_eq!(document, replacement);
}

#[test]
fn borrowed_manual_compaction_never_interrupts_replacement_api() {
    let mut document = open_business(true);
    let owner = document
        .borrow_manual_context_compaction(
            &document
                .capture_manual_context_maintenance_source()
                .unwrap(),
        )
        .unwrap()
        .unwrap();
    let original = document
        .start_api(
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("provider", "model"),
            10,
        )
        .unwrap();
    complete_summary(&mut document, original, 11);
    let replacement = document
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("provider", "model"),
            12,
        )
        .unwrap();
    let before = document.clone();
    assert!(
        document
            .fail_manual_compaction_api(original, "late failure", 13)
            .is_err()
    );
    assert!(
        document
            .fail_borrowed_manual_compaction(owner, "late failure", 13)
            .is_err()
    );
    assert_eq!(document, before);
    assert_eq!(document.active_api().unwrap().api_call_id, replacement);
}

#[test]
fn borrowed_manual_keeps_accepted_continue_and_in_turn_inputs() {
    for interrupted in [false, true] {
        let mut document = open_business(true);
        let turn_id = document.active_turn().unwrap().turn_id;
        let action = ClientActionId::try_new("continue-before-manual").unwrap();
        document
            .continue_open_turn(ContinuationRequest::new(action.clone(), "", vec![]), 6)
            .unwrap();
        publish(&mut document);
        let owner = document
            .borrow_manual_context_compaction(
                &document
                    .capture_manual_context_maintenance_source()
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
        let api = document
            .start_api(
                ApiPurposeKind::Compaction,
                ApiRequestSnapshot::new("provider", "model"),
                10,
            )
            .unwrap();
        document
            .add_pending_in_turn_input_fixture("Preserve new guidance", vec![], 11)
            .unwrap();
        let pending = document.metadata.input_state.pending_in_turn_inputs.clone();
        if interrupted {
            document
                .fail_manual_compaction_api(api, "fixture failure", 12)
                .unwrap();
            document
                .fail_borrowed_manual_compaction(owner, "fixture failure", 13)
                .unwrap();
        } else {
            complete_summary(&mut document, api, 12);
        }
        for expected in &pending {
            assert!(
                document
                    .events()
                    .iter()
                    .any(|event| event.turn_id == Some(turn_id)
                        && matches!(&event.event, EventBody::UserInput(input)
                    if input.input_id == expected.input_id && input.text == expected.text))
            );
        }
        assert!(!document.events().iter().any(|event|
            matches!(&event.event, EventBody::ApiStarted(started)
                if started.purpose == ApiPurposeKind::Compaction && started.continuation_action_id.is_some())));
        publish(&mut document);
        document = reload(&document);
        let continued = document
            .start_api(
                ApiPurposeKind::Continue,
                ApiRequestSnapshot::new("provider", "model"),
                14,
            )
            .unwrap();
        assert!(document.events().iter().any(|event|
            matches!(&event.event, EventBody::ApiStarted(started)
                if started.api_call_id == continued && started.continuation_action_id.as_ref() == Some(&action))));
    }
}

#[test]
fn borrowed_manual_rejects_unsettled_children_and_stale_summary_publication() {
    let mut document = open_business(true);
    let source = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    let owner = document
        .borrow_manual_context_compaction(&source)
        .unwrap()
        .unwrap();
    let turn_id = document.active_turn().unwrap().turn_id;
    let child = SubagentTaskId::try_new("manual-child").unwrap();
    document
        .reserve_child_task(ReserveChildTaskInput {
            operation_id: CollaborationOperationId::try_new("reserve-manual-child").unwrap(),
            source_tool_name: "spawn_agent".into(),
            execution_id: CollaborationExecutionId::try_new("manual-child-execution").unwrap(),
            task_id: child.clone(),
            parent_turn_id: turn_id,
            child: CollaborationParticipant::new(
                "child",
                "borrowed-root",
                "project",
                1,
                None,
                Some(child),
            )
            .unwrap(),
            reserved_at_ms: 6,
        })
        .unwrap();
    publish(&mut document);
    let occupied = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    assert!(matches!(
        document.borrow_manual_context_compaction(&occupied),
        Err(DomainError::CollaborationStateViolation {
            field: "manual_maintenance.competing_owner"
        })
    ));
    assert!(document.borrow_manual_context_compaction(&source).is_err());
    let api = document
        .start_api(
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("provider", "model"),
            10,
        )
        .unwrap();
    let summary = document
        .append_assistant_text(api, "Summary", AssistantSemanticStatus::Completed, 11)
        .unwrap()
        .into_value();
    complete_summary(&mut document, api, 12);
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("changed-control").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 13,
        })
        .unwrap();
    let before = document.clone();
    assert!(
        document
            .record_borrowed_manual_compaction(
                owner,
                CompactionCommitInput::new(
                    api,
                    summary,
                    source.event_revisions.first().unwrap().0,
                    "Summary",
                    LockedBaselineProjection::empty()
                ),
                14
            )
            .is_err()
    );
    assert_eq!(document, before);
    assert!(document.runtime_lock.current_compaction_id.is_none());
}
