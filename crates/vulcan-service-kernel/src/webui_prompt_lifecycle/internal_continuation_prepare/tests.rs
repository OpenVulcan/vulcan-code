//! Real-file first preparation and cold original-Hook recovery for internal continuations.
//! 内部续跑的真实文件首次准备与冷态原 Hook 恢复。

use super::*;
use crate::session_runtime_registry_v1::{SessionFlushReason, SessionMutationAdmission};
use std::sync::Arc;
use vulcan_session_core::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::{
    ApiCompletion, PrepareAndStartApiInput, SubagentReturnBuildRequest, SubagentReturnInput,
};
use vulcan_session_core::v1::event::{
    ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
    AssistantSemanticStatus, ContinuationEligibility, ProviderProjectionProof, TurnOutcome,
};
use vulcan_session_core::v1::ids::SubagentTaskId;
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission, Seq};
use vulcan_workspace_store::workspace_store::{
    create_session_v1_storage_fixture, load_session_v1_full,
};

mod questions;

/// Creates isolated actual storage and its canonical writer; the directory must outlive the handle.
/// 创建隔离实际存储及规范写入器；目录必须比句柄存活更久。
fn stored_handle() -> (tempfile::TempDir, Arc<SessionHandle>) {
    // Each fixture has its own root, so no user or parallel task data participates.
    // 每个夹具拥有独立根目录，不涉及用户或并行任务数据。
    let directory = tempfile::tempdir().unwrap();
    let session_id = SessionId::new_v4();
    let locator =
        create_session_v1_storage_fixture(directory.path(), &session_id.to_string(), 1).unwrap();
    let handle = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_full(&locator).unwrap().document,
    );
    (directory, handle)
}

/// Changes canonical run permission through the real writer without simulating UI state.
/// 通过真实写入器改变规范运行许可，不模拟 UI 状态。
fn set_control(handle: &SessionHandle, state: CollaborationRunControl, now_ms: i64) {
    handle
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "internal-hook-control-{now_ms}"
            ))
            .unwrap(),
            state,
            changed_at_ms: now_ms,
        })
        .unwrap();
}

/// Prepares through the production Hook reducer and returns its actual opaque identity.
/// 通过生产 Hook 归约器准备，并返回其实际不透明身份。
fn prepare_hook(
    runtime: &AgentRuntimeBridge,
    session: SessionId,
    source: PendingSessionPromptSource,
    text: &str,
) -> String {
    // Internal continuation must never produce a deferred user observation.
    // 内部续跑绝不能产生延迟用户观察。
    let prepared = runtime
        .conversation_hook_runtime
        .lock()
        .unwrap()
        .prepare_user_prompt_hook_requests(
            session,
            "internal-hook-test",
            None,
            "D:/internal-hook-test".into(),
            source,
            vulcan_hooks::SystemPromptHookPhase::SubsequentSession,
            text,
            &Default::default(),
            false,
        );
    if matches!(
        source,
        PendingSessionPromptSource::GoalContinuation
            | PendingSessionPromptSource::SubagentReturnContinuation
    ) {
        assert!(prepared.deferred_user_event.is_none());
    }
    prepared.turn_id
}

/// Builds a validated child result from an actual durable API, assistant response and terminal turn.
/// 从实际可靠 API、助手回复及终态回合构建已校验子结果。
fn child_result() -> SubagentReturnInput {
    // All child facts are produced by domain transitions, never fabricated return fields.
    // 全部子事实由领域转换产生，绝不伪造回流字段。
    let (_directory, child) = stored_handle();
    child
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("internal-hook-child").unwrap(),
                "Child task",
                vec![],
            ),
            2,
        )
        .unwrap();
    let request = child
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 3,
            }
        })
        .unwrap();
    let api = child
        .prepare_and_start_api_durable(request)
        .unwrap()
        .value
        .api_call_id;
    let text_seq = child
        .update_assistant_text(
            api,
            "Original child result",
            AssistantSemanticStatus::Completed,
            4,
        )
        .unwrap()
        .value;
    child
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api,
                    ApiCompletion {
                        request: ApiRequestSnapshot::new("openai", "model"),
                        usage: Default::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: 5,
                    },
                )
            },
        )
        .unwrap();
    child
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            |document| document.close_turn(TurnOutcome::Completed, 1, 6),
        )
        .unwrap();
    child
        .read_full_durable(|document| {
            document.build_subagent_return_input(SubagentReturnBuildRequest {
                final_turn_id: document
                    .events()
                    .iter()
                    .find(|event| event.seq == text_seq)
                    .unwrap()
                    .turn_id
                    .unwrap(),
                child_task_id: SubagentTaskId::try_new(child.session_id().to_string()).unwrap(),
                source_assistant_text_seq: text_seq,
            })
        })
        .unwrap()
        .unwrap()
}

