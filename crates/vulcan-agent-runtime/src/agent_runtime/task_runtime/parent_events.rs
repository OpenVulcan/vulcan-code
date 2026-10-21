use super::*;

/// Publishes a completed or stopped child-task lifecycle event back to the parent session.
/// 将完成或停止的子任务生命周期事件发布回父级会话。
pub(super) fn publish_task_finished_to_parent(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
    detail: &str,
) {
    let event = build_task_finished_parent_runtime_event(request, outcome, detail);
    // A detached parent-event failure cannot propagate beyond the background owner.
    // 分离的父级事件失败无法越过后台所有者继续传播。
    if let Err(error) = task_runtime.publish_detached_task_event(
        request.task_id,
        &request.execution_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: request.current_session_id,
            event,
        },
    ) {
        eprintln!(
            "[task-runtime] completed task parent-event publication failed for task {} execution {}: {error}",
            request.task_id, request.execution_id
        );
    }
}

/// Publishes a settled child result only when no queued-message successor run took ownership.
/// 仅在没有排队消息续跑接管子任务时发布已收敛的子任务结果。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the child event stream and parent mailbox.
/// `task_runtime` 持有子任务事件流与父级 mailbox。
/// `request` identifies the settled physical child execution.
/// `request` 标识已收敛的物理子执行。
/// `outcome` is the settled execution outcome.
/// `outcome` 是已收敛执行的结果。
/// `detail` is the user-visible lifecycle detail for a truly terminal result.
/// `detail` 是真正终态结果对应的用户可见生命周期说明。
/// `outcome_committed` states that this execution committed the stable terminal projection.
/// `outcome_committed` 表示该执行已经提交稳定终态投影。
/// `queued_resume_started` states that queued input already started a successor execution.
/// `queued_resume_started` 表示排队输入已经启动了后继执行。
pub(super) fn publish_task_finished_to_parent_after_resume_decision(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
    detail: &str,
    outcome_committed: bool,
    queued_resume_started: bool,
) {
    if !outcome_committed || queued_resume_started {
        return;
    }
    task_runtime.publish_task_outcome_to_parent_mailbox(request, outcome);
    publish_task_finished_to_parent(task_runtime, request, outcome, detail);
}

/// Builds one completed parent event carrying child content and lifecycle identity.
/// 构建携带子任务正文与生命周期身份的父级完成事件。
///
/// # Parameters
/// 参数
/// `request` identifies the exact child execution and parent relationship.
/// `request` 标识精确的子执行及父级关系。
/// `outcome` retains legacy execution counters and result content.
/// `outcome` 保留旧版执行计数与结果内容。
/// `detail` is the human-readable lifecycle explanation.
/// `detail` 是人类可读的生命周期说明。
///
/// # Returns
/// 返回值
/// Returns one DelegateFinished event ready for the exact parent runtime.
/// 返回一条可投递给精确父运行时的 DelegateFinished 事件。
fn build_task_finished_parent_runtime_event(
    request: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
    detail: &str,
) -> RuntimeEvent {
    let finished_payload = build_task_host_payload(
        request,
        outcome.execution_status.as_str(),
        outcome.final_assistant_message_id.as_deref(),
        outcome.final_output_text.as_str(),
        current_unix_millis_text(),
    );
    build_task_delegate_runtime_event(
        RuntimeEventKind::DelegateFinished,
        request,
        detail,
        finished_payload,
    )
}

/// Publishes a failed child-task lifecycle event to both parent feedback channels.
/// 将失败的子任务生命周期事件发布到父级的两条反馈通道。
pub(super) fn publish_task_failure_to_parent(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    error: &str,
) {
    // The process-local mailbox owns wait_agent wake-up, while the detached event owns the visible parent timeline.
    // 进程内 mailbox 负责唤醒 wait_agent，分离事件负责父级时间线可见反馈。
    task_runtime.publish_task_failure_to_parent_mailbox(request, error);
    let event = build_task_failure_parent_runtime_event(request, error);
    // A detached parent-event failure cannot propagate beyond the background owner.
    // 分离的父级事件失败无法越过后台所有者继续传播。
    if let Err(publish_error) = task_runtime.publish_detached_task_event(
        request.task_id,
        &request.execution_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: request.current_session_id,
            event,
        },
    ) {
        eprintln!(
            "[task-runtime] failed task parent-event publication failed for task {} execution {}: {publish_error}",
            request.task_id, request.execution_id
        );
    }
}

