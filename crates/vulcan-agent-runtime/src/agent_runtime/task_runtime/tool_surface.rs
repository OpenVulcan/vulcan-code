use super::*;

/// Builds the session-agent-style child-agent tool surface backed by the shared TaskSession runtime.
/// 构建由共享 TaskSession 运行时承载的 会话代理风格子代理工具面。
pub fn build_subagent_tool_candidates(task_runtime: Arc<TaskRuntime>) -> Vec<VulcanToolCandidate> {
    vec![
        build_spawn_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_send_message_tool_candidate(Arc::clone(&task_runtime)),
        build_followup_task_tool_candidate(Arc::clone(&task_runtime)),
        build_wait_agent_tool_candidate(Arc::clone(&task_runtime)),
        build_list_agents_tool_candidate(Arc::clone(&task_runtime)),
        build_close_agent_tool_candidate(task_runtime),
    ]
}

/// Builds the model-visible non-blocking child-agent spawn tool.
/// 构建模型可见的非阻塞子代理启动工具。
pub(super) fn build_spawn_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("spawn_agent"),
            name: String::from("spawn_agent"),
            description: String::from(
                "Start a child agent asynchronously and return immediately with its task name. For parallel work, issue one spawn_agent call per child agent in the same tool-call batch, then use wait_agent once to collect their terminal statuses.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: spawn_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let trace_started_at = current_unix_millis_u64();
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), "spawn_agent")?;
                task_runtime_trace(
                    "subagent_spawn_agent_start",
                    format!(
                        "parent={} args_chars={}",
                        session_id,
                        args.to_string().chars().count()
                    ),
                );
                let task_args = task_tool_args_from_spawn_agent_args(args)?;
                let result = task_runtime.invoke_spawn_agent(session_id, task_args).await;
                task_runtime_trace(
                    "subagent_spawn_agent_return",
                    format!(
                        "parent={} ok={} elapsed_ms={}",
                        session_id,
                        result.is_ok(),
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                result
            })
        }),
    }
}

/// Builds the model-visible child-agent queue-only message tool.
/// 构建模型可见的子代理仅排队消息工具。
pub(super) fn build_send_message_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("send_message"),
            name: String::from("send_message"),
            description: String::from(
                "Send a message to an existing child agent without starting new child work. The host queues the message for that child agent's next followup_task continuation.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: subagent_message_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), "send_message")?;
                task_runtime.invoke_send_message(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent follow-up task tool.
/// 构建模型可见的子代理后续任务工具。
pub(super) fn build_followup_task_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("followup_task"),
            name: String::from("followup_task"),
            description: String::from(
                "Send a follow-up task to an existing child agent and trigger a child-agent continuation when the child is idle.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: subagent_message_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), "followup_task")?;
                task_runtime.invoke_followup_task(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent wait tool.
/// 构建模型可见的子代理等待工具。
pub(super) fn build_wait_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("wait_agent"),
            name: String::from("wait_agent"),
            description: String::from(
                "Wait for one child agent or all known child agents in the current parent session. Use this after spawning every required child agent; it returns status and terminal result text without starting new work.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: wait_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), "wait_agent")?;
                task_runtime.invoke_wait_agent(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent listing tool.
/// 构建模型可见的子代理列表工具。
pub(super) fn build_list_agents_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("list_agents"),
            name: String::from("list_agents"),
            description: String::from(
                "List known child agents for the current parent session and their latest runtime statuses. This tool never starts or resumes child work.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: list_agents_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), "list_agents")?;
                task_runtime.invoke_list_agents(session_id, args).await
            })
        }),
    }
}

/// Builds the model-visible child-agent stop tool.
/// 构建模型可见的子代理停止工具。
pub(super) fn build_close_agent_tool_candidate(
    task_runtime: Arc<TaskRuntime>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("close_agent"),
            name: String::from("close_agent"),
            description: String::from(
                "Stop one running child agent by task name or task session id. This is for cancellation only and never starts new work.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: close_agent_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let session_id =
                    parse_subagent_tool_session_id(session_id.as_deref(), "close_agent")?;
                task_runtime.invoke_close_agent(session_id, args).await
            })
        }),
    }
}

