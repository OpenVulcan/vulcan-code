//! Durable one-way notification admission and delivery.
//! 可靠单向通知准入与投递。

use super::*;
mod stopped;
pub(super) use stopped::reconcile_stopped_notifications_for_session;
use vulcan_session_core::v1::collaboration::{
    AcceptNotificationInput, CollaborationNotificationId, CollaborationNotificationRecord,
    CollaborationNotificationState, CollaborationOperationSemantic,
    RecordNotificationProgressInput, SendNotificationInput,
};

/// Records a notification before attempting delivery; returns its durable identity on uncertainty.
/// 尝试投递前记录通知；结果不确定时仍返回其可靠身份。
/// The invocation owns sender authority; target and body are validated tool arguments.
/// 调用上下文拥有发送者权威；目标和正文是经过校验的工具参数。
pub(super) async fn send_notification(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    body: String,
) -> Result<Value, String> {
    // Notify is independently authorized so read or steering grants cannot send unsolicited data.
    // Notify 独立授权，避免读取或方向引导授权被用于发送未获授权的数据。
    let target = resolve_authorized_target(
        invocation,
        target_session_id,
        CollaborationGrantAction::Notify,
    )
    .await?;
    participants::require_message_admission(invocation, target_session_id, &target).await?;
    let source = source_participant(invocation).await?;
    let target_turn_id = target
        .handle
        .read(|document| {
            document
                .active_turn()
                .filter(|turn| !turn.closed)
                .map(|turn| turn.turn_id)
        })
        .map_err(|error| format!("failed to read notification target turn: {error}"))?;
    let target_identity =
        target_participant(invocation, target_session_id, &target, target_turn_id).await?;
    let operation_id = collaboration_operation_id(invocation)?;
    let notification_id =
        CollaborationNotificationId::try_new(format!("cn1/{}", invocation.operation_id))
            .map_err(|error| format!("invalid notification identity: {error}"))?;
    let delivery_id = collaboration_delivery_id(invocation)?;
    let cross_project = source.project_id != target_identity.project_id;
    let source_session_id = invocation.source_session_id;
    let input = SendNotificationInput {
        operation_id: operation_id.clone(),
        notification_id: notification_id.clone(),
        delivery_id: delivery_id.clone(),
        source: source.clone(),
        target: target_identity.clone(),
        body: body.clone(),
        created_at_ms: now_ms()?,
    };
    mutate_source(invocation, move |document| {
        if cross_project
            && !document_grant_allows(
                document,
                source_session_id,
                target_session_id,
                CollaborationGrantAction::Notify,
            )
        {
            return Err(
                vulcan_session_core::v1::errors::DomainError::CollaborationUnauthorized {
                    action: "send_notification_grant",
                    identity: target_session_id.to_string(),
                },
            );
        }
        document.send_notification(input)
    })
    .await?;

    // Never turn a post-commit transport failure into an unlocatable failed send.
    // 提交后的传输失败绝不能让已记录发送失去可查询身份。
    let delivery_result = deliver_recorded_notification(
        invocation,
        target_session_id,
        &target,
        delivery_id.clone(),
        notification_id.clone(),
        operation_id.clone(),
        source,
        target_identity,
        body,
    )
    .await;
    let (notification, revision) = invocation
        .source_handle
        .read(|document| {
            (
                document
                    .collaboration_state()
                    .notifications
                    .get(&notification_id)
                    .cloned(),
                document.memory_revision(),
            )
        })
        .map_err(|error| format!("failed to read notification receipt: {error}"))?;
    let notification = notification
        .ok_or_else(|| String::from("recorded notification is missing from its source"))?;
    Ok(json!({
        "operation_id": invocation.operation_id,
        "notification_id": notification_id.as_str(),
        "delivery_id": delivery_id.as_str(),
        "revision": revision,
        "status": notification.state,
        "notification": notification,
        "delivery_error": delivery_result.err(),
    }))
}