/// Publishes a failed child result only when no queued-message successor run took ownership.
/// 仅在没有排队消息续跑接管子任务时发布失败的子任务结果。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the child event stream and parent mailbox.
/// `task_runtime` 持有子任务事件流与父级 mailbox。
/// `request` identifies the failed physical child execution.
/// `request` 标识失败的物理子执行。
/// `error` is the terminal failure text.
/// `error` 是终态失败文本。
/// `failure_committed` states that this execution committed the stable failure projection.
/// `failure_committed` 表示该执行已经提交稳定失败投影。
/// `queued_resume_started` states that queued input already started a successor execution.
/// `queued_resume_started` 表示排队输入已经启动了后继执行。
pub(super) fn publish_task_failure_to_parent_after_resume_decision(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    error: &str,
    failure_committed: bool,
    queued_resume_started: bool,
) {
    if !failure_committed || queued_resume_started {
        return;
    }
    publish_task_failure_to_parent(task_runtime, request, error);
}

/// Builds one failed parent event with an optional ledger-materialized child statistic.
/// 构建一条可携带账本物化子统计的失败父事件。
///
/// # Parameters
/// 参数
/// `request` identifies the exact failed child execution.
/// `request` 标识精确的失败子执行。
/// `error` is the explicit terminal failure text.
/// `error` 是显式终态失败文本。
/// `turn_stats` is the optional persisted statistic containing failed-call Usage.
/// `turn_stats` 是可选的持久化统计，其中包含失败调用 Usage。
///
/// # Returns
/// 返回值
/// Returns one DelegateFinished event that preserves failure accounting.
/// 返回一条保留失败记账的 DelegateFinished 事件。
pub(super) fn build_task_failure_parent_runtime_event(
    request: &TaskExecutionRequest,
    error: &str,
) -> RuntimeEvent {
    let finished_payload =
        build_task_host_payload(request, "failed", None, error, current_unix_millis_text());
    build_task_delegate_runtime_event(
        RuntimeEventKind::DelegateFinished,
        request,
        error,
        finished_payload,
    )
}

/// Builds one backend handler that executes task sessions through the shared runtime preparation bridge only.
/// 构建一条只通过共享运行时准备桥执行任务会话的后端处理器。
pub(crate) fn build_task_execution_handler(
    task_runtime: Arc<TaskRuntime>,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
) -> TaskExecutionHandler {
    Arc::new(move |request: TaskExecutionRequest| {
        let task_runtime = Arc::clone(&task_runtime);
        let base_controller_config = base_controller_config.clone();
        let parent_live_sender = parent_live_sender.clone();
        Box::pin(async move {
            // runtime_context is the exact shared parent context required by the task backend.
            // runtime_context 是任务后端所需的精确共享父级上下文。
            if let Some(runtime_context) =
                task_runtime.execution_context_for_session(request.current_session_id)?
            {
                return run_task_with_shared_runtime_context(
                    Arc::clone(&task_runtime),
                    runtime_context,
                    base_controller_config,
                    parent_live_sender,
                    request,
                )
                .await;
            }

            Err(format!(
                "Task `{}` cannot start because the shared runtime execution context is not registered for parent session `{}`.",
                request.public_task_id, request.current_session_id
            ))
        })
    })
}

/// Builds one synthetic running task outcome used only for persisted status and explicit status checks.
/// 构建一条仅用于持久化状态与显式状态检查的合成运行中任务结果。
pub(super) fn build_running_task_execution_outcome(
    task_id: SessionId,
    controller_config: CoreControllerConfig,
    resolved_config: TaskSessionResolvedConfig,
    snapshot: CoreConversationSnapshot,
) -> TaskExecutionOutcome {
    TaskExecutionOutcome {
        task_id,
        controller_config,
        resolved_config,
        snapshot,
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
        usage_segments: Vec::new(),
    }
}

