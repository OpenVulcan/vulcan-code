//! Execution retry claim contract regression tests.
//! 执行重试领取契约回归测试。

use super::*;
use crate::conversation_continuation::{
    SessionContinuationRuntimeBlockers, conversation_continuation_summary,
};
use crate::conversation_submission::{
    commit_continuation_claim_checkpoint, commit_continuation_preparing_checkpoint,
    execution_retry_claim_failure_blocked_reason,
    invalidate_execution_retry_reference_after_claim_failure,
    prepare_retained_submission_preflight, validate_execution_retry_snapshot_for_claim,
};
use crate::reconcile_workbench_conversation_lifecycle_projection;
use sha2::{Digest, Sha256};
use vulcan_context::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
};
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction,
    ExecutionRetryPreparationInput, ExecutionRetrySnapshot, ExecutionRetrySnapshotRef,
    ExecutionRetrySnapshotState, ExecutionRetryUserFollowup, PendingSessionPromptSource,
    SessionMessageKind, SessionMessageRecordKind, SessionProviderRouteLeaseIdentity,
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
    let canonical_bytes = serde_json::to_vec(&(
        &controller_snapshot.messages,
        &controller_snapshot.continuation,
    ))
    .expect("empty Core snapshot should serialize");
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
        next_provider_attempt_index: 0,
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
        user_followup: None,
        error: None,
        canonical_digest,
        context_generation_id: Some(String::from("context-a")),
        created_at: String::from("1000"),
        updated_at: String::from("1000"),
    }
}

/// Reads the exact retry reference currently retained by one shared test session.
/// 读取一个共享测试会话当前保留的精确重试引用。
///
/// # Parameters
/// 参数
/// `shared_state` owns the production-shaped retained workspace fixture.
/// `shared_state` 持有生产形态的保留工作区测试夹具。
/// `session_id` selects the exact fixture conversation.
/// `session_id` 选择精确的测试会话。
///
/// # Returns
/// 返回值
/// The full immutable retry reference currently owning the session.
/// 返回当前拥有该会话的完整不可变重试引用。
fn retained_execution_retry_reference(
    shared_state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> ExecutionRetrySnapshotRef {
    shared_state
        .lock()
        .expect("shared test state should lock")
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone())
        .expect("retained execution retry reference should exist")
}

/// Returns the complete process-local retry body retained by one test session.
/// 返回一个测试会话保留的完整进程内重试正文。
///
/// # Parameters
/// 参数
/// `shared_state` owns the live workspace and `session_id` selects its exact runtime row.
/// `shared_state` 持有实时工作区，`session_id` 选择其精确运行态记录。
///
/// # Returns
/// 返回值
/// A clone of the transient body used by normal running transitions.
/// 返回正常运行迁移使用的临时正文副本。
fn retained_transient_execution_retry_snapshot(
    shared_state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> ExecutionRetrySnapshot {
    shared_state
        .lock()
        .expect("shared test state should lock")
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .and_then(|runtime_state| {
            runtime_state
                .transient_execution_retry_snapshot
                .as_deref()
                .cloned()
        })
        .expect("retained transient execution retry snapshot should exist")
}

/// Builds one Provider ledger identity that exactly matches the frozen retry route.
/// 构建一份与冻结重试路由精确匹配的 Provider 账本身份。
///
/// # Returns
/// 返回值
/// A main-agent conversation identity suitable for exact stop/error correlation tests.
/// 返回适用于精确停止/错误关联测试的主智能体会话身份。
fn retry_provider_call_identity() -> vulcan_workbench_model::SessionProviderCallIdentity {
    vulcan_workbench_model::SessionProviderCallIdentity {
        logical_request_id: String::from("logical-a"),
        execution_id: String::from("logical-a"),
        turn_id: Some(String::from("turn-a")),
        attempt_index: 0,
        retry_number: 0,
        owner_kind: String::from("main_agent"),
        owner_id: String::from("agent-a"),
        owner_name_snapshot: String::from("Agent A"),
        operation: String::from("conversation"),
        provider_id: String::from("provider-a"),
        provider_name_snapshot: String::from("Provider A"),
        model_id: String::from("model-a"),
        model_name_snapshot: String::from("Model A"),
        connection_id: String::from("connection-a"),
        service_entry_id: String::from("entry-a"),
        protocol_profile_id: String::from("openai-responses.v1"),
        manifest_revision: 1,
        manifest_generation: 1,
        binding_source: String::from("system_manifest"),
        binding_rule: Some(String::from("rule-a")),
        route_revision: String::from("route-a"),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
    }
}

/// Verifies startup recovery consumes an already committed supplement and releases its exact orphaned Continue claim.
/// 验证启动恢复会消费已经提交的补充消息，并释放其精确的孤立 Continue 领取。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This asynchronous test returns no value and fails if the supplement can be appended twice or the persisted claim remains locked.
/// 此异步测试无返回值；若补充消息可能被重复追加，或持久化领取仍被锁定，则测试失败。
#[tokio::test]
async fn startup_recovery_consumes_committed_followup_and_releases_claim() {
    // sessionId identifies the isolated segmented conversation and its detached startup projection.
    // sessionId 标识隔离的分段会话及其分离启动投影。
    let session_id = SessionId::new_v4();
    // controllerRoot owns both immutable retry sidecars and the durable runtime-state revision.
    // controllerRoot 同时持有不可变重试 sidecar 与可靠运行态修订。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime exposes the production startup-recovery persistence path without external I/O.
    // controllerRuntime 在不进行外部 I/O 的情况下暴露生产启动恢复持久化路径。
    let controller_runtime = controller_root.controller_runtime_handle();
    // clientSubmissionId is the exact cross-document owner shared by the claim and transcript admission.
    // clientSubmissionId 是领取与转录准入共享的精确跨文档所有者。
    let client_submission_id = String::from("continue-followup-crash-1");
    // visiblePrompt remains user-visible while providerPrompt includes the hidden recovery guidance.
    // visiblePrompt 保持用户可见，而 providerPrompt 包含隐藏的恢复引导。
    let visible_prompt = String::from("改用新的模型继续");
    let provider_prompt = String::from(
        "改用新的模型继续\n\n[Recovery guidance: continue the interrupted task after reconciling prior tool outcomes.]",
    );
    // claimedSnapshot models a crash after Core committed the supplement but before the next Provider request checkpoint.
    // claimedSnapshot 模拟 Core 已提交补充消息、但下一次 Provider 请求检查点尚未建立时发生崩溃。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.claimed_attempt_id = Some(client_submission_id.clone());
    claimed_snapshot.user_followup = Some(ExecutionRetryUserFollowup {
        visible_prompt: visible_prompt.clone(),
        provider_prompt: provider_prompt.clone(),
        client_submission_id: client_submission_id.clone(),
    });
    // claimedReference binds the retained runtime row to the exact claimed body.
    // claimedReference 将保留运行态记录绑定到精确的已领取正文。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::persist_interrupted_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed retry sidecar should persist");
    // runtimeState contains the canonical supplement already committed immediately before the crash.
    // runtimeState 包含崩溃前一刻已经提交的规范补充消息。
    let mut runtime_state = retry_runtime_state();
    runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("committed-followup-message-1"),
        payload: SessionConversationPayload::UserText {
            text: provider_prompt,
            attachments: Vec::new(),
        },
    }];
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("committed-followup-record-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: visible_prompt.clone(),
            metadata: Some(serde_json::json!({
                "canonical_text": visible_prompt,
                "canonical_user_text": visible_prompt,
                "source": "user_interrupt",
                "prompt_admission_id": client_submission_id,
            })),
            created_at: String::from("1001"),
        });
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("stopped"),
                1000,
                String::from("user"),
                true,
            )
    );
    // claimBoundary is the exact terminal generation and pre-claim ledger revision retained by the UI token.
    // claimBoundary 是界面令牌保留的精确终态代际与领取前账本修订。
    let claim_boundary = runtime_state.provider_call_ledger.continuation.clone();
    runtime_state
        .provider_call_ledger
        .claim_continuation(
            claim_boundary.generation,
            claim_boundary.expected_ledger_revision,
            client_submission_id.clone(),
            1001,
        )
        .expect("fixture continuation should claim");
    // Fresh-process sanitization closes the dead process owner under a new non-eligible problem generation before sidecar hydration.
    // 新进程净化会在 sidecar 水合前，以新的暂不可用问题代际闭合失效的进程所有者。
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(client_submission_id.clone()),
                String::from("continuation_interrupted_after_restart"),
                1002,
                String::from("user"),
                false,
            )
    );
    runtime_state.execution_retry_snapshot = Some(claimed_reference);
    // detachedWorkspace models the fully loaded startup candidate before it is republished to the live service.
    // detachedWorkspace 模拟完整加载、但尚未重新发布到实时服务的启动候选。
    let mut service_state = test_service_state_with_parent_session(session_id);
    let mut detached_workspace = service_state
        .workspace_snapshot
        .clone()
        .expect("test workspace should exist");
    detached_workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState intentionally has no live admission, stream, or kernel owner after the simulated restart.
    // sharedState 在模拟重启后故意不保留任何实时准入、流或 kernel 所有者。
    service_state.active_stream_tasks.remove(&session_id);
    let shared_state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    hydrate_selected_session_runtime_if_needed(
        &shared_state,
        controller_runtime.clone(),
        &mut detached_workspace,
        session_id,
    )
    .await
    .expect("startup recovery should refreeze the committed supplement");

    // recoveredRuntime proves both the sidecar reference and ledger claim were advanced as one safe recovery result.
    // recoveredRuntime 证明 sidecar 引用与账本领取已经作为一个安全恢复结果共同推进。
    let recovered_runtime = detached_workspace
        .session_runtime_state
        .get(&session_id)
        .expect("recovered runtime should remain available");
    let recovered_reference = recovered_runtime
        .execution_retry_snapshot
        .as_ref()
        .expect("recovered retry reference should remain available");
    assert_eq!(
        recovered_reference.state,
        ExecutionRetrySnapshotState::Frozen
    );
    assert!(
        recovered_reference.replay_safety_proven(),
        "recovered reference did not retain a complete replay proof: {recovered_reference:#?}"
    );
    // recoveredTailKind must still match the fresh-process terminal label after canonical refresh.
    // recoveredTailKind 在规范刷新后仍必须匹配新进程终态标签。
    let recovered_tail_kind = canonical_conversation_tail_kind(recovered_runtime);
    assert_eq!(recovered_tail_kind, Some("user"));
    assert_eq!(
        recovered_runtime
            .provider_call_ledger
            .continuation
            .canonical_tail_kind
            .as_deref(),
        recovered_tail_kind
    );
    assert!(
        recovered_runtime
            .provider_call_ledger
            .continuation
            .claimed_at_ms
            .is_none()
    );
    assert!(
        recovered_runtime
            .provider_call_ledger
            .continuation
            .claimed_execution_id
            .is_none()
    );
    assert_eq!(
        recovered_runtime
            .provider_call_ledger
            .continuation
            .expected_ledger_revision,
        recovered_runtime.provider_call_ledger.revision
    );
    assert_eq!(
        recovered_runtime
            .provider_call_ledger
            .in_flight_call_count(),
        0
    );
    assert!(recovered_runtime.provider_call_ledger.continuation.eligible);
    // recoveredSnapshot must consume the durable supplement and rebuild only the next Provider request.
    // recoveredSnapshot 必须消费可靠补充消息，并且只重建下一次 Provider 请求。
    let recovered_snapshot =
        vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            recovered_reference,
        )
        .expect("recovered retry sidecar should remain readable");
    assert!(recovered_snapshot.user_followup.is_none());
    assert!(matches!(
        recovered_snapshot.pending_action,
        ExecutionRetryPendingAction::PrepareNextProviderRequest
    ));
    assert!(recovered_snapshot.error.is_none());
}

