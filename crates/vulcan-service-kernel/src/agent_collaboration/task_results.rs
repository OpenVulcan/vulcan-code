//! Durable result delivery back to the exact original task requester.
//! 向精确原任务请求者可靠投递结果。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationTaskMessageEffectKind, CollaborationTaskMessageKind, CollaborationTaskResult,
    CollaborationTaskResultAcceptance,
};

/// Projects a locally durable `acceptance` under the requester's canonical lifecycle gate.
/// 在请求者规范生命周期锁内投影本地可靠 `acceptance`。
/// `invocation` supplies the registry; `requester` owns the original grant and source turn.
/// `invocation` 提供注册表；`requester` 拥有原授权和来源回合。
/// Returns without insertion if that turn ended since collection; authorization errors remain explicit.
/// 收集后原回合已结束则不插入并返回；授权错误保持明确。
pub(super) async fn project_task_result(
    invocation: &ValidatedInvocation,
    requester: &Arc<SessionHandle>,
    acceptance: CollaborationTaskResultAcceptance,
    stamp: i64,
) -> Result<(), String> {
    let requester_for_write = Arc::clone(requester);
    lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![Arc::clone(requester)],
        move || {
            let eligible = requester_for_write
                .read(|document| {
                    if document
                        .collaboration_state()
                        .received_task_results
                        .get(&acceptance.result.effect.delivery_id)
                        != Some(&acceptance)
                        || document.durable_revision() < acceptance.receiver_revision
                    {
                        return Err(String::from(
                            "task result projection requires the exact durable local acceptance",
                        ));
                    }
                    Ok(document.active_turn().is_some_and(|turn| {
                        !turn.closed
                            && Some(turn.turn_id)
                                == acceptance.result.acceptance.message.source.turn_id
                    }) && document.collaboration_run_control()
                        == CollaborationRunControl::Running
                        && !document
                            .collaboration_state()
                            .task_result_projections
                            .contains_key(&acceptance.result.effect.delivery_id))
                })
                .map_err(|error| error.to_string())??;
            if !eligible {
                return Ok(());
            }
            let message = &acceptance.result.acceptance.message;
            // Local accepted content is immutable authority; fresh model disclosure still consumes the original grant.
            // 本地已接纳正文是不可变权威；新的模型内容交付仍消费原授权。
            if acceptance.result.terminal.is_some()
                && message.source.project_id != message.target.project_id
            {
                let executor_id = Uuid::parse_str(&message.target.session_id)
                    .map_err(|error| error.to_string())?;
                let action = match message.kind {
                    CollaborationTaskMessageKind::TaskStart => CollaborationGrantAction::Start,
                    CollaborationTaskMessageKind::TaskSteer => CollaborationGrantAction::Steer,
                };
                if !source_handle_grant_allows(
                    &requester_for_write,
                    requester_for_write.session_id(),
                    executor_id,
                    action,
                )? {
                    return Err(String::from(
                        "task result model projection is denied after authorization revocation",
                    ));
                }
            }
            requester_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        document
                            .project_task_result_before_api(
                                acceptance.result.effect.delivery_id,
                                stamp,
                            )
                            .map(session_change)
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .await
}

/// Delivers unacknowledged results from the registered executor `session_id` using `state`.
/// 使用 `state` 投递已登记执行者 `session_id` 尚未确认的结果。
/// Returns the delivered count, or an explicit error after attempting all independent pending results.
/// 返回送达数量；尝试所有独立待处理结果后，若有失败则返回明确错误。
pub(crate) async fn reconcile_task_results_for_session(
    state: &SharedServiceBridgeState,
    session_id: Uuid,
) -> Result<usize, String> {
    let (runtime, registry) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        (
            guard.controller_runtime.clone(),
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let executor = registry
        .get(session_id)
        .map_err(|error| error.to_string())?;
    // A failed effect is a target-owned fact, but needs its own durable result before transfer.
    // 失败效果是目标所有事实，但在回传前仍需要独立的可靠结果。
    let unrecorded_failures = executor
        .read(|document| {
            let collaboration = document.collaboration_state();
            collaboration
                .task_message_effects
                .iter()
                .filter(|(delivery_id, effect)| {
                    matches!(effect.outcome, CollaborationTaskMessageEffectKind::Failed { .. })
                        && !collaboration.task_results.contains_key(*delivery_id)
                })
                .map(|(delivery_id, _)| delivery_id.clone())
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    let mut failed = 0;
    for delivery_id in unrecorded_failures {
        let stamp = now_ms()?;
        if let Err(error) = mutate_handle_with_admission(
            &registry,
            &executor,
            session_id,
            SessionMutationAdmission::MetadataOnly,
            move |document| document.record_failed_task_result(delivery_id, stamp),
        )
        .await
        {
            failed += 1;
            tracing::warn!(%session_id, %error, "Task delivery failure result recording remains pending");
        }
    }
    let pending = executor
        .read(|document| {
            let collaboration = document.collaboration_state();
            collaboration
                .task_results
                .iter()
                .filter(|(delivery_id, _)| {
                    !collaboration
                        .task_result_acknowledgements
                        .contains_key(*delivery_id)
                })
                .map(|(_, result)| result.clone())
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    if pending.is_empty() {
        return if failed == 0 {
            Ok(0)
        } else {
            Err(format!("{failed} task delivery failures remain unrecorded"))
        };
    }
    let invocation = recovery_invocation(
        Arc::clone(state),
        runtime.ok_or_else(|| {
            String::from("controller runtime is unavailable for task result delivery")
        })?,
        registry,
        session_id,
        Arc::clone(&executor),
    )?;
    let mut delivered = 0;
    for result in pending {
        let delivery_id = result.effect.delivery_id.clone();
        match deliver_task_result(&invocation, &executor, result).await {
            Ok(()) => delivered += 1,
            Err(error) => {
                failed += 1;
                tracing::warn!(session_id = %session_id, delivery_id = delivery_id.as_str(), %error, "Task result delivery remains pending");
            }
        }
    }
    if failed > 0 {
        return Err(format!(
            "{delivered} task results delivered; {failed} results remain pending"
        ));
    }
    Ok(delivered)
}

/// Publishes `result` at its exact requester and then confirms the immutable receiver receipt.
/// 在精确请求者发布 `result`，然后确认不可变接收回执。
/// `invocation` supplies recovery ownership and `executor` is the result authority; no model is woken.
/// `invocation` 提供恢复归属，`executor` 是结果权威；不唤醒模型。
/// Returns only after both publications, leaving either durable side available for retry on failure.
/// 双方发布完成后返回；失败时保留任一已提交侧供重试。
async fn deliver_task_result(
    invocation: &ValidatedInvocation,
    executor: &Arc<SessionHandle>,
    result: CollaborationTaskResult,
) -> Result<(), String> {
    let requester =
        participants::load_participant_handle(invocation, &result.acceptance.message.source)
            .await?;
    let requester_for_write = Arc::clone(&requester);
    let executor_for_write = Arc::clone(executor);
    lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![requester, Arc::clone(executor)],
        move || {
            let published = executor_for_write
                .read(|document| {
                    document.durable_revision() >= result.target_revision
                        && document
                            .collaboration_state()
                            .task_results
                            .get(&result.effect.delivery_id)
                            == Some(&result)
                })
                .map_err(|error| error.to_string())?;
            if !published {
                return Err(String::from(
                    "task result is not the executor's durable authoritative snapshot",
                ));
            }
            // Already accepted content only needs internal acknowledgement repair; it is not a new disclosure.
            // 已接纳正文只需修复内部确认，不构成新的内容交付。
            let received = requester_for_write
                .read(|document| {
                    document
                        .collaboration_state()
                        .received_task_results
                        .get(&result.effect.delivery_id)
                        .cloned()
                })
                .map_err(|error| error.to_string())?;
            if result.terminal.is_some()
                && received.is_none()
                && result.acceptance.message.source.project_id
                    != result.acceptance.message.target.project_id
            {
                let action = match result.acceptance.message.kind {
                    CollaborationTaskMessageKind::TaskStart => CollaborationGrantAction::Start,
                    CollaborationTaskMessageKind::TaskSteer => CollaborationGrantAction::Steer,
                };
                if !source_handle_grant_allows(
                    &requester_for_write,
                    requester_for_write.session_id(),
                    executor_for_write.session_id(),
                    action,
                )? {
                    return Err(String::from(
                        "task result authorization was revoked before requester acceptance",
                    ));
                }
            }
            let stamp = now_ms()?;
            // UUID plus target-local result sequence is bounded and collision-free in the receiver's operation log.
            // UUID 加目标本地结果序号在接收操作日志中长度有界且身份唯一。
            let stem = format!(
                "task-result-delivery:{}:{}",
                executor_for_write.session_id(),
                result.event_seq.value()
            );
            let repair_operation = CollaborationOperationId::try_new(format!("{stem}:intent"))
                .map_err(|error| error.to_string())?;
            let accept_operation = CollaborationOperationId::try_new(format!("{stem}:receive"))
                .map_err(|error| error.to_string())?;
            let result_for_accept = result.clone();
            let receipt = requester_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        if document
                            .collaboration_state()
                            .received_task_results
                            .contains_key(&result_for_accept.effect.delivery_id)
                        {
                            return document
                                .accept_task_result(accept_operation, result_for_accept, stamp)
                                .map(session_change);
                        }
                        document.apply_atomic_transition(move |document| {
                            document.acknowledge_task_message(
                                repair_operation,
                                result_for_accept.acceptance.clone(),
                                stamp,
                            )?;
                            document
                                .accept_task_result(accept_operation, result_for_accept, stamp)
                                .map(session_change)
                        })
                    },
                )
                .map_err(|error| error.to_string())?;
            let acceptance = receipt.value;
            let receiver_published = requester_for_write
                .read(|document| {
                    document.durable_revision() >= acceptance.receiver_revision
                        && document
                            .collaboration_state()
                            .received_task_results
                            .get(&acceptance.result.effect.delivery_id)
                            == Some(&acceptance)
                })
                .map_err(|error| error.to_string())?;
            if !receiver_published {
                return Err(String::from(
                    "task result requester receipt is not durably published",
                ));
            }
            let ack_operation = CollaborationOperationId::try_new(format!(
                "task-result-delivered:{}",
                result.event_seq.value()
            ))
            .map_err(|error| error.to_string())?;
            executor_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        document
                            .acknowledge_task_result(ack_operation, acceptance, stamp)
                            .map(session_change)
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .await
}
