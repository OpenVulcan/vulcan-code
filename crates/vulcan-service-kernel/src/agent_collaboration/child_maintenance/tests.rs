//! Failure-aware canonical maintenance publication and retained exclusion.
//! 感知失败的规范维护发布与保留互斥。

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    TaskRuntime, TestSubagentRegistryEntrySeed,
};
use vulcan_session_core::v1::collaboration::SetRunControlInput;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::{ApiCompletion, FlushCandidate, PrepareAndStartApiInput};
use vulcan_session_core::v1::event::{
    ApiRequestSnapshot, AssistantSemanticStatus, ProviderProjectionProof, TurnClosureReason,
};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::{
    ApiCallStatus, ApiEndReason, ApiTimings, ContinuationEligibility, TurnOutcome,
};
use vulcan_task_session::TaskSessionStatus;
use vulcan_workspace_store::workspace_store::{
    SessionV1FlushResult, SessionV1StorageError, SessionV1StoreError,
};

/// Controlled persistence acknowledges actual candidates or fails the requested publication.
/// 可控持久化确认实际候选，或使指定发布失败。
pub(super) struct MaintenanceBackend {
    /// Explicitly injected disk failure, independent from domain mutation success.
    /// 显式注入的磁盘失败，独立于领域修改成功。
    pub(super) fail: AtomicBool,
    /// One blocked publication proves cancellation retains the actual writer's ownership.
    /// 一次阻塞发布证明取消保留实际写入器的所有权。
    pub(super) gate: Mutex<
        Option<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
}

impl crate::session_runtime_registry_v1::SessionPersistenceBackend for MaintenanceBackend {
    /// Publishes `candidate` metadata or returns the injected storage error without acknowledgment.
    /// 发布 `candidate` 元数据，或返回注入存储错误且不确认。
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
                "maintenance-admission-test",
                "injected publication failure",
            )
            .into());
        }
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Creates an actually completed child domain with a seeded resident mirror and controllable publication.
/// 创建实际完成的子领域，使用播种的常驻镜像与可控发布。
/// Returns service, canonical child, source, backend and runtime; no model or real maintenance dispatch runs.
/// 返回服务、规范子句柄、来源、后端和运行时；不执行模型或真实维护分发。
pub(super) fn fixture() -> (
    SharedServiceBridgeState,
    Arc<SessionHandle>,
    ChildContextMaintenanceSource,
    Arc<MaintenanceBackend>,
    Arc<TaskRuntime>,
) {
    // Reuse actual initial child creation and its captured profile instead of inventing source fields.
    // 复用实际初始子创建及捕获档案，不编造来源字段。
    let (_temporary, parent, mut document, _reservation, profile) =
        startup_recovery::native_execution::tests::reserved_creation_source();
    // Routing matches the fixture's real active request configuration.
    // 路由匹配夹具实际活动请求配置。
    let request = ApiRequestSnapshot::new("provider", "model");
    // The API and assistant terminal supply real completion evidence.
    // API 和助手终态提供真实完成证据。
    let through_seq = document.events().last().unwrap().seq;
    // Prepare through the public host entry, including the actual projection fingerprint and budget evidence.
    // 经公开宿主入口准备，包含实际投影指纹和预算证据。
    let api = document
        .prepare_and_start_api(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::MainGeneration,
            request: request.clone(),
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .unwrap(),
            },
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms: 5,
        })
        .unwrap()
        .into_value()
        .api_call_id;
    document
        .append_assistant_text(
            api,
            "Completed original business",
            AssistantSemanticStatus::Completed,
            6,
        )
        .unwrap();
    document
        .complete_api_group(
            api,
            ApiCompletion {
                request,
                usage: Default::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: 7,
            },
        )
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 5, 8).unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("maintenance-source-idle").unwrap(),
            state: CollaborationRunControl::NaturallyIdle,
            changed_at_ms: 9,
        })
        .unwrap();
    // This backend confirms domain-issued candidate revisions without touching user storage.
    // 此后端确认领域签发候选修订，不接触用户存储。
    let backend = Arc::new(MaintenanceBackend {
        fail: AtomicBool::new(false),
        gate: Mutex::new(None),
    });
    // The exact child identity comes from its captured domain profile.
    // 精确子身份来自其已捕获领域档案。
    let child = Uuid::parse_str(profile.task_id.as_str()).unwrap();
    // The canonical owner is registered exactly once before auxiliary admission.
    // 在辅助准入前恰好登记一次规范所有者。
    let handle = SessionHandle::with_backend(child, document, backend.clone());
    handle.flush_all().unwrap();
    // Capture only after the successful business is fully published.
    // 仅在成功业务完整发布之后捕获。
    let source = handle
        .read(|document| {
            document.capture_child_context_maintenance_source(
                &CollaborationExecutionId::try_new(&profile.business_execution_id).unwrap(),
            )
        })
        .unwrap()
        .unwrap()
        .unwrap();
    // The existing test seed supplies runtime presence; domain publication remains independently real.
    // 既有测试播种提供运行时常驻状态；领域发布仍独立真实执行。
    let runtime = Arc::new(TaskRuntime::new());
    runtime
        .seed_test_subagent_registry_entry(TestSubagentRegistryEntrySeed {
            root_session_id: Uuid::parse_str(&profile.root_session_id).unwrap(),
            parent_session_id: Uuid::parse_str(&profile.parent_session_id).unwrap(),
            task_id: child,
            public_task_id: profile.runtime_identity.public_task_id.clone(),
            display_title: profile.runtime_identity.display_title.clone(),
            description: None,
            subagent: None,
            status: TaskSessionStatus::Completed,
            result_summary: Some("Completed original business".into()),
        })
        .unwrap();
    runtime
        .bind_test_subagent_execution(child, &profile.business_execution_id)
        .unwrap();
    // Register the actual root as well, since it owns the embedded directory across lifecycle transitions.
    // 同时登记实际根，因为它跨生命周期转换持有嵌入目录。
    let service = crate::tests::test_support::test_service_state_with_parent_session(child);
    service.session_runtime_registry.register(parent).unwrap();
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    (
        Arc::new(Mutex::new(service)),
        handle,
        source,
        backend,
        runtime,
    )
}

