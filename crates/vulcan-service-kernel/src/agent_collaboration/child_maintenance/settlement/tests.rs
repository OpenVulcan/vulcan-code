//! Maintenance settlement with actual domain APIs and controlled persistence failures.
//! 使用实际领域 API 和可控持久化失败验证维护结算。

use super::*;
use std::sync::atomic::Ordering;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::PrepareAndStartApiInput;
use vulcan_session_core::v1::event::{ApiRequestSnapshot, ProviderProjectionProof};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;

/// Starts a real maintenance API on `handle`; returns its exact ID without a model transport.
/// 在 `handle` 上启动真实维护 API；不执行模型传输，返回其精确 ID。
fn start_api(handle: &SessionHandle) -> ApiCallId {
    handle.mutate_durable_if_changed(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::MetadataChanged,
        |document| {
            // Actual current events and revisions prove the host's source window.
            // 实际当前事件和修订证明宿主的来源窗口。
            let through_seq = document.events().last().unwrap().seq;
            document.prepare_and_start_api(PrepareAndStartApiInput {
                purpose: ApiPurposeKind::Compaction,
                request: ApiRequestSnapshot::new("provider", "model"),
                request_budget_evidence: crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)?,
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 11,
            }).map(session_change)
        },
    ).unwrap().value.api_call_id
}

/// Failed publication retains admission and retry closes the exact API without changing original business facts.
/// 发布失败保留准入，重试闭合精确 API 且不改变原业务事实。
/// Exercises both pre-API and streaming maintenance, then checks idempotency and stale identity rejection.
/// 验证 API 前和流中维护，再检查幂等性及过期身份拒绝。
#[tokio::test]
async fn maintenance_settlement_retries_and_preserves_business() {
    for streaming in [false, true] {
        // Each case begins with an actual completed business and independent service admission.
        // 每个用例从实际完成业务和独立服务准入开始。
        let (state, handle, source, backend, runtime) = super::super::tests::fixture();
        // Preserve the original profile and result proof for comparison after cleanup.
        // 保留原档案及结果证明，以便清理后比较。
        let profile = source.profile().clone();
        // The same registry owns every lifecycle transition below.
        // 同一注册表拥有下方全部生命周期转换。
        let registry = state.lock().unwrap().session_runtime_registry.clone();
        // This real task exclusion must survive disk errors.
        // 此真实任务互斥必须在磁盘错误后保持。
        let admission = runtime
            .try_reserve_child_context_maintenance(
                handle.session_id(),
                &profile.business_execution_id,
            )
            .unwrap()
            .unwrap();
        // Publish the actual maintenance start before testing terminal behavior.
        // 测试终态行为前发布实际维护起始。
        let execution = prepare(&state, &registry, &handle, admission, source, 10)
            .await
            .unwrap();
        if streaming {
            start_api(&handle);
        }
        // A wrong identity cannot interrupt the current physical request or close its turn.
        // 错误身份不能中断当前物理请求或闭合其回合。
        let before = handle.read(|document| document.memory_revision()).unwrap();
        assert!(
            settle(
                &state,
                &registry,
                &handle,
                &CollaborationExecutionId::try_new("foreign-maintenance").unwrap(),
                TurnClosureReason::Stopped,
                12
            )
            .await
            .is_err()
        );
        assert_eq!(
            handle.read(|document| document.memory_revision()).unwrap(),
            before
        );
        backend.fail.store(true, Ordering::SeqCst);
        assert!(
            settle(
                &state,
                &registry,
                &handle,
                &execution.execution_id,
                TurnClosureReason::Stopped,
                12
            )
            .await
            .is_err()
        );
        assert!(require_owner(&state, handle.session_id(), &execution).is_err());
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_none()
        );
        backend.fail.store(false, Ordering::SeqCst);
        settle(
            &state,
            &registry,
            &handle,
            &execution.execution_id,
            TurnClosureReason::Stopped,
            13,
        )
        .await
        .unwrap();
        handle
            .read(|document| {
                assert!(document.active_api().is_none());
                assert!(document.active_turn().unwrap().closed);
                assert_eq!(
                    document.metadata.child_execution_profile.as_ref(),
                    Some(&profile)
                );
                assert_eq!(
                    document
                        .metadata
                        .context_maintenance
                        .as_ref()
                        .unwrap()
                        .terminal
                        .as_ref()
                        .unwrap()
                        .reason,
                    TurnClosureReason::Stopped
                );
                assert_eq!(document.memory_revision(), document.durable_revision());
                assert_eq!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::NaturallyIdle
                );
            })
            .unwrap();
        assert!(!runtime.is_task_running(handle.session_id()).unwrap());
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_some()
        );
        // A late failure callback sees the same terminal and cannot replace the stop reason.
        // 迟到失败回调看到同一终态，不能替换停止原因。
        let settled = handle.read(|document| document.memory_revision()).unwrap();
        settle(
            &state,
            &registry,
            &handle,
            &execution.execution_id,
            TurnClosureReason::Failed,
            14,
        )
        .await
        .unwrap();
        assert_eq!(
            handle.read(|document| document.memory_revision()).unwrap(),
            settled
        );
    }
}