/// Verifies final workbench reconciliation disables stale Continue eligibility for an exact live stream owner.
/// 验证最终工作台协调会针对精确实时流所有者禁用过期 Continue 资格。
#[tokio::test]
async fn workbench_reconciliation_blocks_stale_continue_for_live_stream_owner() {
    // session_id identifies the terminal conversation whose stale UI projection is under test.
    // session_id 标识被测过期界面投影对应的终态会话。
    let session_id = SessionId::new_v4();
    // service_state owns the exact active stream fact omitted by the initial pure workbench projection.
    // service_state 持有初始纯工作台投影遗漏的精确活动流事实。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // runtime_state materializes one non-empty failed continuation boundary for reprojection.
    // runtime_state 物化一个非空失败 Continue 边界以供重新投影。
    let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();
    runtime_state.provider_call_ledger.continuation.generation = 7;
    runtime_state
        .provider_call_ledger
        .continuation
        .terminal_reason = Some(String::from("failed"));
    runtime_state
        .provider_call_ledger
        .continuation
        .terminal_at_ms = Some(1000);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_runtime_state
        .insert(session_id, runtime_state);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Failed);
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(Uuid::new_v4(), spawn_service_task(async {}), None),
    );
    // workspace is the exact snapshot committed together with the reconciled workbench.
    // workspace 是与协调后工作台一同提交的精确快照。
    let workspace = service_state
        .workspace_snapshot
        .clone()
        .expect("workspace snapshot");
    // workbench intentionally carries the stale eligible Continue state observed before final locking.
    // workbench 有意携带最终加锁前观测到的过期可用 Continue 状态。
    let mut workbench = fallback_workbench_snapshot(&[]);
    workbench.conversations.push(ConversationSummary {
        id: session_id.to_string(),
        title: String::from("Continue refresh race"),
        status: SessionState::Completed.label().to_string(),
        run_phase: SessionRunPhase::Ready.to_string(),
        last_updated_label: String::from("刚刚"),
        last_updated_at: String::from("1000"),
        unread: false,
        attention: None,
        project_id: Some(String::from("project-a")),
        fork_source: None,
        continuation: Some(vulcan_service_api::ConversationContinuationSummary {
            eligible: true,
            generation: 7,
            expected_ledger_revision: 0,
            reason: Some(String::from("failed")),
            blocked_reason: None,
        }),
    });

    reconcile_workbench_conversation_lifecycle_projection(
        &mut workbench,
        &service_state,
        &workspace,
    )
    .expect("live lifecycle reconciliation should succeed");

    // continuation proves the stale button is replaced by explicit active-session feedback.
    // continuation 证明过期按钮已被显式活动会话反馈取代。
    let continuation = workbench.conversations[0]
        .continuation
        .as_ref()
        .expect("failed terminal boundary should remain visible");
    assert!(!continuation.eligible);
    assert_eq!(
        continuation.blocked_reason.as_deref(),
        Some("ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE")
    );
}

/// Verifies a failed problem turn keeps a disabled Continue control when its runtime row is missing.
/// 验证失败问题回合在运行态记录缺失时仍保留禁用的 Continue 控件。
#[test]
fn workbench_reconciliation_keeps_problem_turn_visible_without_runtime_state() {
    // sessionId identifies the damaged problem turn whose runtime row did not survive.
    // sessionId 标识运行态记录未保留下来的受损问题回合。
    let session_id = SessionId::new_v4();
    // serviceState carries the authoritative failed phase while deliberately omitting session runtime state.
    // serviceState 携带权威失败阶段，同时刻意省略会话运行态记录。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Failed);
    // workspace is the exact retained revision reconciled into the WebUI snapshot.
    // workspace 是协调进 WebUI 快照的精确保留修订。
    let workspace = service_state
        .workspace_snapshot
        .clone()
        .expect("workspace snapshot");
    assert!(!workspace.session_runtime_state.contains_key(&session_id));
    // workbench begins without any Continue projection to prove reconciliation creates the disabled problem-turn state.
    // workbench 初始不含任何 Continue 投影，以证明协调过程会创建问题回合禁用状态。
    let mut workbench = fallback_workbench_snapshot(&[]);
    workbench.conversations.push(ConversationSummary {
        id: session_id.to_string(),
        title: String::from("Missing recovery runtime"),
        status: SessionState::Completed.label().to_string(),
        run_phase: SessionRunPhase::Failed.to_string(),
        last_updated_label: String::from("刚刚"),
        last_updated_at: String::from("1000"),
        unread: false,
        attention: None,
        project_id: Some(String::from("project-a")),
        fork_source: None,
        continuation: None,
    });

    reconcile_workbench_conversation_lifecycle_projection(
        &mut workbench,
        &service_state,
        &workspace,
    )
    .expect("live lifecycle reconciliation should succeed");

    // continuation proves the problem turn remains visible but cannot be claimed without authoritative runtime state.
    // continuation 证明问题回合仍然可见，但缺少权威运行态时不能领取。
    let continuation = workbench.conversations[0]
        .continuation
        .as_ref()
        .expect("failed problem turn should retain a disabled Continue projection");
    assert!(!continuation.eligible);
    assert_eq!(continuation.generation, 0);
    assert_eq!(continuation.expected_ledger_revision, 0);
    assert_eq!(
        continuation.blocked_reason.as_deref(),
        Some("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING")
    );
}

/// Verifies a normal completed turn never projects Continue even when stale abnormal recovery facts remain.
/// 验证正常完成回合即使残留过期异常恢复事实，也绝不投影 Continue。
#[test]
fn continuation_summary_omits_normal_completed_turn() {
    // runtimeState deliberately retains a stale failed boundary to prove the current Ready phase is authoritative.
    // runtimeState 刻意保留过期失败边界，以证明当前 Ready 阶段具有权威性。
    let mut runtime_state = retry_runtime_state();
    runtime_state.provider_call_ledger.continuation.generation = 9;
    runtime_state
        .provider_call_ledger
        .continuation
        .terminal_reason = Some(String::from("failed"));
    runtime_state
        .provider_call_ledger
        .continuation
        .terminal_at_ms = Some(1000);

    assert!(
        conversation_continuation_summary(
            &runtime_state,
            SessionRunPhase::Ready,
            SessionContinuationRuntimeBlockers::default(),
        )
        .is_none(),
        "normal completed turns must not expose a Continue control"
    );
}

/// Builds one lightweight frozen reference for claim-failure invalidation tests.
/// 为领取失败失效测试构建一条轻量冻结引用。
fn retry_reference() -> ExecutionRetrySnapshotRef {
    ExecutionRetrySnapshotRef {
        schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: Uuid::new_v4().to_string(),
        generation: 1,
        state: ExecutionRetrySnapshotState::Frozen,
        content_sha256: "0".repeat(64),
        content_bytes: 128,
        logical_execution_id: String::from("logical-a"),
        pending_action_label: String::from("dispatch_provider_request"),
        provider_route: Some(retry_route()),
        preparation_route_selection: None,
        context_generation_id: None,
        canonical_dispatch_sha256: "0".repeat(64),
        retryable: true,
        requires_manual_confirmation: false,
        blocked_reason: None,
        claimed_attempt_id: None,
        error_summary: None,
        updated_at: String::from("1000"),
    }
}

