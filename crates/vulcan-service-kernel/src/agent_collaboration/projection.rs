//! Source-verified delivery into the next legal provider request boundary.
//! 经过来源核验的投递，进入下一合法 Provider 请求边界。

use super::*;

#[cfg(test)]
mod tests;
use vulcan_session_core::v1::collaboration::{
    CollaborationAnswerDeliveryState, CollaborationDeliveryFailureReason,
    CollaborationInboundDeliveryRecord, CollaborationInboundNotificationRecord,
    CollaborationNotificationState, CollaborationQuestionRecord,
    CollaborationTaskMessageEffectKind, FailAnswerDeliveryInput, FailDeliveryInput,
    FailNotificationInput, RecordNotificationProgressInput,
};

/// Maximum stage notifications admitted at one provider boundary.
/// 一次供应商边界接纳的阶段通知最大条数。
const NOTIFICATION_BATCH_LIMIT: usize = 8;

/// One batch shares a single envelope-sized body budget instead of multiplying it by notification count.
/// 一批共享单个信封大小的正文预算，避免按通知数量倍增。
const NOTIFICATION_BATCH_BYTES: usize =
    vulcan_ai_contract::host_message::HOST_MESSAGE_MAX_BODY_BYTES;

/// One durable incoming fact ordered by its local acceptance sequence.
/// 按本地接纳序号排序的一条持久传入事实。
enum PendingProjection {
    /// Owned child report addressed to its original parent business turn.
    /// 发给原父业务回合的所属子报告。
    ChildTaskResult(vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord),
    /// An accepted result whose original requester turn is still open.
    /// 原请求回合仍开放的已接纳结果。
    TaskResult(vulcan_session_core::v1::collaboration::CollaborationTaskResultAcceptance),
    /// A question whose source must still authorize its body delivery.
    /// 来源仍须授权其正文交付的问题。
    Question(CollaborationInboundDeliveryRecord),
    /// A notification already copied into the target's durable inbox.
    /// 已复制到目标持久收件箱的通知。
    Notification(CollaborationInboundNotificationRecord),
    /// A locally authoritative answer awaiting delivery to this source's execution.
    /// 等待交付给当前来源执行的本地权威答案。
    Answer(CollaborationQuestionRecord),
    /// Exact-turn task guidance, separate from question and notification delivery.
    /// 精确回合任务方向，与问题及通知投递分离。
    Steering(vulcan_session_core::v1::collaboration::CollaborationTaskMessageAcceptance),
}

