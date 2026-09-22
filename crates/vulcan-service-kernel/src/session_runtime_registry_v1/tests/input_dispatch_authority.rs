//! Canonical user-input dispatch remains available after auxiliary requests.
//! 规范用户输入在辅助请求之后仍保留分发资格。

use super::*;
use crate::webui_prompt_lifecycle::{
    pending_in_turn_dispatch_authority_from_v1_handle, user_root_dispatch_authority_from_v1_handle,
};

/// A fresh queued successor atomically activates business, while a replay preserves a later stop.
/// 新排队后继原子激活业务，而重放保留后来的停止状态。
/// Uses the actual durable writer and exact enqueue receipts; no provider is involved.
/// 使用实际可靠写入器及精确排队回执；不涉及供应商。
#[test]
fn queued_successor_control_is_atomic_and_replay_preserves_stop() {
    // Start from genuine natural idle to expose activation omitted by a running-root fixture.
    // 从真正自然空闲开始，暴露运行中根夹具会掩盖的激活遗漏。
    let (_storage, handle) = recording_handle();
    handle
        .mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| document.create_goal("Goal", None, "Continue", 2),
        )
        .unwrap();
    // Capture the previous frontier to prove that both changes publish in one revision.
    // 捕获此前前沿，证明两项变化在同一修订发布。
    let before = handle.read(|document| document.durable_revision()).unwrap();
    let receipt = handle
        .enqueue_input(test_client_input("queued-control", "New work"), 3)
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(receipt.revision, before + 1);
    assert_eq!(
        handle
            .read(|document| document.collaboration_run_control())
            .unwrap(),
        CollaborationRunControl::Running
    );
    handle
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("later-stop").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 4,
        })
        .unwrap();
    // Exact retries acknowledge the old input without undoing explicit stop authority.
    // 精确重试确认旧输入，但不撤销明确停止的权威。
    let replay = handle
        .enqueue_input(test_client_input("queued-control", "New work"), 5)
        .unwrap();
    assert!(!replay.changed);
    assert_eq!(replay.value, receipt.value);
    assert_eq!(
        handle
            .read(|document| document.collaboration_run_control())
            .unwrap(),
        CollaborationRunControl::Stopped
    );
}

