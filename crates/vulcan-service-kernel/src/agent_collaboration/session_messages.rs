//! Reliable session task intents and exact-turn steering at provider boundaries.
//! 可靠会话任务意图与供应商边界的精确回合方向投影。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::{
    SendSessionMessageArgs, SessionMessageDeliveryMode,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationTaskMessage, CollaborationTaskMessageAcceptance, CollaborationTaskMessageKind,
    CollaborationTaskMessageRecord,
};

/// Maps an explicit task message kind to its independently granted action.
/// 将明确任务消息类型映射为独立授权动作。
pub(super) fn task_action(kind: CollaborationTaskMessageKind) -> CollaborationGrantAction {
    match kind {
        CollaborationTaskMessageKind::TaskStart => CollaborationGrantAction::Start,
        CollaborationTaskMessageKind::TaskSteer => CollaborationGrantAction::Steer,
    }
}

/// Returns the source-owned receipt for a recorded intent, including a recoverable delivery error.
/// 返回已记录意图的来源所有回执，包括可恢复投递错误。
/// Acceptance is delivery metadata and does not claim that a queued business task has started.
/// 接纳是投递元数据，不声称排队业务任务已经开始。
fn send_receipt(record: CollaborationTaskMessageRecord, error: Option<String>) -> Value {
    json!({
        "status": if record.acceptance.is_some() { "accepted" } else { "recorded" },
        "operation_id": record.message.operation_id,
        "delivery_id": record.message.delivery_id,
        "message_kind": record.message.kind,
        "record": record,
        "delivery_error": error,
    })
}

/// Records `args` under the admitted model invocation and delivers its immutable intent.
/// 按已准入模型调用记录 `args` 并投递其不可变意图。
/// Returns a durable identity before execution; retries preserve the first exact target binding.
/// 在执行前返回可靠身份；重试保留首次精确目标绑定。
pub(super) async fn send_session_message(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    args: SendSessionMessageArgs,
) -> Result<Value, String> {
    let target_id = Uuid::parse_str(&args.session_id)
        .map_err(|error| format!("invalid task message target identity: {error}"))?;
    // A published source intent remains live until delivery hands off its wakeup owner.
    // 来源意图发布后保持存活，直到投递交接唤醒所有者。
    let _dispatch_owner = dispatch_owner::DispatchOwner::acquire(&invocation.state, target_id)?;
    let kind = match args.delivery_mode {
        SessionMessageDeliveryMode::Queue => CollaborationTaskMessageKind::TaskStart,
        SessionMessageDeliveryMode::Interrupt => CollaborationTaskMessageKind::TaskSteer,
    };
    let operation_id = collaboration_operation_id(invocation)?;
    let delivery_id = collaboration_delivery_id(invocation)?;
    let source = source_participant(invocation).await?;
    // A retry reads the first frozen intent before consulting the target's current turn.
    // 重试先读取首次冻结意图，再决定是否需要查看目标当前回合。
    let previous = invocation
        .source_handle
        .read(|document| {
            document
                .collaboration_state()
                .outbound_task_messages
                .get(&delivery_id)
                .cloned()
        })
        .map_err(|error| error.to_string())?;
    let message = if let Some(previous) = previous {
        if previous.message.operation_id != operation_id
            || previous.message.source != source
            || previous.message.target.session_id != target_id.to_string()
            || previous.message.kind != kind
            || previous.message.body != args.message
        {
            return Err(String::from(
                "task message retry conflicts with the recorded source intent",
            ));
        }
        previous.message
    } else {
        let target = resolve_authorized_target(invocation, target_id, task_action(kind)).await?;
        let target_turn = target.handle.read(|document| {
            if document.metadata.archived_at_ms.is_some()
                || matches!(document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainOnly
                    | CollaborationRunControl::MainAndSubagents)
            {
                return Err(String::from("The target agent has stopped or is archived and rejects new task messages."));
            }
            match kind {
                CollaborationTaskMessageKind::TaskStart => Ok(None),
                CollaborationTaskMessageKind::TaskSteer => document.active_turn()
                    .filter(|turn| !turn.closed && turn.origin != TurnOrigin::QuestionExecution)
                    .map(|turn| Some(turn.turn_id))
                    .ok_or_else(|| String::from("task steering requires an active target business turn")),
            }
        }).map_err(|error| error.to_string())??;
        CollaborationTaskMessage {
            delivery_id: delivery_id.clone(),
            operation_id: operation_id.clone(),
            source,
            target: target_participant(invocation, target_id, &target, target_turn).await?,
            kind,
            body: args.message,
        }
    };
    let source_api = invocation
        .source_api_call_id
        .ok_or_else(|| String::from("task messages require an admitted model API"))?;
    let source_id = invocation.source_session_id;
    let message_for_write = message.clone();
    let created_at_ms = now_ms()?;
    let recorded = mutate_source(invocation, move |document| {
        // Async locator work cannot retain permission from a superseded physical execution.
        // 异步定位工作不能继续使用已被替代的物理执行权限。
        if document.active_api().is_none_or(|api| {
            api.api_call_id != source_api
                || Some(api.turn_id) != message_for_write.source.turn_id
                || matches!(
                    api.purpose,
                    ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
                )
        }) {
            return Err(
                vulcan_session_core::v1::errors::DomainError::CollaborationUnauthorized {
                    action: "send_task_message_execution",
                    identity: source_id.to_string(),
                },
            );
        }
        if message_for_write.source.project_id != message_for_write.target.project_id
            && !document_grant_allows(document, source_id, target_id, task_action(kind))
        {
            return Err(
                vulcan_session_core::v1::errors::DomainError::CollaborationUnauthorized {
                    action: "send_task_message_grant",
                    identity: target_id.to_string(),
                },
            );
        }
        // A concurrent exact retry may have committed while this caller resolved the target.
        // 当前调用解析目标期间，并发的精确重试可能已经提交。
        if let Some(existing) = document
            .collaboration_state()
            .outbound_task_messages
            .get(&message_for_write.delivery_id)
            .cloned()
        {
            if existing.message.operation_id != message_for_write.operation_id
                || existing.message.source != message_for_write.source
                || existing.message.target.session_id != message_for_write.target.session_id
                || existing.message.kind != message_for_write.kind
                || existing.message.body != message_for_write.body
            {
                return Err(
                    vulcan_session_core::v1::errors::DomainError::CollaborationConflict {
                        operation_id: message_for_write.operation_id.as_str().to_owned(),
                    },
                );
            }
            document.send_task_message(existing.message, created_at_ms)
        } else {
            document.send_task_message(message_for_write, created_at_ms)
        }
    })
    .await
    .map_err(|error| {
        format!(
            "Task message operation {} delivery {} publication is uncertain: {error}",
            operation_id.as_str(),
            delivery_id.as_str(),
        )
    })?;
    // Once source publication succeeds, later transport failures retain a queryable stable identity.
    // 来源发布成功后，后续传输失败仍保留可查询稳定身份。
    // Delivery always uses the winning Core receipt, including concurrent retries.
    // 投递始终使用胜出的 Core 回执，包括并发重试。
    let message = recorded.value.record.message.clone();
    let delivered = task_delivery::deliver_recorded_task_message(app, invocation, message).await;
    match delivered {
        Ok(record) => Ok(send_receipt(record, None)),
        Err(error) => Ok(send_receipt(recorded.value.record, Some(error))),
    }
}

