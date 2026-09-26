//! Dedicated auxiliary report settlement, isolated from business task completion.
//! 专用辅助报告结算，与业务任务完成隔离。

use super::*;
use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange, SessionMutationReceipt,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationOperationId, CollaborationOperationSemantic,
    CollaborationQuestionExecutionOutcome, CollaborationQuestionExecutionState,
    CollaborationRunControl, CompleteQuestionExecutionInput,
};
use vulcan_session_core::v1::{
    ApiCallStatus, ApiGroupCompletionReceipt, ApiPurposeKind, EventBody, IdempotentMutation,
};

/// Settles one exact question stream after its physical task owner has been removed and retained.
/// 物理任务所有者移除并保留后，结算一个精确答疑流。
/// `app` publishes derived phase/usage; `state`, `scope` and `owner` retain the original admitted execution.
/// `app` 发布派生阶段及用量；`state`、`scope` 和 `owner` 保留原已准入执行。
/// `report` and `api_terminal` describe the real final request; `business_turn_completed` must be false.
/// `report` 和 `api_terminal` 描述真实最终请求；`business_turn_completed` 必须为 false。
/// Returns whether durable auxiliary authority was settled; failure keeps the exact recovery owner closed.
/// 返回辅助权威是否已可靠结算；失败时保持精确恢复所有者封闭。
pub(crate) async fn handle_question_finished_report(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    report: ExecutionRunReport,
    scope: TerminalSettlementScope,
    api_terminal: Option<SessionMutationReceipt<ApiGroupCompletionReceipt>>,
    business_turn_completed: bool,
    owner: SessionTerminalPersistenceOwnerGuard,
) -> bool {
    // The captured scope cannot be inferred from report preparation, which may be absent on failure.
    // 不能从报告准备信息推断捕获范围，因为失败时准备信息可能为空。
    let TerminalSettlementScope::QuestionExecution { execution_id, .. } = &scope else {
        tracing::warn!("[webui-service] auxiliary finish rejected a business scope");
        return false;
    };
    // Use the actual report session for every state and persistence operation.
    // 每次状态及持久化操作均使用实际报告会话。
    let session_id = report.session_id;
    // Keep a provider failure primary; a projection error is the failure only when the provider succeeded.
    // 优先保留供应商失败；仅供应商成功时才把投影错误作为失败原因。
    let projection_error =
        crate::execution_report_memory::apply_question_execution_report_to_memory(state, &report)
            .err();
    // A QA report must never carry a business completion receipt even if the provider returned text.
    // 即使供应商返回文本，答疑报告也绝不能携带业务完成回执。
    let mut terminal_error = report.error.clone().or(projection_error);
    if business_turn_completed {
        terminal_error = Some("A question stream unexpectedly completed a business turn.".into());
    }
    // Only a successfully persisted execution completion, not ApiStats alone, releases QA ownership.
    // 只有成功持久化的执行完成才释放答疑所有权，单独 ApiStats 不足以授权。
    let completed = if terminal_error.is_none() {
        match persist_successful_question_execution(
            state,
            session_id,
            &scope,
            api_terminal.as_ref(),
        )
        .await
        {
            Ok(()) => {
                match persist_question_queued_successor(state, session_id, execution_id).await {
                    Ok(()) => true,
                    Err(error) => {
                        terminal_error = Some(error);
                        false
                    }
                }
            }
            Err(error) => {
                terminal_error = Some(error);
                false
            }
        }
    } else {
        false
    };
    // Retire only an auxiliary turn's browser grant; an in-turn answer retains its business owner's grant.
    // 只退休辅助回合的浏览器授权；回合内回答保留业务所有者授权。
    if let Err(error) = retire_auxiliary_browser_connection(state, session_id, execution_id).await {
        tracing::warn!(
            "[webui-service] auxiliary browser retirement failed: session={session_id} error={error}"
        );
    }
    // Restore business presentation while this exact owner still blocks replacement admission.
    // 精确所有者仍阻止替代准入时恢复业务展示。
    // The auxiliary card keeps its own durable outcome; restoring the old task's phase does not settle QA.
    // 辅助交互框保留独立可靠结果；恢复旧任务阶段不代表答疑已经结算。
    let phase = registered_session_v1_handle_from_state(state, session_id).and_then(|handle| {
        handle
            .read(|document| question_phase_after_finish(document.collaboration_run_control()))
            .map_err(|error| error.to_string())
    });
    match phase {
        Ok(phase) => {
            if let Err(error) = apply_session_phase(app, state, session_id, phase) {
                tracing::warn!(
                    "[webui-service] auxiliary phase restoration failed: session={session_id} error={error}"
                );
            }
        }
        Err(error) => tracing::warn!(
            "[webui-service] auxiliary run control unavailable: session={session_id} error={error}"
        ),
    }
    // Failed writes use the same exact intent/recovery protocol, preserving an already committed success.
    // 写入失败使用相同精确意图及恢复协议，保留已经提交的成功事实。
    let settled = settle_stream_terminal_authority_with_scope(
        state,
        session_id,
        completed,
        "question_stream_finished_without_execution_terminal",
        terminal_error,
        owner,
        scope,
    )
    .await;
    if !settled {
        return false;
    }
    if let Err(error) = emit_session_addons_changed_delta(app, state, session_id) {
        tracing::warn!(
            "[webui-service] auxiliary usage projection failed: session={session_id} error={error}"
        );
    }
    true
}

