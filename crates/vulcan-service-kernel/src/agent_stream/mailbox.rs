//! WebUI mailbox and wait-agent helpers.
//! WebUI mailbox 与 wait-agent 辅助函数。

use super::*;
use crate::pending_prompts::webui_pending_prompt_dispatch_context;
use vulcan_workbench_model::take_runtime_intervention_guided_pending_prompt_entries;

/// Builds the WebUI runtime intervention that injects guided prompts before the next provider request.
/// 构建 WebUI 运行时干预器，用于在下一次 Provider 请求前注入引导提示词。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit refreshed workbench snapshots.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` is the shared service bridge state that owns the active workspace snapshot.
/// `state` 是拥有当前工作区快照的共享服务桥接状态。
///
/// # Returns
/// 返回值
/// A callback consumed by the host-controlled runtime stream.
/// 返回供宿主控制运行时流消费的回调。
pub(crate) fn build_webui_prompt_intervention(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> RuntimePromptIntervention {
    Arc::new(move |session_id, controller: &mut VulcanCoreController| {
        let (pending_queue_changed, controller_changed) =
            apply_webui_prompt_intervention_transaction(&state, session_id, controller)?;
        if pending_queue_changed
            && let Err(error) = emit_pending_prompts_changed_delta(&app, &state, session_id)
        {
            eprintln!(
                "[webui-service] guided prompt queue delta emit failed after runtime intervention: {error}"
            );
        }
        Ok(controller_changed)
    })
}

/// Applies guided prompts and mailbox inputs as one retained-state and Core-controller transaction.
/// 将引导提示词与 mailbox 输入作为单个保留状态和 Core 控制器事务应用。
///
/// # Parameters
/// 参数
/// `state` owns the workspace queue state and live session kernel.
/// `state` 持有工作区队列状态与实时会话内核。
/// `session_id` identifies the session reaching the safe Provider boundary.
/// `session_id` 标识到达安全 Provider 边界的会话。
/// `controller` owns the authoritative Core snapshot committed only after every draft succeeds.
/// `controller` 持有仅在全部草稿成功后提交的权威 Core 快照。
///
/// # Returns
/// 返回值
/// Returns whether the pending queue and controller changed, or an error with every authoritative input unchanged.
/// 返回待发送队列与控制器是否变化；失败时返回错误且全部权威输入保持不变。
fn apply_webui_prompt_intervention_transaction(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    controller: &mut VulcanCoreController,
) -> Result<(bool, bool), RuntimePromptInterventionError> {
    let mut guard = state
        .lock()
        .map_err(|error| RuntimePromptInterventionError::StateAccess {
            message: format!("failed to lock service state: {error}"),
        })?;
    let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        RuntimePromptInterventionError::StateUnavailable {
            message: String::from("workspace snapshot is not ready"),
        }
    })?;
    let dispatch_context = webui_pending_prompt_dispatch_context(workspace, session_id, false);
    // runtime_state_draft isolates only the session-scoped pending UI state that this transaction can modify.
    // runtime_state_draft 仅隔离该事务能够修改的会话级待发送 UI 状态。
    let mut runtime_state_draft = workspace.session_runtime_state.get(&session_id).cloned();
    // controller_draft makes guided and mailbox projections one Core commit.
    // controller_draft 使引导投影与 mailbox 投影成为一次 Core 提交。
    let mut controller_draft = controller.clone();
    // guided_entries are removed only from the detached workspace draft.
    // guided_entries 仅从分离的工作区草稿中移除。
    let guided_entries = runtime_state_draft
        .as_mut()
        .map(|runtime_state| {
            take_runtime_intervention_guided_pending_prompt_entries(
                &mut runtime_state.ui_state,
                dispatch_context,
            )
        })
        .unwrap_or_default();
    let mut guided_prompt_changed = false;
    for entry in &guided_entries {
        let (prompt, attachments) = normalize_webui_interjected_prompt(entry);
        guided_prompt_changed |= controller_draft
            .append_interjected_user_turn_with_attachments(prompt.as_str(), attachments)
            .is_some();
    }
    if !guided_entries.is_empty()
        && let Some(runtime_state) = runtime_state_draft.as_mut()
    {
        runtime_state.updated_at = current_unix_millis_text();
    }
    // kernel_draft isolates mailbox drains and consumption markers from conversion failures.
    // kernel_draft 隔离 mailbox 取出与消费标记，避免转换失败污染权威状态。
    let mut kernel_draft = guard.session_runtime_kernels.get(&session_id).cloned();
    let mailbox_changed = match kernel_draft.as_mut() {
        Some(kernel) => append_webui_mailbox_intervention_messages_to_draft(
            kernel,
            session_id,
            &mut controller_draft,
        )?,
        None => false,
    };

    if let Some(runtime_state) = runtime_state_draft {
        guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| RuntimePromptInterventionError::StateUnavailable {
                message: String::from("workspace snapshot is not ready during commit"),
            })?
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    if mailbox_changed && let Some(kernel) = kernel_draft {
        guard.session_runtime_kernels.insert(session_id, kernel);
        sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
    }
    *controller = controller_draft;
    Ok((
        !guided_entries.is_empty(),
        guided_prompt_changed || mailbox_changed,
    ))
}