/// Builds the exact lightweight reference produced for one retry snapshot fixture.
/// 为一份重试快照夹具构建精确轻量引用。
///
/// # Parameters
/// 参数
/// `snapshot` is the complete frozen or claimed body whose identity must be mirrored.
/// `snapshot` 是需要镜像身份的完整冻结或已领取正文。
///
/// # Returns
/// 返回值
/// A content-addressed reference whose lifecycle and canonical dispatch fields match the body.
/// 返回生命周期与规范分派字段均匹配正文的内容寻址引用。
fn retry_reference_for_snapshot(snapshot: &ExecutionRetrySnapshot) -> ExecutionRetrySnapshotRef {
    // encoded is the exact byte body used by content length and digest fields.
    // encoded 是内容长度与摘要字段使用的精确字节正文。
    let encoded = serde_json::to_vec(snapshot).expect("retry snapshot should serialize");
    ExecutionRetrySnapshotRef {
        schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: snapshot.snapshot_id.clone(),
        generation: snapshot.generation,
        state: snapshot.state,
        content_sha256: Sha256::digest(&encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        content_bytes: u64::try_from(encoded.len()).expect("fixture length should fit u64"),
        logical_execution_id: snapshot.logical_execution_id.clone(),
        pending_action_label: String::from("dispatch_provider_request"),
        provider_route: snapshot.provider_route.clone(),
        preparation_route_selection: None,
        context_generation_id: snapshot.context_generation_id.clone(),
        canonical_dispatch_sha256: snapshot.canonical_digest.clone(),
        retryable: true,
        requires_manual_confirmation: false,
        blocked_reason: None,
        claimed_attempt_id: snapshot.claimed_attempt_id.clone(),
        error_summary: None,
        updated_at: snapshot.updated_at.clone(),
    }
}

/// Verifies Continue checkpoint commits preserve unrelated sessions, concurrent same-session settings, and the live kernel mirror.
/// 验证 Continue checkpoint 提交会保留无关会话、并发同会话设置与实时 kernel 镜像。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if either scoped commit replaces a detached workspace or target runtime row.
/// 此测试无返回值；任一范围化提交替换分离的工作区或目标运行态整行时失败。
#[test]
fn continuation_checkpoint_commits_preserve_unrelated_state_and_live_kernel() {
    // sessionId is the stopped conversation whose exact retry is claimed.
    // sessionId 是领取精确重试的已停止会话。
    let session_id = SessionId::new_v4();
    // unrelatedSessionId identifies a concurrent workspace mutation absent from the detached candidate.
    // unrelatedSessionId 标识分离候选中不存在的一次并发工作区变更。
    let unrelated_session_id = SessionId::new_v4();
    // frozenRuntimeState owns the canonical context and eligible terminal continuation row.
    // frozenRuntimeState 持有规范上下文与可领取终态 Continue 记录。
    let mut frozen_runtime_state = retry_runtime_state();
    // terminalError is the exact canonical tail required by failed-boundary eligibility.
    // terminalError 是失败边界资格要求的精确规范尾记录。
    let terminal_error = vulcan_workbench_model::SessionMessageEntry::new(
        vulcan_workbench_model::SessionMessageKind::Error,
        "Provider error",
        "failed",
    )
    .with_runtime_metadata("error-a", "1000");
    frozen_runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: terminal_error.id,
            kind: terminal_error.kind,
            record_kind: terminal_error.record_kind,
            visible: terminal_error.visible,
            title: terminal_error.title,
            body: terminal_error.body,
            metadata: terminal_error.metadata,
            created_at: terminal_error.created_at,
        });
    assert!(
        frozen_runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("preparation_failed"),
                1000,
                String::from("error"),
                true,
            )
    );
    // frozenSnapshot is rebuilt from the same canonical state visible at admission.
    // frozenSnapshot 从准入时可见的同一规范状态重建。
    let mut frozen_snapshot = retry_snapshot(session_id);
    frozen_snapshot.controller_snapshot = vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
        &frozen_runtime_state.conversation_state,
    )
    .expect("terminal canonical state should project");
    let frozen_canonical_bytes = serde_json::to_vec(&(
        &frozen_snapshot.controller_snapshot.messages,
        &frozen_snapshot.controller_snapshot.continuation,
    ))
    .expect("terminal canonical state should serialize");
    frozen_snapshot.canonical_digest = Sha256::digest(&frozen_canonical_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    // frozenReference is the immutable lightweight token paired with the retained terminal ledger.
    // frozenReference 是与保留终态账本配对的不可变轻量令牌。
    let frozen_reference = retry_reference_for_snapshot(&frozen_snapshot);
    frozen_runtime_state.execution_retry_snapshot = Some(frozen_reference.clone());
    // continuation captures the exact generation and revision offered to the client.
    // continuation 捕获提供给客户端的精确代际与修订。
    let continuation = frozen_runtime_state
        .provider_call_ledger
        .continuation
        .clone();
    // serviceState begins with only the target so the unrelated row can race after preflight cloning.
    // serviceState 初始只含目标会话，使无关记录可以在预检克隆后并发写入。
    let mut service_state = test_service_state_with_parent_session(session_id);
    let retained_workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist");
    retained_workspace
        .task_projects
        .push(TaskProjectSummary::new(
            "project-a",
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        ));
    retained_workspace
        .session_runtime_state
        .insert(session_id, frozen_runtime_state);
    retained_workspace
        .session_run_phases
        .insert(session_id, SessionRunPhase::Failed);
    // retainedMessage proves Continue merges only its supplemental transcript delta.
    // retainedMessage 证明 Continue 只合并自身的补充转录增量。
    let mut retained_message = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "历史输出",
        "retained before continuation",
    );
    retained_message.id = String::from("retained-before-continuation");
    retained_workspace
        .session_messages
        .insert(session_id, vec![retained_message]);
    // state exposes admission, workspace, and kernel ownership under the production lock.
    // state 在生产锁下暴露准入、工作区与 kernel 所有权。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // preflight owns the exact reservation and detached candidate used by both commits.
    // preflight 持有两次提交使用的精确预留与分离候选。
    let preflight = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .expect("Continue preflight should reserve the target");
    // candidate is an exact detached runtime slice and is intentionally stale with respect to the later unrelated mutation.
    // candidate 是精确分离运行态切片，并且相对于之后的无关变更有意保持陈旧。
    let mut candidate = preflight
        .retained_workspace_snapshot
        .clone()
        .expect("existing preflight should retain one exact session slice");
    assert!(
        candidate.session_messages.is_empty(),
        "Continue preflight must not clone retained presentation history"
    );
    // supplementalMessage is the only visible record created by the detached Continue transaction.
    // supplementalMessage 是分离 Continue 事务新建的唯一可见记录。
    let mut supplemental_message = SessionMessageEntry::new(
        SessionMessageKind::User,
        "继续补充",
        "supplemental continuation message",
    );
    supplemental_message.id = String::from("supplemental-continuation-message");
    candidate
        .session_messages
        .insert(session_id, vec![supplemental_message]);
    // clientSubmissionId owns both the claimed reference and ledger row.
    // clientSubmissionId 同时拥有已领取引用与账本记录。
    let client_submission_id = String::from("continue-attempt-a");
    // claimedSnapshot mirrors the immutable sidecar body after claim publication.
    // claimedSnapshot 镜像领取发布后的不可变 sidecar 正文。
    let mut claimed_snapshot = frozen_snapshot.clone();
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.claimed_attempt_id = Some(client_submission_id.clone());
    claimed_snapshot.updated_at = String::from("2000");
    // claimedReference is the exact content-addressed reference installed into the candidate row.
    // claimedReference 是安装到候选记录中的精确内容寻址引用。
    let claimed_reference = retry_reference_for_snapshot(&claimed_snapshot);
    let candidate_runtime = candidate
        .session_runtime_state
        .get_mut(&session_id)
        .expect("candidate runtime state should exist");
    candidate_runtime
        .provider_call_ledger
        .claim_continuation(
            continuation.generation,
            continuation.expected_ledger_revision,
            client_submission_id.clone(),
            2000,
        )
        .expect("candidate continuation should claim");
    candidate_runtime.execution_retry_snapshot = Some(claimed_reference.clone());
    candidate_runtime.active_precheck_turn_id = Some(String::from("precheck-a"));
    {
        // guard publishes concurrent unrelated and same-session mutations that the detached candidate does not contain.
        // guard 发布分离候选不包含的并发无关变更与同会话变更。
        let mut guard = state.lock().expect("service state lock");
        let retained_workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("retained workspace should exist");
        retained_workspace.session_runtime_state.insert(
            unrelated_session_id,
            vulcan_workbench_model::SessionRuntimeStateEntry {
                active_work_mode: Some(String::from("plan")),
                updated_at: String::from("1500"),
                ..Default::default()
            },
        );
        retained_workspace
            .session_runtime_state
            .get_mut(&session_id)
            .expect("target runtime state should exist")
            .active_work_mode = Some(String::from("review"));
    }
    {
        // guard simulates a same-session reference replacement after the claimed body was prepared.
        // guard 模拟已领取正文准备完成后发生的同会话引用替换。
        let mut guard = state.lock().expect("service state lock");
        guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .and_then(|runtime_state| runtime_state.execution_retry_snapshot.as_mut())
            .expect("frozen retry reference should exist")
            .content_sha256 = "f".repeat(64);
    }
    let stale_error = commit_continuation_claim_checkpoint(
        &state,
        &mut candidate,
        session_id,
        &preflight.turn_admission,
        continuation.generation,
        continuation.expected_ledger_revision,
        client_submission_id.as_str(),
        &frozen_reference,
        &frozen_snapshot,
    )
    .expect_err("a replaced live reference must reject the detached claim");
    assert_eq!(stale_error, "ERR_CONVERSATION_CONTINUE_STALE");
    {
        // guard restores the exact original token so the success path can prove its bounded merge.
        // guard 恢复精确原始令牌，使成功路径可以证明其有界合并。
        let mut guard = state.lock().expect("service state lock");
        guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .expect("target runtime state should exist")
            .execution_retry_snapshot = Some(frozen_reference.clone());
    }

    commit_continuation_claim_checkpoint(
        &state,
        &mut candidate,
        session_id,
        &preflight.turn_admission,
        continuation.generation,
        continuation.expected_ledger_revision,
        client_submission_id.as_str(),
        &frozen_reference,
        &frozen_snapshot,
    )
    .expect("frozen claim checkpoint should commit atomically");
    {
        // guard proves the first target-only commit preserved the concurrent unrelated row.
        // guard 证明第一次仅目标提交保留了并发无关记录。
        let guard = state.lock().expect("service state lock");
        assert_eq!(
            guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_runtime_state.get(&unrelated_session_id))
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            Some("plan")
        );
        assert_eq!(
            guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            Some("review")
        );
    }
    // materializedSession binds kernel registration to the same existing-session reservation.
    // materializedSession 将 kernel 登记绑定到同一个已有会话预留。
    let materialized_session = MaterializedPromptSession {
        session_id,
        project_id: Some(String::from("project-a")),
        session_title: String::from("父会话"),
        workspace_path: String::new(),
        kind: PromptSessionMaterializationKind::Existing,
    };
    // reservationId is the exact owner accepted by runtime-kernel registration.
    // reservationId 是运行时 kernel 登记接受的精确所有者。
    let reservation_id = preflight
        .turn_admission
        .kernel_reservation_id(&materialized_session)
        .expect("materialized session should match admission")
        .expect("existing session should own a reservation");
    {
        // guard starts the exact turn, writes its live kernel mirror, and applies concurrent setting and title mutations.
        // guard 启动精确回合、写入其实时 kernel 镜像，并应用并发设置与标题变更。
        let mut guard = state.lock().expect("service state lock");
        start_reserved_webui_session_kernel_user_turn(
            &mut guard,
            session_id,
            "precheck-a",
            reservation_id,
        )
        .expect("exact reservation should start the kernel turn");
        guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .expect("target runtime state should exist")
            .active_reasoning_effort_explicit = true;
        guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| {
                workspace
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
            })
            .expect("target summary should exist")
            .title = String::from("并发重命名");
    }
    mark_webui_prompt_snapshot_preparing(&mut candidate, session_id);
    commit_continuation_preparing_checkpoint(
        &state,
        &mut candidate,
        session_id,
        &preflight.turn_admission,
        continuation.generation,
        continuation.expected_ledger_revision,
        client_submission_id.as_str(),
        "precheck-a",
    )
    .expect("Preparing checkpoint should merge into the exact active turn");

    // guard proves the second commit retained both the unrelated row and the kernel mirror.
    // guard 证明第二次提交同时保留了无关记录与 kernel 镜像。
    let guard = state.lock().expect("service state lock");
    let retained_workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("retained workspace should exist");
    assert_eq!(
        retained_workspace
            .session_runtime_state
            .get(&unrelated_session_id)
            .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
        Some("plan")
    );
    assert!(
        retained_workspace
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| runtime_state.session_runtime_kernel.as_ref())
            .is_some_and(|kernel| kernel.has_active_turn())
    );
    assert_eq!(
        retained_workspace
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
        Some("review")
    );
    assert!(
        retained_workspace
            .session_runtime_state
            .get(&session_id)
            .is_some_and(|runtime_state| runtime_state.active_reasoning_effort_explicit)
    );
    assert_eq!(
        retained_workspace
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| session.title.as_str()),
        Some("并发重命名")
    );
    assert_eq!(
        retained_workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Preparing)
    );
    assert_eq!(
        retained_workspace
            .session_messages
            .get(&session_id)
            .expect("retained continuation messages")
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "retained-before-continuation",
            "supplemental-continuation-message"
        ],
        "Continue must preserve history and append its supplemental record exactly once"
    );
    assert!(
        candidate
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| runtime_state.session_runtime_kernel.as_ref())
            .is_some_and(|kernel| kernel.has_active_turn())
    );
    assert_eq!(
        candidate
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| session.title.as_str()),
        Some("并发重命名")
    );
}