/// Goal first preparation replaces only an unpublished seed; cold preparation reuses a published seed.
/// Goal 首次准备仅替换未发布种子；冷态准备复用已发布种子。
#[test]
fn goal_hook_preparation_preserves_published_identity_and_rejects_stale_source() {
    for published in [false, true] {
        // The canonical Goal starts independently of any human root input.
        // 规范 Goal 独立于任何人类根输入启动。
        let (_directory, handle) = stored_handle();
        handle
            .mutate_durable(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| document.create_goal("Goal", None, "Goal context", 2),
            )
            .unwrap();
        set_control(&handle, CollaborationRunControl::Running, 3);
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        let original_id = prepare_hook(
            &runtime,
            handle.session_id(),
            PendingSessionPromptSource::GoalContinuation,
            "Original goal preparation",
        );
        if published {
            crate::session_business_hook::capture_business_hook_before_dispatch(
                &handle,
                &runtime,
                handle.session_id(),
                &original_id,
            )
            .unwrap();
        }
        // Reload from disk and discard process-local ownership only in the published recovery case.
        // 已发布恢复场景从磁盘重载并丢弃进程内归属。
        let locator = handle.storage_locator().unwrap();
        let restored = SessionHandle::new(
            handle.session_id(),
            locator.clone(),
            load_session_v1_full(&locator).unwrap().document,
        );
        let runtime = if published {
            crate::tests::test_support::test_agent_runtime_bridge()
        } else {
            runtime
        };
        let authority =
            crate::session_goal_lifecycle::session_goal_dispatch_authority_from_v1_handle(
                &restored,
                restored.session_id(),
            )
            .unwrap()
            .unwrap();
        let mut stale = authority.clone();
        stale.generation += 1;
        assert!(
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::Goal(&stale)
            )
            .is_err()
        );
        // Source validation occurs before mutation: a rejected source cannot erase an unpublished owner.
        // 来源校验先于修改：被拒来源不能清除未发布所有者。
        assert_eq!(
            runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .has_active_turn(restored.session_id()),
            !published
        );
        let revision = restored.read(|doc| doc.durable_revision()).unwrap();
        prepare_internal_continuation_hook_owner(
            &restored,
            &runtime,
            InternalContinuationHookSource::Goal(&authority),
        )
        .unwrap();
        let resumed_id = prepare_hook(
            &runtime,
            restored.session_id(),
            PendingSessionPromptSource::GoalContinuation,
            "Internal continuation",
        );
        assert_eq!(resumed_id == original_id, published);
        crate::session_business_hook::capture_business_hook_before_dispatch(
            &restored,
            &runtime,
            restored.session_id(),
            &resumed_id,
        )
        .unwrap();
        assert_eq!(
            restored.read(|doc| doc.durable_revision()).unwrap(),
            revision + u64::from(!published)
        );
        set_control(&restored, CollaborationRunControl::Stopped, 10);
        assert!(
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::Goal(&authority)
            )
            .is_err()
        );
    }
}

