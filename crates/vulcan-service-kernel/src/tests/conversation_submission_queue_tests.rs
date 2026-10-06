use super::*;
use crate::conversation_submission::{
    ExistingSubmissionQueueDecision, ExistingSubmissionQueueInput,
    commit_existing_submission_v1_queue, emit_queued_prompt_run_phase_delta,
    evaluate_existing_submission_queue, queue_existing_submission,
};
use crate::session_runtime_registry_v1::{SessionHandle, SessionPersistenceBackend};
use std::sync::atomic::{AtomicUsize, Ordering};
use vulcan_session_core::v1::document::FlushCandidate;
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, QueuedInputIdentity, ResourceId, SessionDocument,
    TurnOrigin,
};
use vulcan_workspace_store::workspace_store::{
    SessionV1FlushResult, SessionV1StorageError, SessionV1StoreError,
};

/// In-memory backend that counts successful V1 queue flushes.
/// 统计成功 V1 队列刷新的内存后端。
#[derive(Default)]
struct QueueRecordingBackend {
    /// Number of complete candidates accepted as durable.
    /// 被接纳为可靠状态的完整候选数量。
    flush_count: AtomicUsize,
}

impl SessionPersistenceBackend for QueueRecordingBackend {
    /// Accepts one complete candidate and publishes its exact metadata.
    /// 接纳一份完整候选并发布其精确 metadata。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        self.flush_count.fetch_add(1, Ordering::SeqCst);
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Backend that rejects every V1 queue flush deterministically.
/// 确定性拒绝每次 V1 队列刷新的后端。
struct QueueFailingBackend;

impl SessionPersistenceBackend for QueueFailingBackend {
    /// Returns one injected storage failure without publishing the candidate.
    /// 返回一条注入的存储失败且不发布候选。
    fn flush(
        &self,
        _candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        Err(SessionV1StorageError::io(
            std::path::PathBuf::from("queue-v1-test"),
            "injected queue flush failure",
        )
        .into())
    }
}

/// Builds one isolated V1 queue handle with an explicit persistence backend.
/// 使用显式持久化后端构建一个隔离的 V1 队列句柄。
fn queue_v1_test_handle(
    session_id: SessionId,
    backend: Arc<dyn SessionPersistenceBackend>,
) -> Arc<SessionHandle> {
    queue_v1_test_handle_from_document(
        session_id,
        SessionDocument::new(
            session_id.to_string(),
            "project-queue-v1",
            "queue V1 test",
            1,
        ),
        backend,
    )
}

/// Builds one isolated V1 queue handle from an explicitly prepared document.
/// 使用一份显式准备的文档构建隔离的 V1 队列句柄。
///
/// # Parameters
/// 参数
/// `session_id` identifies the handle, `document` supplies its exact durable facts, and
/// `backend` records or rejects complete flush candidates.
/// `session_id` 标识句柄；`document` 提供精确可靠事实；`backend` 记录或拒绝完整刷新候选。
///
/// # Returns
/// 返回值
/// Returns the isolated session owner used by queue service tests.
/// 返回队列服务测试使用的隔离会话 owner。
fn queue_v1_test_handle_from_document(
    session_id: SessionId,
    document: SessionDocument,
    backend: Arc<dyn SessionPersistenceBackend>,
) -> Arc<SessionHandle> {
    SessionHandle::with_backend(session_id, document, backend)
}

/// Verifies an idle existing session continues immediately instead of entering the visible queue.
/// 验证空闲已有会话会立即继续，而不是进入可见队列。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if idle retained state is treated as blocking work.
/// 此测试无返回值；若空闲保留状态被视为阻塞工作则失败。
#[test]
fn idle_existing_session_submission_starts_immediately() {
    // session_id identifies the idle retained existing conversation.
    // session_id 标识空闲的保留已有会话。
    let session_id = SessionId::new_v4();
    // service_state owns the idle kernel registry and retained workspace.
    // service_state 持有空闲内核注册表与保留工作区。
    let service_state = test_service_state_with_parent_session(session_id);
    // durable_handle is the exact V1 document authority with an empty durable queue.
    // durable_handle 是可靠队列为空的精确 V1 文档权威。
    let durable_handle =
        queue_v1_test_handle(session_id, Arc::new(QueueRecordingBackend::default()));
    // workspace_snapshot is the exact hydrated base observed by the queue decision.
    // workspace_snapshot 是排队决策观察的精确已水合基础。
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    // state exposes the same retained revision used to build the workspace input.
    // state 提供构建工作区输入时使用的同一保留版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // provider_configuration supplies provider-specific pending-lane behavior.
    assert_eq!(
        evaluate_existing_submission_queue(
            &state,
            &workspace_snapshot,
            Some(session_id),
            Some(&durable_handle),
        )
        .expect("idle queue decision"),
        ExistingSubmissionQueueDecision::StartImmediately
    );
}

/// Verifies an active kernel turn routes a new submission into the exact existing-session queue.
/// 验证活动内核回合会将新提交路由到精确已有会话队列。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if active kernel ownership is ignored.
/// 此测试无返回值；若活动内核所有权被忽略则失败。
#[test]
fn active_kernel_turn_routes_submission_to_existing_session_queue() {
    // session_id identifies the retained conversation with active system work.
    // session_id 标识存在活动系统工作的保留会话。
    let session_id = SessionId::new_v4();
    // service_state owns the active kernel turn and retained workspace.
    // service_state 持有活动内核回合与保留工作区。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    // durable_handle is the exact V1 document authority with an empty durable queue.
    // durable_handle 是可靠队列为空的精确 V1 文档权威。
    let durable_handle =
        queue_v1_test_handle(session_id, Arc::new(QueueRecordingBackend::default()));
    // workspace_snapshot is the hydrated base consumed by shared pending-lane rules.
    // workspace_snapshot 是共享待发送通道规则消费的已水合基础。
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    // state exposes the active kernel revision to the decision.
    // state 向决策提供活动内核版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // provider_configuration supplies provider-specific pending-lane behavior.
    assert_eq!(
        evaluate_existing_submission_queue(
            &state,
            &workspace_snapshot,
            Some(session_id),
            Some(&durable_handle),
        )
        .expect("active queue decision"),
        ExistingSubmissionQueueDecision::Queue { session_id }
    );
}

/// Verifies an active stream owner blocks a second submission even without a kernel marker.
/// 验证即使没有内核标记，活动流所有者也会阻止第二次提交。
#[tokio::test(flavor = "current_thread")]
async fn active_stream_owner_routes_submission_to_existing_session_queue() {
    // session_id identifies the retained conversation whose provider stream is active.
    // session_id 标识 Provider 流正在运行的保留会话。
    let session_id = SessionId::new_v4();
    // service_state starts without a kernel turn so the stream-only race is covered.
    // service_state 不启动内核回合，以覆盖仅活动流存在的竞态。
    let service_state = test_service_state_with_parent_session(session_id);
    // durable_handle is the exact V1 document authority with an empty durable queue.
    // durable_handle 是可靠队列为空的精确 V1 文档权威。
    let durable_handle =
        queue_v1_test_handle(session_id, Arc::new(QueueRecordingBackend::default()));
    // workspace_snapshot is the exact retained state observed by queue admission.
    // workspace_snapshot 是排队 admission 观察到的精确保留状态。
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    // state retains a pending stream owner whose future cannot complete by itself.
    // state 保留一个自身不会完成的待处理活动流所有者。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    let task_id = Uuid::new_v4();
    spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async {
            std::future::pending::<()>().await;
        },
    )
    .expect("active stream owner should register");

