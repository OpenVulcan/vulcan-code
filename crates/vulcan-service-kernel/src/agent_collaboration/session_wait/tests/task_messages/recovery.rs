//! Task outbox repair against actual source and target journals.
//! 基于真实来源与目标日志的任务出站修复。

use super::*;
use vulcan_session_core::v1::collaboration::CollaborationTaskMessage;

mod embedded_tree;
mod main_business;

/// A failed indexed pass retains its worker and retries the actual outbox after the index becomes available.
/// 索引扫描失败后保留任务，并在索引恢复可用后重试真实出站箱。
/// Duplicate startup cannot claim another worker; success releases its registry reservation.
/// 重复启动不能认领第二个任务；成功后释放注册表预留。
#[tokio::test]
async fn session_collaboration_startup_worker_retries_real_outbox() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    // Keep the target physically active: this test exercises outbox retry, not cold business dispatch.
    // 保持目标物理活动：本测试验证出站重试，而非冷业务分发。
    start_api(&target);
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "retry-worker",
    )
    .await;
    // Temporarily withhold the published index, reproducing an actual service availability error.
    // 临时撤下已发布索引，复现真实服务可用性错误。
    let workspace = invocation.state.lock().unwrap().workspace_snapshot.take();
    let app = crate::ServiceEventEmitter::new(vec![]);
    let worker = startup_recovery::worker::schedule(&invocation.state, app.clone())
        .unwrap()
        .unwrap();
    assert!(
        startup_recovery::worker::schedule(&invocation.state, app)
            .unwrap()
            .is_none()
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if invocation
                .state
                .lock()
                .unwrap()
                .startup
                .recent_logs
                .iter()
                .any(|line| {
                    line.contains("Collaboration recovery requires a published workspace index")
                })
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_task_messages
                .is_empty())
            .unwrap()
    );
    invocation.state.lock().unwrap().workspace_snapshot = workspace;
    tokio::time::timeout(std::time::Duration::from_secs(8), worker)
        .await
        .unwrap()
        .unwrap();
    // Read both real journals so a resident-only acknowledgement cannot satisfy recovery.
    // 读取双方真实日志，防止仅驻留内存的确认满足恢复断言。
    let source = load_session_v1_for_runtime(&invocation.source_locator)
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let acceptance = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(acceptance)
    );
    assert!(
        invocation
            .registry
            .try_reserve_collaboration_recovery()
            .unwrap()
            .is_some()
    );
}

/// Shutdown suspends scans, cancellation releases ownership, and reopening admission allows real repair.
/// 关闭暂停扫描，取消释放所有权，重新开放准入后允许真实修复。
/// Uses the registry shutdown fence rather than fabricating a completed worker or canonical receipt.
/// 使用注册表关闭栅栏，不伪造已完成任务或规范回执。
#[tokio::test]
async fn session_collaboration_startup_worker_obeys_shutdown_and_cancellation() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    // A live physical owner leaves only delivery work for this lifecycle test.
    // 活动物理所有者使此生命周期测试仅保留投递待办。
    start_api(&target);
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "shutdown-worker",
    )
    .await;
    invocation.registry.begin_shutdown().unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    let cancelled = startup_recovery::worker::schedule(&invocation.state, app.clone())
        .unwrap()
        .unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    let resumed = startup_recovery::worker::schedule(&invocation.state, app)
        .unwrap()
        .unwrap();
    // Hold the shutdown fence over the worker's initial attempt, then reopen the exact same registry.
    // 在任务初次尝试期间保持关闭栅栏，再开放精确同一注册表。
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!resumed.is_finished());
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_task_messages
                .is_empty())
            .unwrap()
    );
    invocation.registry.resume_after_failed_shutdown().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), resumed)
        .await
        .unwrap()
        .unwrap();
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_task_messages
                .contains_key(&message.delivery_id))
            .unwrap()
    );
    assert!(
        invocation
            .registry
            .try_reserve_collaboration_recovery()
            .unwrap()
            .is_some()
    );
}

