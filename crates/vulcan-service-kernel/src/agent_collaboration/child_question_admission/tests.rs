//! Failure and cancellation evidence for the real child admission writer.
//! 真实子准入写入器的失败与取消证据。

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
use vulcan_workspace_store::workspace_store::{
    SessionV1FlushResult, SessionV1StorageError, SessionV1StoreError,
};

/// Controls one deterministic in-flight flush and subsequent storage failures.
/// 控制一次确定的进行中刷写及后续存储失败。
struct AdmissionBackend {
    /// Whether every publication currently fails.
    /// 当前是否让每次发布失败。
    fail: AtomicBool,
    /// Optional next-write rendezvous, consumed exactly once by the blocking writer.
    /// 下一次写入的可选会合点，由阻塞写入器恰好消费一次。
    gate: Mutex<
        Option<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
}

impl crate::session_runtime_registry_v1::SessionPersistenceBackend for AdmissionBackend {
    /// Publishes `candidate`, or blocks/fails at the configured deterministic write boundary.
    /// 发布 `candidate`，或在配置的确定写入边界阻塞及失败。
    /// Returns exact candidate metadata on success and an injected storage error on failure.
    /// 成功返回精确候选元数据，失败返回注入存储错误。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        if let Some((entered, release)) = self.gate.lock().unwrap().take() {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(SessionV1StorageError::io(
                "child-admission-test",
                "injected child admission write failure",
            )
            .into());
        }
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Builds an accepted child question with real canonical registration and a controllable store.
/// 创建已接纳子问题，使用真实规范登记及可控存储。
/// Returns isolated service, handle, source question, backend and task runtime; no execution has begun.
/// 返回隔离服务、句柄、来源问题、后端和任务运行时；尚未开始执行。
fn fixture() -> (
    SharedServiceBridgeState,
    Arc<SessionHandle>,
    CollaborationQuestionRecord,
    Arc<AdmissionBackend>,
    Arc<TaskRuntime>,
) {
    // Each test owns distinct child and root identities.
    // 每个测试拥有独立子身份与根身份。
    let session_id = Uuid::new_v4();
    let mut document = SessionDocument::new(session_id.to_string(), "project", "Child", 1);
    let question = CollaborationQuestionRecord {
        operation_id: CollaborationOperationId::try_new("source-operation").unwrap(),
        question_id: CollaborationQuestionId::try_new("question").unwrap(),
        delivery_id: CollaborationDeliveryId::try_new("delivery").unwrap(),
        source: CollaborationParticipant::new("source", "source", "project", 1, None, None)
            .unwrap(),
        target: CollaborationParticipant::new(
            session_id.to_string(),
            Uuid::new_v4().to_string(),
            "project",
            1,
            None,
            Some(SubagentTaskId::try_new(session_id.to_string()).unwrap()),
        )
        .unwrap(),
        body: "Report your current progress.".into(),
        execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
        state: CollaborationQuestionState::Pending,
        answer: None,
        cancellation: None,
        created_at_ms: 2,
    };
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            question_id: question.question_id.clone(),
            delivery_id: question.delivery_id.clone(),
            source: question.source.clone(),
            target: question.target.clone(),
            bound_turn_id: None,
            execution_purpose: question.execution_purpose,
            accepted_at_ms: 3,
        })
        .unwrap();
    // Publish acceptance before enabling any fault injection.
    // 启用故障注入之前发布接纳。
    let backend = Arc::new(AdmissionBackend {
        fail: AtomicBool::new(false),
        gate: Mutex::new(None),
    });
    let handle = SessionHandle::with_backend(session_id, document, backend.clone());
    handle.flush_all().unwrap();
    let service = crate::tests::test_support::test_service_state_with_parent_session(session_id);
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    (
        Arc::new(Mutex::new(service)),
        handle,
        question,
        backend,
        Arc::new(TaskRuntime::new()),
    )
}

/// A failed first write plus failed cleanup cannot reopen business preparation; recovery can release it.
/// 首次写入及清理失败不能重新开放业务准备；恢复可以释放互斥。
#[tokio::test]
async fn child_question_admission_failed_publication_retains_exclusion() {
    let (state, handle, question, backend, runtime) = fixture();
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        prepare_child_question_execution(&state, &registry, &handle, admission, question, 4)
            .await
            .is_err()
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .lock()
            .unwrap()
            .child_question_execution_owners
            .contains_key(&handle.session_id())
    );
    handle
        .read(|document| {
            assert_eq!(document.collaboration_state().question_executions.len(), 1);
            assert!(document.memory_revision() > document.durable_revision());
        })
        .unwrap();
    backend.fail.store(false, Ordering::SeqCst);
    crate::conversation_run_services::stop_owned_child_question_execution(
        &crate::ServiceEventEmitter::new(Vec::new()),
        &state,
        handle.session_id(),
    )
    .await
    .unwrap();
    assert!(
        state
            .lock()
            .unwrap()
            .child_question_execution_owners
            .is_empty()
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_some()
    );
    handle
        .read(|document| assert_eq!(document.memory_revision(), document.durable_revision()))
        .unwrap();
}

