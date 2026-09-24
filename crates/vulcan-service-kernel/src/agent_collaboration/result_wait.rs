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
                if let Some(identity) = report_identity(report) {
                    reported.insert(identity);
                }
            }
        }
    }
    reported
}

/// Keys one acknowledged observation by work, outcome and reason, without suppressing later changes.
/// 按工作、结果和原因标识已确认观测，不抑制之后发生的变化。
/// `report` must contain the declared report fields; malformed historical output proves no acknowledgement.
/// `report` 必须包含声明字段；畸形历史输出不能证明已确认。
fn report_identity(report: &Value) -> Option<String> {
    Some(
        json!([
            report.get("work_id")?.as_str()?,
            report.get("outcome")?.as_str()?,
            report.get("reason")?
        ])
        .to_string(),
    )
}

/// Retains a new observation unless the same outcome and reason were already durably delivered.
/// 保留新观测，除非相同结果及原因已经可靠交付。
/// Returns the unchanged report or no duplicate; it never changes live-work selection.
/// 返回原报告或去除重复项；绝不改变活动工作选择。
fn unreported(report: Value, reported: &BTreeSet<String>) -> Option<Value> {
    (!report_identity(&report).is_some_and(|identity| reported.contains(&identity)))
        .then_some(report)
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
        let delivery = state.outbound_deliveries.get(&question.delivery_id)?;
        let answer_failed = question.answer.as_ref().is_some_and(|answer|
            answer.delivery.state == vulcan_session_core::v1::collaboration::CollaborationAnswerDeliveryState::Failed);
        if question.state == CollaborationQuestionState::Cancelled || delivery.state == CollaborationDeliveryState::Failed || answer_failed {
            unreported(json!({"work_id": id, "session_id": question.target.session_id,
                "question_id": question.question_id, "outcome": if question.state == CollaborationQuestionState::Cancelled { "cancelled" }
                else if delivery.failure == Some(CollaborationDeliveryFailureReason::TargetStopped) { "stopped" } else { "failed" },
                "reason": if answer_failed { json!("answer_delivery_failed") } else { json!(delivery.failure) }}), reported)
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
        // A new owner may appear after the storage probe; never close it using the older absence.
        // 存储探测后可能出现新所有者；绝不使用较早缺失观测将其关闭。
        if target.is_none() && live {
            return Err(String::from("Target execution appeared during storage observation; retry the check."));
        }
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
        // Child loss is persisted above; its first report must match later durable replays even at deadline.
        // 子任务丢失已在上方持久化；即使到达截止点，首次报告也必须与后续持久重放一致。
        Ok((false, Some(json!({"work_id":work.id,"session_id":session_id,
            "outcome":if stopped {"stopped"} else {"abandoned"},
            "reason":if stopped {"user_stopped"} else if matches!(work.kind, WorkKind::Child(_)) {"execution_not_running"}
                else if target.is_none() {"target_unavailable"} else if !eligible {"original_execution_ended_without_result"} else {"execution_not_running"}}))))
    }).await
}

/// Describes `work` without duplicating bodies; `state` is its current observation.
/// 描述 `work` 且不复制正文；`state` 是当前观测状态。
/// Returns immutable routing identity for the remaining-work summary.
/// 返回剩余工作摘要所需的不可变路由身份。
fn work_summary(work: &PendingWork, state: &str) -> Value {
    json!({"work_id":work.id,"session_id":work.target.session_id,"state":state,
        "kind":match &work.kind { WorkKind::Question(_) => "question", WorkKind::Child(_) => "child_task", WorkKind::Task(_) => "session_task" }})
}