/// Accepts one already recorded notification on its target and advances the source to accepted.
/// 在目标上接纳一条已记录通知，并将来源推进到已接纳。
///
/// # Notes
/// 说明
/// This is the exact transaction `send_notification` performs after it has durably recorded the
/// source half, so it is also the transaction a later recovery pass must repeat. Reusing one
/// implementation keeps both callers on the same identities and the same admission rules.
/// 这正是 `send_notification` 在可靠记录来源一半之后执行的事务，因此也是后续恢复必须重放的事务。
/// 复用同一实现可让两个调用方保持同一身份与同一准入规则。
///
/// # Authorization
/// 授权
/// Both the cross-project grant and the target's document admission are re-evaluated here, so a
/// revocation between the two halves is honored instead of being silently bypassed by recovery.
/// 跨项目授权与目标文档准入都在此处重新判定，因此两半之间的授权撤销会被如实执行，而不会被恢复
/// 路径静默绕过。
// Delivery identity, participants and settlement evidence remain explicit at the cross-session boundary.
// 跨会话边界显式保留投递身份、参与方及结算证据。
#[expect(
    clippy::too_many_arguments,
    reason = "Delivery identity, participants and settlement evidence remain explicit at the cross-session boundary."
)]
pub(super) async fn deliver_recorded_notification(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    target: &ResolvedSession,
    delivery_id: vulcan_session_core::v1::collaboration::CollaborationDeliveryId,
    notification_id: CollaborationNotificationId,
    operation_id: vulcan_session_core::v1::collaboration::CollaborationOperationId,
    source: vulcan_session_core::v1::collaboration::CollaborationParticipant,
    target_identity: vulcan_session_core::v1::collaboration::CollaborationParticipant,
    body: String,
) -> Result<(), String> {
    let cross_project = source.project_id != target_identity.project_id;
    // Recovery handles many notifications in one invocation; each receipt belongs to its source operation.
    // 一次恢复调用会处理多条通知；每份回执必须归属于自身来源操作。
    let accept_operation =
        CollaborationOperationId::try_new(format!("{}:notification-accept", operation_id.as_str()))
            .map_err(|error| format!("invalid notification acceptance identity: {error}"))?;
    // The source acknowledgement shares the same original intent across live delivery and recovery.
    // 实时投递与恢复的来源确认共享同一原始意图身份。
    let progress_operation = CollaborationOperationId::try_new(format!(
        "{}:notification-accepted",
        operation_id.as_str()
    ))
    .map_err(|error| format!("invalid notification progress identity: {error}"))?;
    let input = AcceptNotificationInput {
        operation_id: accept_operation,
        notification_id: notification_id.clone(),
        delivery_id: delivery_id.clone(),
        source_operation_id: operation_id,
        source,
        target: target_identity,
        body,
        accepted_at_ms: now_ms()?,
    };
    let source_for_write = Arc::clone(&invocation.source_handle);
    let target_for_write = Arc::clone(&target.handle);
    let source_id = invocation.source_session_id;
    // A pre-accept rejection has its own durable identity and never fabricates target coordinates.
    // 接纳前拒收拥有独立可靠身份，绝不伪造目标坐标。
    let rejection_operation = CollaborationOperationId::try_new(format!(
        "{}:notification-rejected",
        input.source_operation_id.as_str()
    ))
    .map_err(|error| format!("invalid notification rejection identity: {error}"))?;
    let accepted = lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![Arc::clone(&source_for_write), Arc::clone(&target_for_write)],
        move || {
            // Publish any staged original intent or failure before returning or creating a remote effect.
            // 返回或产生远端效果前，先发布暂存的原始意图或失败。
            source_for_write
                .flush_all()
                .map_err(|error| error.to_string())?;
            // Re-read the source under both gates so a stale retry cannot resurrect a failed intent.
            // 在双方门内重读来源，避免旧重试复活已经失败的意图。
            let recorded = source_for_write
                .read(|document| {
                    document
                        .collaboration_state()
                        .notifications
                        .get(&input.notification_id)
                        .cloned()
                })
                .map_err(|error| error.to_string())?
                .ok_or_else(|| String::from("notification source intent is missing"))?;
            if recorded.delivery_id != input.delivery_id
                || recorded.operation_id != input.source_operation_id
                || recorded.source != input.source
                || recorded.target != input.target
                || recorded.body != input.body
            {
                return Err(String::from("notification source intent binding changed"));
            }
            if recorded.state != CollaborationNotificationState::Recorded {
                return Ok(None);
            }
            // An existing target receipt must be recovered as fact, even after admission changes.
            // 即使准入改变，已有目标回执仍须作为事实恢复。
            let target_rejection = target_for_write
                .read(|document| {
                    if document
                        .collaboration_state()
                        .inbound_notifications
                        .contains_key(&input.notification_id)
                    {
                        return None;
                    }
                    Some(
                        if matches!(
                            document.collaboration_run_control(),
                            CollaborationRunControl::Stopped
                                | CollaborationRunControl::MainAndSubagents
                        ) {
                            Some(CollaborationDeliveryFailureReason::TargetStopped)
                        } else if participants::require_document_message_admission(
                            document,
                            &input.source,
                            &input.target,
                        )
                        .is_err()
                        {
                            Some(CollaborationDeliveryFailureReason::InvalidBinding)
                        } else {
                            None
                        },
                    )
                })
                .map_err(|error| error.to_string())?;
            // Revocation and acceptance share the source gate; an earlier grant read is insufficient.
            // 撤销与接纳共享来源门；先前的授权读取不足以证明本次投递仍获许可。
            let revoked = cross_project
                && !source_handle_grant_allows(
                    &source_for_write,
                    source_id,
                    target_session_id,
                    CollaborationGrantAction::Notify,
                )?;
            if let Some(target_reason) = target_rejection {
                // Only typed authority or lifecycle facts can settle a pre-accept rejection.
                // 只有强类型权限或生命周期事实才能结算接纳前拒收。
                let reason = if revoked {
                    Some(CollaborationDeliveryFailureReason::AuthorizationRevoked)
                } else {
                    target_reason
                };
                if let Some(reason) = reason {
                    source_for_write
                        .mutate_durable_if_changed(
                            SessionMutationAdmission::ControlPlane,
                            SessionFlushReason::MetadataChanged,
                            move |document| {
                                document
                                    .record_notification_progress(RecordNotificationProgressInput {
                                        operation_id: rejection_operation,
                                        notification_id: input.notification_id,
                                        delivery_id: input.delivery_id,
                                        state: CollaborationNotificationState::Failed,
                                        failure: Some(reason),
                                        target_revision: None,
                                        target_event_seq: None,
                                        updated_at_ms: input.accepted_at_ms,
                                    })
                                    .map(session_change)
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    return Ok(None);
                }
            }
            if revoked {
                return Err(String::from(
                    "notification authorization was revoked before delivery",
                ));
            }
            target_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        participants::require_document_message_admission(
                            document,
                            &input.source,
                            &input.target,
                        )?;
                        document.accept_notification(input).map(session_change)
                    },
                )
                .map(Some)
                .map_err(|error| error.to_string())
        },
    )
    .await?;
    let Some(accepted) = accepted else {
        return Ok(());
    };
    let input = RecordNotificationProgressInput {
        operation_id: progress_operation,
        notification_id,
        delivery_id,
        state: CollaborationNotificationState::Accepted,
        failure: None,
        target_revision: Some(accepted.value.notification.target_revision),
        target_event_seq: Some(accepted.value.event_seq),
        updated_at_ms: now_ms()?,
    };
    mutate_source(invocation, move |document| {
        document.record_notification_progress(input)
    })
    .await?;
    Ok(())
}