    assert_eq!(
        evaluate_existing_submission_queue(
            &state,
            &workspace_snapshot,
            Some(session_id),
            Some(&durable_handle),
        )
        .expect("stream-owner queue decision"),
        ExistingSubmissionQueueDecision::Queue { session_id }
    );

    // Cleanup proves the test leaves no asynchronous owner behind.
    // 清理步骤确保测试不会遗留异步所有者。
    let removed = state
        .lock()
        .expect("service state should remain lockable")
        .active_stream_tasks
        .get(&session_id)
        .map(|entry| entry.task_id());
    assert_eq!(removed, Some(task_id));
    state
        .lock()
        .expect("service state should remain lockable")
        .active_stream_tasks
        .get(&session_id)
        .expect("active stream entry")
        .abort();
    state
        .lock()
        .expect("service state should remain lockable")
        .active_stream_tasks
        .remove(&session_id);
}

/// Verifies a queued commit preserves workspace state that advanced after submission preflight.
/// 验证排队提交会保留提交预检后继续推进的工作区状态。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a stale snapshot replaces the latest retained revision.
/// 此测试无返回值；若过期快照替换最新保留版本则失败。
/// Verifies queued acknowledgement projection rereads the phase after the queue transaction releases its lock.
/// 验证排队确认投影会在排队事务释放锁后重新读取阶段。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a stale captured phase is projected.
/// 此测试无返回值；若投影了过期捕获阶段则失败。
#[test]
fn queued_projection_uses_latest_phase() {
    // session_id identifies the conversation whose queued prompt was already dispatched.
    // session_id 标识排队提示词已经开始分发的会话。
    let session_id = SessionId::new_v4();
    // service_state advances to Preparing after the original queue commit released its lock.
    // service_state 模拟原始排队提交释放锁后推进到 Preparing。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    // state exposes the advanced authoritative phase.
    // state 暴露已推进的权威阶段。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app discards the emitted delta because retained state is the assertion target.
    // app 丢弃已发送增量，因为断言目标是保留状态。
    let app = ServiceEventEmitter::new(Vec::new());

    let (_revision, run_phase) = emit_queued_prompt_run_phase_delta(&app, &state, session_id)
        .expect("latest queued projection should commit");

    assert_eq!(run_phase, SessionRunPhase::Preparing);
}