/// An explicit Continue restores original Hook ownership and projects only its exact optional supplement.
/// 显式继续恢复原 Hook 归属，并且仅投影其精确的可选补充输入。
/// Exercises durable admission, cold runtime restoration, receipt rejection and business supersession.
/// 覆盖可靠准入、冷运行时恢复、回执拒绝和业务替代。
#[tokio::test]
async fn explicit_continue_hook_authority_preserves_original_owner() {
    use crate::session_business_hook::{
        capture_business_hook_before_dispatch, continuation_hook_authority,
    };
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::ConversationHookRuntime;
    use vulcan_session_core::v1::document::ContinuationRequest;
    use vulcan_workbench_model::PendingSessionPromptSource;
    for text in ["", "@inject 2 rule\nSupplement"] {
        // Publish the real initial Hook before interrupting its first physical request.
        // 中断首次物理请求前发布真实初始 Hook。
        let (_, handle) = recording_handle();
        let session_id = handle.session_id();
        handle
            .submit_root_input(test_client_input("root", "Original work"), 2)
            .unwrap();
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        let initial = runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .prepare_user_prompt_hook_requests(
                session_id,
                "root",
                None,
                "D:/continue-hook-test".into(),
                PendingSessionPromptSource::UserFollowup,
                vulcan_hooks::SystemPromptHookPhase::FirstSession,
                "Original work",
                &Default::default(),
                false,
            );
        capture_business_hook_before_dispatch(&handle, &runtime, session_id, &initial.turn_id)
            .unwrap();
        start_request(&handle, ApiPurposeKind::MainGeneration, 3);
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| {
                    document
                        .interrupt_active_api_for_process(4)
                        .map(SessionMutationChange::Changed)
                },
            )
            .unwrap();
        let action = ClientActionId::try_new("continue-action").unwrap();
        let receipt = handle
            .continue_turn(ContinuationRequest::new(action.clone(), text, vec![]), 5)
            .unwrap()
            .value;
        let authority = continuation_hook_authority(&handle, &action, &receipt).unwrap();
        assert_eq!(
            authority.state.binding.as_ref().unwrap().hook_turn_id,
            initial.turn_id
        );
        assert_eq!(authority.inputs.len(), usize::from(!text.is_empty()));
        // The fresh runtime receives canonical metadata rather than the most recent prompt as a root.
        // 全新运行时接收规范元数据，不将最近提示词当作根输入。
        let mut cold_hook = ConversationHookRuntime::new();
        cold_hook
            .restore_business_hook_state(&authority.state)
            .unwrap();
        for physical_prompt_id in ["continue-preparation", "continue-retry"] {
            let prepared = cold_hook
                .prepare_canonical_in_turn_hook_requests(
                    session_id,
                    physical_prompt_id,
                    None,
                    "D:/continue-hook-test".into(),
                    PendingSessionPromptSource::ExecutionRetry,
                    vulcan_hooks::SystemPromptHookPhase::SubsequentSession,
                    text,
                    &Default::default(),
                    true,
                    &authority.inputs,
                )
                .unwrap();
            assert_eq!(prepared.turn_id, initial.turn_id);
        }
        let (_, completion) = cold_hook.prepare_turn_completion_dispatch(
            session_id,
            None,
            "done",
            1,
            &Default::default(),
        );
        let report = completion.run().await.report.unwrap();
        let timeline = report.payload.value["timeline"].as_array().unwrap();
        assert_eq!(timeline.len(), usize::from(!text.is_empty()));
        if !text.is_empty() {
            assert_eq!(timeline[0]["source_id"], "continue-action");
            assert_eq!(timeline[0]["kind"], "user_interrupt");
            assert_eq!(timeline[0]["content"], "Supplement");
        }
        // Action, control order and optional input coordinates are independently authoritative.
        // 动作、控制顺序及可选输入坐标分别具有权威性。
        assert!(
            continuation_hook_authority(
                &handle,
                &ClientActionId::try_new("foreign").unwrap(),
                &receipt
            )
            .is_err()
        );
        let mut malformed = receipt.clone();
        malformed.requested_seq = receipt.resumed_seq;
        assert!(continuation_hook_authority(&handle, &action, &malformed).is_err());
        if !text.is_empty() {
            malformed = receipt.clone();
            malformed.input_seq = None;
            assert!(continuation_hook_authority(&handle, &action, &malformed).is_err());
            malformed.input_id = None;
            assert!(continuation_hook_authority(&handle, &action, &malformed).is_err());
        }
        let business = start_request(&handle, ApiPurposeKind::RetryGeneration, 6);
        assert!(continuation_hook_authority(&handle, &action, &receipt).is_err());
        complete_request(&handle, business, 7);
        assert!(continuation_hook_authority(&handle, &action, &receipt).is_err());
    }
}

/// Starts a request of `purpose` using the actual persisted document proof at `now_ms`.
/// 在 `now_ms` 使用真实持久文档凭据启动 `purpose` 请求。
/// Returns its physical identity; fixture failures must fail the test.
/// 返回物理身份；夹具失败必须令测试失败。
fn start_request(handle: &SessionHandle, purpose: ApiPurposeKind, now_ms: i64) -> ApiCallId {
    // Capture the current projection instead of fabricating API events.
    // 捕获当前投影，不伪造 API 事件。
    let input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                purpose,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                now_ms,
            )
        })
        .unwrap();
    handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id
}