/// Restores a phase from durable business run control without changing that control or resuming a Goal.
/// 从可靠业务运行控制恢复阶段，不改变该控制或恢复 Goal。
/// `control` is read before releasing ownership; the returned preparing phase represents the retained unfinished task.
/// `control` 在释放所有权前读取；返回的准备阶段表示保留的未完成任务。
fn question_phase_after_finish(control: CollaborationRunControl) -> SessionRunPhase {
    match control {
        CollaborationRunControl::NaturallyIdle => SessionRunPhase::Ready,
        CollaborationRunControl::Running => SessionRunPhase::Preparing,
        CollaborationRunControl::MainOnly
        | CollaborationRunControl::Stopped
        | CollaborationRunControl::MainAndSubagents => SessionRunPhase::Stopped,
    }
}

/// Persists one eligible queued successor while the auxiliary terminal owner still prevents replacement admission.
/// 辅助终态所有者仍阻止替代准入时，持久化一个符合条件的队列后继。
/// `state`, `session_id` and `execution_id` locate the exact completed attempt; returns only after the selected task is durable.
/// `state`、`session_id` 和 `execution_id` 定位精确已完成尝试；只在选定任务可靠落盘后返回。
async fn persist_question_queued_successor(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    execution_id: &vulcan_session_core::v1::collaboration::CollaborationExecutionId,
) -> Result<(), String> {
    // The same writer serializes final queue arrival, stop control and post-QA selection.
    // 同一写入器串行化最终队列到达、停止控制及答疑后选择。
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    // Preserve the execution identity across the asynchronous writer boundary.
    // 跨异步写入器边界保留执行身份。
    let execution_id = execution_id.clone();
    // Stable operation identity survives failed publication and process restart.
    // 稳定操作身份跨失败发布及进程重启保留。
    let operation_id = CollaborationOperationId::try_new(format!(
        "question-queued-successor:{}",
        execution_id.as_str()
    ))
    .map_err(|error| error.to_string())?;
    // Scheduling is a distinct event after execution completion.
    // 调度是执行完成之后的独立事件。
    let now_ms = current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                // In-turn answers retain their original business task and cannot choose a new root.
                // 回合内回答保留原业务任务，不能选择新根任务。
                let execution = document
                    .collaboration_state()
                    .question_executions
                    .get(&execution_id)
                    .ok_or_else(|| DomainError::CollaborationNotFound {
                        kind: "question_execution",
                        identity: execution_id.as_str().into(),
                    })?;
                if execution.execution_turn_id.is_none() {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                if execution.queued_successor.is_none()
                    && document.collaboration_run_control()
                        != CollaborationRunControl::NaturallyIdle
                {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                match document.schedule_question_queued_successor(
                    execution_id,
                    operation_id,
                    now_ms,
                )? {
                    Some(IdempotentMutation::Committed(_)) => {
                        Ok(SessionMutationChange::Changed(()))
                    }
                    Some(IdempotentMutation::Idempotent(_)) | None => {
                        Ok(SessionMutationChange::Unchanged(()))
                    }
                }
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Dispatches only the queued business task already selected by the durable auxiliary scheduling receipt.
/// 仅分发可靠辅助调度回执已经选定的排队业务任务。
/// Runtime ports retain existing model and preparation gates; `execution_id` fixes the exact predecessor.
/// 运行时端口保留既有模型及准备关卡；`execution_id` 固定精确前驱。
/// Returns whether dispatch starts, or false for an in-turn answer, empty queue, or stopped successor.
/// 返回是否启动分发；回合内回答、空队列或已停止后继返回 false。
pub(super) async fn start_question_queued_successor(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    execution_id: &vulcan_session_core::v1::collaboration::CollaborationExecutionId,
) -> Result<bool, String> {
    // Read the selected identity from durable authority; never take another queue head here.
    // 从可靠权威读取选定身份；绝不在这里另取队首。
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    // A user stop after settlement prevents automatic dispatch while keeping the saved receipt queryable.
    // 结算后的用户停止阻止自动分发，同时保持已保存回执可查询。
    let successor = handle
        .read(|document| {
            if document.collaboration_run_control() != CollaborationRunControl::Running {
                return Ok(None);
            }
            let execution = document
                .collaboration_state()
                .question_executions
                .get(execution_id)
                .ok_or_else(|| {
                    format!("Question execution {} is missing.", execution_id.as_str())
                })?;
            Ok::<_, String>(execution.queued_successor.clone())
        })
        .map_err(|error| error.to_string())??;
    let Some(successor) = successor else {
        return crate::agent_collaboration::wake_session_tasks_if_idle(
            app,
            state,
            controller_runtime,
            session_id,
        )
        .await;
    };
    // The existing dispatcher validates the exact input event and retains ordinary business preparation.
    // 既有分发器校验精确输入事件，并保留普通业务准备。
    let authority = crate::webui_prompt_lifecycle::queued_dispatch_authority_from_v1_handle(
        handle.as_ref(),
        successor.turn_id,
        successor.input_id,
        &successor.client_action_id,
        successor.input_seq,
    )?;
    crate::start_exact_v1_queued_turn(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority,
    )
    .await
}

/// Commits an optional-answer execution outcome using its exact durable final physical API receipt.
/// 使用精确可靠最终物理 API 回执提交允许无回答的执行结果。
/// `scope` owns the execution and API; `receipt` must prove a successful QA request without promoted business inputs.
/// `scope` 拥有执行及 API；`receipt` 必须证明成功答疑请求且未晋升业务输入。
/// Returns only after the execution outcome is durable; storage failure leaves the normal recovery path available.
/// 仅在执行结果可靠落盘后返回；存储失败时仍可使用正常恢复路径。
async fn persist_successful_question_execution(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    scope: &TerminalSettlementScope,
    receipt: Option<&SessionMutationReceipt<ApiGroupCompletionReceipt>>,
) -> Result<(), String> {
    // Every successful auxiliary completion is correlated to a real permitted and completed API.
    // 每次成功辅助完成都关联真实已许可且已完成的 API。
    let TerminalSettlementScope::QuestionExecution {
        execution_id,
        api_call_id: Some(api_call_id),
    } = scope
    else {
        return Err("The question execution has no captured physical API.".into());
    };
    // The stream sink's immutable receipt is the only physical completion source.
    // 流 sink 的不可变回执是唯一物理完成来源。
    let receipt = receipt.ok_or("The question execution has no durable API terminal receipt.")?;
    if receipt.value.api_call_id != *api_call_id || !receipt.value.promoted_input_seqs.is_empty() {
        return Err(
            "The question API receipt has a different owner or promoted business inputs.".into(),
        );
    }
    // Capture immutable inputs for the single canonical document mutation.
    // 为唯一规范文档变更捕获不可变输入。
    let execution_id = execution_id.clone();
    // The completed API receipt is copied, never reconstructed from current UI state.
    // 完成 API 回执直接复制，绝不从当前 UI 状态重建。
    let api_receipt = receipt.value.clone();
    // The observed revision must have reached durable storage before it can authorize execution completion.
    // 所观察修订必须已可靠落盘，才能授权执行完成。
    let api_revision = receipt.revision;
    // The current timestamp is not part of semantic retry identity.
    // 当前时间戳不属于语义重试身份。
    let now_ms = current_v1_unix_millis()?;
    // The sole registered handle serializes answer settlement and auxiliary completion.
    // 唯一已登记句柄串行化答案结算与辅助完成。
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    handle
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            move |document| {
                // Both durable revision and exact event identity must substantiate the supplied receipt.
                // 可靠修订与精确事件身份必须共同支持传入回执。
                let api_proven = document.durable_revision() >= api_revision
                    && document.events().iter().any(|event| {
                        event.seq == api_receipt.stats_seq
                            && event.turn_id == Some(api_receipt.turn_id)
                            && matches!(&event.event, EventBody::ApiStats(api)
                            if api.api_call_id == api_receipt.api_call_id
                                && api.purpose == ApiPurposeKind::QuestionExecution
                                && api.status == ApiCallStatus::Completed)
                    });
                if !api_proven {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "question_execution.api_terminal_proof",
                    });
                }
                // Completion retries retain the immutable begin receipt even after the current record changes.
                // 即使当前记录已经变化，完成重试仍保留不可变启动回执。
                let begin = document.collaboration_state().operations.values().find(|operation| {
                    matches!(&operation.semantic,
                        CollaborationOperationSemantic::BeginQuestionExecution { execution_id: started, .. }
                        if started == &execution_id)
                }).ok_or(DomainError::CollaborationStateViolation {
                    field: "question_execution.begin_proof",
                })?;
                // A prior successful API cannot authorize a later execution or bypass a newer request.
                // 先前成功 API 不能授权后续执行，也不能绕过更新请求。
                let last_start = document.events().iter().rev().find(|event| {
                    event.turn_id == Some(api_receipt.turn_id)
                        && matches!(event.event, EventBody::ApiStarted(_))
                });
                if !last_start.is_some_and(|start| {
                    start.seq > begin.event_seq && start.seq < api_receipt.stats_seq
                        && matches!(&start.event, EventBody::ApiStarted(api)
                            if api.api_call_id == api_receipt.api_call_id
                                && api.purpose == ApiPurposeKind::QuestionExecution)
                }) {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "question_execution.last_api_proof",
                    });
                }
                // The execution, not a latest-question search, fixes the question and logical turn.
                // 由执行固定问题及逻辑回合，不查找最近问题。
                let execution = document
                    .collaboration_state()
                    .question_executions
                    .get(&execution_id)
                    .ok_or_else(|| DomainError::CollaborationNotFound {
                        kind: "question_execution",
                        identity: execution_id.as_str().into(),
                    })?;
                if execution.execution_turn_id.or(execution.parent_turn_id)
                    != Some(api_receipt.turn_id)
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "question_execution.api_turn",
                    });
                }
                // Once committed, an exact retry preserves the original result even if a late answer arrives.
                // 一旦提交，精确重试保留原始结果，即使随后答案到达。
                let outcome = if execution.state == CollaborationQuestionExecutionState::Completed {
                    execution
                        .outcome
                        .ok_or(DomainError::CollaborationStateViolation {
                            field: "question_execution.terminal_outcome",
                        })?
                } else if document
                    .collaboration_state()
                    .inbound_deliveries
                    .values()
                    .any(|delivery| {
                        delivery.question_id == execution.question_id
                            && delivery.settlement.is_some()
                    })
                {
                    CollaborationQuestionExecutionOutcome::Settled
                } else {
                    CollaborationQuestionExecutionOutcome::Unanswered
                };
                // A stable execution-specific operation makes a lost publication receipt replayable.
                // 稳定的执行专属操作使丢失的发布回执可重放。
                let result =
                    document.complete_question_execution(CompleteQuestionExecutionInput {
                        operation_id: CollaborationOperationId::try_new(format!(
                            "question-stream-complete:{}",
                            execution_id.as_str()
                        ))?,
                        execution_id,
                        outcome,
                        completed_at_ms: now_ms,
                    })?;
                Ok(match result {
                    IdempotentMutation::Committed(_) => SessionMutationChange::Changed(()),
                    IdempotentMutation::Idempotent(_) => SessionMutationChange::Unchanged(()),
                })
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Retires browser permissions only for the standalone auxiliary turn owned by `execution_id`.
/// 仅退休 `execution_id` 拥有的独立辅助回合浏览器权限。
/// `state` and `session_id` locate the sole execution record; retained parent turns keep their current connection.
/// `state` 及 `session_id` 定位唯一执行记录；保留父回合维持当前连接。
/// Returns after cleanup or immediately for an in-turn answer, without changing any task ownership.
/// 清理后或回合内回答直接返回，不改变任何任务归属。
async fn retire_auxiliary_browser_connection(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    execution_id: &vulcan_session_core::v1::collaboration::CollaborationExecutionId,
) -> Result<(), String> {
    // The persisted execution supplies whether a new auxiliary turn was actually created.
    // 持久执行提供是否实际创建新辅助回合的事实。
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    // Read only the exact execution; never guess from current phase or the last closed turn.
    // 仅读取精确执行，绝不从当前阶段或最近闭合回合推测。
    let auxiliary = handle
        .read(|document| {
            document
                .collaboration_state()
                .question_executions
                .get(execution_id)
                .map(|execution| execution.execution_turn_id.is_some())
                .ok_or("The captured question execution no longer exists.")
        })
        .map_err(|error| error.to_string())??;
    if auxiliary {
        // Clone the manager before asynchronous cleanup so no service lock crosses the await.
        // 异步清理前克隆管理器，不跨 await 持有服务锁。
        let manager = state
            .lock()
            .map_err(|error| error.to_string())?
            .browser_manager
            .clone();
        manager.retire_ai_connection_after_turn(session_id).await?;
    }
    Ok(())
}