/// Child returns recover the parent's original user Hook and cannot forge a missing parent identity.
/// 子回流恢复父级原用户 Hook，不能伪造缺失的父身份。
#[test]
fn child_return_hook_preparation_restores_parent_and_rejects_missing_owner() {
    for published in [false, true] {
        // The parent and validated child source use real independent file stores.
        // 父级与已校验子来源使用真实独立文件存储。
        let (_directory, parent) = stored_handle();
        parent
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("internal-hook-parent").unwrap(),
                    "Original parent task",
                    vec![],
                ),
                2,
            )
            .unwrap();
        let original_runtime = crate::tests::test_support::test_agent_runtime_bridge();
        let original_id = prepare_hook(
            &original_runtime,
            parent.session_id(),
            PendingSessionPromptSource::UserFollowup,
            "Original parent task",
        );
        if published {
            crate::session_business_hook::capture_business_hook_before_dispatch(
                &parent,
                &original_runtime,
                parent.session_id(),
                &original_id,
            )
            .unwrap();
        }
        let receipt = parent
            .append_subagent_return(child_result(), 8)
            .unwrap()
            .value;
        let locator = parent.storage_locator().unwrap();
        let restored = SessionHandle::new(
            parent.session_id(),
            locator.clone(),
            load_session_v1_full(&locator).unwrap().document,
        );
        let authority =
            super::super::pending_prepare::subagent_return_dispatch_authority_from_v1_handle(
                &restored, &receipt,
            )
            .unwrap();
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        let mut altered = authority.clone();
        altered.content = "Different child result".into();
        assert!(
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::SubagentReturn(&altered)
            )
            .is_err()
        );
        altered = authority.clone();
        altered.generation += 1;
        assert!(
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::SubagentReturn(&altered)
            )
            .is_err()
        );
        let before = restored
            .read(|doc| {
                serde_json::to_value((&doc.metadata, doc.events(), doc.memory_revision())).unwrap()
            })
            .unwrap();
        let recovery = prepare_internal_continuation_hook_owner(
            &restored,
            &runtime,
            InternalContinuationHookSource::SubagentReturn(&authority),
        );
        if published {
            recovery.unwrap();
            assert_eq!(
                prepare_hook(
                    &runtime,
                    restored.session_id(),
                    PendingSessionPromptSource::SubagentReturnContinuation,
                    "Internal return instruction"
                ),
                original_id
            );
            crate::session_business_hook::capture_business_hook_before_dispatch(
                &restored,
                &runtime,
                restored.session_id(),
                &original_id,
            )
            .unwrap();
        } else {
            assert!(
                recovery
                    .unwrap_err()
                    .contains("not the original business source")
            );
            assert!(
                !runtime
                    .conversation_hook_runtime
                    .lock()
                    .unwrap()
                    .has_active_turn(restored.session_id())
            );
        }
        assert_eq!(
            restored
                .read(|doc| serde_json::to_value((
                    &doc.metadata,
                    doc.events(),
                    doc.memory_revision()
                ))
                .unwrap())
                .unwrap(),
            before
        );
    }
}

/// A return-owned canonical turn can initialize only from its exact start and existing run consent.
/// 回流所属规范回合仅能依据精确起点及既有运行许可初始化。
#[test]
fn child_return_initial_hook_requires_running_source_turn() {
    // This exercises the domain's explicit SubagentReturn origin, not automatic wakeup policy.
    // 此处验证领域的显式 SubagentReturn 来源，不验证自动唤醒策略。
    let (_directory, parent) = stored_handle();
    let receipt = parent
        .append_subagent_return(child_result(), 8)
        .unwrap()
        .value;
    let authority =
        super::super::pending_prepare::subagent_return_dispatch_authority_from_v1_handle(
            &parent, &receipt,
        )
        .unwrap();
    let runtime = crate::tests::test_support::test_agent_runtime_bridge();
    let stale_id = prepare_hook(
        &runtime,
        parent.session_id(),
        PendingSessionPromptSource::UserFollowup,
        "Unpublished old preparation",
    );
    assert!(
        prepare_internal_continuation_hook_owner(
            &parent,
            &runtime,
            InternalContinuationHookSource::SubagentReturn(&authority)
        )
        .is_err()
    );
    assert_eq!(
        runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .active_turn_id(parent.session_id()),
        Some(stale_id.as_str())
    );
    set_control(&parent, CollaborationRunControl::Running, 9);
    prepare_internal_continuation_hook_owner(
        &parent,
        &runtime,
        InternalContinuationHookSource::SubagentReturn(&authority),
    )
    .unwrap();
    let prepared_id = prepare_hook(
        &runtime,
        parent.session_id(),
        PendingSessionPromptSource::SubagentReturnContinuation,
        "Internal return instruction",
    );
    assert_ne!(prepared_id, stale_id);
    crate::session_business_hook::capture_business_hook_before_dispatch(
        &parent,
        &runtime,
        parent.session_id(),
        &prepared_id,
    )
    .unwrap();
    parent
        .read(|document| {
            let binding = document.metadata.business_hook.binding.as_ref().unwrap();
            assert_eq!(binding.start_seq, receipt.return_seq);
            assert_eq!(binding.turn_id, receipt.parent_turn_id);
            assert!(!document.events().iter().any(|event| matches!(
                event.event,
                vulcan_session_core::v1::EventBody::UserInput(_)
            )));
        })
        .unwrap();
}