/// Verifies V1 queue admission preserves attachment order and deduplicates one client action.
/// 验证 V1 队列准入保留附件顺序，并对同一客户端动作执行去重。
#[test]
fn v1_queue_commit_preserves_resource_order_and_is_idempotent() {
    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    let ordered_resource_ids = vec![
        ResourceId::try_new(9).expect("first ordered resource"),
        ResourceId::try_new(3).expect("second ordered resource"),
    ];
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queue-action-stable").expect("client action"),
        "queued body",
        ordered_resource_ids.clone(),
    );

    let committed = commit_existing_submission_v1_queue(&handle, &submission, 10)
        .expect("first V1 queue admission");
    let repeated = commit_existing_submission_v1_queue(&handle, &submission, 11)
        .expect("idempotent V1 queue admission");

    assert!(committed.changed);
    assert!(!repeated.changed);
    assert_eq!(committed.value, repeated.value);
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    let persisted_order = handle
        .read(|document| {
            document.metadata.input_state.queued_inputs[0]
                .resource_ids
                .clone()
        })
        .expect("read queued resources");
    assert_eq!(persisted_order, ordered_resource_ids);
}

/// Verifies a durable V1 queue occupancy forces queue admission even when no process mirror exists.
/// 验证即使不存在进程镜像，可靠的 V1 队列占用也会强制进入排队准入。
///
/// A missing or broken compatibility mirror can never make a non-empty durable queue look empty,
/// so a later submission still queues behind the exact durable FIFO order.
/// 缺失或损坏的兼容镜像绝不能让非空可靠队列看起来为空，因此后续提交仍排在可靠的 FIFO 顺序之后。
#[test]
fn durable_v1_queue_occupancy_queues_submission_without_process_mirror() {
    // session_id identifies the conversation whose durable queue already owns one input.
    // session_id 标识可靠队列已持有一条输入的会话。
    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    // submission is durably admitted before the decision under test runs.
    // submission 在被测决策运行前已可靠准入。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("durable-queue-occupancy-action").expect("client action"),
        "durably queued prompt",
        Vec::new(),
    );
    commit_existing_submission_v1_queue(&handle, &submission, 10)
        .expect("durable V1 queue admission should commit");
    // service_state intentionally retains no process mirror for this session.
    // service_state 有意不为该会话保留任何进程镜像。
    let service_state = test_service_state_with_parent_session(session_id);
    assert!(service_state.pending_prompt_process_mirrors.is_empty());
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    assert_eq!(
        evaluate_existing_submission_queue(
            &state,
            &workspace_snapshot,
            Some(session_id),
            Some(&handle),
        )
        .expect("durable queue decision"),
        ExistingSubmissionQueueDecision::Queue { session_id }
    );
    assert_eq!(
        handle
            .read(|document| document.metadata.input_state.queued_inputs.len())
            .expect("durable queue should remain readable"),
        1
    );
}

