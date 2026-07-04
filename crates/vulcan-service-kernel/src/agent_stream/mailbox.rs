//! WebUI mailbox and wait-agent helpers.
//! WebUI mailbox 与 wait-agent 辅助函数。

use super::*;

/// Builds the WebUI runtime intervention that injects guided prompts before the next provider request.
/// 构建 WebUI 运行时干预器，用于在下一次 Provider 请求前注入引导提示词。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit refreshed workbench snapshots.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` is the shared service bridge state that owns the active workspace snapshot.
/// `state` 是拥有当前工作区快照的共享服务桥接状态。
/// `provider_configuration` is the provider configuration used by the shared pending router.
/// `provider_configuration` 是共享待发送路由器使用的 Provider 配置。
///
/// # Returns
/// 返回值
/// A callback consumed by the host-controlled runtime stream.
/// 返回供宿主控制运行时流消费的回调。
pub(crate) fn build_webui_prompt_intervention(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_configuration: ProviderConfigurationFile,
    ai_tool_configuration: PersistedAiToolConfiguration,
) -> RuntimePromptIntervention {
    Arc::new(move |session_id, controller: &mut VulcanCoreController| {
        match take_webui_internal_guidance_prompts(
            &app,
            &state,
            &provider_configuration,
            session_id,
        ) {
            Ok(entries) => {
                let pending_prompt_changed = entries
                    .iter()
                    .filter_map(|entry| {
                        let (prompt, attachments) = normalize_webui_interjected_prompt_for_provider(
                            &provider_configuration,
                            &ai_tool_configuration,
                            entry,
                        );
                        controller.append_interjected_user_turn_with_attachments(
                            prompt.as_str(),
                            attachments,
                        )
                    })
                    .count()
                    > 0;
                let mailbox_changed = match append_webui_mailbox_intervention_messages(
                    &state, session_id, controller,
                ) {
                    Ok(changed) => changed,
                    Err(error) => {
                        eprintln!("[webui-service] mailbox intervention append failed: {error}");
                        false
                    }
                };
                pending_prompt_changed || mailbox_changed
            }
            Err(error) => {
                eprintln!("[webui-service] apply guided prompt intervention failed: {error}");
                false
            }
        }
    })
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
pub(crate) fn append_webui_mailbox_intervention_messages(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    controller: &mut VulcanCoreController,
) -> Result<bool, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) else {
        return Ok(false);
    };
    if kernel.has_wait_scopes() {
        return Ok(false);
    }
    let inputs = kernel
        .input_queue_mut()
        .drain_current_turn_inputs_by_kind(TurnInputKind::SubagentMessage);
    if inputs.is_empty() {
        return Ok(false);
    }
    let mut appended = 0usize;
    for input in inputs {
        let message = webui_agent_message_from_turn_input(session_id, &input);
        controller.append_agent_message(
            message.message_type,
            message.task_name,
            message.sender,
            message.recipient,
            message.payload,
            message.trigger_turn,
            message.metadata,
        );
        appended = appended.saturating_add(1);
    }
    sync_webui_session_runtime_kernel_to_workspace_state(&mut guard, session_id);
    webui_trace(
        "mailbox_agent_messages_appended",
        format!("session={session_id} appended={appended}"),
    );
    Ok(appended > 0)
}

