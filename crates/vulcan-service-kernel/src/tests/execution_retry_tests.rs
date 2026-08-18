//! Execution retry claim contract regression tests.
//! 执行重试领取契约回归测试。

use super::*;
use crate::conversation_submission::validate_execution_retry_snapshot_for_claim;
use sha2::{Digest, Sha256};
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction,
    ExecutionRetryPreparationInput, ExecutionRetrySnapshot, ExecutionRetrySnapshotState,
    PendingSessionPromptSource, SessionProviderRouteLeaseIdentity,
};

/// Builds one route identity shared by the retained state and sidecar fixture.
/// 构建一份由保留状态与 sidecar 测试夹具共享的路由身份。
fn retry_route() -> SessionProviderRouteLeaseIdentity {
    SessionProviderRouteLeaseIdentity {
        schema_version: 1,
        provider_id: String::from("provider-a"),
        connection_id: String::from("connection-a"),
        model_id: String::from("model-a"),
        protocol_profile_id: String::from("openai-responses.v1"),
        manifest_revision: 1,
        manifest_generation: 1,
        catalog_revision: String::from("catalog-a"),
        route_revision: String::from("route-a"),
    }
}

/// Builds a retained runtime state whose canonical Core projection is empty and stable.
/// 构建规范 Core 投影为空且稳定的保留运行态。
fn retry_runtime_state() -> vulcan_workbench_model::SessionRuntimeStateEntry {
    let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
        provider_route_lease: Some(retry_route()),
        ..Default::default()
    };
    runtime_state
        .provider_call_ledger
        .active_context_generation_id = Some(String::from("context-a"));
    runtime_state
}

/// Builds one frozen dispatch snapshot bound to the supplied session.
/// 构建一份绑定到指定会话的冻结分派快照。
fn retry_snapshot(session_id: SessionId) -> ExecutionRetrySnapshot {
    let controller_snapshot = vulcan_core::types::ConversationSnapshot::default();
    let canonical_bytes =
        serde_json::to_vec(&controller_snapshot).expect("empty Core snapshot should serialize");
    let canonical_digest = Sha256::digest(&canonical_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let request = serde_json::from_value(serde_json::json!({
        "operation": "conversation_respond",
        "model_id": "model-a"
    }))
    .expect("minimal public request should decode");
    ExecutionRetrySnapshot {
        schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        generation: 1,
        state: ExecutionRetrySnapshotState::Frozen,
        logical_execution_id: String::from("logical-a"),
        logical_turn_id: Some(String::from("turn-a")),
        physical_attempt_id: String::from("attempt-a"),
        claimed_attempt_id: None,
        provider_route: Some(retry_route()),
        provider_service_entry_id: Some(String::from("entry-a")),
        credential_profile_id: Some(String::from("credential-a")),
        controller_config: Some(vulcan_core::types::ControllerConfig::default()),
        controller_snapshot,
        pending_action: ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(request),
        },
        error: None,
        canonical_digest,
        context_generation_id: Some(String::from("context-a")),
        created_at: String::from("1000"),
        updated_at: String::from("1000"),
    }
}

/// Verifies an exact frozen dispatch snapshot passes the claim guard.
/// 验证精确冻结分派快照能够通过领取守卫。
#[test]
fn execution_retry_claim_accepts_exact_frozen_snapshot() {
    let session_id = Uuid::new_v4();
    validate_execution_retry_snapshot_for_claim(
        &retry_runtime_state(),
        session_id,
        &retry_snapshot(session_id),
    )
    .expect("exact frozen retry snapshot should be claimable");
}

/// Verifies terminal canonical changes are adopted when a retry snapshot is refrozen.
/// 验证重试快照重新冻结时会接纳终态规范状态变化。
#[test]
fn execution_retry_refreeze_refreshes_canonical_state_and_digest() {
    let session_id = Uuid::new_v4();
    let mut runtime_state = retry_runtime_state();
    let mut snapshot = retry_snapshot(session_id);
    runtime_state.conversation_state.continuation.updated_at = Some(String::from("2000"));

    let drift_error =
        validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
            .expect_err("terminal canonical state should differ before refreeze refresh");
    assert_eq!(
        drift_error,
        "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED"
    );

    refresh_execution_retry_canonical_state(&mut snapshot, &runtime_state.conversation_state)
        .expect("refreeze should refresh canonical state and digest together");

    validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
        .expect("refreshed retry snapshot should match the retained canonical state");
}

