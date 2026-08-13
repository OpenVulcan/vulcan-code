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
        provider_route: retry_route(),
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

    validate_execution_retry_snapshot_for_claim(&retry_runtime_state(), session_id, &snapshot)
        .expect("frozen accepted preparation should be claimable");
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