/// Captures pending projection facts in durable local order without mutating their states.
/// 按持久本地顺序捕获待投影事实，不改变其状态。
fn pending_projections(
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> Result<Vec<PendingProjection>, String> {
    let Some(turn) = document.active_turn().filter(|turn| !turn.closed) else {
        return Ok(Vec::new());
    };
    let state = document.collaboration_state();
    let mut pending = Vec::new();
    for inbound in state.inbound_deliveries.values() {
        if inbound.state != CollaborationDeliveryState::Accepted || inbound.settlement.is_some() {
            continue;
        }
        let eligible = match inbound.execution_purpose {
            CollaborationExecutionPurpose::InTurnQuestion => {
                inbound.bound_turn_id == Some(turn.turn_id)
            }
            CollaborationExecutionPurpose::NaturallyIdleQuestion => {
                state.question_executions.values().any(|execution| {
                    execution.question_id == inbound.question_id
                        && execution.state == CollaborationQuestionExecutionState::Running
                        && execution.execution_turn_id == Some(turn.turn_id)
                })
            }
            CollaborationExecutionPurpose::NormalTask => false,
        };
        if !eligible {
            continue;
        }
        let operation = state
            .operations
            .get(&inbound.operation_id)
            .ok_or_else(|| String::from("incoming question has no durable acceptance operation"))?;
        pending.push((
            operation.event_seq,
            PendingProjection::Question(inbound.clone()),
        ));
    }
    for notification in state.inbound_notifications.values() {
        if notification.state != CollaborationNotificationState::Accepted {
            continue;
        }
        let operation = state
            .operations
            .get(&notification.operation_id)
            .ok_or_else(|| {
                String::from("incoming notification has no durable acceptance operation")
            })?;
        pending.push((
            operation.event_seq,
            PendingProjection::Notification(notification.clone()),
        ));
    }
    for question in state.questions.values() {
        let Some(answer) = &question.answer else {
            continue;
        };
        if answer.delivery.state != CollaborationAnswerDeliveryState::Undelivered {
            continue;
        }
        let operation = state
            .operations
            .get(&answer.operation_id)
            .ok_or_else(|| String::from("answer has no durable settlement operation"))?;
        pending.push((
            operation.event_seq,
            PendingProjection::Answer(question.clone()),
        ));
    }
    for acceptance in state.inbound_task_messages.values() {
        if acceptance.message.kind
            == vulcan_session_core::v1::collaboration::CollaborationTaskMessageKind::TaskSteer
            && !state
                .task_message_effects
                .contains_key(&acceptance.message.delivery_id)
        {
            pending.push((
                acceptance.event_seq,
                PendingProjection::Steering(acceptance.clone()),
            ));
        }
    }
    for (delivery_id, acceptance) in &state.received_task_results {
        if state.run_control == CollaborationRunControl::Running
            && acceptance
                .result
                .acceptance
                .message
                .source
                .turn_id
                .is_some_and(|source_turn| turn.turn_id >= source_turn)
            && !state.task_result_projections.contains_key(delivery_id)
        {
            pending.push((
                acceptance.event_seq,
                PendingProjection::TaskResult(acceptance.clone()),
            ));
        }
    }
    for task in state.child_tasks.values() {
        if state.run_control != CollaborationRunControl::Running
            || task.parent_turn_id != turn.turn_id
            || task.non_success_projection.is_some()
        {
            continue;
        }
        if let Some(result) = &task.non_success_result {
            pending.push((
                result.event_seq,
                PendingProjection::ChildTaskResult(task.clone()),
            ));
        }
    }
    // Questions and terminal results precede stage updates; sequence remains stable within each class.
    // 问题与最终结果先于阶段更新；各类内部序号顺序保持稳定。
    pending.sort_by_key(|(seq, fact)| (matches!(fact, PendingProjection::Notification(_)), *seq));
    // Retain excess notifications as Accepted; this selector never marks deferred bodies delivered.
    // 超额通知保持已接纳；此选择器绝不把延后正文标记为已投递。
    let mut notification_count = 0;
    let mut notification_bytes = 0;
    let mut notification_batch_full = false;
    Ok(pending
        .into_iter()
        .filter_map(|(_, fact)| {
            if let PendingProjection::Notification(notification) = &fact {
                if notification_batch_full
                    || notification_count >= NOTIFICATION_BATCH_LIMIT
                    || notification_bytes + notification.body.len() > NOTIFICATION_BATCH_BYTES
                {
                    notification_batch_full = true;
                    return None;
                }
                notification_count += 1;
                notification_bytes += notification.body.len();
            }
            Some(fact)
        })
        .collect())
}

/// Reports actionable inbox readiness through the same selector used for actual context delivery.
/// 通过实际上下文投递使用的同一选择器报告可处理收件状态。
/// Does not consume messages; cancellation before the next provider boundary leaves them pending.
/// 不消费消息；下一供应商边界之前取消会保留待处理消息。
pub(super) fn inbox_messages(
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> Result<Vec<Value>, String> {
    pending_projections(document)?.into_iter().map(|pending| Ok(match pending {
        PendingProjection::Question(inbound) => json!({"kind":"question","question_id":inbound.question_id,"session_id":inbound.source.session_id}),
        PendingProjection::Notification(notification) => json!({"kind":"notification","notification_id":notification.notification_id,"session_id":notification.source.session_id}),
        PendingProjection::Answer(question) => json!({"kind":"answer","question_id":question.question_id,"session_id":question.target.session_id,"outcome":question.state}),
        PendingProjection::Steering(acceptance) => json!({"kind":"guidance","delivery_id":acceptance.message.delivery_id,"session_id":acceptance.message.source.session_id}),
        PendingProjection::TaskResult(acceptance) => json!({"kind":"task_result","delivery_id":acceptance.result.effect.delivery_id,
            "session_id":acceptance.result.acceptance.message.target.session_id,"outcome":match &acceptance.result.terminal {
                Some(terminal) => json!(terminal.outcome),
                None => match &acceptance.result.effect.outcome {
                    CollaborationTaskMessageEffectKind::Failed { reason: CollaborationDeliveryFailureReason::TargetStopped } => json!("stopped"),
                    CollaborationTaskMessageEffectKind::Failed { reason: CollaborationDeliveryFailureReason::TargetExecutionLost } => json!("abandoned"),
                    _ => json!("failed"),
                },
            }}),
        PendingProjection::ChildTaskResult(task) => json!({"kind":"task_result","execution_id":task.execution_id,"session_id":task.child.session_id,"outcome":task.terminal_outcome}),
    })).collect()
}

/// Projects accepted collaboration facts immediately before physical API preparation.
/// 紧邻物理 API 准备前投影已接纳的协作事实。
/// The canonical handle is retained by the active stream; no user prompt or tool result is rewritten.
/// 规范句柄由活动流持有；不改写用户提示词或工具结果。
pub(crate) async fn project_collaboration_before_api(
    state: &SharedServiceBridgeState,
    handle: &Arc<SessionHandle>,
    now_ms: i64,
) -> Result<(), String> {
    let pending = handle
        .read(pending_projections)
        .map_err(|error| error.to_string())??;
    if pending.is_empty() {
        return Ok(());
    }
    let (runtime, registry) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        (
            guard
                .controller_runtime
                .clone()
                .ok_or_else(|| String::from("controller runtime is unavailable"))?,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let invocation = recovery_invocation(
        Arc::clone(state),
        runtime,
        registry,
        handle.session_id(),
        Arc::clone(handle),
    )?;
    for fact in pending {
        match fact {
            PendingProjection::ChildTaskResult(task) => {
                project_child_task_result(&invocation, handle, task, now_ms).await?;
            }
            PendingProjection::TaskResult(acceptance) => {
                task_results::project_task_result(&invocation, handle, acceptance, now_ms).await?;
            }
            PendingProjection::Steering(acceptance) => {
                let delivery_id = acceptance.message.delivery_id.clone();
                session_messages::project_steering(&invocation, handle, acceptance, now_ms).await?;
                let failed = handle
                    .read(|document| {
                        document
                            .collaboration_state()
                            .task_message_effects
                            .get(&delivery_id)
                            .is_some_and(|effect| {
                                matches!(
                                    effect.outcome,
                                    CollaborationTaskMessageEffectKind::Failed { .. }
                                )
                            })
                    })
                    .map_err(|error| error.to_string())?;
                if failed
                    && let Err(error) =
                        reconcile_task_results_for_session(state, handle.session_id()).await
                {
                    tracing::warn!(session_id = %handle.session_id(), %error,
                            "Steering application failure return remains pending");
                }
            }
            PendingProjection::Question(inbound) => {
                let source =
                    participants::load_participant_handle(&invocation, &inbound.source).await?;
                let source_id = source.session_id();
                // Read source state and authorization only after both canonical lifecycle gates are held.
                // 只在持有双方规范生命周期锁后读取来源状态与授权。
                let source_for_write = Arc::clone(&source);
                let target_for_write = Arc::clone(handle);
                let delivery = inbound.clone();
                let receipt = lifecycle_write::with_canonical_handles(
                    &invocation.registry,
                    vec![Arc::clone(&source), Arc::clone(handle)],
                    move || {
                        let (question, _, _) =
                            read_question_source(&source_for_write, &delivery.question_id)?;
                        if question.state != CollaborationQuestionState::Pending {
                            return Ok(None);
                        }
                        let revoked = delivery.source.project_id != delivery.target.project_id
                            && !source_handle_grant_allows(
                                &source_for_write,
                                source_id,
                                target_for_write.session_id(),
                                CollaborationGrantAction::Ask,
                            )?;
                        // Settle an already accepted question on its target before returning
                        // failure to the source. A crash between those writes is repaired from
                        // the target's durable failure fact by the source outbox.
                        // 已接纳问题先在目标结算，再把失败回传来源；两次写入之间崩溃时，来源出站箱
                        // 根据目标的可靠失败事实修复。
                        target_for_write
                            .mutate_durable_if_changed(
                                SessionMutationAdmission::MetadataOnly,
                                SessionFlushReason::MetadataChanged,
                                move |document| {
                                    if revoked {
                                        let operation_id = CollaborationOperationId::try_new(
                                            format!("{}:fail", delivery.delivery_id.as_str()),
                                        )?;
                                        Ok(session_change(document.fail_delivery(FailDeliveryInput {
                                            operation_id,
                                            delivery_id: delivery.delivery_id,
                                            question_id: delivery.question_id,
                                            reason: CollaborationDeliveryFailureReason::AuthorizationRevoked,
                                            failed_at_ms: now_ms,
                                        })?))
                                    } else {
                                        Ok(session_change(
                                            document.project_question_before_api(question, now_ms)?,
                                        ))
                                    }
                                },
                            )
                            .map(Some)
                            .map_err(|error| error.to_string())
                    },
                )
                .await?;
                let Some(receipt) = receipt else {
                    continue;
                };
                // Acknowledgement is a later source transaction, outside the two-participant write.
                // 确认是之后独立的来源事务，位于双方写入范围之外。
                let failed = receipt.value.state == CollaborationDeliveryState::Failed;
                let input = RecordDeliveryProgressInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "{}:{}",
                        inbound.delivery_id.as_str(),
                        if failed {
                            "source-failed"
                        } else {
                            "source-projected"
                        }
                    ))
                    .map_err(|error| error.to_string())?,
                    delivery_id: inbound.delivery_id,
                    question_id: inbound.question_id,
                    state: receipt.value.state,
                    failure: failed
                        .then_some(CollaborationDeliveryFailureReason::AuthorizationRevoked),
                    target_revision: Some(receipt.value.revision),
                    target_event_seq: Some(receipt.value.event_seq),
                    updated_at_ms: now_ms,
                };
                mutate_handle(&invocation.registry, &source, source_id, move |document| {
                    document.record_delivery_progress(input)
                })
                .await?;
            }
            PendingProjection::Notification(inbound) => {
                let source =
                    participants::load_participant_handle(&invocation, &inbound.source).await?;
                let source_id = source.session_id();
                // Revocation cannot pass this source gate until the target's actual write completes.
                // 目标实际写入完成前，撤销不能通过此来源锁。
                let source_for_write = Arc::clone(&source);
                let target_for_write = Arc::clone(handle);
                let delivery = inbound.clone();
                let receipt = lifecycle_write::with_canonical_handles(
                    &invocation.registry, vec![Arc::clone(&source), Arc::clone(handle)], move || {
                        let original = source_for_write.read(|document| document.collaboration_state().notifications.get(&delivery.notification_id).cloned())
                            .map_err(|error| error.to_string())?.ok_or_else(|| String::from("notification source record is missing"))?;
                        if original.source != delivery.source || original.target != delivery.target
                            || original.delivery_id != delivery.delivery_id || original.body != delivery.body
                        {
                            return Err(String::from("notification inbox does not match its source authority"));
                        }
                        let revoked = delivery.source.project_id != delivery.target.project_id
                            && !source_handle_grant_allows(&source_for_write, source_id, target_for_write.session_id(), CollaborationGrantAction::Notify)?;
                        // The accepted body cannot be projected after revocation. Settle it on
                        // the target first; the source outbox can replay its progress if the
                        // following source write is interrupted.
                        // 授权撤销后不能投影已接纳正文。先在目标结算；若后续来源写入中断，来源出站箱
                        // 可以重放该进度。
                        target_for_write.mutate_durable_if_changed(
                            SessionMutationAdmission::MetadataOnly, SessionFlushReason::MetadataChanged,
                            move |document| {
                                if revoked {
                                    let operation_id = CollaborationOperationId::try_new(format!(
                                        "{}:fail", delivery.delivery_id.as_str()
                                    ))?;
                                    Ok(session_change(document.fail_notification(FailNotificationInput {
                                        operation_id,
                                        notification_id: delivery.notification_id,
                                        delivery_id: delivery.delivery_id,
                                        reason: CollaborationDeliveryFailureReason::AuthorizationRevoked,
                                        failed_at_ms: now_ms,
                                    })?))
                                } else {
                                    Ok(session_change(document.project_notification_before_api(delivery.notification_id, now_ms)?))
                                }
                            },
                        ).map_err(|error| error.to_string())
                    },
                ).await?;
                let (operation_suffix, state) = match receipt.value.notification.state {
                    CollaborationNotificationState::Projected => (
                        "source-projected",
                        CollaborationNotificationState::Projected,
                    ),
                    CollaborationNotificationState::Failed => {
                        ("source-failed", CollaborationNotificationState::Failed)
                    }
                    CollaborationNotificationState::Recorded
                    | CollaborationNotificationState::Accepted => {
                        return Err(String::from(
                            "notification projection produced no terminal target progress",
                        ));
                    }
                };
                let input = RecordNotificationProgressInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "{}:{operation_suffix}",
                        inbound.delivery_id.as_str()
                    ))
                    .map_err(|error| error.to_string())?,
                    notification_id: inbound.notification_id,
                    delivery_id: inbound.delivery_id,
                    state,
                    failure: receipt.value.notification.failure,
                    target_revision: Some(receipt.value.notification.target_revision),
                    target_event_seq: Some(receipt.value.event_seq),
                    updated_at_ms: now_ms,
                };
                mutate_handle(&invocation.registry, &source, source_id, move |document| {
                    document.record_notification_progress(input)
                })
                .await?;
            }
            PendingProjection::Answer(question) => {
                let target_id = Uuid::parse_str(&question.target.session_id)
                    .map_err(|error| error.to_string())?;
                // The original question source owns both this reply grant and the answer projection.
                // 原问题来源同时拥有此回复授权与答案投影。
                let source_for_write = Arc::clone(handle);
                lifecycle_write::with_canonical_handles(
                    &invocation.registry,
                    vec![Arc::clone(handle)],
                    move || {
                        let still_undelivered = source_for_write
                            .read(|document| {
                                document
                                    .question_snapshot(&question.question_id)
                                    .and_then(|current| current.answer.as_ref())
                                    .is_some_and(|answer| {
                                        answer.delivery.state
                                            == CollaborationAnswerDeliveryState::Undelivered
                                    })
                            })
                            .map_err(|error| error.to_string())?;
                        if !still_undelivered {
                            return Ok(None);
                        }
                        let revoked = question.source.project_id != question.target.project_id
                            && !source_handle_grant_allows(
                                &source_for_write,
                                source_for_write.session_id(),
                                target_id,
                                CollaborationGrantAction::Reply,
                            )?;
                        source_for_write
                            .mutate_durable_if_changed(
                                SessionMutationAdmission::MetadataOnly,
                                SessionFlushReason::MetadataChanged,
                                move |document| {
                                    if revoked {
                                        let operation_id = CollaborationOperationId::try_new(
                                            format!("{}:answer:fail", question.delivery_id.as_str()),
                                        )?;
                                        Ok(session_change(document.fail_answer_delivery(
                                            FailAnswerDeliveryInput {
                                                operation_id,
                                                question_id: question.question_id,
                                                reason: CollaborationDeliveryFailureReason::AuthorizationRevoked,
                                                failed_at_ms: now_ms,
                                            },
                                        )?))
                                    } else {
                                        Ok(session_change(document.project_answer_before_api(
                                            question.question_id,
                                            now_ms,
                                        )?))
                                    }
                                },
                            )
                            .map(Some)
                            .map_err(|error| error.to_string())
                    },
                )
                .await?;
            }
        }
    }
    Ok(())
}