/// Copies an already durable target acceptance, projection, or failure to its notification source.
/// 将目标已可靠保存的接纳、投影或失败回执补记到通知来源。
/// `record` is the source snapshot and `target` is its exact canonical participant; returns whether
/// a target fact needed and completed a source progress write.
/// `record` 是来源快照，`target` 是其精确规范参与者；返回目标事实是否需要并完成来源进度写入。
pub(super) async fn recover_existing_notification_progress(
    invocation: &ValidatedInvocation,
    record: &CollaborationNotificationRecord,
    target: &ResolvedSession,
) -> Result<bool, String> {
    let target_progress = target
        .handle
        .read(|document| {
            let Some(inbound) = document
                .collaboration_state()
                .inbound_notifications
                .get(&record.notification_id)
            else {
                if record.state == CollaborationNotificationState::Accepted {
                    return Err(String::from(
                        "accepted notification has no matching target receipt",
                    ));
                }
                return Ok(None);
            };
            // Volatile state after a failed flush is not a receipt that another journal may acknowledge.
            // 刷盘失败后的易失状态不能成为另一份日志可确认的回执。
            if inbound.target_revision > document.durable_revision() {
                return Err(String::from(
                    "notification target progress is not durably published",
                ));
            }
            if inbound.delivery_id != record.delivery_id
                || inbound.source_operation_id != record.operation_id
                || inbound.source != record.source
                || inbound.target != record.target
                || inbound.body != record.body
            {
                return Err(String::from(
                    "notification target receipt conflicts with its source authority",
                ));
            }
            let operation_id = match inbound.state {
                CollaborationNotificationState::Accepted => inbound.operation_id.clone(),
                CollaborationNotificationState::Projected => CollaborationOperationId::try_new(
                    format!("{}:project", record.delivery_id.as_str()),
                )
                .map_err(|error| format!("invalid notification projection identity: {error}"))?,
                CollaborationNotificationState::Failed => CollaborationOperationId::try_new(
                    format!("{}:fail", record.delivery_id.as_str()),
                )
                .map_err(|error| format!("invalid notification failure identity: {error}"))?,
                CollaborationNotificationState::Recorded => {
                    return Err(String::from(
                        "notification target has no valid progress receipt",
                    ));
                }
            };
            let operation = document
                .collaboration_state()
                .operations
                .get(&operation_id)
                .ok_or_else(|| {
                    String::from("notification target has no durable progress operation")
                })?;
            let matching_operation = match &operation.semantic {
                CollaborationOperationSemantic::AcceptNotification {
                    notification_id,
                    delivery_id,
                    source_operation_id,
                    source,
                    target,
                } if inbound.state == CollaborationNotificationState::Accepted => {
                    notification_id == &record.notification_id
                        && delivery_id == &record.delivery_id
                        && source_operation_id == &record.operation_id
                        && source == &record.source
                        && target == &record.target
                }
                CollaborationOperationSemantic::ProjectNotification {
                    notification_id,
                    delivery_id,
                    projected_seq,
                    projected_input_id,
                    projected_turn_id,
                } if inbound.state == CollaborationNotificationState::Projected => {
                    notification_id == &record.notification_id
                        && delivery_id == &record.delivery_id
                        && Some(*projected_seq) == inbound.projected_seq
                        && Some(*projected_input_id) == inbound.projected_input_id
                        && Some(*projected_turn_id) == inbound.projected_turn_id
                }
                CollaborationOperationSemantic::FailNotification {
                    notification_id,
                    delivery_id,
                    reason,
                } if inbound.state == CollaborationNotificationState::Failed => {
                    notification_id == &record.notification_id
                        && delivery_id == &record.delivery_id
                        && Some(*reason) == inbound.failure
                }
                _ => false,
            };
            if !matching_operation {
                return Err(String::from(
                    "notification progress operation does not match its target receipt",
                ));
            }
            Ok(Some((
                inbound.state,
                inbound.failure,
                inbound.target_revision,
                operation.event_seq,
            )))
        })
        .map_err(|error| format!("failed to read notification target receipt: {error}"))??;
    let Some((state, failure, target_revision, target_event_seq)) = target_progress else {
        return Ok(false);
    };
    if record.state == state
        && record.failure == failure
        && record.target_revision == Some(target_revision)
        && record.target_event_seq == Some(target_event_seq)
    {
        // A prior source flush may have failed after publishing its in-memory progress.
        // 先前来源刷盘可能在发布内存进度后失败。
        invocation
            .source_handle
            .flush_all()
            .map_err(|error| error.to_string())?;
        return Ok(false);
    }
    // This records a fact that was already delivered, so a later grant revocation cannot erase it.
    // 此处只确认已经投递的事实，后续授权撤销不能抹去既有投递。
    let operation_id = match state {
        CollaborationNotificationState::Accepted => {
            format!("{}:notification-accepted", record.operation_id.as_str())
        }
        CollaborationNotificationState::Projected => {
            format!("{}:source-projected", record.delivery_id.as_str())
        }
        CollaborationNotificationState::Failed => {
            format!("{}:source-failed", record.delivery_id.as_str())
        }
        CollaborationNotificationState::Recorded => {
            return Err(String::from(
                "notification target progress is not recoverable",
            ));
        }
    };
    let input = RecordNotificationProgressInput {
        operation_id: CollaborationOperationId::try_new(operation_id)
            .map_err(|error| format!("invalid notification progress identity: {error}"))?,
        notification_id: record.notification_id.clone(),
        delivery_id: record.delivery_id.clone(),
        state,
        failure,
        target_revision: Some(target_revision),
        target_event_seq: Some(target_event_seq),
        updated_at_ms: now_ms()?,
    };
    mutate_source(invocation, move |document| {
        document.record_notification_progress(input)
    })
    .await?;
    Ok(true)
}