/// Applies one accepted steering message to `handle` at the legal request boundary.
/// 在合法请求边界将一条已接纳方向消息应用到 `handle`。
/// Source lookup failures leave delivery pending; binding and grant failures receive explicit effects.
/// 来源查询失败保留待投递状态；绑定及授权失败获得明确效果回执。
pub(super) async fn project_steering(
    invocation: &ValidatedInvocation,
    handle: &Arc<SessionHandle>,
    acceptance: CollaborationTaskMessageAcceptance,
    now_ms: i64,
) -> Result<(), String> {
    let source =
        match participants::load_participant_handle(invocation, &acceptance.message.source).await {
            Ok(source) => source,
            Err(error) => {
                tracing::warn!(delivery_id = acceptance.message.delivery_id.as_str(), %error,
                "Task steering source lookup deferred");
                return Ok(());
            }
        };
    let source_for_write = source.clone();
    let target_for_write = handle.clone();
    lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![source, handle.clone()],
        move || {
            let original = source_for_write
                .read(|document| {
                    document
                        .collaboration_state()
                        .outbound_task_messages
                        .get(&acceptance.message.delivery_id)
                        .cloned()
                })
                .map_err(|error| error.to_string())?;
            let source_matches =
                original.is_some_and(|record| record.message == acceptance.message);
            let allowed = acceptance.message.source.project_id
                == acceptance.message.target.project_id
                || source_handle_grant_allows(
                    &source_for_write,
                    source_for_write.session_id(),
                    target_for_write.session_id(),
                    CollaborationGrantAction::Steer,
                )?;
            let operation_id = CollaborationOperationId::try_new(format!(
                "{}:task-message-effect",
                acceptance.message.delivery_id.as_str()
            ))
            .map_err(|error| error.to_string())?;
            target_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        // Terminal receipt retries are metadata-only and must not append guidance again.
                        // 终态回执重试只读取元数据，不得再次追加方向。
                        if let Some(effect) = document
                            .collaboration_state()
                            .task_message_effects
                            .get(&acceptance.message.delivery_id)
                        {
                            return Ok(SessionMutationChange::Unchanged(effect.clone()));
                        }
                        let failure = if !source_matches {
                            Some(CollaborationDeliveryFailureReason::InvalidBinding)
                        } else if !allowed {
                            Some(CollaborationDeliveryFailureReason::AuthorizationRevoked)
                        } else if document.metadata.archived_at_ms.is_some()
                            || matches!(
                                document.collaboration_run_control(),
                                CollaborationRunControl::Stopped
                                    | CollaborationRunControl::MainOnly
                                    | CollaborationRunControl::MainAndSubagents
                            )
                        {
                            Some(CollaborationDeliveryFailureReason::TargetStopped)
                        } else if document.active_turn().is_none_or(|turn| {
                            turn.closed
                                || Some(turn.turn_id) != acceptance.message.target.turn_id
                                || turn.origin == TurnOrigin::QuestionExecution
                        }) {
                            Some(CollaborationDeliveryFailureReason::InvalidBinding)
                        } else {
                            None
                        };
                        if let Some(reason) = failure {
                            document
                                .fail_task_message(
                                    operation_id,
                                    acceptance.message.delivery_id,
                                    reason,
                                    now_ms,
                                )
                                .map(session_change)
                        } else {
                            document
                                .apply_task_steering_before_api(
                                    operation_id,
                                    acceptance.message.delivery_id,
                                    now_ms,
                                )
                                .map(session_change)
                        }
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .await
}