/// Builds the JSON Schema advertised to the model for `spawn_agent`.
/// 为模型端 `spawn_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn spawn_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {
                "type": "string",
                "description": "Required child-agent instruction. Include the goal, boundaries, and expected return format."
            },
            "task_name": {
                "type": "string",
                "description": "Stable child-agent name used for later wait_agent/list_agents/close_agent calls."
            },
            "agent_type": {
                "type": "string",
                "description": "Optional configured agent profile or role label. If omitted, the child agent clones the parent agent configuration."
            },
            "description": {
                "type": "string",
                "description": "Optional short human-readable title for UI display."
            },
            "model": {
                "type": "string",
                "description": "Optional model override for this child agent."
            },
            "provider": {
                "type": "string",
                "description": "Optional provider override for this child agent."
            },
            "reasoning_effort": {
                "type": "string",
                "description": "Optional reasoning effort override for this child agent."
            },
            "tools": {
                "description": "Optional tool exposure override for this child agent.",
                "anyOf": [
                    { "type": "string" },
                    { "type": "array", "items": { "type": "string" } },
                    { "type": "object", "additionalProperties": true }
                ]
            },
            "extra": {
                "type": "object",
                "additionalProperties": true,
                "description": "Optional caller-owned JSON payload persisted with this child-agent invocation."
            }
        },
        "required": ["message"],
        "additionalProperties": true
    })
}

/// Builds the JSON Schema advertised to the model for child-agent message tools.
/// 为子代理消息工具构建对模型声明的 JSON Schema。
pub(super) fn subagent_message_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "description": "Required child-agent name returned from spawn_agent."
            },
            "message": {
                "type": "string",
                "description": "Required message or follow-up task instruction to send to the child agent."
            }
        },
        "required": ["target", "message"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `wait_agent`.
/// 为模型端 `wait_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn wait_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task_name": {
                "type": "string",
                "description": "Optional child-agent name returned from spawn_agent. Omit it to wait for every child agent under the current parent session."
            },
            "task_id": {
                "type": "string",
                "description": "Alias for task_name."
            },
            "timeout_ms": {
                "type": "integer",
                "minimum": 0,
                "description": "Maximum wait time in milliseconds. Omit for the default host wait window."
            }
        },
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `list_agents`.
/// 为模型端 `list_agents` 工具构建对外声明的 JSON Schema。
pub(super) fn list_agents_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task_name": {
                "type": "string",
                "description": "Optional child-agent selector. Omit it to list every child agent under the current parent session."
            },
            "task_id": {
                "type": "string",
                "description": "Alias for task_name."
            }
        },
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `close_agent`.
/// 为模型端 `close_agent` 工具构建对外声明的 JSON Schema。
pub(super) fn close_agent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task_name": {
                "type": "string",
                "description": "Child-agent name returned from spawn_agent."
            },
            "task_id": {
                "type": "string",
                "description": "Alias for task_name."
            }
        },
        "additionalProperties": false
    })
}

/// Parses the session identifier required by every session-agent-style subagent tool.
/// 解析每个 会话代理风格子代理工具都需要的会话标识。
pub(super) fn parse_subagent_tool_session_id(
    session_id: Option<&str>,
    tool_name: &str,
) -> Result<SessionId, String> {
    let Some(session_id) = session_id else {
        return Err(format!(
            "The `{tool_name}` tool requires a session identifier and cannot run without it."
        ));
    };
    Uuid::parse_str(session_id).map_err(|error| {
        format!("Failed to parse the session identifier for `{tool_name}`: {error}")
    })
}