/// Host-side normalized representation of one mailbox agent message.
/// 单条 mailbox 代理消息的宿主侧规范化表示。
struct WebuiMailboxAgentMessage {
    /// Stable model-visible message type.
    /// 稳定的模型可见消息类型。
    message_type: String,
    /// Logical task name attached to the child-agent message.
    /// 绑定到子代理消息的逻辑任务名称。
    task_name: String,
    /// Logical sender name attached to the child-agent message.
    /// 绑定到子代理消息的逻辑发送方名称。
    sender: String,
    /// Logical recipient name attached to the child-agent message.
    /// 绑定到子代理消息的逻辑接收方名称。
    recipient: String,
    /// Model-visible payload text.
    /// 模型可见载荷文本。
    payload: String,
    /// Whether this message may wake an idle parent turn.
    /// 该消息是否允许唤醒空闲父级回合。
    trigger_turn: bool,
    /// Host-only metadata for UI and diagnostics.
    /// 供 UI 与诊断使用的宿主侧元数据。
    metadata: Option<Value>,
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
pub(crate) fn webui_agent_message_text_from_turn_input(
    parent_session_id: SessionId,
    input: &TurnInput,
) -> String {
    let message = webui_agent_message_from_turn_input(parent_session_id, input);
    vulcan_core::types::ConversationPayload::render_agent_message_text(
        &message.message_type,
        &message.task_name,
        &message.sender,
        &message.payload,
    )
}

/// Builds one typed inter-agent message from a queued WebUI mailbox input.
/// 从一条排队的 WebUI mailbox 输入构建类型化代理间消息。
fn webui_agent_message_from_turn_input(
    parent_session_id: SessionId,
    input: &TurnInput,
) -> WebuiMailboxAgentMessage {
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
        .unwrap_or("subagent")
        .to_string();
    let payload = mailbox_event_payload_text(event_payload);
    let status = event_payload
        .get("status")
        .or_else(|| event_payload.get("execution_status"))
        .or_else(|| mailbox_event.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("completed");
    let message_type = if matches!(
        status,
        "completed" | "failed" | "cancelled" | "canceled" | "stopped"
    ) {
        String::from("FINAL_ANSWER")
    } else {
        String::from("MESSAGE")
    };
    WebuiMailboxAgentMessage {
        message_type,
        task_name: task_name.clone(),
        sender: task_name,
        recipient: format!("parent:{parent_session_id}"),
        payload,
        trigger_turn: input.trigger_turn,
        metadata: Some(json!({
            "input_id": input.input_id,
            "node_id": input.node_id.to_string(),
            "parent_turn_id": input.parent_turn_id,
            "timestamp_ms": input.timestamp_ms,
            "mailbox_event": mailbox_event,
        })),
    }
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

/// Updates the parent-session wait_agent guard from provider runtime tool events.
/// 根据 Provider 运行时工具事件更新父会话 wait_agent 等待闸门。
pub(crate) fn update_webui_wait_agent_runtime_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) {
    let Some(tool_name) = runtime_event_tool_name(event) else {
        return;
    };
    let Ok(mut guard) = state.lock() else {
        return;
    };
    match (tool_name, event.kind) {
        ("wait_agent", RuntimeEventKind::ToolStarted) => {
            guard
                .parent_sessions_waiting_for_subagents
                .insert(session_id);
            let scope_id = wait_agent_scope_id(event);
            let kernel = session_runtime_kernel_mut(&mut guard, session_id);
            if !kernel.has_wait_scopes()
                && let Err(error) = kernel.register_wait_scope(
                    scope_id,
                    Vec::<String>::new(),
                    current_unix_millis_u64(),
                )
            {
                webui_trace(
                    "wait_agent_scope_register_failed",
                    format!("session={session_id} error={}", error.message),
                );
            }
            kernel
                .input_queue_mut()
                .accept_mailbox_delivery_for_current_turn();
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
        _ => {}
    }
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
/// `provider_configuration` is the active provider configuration captured by the running stream.
/// `provider_configuration` 是当前运行流捕获的活跃 Provider 配置。
/// `ai_tool_configuration` is the active AI tool routing configuration captured by the running stream.
/// `ai_tool_configuration` 是当前运行流捕获的活跃 AI 工具路由配置。
/// `entry` is the internal queued prompt selected for runtime guidance.
/// `entry` 是被选中用于运行时引导的内部排队提示词。
///
/// # Returns
/// 返回值
/// Provider-safe prompt text and attachments after unsupported modalities have been downgraded.
/// 返回不支持模态被降级后的 Provider 安全提示词文本与附件集合。
pub(crate) fn normalize_webui_interjected_prompt_for_provider(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    entry: &PendingSessionPromptEntry,
) -> (String, Vec<ConversationAttachment>) {
    let scoped_provider_configuration = provider_configuration_for_model_reference(
        provider_configuration,
        entry.model_reference.as_ref(),
    );
    let Some((provider_id, model_id)) = scoped_provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
    else {
        return (entry.prompt.clone(), entry.attachments.clone());
    };
    let Some(definition) = provider_config::supported_provider_by_id(provider_id) else {
        return (entry.prompt.clone(), entry.attachments.clone());
    };
    let effective_ai_tool_configuration = effective_ai_tool_configuration_for_prompt_overrides(
        ai_tool_configuration,
        &entry.runtime_options.disabled_ai_capability_kinds,
        &BTreeSet::new(),
        &BTreeMap::new(),
    );
    let active_model =
        provider_config::effective_model_choices(provider_id, &scoped_provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    let capabilities = resolve_effective_model_capabilities(
        definition,
        active_model.as_ref(),
        &effective_ai_tool_configuration,
    );
    let api_family = definition
        .effective_runtime_for_model(model_id)
        .api_family();
    let normalized = normalize_provider_user_input_for_model(
        api_family,
        Some(&capabilities),
        &entry.prompt,
        &entry.attachments,
    );
    (normalized.text, normalized.attachments)
}