/// Builds one synthetic stopped task outcome used when a child subagent is intentionally interrupted before it can emit a normal final answer.
/// 当子代理在来得及产出正常最终答案之前被有意中断时，构建一条合成的停止结果。
pub(super) fn build_stopped_task_execution_outcome(
    task_id: SessionId,
    controller_config: CoreControllerConfig,
    resolved_config: TaskSessionResolvedConfig,
    snapshot: CoreConversationSnapshot,
) -> TaskExecutionOutcome {
    TaskExecutionOutcome {
        task_id,
        controller_config,
        resolved_config,
        snapshot,
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(STOPPED_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Stopped,
        usage_segments: Vec::new(),
    }
}

/// Formats one spawned child-task join error into a short user-facing diagnostic string.
/// 将一条已派生子任务的 JoinError 格式化为简短的面向用户诊断文本。
pub(super) fn format_task_join_error(error: &tokio::task::JoinError) -> String {
    if error.is_panic() {
        String::from("The subagent task panicked internally.")
    } else if error.is_cancelled() {
        String::from("The subagent task was cancelled.")
    } else {
        error.to_string()
    }
}

/// Returns the persisted task-invocation status text used by both runtime metadata and tool results.
/// 返回运行时元数据与工具结果共同使用的持久化任务调用状态文本。
pub(super) fn task_invocation_status_text(is_resumed: bool) -> &'static str {
    if is_resumed { "resumed" } else { "created" }
}

/// Prefix marking a child-task backend error that happened before a task session actually started.
/// 标记子任务后端错误发生在任务会话真正启动前的前缀。
const TASK_EXECUTION_PRESTART_CANCELLED_PREFIX: &str = "[task-prestart-cancelled]";

/// Builds a pre-start cancellation error for a child task whose parent live stream is gone.
/// 为父级实时流已经消失的子任务构建启动前取消错误。
pub(super) fn task_start_parent_stream_closed_error(public_task_id: &str) -> String {
    format!(
        "{TASK_EXECUTION_PRESTART_CANCELLED_PREFIX} Task `{public_task_id}` cannot start because the parent live stream closed before DelegateStarted could be delivered."
    )
}

/// Builds a pre-start cancellation error for an execution that lost stable-record ownership.
/// 为失去稳定记录所有权的执行构建启动前取消错误。
///
/// # Parameters
/// 参数
/// `task_id` identifies the stable task record.
/// `task_id` 标识稳定任务记录。
/// `execution_id` identifies the superseded execution.
/// `execution_id` 标识被取代的执行。
/// `current_execution_id` identifies the execution that now owns the record.
/// `current_execution_id` 标识当前拥有记录的执行。
///
/// # Returns
/// 返回值
/// A prestart-cancellation diagnostic consumed by invocation rollback handling.
/// 返回由调用回滚处理消费的启动前取消诊断。
pub(super) fn task_execution_record_owner_conflict_error(
    task_id: SessionId,
    execution_id: &str,
    current_execution_id: &str,
) -> String {
    format!(
        "{TASK_EXECUTION_PRESTART_CANCELLED_PREFIX} Task `{task_id}` execution `{execution_id}` lost its preparation record to execution `{current_execution_id}`."
    )
}

/// Marks a child-creation failure that happened before the host created the child document.
/// 标记发生在宿主创建子文档之前的子代理创建失败。
/// `reason` is the visible failure; returns it with the internal pre-start marker.
/// `reason` 是对外可见的失败；返回带内部启动前标记的同一文本。
pub(super) fn task_execution_prestart_rejected_error(reason: String) -> String {
    format!("{TASK_EXECUTION_PRESTART_CANCELLED_PREFIX} {reason}")
}

/// Returns whether one backend error represents a pre-start cancellation that must not be persisted as a failed child agent.
/// 返回某条后端错误是否表示不应持久化为失败子代理的启动前取消。
pub(super) fn task_execution_error_is_prestart_cancelled(error: &str) -> bool {
    error.starts_with(TASK_EXECUTION_PRESTART_CANCELLED_PREFIX)
}

