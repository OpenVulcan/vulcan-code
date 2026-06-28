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
    task_runtime.publish_detached_task_event(
        request.task_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: request.current_session_id,
            event,
        },
    );
}

/// Builds a terminal child-task lifecycle event for the parent session.
/// 为父级会话构建终态子任务生命周期事件。
pub(super) fn build_task_finished_parent_runtime_event(
    request: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
    detail: &str,
) -> RuntimeEvent {
    let finished_payload = build_task_host_payload(
        request,
        outcome.execution_status.as_str(),
        outcome.provider_turns_executed,
        outcome.tool_calls_processed,
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

/// Publishes a failed child-task lifecycle event back to the parent session.
/// 将失败的子任务生命周期事件发布回父级会话。
pub(super) fn publish_task_failure_to_parent(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    error: &str,
) {
    let event = build_task_failure_parent_runtime_event(request, error);
    task_runtime.publish_detached_task_event(
        request.task_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: request.current_session_id,
            event,
        },
    );
}

/// Builds a failed child-task lifecycle event for the parent session.
/// 为父级会话构建失败子任务生命周期事件。
pub(super) fn build_task_failure_parent_runtime_event(
    request: &TaskExecutionRequest,
    error: &str,
) -> RuntimeEvent {
    let finished_payload = build_task_host_payload(
        request,
        "failed",
        0,
        0,
        None,
        error,
        current_unix_millis_text(),
    );
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
            if let Some(runtime_context) =
                task_runtime.execution_context_for_session(request.current_session_id)
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

/// Returns whether one task execution error most likely came from a user-visible stop or transport cancellation rather than a real logical failure.
/// 返回某条任务执行错误是否更可能源于用户可见的停止或传输取消，而不是真正的逻辑失败。
pub(super) fn task_execution_error_is_stopped(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("cancelled")
        || normalized.contains("canceled")
        || normalized.contains("aborted")
        || normalized.contains("stopped")
}

/// Builds one host-only payload for task-session UI metadata without leaking large child snapshots into the model-visible tool result.
/// 构建一份仅供宿主界面使用的任务会话载荷，避免把庞大的子会话快照泄漏到模型可见工具结果中。
pub(super) fn build_task_host_payload(
    invocation: &TaskExecutionRequest,
    execution_status: &str,
    provider_turns_executed: usize,
    tool_calls_processed: usize,
    final_assistant_message_id: Option<&str>,
    result_text: &str,
    updated_at: String,
) -> Value {
    let task_status = task_session_status_from_execution_status_text(execution_status);
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
            String::from("provider_turns_executed"),
            serde_json::json!(provider_turns_executed),
        );
        payload.insert(
            String::from("tool_calls_processed"),
            serde_json::json!(tool_calls_processed),
        );
        payload.insert(
            String::from("final_assistant_message_id"),
            serde_json::json!(final_assistant_message_id),
        );
    }
    json!({
        "task_id": invocation.public_task_id.clone(),
        "task_session_id": invocation.task_id.to_string(),
        "display_title": invocation.display_title.clone(),
        "status": task_invocation_status_text(invocation.is_resumed),
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "prompt": invocation.prompt.clone(),
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "provider_turns_executed": provider_turns_executed,
        "tool_calls_processed": tool_calls_processed,
        "final_assistant_message_id": final_assistant_message_id,
        "result": result_text,
        "updated_at": updated_at,
        "subagent_mailbox_event": subagent_mailbox_event,
    })
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
        "kind": "task_session",
        "payload": payload,
    })))
}