/// Verifies stop-time refreeze binds the caller's post-stop state instead of stale shared state.
/// 验证停止时重冻会绑定调用方的停止后状态，而不是过期共享状态。
#[test]
fn execution_retry_stop_refreeze_uses_post_stop_canonical_state() {
    // sessionId identifies the isolated retained session and retry sidecar.
    // sessionId 标识隔离的保留会话与重试 sidecar。
    let session_id = Uuid::new_v4();
    // controllerRoot owns the exact temporary segmented session directory.
    // controllerRoot 持有精确的临时分段会话目录。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime is the production-shaped persistence handle for the fixture.
    // controllerRuntime 是测试夹具使用的生产形态持久化句柄。
    let controller_runtime = controller_root.controller_runtime_handle();
    // claimedSnapshot models a Continue execution already claimed before the user stops it.
    // claimedSnapshot 模拟用户停止前已被 Continue 领取的执行。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.claimed_attempt_id = Some(String::from("attempt-claimed"));
    // claimedReference is the authoritative shared-state reference before stop projection.
    // claimedReference 是停止投影前共享状态中的权威引用。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed retry sidecar should persist");
    // staleRuntimeState deliberately retains the pre-stop canonical state in shared memory.
    // staleRuntimeState 刻意在共享内存中保留停止前规范状态。
    let mut stale_runtime_state = retry_runtime_state();
    stale_runtime_state.execution_retry_snapshot = Some(claimed_reference);
    // serviceState owns the stale shared snapshot observed before stop projection commits.
    // serviceState 持有停止投影提交前观察到的过期共享快照。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, stale_runtime_state.clone());
    // sharedState matches the production mutex ownership used by refreeze helpers.
    // sharedState 对应重冻辅助逻辑使用的生产互斥所有权。
    let shared_state = Arc::new(Mutex::new(service_state));
    // stoppedConversationState is the detached canonical state after the stop message was appended.
    // stoppedConversationState 是追加停止消息后的分离规范状态。
    let mut stopped_conversation_state = stale_runtime_state.conversation_state.clone();
    stopped_conversation_state.continuation.updated_at = Some(String::from("3000"));

    refreeze_claimed_execution_retry_snapshot_with_canonical_state(
        &shared_state,
        &controller_runtime,
        session_id,
        &stopped_conversation_state,
        "test stop",
    )
    .expect("stop-time refreeze should use the supplied canonical state");

    // refreshedReference points to the sidecar atomically rewritten by stop-time refreeze.
    // refreshedReference 指向停止时重冻原子改写后的 sidecar。
    let refreshed_reference = shared_state
        .lock()
        .expect("shared test state should lock")
        .workspace_snapshot
        .as_ref()
        .expect("test workspace should remain available")
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone())
        .expect("refreshed retry reference should remain available");
    // refreshedSnapshot is the durable sidecar validated against the post-stop state.
    // refreshedSnapshot 是针对停止后状态校验的持久 sidecar。
    let refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &refreshed_reference,
        )
        .expect("refreshed retry sidecar should load");
    // expectedRuntimeState mirrors the workspace that the stop service publishes next.
    // expectedRuntimeState 对应停止服务随后发布的工作区状态。
    let mut expected_runtime_state = stale_runtime_state;
    expected_runtime_state.conversation_state = stopped_conversation_state;
    expected_runtime_state.execution_retry_snapshot = Some(refreshed_reference);

    assert_eq!(
        refreshed_snapshot.state,
        ExecutionRetrySnapshotState::Frozen
    );
    assert!(refreshed_snapshot.claimed_attempt_id.is_none());
    validate_execution_retry_snapshot_for_claim(
        &expected_runtime_state,
        session_id,
        &refreshed_snapshot,
    )
    .expect("refrozen retry snapshot should match the published post-stop state");
}

/// Verifies transcript-only terminal records do not invalidate the frozen provider dispatch.
/// 验证仅供转录使用的终态记录不会使冻结的 Provider 分发失效。
#[test]
fn execution_retry_claim_accepts_transcript_only_record_drift() {
    let session_id = Uuid::new_v4();
    let runtime_state = retry_runtime_state();
    let mut snapshot = retry_snapshot(session_id);
    snapshot
        .controller_snapshot
        .records
        .push(vulcan_core::types::ConversationRecord {
            id: String::from("terminal-error-a"),
            role: vulcan_core::types::TranscriptRole::System,
            record_kind: vulcan_core::types::TranscriptRecordKind::Visible,
            visible: true,
            title: String::from("Execution failed"),
            body: String::from("Provider request failed."),
            metadata: None,
            created_at: String::from("2000"),
        });

    validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
        .expect("transcript-only record drift should preserve the frozen provider dispatch");
}