/// Verifies an existing session submission without its exact V1 document authority fails closed.
/// 验证缺少精确 V1 文档权威的已有会话提交会失败关闭而不是放行即时准入。
#[test]
fn existing_session_submission_without_v1_authority_fails_closed() {
    // session_id identifies the existing conversation targeted by the submission.
    // session_id 标识提交目标的已有会话。
    let session_id = SessionId::new_v4();
    let service_state = test_service_state_with_parent_session(session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    let error =
        evaluate_existing_submission_queue(&state, &workspace_snapshot, Some(session_id), None)
            .expect_err("a missing durable authority must not unlock immediate admission");

    assert!(error.contains("V1 document authority"));
}

/// Verifies a repeated committed browser action cannot duplicate its process mirror or fabricate a Hook turn.
/// 验证重复的已提交浏览器动作无法重复创建进程镜像，也不能伪造 Hook 回合。
#[test]
fn queue_service_idempotent_retry_does_not_duplicate_process_mirror_or_hook() {
    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queue-service-idempotent-action")
            .expect("client action should be valid"),
        "queued once",
        Vec::new(),
    );
    let service_state = test_service_state_with_parent_session(session_id);
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    let agent_runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let queue_input = || ExistingSubmissionQueueInput {
        v1_handle: Arc::clone(&handle),
        v1_submission: submission.clone(),
        session_id,
        workspace_path: String::from("D:/project-a"),
        hidden_model_guidance: None,
        attachments: Vec::new(),
        attachment_draft_scope: None,
        reasoning_effort: None,
        model_selection: None,
        submit_total_started_at: current_unix_millis_value(),
    };

    queue_existing_submission(&app, &state, &agent_runtime, queue_input())
        .expect("first queue submission should commit");
    // Queued notifications only record into an existing business Hook turn; they never create one.
    // 排队通知只记录到已有业务 Hook 回合中，绝不创建新回合。
    assert_eq!(
        agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable after first dispatch")
            .active_turn_id(session_id),
        None
    );
    queue_existing_submission(&app, &state, &agent_runtime, queue_input())
        .expect("idempotent retry should return the existing receipt");

    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    let guard = state.lock().expect("service state should remain readable");
    let mirror = guard
        .pending_prompt_process_mirrors
        .get(&session_id)
        .expect("committed queue should have one process mirror");
    assert_eq!(mirror.ui_state.pending_prompt_panel_queue.len(), 1);
    assert_eq!(
        mirror.ui_state.pending_prompt_panel_queue[0].id,
        "queue-service-idempotent-action"
    );
    drop(guard);
    let hook_runtime = agent_runtime
        .conversation_hook_runtime
        .lock()
        .expect("Hook runtime should remain readable");
    assert_eq!(
        hook_runtime.active_turn_id(session_id),
        None,
        "an idempotent retry must not fabricate a Hook turn"
    );
}

