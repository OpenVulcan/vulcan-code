//! Durable queue admission and asynchronous startup of independent session tasks.
//! 独立会话任务的可靠队列准入与异步启动。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationTaskMessageAcceptance, CollaborationTaskMessageEffect,
    CollaborationTaskMessageEffectKind, CollaborationTaskMessageKind,
};

/// Preserves the Core revision decision while wrapping an admitted effect as an optional result.
/// 将准入效果包装为可选结果时，保留 Core 的修订判定。
fn optional_effect(
    effect: IdempotentMutation<CollaborationTaskMessageEffect>,
) -> SessionMutationChange<Option<CollaborationTaskMessageEffect>> {
    match effect {
        IdempotentMutation::Committed(effect) => SessionMutationChange::Changed(Some(effect)),
        IdempotentMutation::Idempotent(effect) => SessionMutationChange::Unchanged(Some(effect)),
    }
}

/// Rechecks the durable task queue after a business or auxiliary execution released its owner.
/// 在业务或辅助执行释放所有者后，重新检查可靠任务队列。
/// Uses `target_id` and the host registry; no completed model ToolCall is reused for authorization.
/// 使用 `target_id` 与宿主注册表；不复用已结束模型工具调用的授权。
pub(crate) async fn wake_session_tasks_if_idle(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &vulcan_workspace_store::data_plane::HostRuntimeHandle,
    target_id: Uuid,
) -> Result<bool, String> {
    let target =
        crate::session_goal_lifecycle::session_v1_handle_from_state(state, target_id).await?;
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    let invocation = recovery_invocation(
        state.clone(),
        controller_runtime.clone(),
        registry,
        target_id,
        target,
    )?;
    wake_session_task(app, &invocation, target_id).await
}

/// Returns the oldest pending task, or the exact undispatched claim already owning this session.
/// 返回最早待执行任务，或已拥有此会话的精确未分发认领。
/// `handle` is the canonical target; no state is changed by selection.
/// `handle` 是规范目标；选择不改变任何状态。
fn next_task_candidate(
    handle: &SessionHandle,
) -> Result<Option<CollaborationTaskMessageAcceptance>, String> {
    handle.read(|document| {
        // An existing claim can be retried only while its own business turn remains open.
        // 只有原业务回合仍开放时，现有认领才能重试。
        let state = document.collaboration_state();
        if let Some(turn) = document.active_turn().filter(|turn| !turn.closed) {
            if turn.origin != TurnOrigin::AgentTask || document.active_api().is_some()
                || document.events().iter().any(|event| event.turn_id == Some(turn.turn_id)
                    && matches!(&event.event, EventBody::ApiStarted(started)
                        if !matches!(started.purpose,
                            ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))) {
                return None;
            }
            return state.task_message_effects.values().find_map(|effect| {
                matches!(&effect.outcome, CollaborationTaskMessageEffectKind::Claimed { turn_id, .. }
                    if *turn_id == turn.turn_id)
                    .then(|| state.inbound_task_messages.get(&effect.delivery_id).cloned())
                    .flatten()
            });
        }
        state.inbound_task_messages.values()
            .filter(|acceptance| acceptance.message.kind == CollaborationTaskMessageKind::TaskStart
                && !state.task_message_effects.contains_key(&acceptance.message.delivery_id))
            .min_by_key(|acceptance| acceptance.event_seq)
            .cloned()
    }).map_err(|error| error.to_string())
}