/// Verifies an accepted submission can be claimed before its first Provider request exists.
/// 验证已接纳提交能够在首个 Provider 请求形成前被领取。
#[test]
fn execution_retry_claim_accepts_frozen_resume_preparation_snapshot() {
    let session_id = Uuid::new_v4();
    let mut snapshot = retry_snapshot(session_id);
    snapshot.provider_service_entry_id = None;
    snapshot.credential_profile_id = None;
    snapshot.controller_config = None;
    snapshot.pending_action = ExecutionRetryPendingAction::ResumePreparation {
        input: Box::new(ExecutionRetryPreparationInput {
            preparation_turn_id: String::from("turn-a"),
            session_title: String::from("session-a"),
            workspace_path: String::from("D:\\workspace"),
            active_provider_id: Some(String::from("provider-a")),
            active_connection_id: Some(String::from("connection-a")),
            active_model_id: Some(String::from("model-a")),
            original_source: PendingSessionPromptSource::UserFollowup,
            prompt: String::from("original prompt"),
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            execution_mode: String::from("agent"),
            reasoning_effort: None,
            boundary_events: Vec::new(),
            canonical_user_message_id: Some(String::from("user-message-a")),
            precheck_payload: serde_json::json!({"turn_id": "turn-a"}),
            profile_payload: serde_json::json!({"turn_id": "turn-a"}),
        }),
    };
    snapshot.provider_route = None;
    let mut runtime_state = retry_runtime_state();
    runtime_state.provider_route_lease = None;
    runtime_state.active_provider_id = Some(String::from("provider-a"));
    runtime_state.active_connection_id = Some(String::from("connection-a"));
    runtime_state.active_model_id = Some(String::from("model-a"));

    validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
        .expect("frozen accepted preparation should be claimable");
}

/// Verifies a changed pre-route model selection invalidates preparation retry.
/// 验证路由形成前的模型选择发生变化会使准备态重试失效。
#[test]
fn execution_retry_claim_rejects_resume_preparation_selection_drift() {
    let session_id = Uuid::new_v4();
    let mut snapshot = retry_snapshot(session_id);
    snapshot.provider_route = None;
    snapshot.provider_service_entry_id = None;
    snapshot.credential_profile_id = None;
    snapshot.controller_config = None;
    snapshot.pending_action = ExecutionRetryPendingAction::ResumePreparation {
        input: Box::new(ExecutionRetryPreparationInput {
            preparation_turn_id: String::from("turn-a"),
            session_title: String::from("session-a"),
            workspace_path: String::from("D:\\workspace"),
            active_provider_id: Some(String::from("provider-a")),
            active_connection_id: Some(String::from("connection-a")),
            active_model_id: Some(String::from("model-a")),
            original_source: PendingSessionPromptSource::UserFollowup,
            prompt: String::from("original prompt"),
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            execution_mode: String::from("agent"),
            reasoning_effort: None,
            boundary_events: Vec::new(),
            canonical_user_message_id: Some(String::from("user-message-a")),
            precheck_payload: serde_json::json!({"turn_id": "turn-a"}),
            profile_payload: serde_json::json!({"turn_id": "turn-a"}),
        }),
    };
    let mut runtime_state = retry_runtime_state();
    runtime_state.provider_route_lease = None;
    runtime_state.active_provider_id = Some(String::from("provider-a"));
    runtime_state.active_connection_id = Some(String::from("connection-a"));
    runtime_state.active_model_id = Some(String::from("changed-model"));

    let error = validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
        .expect_err("changed pre-route selection must not be claimable");

    assert_eq!(error, "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED");
}

/// Verifies a tool-side-effect boundary blocks automatic replay.
/// 验证工具副作用边界会阻止自动重放。
#[test]
fn execution_retry_claim_rejects_non_retryable_tool_boundary() {
    let session_id = Uuid::new_v4();
    let mut snapshot = retry_snapshot(session_id);
    snapshot.error = Some(vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
        subcategory: Some(String::from("tool_side_effect_in_flight")),
        stage: String::from("tool_execution"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: None,
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: Some(String::from("provider-a")),
        entry_id: None,
        model_id: Some(String::from("model-a")),
        message: String::from("unsafe tool boundary"),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: true,
        side_effect_observed: true,
        terminal: false,
        retryable: false,
        retry_after: None,
    });

    let error =
        validate_execution_retry_snapshot_for_claim(&retry_runtime_state(), session_id, &snapshot)
            .expect_err("unsafe tool boundary must not be claimable");

    assert_eq!(error, "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE");
}

/// Verifies route drift fails closed before a retry claim changes state.
/// 验证路由漂移会在重试领取改变状态前失败关闭。
#[test]
fn execution_retry_claim_rejects_route_drift() {
    let session_id = Uuid::new_v4();
    let mut runtime_state = retry_runtime_state();
    runtime_state
        .provider_route_lease
        .as_mut()
        .expect("route should exist")
        .route_revision = String::from("changed-route");

    let error = validate_execution_retry_snapshot_for_claim(
        &runtime_state,
        session_id,
        &retry_snapshot(session_id),
    )
    .expect_err("route drift must not be claimable");

    assert_eq!(error, "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED");
}
