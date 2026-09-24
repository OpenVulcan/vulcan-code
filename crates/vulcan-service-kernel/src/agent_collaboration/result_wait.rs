//! Unified inbox readiness and live execution observation.
//! 统一收件就绪与实时执行观测。

use super::*;
use std::collections::BTreeSet;
use vulcan_agent_runtime::tooling::collaboration::WAIT_AGENTS_RESULT_TOOL_ID;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskState, CollaborationState, CollaborationTaskMessageEffectKind,
};

/// One source-owned obligation, keyed by immutable work identity instead of reusable session name.
/// 一条来源拥有的工作，按不可变工作身份关联，而非可复用会话名称。
#[derive(Clone, PartialEq, Eq)]
struct PendingWork {
    /// Stable reply correlation supplied by canonical source facts.
    /// 规范来源事实提供的稳定回复关联。
    id: String,
    /// Exact recipient scope, including generation and root ownership.
    /// 精确接收方范围，包含代次与根归属。
    target: CollaborationParticipant,
    /// Exact source record used to inspect the corresponding target execution.
    /// 用于检查对应目标执行的精确来源记录。
    kind: WorkKind,
}

/// Retains typed identities so another turn on the same session cannot satisfy this wait.
/// 保留强类型身份，避免同一会话的另一回合满足本次等待。
#[derive(Clone, PartialEq, Eq)]
enum WorkKind {
    /// Question identity owned by the caller.
    /// 调用方拥有的问题身份。
    Question(CollaborationQuestionId),
    /// Immutable native child execution.
    /// 不可变原生子执行。
    Child(vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord),
    /// Accepted queue or steering identity.
    /// 已接纳排队或引导身份。
    Task(vulcan_session_core::v1::collaboration::CollaborationDeliveryId),
}

/// Selects outstanding questions, delegated executions and queued cross-session tasks.
/// 选择未结问题、委派执行与跨会话排队任务。
/// Notifications never create a reply obligation; completed work is delivered by the inbox path.
/// 通知绝不创建回复义务；已完成工作通过收件链路投递。
fn pending_work(state: &CollaborationState) -> Vec<PendingWork> {
    let mut pending = Vec::new();
    for question in state.questions.values() {
        if question.state == CollaborationQuestionState::Pending
            && state
                .outbound_deliveries
                .get(&question.delivery_id)
                .is_some_and(|delivery| delivery.state != CollaborationDeliveryState::Failed)
        {
            pending.push(PendingWork {
                id: format!("question:{}", question.question_id.as_str()),
                target: question.target.clone(),
                kind: WorkKind::Question(question.question_id.clone()),
            });
        }
    }
    for task in state.child_tasks.values() {
        if task.state != CollaborationChildTaskState::Terminal {
            pending.push(PendingWork {
                id: format!("child:{}", task.execution_id.as_str()),
                target: task.child.clone(),
                kind: WorkKind::Child(task.clone()),
            });
        }
    }
    for task in state.outbound_task_messages.values() {
        if !state
            .received_task_results
            .contains_key(&task.message.delivery_id)
        {
            pending.push(PendingWork {
                id: format!("task:{}", task.message.delivery_id.as_str()),
                target: task.message.target.clone(),
                kind: WorkKind::Task(task.message.delivery_id.clone()),
            });
        }
    }
    pending
}