/// Appends current-turn mailbox messages to detached kernel and controller drafts.
/// 将当前回合 mailbox 消息追加到分离的内核与控制器草稿。
///
/// # Parameters
/// 参数
/// `kernel` is the detached session kernel whose input queue may be drained.
/// `kernel` 是输入队列可被取出的分离会话内核。
/// `session_id` identifies the parent WebUI session encoded into typed messages.
/// `session_id` 标识编码到类型化消息中的父 WebUI 会话。
/// `controller` is the detached Core controller receiving all converted messages.
/// `controller` 是接收全部已转换消息的分离 Core 控制器。
///
/// # Returns
/// 返回值
/// Returns whether mailbox input was appended, or a conversion error without touching authoritative state.
/// 返回是否追加了 mailbox 输入；转换失败时返回错误且不修改权威状态。
fn append_webui_mailbox_intervention_messages_to_draft(
    kernel: &mut SessionRuntimeKernel,
    session_id: SessionId,
    controller: &mut VulcanCoreController,
) -> Result<bool, RuntimePromptInterventionError> {
    if kernel.has_wait_scopes() {
        return Ok(false);
    }
    // inputs are cloned from the draft before any drain so every conversion must succeed first.
    // inputs 在任何取出动作前从草稿克隆，因此全部转换必须先成功。
    let inputs = kernel
        .input_queue()
        .get_pending_input()
        .into_iter()
        .filter(|input| input.kind == TurnInputKind::SubagentMessage)
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return Ok(false);
    }
    // messages materializes the full typed batch before changing either draft.
    // messages 在修改任一草稿前物化完整类型化批次。
    let messages = inputs
        .iter()
        .map(|input| webui_provider_visible_agent_message_from_turn_input(session_id, input))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|message| RuntimePromptInterventionError::InvalidInput { message })?;
    for message in messages {
        controller.append_agent_message(message.to_core_append_request());
    }
    let drained_inputs = kernel
        .input_queue_mut()
        .drain_current_turn_inputs_by_kind(TurnInputKind::SubagentMessage);
    if drained_inputs.len() != inputs.len() {
        return Err(RuntimePromptInterventionError::InvariantViolation {
            message: format!(
                "Mailbox intervention expected to drain {} subagent input(s) but drained {}.",
                inputs.len(),
                drained_inputs.len()
            ),
        });
    }
    Ok(true)
}

/// Appends current-turn mailbox messages to the controller as typed inter-agent messages.
/// 将当前回合 mailbox 消息以类型化代理间消息追加到控制器。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns session runtime kernels.
/// `state` 是持有会话运行内核的共享服务状态。
/// `session_id` identifies the parent WebUI session.
/// `session_id` 标识父级 WebUI 会话。
/// `controller` owns the canonical provider-visible conversation history.
/// `controller` 持有规范 Provider 可见会话历史。
///
/// # Returns
/// 返回值
/// `true` when at least one mailbox message was appended.
/// 至少追加一条 mailbox 消息时返回 `true`。
#[cfg(test)]
pub(crate) fn append_webui_mailbox_intervention_messages(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    controller: &mut VulcanCoreController,
) -> Result<bool, RuntimePromptInterventionError> {
    let mut guard = state
        .lock()
        .map_err(|error| RuntimePromptInterventionError::StateAccess {
            message: format!("failed to lock service state: {error}"),
        })?;
    let Some(mut kernel_draft) = guard.session_runtime_kernels.get(&session_id).cloned() else {
        return Ok(false);
    };
    let mut controller_draft = controller.clone();
    let changed = append_webui_mailbox_intervention_messages_to_draft(
        &mut kernel_draft,
        session_id,
        &mut controller_draft,
    )?;
    if !changed {
        return Ok(false);
    }
    guard
        .session_runtime_kernels
        .insert(session_id, kernel_draft);
    sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
    *controller = controller_draft;
    webui_trace(
        "mailbox_agent_messages_appended",
        format!("session={session_id}"),
    );
    Ok(true)
}