/// A duplicate durable admission repairs a queue lease that failed after the first commit.
/// 重复的可靠准入会修复首次提交后失败的队列租约转交。
#[test]
fn idempotent_queue_retry_repairs_failed_asset_handoff() {
    use vulcan_agent_runtime::attachment_asset_index::{
        acquire_staged_attachment_ref, load_attachment_asset_record_for_attachment,
    };
    use vulcan_agent_runtime::attachment_assets::{
        attachment_descriptor_from_mime_type, stage_attachment_bytes,
    };

    let workspace = tempfile::tempdir().expect("isolated attachment workspace");
    let wrong_workspace = tempfile::tempdir().expect("isolated missing-index workspace");
    let workspace_path = workspace.path().to_string_lossy().into_owned();
    let descriptor =
        attachment_descriptor_from_mime_type("image/png").expect("image attachment descriptor");
    let attachment = stage_attachment_bytes(
        &workspace_path,
        &descriptor,
        "retry.png",
        None,
        &[1, 2, 3, 4],
    )
    .expect("managed asset staging");
    let draft_scope = "composer-draft:queue-retry";
    acquire_staged_attachment_ref(
        &workspace_path,
        draft_scope,
        std::slice::from_ref(&attachment),
    )
    .expect("composer owns the staged asset");

    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queue-asset-retry-action").expect("valid action"),
        "queued with an asset",
        Vec::new(),
    );
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    let agent_runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let queue_input = |path: String| ExistingSubmissionQueueInput {
        v1_handle: Arc::clone(&handle),
        v1_submission: submission.clone(),
        session_id,
        workspace_path: path,
        hidden_model_guidance: None,
        attachments: vec![attachment.clone()],
        attachment_draft_scope: Some(String::from(draft_scope)),
        reasoning_effort: None,
        model_selection: None,
        submit_total_started_at: current_unix_millis_value(),
    };

    queue_existing_submission(
        &app,
        &state,
        &agent_runtime,
        queue_input(wrong_workspace.path().to_string_lossy().into_owned()),
    )
    .expect("V1 queue remains committed despite first asset-index failure");
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        load_attachment_asset_record_for_attachment(&attachment)
            .expect("composer must retain asset after failed handoff")
            .staging_refs,
        vec![String::from(draft_scope)]
    );

    let acknowledgement =
        queue_existing_submission(&app, &state, &agent_runtime, queue_input(workspace_path))
            .expect("idempotent retry should repair the asset handoff");
    assert!(acknowledgement.queued);
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        load_attachment_asset_record_for_attachment(&attachment)
            .expect("repaired queue must own asset")
            .staging_refs,
        vec![format!(
            "pending-prompt:v2:{session_id}:queue-asset-retry-action"
        )]
    );
}