/// Verifies live zero-byte retry references stay out of persisted session state.
/// 验证零字节实时重试引用不会进入持久化会话状态。
#[test]
fn transient_retry_reference_is_omitted_from_serialized_runtime_state() {
    let mut runtime_state = retry_runtime_state();
    let mut reference = retry_reference();
    reference.content_bytes = 0;
    runtime_state.execution_retry_snapshot = Some(reference);

    let encoded = serde_json::to_value(&runtime_state)
        .expect("runtime state with transient retry should serialize");

    assert!(encoded.get("execution_retry_snapshot").is_none());
}

/// Verifies an unreadable sidecar permanently closes the same visible claim token.
/// 验证不可读取的 sidecar 会永久关闭同一个可见领取令牌。
#[test]
fn execution_retry_unreadable_sidecar_invalidates_visible_reference() {
    // expectedReference is the exact lightweight token whose body failed validation.
    // expectedReference 是正文校验失败对应的精确轻量令牌。
    let expected_reference = retry_reference();
    // runtimeState owns both the visible reference and its mirrored ledger eligibility.
    // runtimeState 同时持有可见引用及其镜像账本资格。
    let mut runtime_state = retry_runtime_state();
    runtime_state.execution_retry_snapshot = Some(expected_reference.clone());
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("failed"),
                1000,
                String::from("error"),
                true,
            )
    );

    assert!(invalidate_execution_retry_reference_after_claim_failure(
        &mut runtime_state,
        &expected_reference,
        "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE",
        "The retained execution retry sidecar failed validation.",
        String::from("2000"),
    ));

    // invalidatedReference is the durable lightweight fact projected after the failed body read.
    // invalidatedReference 是正文读取失败后投影的可靠轻量事实。
    let invalidated_reference = runtime_state
        .execution_retry_snapshot
        .as_ref()
        .expect("invalidated reference should remain available for diagnostics");
    assert_eq!(
        invalidated_reference.state,
        ExecutionRetrySnapshotState::Invalidated
    );
    assert!(!invalidated_reference.retryable);
    assert!(invalidated_reference.requires_manual_confirmation);
    assert_eq!(
        invalidated_reference.blocked_reason.as_deref(),
        Some("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE")
    );
    assert!(!runtime_state.provider_call_ledger.continuation.eligible);
    assert_eq!(runtime_state.updated_at, "2000");
    // continuationSummary proves the corrected Workbench projection keeps a disabled problem-turn control with the concrete reason.
    // continuationSummary 证明修正后的 Workbench 投影会保留问题回合的禁用控件及其具体原因。
    let continuation_summary = conversation_continuation_summary(
        &runtime_state,
        vulcan_workbench_model::SessionRunPhase::Failed,
        SessionContinuationRuntimeBlockers::default(),
    )
    .expect("invalidated terminal boundary should remain visible for diagnostics");
    assert!(!continuation_summary.eligible);
    assert_eq!(
        continuation_summary.blocked_reason.as_deref(),
        Some("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE")
    );
}

/// Verifies an unlocked sidecar failure cannot invalidate a concurrently replaced reference.
/// 验证无锁 sidecar 失败不能使并发替换后的引用失效。
#[test]
fn execution_retry_sidecar_failure_preserves_concurrent_replacement() {
    // staleReference identifies the sidecar checked before the concurrent replacement.
    // staleReference 标识并发替换前接受检查的 sidecar。
    let stale_reference = retry_reference();
    // replacementReference is the newer authoritative token installed while I/O was unlocked.
    // replacementReference 是无锁 I/O 期间安装的更新权威令牌。
    let replacement_reference = retry_reference();
    // runtimeState exposes only the replacement and must remain unchanged.
    // runtimeState 只暴露替代引用，且必须保持不变。
    let mut runtime_state = retry_runtime_state();
    runtime_state.execution_retry_snapshot = Some(replacement_reference.clone());

    assert!(!invalidate_execution_retry_reference_after_claim_failure(
        &mut runtime_state,
        &stale_reference,
        "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE",
        "The retained execution retry sidecar failed validation.",
        String::from("2000"),
    ));
    assert_eq!(
        runtime_state.execution_retry_snapshot.as_ref(),
        Some(&replacement_reference)
    );
}

/// Verifies only deterministic retry-body failures are eligible for persistent invalidation.
/// 验证只有确定性的重试正文失败才允许持久失效。
#[test]
fn execution_retry_claim_failure_classification_is_fail_closed_and_scoped() {
    assert_eq!(
        execution_retry_claim_failure_blocked_reason(
            "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED"
        ),
        Some("ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED")
    );
    assert_eq!(
        execution_retry_claim_failure_blocked_reason(
            "ERR_CONVERSATION_CONTINUE_RETRY_PROVIDER_MISSING"
        ),
        Some("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING")
    );
    assert_eq!(
        execution_retry_claim_failure_blocked_reason("ERR_CONVERSATION_CONTINUE_SESSION_BUSY"),
        None
    );
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

/// Verifies a completed tool boundary accepts the next-provider action without replaying the old request.
/// 验证已完成工具边界可以接受下一 Provider 动作，而不会重放旧请求。
#[test]
fn execution_retry_claim_accepts_next_provider_boundary_after_tool_result() {
    let session_id = Uuid::new_v4();
    let mut runtime_state = retry_runtime_state();
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("tool-result-1"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                is_error: false,
                text: String::from("tool completed"),
            },
        });
    let mut snapshot = retry_snapshot(session_id);
    snapshot.pending_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
    snapshot.controller_snapshot =
        vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
            &runtime_state.conversation_state,
        )
        .expect("tool result should project to the Core snapshot");
    let canonical_bytes = serde_json::to_vec(&(
        &snapshot.controller_snapshot.messages,
        &snapshot.controller_snapshot.continuation,
    ))
    .expect("projected Core snapshot should serialize");
    snapshot.canonical_digest = Sha256::digest(&canonical_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
        .expect("completed tool result should admit the next provider request");
}

/// Verifies a committed recovery supplement is also an exact next-Provider seed boundary.
/// 验证已提交的恢复补充消息同样是精确的下一 Provider 播种边界。
#[test]
fn execution_retry_claim_accepts_next_provider_boundary_after_user_followup() {
    let session_id = Uuid::new_v4();
    let mut runtime_state = retry_runtime_state();
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("user-followup-1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("Apply the additional user constraint before continuing."),
                attachments: Vec::new(),
            },
        });
    let mut snapshot = retry_snapshot(session_id);
    snapshot.pending_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
    snapshot.controller_snapshot =
        vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
            &runtime_state.conversation_state,
        )
        .expect("user follow-up should project to the Core snapshot");
    let canonical_bytes = serde_json::to_vec(&(
        &snapshot.controller_snapshot.messages,
        &snapshot.controller_snapshot.continuation,
    ))
    .expect("projected Core snapshot should serialize");
    snapshot.canonical_digest = Sha256::digest(&canonical_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
        .expect("committed user follow-up should admit the next provider request");
}

/// Verifies the next-provider boundary cannot bypass the frozen route contract.
/// 验证下一 Provider 边界不能绕过冻结路由契约。
#[test]
fn execution_retry_next_provider_boundary_requires_route() {
    let session_id = Uuid::new_v4();
    let mut runtime_state = retry_runtime_state();
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("tool-result-1"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                is_error: false,
                text: String::from("tool completed"),
            },
        });
    let mut snapshot = retry_snapshot(session_id);
    snapshot.pending_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
    snapshot.provider_route = None;

    assert_eq!(
        validate_execution_retry_snapshot_for_claim(&runtime_state, session_id, &snapshot)
            .expect_err("next-provider continuation without route must be rejected"),
        "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING"
    );
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
    // claimedReference is the process-local shared-state reference before stop projection.
    // claimedReference 是停止投影前共享状态中的进程内引用。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed retry body should remain process-local");
    // staleRuntimeState deliberately retains the pre-stop canonical state in shared memory.
    // staleRuntimeState 刻意在共享内存中保留停止前规范状态。
    let mut stale_runtime_state = retry_runtime_state();
    stale_runtime_state.transient_execution_retry_snapshot =
        Some(std::sync::Arc::new(claimed_snapshot.clone()));
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
    stopped_conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("tool-result-after-stop"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-after-stop")),
                is_error: false,
                text: String::from("completed before stop"),
            },
        });
    stopped_conversation_state.continuation.updated_at = Some(String::from("3000"));

    refreeze_claimed_execution_retry_snapshot_with_canonical_state(
        &shared_state,
        &controller_runtime,
        session_id,
        &stopped_conversation_state,
        None,
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
            controller_root.session_directory(),
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
    assert_eq!(
        refreshed_snapshot.pending_action,
        ExecutionRetryPendingAction::PrepareNextProviderRequest
    );
    validate_execution_retry_snapshot_for_claim(
        &expected_runtime_state,
        session_id,
        &refreshed_snapshot,
    )
    .expect("refrozen retry snapshot should match the published post-stop state");
}