/// Renders one queued WebUI mailbox input as a Codex-compatible inter-agent message.
/// 将一条排队的 WebUI mailbox 输入渲染为 Codex 兼容的代理间消息。
///
/// # Parameters
/// 参数
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `input` is the queued mailbox input being delivered to the parent.
/// `input` 是待投递给父级的排队 mailbox 输入。
///
/// # Returns
/// 返回值
/// Provider-visible inter-agent message text.
/// Provider 可见的代理间消息文本。
#[cfg(test)]
pub(crate) fn webui_agent_message_text_from_turn_input(
    parent_session_id: SessionId,
    input: &TurnInput,
) -> String {
    let message = webui_provider_visible_agent_message_from_turn_input(parent_session_id, input)
        .expect("valid mailbox input should render as an agent message");
    vulcan_core::types::ConversationPayload::render_agent_message_text(
        &message.message_type,
        &message.task_name,
        &message.sender,
        &message.payload,
    )
}

/// Builds one provider-visible typed inter-agent message from a queued WebUI mailbox input.
/// 从一条排队的 WebUI mailbox 输入构建 Provider 可见类型化代理间消息。
pub(crate) fn webui_provider_visible_agent_message_from_turn_input(
    parent_session_id: SessionId,
    input: &TurnInput,
) -> Result<ExecutionProviderVisibleAgentMessage, String> {
    let mailbox_event = input
        .payload
        .get("mailbox_event")
        .cloned()
        .unwrap_or_else(|| input.payload.clone());
    let event_payload = mailbox_event
        .get("payload")
        .filter(|payload| payload.is_object())
        .unwrap_or(&mailbox_event);
    let task_name = event_payload
        .get("task_name")
        .or_else(|| event_payload.get("public_id"))
        .or_else(|| input.payload.get("task_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "mailbox input `{}` 缺少 task_name/public_id/task_id，无法构建智能体消息。",
                input.input_id
            )
        })?
        .to_string();
    let payload = mailbox_event_payload_text(event_payload);
    if payload.trim().is_empty() {
        return Err(format!(
            "mailbox input `{}` 的消息载荷为空，无法构建智能体消息。",
            input.input_id
        ));
    }
    let terminal_status = input
        .payload
        .get("delivery_status")
        .or_else(|| event_payload.get("status"))
        .or_else(|| event_payload.get("execution_status"))
        .or_else(|| mailbox_event.get("kind"))
        .and_then(Value::as_str)
        .and_then(canonical_subagent_mailbox_terminal_status_label);
    let message_type = if terminal_status.is_some() {
        String::from("FINAL_ANSWER")
    } else {
        String::from("MESSAGE")
    };
    Ok(ExecutionProviderVisibleAgentMessage {
        delivery_id: input.input_id.clone(),
        message_type,
        task_name: task_name.clone(),
        sender: task_name,
        recipient: format!("parent:{parent_session_id}"),
        payload,
        trigger_turn: input.trigger_turn,
        timestamp_ms: input.timestamp_ms,
        metadata: Some(json!({
            "input_id": input.input_id,
            "node_id": input.node_id.to_string(),
            "parent_turn_id": input.parent_turn_id,
            "timestamp_ms": input.timestamp_ms,
            "mailbox_event": mailbox_event,
        })),
    })
}

/// Extracts model-visible payload text from one mailbox event.
/// 从单条 mailbox 事件中提取模型可见载荷文本。
fn mailbox_event_payload_text(mailbox_event: &Value) -> String {
    for key in ["result_text", "result", "error_text", "error", "message"] {
        if let Some(value) = mailbox_event.get(key) {
            if let Some(text) = value.as_str().filter(|text| !text.trim().is_empty()) {
                return text.to_string();
            }
            if !value.is_null() {
                return serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            }
        }
    }
    serde_json::to_string_pretty(mailbox_event).unwrap_or_else(|_| mailbox_event.to_string())
}

/// Describes whether one wait-agent runtime event changed retained coordination state.
/// 描述单条 wait-agent 运行时事件是否修改了保留协调状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitAgentRuntimeStateCommit {
    /// The event is unrelated to wait-agent coordination.
    /// 该事件与 wait-agent 协调无关。
    Ignored,
    /// The exact active root task committed the coordination update.
    /// 精确活跃根任务已提交协调更新。
    Committed,
    /// A replacement root task owns the session, so the stale event was rejected.
    /// 替代根任务已持有会话，因此旧事件被拒绝。
    Superseded,
}