/// Removes the internal pre-start cancellation marker from a backend error before surfacing it.
/// 在向外暴露后端错误前移除内部启动前取消标记。
pub(super) fn task_execution_visible_error(error: &str) -> &str {
    error
        .strip_prefix(TASK_EXECUTION_PRESTART_CANCELLED_PREFIX)
        .map(str::trim_start)
        .unwrap_or(error)
}

/// Builds one host-only payload for task-session UI metadata without leaking large child snapshots into the model-visible tool result.
/// 构建一份仅供宿主界面使用的任务会话载荷，避免把庞大的子会话快照泄漏到模型可见工具结果中。
pub(super) fn build_task_host_payload(
    invocation: &TaskExecutionRequest,
    execution_status: &str,
    final_assistant_message_id: Option<&str>,
    result_text: &str,
    updated_at: String,
) -> Value {
    let task_status = task_session_status_from_execution_status_text(execution_status)
        .expect("task host payload execution status must be canonical");
    let timestamp_ms = updated_at
        .parse::<u64>()
        .unwrap_or_else(|_| current_unix_millis_u64());
    let result_for_mailbox = matches!(
        task_status,
        TaskSessionStatus::Completed | TaskSessionStatus::Stopped
    )
    .then_some(result_text);
    let error_for_mailbox = (task_status == TaskSessionStatus::Failed).then_some(result_text);
    let mut subagent_mailbox_event = build_subagent_mailbox_event(
        invocation,
        task_status,
        timestamp_ms,
        result_for_mailbox,
        error_for_mailbox,
        None,
    );
    if let Some(payload) = subagent_mailbox_event.payload.as_object_mut() {
        payload.insert(
            String::from("final_assistant_message_id"),
            serde_json::json!(final_assistant_message_id),
        );
    }
    json!({
        "task_id": invocation.public_task_id.clone(),
        (SUBAGENT_TASK_ID_KEY): invocation.task_id.to_string(),
        "execution_id": invocation.execution_id.clone(),
        "display_title": invocation.display_title.clone(),
        "status": task_invocation_status_text(invocation.is_resumed),
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "prompt": invocation.prompt.clone(),
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "final_assistant_message_id": final_assistant_message_id,
        "result": result_text,
        "updated_at": updated_at,
        "debug": subagent_runtime_debug_payload(&invocation.invocation_config),
        "subagent_mailbox_event": subagent_mailbox_event,
    })
}

/// Reads the non-secret subagent creation debug payload from one invocation.
/// 从单次调用中读取不含敏感提示词正文的子代理创建调试载荷。
///
/// # Parameters
/// 参数
/// `invocation` is the task invocation configuration to inspect.
/// `invocation` 是需要检查的任务调用配置。
///
/// # Returns
/// 返回值
/// A JSON object with creation-facing diagnostics when available.
/// 可用时返回包含创建侧诊断信息的 JSON 对象。
pub(super) fn subagent_runtime_debug_payload(invocation: &TaskInvocationConfig) -> Value {
    invocation
        .extra
        .as_object()
        .and_then(|extra| extra.get("_subagent_runtime_debug"))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Builds one delegate runtime event used by the parent session to track child task lifecycle without replaying child transcript output inline.
/// 构建一条供父会话追踪子任务生命周期的委派运行时事件，同时避免把子任务正文直接回放到父会话中。
pub(super) fn build_task_delegate_runtime_event(
    kind: RuntimeEventKind,
    invocation: &TaskExecutionRequest,
    detail: &str,
    payload: Value,
) -> RuntimeEvent {
    let task_label = invocation.display_title.as_str();
    let title = match kind {
        RuntimeEventKind::DelegateStarted => {
            format!("Subagent started · {} · {task_label}", invocation.task_id)
        }
        RuntimeEventKind::DelegateFinished => {
            format!("Subagent finished · {} · {task_label}", invocation.task_id)
        }
        _ => format!("Subagent event · {} · {task_label}", invocation.task_id),
    };

    RuntimeEvent::new(kind, title, detail).with_metadata(Some(json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": payload,
    })))
}