/// Claims `acceptance` under current source authorization and both canonical lifecycle owners.
/// 在当前来源授权及双方规范生命周期所有权下认领 `acceptance`。
/// Returns a durable effect, or None while human input, another execution or service admission owns the target.
/// 返回可靠效果；用户输入、其他执行或服务准入占用目标时返回 None。
pub(super) async fn claim_task_candidate(
    invocation: &ValidatedInvocation,
    target: &Arc<SessionHandle>,
    acceptance: CollaborationTaskMessageAcceptance,
) -> Result<Option<CollaborationTaskMessageEffect>, String> {
    // Stop and competing root submissions cannot pass the actual blocking claim writer.
    // 停止与竞争根提交不能越过实际阻塞认领写入器。
    let Some(admission) =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &invocation.state,
            target.session_id(),
        )?
    else {
        return Ok(None);
    };
    let source =
        participants::load_participant_handle(invocation, &acceptance.message.source).await?;
    let source_for_write = source.clone();
    let target_for_write = target.clone();
    let claimed_at_ms = now_ms()?;
    lifecycle_write::with_canonical_handles(
        &invocation.registry, vec![source, target.clone()], move || {
            // Retain this admission inside the writer even if its async waiter is cancelled.
            // 即使异步等待者取消，也在写入器内保留本次准入。
            let _admission = admission;
            let original = source_for_write.read(|document| {
                document.collaboration_state().outbound_task_messages
                    .get(&acceptance.message.delivery_id).cloned()
            }).map_err(|error| error.to_string())?;
            let source_matches = original.is_some_and(|record| record.message == acceptance.message);
            let allowed = acceptance.message.source.project_id == acceptance.message.target.project_id
                || source_handle_grant_allows(&source_for_write, source_for_write.session_id(),
                    target_for_write.session_id(), CollaborationGrantAction::Start)?;
            let operation_id = CollaborationOperationId::try_new(format!("{}:task-message-effect",
                acceptance.message.delivery_id.as_str())).map_err(|error| error.to_string())?;
            // RootTurn is mandatory: shutdown rejects new inference ownership even before run-control changes.
            // 必须使用 RootTurn：即使运行控制尚未改变，关闭也会拒绝新的推理所有权。
            target_for_write.mutate_durable_if_changed(
                SessionMutationAdmission::RootTurn, SessionFlushReason::MetadataChanged, move |document| {
                    if acceptance.message.kind != CollaborationTaskMessageKind::TaskStart
                        || document.collaboration_state().inbound_task_messages.get(&acceptance.message.delivery_id) != Some(&acceptance) {
                        return Err(vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                            field: "collaboration.task_message.claim_acceptance",
                        });
                    }
                    if let Some(effect) = document.collaboration_state().task_message_effects
                        .get(&acceptance.message.delivery_id) {
                        return Ok(SessionMutationChange::Unchanged(Some(effect.clone())));
                    }
                    let failure = if !source_matches {
                        Some(CollaborationDeliveryFailureReason::InvalidBinding)
                    } else if !allowed {
                        Some(CollaborationDeliveryFailureReason::AuthorizationRevoked)
                    } else if document.metadata.archived_at_ms.is_some() || matches!(document.collaboration_run_control(),
                        CollaborationRunControl::Stopped | CollaborationRunControl::MainOnly | CollaborationRunControl::MainAndSubagents) {
                        Some(CollaborationDeliveryFailureReason::TargetStopped)
                    } else { None };
                    if let Some(reason) = failure {
                        return document.fail_task_message(operation_id, acceptance.message.delivery_id,
                            reason, claimed_at_ms).map(optional_effect);
                    }
                    if document.active_turn().is_some_and(|turn| !turn.closed)
                        || document.active_api().is_some()
                        || !document.metadata.input_state.queued_inputs.is_empty()
                        || document.collaboration_state().question_executions.values()
                            .any(|execution| execution.state == CollaborationQuestionExecutionState::Running) {
                        return Ok(SessionMutationChange::Unchanged(None));
                    }
                    document.claim_task_message_before_api(operation_id, acceptance.message.delivery_id,
                        claimed_at_ms).map(optional_effect)
                },
            ).map(|receipt| receipt.value).map_err(|error| error.to_string())
        },
    ).await
}

/// Schedules a nonblocking wakeup after reliable queue acceptance; errors remain visible diagnostics.
/// 可靠队列接纳后调度非阻塞唤醒；错误保留为可见诊断。
/// `target_id` names the existing independent session, not a newly created child.
/// `target_id` 指向已有独立会话，而不是新创建的子代理。
pub(super) fn schedule_task_wakeup(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    target_id: Uuid,
) -> Result<(), String> {
    let owner = dispatch_owner::DispatchOwner::acquire(&invocation.state, target_id)?;
    let app = app.clone();
    let invocation = invocation.clone();
    crate::spawn_service_task(async move {
        let _owner = owner;
        if let Err(error) = wake_session_task(&app, &invocation, target_id).await {
            tracing::warn!(%target_id, %error, "Agent task wakeup failed");
            if let Err(diagnostic_error) = crate::push_webui_service_diagnostic(
                &invocation.state,
                format!("Failed to start agent task session {target_id}: {error}"),
            ) {
                tracing::warn!(%diagnostic_error, "Agent task diagnostic publication failed");
            }
        }
    });
    Ok(())
}