/// An indexed startup pass leaves unrelated cold histories unloaded.
/// 索引启动扫描保持无关冷历史未加载。
#[tokio::test]
async fn session_collaboration_startup_skips_empty_cold_sessions() {
    // Discard every canonical handle to reproduce a process with only an indexed workspace.
    // 丢弃所有规范句柄，复现只有索引工作区的进程。
    let (_root, invocation, target) = fixture(false).await;
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [invocation.source_session_id, target.session_id()]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap();
    assert!(registry.get(invocation.source_session_id).is_err());
    assert!(registry.get(target.session_id()).is_err());
}

/// Cold indexed repair reloads persisted intents and isolates an unavailable index entry.
/// 冷索引修复重载持久意图，并隔离不可用的索引条目。
#[tokio::test]
async fn session_collaboration_startup_repairs_cold_outbox_despite_unavailable_peer() {
    // The new registry cannot use either original live handle as a recovery shortcut.
    // 新注册表不能将任何原活动句柄作为恢复捷径。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "startup",
    )
    .await;
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    let missing = Uuid::new_v4();
    let failure = startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [missing, invocation.source_session_id]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap_err();
    assert!(failure.contains(&missing.to_string()));
    let source = load_session_v1_for_runtime(&invocation.source_locator)
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let accepted = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(accepted)
    );
    assert_eq!(accepted.message, message);
    assert!(
        receiver
            .collaboration_state()
            .task_message_effects
            .is_empty()
    );
    assert!(!Arc::ptr_eq(
        &registry.get(invocation.source_session_id).unwrap(),
        &invocation.source_handle
    ));
}

/// An unpublished receiver receipt cannot become a durable source acknowledgement.
/// 尚未发布的接收方回执不能成为可靠来源确认。
#[tokio::test]
async fn session_task_outbox_recovery_rejects_unpublished_acceptance() {
    // Keep both real stores alive while simulating a receiver with an unflushed acceptance.
    // 模拟接收方尚未刷写接纳期间保留双方真实存储。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "unpublished",
    )
    .await;
    target
        .mutate_in_memory_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_task_message(
                        CollaborationOperationId::try_new("unpublished-acceptance").unwrap(),
                        message.clone(),
                        6,
                    )
                    .map(session_change)
            },
        )
        .unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap_err()
        .contains("not durably published")
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .is_none()
    );
    let before = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        !before
            .collaboration_state()
            .inbound_task_messages
            .contains_key(&message.delivery_id)
    );
    target.flush_all().unwrap();
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap(),
        1
    );
}

/// Publishes only the source intent to reproduce a crash before target delivery.
/// 仅发布来源意图，复现目标投递前崩溃。
/// `label` distinguishes operations; `kind` controls whether the original target turn is frozen.
/// `label` 区分操作；`kind` 决定是否冻结原目标回合。
/// Returns the exact stored message without accepting or scheduling it.
/// 返回精确存储消息，不接纳或调度它。
async fn record_pending_task(
    invocation: &ValidatedInvocation,
    target: &Arc<SessionHandle>,
    kind: CollaborationTaskMessageKind,
    label: &str,
) -> CollaborationTaskMessage {
    // Participants come from the same canonical resolver as normal dispatch.
    // 参与者来自与正常分发相同的规范解析器。
    let source = source_participant(invocation).await.unwrap();
    let target_turn = match kind {
        CollaborationTaskMessageKind::TaskStart => None,
        CollaborationTaskMessageKind::TaskSteer => Some(
            target
                .read(|document| document.active_turn().unwrap().turn_id.value())
                .unwrap(),
        ),
    };
    let receiver =
        participants::participant_for_handle(invocation, target.session_id(), target, target_turn)
            .await
            .unwrap();
    let message = CollaborationTaskMessage {
        delivery_id: CollaborationDeliveryId::try_new(format!("recover-delivery-{label}")).unwrap(),
        operation_id: CollaborationOperationId::try_new(format!("recover-source-{label}")).unwrap(),
        source,
        target: receiver,
        kind,
        body: format!("Durable task {label}"),
    };
    invocation
        .source_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .send_task_message(message.clone(), 5)
                    .map(session_change)
            },
        )
        .unwrap();
    message
}