/// Reads report identities only from successfully persisted native wait outputs.
/// 仅从成功持久化的原生等待输出读取报告身份。
/// Failed or cancelled invocations cannot acknowledge a report; no state is consumed by waiting.
/// 失败或取消的调用不能确认报告；等待本身不消费状态。
fn reported_work(
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> BTreeSet<String> {
    let mut reported = BTreeSet::new();
    for event in document.events() {
        let EventBody::ToolCall(tool) = &event.event else {
            continue;
        };
        if tool.tool_name != WAIT_AGENTS_RESULT_TOOL_ID
            || tool.status != vulcan_session_core::v1::event::ToolCallStatus::Completed
        {
            continue;
        }
        let Some(output) = &tool.output else { continue };
        let Ok(output) = serde_json::to_value(output) else {
            continue;
        };
        if output.get("is_error").and_then(Value::as_bool) != Some(false)
            || output.get("tool_name").and_then(Value::as_str) != Some(WAIT_AGENTS_RESULT_TOOL_ID)
        {
            continue;
        }
        let Some(text) = output.get("model_output").and_then(Value::as_str) else {
            continue;
        };
        let Ok(snapshot) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if let Some(reports) = snapshot.get("reports").and_then(Value::as_array) {
            for report in reports {
                if let Some(id) = report.get("work_id").and_then(Value::as_str) {
                    reported.insert(id.to_owned());
                }
            }
        }
    }
    reported
}

/// Checks all process-owned preparation, execution and terminal-publication phases.
/// 检查进程拥有的准备、执行与终态发布阶段。
/// Historical open turns are deliberately excluded; only actual owners make a wait useful.
/// 刻意排除历史开放回合；只有实际所有者让等待有意义。
fn has_live_owner(invocation: &ValidatedInvocation, session_id: Uuid) -> Result<bool, String> {
    let runtime = {
        let state = invocation.state.lock().map_err(|error| error.to_string())?;
        if state
            .session_terminal_persistence_owners
            .contains_key(&session_id)
            || state
                .active_stream_tasks
                .get(&session_id)
                .is_some_and(|task| task.is_running())
            || state
                .question_preparation_tasks
                .get(&session_id)
                .is_some_and(|task| task.task.is_running())
            || state
                .session_turn_admission_reservations
                .contains_key(&session_id)
            || state
                .collaboration_dispatch_owners
                .contains_key(&session_id)
            || state
                .detached_subagent_task_listeners
                .contains_key(&session_id)
        {
            return Ok(true);
        }
        state.agent_runtime.clone()
    };
    match runtime {
        Some(runtime) => runtime.task_runtime.is_task_running(session_id),
        None => Ok(false),
    }
}

/// Keeps exact child terminal publication live, without borrowing a later execution on the same child.
/// 保持精确子执行的终态发布存活，不借用同一子会话的后续执行。
/// `record` supplies immutable execution identity; `eligible` permits only its pending startup handoff.
/// `record` 提供不可变执行身份；`eligible` 只允许属于它的待启动交接。
fn has_live_child_owner(
    invocation: &ValidatedInvocation,
    record: &vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord,
    eligible: bool,
) -> Result<bool, String> {
    let session_id =
        Uuid::parse_str(&record.child.session_id).map_err(|error| error.to_string())?;
    let runtime = {
        let state = invocation.state.lock().map_err(|error| error.to_string())?;
        if state
            .detached_subagent_task_listeners
            .get(&session_id)
            .is_some_and(|id| id == record.execution_id.as_str())
            || (eligible
                && state
                    .collaboration_dispatch_owners
                    .contains_key(&session_id))
        {
            return Ok(true);
        }
        state.agent_runtime.clone()
    };
    match runtime {
        Some(runtime) => runtime
            .task_runtime
            .is_task_execution_running(session_id, record.execution_id.as_str()),
        None => Ok(false),
    }
}

/// Builds explicit source-owned failure reports, without presenting delivery changes as answers.
/// 构建明确的来源侧失败报告，不将投递变化冒充答案。
/// Returns only settlements not already delivered by a successful unified wait.
/// 仅返回尚未通过成功统一等待交付的结算。
fn failed_questions(state: &CollaborationState, reported: &BTreeSet<String>) -> Vec<Value> {
    state.questions.values().filter_map(|question| {
        let id = format!("question:{}", question.question_id.as_str());
        if reported.contains(&id) { return None; }
        let delivery = state.outbound_deliveries.get(&question.delivery_id)?;
        let answer_failed = question.answer.as_ref().is_some_and(|answer|
            answer.delivery.state == vulcan_session_core::v1::collaboration::CollaborationAnswerDeliveryState::Failed);
        if question.state == CollaborationQuestionState::Cancelled || delivery.state == CollaborationDeliveryState::Failed || answer_failed {
            Some(json!({"work_id": id, "session_id": question.target.session_id,
                "question_id": question.question_id, "outcome": if question.state == CollaborationQuestionState::Cancelled { "cancelled" }
                else if delivery.failure == Some(CollaborationDeliveryFailureReason::TargetStopped) { "stopped" } else { "failed" },
                "reason": if answer_failed { json!("answer_delivery_failed") } else { json!(delivery.failure) }}))
        } else { None }
    }).collect()
}

/// Resolves the exact bound turn; queued work remains eligible while a physical owner can drain it.
/// 解析精确绑定回合；只要物理所有者能够处理队列，排队工作仍可等待。
/// Returns false for a settled original execution even when another turn now owns the session.
/// 原执行已结算时返回假，即使现在有另一回合占用该会话。
fn work_can_run(
    work: &PendingWork,
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> bool {
    let state = document.collaboration_state();
    let bound_turn = match &work.kind {
        WorkKind::Question(question_id) => {
            if let Some(execution) = state
                .question_executions
                .values()
                .find(|execution| &execution.question_id == question_id)
            {
                if execution.state != CollaborationQuestionExecutionState::Running {
                    return false;
                }
                match execution.purpose {
                    CollaborationExecutionPurpose::NaturallyIdleQuestion => {
                        execution.execution_turn_id
                    }
                    CollaborationExecutionPurpose::InTurnQuestion => execution.parent_turn_id,
                    CollaborationExecutionPurpose::NormalTask => return false,
                }
            } else {
                let Some(inbound) = state
                    .inbound_deliveries
                    .values()
                    .find(|inbound| &inbound.question_id == question_id)
                else {
                    return false;
                };
                if inbound.state == CollaborationDeliveryState::Failed
                    || inbound.settlement.is_some()
                {
                    return false;
                }
                inbound.bound_turn_id
            }
        }
        WorkKind::Task(delivery_id) => match state
            .task_message_effects
            .get(delivery_id)
            .map(|effect| &effect.outcome)
        {
            Some(
                CollaborationTaskMessageEffectKind::Claimed { turn_id, .. }
                | CollaborationTaskMessageEffectKind::Steered { turn_id, .. },
            ) => Some(*turn_id),
            Some(CollaborationTaskMessageEffectKind::Failed { .. }) => return false,
            None => return true,
        },
        WorkKind::Child(record) => document
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(input)
                    if input.input_source
                        == vulcan_session_core::v1::event::UserInputSource::Root
                        && input.client_action_id.as_str() == record.execution_id.as_str() =>
                {
                    event.turn_id
                }
                _ => None,
            }),
    };
    bound_turn.is_none_or(|turn_id| {
        document
            .active_turn()
            .is_some_and(|turn| !turn.closed && turn.turn_id == turn_id)
    })
}

