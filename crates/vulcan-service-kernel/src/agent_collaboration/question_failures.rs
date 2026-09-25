//! Returns target stop failures to their original question and notification owners.
//! 将目标停止失败回传给原问题和通知所有者。

use super::*;

/// Returns stopped question and notification deliveries without changing pending question state.
/// 回传停止的问题和通知投递，但不改变待答问题状态。
/// `state` resolves canonical owners; `target_session_id` identifies the stopped recipient.
/// `state` 解析规范所有者；`target_session_id` 标识已停止的接收者。
/// Returns the number of new source receipts after trying every independent failure.
/// 尝试全部独立失败后返回新来源回执数量。
pub(crate) async fn reconcile_stopped_question_deliveries_for_session(
    state: &SharedServiceBridgeState,
    target_session_id: Uuid,
) -> Result<usize, String> {
    let (controller, registry) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("controller runtime is unavailable for stopped question return")
            })?,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let target = registry
        .get(target_session_id)
        .map_err(|error| error.to_string())?;
    // Notifications share stop publication but retain their independent delivery state machine.
    // 通知共享停止发布入口，但保留独立投递状态机。
    let notification_result =
        notifications::reconcile_stopped_notifications_for_session(state, &target).await;
    let failures = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .filter(|delivery| {
                    delivery.state == CollaborationDeliveryState::Failed
                        && delivery.failure
                            == Some(CollaborationDeliveryFailureReason::TargetStopped)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    if failures.is_empty() {
        return notification_result;
    }
    let invocation = recovery_invocation(
        Arc::clone(state),
        controller,
        registry,
        target_session_id,
        Arc::clone(&target),
    )?;
    let (mut delivered, mut errors) = match notification_result {
        Ok(count) => (count, Vec::new()),
        Err(error) => (0, vec![error]),
    };
    for inbound in failures {
        let question_id = inbound.question_id.clone();
        let source = match participants::load_participant_handle(&invocation, &inbound.source).await
        {
            Ok(source) => source,
            Err(error) => {
                errors.push(format!(
                    "question {} source resolution failed: {error}",
                    question_id.as_str()
                ));
                continue;
            }
        };
        let source_for_write = Arc::clone(&source);
        let target_for_read = Arc::clone(&target);
        let result = lifecycle_write::with_canonical_handles(
            &invocation.registry,
            vec![Arc::clone(&source), Arc::clone(&target)],
            move || {
                let (question, delivery, _) =
                    read_question_source(&source_for_write, &inbound.question_id)?;
                let current = target_for_read
                    .read(|document| {
                        validate_question_target_delivery(document, &question, &delivery)
                            .map_err(|error| error.to_string())?;
                        document
                            .inbound_delivery_snapshot(&inbound.delivery_id)
                            .cloned()
                            .ok_or_else(|| String::from("stopped question has no target receipt"))
                    })
                    .map_err(|error| error.to_string())??;
                if current.state != CollaborationDeliveryState::Failed
                    || current.failure != Some(CollaborationDeliveryFailureReason::TargetStopped)
                    || current.source != inbound.source
                    || current.target != inbound.target
                    || current.execution_purpose != inbound.execution_purpose
                    || current.bound_turn_id != inbound.bound_turn_id
                {
                    return Err(String::from("stopped question receipt changed identity"));
                }
                let target_event_seq = target_inbound_event_seq(&target_for_read, &current)?;
                // A prior return is valid only for this exact published target failure.
                // 既有回传只有指向目标这次精确发布的失败才有效。
                if delivery.state == CollaborationDeliveryState::Failed {
                    return if delivery.failure == current.failure
                        && delivery.target_revision == Some(current.target_revision)
                        && delivery.target_event_seq == Some(target_event_seq)
                    {
                        Ok(false)
                    } else {
                        Err(String::from("source failure does not match stopped target"))
                    };
                }
                let progress = RecordDeliveryProgressInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "{}:source-failed",
                        current.delivery_id.as_str()
                    ))
                    .map_err(|error| error.to_string())?,
                    delivery_id: current.delivery_id,
                    question_id: current.question_id,
                    state: CollaborationDeliveryState::Failed,
                    failure: current.failure,
                    target_revision: Some(current.target_revision),
                    target_event_seq: Some(target_event_seq),
                    updated_at_ms: now_ms()?,
                };
                source_for_write
                    .mutate_durable_if_changed(
                        SessionMutationAdmission::MetadataOnly,
                        SessionFlushReason::MetadataChanged,
                        move |document| {
                            document
                                .record_delivery_progress(progress)
                                .map(session_change)
                        },
                    )
                    .map_err(|error| error.to_string())?;
                Ok(true)
            },
        )
        .await;
        match result {
            Ok(true) => delivered += 1,
            Ok(false) => {}
            Err(error) => errors.push(format!(
                "question {} stop return failed: {error}",
                question_id.as_str()
            )),
        }
    }
    if errors.is_empty() {
        Ok(delivered)
    } else {
        Err(errors.join("; "))
    }
}