/// Completes `api_id` through the real persistence owner at `now_ms` without closing business.
/// 在 `now_ms` 通过真实持久所有者完成 `api_id`，不关闭业务。
/// Returns the exact terminal receipt including any promoted input events.
/// 返回包含提升输入事件的精确终态回执。
fn complete_request(
    handle: &SessionHandle,
    api_id: ApiCallId,
    now_ms: i64,
) -> vulcan_session_core::v1::document::ApiGroupCompletionReceipt {
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document
                    .complete_api_group(
                        api_id,
                        ApiCompletion {
                            request: ApiRequestSnapshot::new("openai", "gpt-5"),
                            usage: UsageSnapshot::default(),
                            status: ApiCallStatus::Completed,
                            end_reason: Some(ApiEndReason::CompletedSuccessfully),
                            error: None,
                            continuation_eligibility: ContinuationEligibility::Forbidden,
                            timing: ApiTimings::default(),
                            provider_source: None,
                            completed_at_ms: now_ms,
                        },
                    )
                    .map(SessionMutationChange::Changed)
            },
        )
        .unwrap()
        .value
}

/// Finished auxiliary work preserves the exact root while active APIs and started business block redispatch.
/// 已结束辅助工作保留精确根输入，活动 API 和已启动业务阻止再次分发。
/// Each purpose uses an independent canonical document and publication owner.
/// 每种用途使用独立规范文档和发布所有者。
#[test]
fn root_input_dispatch_authority_distinguishes_auxiliary_requests() {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // The root boundary is obtained from admission rather than a display message position.
        // 根边界来自准入，不使用显示消息位置。
        let (_, handle) = recording_handle();
        handle
            .submit_root_input(test_client_input("root", "Original work"), 2)
            .unwrap();
        let root_seq = handle
            .read(|document| document.active_turn().unwrap().start_seq)
            .unwrap();
        let original = user_root_dispatch_authority_from_v1_handle(&handle, root_seq).unwrap();
        let auxiliary = start_request(&handle, purpose, 3);
        assert!(user_root_dispatch_authority_from_v1_handle(&handle, root_seq).is_err());
        complete_request(&handle, auxiliary, 4);
        assert_eq!(
            user_root_dispatch_authority_from_v1_handle(&handle, root_seq).unwrap(),
            original
        );
        let business = start_request(&handle, ApiPurposeKind::MainGeneration, 5);
        complete_request(&handle, business, 6);
        assert!(user_root_dispatch_authority_from_v1_handle(&handle, root_seq).is_err());
    }
}

/// Auxiliary execution cannot consume an exact promoted batch or let a business retry run twice.
/// 辅助执行不能消费精确提升批次，也不能让业务重试重复运行。
/// Checks the production authority reader against real API completion and queue-promotion receipts.
/// 使用真实 API 完成及队列提升回执检查生产权威读取器。
#[test]
fn in_turn_input_dispatch_authority_distinguishes_auxiliary_requests() {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // Admit a real queued input and promote it at the original API terminal.
        // 准入真实排队输入，并在原 API 终态提升它。
        let (_, handle) = recording_handle();
        handle
            .submit_root_input(test_client_input("root", "Original work"), 2)
            .unwrap();
        let initial = start_request(&handle, ApiPurposeKind::MainGeneration, 3);
        let queued = handle
            .enqueue_input(test_client_input("steer", "New direction"), 4)
            .unwrap()
            .value;
        handle
            .promote_queued_input_to_pending_turn(
                QueuedInputIdentity {
                    client_action_id: queued.client_action_id,
                    input_id: queued.input_id,
                },
                5,
            )
            .unwrap();
        let terminal = complete_request(&handle, initial, 6);
        assert_eq!(terminal.promoted_input_seqs.len(), 1);
        let original =
            pending_in_turn_dispatch_authority_from_v1_handle(&handle, &terminal).unwrap();
        let auxiliary = start_request(&handle, purpose, 7);
        assert!(pending_in_turn_dispatch_authority_from_v1_handle(&handle, &terminal).is_err());
        complete_request(&handle, auxiliary, 8);
        assert_eq!(
            pending_in_turn_dispatch_authority_from_v1_handle(&handle, &terminal).unwrap(),
            original
        );
        let business = start_request(&handle, ApiPurposeKind::RetryGeneration, 9);
        complete_request(&handle, business, 10);
        assert!(pending_in_turn_dispatch_authority_from_v1_handle(&handle, &terminal).is_err());
    }
}