/// Reads actionable inbox and current-turn reply obligations at the frozen `api_seq` frontier.
/// 在冻结的 `api_seq` 前沿读取可处理收件与当前回合待答事项。
/// Returns source state, message identities, unanswered accepted questions, and user interruption.
/// 返回来源状态、消息身份、已接纳未回答问题以及用户插队事实。
fn inbox_snapshot(
    invocation: &ValidatedInvocation,
    api_seq: Option<vulcan_session_core::v1::ids::Seq>,
) -> Result<(CollaborationState, Vec<Value>, Vec<Value>, bool), String> {
    invocation.source_handle.read(|document| {
        // Child results already exist as model-visible durable events.
        // 子任务结果已经作为模型可见可靠事件存在。
        let mut messages = projection::inbox_messages(document)?;
        for event in document.events().iter().filter(|event| api_seq.is_some_and(|seq| event.seq > seq)) {
            if let EventBody::SubagentReturn(child) = &event.event {
                messages.push(json!({"kind":"task_result","execution_id":child.child_root_action_id,"session_id":child.child_session_id,"outcome":"completed"}));
            }
        }
        // Only questions delivered into this turn require attention before another blocking wait.
        // 只有已投递到此回合的问题要求在再次阻塞等待前处理。
        let questions = document.collaboration_state().inbound_deliveries.values().filter(|inbound|
            matches!(inbound.state, CollaborationDeliveryState::Accepted | CollaborationDeliveryState::Projected) && inbound.settlement.is_none()
            && (inbound.bound_turn_id.is_some_and(|turn| turn.value() == invocation.source_turn_id)
                || document.collaboration_state().question_executions.values().any(|execution|
                    execution.question_id == inbound.question_id
                    && execution.state == CollaborationQuestionExecutionState::Running
                    && execution.execution_turn_id.is_some_and(|turn| turn.value() == invocation.source_turn_id))))
            .map(|inbound| json!({"question_id":inbound.question_id,"session_id":inbound.source.session_id,"action":"reply_question"})).collect();
        // Human future queueing does not interrupt this turn.
        // 用户未来任务排队不打断此回合。
        let interrupted = document.active_api().is_none_or(|api| Some(api.api_call_id) != invocation.source_api_call_id)
            || document.metadata.input_state.pending_in_turn_inputs.iter().any(|input| input.turn_id.value() == invocation.source_turn_id);
        Ok((document.collaboration_state().clone(), messages, questions, interrupted))
    }).map_err(|error| error.to_string())?
}