/// Cancelling an async admission waiter cannot detach a blocked writer from its exclusion guard.
/// 取消异步准入等待者不能使阻塞写入器与其互斥守卫脱离。
/// Returns nothing; lifecycle reacquisition proves publication and ownership transfer finished.
/// 不返回值；重新获取生命周期门证明发布和所有权转交已结束。
#[tokio::test]
async fn maintenance_cancelled_waiter_preserves_publication_owner() {
    // The real canonical writer pauses inside the test persistence backend.
    // 真实规范写入器在测试持久化后端内部暂停。
    let (state, handle, source, backend, runtime) = fixture();
    // Retain this registry to prove the background writer has finished.
    // 保留此注册表以证明后台写入器已完成。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    // Entry and release use separate channels with deterministic ordering.
    // 进入和释放使用独立通道，保持确定顺序。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // This sender releases only the deliberately blocked flush.
    // 此发送端仅释放刻意阻塞的刷写。
    let (release, blocked) = std::sync::mpsc::channel();
    *backend.gate.lock().unwrap() = Some((entered, blocked));
    // Reserve the actual completed child before moving its source into admission.
    // 将来源移入准入之前预留实际已完成子代理。
    let admission = runtime
        .try_reserve_child_context_maintenance(
            handle.session_id(),
            &source.profile().business_execution_id,
        )
        .unwrap()
        .unwrap();
    // Aborting this waiter must not abort or detach its blocking publication owner.
    // 中止此等待者不得中止或脱离其阻塞发布所有者。
    let pending = tokio::spawn({
        // Keep exact service identity inside the asynchronous caller.
        // 在异步调用者内保持精确服务身份。
        let state = state.clone();
        // Publication acquires this canonical registry's lifecycle gate.
        // 发布获取此规范注册表的生命周期门。
        let registry = registry.clone();
        // The same handle is retained by both the caller and the writer.
        // 调用者和写入器保留同一句柄。
        let handle = handle.clone();
        async move { prepare(&state, &registry, &handle, admission, source, 10).await }
    });
    observed.await.unwrap();
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    release.send(()).unwrap();
    // The publication task releases this gate only after retaining service ownership.
    // 发布任务仅在保留服务所有权后释放此门。
    let _lifecycle = registry
        .acquire_lifecycle_transition(handle.session_id())
        .await
        .unwrap();
    // This is the production-created record, despite disappearance of the original waiter.
    // 即使原等待者消失，此记录仍由生产代码创建。
    let execution = state.lock().unwrap().child_maintenance_owners[&handle.session_id()]
        .execution
        .clone();
    require_owner(&state, handle.session_id(), &execution).unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .finish_child_context_maintenance(
                        &execution.execution_id,
                        TurnClosureReason::Stopped,
                        11,
                    )
                    .map(session_change)
            },
        )
        .unwrap();
    release_owner(&state, &handle, &execution.execution_id).unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_some()
    );
}