/// Verifies refreezing a claimed local boundary reopens the matching terminal ledger cache.
/// 验证重冻已领取的本地边界会重新开启匹配的终态账本缓存。
#[test]
fn execution_retry_refreeze_reopens_matching_terminal_eligibility() {
    // sessionId identifies the isolated retained retry owner.
    // sessionId 标识隔离的保留重试所有者。
    let session_id = Uuid::new_v4();
    // controllerRoot owns the temporary segmented session directory used by the immutable sidecar.
    // controllerRoot 持有不可变 sidecar 使用的临时分段会话目录。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime exposes the production persistence route for the fixture.
    // controllerRuntime 为测试夹具暴露生产形态持久化路由。
    let controller_runtime = controller_root.controller_runtime_handle();
    // claimedSnapshot models a Continue attempt interrupted at a proven local pre-provider boundary.
    // claimedSnapshot 模拟在已证明的 Provider 前本地边界中断的 Continue 尝试。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.claimed_attempt_id = Some(String::from("claimed-local-attempt"));
    claimed_snapshot.pending_action = ExecutionRetryPendingAction::PrepareNextProviderRequest;
    // claimedReference is the exact process-local pointer published before terminal cleanup.
    // claimedReference 是终态清理前发布的精确进程内指针。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed local retry body should remain process-local");
    // runtimeState owns a real error tail and a terminal ledger that has already closed claim ownership.
    // runtimeState 持有真实错误尾部以及已经关闭领取所有权的终态账本。
    let mut runtime_state = retry_runtime_state();
    runtime_state.transient_execution_retry_snapshot =
        Some(std::sync::Arc::new(claimed_snapshot.clone()));
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("refreeze-error-tail"),
            kind: SessionMessageKind::Error,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("执行中断"),
            body: String::from("准备阶段被中断"),
            metadata: None,
            created_at: String::from("1000"),
        });
    // committedToolResult proves the local action completed and the next Provider request can be rebuilt safely.
    // committedToolResult 证明本地动作已经完成，下一次 Provider 请求可以安全重建。
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("refreeze-tool-result"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("refreeze-tool-call")),
                is_error: false,
                text: String::from("completed"),
            },
        });
    runtime_state.execution_retry_snapshot = Some(claimed_reference);
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("failed"),
                1000,
                String::from("error"),
                true,
            )
    );
    // claimableBoundary captures the original visible token before the execution claim.
    // claimableBoundary 捕获执行领取前原始的可见令牌。
    let claimable_boundary = runtime_state.provider_call_ledger.continuation.clone();
    runtime_state
        .provider_call_ledger
        .claim_continuation(
            claimable_boundary.generation,
            claimable_boundary.expected_ledger_revision,
            String::from("claimed-local-attempt"),
            1001,
        )
        .expect("local retry boundary should claim");
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("claimed-local-attempt")),
                String::from("preparation_failed"),
                1002,
                String::from("error"),
                false,
            )
    );
    assert!(!runtime_state.provider_call_ledger.continuation.eligible);
    // canonicalState is the exact post-failure dialogue state used for sidecar refresh.
    // canonicalState 是用于刷新 sidecar 的精确失败后对话状态。
    let canonical_state = runtime_state.conversation_state.clone();
    // serviceState exposes the retained runtime through the production shared-state lock.
    // serviceState 通过生产共享状态锁暴露保留运行态。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState is consumed by the same refreeze publication path used by runtime failures.
    // sharedState 由运行时失败所使用的同一重冻发布路径消费。
    let shared_state = Arc::new(Mutex::new(service_state));

    refreeze_claimed_execution_retry_snapshot_with_canonical_state(
        &shared_state,
        &controller_runtime,
        session_id,
        &canonical_state,
        None,
        "test local interruption",
    )
    .expect("local retry boundary should refreeze");

    // refreshedRuntime is the authoritative state after sidecar publication and cache reconciliation.
    // refreshedRuntime 是 sidecar 发布与缓存协调后的权威状态。
    let guard = shared_state.lock().expect("shared test state should lock");
    let refreshed_runtime = guard
        .workspace_snapshot
        .as_ref()
        .expect("test workspace should remain available")
        .session_runtime_state
        .get(&session_id)
        .expect("refreshed runtime should remain available");
    // refreshedReference proves the sidecar returned to an executable frozen state.
    // refreshedReference 证明 sidecar 已返回可执行的冻结状态。
    let refreshed_reference = refreshed_runtime
        .execution_retry_snapshot
        .as_ref()
        .expect("refreshed retry reference should remain available");
    assert_eq!(
        refreshed_reference.state,
        ExecutionRetrySnapshotState::Frozen
    );
    assert!(refreshed_reference.replay_safety_proven());
    assert!(refreshed_runtime.provider_call_ledger.continuation.eligible);
    assert_eq!(refreshed_runtime.updated_at, refreshed_reference.updated_at);
}

/// Verifies a retained assistant tool tail replaces stale Provider replay with unprepared local recovery slots.
/// 验证已保留的助手工具尾部会用未准备的本地恢复槽位替换过期 Provider 重放。
#[test]
fn retained_assistant_tool_tail_selects_unprepared_tool_batch_recovery() {
    // sessionId owns the exact retry snapshot whose Provider action became stale after completion.
    // sessionId 持有 Provider 完成后其动作已过期的精确重试快照。
    let session_id = Uuid::new_v4();
    // request is the exact canonical assistant invocation that must never trigger Provider replay.
    // request 是绝不能触发 Provider 重放的精确规范助手调用。
    let request = vulcan_core::types::ToolInvocationRequest {
        tool_call_id: String::from("tool-item-crash-gap-1"),
        tool_name: String::from("read_file"),
        arguments: serde_json::json!({"path": "README.md"}),
        argument_parse_error: None,
    };
    // snapshot still carries the pre-completion dispatch action from the last durable checkpoint.
    // snapshot 仍携带上一可靠检查点留下的 Provider 完成前分派动作。
    let mut snapshot = retry_snapshot(session_id);
    // conversationState proves the Provider completed and retained the exact assistant tool batch.
    // conversationState 证明 Provider 已完成并保留精确助手工具批次。
    let mut conversation_state = retry_runtime_state().conversation_state;
    conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("assistant-crash-gap-1"),
        payload: SessionConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("provider-message-crash-gap-1")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![vulcan_context::SessionConversationToolCall {
                id: String::from("tool-item-crash-gap-1"),
                // An empty Provider-visible call id must resolve through the same canonical Core path during normal execution and crash recovery.
                // 空白 Provider 可见调用标识在正常执行与崩溃恢复中必须通过同一 Core 规范路径解析。
                tool_call_id: Some(String::from("   ")),
                tool_name: request.tool_name.clone(),
                arguments: request.arguments.clone(),
                argument_parse_error: request.argument_parse_error.clone(),
            }],
        },
    }];

    assert!(
        crate::reconcile_interrupted_execution_retry_canonical_state(
            &mut snapshot,
            &conversation_state,
        )
        .expect("retained assistant batch should reconcile")
    );

    let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = snapshot.pending_action else {
        panic!("retained assistant batch must replace stale Provider replay");
    };
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].request, request);
    assert_eq!(
        tools[0].state,
        vulcan_workbench_model::ExecutionRetryToolState::NotStarted
    );
    assert!(tools[0].binding_fingerprint.is_none());
    assert!(tools[0].result.is_none());
    assert!(snapshot.error.is_none());
}