/// Starts the oldest accepted task when its target has no competing business owner.
/// 在目标没有竞争业务所有者时启动最早接纳任务。
/// Returns whether the production preparation pipeline started; busy targets retain their queue.
/// 返回生产准备链是否启动；繁忙目标保留队列。
async fn wake_session_task(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    target_id: Uuid,
) -> Result<bool, String> {
    let _dispatch_owner = dispatch_owner::DispatchOwner::acquire(&invocation.state, target_id)?;
    let target =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&invocation.state, target_id)
            .await?;
    loop {
        let Some(candidate) = next_task_candidate(&target)? else {
            return Ok(false);
        };
        // Empty queues need no model runtime; real candidates require readiness before admission.
        // 空队列不需要模型运行时；真实候选必须在准入前确认就绪。
        let runtime = invocation
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .agent_runtime
            .clone()
            .ok_or_else(|| {
                String::from("Agent task dispatch requires an initialized agent runtime.")
            })?;
        let Some(effect) = claim_task_candidate(invocation, &target, candidate).await? else {
            return Ok(false);
        };
        if matches!(
            effect.outcome,
            CollaborationTaskMessageEffectKind::Failed { .. }
        ) {
            if let Err(error) =
                reconcile_task_results_for_session(&invocation.state, target_id).await
            {
                tracing::warn!(%target_id, %error, "Task application failure return remains pending");
            }
            continue;
        }
        let preparation = async {
            let authority =
                crate::webui_prompt_lifecycle::agent_task_dispatch_authority_from_v1_handle(
                    &target,
                    &effect.delivery_id,
                )?;
            crate::webui_prompt_lifecycle::maybe_prepare_webui_agent_task(
                app,
                &invocation.state,
                &invocation.controller_runtime,
                &runtime,
                target_id,
                authority,
            )
            .await
        }
        .await;
        if let Err(error) = preparation {
            // Before kernel registration, the durable claim still needs an exact terminal owner.
            // 在内核登记之前，可靠认领仍需精确终态所有者。
            let settled = settle_undispatched_task_claim(invocation, &target, effect)
                .await
                .map_err(|settlement_error| {
                    format!("{error}; agent task claim settlement failed: {settlement_error}")
                })?;
            if settled {
                schedule_task_wakeup(app, invocation, target_id)?;
            }
            // Kernel-owned preparation failure may already have closed the turn even when our cleanup did not.
            // 内核持有的准备失败可能已闭合回合，即使本次清理没有再次修改。
            if let Err(delivery_error) =
                reconcile_task_results_for_session(&invocation.state, target_id).await
            {
                tracing::warn!(%target_id, %delivery_error, "Preparation failure result return remains pending");
            }
            return Err(error);
        }
        return preparation;
    }
}

/// Settles only `effect`'s current, never-dispatched business turn after preparation fails.
/// 准备失败后，仅结算 `effect` 当前持有且从未分发的业务回合。
/// `target` must remain canonical; competing owners or already-ended turns return false unchanged.
/// `target` 必须保持规范归属；竞争所有者或已结束回合返回 false 且不修改状态。
pub(super) async fn settle_undispatched_task_claim(
    invocation: &ValidatedInvocation,
    target: &Arc<SessionHandle>,
    effect: CollaborationTaskMessageEffect,
) -> Result<bool, String> {
    let Some(admission) =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &invocation.state,
            target.session_id(),
        )?
    else {
        return Ok(false);
    };
    let target_for_write = target.clone();
    let failed_at_ms = now_ms()?;
    lifecycle_write::with_canonical_handles(&invocation.registry, vec![target.clone()], move || {
        // The writer owns exclusion until publication, even if the awaiting caller is cancelled.
        // 即使等待调用者取消，写入器也持有排他权直到发布完成。
        let _admission = admission;
        target_for_write
            .mutate_durable_if_changed(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                move |document| {
                    let CollaborationTaskMessageEffectKind::Claimed { turn_id, .. } =
                        &effect.outcome
                    else {
                        return Ok(SessionMutationChange::Unchanged(false));
                    };
                    if document
                        .collaboration_state()
                        .task_message_effects
                        .get(&effect.delivery_id)
                        != Some(&effect)
                        || !document.active_turn().is_some_and(|turn| {
                            !turn.closed
                                && turn.origin == TurnOrigin::AgentTask
                                && turn.turn_id == *turn_id
                                && turn.start_seq == effect.event_seq
                        })
                        || document.active_api().is_some()
                        || document.events().iter().any(|event| {
                            event.turn_id == Some(*turn_id)
                                // Preparation can still fail before business dispatch after auxiliary work.
                                // 辅助工作之后、业务分发之前，准备仍然可能失败。
                                && matches!(&event.event, EventBody::ApiStarted(started)
                                    if !matches!(started.purpose,
                                        ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                        })
                        || document
                            .collaboration_state()
                            .question_executions
                            .values()
                            .any(|execution| {
                                execution.state == CollaborationQuestionExecutionState::Running
                            })
                    {
                        return Ok(SessionMutationChange::Unchanged(false));
                    }
                    // Persist a fixed diagnostic; raw hydration/provider errors can contain private data.
                    // 持久化固定诊断；原始水合或供应商错误可能包含隐私数据。
                    document
                        .fail_turn_without_active_api(
                            "agent_task_preparation_failed",
                            Some("Agent task preparation failed before its first API attempt."),
                            failed_at_ms,
                        )
                        .map(|changed| {
                            if changed {
                                SessionMutationChange::Changed(true)
                            } else {
                                SessionMutationChange::Unchanged(false)
                            }
                        })
                },
            )
            .map(|receipt| receipt.value)
            .map_err(|error| error.to_string())
    })
    .await
}