/// Converts session-agent-style `spawn_agent` arguments into the existing TaskSession invocation shape.
/// 将 会话代理风格 `spawn_agent` 参数转换为既有 TaskSession 调用形态。
pub(super) fn task_tool_args_from_spawn_agent_args(args: Value) -> Result<Value, String> {
    let Value::Object(map) = args else {
        return Err(String::from(
            "`spawn_agent` arguments must be a JSON object.",
        ));
    };
    let prompt = string_field(&map, &["message"]).ok_or_else(|| {
        String::from("`spawn_agent` requires `message` with the child-agent instruction.")
    })?;
    let task_id = string_field(&map, &["task_name"]);
    let agent_type = string_field(&map, &["agent_type"]);
    let description = string_field(&map, &["description"])
        .or_else(|| task_id.clone())
        .or_else(|| build_task_prompt_preview(&prompt));
    let mut task_args = serde_json::Map::new();
    task_args.insert(String::from("prompt"), Value::String(prompt));
    if let Some(task_id) = task_id {
        task_args.insert(String::from("task_id"), Value::String(task_id));
    }
    if let Some(description) = description {
        task_args.insert(String::from("description"), Value::String(description));
    }
    if let Some(agent_type) = agent_type {
        task_args.insert(String::from("subagent"), Value::String(agent_type));
    }
    for key in [
        "inherit_parent_config",
        "system_prompt",
        "system_prompt_append",
        "provider",
        "model",
        "reasoning_effort",
        "execution_mode",
        "provider_model",
        "tools",
        "ai_capabilities",
        "skills",
        "prompt_skills",
        "luaskills",
        "mcp",
        "context_compaction",
        "runtime_policy",
        "extra",
    ] {
        if let Some(value) = map.get(key) {
            task_args.insert(key.to_string(), value.clone());
        }
    }
    Ok(Value::Object(task_args))
}

/// Parsed arguments shared by child-agent message tools.
/// 子代理消息工具共享的已解析参数。
pub(super) struct SubagentMessageToolArgs {
    /// Target child-agent identifier.
    /// 目标子代理标识。
    pub(super) target: String,
    /// Message sent from the parent agent.
    /// 父代理发送的消息。
    pub(super) message: String,
}

/// Parses the shared argument payload for child-agent message tools.
/// 解析子代理消息工具共享的参数载荷。
pub(super) fn parse_subagent_message_tool_arguments(
    args: Value,
    tool_name: &str,
) -> Result<SubagentMessageToolArgs, String> {
    let Value::Object(map) = args else {
        return Err(format!("`{tool_name}` arguments must be a JSON object."));
    };
    let target = string_field(&map, &["target"])
        .ok_or_else(|| format!("`{tool_name}` requires `target`."))?;
    let message = string_field(&map, &["message"])
        .ok_or_else(|| format!("`{tool_name}` requires `message`."))?;
    let target = normalize_public_task_identifier(&target)?;
    if message.trim().is_empty() {
        return Err(format!("`{tool_name}` message cannot be empty."));
    }
    Ok(SubagentMessageToolArgs { target, message })
}

/// Builds the prompt used by followup_task from queued send_message items and the latest message.
/// 根据已排队的 send_message 条目和最新消息构建 followup_task 提示词。
pub(super) fn followup_prompt_from_messages(
    mut queued_messages: Vec<String>,
    latest_message: String,
) -> String {
    queued_messages.push(latest_message);
    if queued_messages.len() == 1 {
        return queued_messages.remove(0);
    }
    let mut prompt = String::from("Parent agent follow-up messages, oldest first:\n");
    for (index, message) in queued_messages.iter().enumerate() {
        prompt.push_str(&format!("\n[{}]\n{}", index + 1, message.trim()));
    }
    prompt
}

/// Reads the optional child-agent selector from a session-agent-style subagent tool request.
/// 从 会话代理风格子代理工具请求中读取可选子代理选择器。
pub(super) fn subagent_tool_target(args: &Value) -> Option<String> {
    let Value::Object(map) = args else {
        return None;
    };
    string_field(
        map,
        &[
            "task_name",
            "task_id",
            "agent_id",
            "child_thread_id",
            "target",
            "name",
        ],
    )
}

/// Reads the optional wait timeout from a session-agent-style subagent tool request.
/// 从 会话代理风格子代理工具请求中读取可选等待超时。
pub(super) fn subagent_tool_timeout_ms(args: &Value) -> Option<u64> {
    let Value::Object(map) = args else {
        return None;
    };
    integer_field(map, &["timeout_ms", "timeout"])
}