/// Repairs after the original source API ends, and repeated repair changes neither journal.
/// 原来源 API 结束后仍可修复，重复修复不改变双方日志。
#[tokio::test]
async fn session_task_outbox_recovery_preserves_original_intent_after_source_api() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "original",
    )
    .await;
    invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| document.interrupt_active_api(6),
        )
        .unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap(),
        1
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let acceptance = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(acceptance.message, message);
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(acceptance)
    );
    assert!(
        receiver
            .collaboration_state()
            .task_message_effects
            .is_empty()
    );
    let source_revision = source.durable_revision();
    let target_revision = receiver.durable_revision();
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        invocation
            .source_handle
            .read(|document| document.durable_revision())
            .unwrap(),
        source_revision
    );
    assert_eq!(
        target.read(|document| document.durable_revision()).unwrap(),
        target_revision
    );
}

/// One obsolete steering delivery cannot block another queued intent or bind a replacement task.
/// 一条过期方向投递不能阻止另一条排队意图，也不能绑定替代任务。
#[tokio::test]
async fn session_task_outbox_recovery_isolates_obsolete_turns() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let obsolete = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "a-obsolete",
    )
    .await;
    let queued = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskStart,
        "b-queued",
    )
    .await;
    target
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.close_turn(vulcan_session_core::v1::event::TurnOutcome::Completed, 0, 6)
            },
        )
        .unwrap();
    let successor = target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("recovery-successor").unwrap(),
                "Unrelated work",
                vec![],
            ),
            7,
        )
        .unwrap()
        .value;
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap_err()
        .contains("target_turn")
    );
    target
        .read(|document| {
            assert_eq!(document.active_turn().unwrap().turn_id, successor);
            assert!(
                !document
                    .collaboration_state()
                    .inbound_task_messages
                    .contains_key(&obsolete.delivery_id)
            );
            assert_eq!(
                document.collaboration_state().inbound_task_messages[&queued.delivery_id].message,
                queued
            );
            assert!(
                document
                    .collaboration_state()
                    .task_message_effects
                    .is_empty()
            );
        })
        .unwrap();
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        source.collaboration_state().outbound_task_messages[&obsolete.delivery_id]
            .acceptance
            .is_none()
    );
    assert!(
        source.collaboration_state().outbound_task_messages[&queued.delivery_id]
            .acceptance
            .is_some()
    );
}

/// Revocation blocks first delivery but permits copying a preexisting acceptance without new input.
/// 撤销授权阻止首次投递，但允许复制已有接纳且不新增输入。
#[tokio::test]
async fn session_task_outbox_recovery_separates_new_delivery_from_receipt_repair() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Steer].into_iter().collect(),
    )
    .await
    .unwrap();
    let accepted = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "accepted",
    )
    .await;
    let pending = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "pending",
    )
    .await;
    let original = target
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_task_message(
                        CollaborationOperationId::try_new("original-acceptance").unwrap(),
                        accepted.clone(),
                        6,
                    )
                    .map(session_change)
            },
        )
        .unwrap()
        .value;
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    let target_revision = target.read(|document| document.durable_revision()).unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap_err()
        .contains("authorization was revoked")
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&accepted.delivery_id]
            .acceptance
            .as_ref(),
        Some(&original)
    );
    assert!(
        source.collaboration_state().outbound_task_messages[&pending.delivery_id]
            .acceptance
            .is_none()
    );
    assert_eq!(
        target.read(|document| document.durable_revision()).unwrap(),
        target_revision
    );
}