/// Checks one exact work item; errors remain local to this target and never prove abandonment.
/// 检查一个精确工作项；错误只属于此目标，绝不证明执行中断。
/// Returns live ownership and a terminal observation, or a retryable check failure.
/// 返回实际存活与终态观测，或可重试的检查失败。
async fn inspect_work(
    invocation: &ValidatedInvocation,
    work: &PendingWork,
) -> Result<(bool, Option<Value>), String> {
    let session_id = Uuid::parse_str(&work.target.session_id).map_err(|error| error.to_string())?;
    // Resolve only the source-owned participant; later canonical gates recheck its ownership.
    // 仅解析来源拥有的参与者；后续规范锁内重新检查其所有权。
    let target = match participants::load_participant_handle(invocation, &work.target).await {
        Ok(target) => Some(target),
        Err(error) => {
            // Absence is an independent typed storage fact, never inferred from a read error.
            // 缺失是独立的强类型存储事实，绝不从读取错误推断。
            if has_live_owner(invocation, session_id)?
                || !participants::participant_storage_missing(invocation, &work.target).await?
            {
                return Err(error);
            }
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
            task_results::reconcile_task_results_for_session(&invocation.state, session_id).await?;
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
    let task_target = if matches!(&work.kind, WorkKind::Task(_)) {
        target.as_ref().map(|target| target.session_id())
    } else {
        None
    };
    let (live, report) = observe_work(invocation, work.clone(), target).await?;
    if !live && let Some(target_id) = task_target {
        task_results::reconcile_task_results_for_session(&invocation.state, target_id).await?;
    }

    Ok((live, report))
}

/// Waits for any actionable inbox fact, or returns immediately when no associated work remains live.
/// 等待任意可处理收件事实，或在没有关联活动工作时立即返回。
/// Canonical context projection delivers full bodies once after this tool; reports carry stopped/lost work.
/// 规范上下文投影在工具之后一次交付完整正文；报告携带停止或丢失工作。
pub(super) async fn wait_agents_result(
    invocation: &ValidatedInvocation,
    timeout_ms: u64,
) -> Result<Value, String> {
    // One absolute budget bounds re-observation even when publications keep changing.
    // 一个绝对预算约束重新观测，即使发布不断变化也不延长等待。
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(timeout_ms))
        .ok_or_else(|| String::from("agent wait timeout exceeds runtime range"))?;
    // The frozen request frontier identifies messages not already included in this model request.
    // 冻结的请求前沿标识尚未包含在本次模型请求中的消息。
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
    loop {
        // Only the canonical owner may observe or reconcile this inbox.
        // 只有规范所有者可以观测或对账此收件箱。
        let canonical = invocation
            .registry
            .get(invocation.source_session_id)
            .map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(&canonical, &invocation.source_handle) {
            return Err("Agent wait owner was replaced.".into());
        }
        // Refresh messages and question obligations from one source snapshot.
        // 从同一来源快照刷新消息与待答问题。
        let (state, _, _, interrupted) = inbox_snapshot(invocation, api_seq)?;
        // Source failure reports are durable; external inspection errors are deliberately not acknowledged.
        // 来源失败报告可靠持久化；外部检查错误刻意不确认。
        let mut reports = failed_questions(&state, &reported);
        for child in state
            .child_tasks
            .values()
            .filter(|child| child.state == CollaborationChildTaskState::Terminal)
        {
            // A cancelled earlier wait still needs to report its durably released child.
            // 被取消的先前等待仍需要报告已经可靠释放的子任务。
            let operation = CollaborationOperationId::try_new(format!(
                "wait-lost-child:{}",
                child.execution_id.as_str()
            ))
            .map_err(|error| error.to_string())?;
            let id = format!("child:{}", child.execution_id.as_str());
            if state.operations.contains_key(&operation) {
                let stopped = child.terminal_outcome == Some(vulcan_session_core::v1::collaboration::CollaborationChildTaskTerminalOutcome::Stopped);
                reports.extend(unreported(json!({"work_id":id,"session_id":child.child.session_id,"outcome":if stopped {"stopped"} else {"abandoned"},
                    "reason":if stopped {"user_stopped"} else {"execution_not_running"}}), &reported));
            }
        }
        // Every exit includes the same work summary; unknown work is never counted as completed.
        // 每个出口包含同样的工作摘要；未知工作绝不计为完成。
        let mut remaining = Vec::new();
        // Target-local failures must not hide another participant's ready message.
        // 目标局部失败不得隐藏其他参与者的就绪消息。
        let mut errors = Vec::new();
        for work in pending_work(&state) {
            // User interruption takes precedence over remote reads and durable repair.
            // 用户插队优先于远端读取与可靠修复。
            if interrupted {
                remaining.push(work_summary(&work, "unchecked"));
                continue;
            }
            match inspect_work(invocation, &work).await {
                Ok((true, _)) => remaining.push(work_summary(&work, "running")),
                Ok((false, report)) => {
                    // Acknowledging an observation suppresses only repetition, never future liveness checks.
                    // 确认观测仅抑制重复报告，绝不抑制今后的存活检查。
                    if let Some(report) = report {
                        reports.extend(unreported(report, &reported));
                    }
                }
                Err(error) => {
                    remaining.push(work_summary(&work, "check_failed"));
                    errors.push(
                        json!({"work_id":work.id,"session_id":work.target.session_id,
                        "code":"target_check_failed","retryable":true,"detail":error}),
                    );
                }
            }
        }
        // Re-read after reconciliation so a produced terminal or concurrently received question is returned now.
        // 对账后重新读取，立即返回刚产生的终态或并发收到的问题。
        let (current, messages, questions, fresh_interrupted) =
            inbox_snapshot(invocation, api_seq)?;
        if current != state
            && !interrupted
            && !fresh_interrupted
            && tokio::time::Instant::now() < deadline
        {
            continue;
        }
        if current != state {
            // An exhausted budget returns an honest unchecked snapshot instead of stale completion claims.
            // 预算耗尽时返回真实的未检查快照，而非过期的完成断言。
            remaining = pending_work(&current)
                .iter()
                .map(|work| work_summary(work, "unchecked"))
                .collect();
            errors.retain(|error| {
                remaining
                    .iter()
                    .any(|work| work["work_id"] == error["work_id"])
            });
        }
        // Fresh source facts are authoritative even on timeout and user interruption.
        // 即使超时或用户插队，新来源事实仍是权威。
        let interrupted = interrupted || fresh_interrupted;
        let status = if interrupted {
            "interrupted"
        } else if !messages.is_empty() || !reports.is_empty() {
            "messages"
        } else if !questions.is_empty() {
            "needs_reply"
        } else if !errors.is_empty() {
            "check_failed"
        } else if remaining.is_empty() {
            "no_pending_work"
        } else {
            "timeout"
        };
        if status != "timeout" || tokio::time::Instant::now() >= deadline {
            return Ok(json!({
                "status":status, "interrupted":interrupted,
                "context_messages_pending":!messages.is_empty(), "messages":messages, "reports":reports,
                "pending_work_count":remaining.iter().filter(|work| work["state"] == "running").count(),
                "unverified_work_count":remaining.iter().filter(|work| work["state"] != "running").count(),
                "remaining_work":remaining, "pending_questions":questions, "check_errors":errors,
                "operation_id":invocation.operation_id
            }));
        }
        tokio::time::sleep(
            deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .min(COLLABORATION_WAIT_RECHECK_INTERVAL),
        )
        .await;
    }
}