/// Verifies retries of moved or removed admissions never recreate process-only queue state.
/// 验证已移动或已删除准入的重试绝不会重建进程内队列状态。
#[test]
fn resolved_v1_admission_retry_does_not_rebuild_process_mirror_or_hook() {
    for moved_to_draft in [true, false] {
        // session_id isolates each terminal queue resolution.
        // session_id 隔离每一种终态队列处置。
        let session_id = SessionId::new_v4();
        // submission is the exact historical browser admission retried after resolution.
        // submission 是处置完成后重试的精确历史浏览器准入。
        let submission = ClientInputSubmission::new(
            ClientActionId::try_new(format!("resolved-queue-action-{moved_to_draft}"))
                .expect("client action should be valid"),
            "already resolved",
            Vec::new(),
        );
        // document prepares the complete durable history before the service process starts.
        // document 在服务进程启动前准备完整可靠历史。
        let mut document = SessionDocument::new(
            session_id.to_string(),
            "project-queue-v1",
            "resolved queue V1 test",
            1,
        );
        let admission = document
            .enqueue_input(submission.clone(), 2)
            .expect("historical admission should commit")
            .into_value();
        let identity = QueuedInputIdentity {
            client_action_id: admission.client_action_id,
            input_id: admission.input_id,
        };
        if moved_to_draft {
            document
                .move_queued_input_to_draft(identity, 3)
                .expect("historical admission should move to draft");
        } else {
            document
                .remove_queued_input(identity, 3)
                .expect("historical admission should be removed");
        }
        let backend = Arc::new(QueueRecordingBackend::default());
        let handle = queue_v1_test_handle_from_document(session_id, document, backend.clone());
        let state: SharedServiceBridgeState = Arc::new(Mutex::new(
            test_service_state_with_parent_session(session_id),
        ));
        let agent_runtime = test_agent_runtime_bridge();
        let app = ServiceEventEmitter::new(Vec::new());

        let acknowledgement = queue_existing_submission(
            &app,
            &state,
            &agent_runtime,
            ExistingSubmissionQueueInput {
                v1_handle: Arc::clone(&handle),
                v1_submission: submission,
                session_id,
                workspace_path: String::from("D:/project-a"),
                hidden_model_guidance: None,
                attachments: Vec::new(),
                attachment_draft_scope: None,
                reasoning_effort: None,
                model_selection: None,
                submit_total_started_at: current_unix_millis_value(),
            },
        )
        .expect("resolved admission retry should remain idempotent");

        assert!(!acknowledgement.queued);
        assert!(acknowledgement.turn_id.is_none());
        assert_eq!(
            handle
                .read(|document| document.metadata.input_state.queued_inputs.len())
                .expect("resolved V1 queue should remain readable"),
            0
        );
        assert!(
            !state
                .lock()
                .expect("service state should remain readable")
                .pending_prompt_process_mirrors
                .contains_key(&session_id)
        );
        assert!(
            !agent_runtime
                .conversation_hook_runtime
                .lock()
                .expect("Hook runtime should remain readable")
                .has_active_turn(session_id)
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }
}

/// Verifies a user admission that supersedes an undispatched Goal is not projected as queued.
/// 验证取代尚未派发 Goal 的用户准入不会被投影为排队状态。
#[test]
fn superseded_goal_admission_returns_scheduled_turn_without_queue_mirror() {
    // session_id identifies the Goal session whose unopened work is superseded.
    // session_id 标识尚未派发工作被取代的 Goal 会话。
    let session_id = SessionId::new_v4();
    // document starts with one durable, undispatched GoalContinuation Turn.
    // document 以一个可靠且尚未派发的 GoalContinuation 回合作为初始状态。
    let mut document = SessionDocument::new(
        session_id.to_string(),
        "project-queue-v1",
        "superseded Goal queue test",
        1,
    );
    document
        .create_goal("finish the goal", None, "Continue the active goal.", 2)
        .expect("Goal continuation should open");
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle_from_document(session_id, document, backend.clone());
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("goal-superseding-user-action")
            .expect("client action should be valid"),
        "user takes priority",
        Vec::new(),
    );
    let client_action_id = submission.client_action_id.clone();
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    let agent_runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());

    let acknowledgement = queue_existing_submission(
        &app,
        &state,
        &agent_runtime,
        ExistingSubmissionQueueInput {
            v1_handle: Arc::clone(&handle),
            v1_submission: submission,
            session_id,
            workspace_path: String::from("D:/project-a"),
            hidden_model_guidance: None,
            attachments: Vec::new(),
            attachment_draft_scope: None,
            reasoning_effort: None,
            model_selection: None,
            submit_total_started_at: current_unix_millis_value(),
        },
    )
    .expect("Goal-superseding admission should be durably accepted");

    let active_turn = handle
        .read(|document| document.active_turn().cloned())
        .expect("scheduled UserRoot Turn should remain readable")
        .expect("scheduled UserRoot Turn should remain open");
    let dispatch_authority =
        queued_dispatch_authority_for_client_action(handle.as_ref(), &client_action_id)
            .expect("scheduled UserRoot Turn should resolve by its durable browser action");
    // expected_turn_id keeps the exact durable numeric identity alive for the RPC assertion.
    // expected_turn_id 为 RPC 断言保留精确可靠数字身份的所有权。
    let expected_turn_id = active_turn.turn_id.value().to_string();
    assert!(!acknowledgement.queued);
    assert_eq!(
        acknowledgement.turn_id.as_deref(),
        Some(expected_turn_id.as_str())
    );
    assert_eq!(active_turn.origin, TurnOrigin::UserRoot);
    assert_eq!(dispatch_authority.turn_id, active_turn.turn_id);
    assert_eq!(dispatch_authority.client_action_id, client_action_id);
    assert_eq!(
        handle
            .read(|document| document.metadata.input_state.queued_inputs.len())
            .expect("V1 queue should remain readable"),
        0
    );
    assert!(
        !state
            .lock()
            .expect("service state should remain readable")
            .pending_prompt_process_mirrors
            .contains_key(&session_id)
    );
    assert!(
        !agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable")
            .has_active_turn(session_id)
    );
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
}