/// Verifies a claimed Continue snapshot retains live batch, start, and result boundaries without retry JSON.
/// 验证已领取 Continue 快照无需重试 JSON 即可保留实时批次、启动与结果边界。
#[test]
fn execution_retry_tool_boundaries_retain_live_batch_start_and_result() {
    // sessionId identifies the exact retained conversation used by every durability assertion.
    // sessionId 标识每项可靠性断言使用的精确保留会话。
    let session_id = Uuid::new_v4();
    // controllerRoot owns the immutable retry bodies and segmented session state.
    // controllerRoot 持有不可变重试正文与分段会话状态。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime exposes the production persistence implementation.
    // controllerRuntime 暴露生产持久化实现。
    let controller_runtime = controller_root.controller_runtime_handle();
    // request is the exact original model-issued tool invocation.
    // request 是模型发出的精确原始工具调用。
    let request = vulcan_core::types::ToolInvocationRequest {
        tool_call_id: String::from("tool-item-1"),
        tool_name: String::from("read_file"),
        arguments: serde_json::json!({"path": "README.md"}),
        argument_parse_error: None,
    };
    // bindingFingerprint identifies the resolved descriptor, schema, and execution policy.
    // bindingFingerprint 标识已解析的描述、Schema 与执行策略。
    let binding_fingerprint = "a".repeat(64);
    // claimedSnapshot models a Continue attempt after the Provider completed a tool batch.
    // claimedSnapshot 模拟 Provider 完成工具批次后的 Continue 尝试。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.claimed_attempt_id = Some(String::from("claimed-tool-attempt"));
    claimed_snapshot.controller_snapshot.messages = vec![vulcan_core::types::ConversationMessage {
        id: String::from("assistant-tool-batch-1"),
        payload: vulcan_core::types::ConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("provider-message-1")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![vulcan_core::types::ConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("   ")),
                tool_name: request.tool_name.clone(),
                arguments: request.arguments.clone(),
                argument_parse_error: request.argument_parse_error.clone(),
            }],
        },
    }];
    // pendingAction represents the hard-crash window after the assistant batch was retained but
    // before the runtime could resolve and publish its complete BatchPrepared binding set.
    // pendingAction 表示助手批次已经保留、但运行时尚未来得及解析并发布完整 BatchPrepared
    // 绑定集合的硬崩溃窗口。
    claimed_snapshot.pending_action = ExecutionRetryPendingAction::ExecuteToolBatch {
        tools: vec![vulcan_workbench_model::ExecutionRetryToolProgress {
            request: request.clone(),
            result: None,
            binding_fingerprint: None,
            state: vulcan_workbench_model::ExecutionRetryToolState::NotStarted,
        }],
    };
    // claimedReference is the lightweight exact pointer owned by the retained session.
    // claimedReference 是保留会话持有的轻量精确指针。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::persist_interrupted_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed tool retry sidecar should persist");
    vulcan_workspace_store::workspace_store::delete_execution_retry_snapshot(
        controller_root.session_directory(),
        session_id,
        &claimed_reference,
    )
    .expect("a consumed Continue body should be removed before live tool execution");
    // claimedContentSha identifies the consumed durable boundary before live transitions begin.
    // claimedContentSha 标识实时迁移开始前已经消费的持久边界。
    let claimed_content_sha = claimed_reference.content_sha256.clone();
    assert!(
        !controller_root
            .session_directory()
            .join("retry.json")
            .exists()
    );
    // runtimeState mirrors the canonical assistant batch refreshed before tool execution.
    // runtimeState 镜像工具执行前刷新的规范助手批次。
    let mut runtime_state = retry_runtime_state();
    runtime_state.transient_execution_retry_snapshot =
        Some(std::sync::Arc::new(claimed_snapshot.clone()));
    runtime_state.execution_retry_snapshot = Some(claimed_reference);
    runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("assistant-tool-batch-1"),
        payload: SessionConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("provider-message-1")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![vulcan_context::SessionConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("   ")),
                tool_name: request.tool_name.clone(),
                arguments: request.arguments.clone(),
                argument_parse_error: request.argument_parse_error.clone(),
            }],
        },
    }];
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("failed"),
                1000,
                String::from("error"),
                true,
            )
    );
    // serviceState installs the retained runtime under the production shared-state lock.
    // serviceState 在生产共享状态锁下安装保留运行态。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState is consumed by the same boundary publisher used in production.
    // sharedState 由生产环境使用的同一边界发布器消费。
    let shared_state = Arc::new(Mutex::new(service_state));
    // runtime deterministically drives asynchronous live-boundary publication.
    // runtime 以确定方式驱动异步实时边界发布。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // preparedBoundary proves the whole batch is retained before any tool body starts.
        // preparedBoundary 证明任何工具主体启动前完整批次已经保留。
        let prepared_boundary =
            vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary::BatchPrepared(
                vec![vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest {
                    request: request.clone(),
                    binding_fingerprint: binding_fingerprint.clone(),
                }],
            );
        // preparedOwner is the immutable reference that must still own the batch transition.
        // preparedOwner 是仍必须拥有批次迁移的不可变引用。
        let prepared_owner = retained_execution_retry_reference(&shared_state, session_id);
        crate::persist_execution_retry_tool_recovery_boundary(
            &shared_state,
            &controller_runtime,
            session_id,
            &prepared_owner,
            &prepared_boundary,
        )
        .await
        .expect("complete tool batch should be retained");

        // preparedReference selects the exact immutable body published by the batch boundary.
        // preparedReference 选择批次边界发布的精确不可变正文。
        let prepared_reference = shared_state
            .lock()
            .expect("shared test state should lock")
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|state| state.execution_retry_snapshot.clone())
            .expect("prepared retry reference should exist");
        let prepared_snapshot =
            retained_transient_execution_retry_snapshot(&shared_state, session_id);
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
            &prepared_snapshot.pending_action
        else {
            panic!("prepared boundary must materialize a tool batch");
        };
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].request, request);
        assert_eq!(
            tools[0].binding_fingerprint.as_deref(),
            Some(binding_fingerprint.as_str())
        );
        assert_eq!(
            tools[0].state,
            vulcan_workbench_model::ExecutionRetryToolState::NotStarted
        );
        assert!(tools[0].result.is_none());
        assert!(prepared_reference.live_boundary_safety_proven());
        assert_ne!(prepared_reference.content_sha256, claimed_content_sha);

        // startedBoundary records that the exact external body may now have side effects.
        // startedBoundary 记录精确外部工具主体现在可能产生副作用。
        let started_boundary =
            vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary::Started {
                request: request.clone(),
                binding_fingerprint: binding_fingerprint.clone(),
            };
        // startedOwner is the exact reference published by the preceding batch transition.
        // startedOwner 是前一批次迁移发布的精确引用。
        let started_owner = retained_execution_retry_reference(&shared_state, session_id);
        crate::persist_execution_retry_tool_recovery_boundary(
            &shared_state,
            &controller_runtime,
            session_id,
            &started_owner,
            &started_boundary,
        )
        .await
        .expect("tool start should be retained");

        // startedReference proves side-effect uncertainty remains reconcilable.
        // startedReference 证明副作用不确定性仍可协调。
        let started_reference = shared_state
            .lock()
            .expect("shared test state should lock")
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|state| state.execution_retry_snapshot.clone())
            .expect("started retry reference should exist");
        let started_snapshot =
            retained_transient_execution_retry_snapshot(&shared_state, session_id);
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
            &started_snapshot.pending_action
        else {
            panic!("started boundary must retain the tool batch");
        };
        assert_eq!(
            tools[0].state,
            vulcan_workbench_model::ExecutionRetryToolState::Running
        );
        assert!(
            started_snapshot
                .error
                .as_ref()
                .is_some_and(|error| error.side_effect_observed)
        );
        assert!(started_reference.live_boundary_safety_proven());
        assert_ne!(started_reference.content_sha256, prepared_reference.content_sha256);

        // result is the exact terminal value retained for later recovery reuse.
        // result 是为后续恢复复用而保留的精确终态值。
        let result = vulcan_core::types::ToolInvocationResult {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            model_output: String::from("file contents"),
            host_result: Some(serde_json::json!({"bytes": 13})),
            is_error: false,
        };
        // finishedBoundary replaces running uncertainty with the exact retained result.
        // finishedBoundary 用精确保留结果替换运行中不确定性。
        let finished_boundary =
            vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary::Finished(
                result.clone(),
            );
        // finishedOwner is the exact running reference that may receive this terminal result.
        // finishedOwner 是可以接收该终态结果的精确运行中引用。
        let finished_owner = retained_execution_retry_reference(&shared_state, session_id);
        crate::persist_execution_retry_tool_recovery_boundary(
            &shared_state,
            &controller_runtime,
            session_id,
            &finished_owner,
            &finished_boundary,
        )
        .await
        .expect("tool result should be retained");

        // finishedReference selects the final exact tool state.
        // finishedReference 选择最终精确工具状态。
        let finished_reference = shared_state
            .lock()
            .expect("shared test state should lock")
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|state| state.execution_retry_snapshot.clone())
            .expect("finished retry reference should exist");
        let finished_snapshot =
            retained_transient_execution_retry_snapshot(&shared_state, session_id);
        assert_ne!(finished_reference.content_sha256, started_reference.content_sha256);
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
            &finished_snapshot.pending_action
        else {
            panic!("finished boundary must retain the tool batch");
        };
        assert_eq!(
            tools[0].state,
            vulcan_workbench_model::ExecutionRetryToolState::Completed
        );
        assert_eq!(tools[0].result.as_ref(), Some(&result));
        assert!(finished_snapshot.error.is_none());
        assert!(finished_reference.live_boundary_safety_proven());
        assert!(
            !controller_root.session_directory().join("retry.json").exists(),
            "live tool boundaries must not recreate durable retry JSON"
        );
    });

    // persistedState proves live retry identity remains process-local while non-retry ledger state persists.
    // persistedState 证明实时重试身份保持在进程内，同时非重试账本状态仍会持久化。
    let state_path = controller_root
        .data_root()
        .join("sessions")
        .join("project-a")
        .join(format!("1970-01-01-{session_id}"))
        .join("state.json");
    let persisted_state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&state_path).expect("durable runtime state should exist"),
    )
    .expect("durable runtime state should decode");
    assert!(
        persisted_state
            .pointer("/runtime_state/execution_retry_snapshot")
            .is_none()
    );
    assert_eq!(
        persisted_state
            .pointer("/runtime_state/provider_call_ledger/continuation/eligible")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

/// Verifies a late tool event cannot mutate or invalidate a concurrently replaced recovery reference.
/// 验证较晚到达的工具事件不能修改或失效并发替换后的恢复引用。
#[test]
fn execution_retry_tool_boundary_preserves_concurrent_replacement() {
    // sessionId identifies the isolated retained conversation used by the stale-owner assertion.
    // sessionId 标识过期所有者断言使用的隔离保留会话。
    let session_id = Uuid::new_v4();
    // controllerRoot owns the immutable sidecars required by the production boundary function.
    // controllerRoot 持有生产边界函数所需的不可变 sidecar。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime exposes the exact production persistence handle.
    // controllerRuntime 暴露精确的生产持久化句柄。
    let controller_runtime = controller_root.controller_runtime_handle();
    // staleSnapshot is the body owned when an older tool event was emitted.
    // staleSnapshot 是较旧工具事件发出时拥有的正文。
    let stale_snapshot = retry_snapshot(session_id);
    // staleReference is the immutable owner captured by that older event.
    // staleReference 是该较旧事件捕获的不可变所有者。
    let stale_reference =
        vulcan_workspace_store::workspace_store::persist_interrupted_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            &stale_snapshot,
        )
        .expect("stale retry sidecar should persist");
    // replacementSnapshot represents the newer stop/restart recovery generation.
    // replacementSnapshot 表示更新的停止或重启恢复代际。
    let mut replacement_snapshot = retry_snapshot(session_id);
    replacement_snapshot.generation = stale_snapshot.generation + 1;
    // replacementReference is the only owner allowed to remain after the stale event fails.
    // replacementReference 是过期事件失败后唯一允许保留的所有者。
    let replacement_reference =
        vulcan_workspace_store::workspace_store::persist_interrupted_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            &replacement_snapshot,
        )
        .expect("replacement retry sidecar should persist");
    // runtimeState exposes only the replacement before the stale boundary is processed.
    // runtimeState 在处理过期边界前只暴露替代引用。
    let mut runtime_state = retry_runtime_state();
    runtime_state.execution_retry_snapshot = Some(replacement_reference.clone());
    // serviceState installs the replacement under the production shared lock.
    // serviceState 在生产共享锁下安装替代引用。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState is consumed by both exact persistence and exact invalidation.
    // sharedState 由精确持久化与精确失效共同消费。
    let shared_state = Arc::new(Mutex::new(service_state));
    // request gives the stale batch a syntactically complete tool identity.
    // request 为过期批次提供结构完整的工具身份。
    let request = vulcan_core::types::ToolInvocationRequest {
        tool_call_id: String::from("stale-tool-call"),
        tool_name: String::from("read_file"),
        arguments: serde_json::json!({"path": "README.md"}),
        argument_parse_error: None,
    };
    // boundary is the late prepared event still bound to staleReference.
    // boundary 是仍绑定到 staleReference 的较晚已准备事件。
    let boundary =
        vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary::BatchPrepared(
            vec![
                vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest {
                    request,
                    binding_fingerprint: "a".repeat(64),
                },
            ],
        );
    // runtime drives the asynchronous persistence rejection deterministically.
    // runtime 以确定方式驱动异步持久化拒绝。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    // error proves the stale immutable owner is rejected before the replacement can be loaded or changed.
    // error 证明过期不可变所有者会在替代引用被加载或修改前遭到拒绝。
    let error = runtime
        .block_on(crate::persist_execution_retry_tool_recovery_boundary(
            &shared_state,
            &controller_runtime,
            session_id,
            &stale_reference,
            &boundary,
        ))
        .expect_err("stale tool boundary owner must be rejected");
    assert!(error.starts_with("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING"));

    // invalidated proves the caller's exact failure cleanup cannot target the replacement.
    // invalidated 证明调用方的精确失败清理不能作用到替代引用。
    let invalidated = invalidate_execution_retry_reference_if_matches(
        &shared_state,
        session_id,
        &stale_reference,
        "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
        "The stale tool recovery boundary was rejected.",
    )
    .expect("exact stale-owner invalidation should complete");
    assert!(!invalidated);
    assert_eq!(
        retained_execution_retry_reference(&shared_state, session_id),
        replacement_reference
    );
}