/// Shutdown rejects new maintenance before domain mutation and releases the unused reservation.
/// 关闭过程在领域修改前拒绝新维护，并释放未使用预留。
/// Returns nothing; the original durable child and service owner table stay unchanged.
/// 不返回值；原可靠子文档及服务所有者表保持不变。
#[tokio::test]
async fn maintenance_shutdown_rejection_preserves_business() {
    // Capture valid success before the actual handle shuts down admission.
    // 在实际句柄关闭准入之前捕获有效成功。
    let (state, handle, source, _backend, runtime) = fixture();
    // The registered lifecycle owner remains canonical throughout shutdown.
    // 关闭期间已登记生命周期所有者始终保持规范。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    // The original revision must remain unchanged when admission rejects new inference.
    // 准入拒绝新推理时原修订必须保持不变。
    let revision = handle.read(|document| document.memory_revision()).unwrap();
    // Runtime exclusion alone cannot authorize a request after host shutdown begins.
    // 宿主开始关闭后，仅运行时互斥不能授权请求。
    let admission = runtime
        .try_reserve_child_context_maintenance(
            handle.session_id(),
            &source.profile().business_execution_id,
        )
        .unwrap()
        .unwrap();
    handle.begin_shutdown().unwrap();
    assert!(
        prepare(&state, &registry, &handle, admission, source, 10)
            .await
            .is_err()
    );
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        revision
    );
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_some()
    );
}

/// Both success and failed flush retain exclusion until this exact maintenance terminal is durable.
/// 成功和刷写失败均保持互斥，直到本精确维护终态可靠发布。
/// Returns nothing; verifies dispatch gating, stale release rejection and original business completion.
/// 不返回值；验证分发门控、过期释放拒绝和原业务完成。
#[tokio::test]
async fn maintenance_admission_retains_owner_until_durable_terminal() {
    for publication_fails in [false, true] {
        // Start from the same real success source with independently controlled disk behavior.
        // 从相同真实成功来源开始，独立控制磁盘行为。
        let (state, handle, source, backend, runtime) = fixture();
        // Production lifecycle gates use this exact registry.
        // 生产生命周期门使用此精确注册表。
        let registry = state.lock().unwrap().session_runtime_registry.clone();
        // Retained repair keeps storage exclusion even without any registered worker task.
        // 即使没有已登记工作任务，保留修复仍保持存储互斥。
        let scope = MaintenanceStorageScope::capture(&handle, &registry).unwrap();
        // Actual process admission excludes both business and question starts.
        // 实际进程准入排除业务及答疑启动。
        let admission = runtime
            .try_reserve_child_context_maintenance(
                handle.session_id(),
                &source.profile().business_execution_id,
            )
            .unwrap()
            .unwrap();
        backend.fail.store(publication_fails, Ordering::SeqCst);
        // Capture the public preparation result, not a manufactured domain execution.
        // 捕获公开准备结果，不制造领域执行。
        let prepared = prepare(&state, &registry, &handle, admission, source, 10).await;
        assert_eq!(prepared.is_err(), publication_fails, "{prepared:?}");
        assert!(state.lock().unwrap().child_maintenance_tasks.is_empty());
        assert!(
            crate::project_conversation_services::begin_session_lifecycle_transition(
                &state,
                [scope.root_session_id],
                "archive",
            )
            .is_err()
        );
        assert!(
            crate::project_conversation_services::begin_project_lifecycle_transition(
                &state,
                &scope.project_id,
                [],
                "remove",
            )
            .is_err()
        );
        // Failed writes still transfer the exact Core receipt into retained service ownership.
        // 写入失败仍将精确 Core 回执转交保留服务所有权。
        let execution = state.lock().unwrap().child_maintenance_owners[&handle.session_id()]
            .execution
            .clone();
        assert_eq!(
            require_owner(&state, handle.session_id(), &execution).is_err(),
            publication_fails
        );
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_none()
        );
        assert!(!runtime.is_task_running(handle.session_id()).unwrap());
        assert!(release_owner(&state, &handle, &execution.execution_id).is_err());
        assert!(
            release_owner(
                &state,
                &handle,
                &CollaborationExecutionId::try_new("stale").unwrap()
            )
            .is_err()
        );
        // Closing only in memory is insufficient to release the reservation.
        // 仅在内存关闭不足以释放预留。
        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.finish_child_context_maintenance(
                        &execution.execution_id,
                        TurnClosureReason::Failed,
                        11,
                    )
                },
            )
            .unwrap();
        assert!(release_owner(&state, &handle, &execution.execution_id).is_err());
        backend.fail.store(false, Ordering::SeqCst);
        handle.flush_all().unwrap();
        release_owner(&state, &handle, &execution.execution_id).unwrap();
        drop(
            crate::project_conversation_services::begin_session_lifecycle_transition(
                &state,
                [scope.root_session_id],
                "archive",
            )
            .unwrap(),
        );
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_some()
        );
        assert!(!runtime.is_task_running(handle.session_id()).unwrap());
    }
}