/// Verifies process-mirror rejection after a durable V1 commit cannot revoke the queued input.
/// 验证可靠 V1 提交后的进程镜像拒绝无法撤销排队输入。
#[test]
fn queue_service_process_mirror_failure_does_not_revoke_v1_commit() {
    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queue-service-mirror-failure")
            .expect("client action should be valid"),
        "durable before mirror",
        Vec::new(),
    );
    let mut service_state = test_service_state_with_parent_session(session_id);
    let process_mirror = service_state
        .pending_prompt_process_mirrors
        .entry(session_id)
        .or_default();
    for index in 0..vulcan_workbench_model::PENDING_PROMPT_QUEUE_LIMIT {
        process_mirror.ui_state.pending_prompt_panel_queue.push(
            vulcan_workbench_model::PendingPromptPanelEntry {
                id: format!("existing-row-{index}"),
                prompt: format!("existing prompt {index}"),
                hidden_model_guidance: None,
                preview: format!("existing prompt {index}"),
                attachments: Vec::new(),
                model_reference: None,
                runtime_options: Default::default(),
                state: vulcan_workbench_model::PendingPromptPanelEntryState::Ready,
                created_at: index.to_string(),
            },
        );
    }
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    let agent_runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());

    queue_existing_submission(
        &app,
        &state,
        &agent_runtime,
        ExistingSubmissionQueueInput {
            v1_handle: Arc::clone(&handle),
            v1_submission: submission,
            session_id,
            workspace_path: String::from("D:/project-a"),
            hidden_model_guidance: None,
            attachments: Vec::new(),
            attachment_draft_scope: None,
            reasoning_effort: None,
            model_selection: None,
            submit_total_started_at: current_unix_millis_value(),
        },
    )
    .expect("mirror failure should not reject the durable queue acknowledgement");

    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    let queued_count = handle
        .read(|document| document.metadata.input_state.queued_inputs.len())
        .expect("durable V1 queue should remain readable");
    assert_eq!(queued_count, 1);
    let guard = state.lock().expect("service state should remain readable");
    assert_eq!(
        guard
            .pending_prompt_process_mirrors
            .get(&session_id)
            .expect("pre-existing process mirror should remain")
            .ui_state
            .pending_prompt_panel_queue
            .len(),
        vulcan_workbench_model::PENDING_PROMPT_QUEUE_LIMIT
    );
    drop(guard);
    assert!(
        !agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable")
            .has_active_turn(session_id)
    );
}