/// Cancelling the publication waiter cannot release the guard while its blocking write is still running.
/// 取消发布等待者不能在阻塞写入仍运行时释放守卫。
#[tokio::test]
async fn child_question_admission_cancelled_waiter_retains_writer_ownership() {
    let (state, handle, question, backend, runtime) = fixture();
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    // A real write rendezvous establishes the dangerous memory-committed, not-yet-durable interval.
    // 真实写入会合点建立内存已提交而尚未可靠发布的危险区间。
    let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    *backend.gate.lock().unwrap() = Some((entered_sender, release_receiver));
    backend.fail.store(true, Ordering::SeqCst);
    let state_for_waiter = state.clone();
    let registry_for_waiter = registry.clone();
    let handle_for_waiter = handle.clone();
    let waiter = tokio::spawn(async move {
        prepare_child_question_execution(
            &state_for_waiter,
            &registry_for_waiter,
            &handle_for_waiter,
            admission,
            question,
            4,
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), entered_receiver)
        .await
        .unwrap()
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    release_sender.send(()).unwrap();
    // Lifecycle reacquisition waits for the actual writer to finish transferring its guard.
    // 重新取得生命周期锁会等待实际写入器完成守卫转交。
    let lifecycle = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        registry.acquire_lifecycle_transition(handle.session_id()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        state
            .lock()
            .unwrap()
            .child_question_execution_owners
            .contains_key(&handle.session_id())
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    drop(lifecycle);
    backend.fail.store(false, Ordering::SeqCst);
    crate::conversation_run_services::stop_owned_child_question_execution(
        &crate::ServiceEventEmitter::new(Vec::new()),
        &state,
        handle.session_id(),
    )
    .await
    .unwrap();
    assert!(
        state
            .lock()
            .unwrap()
            .child_question_execution_owners
            .is_empty()
    );
}

/// Successful admission enables dispatch; a stopped target fails before creating any execution.
/// 成功准入允许分发；停止目标在创建任何执行之前失败。
#[tokio::test]
async fn child_question_admission_success_and_stopped_rejection_preserve_ownership() {
    for stopped in [false, true] {
        // Each branch gets a new accepted question and canonical task reservation.
        // 每个分支取得新的已接纳问题及规范任务预留。
        let (state, handle, question, _backend, runtime) = fixture();
        let registry = state.lock().unwrap().session_runtime_registry.clone();
        if stopped {
            handle
                .set_collaboration_run_control_durable(
                    vulcan_session_core::v1::collaboration::SetRunControlInput {
                        operation_id: CollaborationOperationId::try_new("stop-before-admission")
                            .unwrap(),
                        state: CollaborationRunControl::Stopped,
                        changed_at_ms: 4,
                    },
                )
                .unwrap();
        }
        let before = handle.read(|document| document.memory_revision()).unwrap();
        let admission = runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .unwrap();
        let result =
            prepare_child_question_execution(&state, &registry, &handle, admission, question, 5)
                .await;
        if stopped {
            assert!(result.is_err());
            handle
                .read(|document| {
                    assert_eq!(document.memory_revision(), before);
                    assert!(
                        document
                            .collaboration_state()
                            .question_executions
                            .is_empty()
                    );
                })
                .unwrap();
            assert!(
                state
                    .lock()
                    .unwrap()
                    .child_question_execution_owners
                    .is_empty()
            );
        } else {
            // Only actual reliable publication permits the production stream ownership check.
            // 只有实际可靠发布才允许通过生产流所有权检查。
            let execution = result.unwrap();
            child_execution_ownership::require_child_question_execution_owner(
                &state,
                handle.session_id(),
                &execution.execution_id,
            )
            .unwrap();
            assert!(
                runtime
                    .try_reserve_child_question_execution(handle.session_id())
                    .unwrap()
                    .is_none()
            );
            crate::conversation_run_services::stop_owned_child_question_execution(
                &crate::ServiceEventEmitter::new(Vec::new()),
                &state,
                handle.session_id(),
            )
            .await
            .unwrap();
        }
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_some()
        );
        assert!(!runtime.is_task_running(handle.session_id()).unwrap());
    }
}