/// Updates retained wait-agent coordination state for one exact active root task.
/// 为单个精确活跃根任务更新保留的 wait-agent 协调状态。
///
/// # Parameters
/// # 参数
/// `state` owns the active-stream registry, wait-agent coordination, kernels, and workspace mirror.
/// `state` 持有活跃流注册表、wait-agent 协调状态、内核与工作区镜像。
/// `session_id` identifies the root session receiving the runtime event.
/// `session_id` 标识接收运行时事件的根会话。
/// `active_stream_task_id` identifies the exact root task allowed to commit coordination state.
/// `active_stream_task_id` 标识获准提交协调状态的精确根任务。
/// `event` is the authoritative runtime tool lifecycle event emitted by that task.
/// `event` 是该任务发出的权威运行时工具生命周期事件。
///
/// # Returns
/// # 返回值
/// Returns an explicit commit outcome, or an error when the shared state lock is unavailable.
/// 返回明确的提交结果；共享状态锁不可用时返回错误。
pub(crate) fn update_webui_wait_agent_runtime_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    event: &RuntimeEvent,
) -> Result<WaitAgentRuntimeStateCommit, String> {
    let Some(tool_name) = runtime_event_tool_name(event) else {
        return Ok(WaitAgentRuntimeStateCommit::Ignored);
    };
    if tool_name != "wait_agent"
        || !matches!(
            event.kind,
            RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
        )
    {
        return Ok(WaitAgentRuntimeStateCommit::Ignored);
    }
    // The guard keeps exact task ownership stable across every coordination and workspace mutation.
    // 该锁守卫保证精确任务所有权在全部协调状态与工作区变更期间保持稳定。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
        return Ok(WaitAgentRuntimeStateCommit::Superseded);
    }
    match (tool_name, event.kind) {
        ("wait_agent", RuntimeEventKind::ToolStarted) => {
            let scope_id = wait_agent_scope_id(event);
            let kernel = session_runtime_kernel_mut(&mut guard, session_id);
            let removed_stale_wait_scopes = kernel.clear_wait_scopes();
            if removed_stale_wait_scopes > 0 {
                webui_trace(
                    "wait_agent_stale_scopes_cleared",
                    format!("session={session_id} removed_wait_scopes={removed_stale_wait_scopes}"),
                );
            }
            // Clearing every prior scope guarantees the replacement identifier is unique under the kernel contract.
            // 清空全部旧范围可依据内核契约保证替代标识唯一。
            kernel
                .register_wait_scope(scope_id, Vec::<String>::new(), current_unix_millis_u64())
                .expect("cleared wait scopes guarantee a unique replacement scope identifier");
            kernel
                .input_queue_mut()
                .accept_mailbox_delivery_for_current_turn();
            guard
                .parent_sessions_waiting_for_subagents
                .insert(session_id);
            sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
        }
        ("wait_agent", RuntimeEventKind::ToolFinished) => {
            guard
                .parent_sessions_waiting_for_subagents
                .remove(&session_id);
            if let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) {
                let scope_id = wait_agent_scope_id(event);
                let _ = kernel.clear_wait_scope(&scope_id);
            }
            sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
        }
        _ => unreachable!("wait-agent event kind was validated before acquiring state"),
    }
    Ok(WaitAgentRuntimeStateCommit::Committed)
}

/// Returns the stable kernel wait-scope id for one wait_agent runtime event.
/// 返回单条 wait_agent 运行时事件对应的稳定内核等待范围标识。
pub(crate) fn wait_agent_scope_id(event: &RuntimeEvent) -> String {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_call_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|tool_call_id| format!("wait-agent-{tool_call_id}"))
        .unwrap_or_else(|| String::from("wait-agent-current"))
}

/// Extracts the canonical tool name carried by one runtime event metadata payload.
/// 提取单条运行时事件元数据中携带的规范工具名称。
pub(crate) fn runtime_event_tool_name(event: &RuntimeEvent) -> Option<&str> {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// Normalizes one dynamically interjected WebUI prompt through the same provider/model capability gate used by request bootstrap.
/// 通过与请求 bootstrap 相同的 Provider/模型能力门禁归一化一条 WebUI 动态引导提示词。
///
/// # Parameters
/// 参数
/// `entry` is the internal queued prompt selected for runtime guidance.
/// `entry` 是被选中用于运行时引导的内部排队提示词。
///
/// # Returns
/// 返回值
/// Provider-safe prompt text and attachments after unsupported modalities have been downgraded.
/// 返回不支持模态被降级后的 Provider 安全提示词文本与附件集合。
pub(crate) fn normalize_webui_interjected_prompt(
    entry: &PendingSessionPromptEntry,
) -> (String, Vec<ConversationAttachment>) {
    (entry.prompt.clone(), entry.attachments.clone())
}
