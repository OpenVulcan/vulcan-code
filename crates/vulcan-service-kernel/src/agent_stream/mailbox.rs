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
                let mailbox_changed =
                    match take_webui_mailbox_intervention_prompt(&state, session_id) {
                        Ok(Some(prompt)) => {
                            let changed =
                                controller.append_interjected_user_turn(&prompt).is_some();
                            webui_trace(
                                "mailbox_intervention_appended",
                                format!(
                                    "session={session_id} changed={changed} prompt_chars={}",
                                    prompt.chars().count()
                                ),
                            );
                            changed
                        }
                        Ok(None) => false,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] mailbox intervention prompt failed: {error}"
                            );
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

/// Takes one provider-loop mailbox intervention prompt and marks included mailbox inputs consumed.
/// 获取一条 Provider 循环 mailbox 干预提示词，并将包含的 mailbox 输入标记为已消费。
pub(crate) fn take_webui_mailbox_intervention_prompt(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    let Some(prompt) = webui_session_mailbox_input_from_kernel(state, session_id, None)? else {
        return Ok(None);
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) else {
        return Ok(None);
    };
    if kernel.has_wait_scopes() {
        return Ok(None);
    }
    let input_ids = kernel
        .input_queue()
        .get_unconsumed_mailbox_input()
        .into_iter()
        .filter(|input| input.kind == TurnInputKind::SubagentMessage)
        .map(|input| input.input_id)
        .collect::<Vec<_>>();
    if input_ids.is_empty() {
        return Ok(None);
    }
    let mut consumed = 0usize;
    for input_id in input_ids {
        if kernel.input_queue_mut().mark_input_consumed(input_id) {
            consumed = consumed.saturating_add(1);
        }
    }
    webui_trace(
        "mailbox_intervention_inputs_consumed",
        format!("session={session_id} consumed={consumed}"),
    );
    Ok(Some(prompt))
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
        ("spawn_agent" | "followup_task", RuntimeEventKind::ToolStarted) => {
            guard
                .parent_session_completed_wait_agent_selectors
                .remove(&session_id);
        }
        ("wait_agent", RuntimeEventKind::ToolStarted) => {
            guard
                .parent_sessions_waiting_for_subagents
                .insert(session_id);
            guard
                .parent_session_completed_wait_agent_selectors
                .remove(&session_id);
            let scope_id = wait_agent_scope_id(event);
            let target_agent_ids = wait_agent_target_agent_ids_from_event(event);
            let kernel = session_runtime_kernel_mut(&mut guard, session_id);
            if !kernel.has_wait_scopes()
                && let Err(error) = kernel.register_wait_scope(
                    scope_id,
                    target_agent_ids,
                    current_unix_millis_u64(),
                )
            {
                webui_trace(
                    "wait_agent_scope_register_failed",
                    format!("session={session_id} error={}", error.message),
                );
            }
        }
        ("wait_agent", RuntimeEventKind::ToolFinished) => {
            guard
                .parent_sessions_waiting_for_subagents
                .remove(&session_id);
            if wait_agent_runtime_event_completed(event) {
                let consumed =
                    consume_wait_agent_mailbox_inputs_from_event(&mut guard, session_id, event);
                webui_trace(
                    "wait_agent_mailbox_inputs_consumed",
                    format!("session={session_id} consumed={consumed}"),
                );
                let completed_selectors = wait_agent_completed_agent_selectors(event);
                let selector_count = completed_selectors.len();
                guard
                    .parent_session_completed_wait_agent_selectors
                    .insert(session_id, completed_selectors);
                webui_trace(
                    "wait_agent_completed_selectors_recorded",
                    format!("session={session_id} selectors={selector_count}"),
                );
            }
            if let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) {
                let scope_id = wait_agent_scope_id(event);
                let _ = kernel.clear_wait_scope(&scope_id);
            }
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

/// Extracts requested child-agent selectors from wait_agent tool-start metadata.
/// 从 wait_agent 工具开始事件元数据中提取请求的子代理选择器。
pub(crate) fn wait_agent_target_agent_ids_from_event(event: &RuntimeEvent) -> Vec<String> {
    let Some(arguments) = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("arguments"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    [
        "task_name",
        "task_id",
        "agent_id",
        "child_thread_id",
        "name",
    ]
    .iter()
    .filter_map(|key| arguments.get(*key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToString::to_string)
    .collect()
}

/// Marks matching parent mailbox inputs as consumed by a completed wait_agent result.
/// 将已完成 wait_agent 结果匹配到的父级 mailbox 输入标记为已消费。
pub(crate) fn consume_wait_agent_mailbox_inputs_from_event(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> usize {
    let selectors = wait_agent_completed_agent_selectors(event);
    let scope_id = wait_agent_scope_id(event);
    let Some(kernel) = state.session_runtime_kernels.get_mut(&session_id) else {
        return 0;
    };
    if !kernel.has_wait_scopes() {
        let _ = kernel.register_wait_scope(
            scope_id.clone(),
            Vec::<String>::new(),
            current_unix_millis_u64(),
        );
    }
    let input_ids = kernel
        .input_queue()
        .get_unconsumed_mailbox_input()
        .into_iter()
        .filter(|input| {
            input.kind == TurnInputKind::SubagentMessage
                && (selectors.is_empty() || mailbox_input_matches_any_selector(input, &selectors))
        })
        .map(|input| input.input_id)
        .collect::<Vec<_>>();
    let mut consumed = 0usize;
    for input_id in input_ids {
        if kernel.consume_mailbox_input_for_wait_scope(&scope_id, &input_id) {
            consumed = consumed.saturating_add(1);
        }
    }
    consumed
}

/// Extracts completed child-agent selectors from a wait_agent tool-finished metadata payload.
/// 从 wait_agent 工具完成元数据中提取已完成子代理选择器。
pub(crate) fn wait_agent_completed_agent_selectors(event: &RuntimeEvent) -> BTreeSet<String> {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| host_result.get("payload"))
        .and_then(|payload| payload.get("agents"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|agent| {
            [
                agent.get("task_name").and_then(Value::as_str),
                agent.get("task_session_id").and_then(Value::as_str),
                agent.get("agent_id").and_then(Value::as_str),
                agent.get("child_thread_id").and_then(Value::as_str),
                agent.get("name").and_then(Value::as_str),
            ]
        })
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Returns whether one mailbox input belongs to any wait_agent-completed selector.
/// 返回一条 mailbox 输入是否属于任一 wait_agent 已完成选择器。
pub(crate) fn mailbox_input_matches_any_selector(
    input: &TurnInput,
    selectors: &BTreeSet<String>,
) -> bool {
    mailbox_input_selector_values(input)
        .iter()
        .any(|value| selectors.contains(value))
}

/// Collects stable selector values carried by one subagent mailbox input.
/// 收集单条子代理 mailbox 输入携带的稳定选择器值。
pub(crate) fn mailbox_input_selector_values(input: &TurnInput) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    push_json_string_value(&mut values, input.payload.get("task_id"));
    push_json_string_value(&mut values, input.payload.get("parent_session_id"));
    if let Some(mailbox_event) = input.payload.get("mailbox_event") {
        push_json_string_value(&mut values, mailbox_event.get("child_thread_id"));
        push_json_string_value(&mut values, mailbox_event.get("parent_thread_id"));
        if let Some(payload) = mailbox_event.get("payload") {
            for key in [
                "task_name",
                "task_id",
                "task_session_id",
                "public_id",
                "child_thread_id",
                "agent_id",
                "name",
            ] {
                push_json_string_value(&mut values, payload.get(key));
            }
        }
    }
    values
}

/// Collects selector values from one delegate-finished event before it is enqueued into the mailbox.
/// 在委派完成事件进入 mailbox 前收集其选择器值。
pub(crate) fn delegate_finished_mailbox_selector_values(
    parent_session_id: SessionId,
    task_id: SessionId,
    public_task_id: &str,
    metadata: &Value,
) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    values.insert(parent_session_id.to_string());
    values.insert(task_id.to_string());
    if !public_task_id.trim().is_empty() {
        values.insert(public_task_id.trim().to_string());
    }
    if let Some(payload) = metadata.get("payload") {
        for key in [
            "task_name",
            "task_id",
            "task_session_id",
            "public_id",
            "child_thread_id",
            "agent_id",
            "name",
        ] {
            push_json_string_value(&mut values, payload.get(key));
        }
        if let Some(mailbox_event) = payload.get("subagent_mailbox_event") {
            push_json_string_value(&mut values, mailbox_event.get("child_thread_id"));
            push_json_string_value(&mut values, mailbox_event.get("parent_thread_id"));
            if let Some(mailbox_payload) = mailbox_event.get("payload") {
                for key in [
                    "task_name",
                    "task_id",
                    "task_session_id",
                    "public_id",
                    "child_thread_id",
                    "agent_id",
                    "name",
                ] {
                    push_json_string_value(&mut values, mailbox_payload.get(key));
                }
            }
        }
    }
    values
}

/// Inserts a non-empty JSON string into a selector set.
/// 将非空 JSON 字符串插入选择器集合。
pub(crate) fn push_json_string_value(values: &mut BTreeSet<String>, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str).map(str::trim)
        && !value.is_empty()
    {
        values.insert(value.to_string());
    }
}

/// Returns whether one wait_agent tool-finished event collected all selected child agents.
/// 返回一次 wait_agent 工具完成事件是否已经收集了所有选中的子代理。
pub(crate) fn wait_agent_runtime_event_completed(event: &RuntimeEvent) -> bool {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| host_result.get("payload"))
        .and_then(|payload| payload.get("completed"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
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