/// Verifies stopping a started tool converts it to outcome-unknown and reopens the problem turn.
/// 验证停止已启动工具会将其转换为结果未知，并重新开放问题回合。
#[test]
fn execution_retry_stop_refreeze_reconciles_running_tool_without_reexecution() {
    // sessionId identifies the isolated retained conversation used by the stop transition.
    // sessionId 标识停止状态转换使用的隔离保留会话。
    let session_id = Uuid::new_v4();
    // controllerRoot owns the immutable retry sidecars and durable main-session state.
    // controllerRoot 持有不可变重试 sidecar 与可靠主会话状态。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime exposes the exact production persistence path.
    // controllerRuntime 暴露精确的生产持久化路径。
    let controller_runtime = controller_root.controller_runtime_handle();
    // request is the original invocation whose body crossed the Started boundary.
    // request 是主体已经跨过 Started 边界的原始调用。
    let request = vulcan_core::types::ToolInvocationRequest {
        tool_call_id: String::from("tool-call-stop-1"),
        tool_name: String::from("write_file"),
        arguments: serde_json::json!({"path": "result.txt", "content": "value"}),
        argument_parse_error: None,
    };
    // bindingFingerprint freezes the exact resolved tool implementation used by the interrupted attempt.
    // bindingFingerprint 冻结被中断尝试使用的精确已解析工具实现。
    let binding_fingerprint = "b".repeat(64);
    // claimedSnapshot represents the live Continue attempt before its tool batch is materialized.
    // claimedSnapshot 表示工具批次物化前的实时 Continue 尝试。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.claimed_attempt_id = Some(String::from("claimed-stop-attempt"));
    claimed_snapshot.controller_snapshot.messages = vec![vulcan_core::types::ConversationMessage {
        id: String::from("assistant-tool-stop-1"),
        payload: vulcan_core::types::ConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("provider-message-stop-1")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![vulcan_core::types::ConversationToolCall {
                id: String::from("tool-item-stop-1"),
                tool_call_id: Some(request.tool_call_id.clone()),
                tool_name: request.tool_name.clone(),
                arguments: request.arguments.clone(),
                argument_parse_error: request.argument_parse_error.clone(),
            }],
        },
    }];
    // claimedReference binds the retained runtime to the process-local claimed body.
    // claimedReference 将保留运行态绑定到进程内已领取正文。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed stop retry body should remain process-local");
    // runtimeState carries both the canonical tool batch and a visible problem-turn error tail.
    // runtimeState 同时携带规范工具批次与用户可见的问题回合错误尾部。
    let mut runtime_state = retry_runtime_state();
    runtime_state.transient_execution_retry_snapshot =
        Some(std::sync::Arc::new(claimed_snapshot.clone()));
    runtime_state.execution_retry_snapshot = Some(claimed_reference);
    runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("assistant-tool-stop-1"),
        payload: SessionConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("provider-message-stop-1")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![vulcan_context::SessionConversationToolCall {
                id: String::from("tool-item-stop-1"),
                tool_call_id: Some(request.tool_call_id.clone()),
                tool_name: request.tool_name.clone(),
                arguments: request.arguments.clone(),
                argument_parse_error: request.argument_parse_error.clone(),
            }],
        },
    }];
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("error-stop-1"),
            kind: SessionMessageKind::Error,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("Interrupted"),
            body: String::from("Tool execution was interrupted."),
            metadata: None,
            created_at: String::from("1000"),
        });
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("stopped"),
                1000,
                String::from("error"),
                true,
            )
    );
    // canonicalState is the exact post-stop dialogue state used for refreeze.
    // canonicalState 是重冻结使用的精确停止后对话状态。
    let canonical_state = runtime_state.conversation_state.clone();
    // serviceState installs the fixture under the production shared-state lock.
    // serviceState 在生产共享状态锁下安装测试夹具。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState is consumed by the exact live boundary and refreeze implementations.
    // sharedState 由精确的实时边界与重冻结实现消费。
    let shared_state = Arc::new(Mutex::new(service_state));
    // runtime drives the two durable tool transitions before the synchronous refreeze.
    // runtime 在同步重冻结前驱动两次可靠工具状态转换。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // preparedBoundary retains the entire original tool batch before execution.
        // preparedBoundary 在执行前保留完整原始工具批次。
        let prepared_boundary =
            vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary::BatchPrepared(
                vec![vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest {
                    request: request.clone(),
                    binding_fingerprint: binding_fingerprint.clone(),
                }],
            );
        // preparedOwner is the immutable reference that must still own the stop-test batch transition.
        // preparedOwner 是仍必须拥有停止测试批次迁移的不可变引用。
        let prepared_owner = retained_execution_retry_reference(&shared_state, session_id);
        crate::persist_execution_retry_tool_recovery_boundary(
            &shared_state,
            &controller_runtime,
            session_id,
            &prepared_owner,
            &prepared_boundary,
        )
        .await
        .expect("tool batch should become durable before start");
        // startedBoundary proves the external body may have executed before the stop.
        // startedBoundary 证明外部工具主体可能已在停止前执行。
        let started_boundary =
            vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryBoundary::Started {
                request: request.clone(),
                binding_fingerprint: binding_fingerprint.clone(),
            };
        // startedOwner is the exact prepared reference that may cross into side-effect uncertainty.
        // startedOwner 是可以跨入副作用不确定状态的精确已准备引用。
        let started_owner = retained_execution_retry_reference(&shared_state, session_id);
        crate::persist_execution_retry_tool_recovery_boundary(
            &shared_state,
            &controller_runtime,
            session_id,
            &started_owner,
            &started_boundary,
        )
        .await
        .expect("tool start should become durable");
    });

    refreeze_claimed_execution_retry_snapshot_with_canonical_state(
        &shared_state,
        &controller_runtime,
        session_id,
        &canonical_state,
        None,
        "test user stop after tool start",
    )
    .expect("started tool should refreeze into a reconcilable outcome");

    // frozenRuntime is the authoritative problem-turn state after the stop transaction.
    // frozenRuntime 是停止事务后的权威问题回合状态。
    let frozen_runtime = shared_state
        .lock()
        .expect("shared test state should lock")
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .cloned()
        .expect("frozen runtime should remain available");
    // frozenReference selects the exact reconciled immutable body.
    // frozenReference 选择精确协调后的不可变正文。
    let frozen_reference = frozen_runtime
        .execution_retry_snapshot
        .as_ref()
        .expect("frozen retry reference should remain available");
    assert_eq!(frozen_reference.state, ExecutionRetrySnapshotState::Frozen);
    assert!(frozen_reference.replay_safety_proven());
    assert!(frozen_runtime.provider_call_ledger.continuation.eligible);
    // frozenSnapshot proves Continue will synthesize an unknown-result ToolResult instead of re-executing.
    // frozenSnapshot 证明 Continue 将合成结果未知的 ToolResult，而不是重新执行。
    let frozen_snapshot = vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        controller_root.session_directory(),
        session_id,
        frozen_reference,
    )
    .expect("frozen retry sidecar should remain readable");
    let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = &frozen_snapshot.pending_action
    else {
        panic!("stopped tool must retain its recovery batch");
    };
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].request, request);
    assert_eq!(
        tools[0].binding_fingerprint.as_deref(),
        Some(binding_fingerprint.as_str())
    );
    assert_eq!(
        tools[0].state,
        vulcan_workbench_model::ExecutionRetryToolState::OutcomeUnknown
    );
    assert!(tools[0].result.is_none());
}

/// Verifies an early stream exit replaces a claimed token and reopens its safety-proven failed boundary.
/// 验证流提前退出会替换已领取令牌，并重新开放已证明安全的失败边界。
#[test]
fn execution_retry_early_stream_terminal_reopens_safe_failed_ledger() {
    // sessionId owns the minimal retained workspace used by the early-exit transaction.
    // sessionId 持有提前退出事务使用的最小保留工作区。
    let session_id = Uuid::new_v4();
    // serviceState starts with one regular session and receives its retained retry state below.
    // serviceState 从单个常规会话开始，并在下方接收其保留重试状态。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // runtimeState simulates the exact ledger state after a Continue claim crossed CAS.
    // runtimeState 模拟 Continue 领取跨过 CAS 后的精确账本状态。
    let mut runtime_state = retry_runtime_state();
    runtime_state.execution_retry_snapshot = Some(retry_reference());
    assert!(
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("logical-a")),
                String::from("failed"),
                1000,
                String::from("error"),
                true,
            )
    );
    // claimedGeneration and claimedRevision identify the old token that must not survive owner cleanup.
    // claimedGeneration 与 claimedRevision 标识所有权清理后不得继续存在的旧令牌。
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
            String::from("continue-attempt-a"),
            1001,
        )
        .expect("fixture continuation should claim");
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("fixture workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState is the production-shaped lock boundary consumed by the materializer.
    // sharedState 是终态物化器使用的生产形态加锁边界。
    let shared_state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    materialize_early_stream_terminal_snapshot(&shared_state, session_id)
        .expect("early exit should materialize a terminal continuation");
    // terminalRuntime is read from authoritative retained memory and must contain no prior claim ownership.
    // terminalRuntime 从权威保留内存读取，且不得保留先前的领取所有权。
    let terminal_guard = shared_state.lock().expect("lock terminalized state");
    let terminal_runtime = terminal_guard
        .workspace_snapshot
        .as_ref()
        .expect("terminal workspace should remain retained")
        .session_runtime_state
        .get(&session_id)
        .expect("terminal runtime should remain present");
    assert_eq!(
        terminal_runtime
            .provider_call_ledger
            .continuation
            .generation,
        claimed_generation + 1
    );
    assert_eq!(
        terminal_runtime
            .provider_call_ledger
            .continuation
            .terminal_reason
            .as_deref(),
        Some("failed")
    );
    assert_eq!(
        terminal_runtime
            .provider_call_ledger
            .continuation
            .claimed_at_ms,
        None
    );
    assert_eq!(
        terminal_runtime
            .provider_call_ledger
            .continuation
            .claimed_execution_id,
        None
    );
    assert!(terminal_runtime.provider_call_ledger.continuation.eligible);
}