/// Observes one work item under the same canonical gates used by launch and terminal publication.
/// 在启动及终态发布使用的同一规范锁内观测一项工作。
/// Returns live ownership or an explicit outcome; missing child ownership is also durably released.
/// 返回活动所有权或明确结果；丢失的子执行所有权还会可靠解除父侧义务。
async fn observe_work(
    invocation: &ValidatedInvocation,
    work: PendingWork,
    target: Option<Arc<SessionHandle>>,
) -> Result<(bool, Option<Value>), String> {
    let invocation = invocation.clone();
    let mut handles = vec![invocation.source_handle.clone()];
    if let Some(target) = &target {
        handles.push(target.clone());
    }
    let registry = invocation.registry.clone();
    lifecycle_write::with_canonical_handles(&registry, handles, move || {
        let session_id = Uuid::parse_str(&work.target.session_id).map_err(|error| error.to_string())?;
        if !invocation.source_handle.read(|document| pending_work(document.collaboration_state()).contains(&work))
            .map_err(|error| error.to_string())? { return Ok((false, None)); }
        let live = has_live_owner(&invocation, session_id)?;
        let question_source = invocation.source_handle.read(|document| match &work.kind {
            WorkKind::Question(question_id) => document.collaboration_state().questions.get(question_id).map(|question| question.source.clone()),
            _ => None,
        }).map_err(|error| error.to_string())?;
        let (stopped, eligible) = match &target {
            Some(target) => target.read(|document| {
                let control = document.collaboration_run_control();
                let parent_exception = question_source.as_ref().is_some_and(|source|
                    participants::require_document_message_admission(document, source, &work.target).is_ok());
                (matches!(control, CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents)
                    || (control == CollaborationRunControl::MainOnly && (!parent_exception || !live)), work_can_run(&work, document))
            }).map_err(|error| error.to_string())?,
            None => (false, false),
        };
        let live = match &work.kind {
            WorkKind::Child(record) => target.is_some() && has_live_child_owner(&invocation, record, eligible)?,
            _ => live && eligible,
        };
        if !stopped && live { return Ok((true, None)); }
        // Close only the exact queued/claimed work whose physical execution is gone.
        // 仅关闭物理执行已消失的精确排队或已认领工作。
        if !live && !stopped && let (WorkKind::Task(delivery_id), Some(target)) = (&work.kind, &target) {
            let delivery_id = delivery_id.clone();
            let stamp = now_ms()?;
            target.mutate_durable_if_changed(SessionMutationAdmission::ControlPlane, SessionFlushReason::MetadataChanged,
                |document| {
                    let state = document.collaboration_state();
                    if !state.inbound_task_messages.contains_key(&delivery_id) || state.task_results.contains_key(&delivery_id) {
                        return Ok(SessionMutationChange::Unchanged(()));
                    }
                    let effect = state.task_message_effects.get(&delivery_id).map(|effect| effect.outcome.clone());
                    match effect {
                        None => {
                            // Publish the failed effect first; result reconciliation requires a later durable revision.
                            // 先发布失败效果；结果对账要求后续独立可靠修订。
                            document.fail_task_message(CollaborationOperationId::try_new(format!("wait-abandoned:{}", delivery_id.as_str()))?,
                                delivery_id, CollaborationDeliveryFailureReason::TargetExecutionLost, stamp)?;
                        }
                        Some(CollaborationTaskMessageEffectKind::Claimed { turn_id, .. } | CollaborationTaskMessageEffectKind::Steered { turn_id, .. })
                            if document.active_turn().is_some_and(|turn| !turn.closed && turn.turn_id == turn_id) => {
                                document.abandon_unowned_execution(stamp)?;
                            }
                        _ => return Ok(SessionMutationChange::Unchanged(())),
                    }
                    Ok(SessionMutationChange::Changed(()))
                }).map_err(|error| error.to_string())?;
        }
        // Launch and final-result publication use these same gates, so an absent owner cannot race this settlement.
        // 启动与最终结果发布使用相同锁，因此所有者缺失结算不会与它们竞态。
        if let WorkKind::Child(record) = &work.kind {
            if !live {
                let input = vulcan_session_core::v1::collaboration::TerminalChildTaskInput {
                    operation_id: CollaborationOperationId::try_new(format!("wait-lost-child:{}", record.execution_id.as_str())).map_err(|error| error.to_string())?,
                    execution_id: record.execution_id.clone(), task_id: record.task_id.clone(),
                    outcome: if stopped { vulcan_session_core::v1::collaboration::CollaborationChildTaskTerminalOutcome::Stopped }
                        else { vulcan_session_core::v1::collaboration::CollaborationChildTaskTerminalOutcome::Failed },
                    terminal_at_ms: now_ms()?,
                };
                invocation.source_handle.mutate_durable_if_changed(SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::SubagentReturned, move |document| document.terminal_child_task(input).map(session_change))
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok((false, Some(json!({"work_id":work.id,"session_id":session_id,
            "outcome":if stopped {"stopped"} else {"abandoned"},
            "reason":if stopped {"user_stopped"} else if target.is_none() {"target_unavailable"} else if !eligible {"original_execution_ended_without_result"} else {"execution_not_running"}}))))
    }).await
}

/// Waits for any actionable inbox fact, or returns immediately when no associated work remains live.
/// 等待任意可处理收件事实，或在没有关联活动工作时立即返回。
/// Canonical context projection delivers full bodies once after this tool; reports carry stopped/lost work.
/// 规范上下文投影在工具之后一次交付完整正文；报告携带停止或丢失工作。
pub(super) async fn wait_agents_result(
    invocation: &ValidatedInvocation,
    timeout_ms: u64,
) -> Result<Value, String> {
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(timeout_ms))
        .ok_or_else(|| String::from("agent wait timeout exceeds runtime range"))?;
    // Historical acknowledgements and the frozen request frontier do not change within this invocation.
    // 历史确认与冻结请求边界在本次调用内不会变化。
    let (api_seq, reported) = invocation
        .source_handle
        .read_full_durable(|document| {
            (
                document
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        EventBody::ApiStarted(api)
                            if Some(api.api_call_id) == invocation.source_api_call_id =>
                        {
                            Some(api.projection.through_seq)
                        }
                        _ => None,
                    }),
                reported_work(document),
            )
        })
        .map_err(|error| error.to_string())?;
    // Always inspect ownership once, but bound retries caused by concurrent durable changes.
    // 始终检查一次所有权，但限制并发可靠状态变化引起的重试时间。
    let mut observed = false;
    loop {
        let canonical = invocation
            .registry
            .get(invocation.source_session_id)
            .map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(&canonical, &invocation.source_handle) {
            return Err("Agent wait owner was replaced.".into());
        }
        let (state, messages, interrupted) = invocation.source_handle.read(|document| {
            // Child final output is already a provider-visible durable event, not a separate mailbox copy.
            // 子最终输出已经是供应商可见的可靠事件，不是另一份邮箱副本。
            let mut messages = projection::inbox_messages(document)?;
            for event in document.events().iter().filter(|event| api_seq.is_some_and(|seq| event.seq > seq)) {
                if let EventBody::SubagentReturn(child) = &event.event {
                    messages.push(json!({"kind":"task_result","execution_id":child.child_root_action_id,"session_id":child.child_session_id,"outcome":"completed"}));
                }
            }
            // Human interjections release the wait; queued future tasks deliberately do not.
            // 用户回合内插队解除等待；未来任务排队刻意不解除。
            let interrupted = document.active_api().is_none_or(|api| Some(api.api_call_id) != invocation.source_api_call_id)
                || document.metadata.input_state.pending_in_turn_inputs.iter().any(|input| input.turn_id.value() == invocation.source_turn_id);
            Ok::<_, String>((document.collaboration_state().clone(), messages, interrupted))
        }).map_err(|error| error.to_string())??;
        if interrupted {
            return Ok(json!({"status":"interrupted", "interrupted":true,
                "context_messages_pending":!messages.is_empty(), "messages":messages, "reports":[],
                "operation_id":invocation.operation_id}));
        }
        let ready = !messages.is_empty();
        let mut reports = failed_questions(&state, &reported);
        // Lost child ownership is a durable terminal, so a cancelled wait can report it on retry.
        // 丢失子所有权是可靠终态，因此取消的等待可在重试时报告它。
        for child in state
            .child_tasks
            .values()
            .filter(|child| child.state == CollaborationChildTaskState::Terminal)
        {
            let operation = CollaborationOperationId::try_new(format!(
                "wait-lost-child:{}",
                child.execution_id.as_str()
            ))
            .map_err(|error| error.to_string())?;
            let id = format!("child:{}", child.execution_id.as_str());
            if state.operations.contains_key(&operation) && !reported.contains(&id) {
                let stopped = child.terminal_outcome == Some(vulcan_session_core::v1::collaboration::CollaborationChildTaskTerminalOutcome::Stopped);
                reports.push(json!({"work_id":id,"session_id":child.child.session_id,"outcome":if stopped {"stopped"} else {"abandoned"},
                    "reason":if stopped {"user_stopped"} else {"execution_not_running"}}));
            }
        }
        // Ready messages win over stale target observations and require no remote storage reads.
        // 就绪消息优先于过期目标观测，且不需要读取远端存储。
        if ready {
            return Ok(
                json!({"status":"messages", "context_messages_pending":true, "messages":messages,"reports":reports,
                "operation_id":invocation.operation_id}),
            );
        }
        if observed && reports.is_empty() && tokio::time::Instant::now() >= deadline {
            return Ok(json!({"status":"timeout", "context_messages_pending":false,
                "messages":[], "reports":[], "operation_id":invocation.operation_id}));
        }
        observed = true;
        let mut active = 0;
        for work in pending_work(&state) {
            if reported.contains(&work.id) && !matches!(&work.kind, WorkKind::Child(_)) {
                continue;
            }
            let session_id =
                Uuid::parse_str(&work.target.session_id).map_err(|error| error.to_string())?;
            // Resolve only the source-owned participant; later canonical gates recheck its ownership.
            // 仅解析来源拥有的参与者；后续规范锁内重新检查其所有权。
            let target = match participants::load_participant_handle(invocation, &work.target).await
            {
                Ok(target) => Some(target),
                Err(error) => {
                    tracing::warn!(%session_id, %error, "Unified wait target cannot be resolved");
                    None
                }
            };
            if let Some(target) = &target {
                // Live requests need only resident facts; reconstruct full history only for terminal repair.
                // 活动请求只需常驻事实；仅在终态修复时重建完整历史。
                let execution_live = match &work.kind {
                    WorkKind::Child(record) => has_live_child_owner(invocation, record, false)?,
                    _ => has_live_owner(invocation, session_id)?,
                };
                let repair_results = !execution_live
                    || target
                        .read(|document| {
                            document.active_turn().is_some_and(|turn| turn.closed)
                                || match &work.kind {
                                    WorkKind::Task(delivery_id) => document
                                        .collaboration_state()
                                        .task_results
                                        .contains_key(delivery_id),
                                    _ => false,
                                }
                        })
                        .map_err(|error| error.to_string())?;
                if repair_results {
                    task_results::reconcile_task_results_for_session(&invocation.state, session_id)
                        .await?;
                }
                if matches!(&work.kind, WorkKind::Question(_)) {
                    question_failures::reconcile_stopped_question_deliveries_for_session(
                        &invocation.state,
                        session_id,
                    )
                    .await?;
                }
                if repair_results && let WorkKind::Child(record) = &work.kind {
                    let root_id = Uuid::parse_str(&record.child.root_session_id)
                        .map_err(|error| error.to_string())?;
                    let root = invocation
                        .registry
                        .get(root_id)
                        .map_err(|error| error.to_string())?;
                    let generation = root
                        .read(|document| document.segment_generation())
                        .map_err(|error| error.to_string())?;
                    startup_recovery::native_results::recover_terminal_returns(
                        &invocation.state,
                        &invocation.registry,
                        root,
                        generation,
                        invocation.source_handle.clone(),
                        target.clone(),
                        vec![record.clone()],
                    )
                    .await?;
                }
            }
            let already_reported = reported.contains(&work.id);
            let task_target = if matches!(&work.kind, WorkKind::Task(_)) {
                target.as_ref().map(|target| target.session_id())
            } else {
                None
            };
            let (live, report) = observe_work(invocation, work, target).await?;
            if !live && let Some(target_id) = task_target {
                task_results::reconcile_task_results_for_session(&invocation.state, target_id)
                    .await?;
            }
            if live {
                active += 1;
            }
            if !already_reported && let Some(report) = report {
                reports.push(report);
            }
        }
        // Publication may have raced liveness inspection; re-read the inbox before reporting lost work.
        // 发布可能与存活检查竞态；报告工作丢失前重新读取收件箱。
        let changed = invocation
            .source_handle
            .read(|document| document.collaboration_state() != &state)
            .map_err(|error| error.to_string())?;
        if changed {
            continue;
        }
        if ready || !reports.is_empty() || active == 0 || tokio::time::Instant::now() >= deadline {
            return Ok(
                json!({"status": if ready || !reports.is_empty() {"messages"} else if active == 0 {"no_pending_work"} else {"timeout"},
                "context_messages_pending":ready,"messages":messages,"reports":reports,"pending_work_count":active,
                "operation_id":invocation.operation_id}),
            );
        }
        // Periodic ownership reconciliation covers lost stop notifications without model-side polling.
        // 周期性所有权对账覆盖丢失的停止通知，无需模型侧轮询。
        tokio::time::sleep(
            deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .min(COLLABORATION_WAIT_RECHECK_INTERVAL),
        )
        .await;
    }
}
