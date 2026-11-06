//! Shared delivery of durable task intents for live tools and host recovery.
//! 实时工具与宿主恢复共用的可靠任务意图投递。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationTaskMessage, CollaborationTaskMessageKind, CollaborationTaskMessageRecord,
};

/// Delivers `message` only after checking its exact canonical source record.
/// 仅在核对精确规范来源记录后投递 `message`。
/// `app` publishes wakeups; `invocation` supplies canonical ownership, not new model authority.
/// `app` 发布唤醒；`invocation` 提供规范所有权，不授予新的模型权限。
/// Returns the source receipt; an error preserves any already committed target acceptance.
/// 返回来源回执；错误仍保留任何已经提交的目标接纳。
pub(super) async fn deliver_recorded_task_message(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    message: CollaborationTaskMessage,
) -> Result<CollaborationTaskMessageRecord, String> {
    // Resolve the frozen participant, never the target's replacement turn or generation.
    // 解析冻结的参与者，绝不改用目标的替代回合或代际。
    let target = participants::load_participant_handle(invocation, &message.target).await?;
    let target_id = target.session_id();
    // Span durable acceptance through scheduling; a waiter must not settle the handoff gap.
    // 跨越可靠接纳到调度，等待者不得结算交接间隙。
    let _dispatch_owner = dispatch_owner::DispatchOwner::acquire(&invocation.state, target_id)?;
    let source_id = invocation.source_session_id;
    let source = invocation.source_handle.clone();
    let receiver = target.clone();
    let intent = message.clone();
    let accepted_at_ms = now_ms()?;
    let acceptance = lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![source.clone(), target],
        move || {
            // A failed source publication may leave a staged intent; publish it before any remote effect.
            // 来源发布失败可能留下暂存意图；任何远端效果之前先将其可靠发布。
            source.flush_all().map_err(|error| error.to_string())?;
            // A target acceptance whose flush failed is republished here instead of blocking retries.
            // 刷盘失败的目标接纳在此重新发布，而不是让重试一直被拒。
            receiver
                .flush_if_dirty()
                .map_err(|error| error.to_string())?;
            // Recovery can outlive the sending API, but cannot invent or modify its durable intent.
            // 恢复可以晚于发送 API，但不能创建或修改它的可靠意图。
            let recorded = source
                .read(|document| {
                    document
                        .collaboration_state()
                        .outbound_task_messages
                        .get(&intent.delivery_id)
                        .cloned()
                })
                .map_err(|error| error.to_string())?
                .ok_or_else(|| String::from("task delivery has no durable source intent"))?;
            if recorded.message != intent || intent.source.session_id != source_id.to_string() {
                return Err(String::from(
                    "task delivery conflicts with the durable source intent",
                ));
            }
            let previous = receiver
                .read(|document| {
                    let acceptance = document
                        .collaboration_state()
                        .inbound_task_messages
                        .get(&intent.delivery_id)
                        .cloned();
                    if acceptance
                        .as_ref()
                        .is_some_and(|record| record.target_revision > document.durable_revision())
                    {
                        return Err(String::from(
                            "task target acceptance is not durably published",
                        ));
                    }
                    Ok(acceptance)
                })
                .map_err(|error| error.to_string())??;
            if let Some(previous) = previous {
                if previous.message != intent {
                    return Err(String::from(
                        "task delivery conflicts with target acceptance",
                    ));
                }
                // Copying an existing receipt performs no new delivery or inference after revocation.
                // 复制已有回执不会在撤销授权后新增投递或推理。
                return Ok(previous);
            }
            if intent.source.project_id != intent.target.project_id
                && !source_handle_grant_allows(
                    &source,
                    source_id,
                    target_id,
                    session_messages::task_action(intent.kind),
                )?
            {
                return Err(String::from(
                    "task message authorization was revoked before target acceptance",
                ));
            }
            let operation = CollaborationOperationId::try_new(format!(
                "{}:task-message-accept",
                intent.operation_id.as_str()
            ))
            .map_err(|error| error.to_string())?;
            receiver
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        document
                            .accept_task_message(operation, intent, accepted_at_ms)
                            .map(session_change)
                    },
                )
                .map(|receipt| receipt.value)
                .map_err(|error| error.to_string())
        },
    )
    .await?;
    // Wakeup is advisory; its claim independently checks the source grant and target control again.
    // 唤醒仅是提示；认领会独立再次核验来源授权和目标控制。
    if message.kind == CollaborationTaskMessageKind::TaskStart {
        task_launch::schedule_task_wakeup(app, invocation, target_id)?;
    }
    // The operation derives from the original intent so live retries and restart repair converge.
    // 操作从原始意图派生，使实时重试和重启修复收敛。
    let acknowledgement = CollaborationOperationId::try_new(format!(
        "{}:task-message-accepted",
        message.operation_id.as_str()
    ))
    .map_err(|error| error.to_string())?;
    let acknowledged_at_ms = now_ms()?;
    mutate_source(invocation, move |document| {
        document.acknowledge_task_message(acknowledgement, acceptance, acknowledged_at_ms)
    })
    .await
    .map(|receipt| receipt.value.record)
}

/// Repairs every unacknowledged task intent owned by `invocation` without replaying the model tool.
/// 不重放模型工具，修复 `invocation` 拥有的所有未确认任务意图。
/// Returns the repaired count, or an aggregate error after attempting every independent delivery.
/// 返回修复数量；独立投递均尝试后如有失败则返回汇总错误。
pub(super) async fn reconcile_task_message_outbox(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
) -> Result<u64, String> {
    // A snapshot is only an iteration list; each write rechecks the canonical immutable intent.
    // 快照只作为遍历列表；每次写入重新核验规范不可变意图。
    let pending = invocation
        .source_handle
        .read(|document| {
            document
                .collaboration_state()
                .outbound_task_messages
                .values()
                .filter(|record| record.acceptance.is_none())
                .map(|record| record.message.clone())
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    let mut repaired = 0;
    let mut failures = Vec::new();
    for message in pending {
        let delivery = message.delivery_id.clone();
        match deliver_recorded_task_message(app, invocation, message).await {
            Ok(_) => repaired += 1,
            Err(error) => failures.push(format!("delivery {}: {error}", delivery.as_str())),
        }
    }
    if failures.is_empty() {
        Ok(repaired)
    } else {
        Err(format!(
            "task outbox repair completed with {} unresolved item(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}