/// Projects a captured child report while the actual parent lifecycle owner holds admission.
/// 实际父生命周期所有者持有准入期间，投影已捕获子报告。
/// `task` is the immutable local reservation snapshot, not a caller-selected source body.
/// `task` 为不可变本地预留快照，而非调用方选择的来源正文。
/// Returns after durable projection or harmless staleness; unproven publication remains an error.
/// 可靠投影或无害过期后返回；未证明的发布仍返回错误。
async fn project_child_task_result(
    invocation: &ValidatedInvocation,
    parent: &Arc<SessionHandle>,
    task: vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord,
    stamp: i64,
) -> Result<(), String> {
    // Retain the canonical handle inside the actual blocking writer across waiter cancellation.
    // 在实际阻塞写入器内部保留规范句柄，跨越等待者取消。
    let parent_for_write = Arc::clone(parent);
    lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![Arc::clone(parent)],
        move || {
            // Recheck local source proof and original turn after acquiring lifecycle admission.
            // 取得生命周期准入后重查本地来源凭据和原回合。
            let eligible = parent_for_write
                .read(|document| {
                    let Some(current) = document
                        .collaboration_state()
                        .child_tasks
                        .get(&task.execution_id)
                    else {
                        return Ok(false);
                    };
                    if current.reservation_operation_id != task.reservation_operation_id
                        || current.parent_segment_generation != task.parent_segment_generation
                        || current.non_success_result != task.non_success_result
                    {
                        return Err(String::from(
                            "Child result projection no longer matches its original reservation.",
                        ));
                    }
                    let result = current.non_success_result.as_ref().ok_or_else(|| {
                        String::from("Child result projection has no durable report.")
                    })?;
                    if document.durable_revision() < result.parent_revision {
                        return Err(String::from(
                            "Child result projection requires its published parent revision.",
                        ));
                    }
                    Ok(current.non_success_projection.is_none()
                        && document.segment_generation() == task.parent_segment_generation
                        && document.collaboration_run_control() == CollaborationRunControl::Running
                        && document.active_turn().is_some_and(|turn| {
                            !turn.closed && turn.turn_id == task.parent_turn_id
                        }))
                })
                .map_err(|error| error.to_string())??;
            if !eligible {
                return Ok(());
            }
            parent_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        document
                            .project_child_task_non_success_before_api(task.execution_id, stamp)
                            .map(session_change)
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .await
}