/// A terminal saved only in memory is published as-is rather than replaced by a later cleanup reason.
/// 仅保存在内存的终态按原样发布，不被后来的清理原因替换。
#[tokio::test]
async fn maintenance_settlement_keeps_existing_terminal() {
    // The controlled backend lets the test separate the domain terminal from its disk confirmation.
    // 可控后端使测试能分离领域终态及其磁盘确认。
    let (state, handle, source, _backend, runtime) = super::super::tests::fixture();
    // Preserve the canonical registry during all publication steps.
    // 所有发布步骤中保留规范注册表。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    // Reserve the completed business independently of the new maintenance turn.
    // 独立于新维护回合预留已完成业务。
    let admission = runtime
        .try_reserve_child_context_maintenance(
            handle.session_id(),
            &source.profile().business_execution_id,
        )
        .unwrap()
        .unwrap();
    // The exact record is produced by the real service admission.
    // 精确记录由真实服务准入产生。
    let execution = prepare(&state, &registry, &handle, admission, source, 10)
        .await
        .unwrap();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.finish_context_maintenance(
                    &execution.execution_id,
                    TurnClosureReason::Failed,
                    11,
                )
            },
        )
        .unwrap();
    settle(
        &state,
        &registry,
        &handle,
        &execution.execution_id,
        TurnClosureReason::Stopped,
        12,
    )
    .await
    .unwrap();
    handle
        .read(|document| {
            assert_eq!(
                document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .unwrap()
                    .terminal
                    .as_ref()
                    .unwrap()
                    .reason,
                TurnClosureReason::Failed
            );
            assert_eq!(document.memory_revision(), document.durable_revision());
        })
        .unwrap();
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
}

/// Cancelling the caller while terminal storage is blocked cannot release its writer or runtime reservation.
/// 终态存储阻塞时取消调用者，不能释放其写入器或运行时预留。
#[tokio::test]
async fn maintenance_settlement_cancelled_waiter_keeps_writer_owned() {
    // The admitted writer will pause in the same persistence backend used by service publication.
    // 已准入写入器将在服务发布使用的同一持久化后端暂停。
    let (state, handle, source, backend, runtime) = super::super::tests::fixture();
    // Keep the exact registry to drain the actual background writer after caller cancellation.
    // 保留精确注册表，在调用者取消后排空实际后台写入器。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    // The guard protects the child until durable terminal release.
    // 守卫保护子任务直到可靠终态释放。
    let admission = runtime
        .try_reserve_child_context_maintenance(
            handle.session_id(),
            &source.profile().business_execution_id,
        )
        .unwrap()
        .unwrap();
    // The domain start has already been saved successfully.
    // 领域起始已经成功保存。
    let execution = prepare(&state, &registry, &handle, admission, source, 10)
        .await
        .unwrap();
    // Separate channels make storage entry and release deterministic, without sleeps.
    // 独立通道使存储进入与释放具有确定性，不使用延时。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Release only this deliberate blocking write.
    // 仅释放此刻意阻塞的写入。
    let (release, blocked) = std::sync::mpsc::channel();
    *backend.gate.lock().unwrap() = Some((entered, blocked));
    // Copies keep exact ownership inside the abortable caller.
    // 副本在可中止调用者中保持精确所有权。
    let call_state = state.clone();
    // This handle is the registered canonical child.
    // 此句柄是已登记规范子任务。
    let call_handle = handle.clone();
    // The background callback must retain this same registry.
    // 后台回调必须保留此同一注册表。
    let call_registry = registry.clone();
    // Abort only the waiter, leaving actual blocking publication accountable.
    // 仅中止等待者，保持实际阻塞发布可追踪。
    let caller = tokio::spawn(async move {
        settle(
            &call_state,
            &call_registry,
            &call_handle,
            &execution.execution_id,
            TurnClosureReason::Stopped,
            12,
        )
        .await
    });
    observed.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    release.send(()).unwrap();
    lifecycle_write::with_canonical_handles(&registry, vec![handle.clone()], || Ok(()))
        .await
        .unwrap();
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
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