/// Verifies a post-commit Hook failure leaves both V1 authority and the process mirror intact.
/// 验证提交后的 Hook 失败会完整保留 V1 权威与进程镜像。
#[test]
fn queue_service_hook_failure_does_not_revoke_v1_or_process_mirror() {
    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queue-service-hook-failure")
            .expect("client action should be valid"),
        "Hook failure follows V1 commit",
        Vec::new(),
    );
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    let agent_runtime = test_agent_runtime_bridge();
    let poisoned_hook_runtime = Arc::clone(&agent_runtime.conversation_hook_runtime);
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = poisoned_hook_runtime
            .lock()
            .expect("Hook runtime should lock before deliberate poison");
        panic!("poison queue Hook runtime");
    }));
    assert!(poison_result.is_err());
    let app = ServiceEventEmitter::new(Vec::new());

    queue_existing_submission(
        &app,
        &state,
        &agent_runtime,
        ExistingSubmissionQueueInput {
            v1_handle: Arc::clone(&handle),
            v1_submission: submission,
            session_id,
            workspace_path: String::from("D:/project-a"),
            hidden_model_guidance: None,
            attachments: Vec::new(),
            attachment_draft_scope: None,
            reasoning_effort: None,
            model_selection: None,
            submit_total_started_at: current_unix_millis_value(),
        },
    )
    .expect("Hook failure should not reject the durable queue acknowledgement");

    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        handle
            .read(|document| document.metadata.input_state.queued_inputs.len())
            .expect("durable V1 queue should remain readable"),
        1
    );
    let guard = state.lock().expect("service state should remain readable");
    assert_eq!(
        guard
            .pending_prompt_process_mirrors
            .get(&session_id)
            .expect("process mirror should remain after Hook failure")
            .ui_state
            .pending_prompt_panel_queue
            .len(),
        1
    );
}

/// Verifies reusing one client action with different content fails without a second queue row.
/// 验证同一客户端动作携带不同内容时显式失败，且不产生第二条队列记录。
#[test]
fn v1_queue_commit_rejects_client_action_payload_drift() {
    let session_id = SessionId::new_v4();
    let backend = Arc::new(QueueRecordingBackend::default());
    let handle = queue_v1_test_handle(session_id, backend.clone());
    let client_action_id = ClientActionId::try_new("queue-action-conflict").expect("client action");
    let original = ClientInputSubmission::new(client_action_id.clone(), "first body", Vec::new());
    let drifted = ClientInputSubmission::new(client_action_id, "changed body", Vec::new());

    commit_existing_submission_v1_queue(&handle, &original, 20)
        .expect("original V1 queue admission");
    let error = commit_existing_submission_v1_queue(&handle, &drifted, 21)
        .expect_err("payload drift must fail");

    assert!(error.contains("client action"));
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    let queued_count = handle
        .read(|document| document.metadata.input_state.queued_inputs.len())
        .expect("read queued count");
    assert_eq!(queued_count, 1);
}

/// Verifies a V1 flush failure returns before any legacy queue projection can be mutated.
/// 验证 V1 刷新失败会在任何旧队列投影可被修改前返回。
#[test]
fn v1_queue_flush_failure_never_mutates_legacy_queue_projection() {
    let session_id = SessionId::new_v4();
    let handle = queue_v1_test_handle(session_id, Arc::new(QueueFailingBackend));
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queue-action-flush-failure").expect("client action"),
        "durability first",
        Vec::new(),
    );
    let service_state = test_service_state_with_parent_session(session_id);
    let before_panel_ids = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .session_runtime_state
        .get(&session_id)
        .map(|runtime| {
            runtime
                .ui_state
                .pending_prompt_panel_queue
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let result = commit_existing_submission_v1_queue(&handle, &submission, 30);

    assert!(result.is_err());
    let after_panel_ids = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .session_runtime_state
        .get(&session_id)
        .map(|runtime| {
            runtime
                .ui_state
                .pending_prompt_panel_queue
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(after_panel_ids, before_panel_ids);
}