/// Builds one compact JSON status payload for a child-agent record.
/// 为一条子代理记录构建紧凑 JSON 状态载荷。
pub(super) fn subagent_record_status_payload(record: &TaskSessionRecord) -> Value {
    json!({
        "task_name": record.public_task_id,
        "task_session_id": record.task_id.to_string(),
        "display_title": record.display_title,
        "description": record.description,
        "agent_type": record.subagent,
        "status": record.last_status,
        "depth": record.depth,
        "result": record.last_result_text,
        "updated_at": record.updated_at,
    })
}

/// Builds one compact JSON status payload for a native subagent registry entry.
/// 为一条原生子代理注册表条目构建紧凑 JSON 状态载荷。
///
/// # Parameters
/// 参数
/// `entry` is the subagent registry entry to expose.
/// `entry` 是需要暴露的子代理注册表条目。
///
/// # Returns
/// 返回值
/// A model-visible JSON status payload.
/// 模型可见的 JSON 状态载荷。
pub(super) fn subagent_registry_entry_status_payload(entry: &SubagentRegistryEntry) -> Value {
    json!({
        "task_name": &entry.agent_id,
        "task_session_id": entry.session_id.to_string(),
        "display_title": &entry.agent_id,
        "description": Value::Null,
        "agent_type": entry.prompt_source.clone(),
        "agent_kind": entry.agent_kind,
        "prompt_source": entry.prompt_source.clone(),
        "status": entry.status,
        "depth": Value::Null,
        "result": entry.result_summary.clone(),
        "updated_at": entry.updated_at_ms.to_string(),
        "segments": entry.segments.clone(),
    })
}

/// Builds a model-visible status result for session-agent-style subagent control tools.
/// 为 会话代理风格子代理控制工具构建模型可见状态结果。
pub(super) fn build_subagent_status_tool_result(
    tool_name: &str,
    current_session_id: SessionId,
    records: Vec<TaskSessionRecord>,
    timeout_ms: u64,
    completed: bool,
) -> VulcanToolInvocationResult {
    let agents = records
        .iter()
        .map(subagent_record_status_payload)
        .collect::<Vec<_>>();
    let payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "completed": completed,
        "timeout_ms": timeout_ms,
        "agents": agents,
    });
    VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_status"),
            payload,
        },
    )
}

/// Builds a model-visible status result from native subagent registry entries.
/// 根据原生子代理注册表条目构建模型可见状态结果。
///
/// # Parameters
/// 参数
/// `tool_name` is the subagent control tool name.
/// `tool_name` 是子代理控制工具名称。
/// `current_session_id` is the parent session id.
/// `current_session_id` 是父会话标识。
/// `entries` are the registry entries to expose.
/// `entries` 是需要暴露的注册表条目。
/// `timeout_ms` is the wait timeout represented in the tool result.
/// `timeout_ms` 是工具结果中表示的等待超时时间。
/// `completed` indicates whether the requested wait/list operation is complete.
/// `completed` 表示请求的等待或列出操作是否完成。
///
/// # Returns
/// 返回值
/// A model-visible tool invocation result.
/// 模型可见的工具调用结果。
pub(super) fn build_subagent_registry_status_tool_result(
    tool_name: &str,
    current_session_id: SessionId,
    entries: Vec<SubagentRegistryEntry>,
    timeout_ms: u64,
    completed: bool,
) -> VulcanToolInvocationResult {
    let agents = entries
        .iter()
        .map(subagent_registry_entry_status_payload)
        .collect::<Vec<_>>();
    let payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "completed": completed,
        "timeout_ms": timeout_ms,
        "agents": agents,
    });
    VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_status"),
            payload,
        },
    )
}

/// Builds a model-visible result for child-agent message delivery tools.
/// 为子代理消息投递工具构建模型可见结果。
pub(super) fn build_subagent_message_tool_result(
    tool_name: &str,
    current_session_id: SessionId,
    target: &str,
    target_task_ids: Vec<SessionId>,
    trigger_turn: bool,
    queued: bool,
    note: Option<String>,
) -> VulcanToolInvocationResult {
    let payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "target": target,
        "target_task_session_ids": target_task_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "trigger_turn": trigger_turn,
        "queued": queued,
        "note": note,
    });
    VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_message"),
            payload,
        },
    )
}