/// Verifies stop refreeze preserves a proven dispatched/no-output Provider boundary as claimable.
/// 验证停止重冻会把已证明分派且无输出的 Provider 边界保持为可领取状态。
#[test]
fn execution_retry_stop_refreeze_uses_exact_provider_call_evidence() {
    // sessionId identifies the isolated retained session and sidecar.
    // sessionId 标识隔离的保留会话与 sidecar。
    let session_id = Uuid::new_v4();
    // streamTaskId is shared by the retry body's physical attempt and the retained call binding.
    // streamTaskId 由重试正文物理尝试与保留调用绑定共享。
    let stream_task_id = Uuid::new_v4();
    // providerCallId is the exact ledger key bound by the Started telemetry boundary.
    // providerCallId 是 Started 遥测边界绑定的精确账本键。
    let provider_call_id = String::from("provider-call-a");
    // controllerRoot owns the exact temporary segmented session directory.
    // controllerRoot 持有精确临时分段会话目录。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime is the production-shaped persistence handle for the fixture.
    // controllerRuntime 是测试夹具使用的生产形态持久化句柄。
    let controller_runtime = controller_root.controller_runtime_handle();
    // claimedSnapshot models an in-flight retry at the exact Provider request boundary.
    // claimedSnapshot 模拟精确 Provider 请求边界上的执行中重试。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.physical_attempt_id = stream_task_id.to_string();
    claimed_snapshot.claimed_attempt_id = Some(stream_task_id.to_string());
    // claimedReference is the process-local lightweight token retained by the session state.
    // claimedReference 是会话状态保留的进程内轻量令牌。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed Provider retry body should remain process-local");
    // runtimeState owns both the retry reference and exact terminal Provider ledger fact.
    // runtimeState 同时持有重试引用与精确 Provider 终态账本事实。
    let mut runtime_state = retry_runtime_state();
    runtime_state.transient_execution_retry_snapshot =
        Some(std::sync::Arc::new(claimed_snapshot.clone()));
    runtime_state.execution_retry_snapshot = Some(claimed_reference);
    // providerIdentity is reused by Started and Completed to satisfy immutable ledger identity.
    // providerIdentity 由 Started 与 Completed 复用，以满足不可变账本身份约束。
    let provider_identity = retry_provider_call_identity();
    assert!(runtime_state.provider_call_ledger.observe_started(
        vulcan_workbench_model::SessionProviderCallStarted {
            provider_call_id: provider_call_id.clone(),
            identity: provider_identity.clone(),
            prepared_at_ms: 1_000,
            request_assembly_fingerprint: Some(String::from("request-a")),
            context_generation_id: Some(String::from("context-a")),
            context_start_record_id: None,
            model_route_fingerprint: Some(String::from("route-a")),
            toolset_fingerprint: None,
            system_prompt_fingerprint: None,
            request_text_measurement: None,
            budget_accounting: None,
        }
    ));
    assert!(runtime_state.provider_call_ledger.observe_completed(
        vulcan_workbench_model::SessionProviderCallCompleted {
            provider_call_id: provider_call_id.clone(),
            identity: provider_identity,
            status: vulcan_workbench_model::SessionProviderCallStatus::Cancelled,
            provider_usage_evidence: vulcan_workbench_model::SessionProviderUsageEvidence {
                availability: vulcan_workbench_model::SessionProviderUsageAvailability::Missing,
                provider_usage: None,
            },
            prepared_at_ms: 1_000,
            dispatched_at_ms: Some(1_001),
            first_event_at_ms: None,
            first_token_at_ms: None,
            completed_at_ms: 1_002,
            dispatch_outcome: Some(String::from("dispatched")),
            dispatch_outcome_known: Some(true),
            semantic_output_observed: Some(false),
            semantic_output_observed_known: Some(true),
            replay_safe: Some(true),
            error_category: Some(String::from("host_cancelled")),
            error_code: Some(String::from("consumer_stream_dropped")),
        }
    ));
    // canonicalState remains unchanged because no visible output or tool result crossed the boundary.
    // canonicalState 保持不变，因为没有可见输出或工具结果越过边界。
    let canonical_state = runtime_state.conversation_state.clone();
    // serviceState exposes the exact ledger and reference through the production shared-state shape.
    // serviceState 通过生产共享状态形态暴露精确账本与引用。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState matches the production mutex boundary used by stop refreeze.
    // sharedState 对应停止重冻使用的生产互斥边界。
    let shared_state = Arc::new(Mutex::new(service_state));

    refreeze_claimed_execution_retry_snapshot_with_canonical_state(
        &shared_state,
        &controller_runtime,
        session_id,
        &canonical_state,
        Some((stream_task_id, provider_call_id)),
        "test stop after dispatch",
    )
    .expect("exact safe Provider evidence should refreeze the retry");

    // refreshedReference must remain claimable instead of regressing to dispatch-unknown.
    // refreshedReference 必须保持可领取，而不是退化为分派未知。
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
    assert!(refreshed_reference.replay_safety_proven());
    assert_eq!(refreshed_reference.blocked_reason, None);
    // refreshedSnapshot retains the exact structured dispatch and semantic-output facts.
    // refreshedSnapshot 保留精确结构化分派与语义输出事实。
    let refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            &refreshed_reference,
        )
        .expect("refreshed retry sidecar should load");
    let interruption_error = refreshed_snapshot
        .error
        .expect("refrozen snapshot should retain interruption facts");
    assert_eq!(
        interruption_error.dispatch_outcome,
        vulcan_ai_contract::DispatchOutcome::Dispatched
    );
    assert!(!interruption_error.semantic_output_observed);
    assert!(interruption_error.retryable);
}

/// Verifies a launch failure before the stream body starts refreezes a claimed Provider action as proven not dispatched.
/// 验证流主体启动前的启动失败会将已领取 Provider 动作重冻为已证明未分派。
#[test]
fn execution_retry_pre_stream_launch_failure_is_retryable_without_dispatch_ambiguity() {
    // sessionId identifies the isolated retained session and immutable retry sidecar.
    // sessionId 标识隔离的保留会话与不可变重试 sidecar。
    let session_id = Uuid::new_v4();
    // controllerRoot owns the exact temporary segmented session directory.
    // controllerRoot 持有精确临时分段会话目录。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // controllerRuntime exposes the production-shaped sidecar persistence handle.
    // controllerRuntime 暴露生产形态的 sidecar 持久化句柄。
    let controller_runtime = controller_root.controller_runtime_handle();
    // attemptId identifies the Continue claim that failed before its gated future ran.
    // attemptId 标识在闸门 future 运行前失败的 Continue 领取。
    let attempt_id = Uuid::new_v4().to_string();
    // claimedSnapshot retains a DispatchProviderRequest action whose stream body never started.
    // claimedSnapshot 保留流主体从未启动的 DispatchProviderRequest 动作。
    let mut claimed_snapshot = retry_snapshot(session_id);
    claimed_snapshot.state = ExecutionRetrySnapshotState::Claimed;
    claimed_snapshot.physical_attempt_id = attempt_id.clone();
    claimed_snapshot.claimed_attempt_id = Some(attempt_id);
    // claimedReference is the exact process-local token published before stream registration.
    // claimedReference 是流注册前发布的精确进程内令牌。
    let claimed_reference =
        vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
            session_id,
            &claimed_snapshot,
        )
        .expect("claimed launch retry body should remain process-local");
    // runtimeState mirrors the claimed token through the production session runtime shape.
    // runtimeState 通过生产会话运行态形态镜像已领取令牌。
    let mut runtime_state = retry_runtime_state();
    runtime_state.transient_execution_retry_snapshot =
        Some(std::sync::Arc::new(claimed_snapshot.clone()));
    runtime_state.execution_retry_snapshot = Some(claimed_reference);
    // serviceState owns the retained workspace used by the production refreeze transaction.
    // serviceState 持有生产重冻事务使用的保留工作区。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist")
        .session_runtime_state
        .insert(session_id, runtime_state);
    // sharedState matches the exact mutex boundary used by a failed stream launch.
    // sharedState 对应流启动失败使用的精确互斥边界。
    let shared_state = Arc::new(Mutex::new(service_state));

    refreeze_claimed_execution_retry_snapshot_before_provider_dispatch(
        &shared_state,
        &controller_runtime,
        session_id,
        "test start gate rejected before stream execution",
    )
    .expect("pre-stream failure should refreeze the claimed retry");

    // refreshedReference must be frozen and immediately eligible for exact claim validation.
    // refreshedReference 必须已冻结，并可立即接受精确领取校验。
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
    assert_eq!(
        refreshed_reference.state,
        ExecutionRetrySnapshotState::Frozen
    );
    assert!(refreshed_reference.claimed_attempt_id.is_none());
    assert!(refreshed_reference.replay_safety_proven());
    // refreshedSnapshot proves the attempt never dispatched instead of introducing an unknown boundary.
    // refreshedSnapshot 证明本次尝试从未分派，而不是引入未知边界。
    let refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
            controller_root.session_directory(),
            session_id,
            &refreshed_reference,
        )
        .expect("refreshed retry sidecar should load");
    let interruption_error = refreshed_snapshot
        .error
        .expect("pre-stream refreeze should retain interruption facts");
    assert_eq!(
        interruption_error.dispatch_outcome,
        vulcan_ai_contract::DispatchOutcome::NotDispatched
    );
    assert!(!interruption_error.semantic_output_observed);
    assert!(!interruption_error.side_effect_observed);
    assert!(interruption_error.retryable);
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
            provider_attachments: Vec::new(),
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
            provider_attachments: Vec::new(),
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

/// Verifies an interrupted Provider boundary can replay after prior tool effects were retained canonically.
/// 验证先前工具影响已规范保留后，中断的 Provider 边界可以重放。
#[test]
fn execution_retry_claim_allows_provider_replay_after_retained_tool_side_effect() {
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
        retryable: true,
        retry_after: None,
    });

    validate_execution_retry_snapshot_for_claim(&retry_runtime_state(), session_id, &snapshot)
        .expect("Provider replay must not re-execute already retained tool effects");
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
